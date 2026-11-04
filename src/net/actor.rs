//! The actor that owns the protocol state and drives the connections.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use bytes::Bytes;
use iroh::{endpoint::Connection, Endpoint, EndpointAddr, EndpointId, PublicKey, Watcher};
use n0_future::{
    task::JoinSet,
    time::{self, Instant, Interval, MissedTickBehavior},
    Stream, StreamExt as _,
};
use rand::{rngs::StdRng, SeedableRng};
use tokio::sync::{mpsc, mpsc::error::TrySendError};
use tracing::{debug, error_span, trace, warn, Instrument};

use super::{
    address_lookup::{decode_peer_data, encode_peer_data, GossipAddressLookup},
    conn::{connection_loop, task_output, ConnectionLoopError},
    dialer::Dialer,
    peers::{Finished, PeerState, PEER_RETENTION},
    queue::{self, Pushed},
    subscriptions::{
        all_eager_peers_busy, topic_subscriber_loop, SubscriptionCommands, TopicHandles,
    },
    InEvent, Join, OutEvent, ProtoCommand, QueueOptions, Timer, ToActor, GOSSIP_ALPN,
};
use crate::{
    api::{Command, Event},
    metrics::Metrics,
    proto::{self, util::TimerMap, Scope, TopicId},
};

#[cfg(test)]
mod tests;

/// The settings that the [`Builder`](super::Builder) passes to the actor.
#[derive(Debug, Clone)]
pub(super) struct ActorConfig {
    pub(super) proto: proto::Config,
    pub(super) alpn: Bytes,
    pub(super) send_queue: QueueOptions,
}

impl Default for ActorConfig {
    fn default() -> Self {
        Self {
            proto: Default::default(),
            alpn: GOSSIP_ALPN.to_vec().into(),
            send_queue: Default::default(),
        }
    }
}

/// The capacity of the channel from the [`Gossip`](super::Gossip) handle to the actor.
const TO_ACTOR_CAP: usize = 64;
/// The capacity of the channel from the connection tasks to the actor.
const IN_EVENT_CAP: usize = 1024;

/// The actor that owns the protocol state and the peers, and runs the connection tasks.
pub(super) struct Actor {
    alpn: Bytes,
    /// The state of the gossip protocol.
    pub(super) state: proto::State<PublicKey, StdRng>,
    /// The endpoint through which we dial peers
    pub(super) endpoint: Endpoint,
    /// Dial machine to connect to peers
    dialer: Dialer,
    /// Messages from the [`Gossip`](super::Gossip) handle.
    to_actor_rx: mpsc::Receiver<ToActor>,
    /// Sender for the state input (cloned into the connection loops)
    in_event_tx: mpsc::Sender<InEvent>,
    /// Input events to the state (emitted from the connection loops)
    in_event_rx: mpsc::Receiver<InEvent>,
    /// Queued timers
    timers: TimerMap<Timer>,
    /// Map of topics to their state.
    topics: HashMap<TopicId, TopicHandles>,
    /// Map of peers to their state.
    peers: HashMap<EndpointId, PeerState>,
    /// Sweeps idle peers after their retention period.
    peer_cleanup: Interval,
    /// Receives subscription commands and applies broadcast backpressure.
    command_rx: SubscriptionCommands,
    /// Internal queue of topic to close because all handles were dropped.
    quit_queue: VecDeque<TopicId>,
    /// Tasks for the connection loops, to keep track of panics.
    connection_tasks: JoinSet<(EndpointId, Connection, Result<bool, ConnectionLoopError>)>,
    metrics: Arc<Metrics>,
    topic_event_forwarders: JoinSet<TopicId>,
    address_lookup: GossipAddressLookup,
    /// The size of the queue to each peer, see [`QueueOptions::size`].
    send_queue_size: usize,
    /// Events that the handling of an event caused, handled after it.
    follow_ups: VecDeque<InEvent>,
    /// Scratch buffer for use in [`Self::handle_proto_event`].
    scratch_touched_peers: Vec<EndpointId>,
    /// Scratch buffer for use in [`Self::handle_proto_event`].
    scratch_aborted_peers: Vec<EndpointId>,
}

