//! The actor that owns the protocol state and drives the connections.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use bytes::Bytes;
use iroh::{endpoint::Connection, Endpoint, EndpointAddr, EndpointId, PublicKey, Watcher};
use n0_future::{
    task::JoinSet,
    time::{self, Duration, Instant, Interval, MissedTickBehavior},
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
    InEvent, Join, ProtoCommand, ProtoEvent, ProtoMessage, QueueOptions, Timer, ToActor,
    GOSSIP_ALPN,
};
use crate::{
    api::{Command, Event},
    metrics::Metrics,
    proto::{self, state::MessageKind, util::TimerMap, PeerData, Scope, TopicId, IO},
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
    /// The state of the gossip protocol.
    pub(super) proto: proto::State<PublicKey, StdRng>,
    state: ActorState,
    /// The endpoint of this node.
    pub(super) endpoint: Endpoint,
    /// Messages from the [`Gossip`](super::Gossip) handle.
    to_actor_rx: mpsc::Receiver<ToActor>,
    /// Sender for the state input (cloned into the connection loops)
    in_event_tx: mpsc::Sender<InEvent>,
    /// Input events to the state (emitted from the connection loops)
    in_event_rx: mpsc::Receiver<InEvent>,
    /// Sweeps idle peers after their retention period.
    peer_cleanup: Interval,
    /// Receives subscription commands and applies broadcast backpressure.
    command_rx: SubscriptionCommands,
    /// Tasks for the connection loops, to keep track of panics.
    connection_tasks: JoinSet<(EndpointId, Connection, Result<bool, ConnectionLoopError>)>,
    topic_event_forwarders: JoinSet<TopicId>,
    metrics: Arc<Metrics>,
}

/// The state of the actor that the protocol reads and changes through [`ActorIO`].
struct ActorState {
    /// Map of peers to their state.
    peers: HashMap<EndpointId, PeerState>,
    /// The size of the queue to each peer, see [`QueueOptions::size`].
    send_queue_size: usize,
    /// Dial machine to connect to peers.
    dialer: Dialer,
    /// Map of topics to their state.
    topics: HashMap<TopicId, TopicHandles>,
    /// Internal queue of topics to close because all handles were dropped.
    quit_queue: VecDeque<TopicId>,
    /// Events that the handling of an event caused, handled after it.
    follow_ups: VecDeque<InEvent>,
    /// Queued timers
    timers: TimerMap<Timer>,
    address_lookup: GossipAddressLookup,
    metrics: Arc<Metrics>,
    /// The peers that overflowed in the current call of [`Actor::run_proto`].
    overflowed_peers: Vec<EndpointId>,
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
        let proto = proto::State::new(
            peer_id,
            Default::default(),
            proto_config,
            rand::rngs::StdRng::from_rng(&mut rand::rng()),
        );
        let send_queue_size = queue::queue_size(send_queue.size, proto.max_message_size());
        let (to_actor_tx, to_actor_rx) = mpsc::channel(TO_ACTOR_CAP);
        let (in_event_tx, in_event_rx) = mpsc::channel(IN_EVENT_CAP);
        let mut peer_cleanup = time::interval(PEER_RETENTION);
        peer_cleanup.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let state = ActorState {
            peers: Default::default(),
            send_queue_size,
            dialer: Dialer::new(endpoint.clone(), alpn),
            topics: Default::default(),
            quit_queue: Default::default(),
            follow_ups: Default::default(),
            timers: TimerMap::new(),
            address_lookup,
            metrics: metrics.clone(),
            overflowed_peers: Vec::new(),
        };