impl Actor {
    pub(super) fn new(
        endpoint: Endpoint,
        config: ActorConfig,
        metrics: Arc<Metrics>,
        address_lookup: GossipAddressLookup,
    ) -> (Self, mpsc::Sender<ToActor>) {
        let ActorConfig {
            proto: proto_config,
            alpn,
            send_queue,
        } = config;
        let peer_id = endpoint.id();
        let dialer = Dialer::new(endpoint.clone());
        let state = proto::State::new(
            peer_id,
            Default::default(),
            proto_config,
            rand::rngs::StdRng::from_rng(&mut rand::rng()),
        );
        let send_queue_size = queue::queue_size(send_queue.size, state.max_message_size());
        let (to_actor_tx, to_actor_rx) = mpsc::channel(TO_ACTOR_CAP);
        let (in_event_tx, in_event_rx) = mpsc::channel(IN_EVENT_CAP);
        let mut peer_cleanup = time::interval(PEER_RETENTION);
        peer_cleanup.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let actor = Actor {
            alpn,
            endpoint,
            state,
            dialer,
            to_actor_rx,
            in_event_rx,
            in_event_tx,
            timers: TimerMap::new(),
            command_rx: SubscriptionCommands::default(),
            peers: Default::default(),
            peer_cleanup,
            topics: Default::default(),
            quit_queue: Default::default(),
            connection_tasks: Default::default(),
            metrics,
            topic_event_forwarders: Default::default(),
            address_lookup,
            send_queue_size,
            follow_ups: Default::default(),
            scratch_touched_peers: Vec::new(),
            scratch_aborted_peers: Vec::new(),
        };

        (actor, to_actor_tx)
    }

    pub(super) async fn run(mut self) {
        let mut addr_update_stream = self.setup();

        while self.event_loop(&mut addr_update_stream).await {}
    }

    /// Handles a message from the [`Gossip`](super::Gossip) handle, and returns `false` to stop the actor.
    fn on_message(&mut self, msg: Option<ToActor>) -> bool {
        if msg.is_some() {
            self.metrics.actor_tick_rx.inc();
        }
        match msg {
            Some(ToActor::Join(join)) => {
                self.on_join(join, Instant::now());
                true
            }
            Some(ToActor::Connection(conn)) => {
                self.add_connection(conn);
                true
            }
            Some(ToActor::Shutdown { reply }) => {
                debug!("received shutdown message, quit all topics");
                self.quit_queue.extend(self.topics.keys().copied());
                self.process_quit_queue();
                debug!("all topics quit, stop gossip actor");
                reply.send(()).ok();
                false
            }
            None => {
                debug!("all gossip handles dropped, stop gossip actor");
                false
            }
        }
    }

    /// Performs the initial actor setup to run the [`Actor::event_loop`].
    ///
    /// This updates our current address and return it. It also returns the home relay stream and
    /// direct addr stream.
    fn setup(&mut self) -> impl Stream<Item = EndpointAddr> + Send + Unpin + use<> {
        let addr_update_stream = self.endpoint.watch_addr().stream();
        let initial_addr = self.endpoint.addr();
        self.on_addr_update(initial_addr);
        addr_update_stream
    }