        let actor = Actor {
            proto,
            state,
            endpoint,
            to_actor_rx,
            in_event_rx,
            in_event_tx,
            command_rx: SubscriptionCommands::default(),
            peer_cleanup,
            connection_tasks: Default::default(),
            topic_event_forwarders: Default::default(),
            metrics,
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
                self.state
                    .quit_queue
                    .extend(self.state.topics.keys().copied());
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
                all_eager_peers_busy(&self.proto, &self.state.peers, topic)
            }), if !self.command_rx.is_empty() => {
                self.on_command(topic, command);
            },
            _ = self.peer_cleanup.tick(), if !self.state.peers.is_empty() => {
                self.forget_peers(Instant::now());
            },
            Some(new_address) = addr_updates.next() => {
                self.metrics.actor_tick_endpoint.inc();
                self.on_addr_update(new_address);
            }
            (peer_id, res) = self.state.dialer.next_conn() => {
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
                match self.state.timers.first() {
                    Some(instant) => time::sleep_until(*instant).await,
                    None => std::future::pending().await,
                }
            } => {
                self.metrics.actor_tick_timers.inc();
                let now = Instant::now();
                while let Some((_instant, timer)) = self.state.timers.pop_before(now) {
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
                if let Some(handles) = self.state.topics.get_mut(&topic_id) {
                    if !handles.has_handles() {
                        self.state.quit_queue.push_back(topic_id);
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
        let Some(handles) = self.state.topics.get_mut(&topic) else {
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
                    self.state.quit_queue.push_back(topic);
                    self.process_quit_queue();
                }
            }
        }
    }

    fn add_connection(&mut self, conn: Connection) {
        let peer_id = conn.remote_id();
        let side = conn.side();
        let size = self.state.send_queue_size;
        let peer = self
            .state
            .peers
            .entry(peer_id)
            .or_insert_with(|| PeerState::new(size));
        let queue_lease = peer.handle_connection(side, conn.stable_id());
        let initial_topics = match queue_lease {
            Some(_) => self.proto.topics_wanting(&peer_id).collect(),
            None => Vec::new(),
        };

        let aborted = peer.aborted.clone();
        let handles_close = peer.handles_close.clone();
        let max_message_size = self.proto.max_message_size();
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
            .state
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
        self.state
            .peers
            .retain(|id, peer| self.state.dialer.is_dialing(*id) || !peer.can_forget(now));
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
            .state
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
                self.state.dialer.queue_dial(peer_id);
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
        } = self.state.topics.entry(topic_id).or_default();
        // The new subscriber first gets the current neighbors.
        let gone = self.proto.neighbors(&topic_id).any(|neighbor| {
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
        while let Some(topic_id) = self.state.quit_queue.pop_front() {
            self.handle_with_follow_ups(
                InEvent::Command(topic_id, ProtoCommand::Quit),
                Instant::now(),
            );
            if self.state.topics.remove(&topic_id).is_some() {
                debug!(%topic_id, "no publisher or subscriber left, leave the topic");
            }
        }
    }

    fn handle_with_follow_ups(&mut self, event: InEvent, now: Instant) {
        self.run_proto(event, now);
        while let Some(event) = self.state.follow_ups.pop_front() {
            self.run_proto(event, now);
        }
    }

    /// Handles one event of the protocol, and queues the events it causes in [`ActorState::follow_ups`].
    fn run_proto(&mut self, event: InEvent, now: Instant) {
        // Drop the queue first, so that a message the event causes dials anew.
        if let InEvent::PeerDisconnected(peer) = &event {
            if let Some(peer) = self.state.peers.get_mut(peer) {
                peer.discard_queue(now);
            }
        }
        // The follow-up disconnect drops what we would queue for the peers that
        // overflowed, so we neither reopen their queue nor dial them.
        self.state.overflowed_peers.clear();
        if matches!(event, InEvent::TimerExpired(_)) {
            trace!(?event, "handle in_event");
        } else {
            debug!(?event, "handle in_event");
        };
        if let InEvent::RecvMessage(_from, message) = &event {
            let len = super::wire::message_len(&message.message).ok();
            track_message(message, len, false, &self.metrics);
        }
        let may_change_wants = event.may_change_wants();
        let mut io = ActorIO {
            state: &mut self.state,
            now,
            opened_queue: false,
        };
        self.proto.handle(event, now, &mut io);
        // A topic wants a peer only through its views and its open requests, which
        // only a membership event changes. A send also opens a closed queue. A Plumtree
        // reply to a peer that no topic wants must not leave its queue open.
        if may_change_wants || io.opened_queue {
            for (peer_id, peer) in self.state.peers.iter() {
                if !peer.queue.is_closed() && !self.proto.wants(peer_id) {
                    peer.queue.close();
                }
            }
        }
    }

    #[cfg(test)]
    fn joined(&self, topic: &TopicId) -> bool {
        self.proto.neighbors(topic).next().is_some()
    }
}

/// Counts a message of `len` encoded bytes in the metrics, sent or received.
fn track_message(message: &ProtoMessage, len: Option<usize>, sent: bool, metrics: &Metrics) {
    let len = len.unwrap_or(0) as u64;
    let (count, size) = match (message.kind(), sent) {
        (MessageKind::Data, true) => (&metrics.msgs_data_sent, &metrics.msgs_data_sent_size),
        (MessageKind::Data, false) => (&metrics.msgs_data_recv, &metrics.msgs_data_recv_size),
        (MessageKind::Control, true) => (&metrics.msgs_ctrl_sent, &metrics.msgs_ctrl_sent_size),
        (MessageKind::Control, false) => (&metrics.msgs_ctrl_recv, &metrics.msgs_ctrl_recv_size),
    };
    count.inc();
    size.inc_by(len);
}

/// The [`IO`] of the protocol state for one call of [`proto::State::handle`].
///
/// It sends to the peers and the topics, and sets the timers.
struct ActorIO<'a> {
    state: &'a mut ActorState,
    now: Instant,
    /// Whether a send opened a closed queue.
    opened_queue: bool,
}

impl IO<PublicKey> for ActorIO<'_> {
    fn send(&mut self, peer_id: PublicKey, message: ProtoMessage) -> Option<ProtoMessage> {
        let state = &mut *self.state;
        debug!(peer = %peer_id.fmt_short(), ?message, "send");
        let len = super::wire::message_len(&message.message).ok();
        track_message(&message, len, true, &state.metrics);
        if state.overflowed_peers.contains(&peer_id) {
            return None;
        }
        let size = state.send_queue_size;
        let peer = state
            .peers
            .entry(peer_id)
            .or_insert_with(|| PeerState::new(size));
        self.opened_queue |= peer.queue.is_closed();
        let dropped = match peer.push(message, len) {
            Pushed::Queued => None,
            Pushed::Dropped(message) => {
                trace!(peer = %peer_id.fmt_short(), "send queue full, drop the message");
                state.metrics.msgs_dropped.inc();
                Some(message)
            }
            Pushed::Overflow => {
                warn!(peer = %peer_id.fmt_short(), "send queue full with a control message, abort the peer");
                peer.abort(self.now);
                state.overflowed_peers.push(peer_id);
                state
                    .follow_ups
                    .push_back(InEvent::PeerDisconnected(peer_id));
                state.metrics.peers_send_queue_full.inc();
                None
            }
        };
        // Also after a dropped message, which can reopen a full closed queue.
        if peer.needs_dial() && !state.dialer.is_dialing(peer_id) {
            debug!(peer = %peer_id.fmt_short(), "start to dial");
            state.dialer.queue_dial(peer_id);
        }
        // An open queue without a writer relies on the dial: it either
        // supplies the writer, or its failure disconnects the peer.
        debug_assert!(
            peer.writer.is_some() || peer.queue.is_closed() || state.dialer.is_dialing(peer_id),
            "open queue without a writer or a dial"
        );
        dropped
    }

    fn emit(&mut self, topic_id: TopicId, event: ProtoEvent) {
        debug!(?topic_id, ?event, "emit");
        match &event {
            ProtoEvent::NeighborUp(_) => {
                self.state.metrics.neighbor_up.inc();
            }
            ProtoEvent::NeighborDown(_) => {
                self.state.metrics.neighbor_down.inc();
            }
            _ => {}
        }
        let Some(handles) = self.state.topics.get_mut(&topic_id) else {
            warn!(?topic_id, "gossip state emitted event for unknown topic");
            return;
        };
        handles.events.send(event).ok();
        if !handles.has_handles() {
            self.state.quit_queue.push_back(topic_id);
        }
    }

    fn schedule(&mut self, delay: Duration, timer: Timer) {
        trace!(?delay, ?timer, "schedule");
        self.state.timers.insert(self.now + delay, timer);
    }

    fn peer_data(&mut self, endpoint_id: PublicKey, data: PeerData) {
        match decode_peer_data(&data) {
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

                self.state.address_lookup.add(endpoint_addr);
            }
        }
    }
}