    /// One event loop processing step.
    ///
    /// Returns `false` when the actor should stop.
    async fn event_loop(
        &mut self,
        addr_updates: &mut (impl Stream<Item = EndpointAddr> + Send + Unpin),
    ) -> bool {
        self.metrics.actor_tick_main.inc();
        // A shutdown skips the other branches. It waits only for the messages of the
        // handle queued before it. Each tick still runs the select below, so these
        // messages starve no other branch.
        if let Ok(msg) = self.to_actor_rx.try_recv() {
            if !self.on_message(Some(msg)) {
                return false;
            }
        }
        // Not biased: a publisher that always has a command ready would starve the
        // branches after it, such as received messages, timers and closed connections.
        tokio::select! {
            msg = self.to_actor_rx.recv() => {
                if !self.on_message(msg) {
                    return false;
                }
            }
            Some((topic, command)) = self.command_rx.next(|topic| {
                all_eager_peers_busy(&self.state, &self.peers, topic)
            }), if !self.command_rx.is_empty() => {
                self.on_command(topic, command);
            },
            _ = self.peer_cleanup.tick(), if !self.peers.is_empty() => {
                self.forget_peers(Instant::now());
            },
            Some(new_address) = addr_updates.next() => {
                self.metrics.actor_tick_endpoint.inc();
                self.on_addr_update(new_address);
            }
            (peer_id, res) = self.dialer.next_conn() => {
                self.metrics.actor_tick_dialer.inc();
                match res {
                    Ok(conn) => {
                        debug!(peer = %peer_id.fmt_short(), "dial successful");
                        self.metrics.actor_tick_dialer_success.inc();
                        self.add_connection(conn);
                    }
                    Err(err) => {
                        warn!(peer = %peer_id.fmt_short(), "dial failed: {err}");
                        self.metrics.actor_tick_dialer_failure.inc();
                        self.on_dial_failed(peer_id);
                    }
                }
            }
            event = self.in_event_rx.recv() => {
                self.metrics.actor_tick_in_event_rx.inc();
                let event = event.expect("unreachable: in_event_tx is never dropped before receiver");
                self.handle_in_event(event, Instant::now());
            }
            _ = async {
                match self.timers.first() {
                    Some(instant) => time::sleep_until(*instant).await,
                    None => std::future::pending().await,
                }
            } => {
                self.metrics.actor_tick_timers.inc();
                let now = Instant::now();
                while let Some((_instant, timer)) = self.timers.pop_before(now) {
                    self.handle_in_event(InEvent::TimerExpired(timer), now);
                }
            }
            Some(res) = self.connection_tasks.join_next(), if !self.connection_tasks.is_empty() => {
                let Some((peer_id, conn, result)) = task_output(res) else {
                    debug!("connection task cancelled, the runtime shuts down");
                    return false;
                };
                self.on_conn_task_finished(peer_id, conn, result);
            }
            Some(res) = self.topic_event_forwarders.join_next(), if !self.topic_event_forwarders.is_empty() => {
                let Some(topic_id) = task_output(res) else {
                    debug!("topic task cancelled, the runtime shuts down");
                    return false;
                };
                if let Some(handles) = self.topics.get_mut(&topic_id) {
                    if !handles.has_handles() {
                        self.quit_queue.push_back(topic_id);
                        self.process_quit_queue();
                    }
                }
            }
        }

        true
    }

    fn on_addr_update(&mut self, endpoint_addr: EndpointAddr) {
        let peer_data = encode_peer_data(&endpoint_addr.into());
        self.handle_in_event(InEvent::UpdatePeerData(peer_data), Instant::now());
    }

    fn on_command(&mut self, topic: TopicId, command: Option<Command>) {
        debug!(?topic, ?command, "handle command");
        let Some(handles) = self.topics.get_mut(&topic) else {
            warn!("received command for unknown topic");
            return;
        };
        match command {
            Some(command) => {
                let command = match command {
                    Command::Broadcast(message) => ProtoCommand::Broadcast(message, Scope::Swarm),
                    Command::BroadcastNeighbors(message) => {
                        ProtoCommand::Broadcast(message, Scope::Neighbors)
                    }
                    Command::JoinPeers(peers) => ProtoCommand::Join(peers),
                };
                self.handle_in_event(proto::InEvent::Command(topic, command), Instant::now());
            }
            None => {
                handles.command_receivers = handles
                    .command_receivers
                    .checked_sub(1)
                    .expect("finished command receiver must have been registered");
                if !handles.has_handles() {
                    self.quit_queue.push_back(topic);
                    self.process_quit_queue();
                }
            }
        }
    }

    fn add_connection(&mut self, conn: Connection) {
        let peer_id = conn.remote_id();
        let side = conn.side();
        let size = self.send_queue_size;
        let peer = self
            .peers
            .entry(peer_id)
            .or_insert_with(|| PeerState::new(size));
        let queue_lease = peer.handle_connection(side, conn.stable_id());
        let initial_topics = match queue_lease {
            Some(_) => self.state.topics_of(&peer_id).collect(),
            None => Vec::new(),
        };

        let aborted = peer.aborted.clone();
        let handles_close = peer.handles_close.clone();
        let max_message_size = self.state.max_message_size();
        let in_event_tx = self.in_event_tx.clone();

        // Spawn a task for this connection
        self.connection_tasks.spawn(
            async move {
                let res = connection_loop(
                    peer_id,
                    conn.clone(),
                    aborted,
                    queue_lease,
                    initial_topics,
                    in_event_tx,
                    max_message_size,
                    handles_close,
                )
                .await;
                // Closed here, so that a busy actor does not delay the close.
                if conn.close_reason().is_none() {
                    conn.close(0u32.into(), b"close from disconnect");
                }
                (peer_id, conn, res)
            }
            .instrument(error_span!("conn", peer = %peer_id.fmt_short())),
        );
    }

    fn on_dial_failed(&mut self, peer_id: EndpointId) {
        // A connection writes the queue, so the failed dial loses nothing.
        if !self
            .peers
            .get(&peer_id)
            .is_some_and(|peer| peer.writer.is_some())
        {
            self.handle_in_event(InEvent::PeerDisconnected(peer_id), Instant::now());
        }
    }

    /// Removes the peers whose retention ended.
    fn forget_peers(&mut self, now: Instant) {
        // A pending dial may still need the capability learned on an earlier connection.
        self.peers
            .retain(|id, peer| self.dialer.is_dialing(*id) || !peer.can_forget(now));
    }

    #[tracing::instrument(name = "conn", skip_all, fields(peer = %peer_id.fmt_short()))]
    fn on_conn_task_finished(
        &mut self,
        peer_id: EndpointId,
        conn: Connection,
        task_result: Result<bool, ConnectionLoopError>,
    ) {
        // The receive loop sends its messages before its task ends. Handle the ones
        // still queued first, so that a message never comes after its own close.
        // Only those: other connections can refill the queue while we handle it.
        for _ in 0..self.in_event_rx.len() {
            let Ok(event) = self.in_event_rx.try_recv() else {
                break;
            };
            self.handle_in_event(event, Instant::now());
        }
        let reason = conn.close_reason().expect("closed by its task");
        let retired = matches!(task_result, Ok(true));
        let error = task_result.err();
        debug!(%reason, ?error, "connection closed");
        let now = Instant::now();
        let finished = self
            .peers
            .get_mut(&peer_id)
            .expect("unfinished connection must keep its peer state")
            .connection_finished(conn.stable_id(), now, retired);
        match finished {
            Finished::Quiet => {}
            Finished::Disconnected => {
                debug!("active send connection closed, mark peer as disconnected");
                self.handle_in_event(InEvent::PeerDisconnected(peer_id), now);
            }
            Finished::Redial => {
                debug!("retired connection left messages, dial again");
                self.dialer.queue_dial(peer_id, self.alpn.clone());
            }
        }
    }

    /// Registers the topic of a join, and starts the join.
    fn on_join(&mut self, join: Join, now: Instant) {
        trace!(?join, "handle join");
        let Join {
            topic_id,
            bootstrap,
            events_tx,
            commands_rx,
            registered,
        } = join;
        let TopicHandles {
            events,
            command_receivers,
        } = self.topics.entry(topic_id).or_default();
        // The new subscriber first gets the current neighbors.
        let gone = self.state.neighbors(&topic_id).any(|neighbor| {
            let sent = events_tx.try_send(Event::NeighborUp(*neighbor));
            matches!(sent, Err(TrySendError::Closed(_)))
        });
        if !gone {
            let subscriber = topic_subscriber_loop(events_tx, events.subscribe());
            let forwarder = async move {
                subscriber.await;
                topic_id
            };
            self.topic_event_forwarders
                .spawn(forwarder.instrument(tracing::Span::current()));
        }
        self.command_rx.insert(topic_id, commands_rx);
        *command_receivers += 1;

        self.handle_in_event(
            InEvent::Command(
                topic_id,
                ProtoCommand::Join(bootstrap.into_iter().collect()),
            ),
            now,
        );
        // The caller may be gone already.
        registered.send(()).ok();
    }

    fn handle_in_event(&mut self, event: InEvent, now: Instant) {
        self.handle_with_follow_ups(event, now);
        self.process_quit_queue();
    }

    fn process_quit_queue(&mut self) {
        while let Some(topic_id) = self.quit_queue.pop_front() {
            self.handle_with_follow_ups(
                InEvent::Command(topic_id, ProtoCommand::Quit),
                Instant::now(),
            );
            if self.topics.remove(&topic_id).is_some() {
                tracing::debug!(%topic_id, "publishers and subscribers gone; unsubscribing");
            }
        }
    }

    fn handle_with_follow_ups(&mut self, event: InEvent, now: Instant) {
        self.handle_proto_event(event, now);
        while let Some(event) = self.follow_ups.pop_front() {
            self.handle_proto_event(event, now);
        }
    }

    /// Handles one event of the protocol, and queues the events it causes in [`Self::follow_ups`].
    fn handle_proto_event(&mut self, event: InEvent, now: Instant) {
        // Drop the queue first, so that a message the event causes dials anew.
        if let InEvent::PeerDisconnected(peer) = &event {
            if let Some(peer) = self.peers.get_mut(peer) {
                peer.discard_queue(now);
            }
        }
        // The peers whose use by a topic may change in this batch.
        let touched = &mut self.scratch_touched_peers;
        touched.clear();
        // Peers that overflowed in this batch. The follow-up disconnect drops what
        // we would queue for them, so we neither reopen their queue nor dial them.
        let aborted = &mut self.scratch_aborted_peers;
        aborted.clear();
        if let InEvent::RecvMessage(peer, _) | InEvent::PeerDisconnected(peer) = &event {
            touched.push(*peer);
        }
        if matches!(event, InEvent::TimerExpired(_)) {
            trace!(?event, "handle in_event");
        } else {
            debug!(?event, "handle in_event");
        };
        let out = self.state.handle(event, now, Some(&self.metrics));
        for event in out {
            if matches!(event, OutEvent::ScheduleTimer(_, _)) {
                trace!(?event, "handle out_event");
            } else {
                debug!(?event, "handle out_event");
            };
            match event {
                OutEvent::SendMessage(peer_id, message) => {
                    touched.push(peer_id);
                    if aborted.contains(&peer_id) {
                        continue;
                    }
                    let size = self.send_queue_size;
                    let peer = self
                        .peers
                        .entry(peer_id)
                        .or_insert_with(|| PeerState::new(size));
                    match peer.send(message) {
                        Pushed::Queued => {}
                        Pushed::Dropped(message) => {
                            trace!(peer = %peer_id.fmt_short(), "send queue full, drop the message");
                            self.metrics.msgs_dropped.inc();
                            self.follow_ups
                                .push_back(InEvent::SendDropped(peer_id, message));
                        }
                        Pushed::Overflow => {
                            warn!(peer = %peer_id.fmt_short(), "send queue full with a control message, abort the peer");
                            peer.abort(now);
                            aborted.push(peer_id);
                            self.follow_ups
                                .push_back(InEvent::PeerDisconnected(peer_id));
                            self.metrics.peers_send_queue_full.inc();
                        }
                    }
                    // Also after a dropped message, which can reopen a full closed queue.
                    if peer.needs_dial() && !self.dialer.is_dialing(peer_id) {
                        debug!(peer = %peer_id.fmt_short(), "start to dial");
                        self.dialer.queue_dial(peer_id, self.alpn.clone());
                    }
                    // An open queue without a writer relies on the dial: it either
                    // supplies the writer, or its failure disconnects the peer.
                    debug_assert!(
                        peer.writer.is_some()
                            || peer.queue.is_closed()
                            || self.dialer.is_dialing(peer_id),
                        "open queue without a writer or a dial"
                    );
                }
                OutEvent::EmitEvent(topic_id, event) => {
                    let Some(handles) = self.topics.get_mut(&topic_id) else {
                        warn!(?topic_id, "gossip state emitted event for unknown topic");
                        continue;
                    };
                    handles.events.send(event).ok();
                    if !handles.has_handles() {
                        self.quit_queue.push_back(topic_id);
                    }
                }
                OutEvent::ScheduleTimer(delay, timer) => {
                    self.timers.insert(now + delay, timer);
                }
                OutEvent::DisconnectPeer(peer_id) => {
                    debug!(peer=%peer_id.fmt_short(), "gossip state indicates disconnect");
                    touched.push(peer_id);
                }
                OutEvent::PeerData(endpoint_id, data) => match decode_peer_data(&data) {
                    Err(err) => warn!("Failed to decode {data:?} from {endpoint_id}: {err}"),
                    Ok(addr_info) => {
                        debug!(peer = ?endpoint_id, "add known addrs: {addr_info:?}");
                        let mut endpoint_addr = EndpointAddr::new(endpoint_id);
                        for addr in addr_info.direct_addresses {
                            endpoint_addr = endpoint_addr.with_ip_addr(addr);
                        }
                        if let Some(relay_url) = addr_info.relay_url {
                            endpoint_addr = endpoint_addr.with_relay_url(relay_url);
                        }

                        self.address_lookup.add(endpoint_addr);
                    }
                },
            }
        }
        for peer in touched.drain(..) {
            if let Some(state) = self.peers.get_mut(&peer) {
                if !self.state.uses_peer(&peer) {
                    state.queue.close();
                }
            }
        }
    }

    #[cfg(test)]
    fn joined(&self, topic: &TopicId) -> bool {
        self.state.neighbors(topic).next().is_some()
    }
}
