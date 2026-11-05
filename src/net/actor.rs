//! The actor that owns the protocol state and drives the connections.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use bytes::Bytes;
use iroh::{endpoint::Connection, Endpoint, EndpointAddr, EndpointId, PublicKey, Watcher};
use n0_future::{
    boxed::BoxFuture,
    task::JoinSet,
    time::{self, Duration, Instant},
    FuturesUnordered, Stream, StreamExt as _,
};
use rand::{rngs::StdRng, SeedableRng};
use tokio::sync::{mpsc, mpsc::error::TrySendError};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error_span, trace, warn, Instrument};

use super::{
    address_lookup::{decode_peer_data, encode_peer_data, GossipAddressLookup},
    auth::TopicAuth,
    conn::{
        connection_loop, task_output, ConnArgs, ConnEvent, ConnEventKind, ConnExit, ConnShared,
        Mailbox, TakeQueue,
    },
    dialer::Dialer,
    peers::{Action, ConnId, PeerState, Peers},
    queue::{self, Pushed},
    subscriptions::{topic_subscriber_loop, TopicHandles},
    wire::{CloseCode, Session},
    InEvent, Join, MessageFilter, ProtoCommand, ProtoEvent, ProtoMessage, QueueOptions, Timer,
    ToActor, GOSSIP_ALPN,
};
use crate::{
    api::{Command, Event},
    metrics::Metrics,
    proto::{self, state::MessageKind, util::TimerMap, PeerData, Scope, TopicId, IO},
};

#[cfg(test)]
pub(super) mod tests;

/// How long an idle connection waits before it retires, by default.
const DEFAULT_RETIRE_AFTER: Duration = Duration::from_secs(10);

/// The settings that the [`Builder`](super::Builder) passes to the actor.
#[derive(Debug, Clone)]
pub(super) struct ActorConfig {
    pub(super) proto: proto::Config,
    pub(super) alpn: Bytes,
    pub(super) send_queue: QueueOptions,
    /// The application's filter of received messages, if any.
    pub(super) filter: Option<MessageFilter>,
    pub(super) topic_auth: TopicAuth,
    /// How long an idle connection waits before it retires.
    pub(super) retire_after: Duration,
}

impl Default for ActorConfig {
    fn default() -> Self {
        Self {
            proto: Default::default(),
            alpn: GOSSIP_ALPN.to_vec().into(),
            send_queue: Default::default(),
            filter: None,
            topic_auth: Default::default(),
            retire_after: DEFAULT_RETIRE_AFTER,
        }
    }
}

/// How long a shutdown waits for the connections to send what is queued and to close.
///
/// A peer that reads nothing must not hold up the shutdown (#47).
const SHUTDOWN_DRAIN: Duration = Duration::from_secs(1);

/// The next command of a subscription, with the receiver for the commands after it.
type CommandFuture = BoxFuture<(TopicId, mpsc::Receiver<Command>, Option<Command>)>;

/// Waits for the next command of a subscription.
fn next_command(topic: TopicId, mut commands: mpsc::Receiver<Command>) -> CommandFuture {
    Box::pin(async move {
        let command = commands.recv().await;
        (topic, commands, command)
    })
}

/// The actor side of a connection task.
#[derive(Debug)]
struct ConnHandle {
    conn: Connection,
    /// Retires the connection after it lost a crossed dial.
    retire: CancellationToken,
    /// Gives the writer of the connection the queue of the peer.
    take_queue: Arc<Mailbox<TakeQueue>>,
}

/// The capacity of the channel from the [`Gossip`](super::Gossip) handle to the actor.
const TO_ACTOR_CAP: usize = 64;
/// The capacity of the channel from the connection tasks to the actor.
const CONN_EVENT_CAP: usize = 1024;

/// The actor that owns the protocol state and the peers, and runs the connection tasks.
pub(super) struct Actor {
    /// The state of the gossip protocol.
    pub(super) proto: proto::State<PublicKey, StdRng>,
    state: ActorState,
    /// The endpoint of this node.
    pub(super) endpoint: Endpoint,
    /// Messages from the [`Gossip`](super::Gossip) handle.
    to_actor_rx: mpsc::Receiver<ToActor>,
    /// The inputs that every connection task shares.
    conn_shared: ConnShared,
    /// Events of the connection tasks.
    conn_events_rx: mpsc::Receiver<ConnEvent>,
    /// The actor side of each running connection task.
    conns: HashMap<ConnId, ConnHandle>,
    /// The id of the next connection.
    next_conn: ConnId,
    /// The next command of each subscription.
    command_rx: FuturesUnordered<CommandFuture>,
    /// Tasks for the connection loops, to keep track of panics.
    connection_tasks: JoinSet<(EndpointId, ConnId, ConnExit)>,
    topic_event_forwarders: JoinSet<TopicId>,
    metrics: Arc<Metrics>,
}

/// The state of the actor that the protocol reads and changes through [`ActorIO`].
struct ActorState {
    /// The state of each peer.
    peers: Peers,
    /// Dial machine to connect to peers.
    dialer: Dialer,
    /// Map of topics to their state.
    topics: HashMap<TopicId, TopicHandles>,
    /// The tokens that we send, by topic.
    tokens: HashMap<TopicId, Bytes>,
    /// Internal queue of topics to close because all handles were dropped.
    quit_queue: VecDeque<TopicId>,
    /// Events that the handling of an event caused, handled after it.
    follow_ups: VecDeque<InEvent>,
    /// Queued timers
    timers: TimerMap<Timer>,
    /// The application's filter of received messages, if any.
    filter: Option<MessageFilter>,
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
            filter,
            topic_auth,
            retire_after,
        } = config;
        let me = endpoint.id();
        let proto = proto::State::new(
            me,
            Default::default(),
            proto_config,
            rand::rngs::StdRng::from_rng(&mut rand::rng()),
        );
        let max_message_size = proto.max_message_size();
        let (to_actor_tx, to_actor_rx) = mpsc::channel(TO_ACTOR_CAP);
        let (conn_events_tx, conn_events_rx) = mpsc::channel(CONN_EVENT_CAP);
        let conn_shared = ConnShared {
            events: conn_events_tx,
            max_message_size,
            verifier: topic_auth,
            session: Session::random(),
            retire_after,
            shutdown: CancellationToken::new(),
        };
        let state = ActorState {
            peers: Peers::new(me, queue::queue_size(send_queue.size, max_message_size)),
            dialer: Dialer::new(endpoint.clone(), alpn),
            topics: Default::default(),
            tokens: Default::default(),
            quit_queue: Default::default(),
            follow_ups: Default::default(),
            timers: TimerMap::new(),
            filter,
            address_lookup,
            metrics: metrics.clone(),
            overflowed_peers: Vec::new(),
        };

        let actor = Actor {
            proto,
            state,
            endpoint,
            to_actor_rx,
            conn_shared,
            conn_events_rx,
            conns: Default::default(),
            next_conn: 0,
            command_rx: Default::default(),
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
    async fn on_message(&mut self, msg: Option<ToActor>) -> bool {
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
                self.shutdown().await;
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

    /// Leaves all topics, and gives the writers time to send what is left.
    ///
    /// Each writer finishes its streams once its queue is empty, and its
    /// connection closes with [`CloseCode::Shutdown`], which the peers see as a
    /// failure. The drain takes at most [`SHUTDOWN_DRAIN`]. Then the actor closes the rest.
    async fn shutdown(&mut self) {
        self.state
            .quit_queue
            .extend(self.state.topics.keys().copied());
        self.process_quit_queue();
        self.conn_shared.shutdown.cancel();
        let drain = async {
            // A task ends only after it sent its events, so the drain drops the events meanwhile.
            while !self.connection_tasks.is_empty() {
                tokio::select! {
                    _ = self.conn_events_rx.recv() => {}
                    res = self.connection_tasks.join_next() => {
                        task_output(res.expect("not empty"));
                    }
                }
            }
        };
        if time::timeout(SHUTDOWN_DRAIN, drain).await.is_err() {
            warn!("the connections did not drain, close them");
        }
        for handle in self.conns.values() {
            handle.conn.close(CloseCode::Shutdown.code(), b"shutdown");
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
            if !self.on_message(Some(msg)).await {
                return false;
            }
        }
        // No await in a select branch. A shutdown waits up to [`SHUTDOWN_DRAIN`].
        let mut message = None;
        // Not biased: a publisher that always has a command ready would starve the
        // branches after it, such as received messages, timers and closed connections.
        tokio::select! {
            msg = self.to_actor_rx.recv() => {
                message = Some(msg);
            }
            Some((topic, commands, command)) = self.command_rx.next(), if !self.command_rx.is_empty() => {
                if command.is_some() {
                    self.command_rx.push(next_command(topic, commands));
                }
                self.on_command(topic, command);
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
            event = self.conn_events_rx.recv() => {
                self.metrics.actor_tick_in_event_rx.inc();
                let event = event.expect("unreachable: conn_shared.events is never dropped before receiver");
                self.on_conn_event(event, Instant::now());
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
                let Some((peer_id, id, exit)) = task_output(res) else {
                    debug!("connection task cancelled, the runtime shuts down");
                    return false;
                };
                self.on_conn_task_finished(peer_id, id, exit);
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

        match message {
            Some(msg) => self.on_message(msg).await,
            None => true,
        }
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
        let id = self.next_conn;
        self.next_conn += 1;
        let peer = self.state.peers.get_or_insert(peer_id);
        let mut actions = Vec::new();
        peer.add_conn(id, side, &mut actions);

        let handle = ConnHandle {
            conn: conn.clone(),
            retire: CancellationToken::new(),
            take_queue: Default::default(),
        };
        let args = ConnArgs {
            id,
            conn,
            aborted: peer.aborted.clone(),
            retire: handle.retire.clone(),
            take_queue: handle.take_queue.clone(),
            shared: self.conn_shared.clone(),
        };
        self.conns.insert(id, handle);
        self.connection_tasks.spawn(
            async move { (peer_id, id, connection_loop(args).await) }
                .instrument(error_span!("conn", peer = %peer_id.fmt_short())),
        );
        let now = Instant::now();
        self.perform_peer_actions(peer_id, actions, now);
        self.handle_in_event(InEvent::PeerConnected(peer_id), now);
    }

    /// Performs the actions that the state of a peer returned.
    fn perform_peer_actions(&mut self, peer_id: EndpointId, actions: Vec<Action>, now: Instant) {
        for action in actions {
            match action {
                Action::Close(id, code) => {
                    if let Some(handle) = self.conns.get(&id) {
                        debug!(?code, "close a connection");
                        handle.conn.close(code.code(), b"closed by the node");
                    }
                }
                Action::Retire(id) => {
                    if let Some(handle) = self.conns.get(&id) {
                        handle.retire.cancel();
                    }
                }
                Action::TakeQueue(id) => {
                    let Some(handle) = self.conns.get(&id) else {
                        continue;
                    };
                    let peer = self.state.peers.get_mut(&peer_id);
                    let Some(reader) = peer.and_then(|peer| peer.reader_for(id)) else {
                        continue;
                    };
                    let topics = self
                        .proto
                        .topics_wanting(&peer_id)
                        .map(|topic| (topic, self.state.tokens.get(&topic).cloned()))
                        .collect();
                    handle.take_queue.put(TakeQueue { reader, topics });
                }
                Action::Disconnect => {
                    debug!(peer=%peer_id.fmt_short(), "report the peer as disconnected");
                    self.handle_in_event(InEvent::PeerDisconnected(peer_id), now);
                }
                Action::Dial => {
                    debug!(peer=%peer_id.fmt_short(), "dial for the queued messages");
                    self.state.dialer.queue_dial(peer_id);
                }
            }
        }
    }

    fn on_dial_failed(&mut self, peer_id: EndpointId) {
        let mut actions = Vec::new();
        match self.state.peers.get_mut(&peer_id) {
            Some(peer) => peer.on_dial_failed(&mut actions),
            None => actions.push(Action::Disconnect),
        }
        self.perform_peer_actions(peer_id, actions, Instant::now());
        self.state.forget_if_idle(&peer_id);
    }

    /// Handles an event of a connection task, unless the connection is gone.
    fn on_conn_event(&mut self, event: ConnEvent, now: Instant) {
        let ConnEvent { peer, conn, kind } = event;
        let Some(peer_state) = self
            .state
            .peers
            .get_mut(&peer)
            .filter(|peer_state| peer_state.has_conn(conn))
        else {
            return;
        };
        match kind {
            ConnEventKind::Message(message) => {
                self.handle_in_event(InEvent::RecvMessage(peer, message), now)
            }
            ConnEventKind::Refused(topic) => {
                self.handle_in_event(InEvent::PeerRefused(topic, peer), now)
            }
            ConnEventKind::Rejected(topic) => {
                self.handle_in_event(InEvent::PeerRejected(topic, peer), now)
            }
            ConnEventKind::Header(session) => {
                let mut actions = Vec::new();
                peer_state.on_header(conn, session, &mut actions);
                self.perform_peer_actions(peer, actions, now);
            }
            ConnEventKind::Retiring => {
                let mut actions = Vec::new();
                peer_state.on_retiring(conn, &mut actions);
                self.perform_peer_actions(peer, actions, now);
            }
        }
    }

    #[tracing::instrument(name = "conn", skip_all, fields(peer = %peer_id.fmt_short()))]
    fn on_conn_task_finished(&mut self, peer_id: EndpointId, id: ConnId, exit: ConnExit) {
        // The receive loop sends its messages before its task ends. Handle the ones
        // still queued first, so that a message never comes after its own close.
        // Only those: other connections can refill the queue while we handle it.
        for _ in 0..self.conn_events_rx.len() {
            let Ok(event) = self.conn_events_rx.try_recv() else {
                break;
            };
            self.on_conn_event(event, Instant::now());
        }
        let ConnExit { end, reader } = exit;
        debug!(?end, "connection closed");
        // A reader that the writer did not take is newer than the one it had.
        let untaken = self
            .conns
            .remove(&id)
            .and_then(|handle| handle.take_queue.try_take());
        let reader = untaken.map(|take| take.reader).or(reader);
        let mut actions = Vec::new();
        if let Some(peer) = self.state.peers.get_mut(&peer_id) {
            peer.on_closed(id, end, reader, &mut actions);
        }
        self.perform_peer_actions(peer_id, actions, Instant::now());
        self.state.forget_if_idle(&peer_id);
    }

    /// Registers the topic of a join, and starts the join.
    fn on_join(&mut self, join: Join, now: Instant) {
        trace!(?join, "handle join");
        let Join {
            topic_id,
            bootstrap,
            auth_token,
            events_tx,
            commands_rx,
            registered,
        } = join;
        if let Some(token) = auth_token {
            let tokens = &mut self.state.tokens;
            if *tokens.entry(topic_id).or_insert_with(|| token.clone()) != token {
                warn!(topic = %topic_id.fmt_short(), "joined with another auth token than the topic has, keep the first");
            }
        }
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
        self.command_rx.push(next_command(topic_id, commands_rx));
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
            self.state.tokens.remove(&topic_id);
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

impl ActorState {
    /// Removes the state of a peer that holds nothing, unless a dial is pending.
    fn forget_if_idle(&mut self, peer_id: &EndpointId) {
        let idle = self.peers.get(peer_id).is_some_and(PeerState::is_idle);
        if idle && !self.dialer.is_dialing(*peer_id) {
            self.peers.remove(peer_id);
        }
    }
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
        let peer = state.peers.get_or_insert(peer_id);
        let token = state.tokens.get(&message.topic).cloned();
        self.opened_queue |= peer.queue.is_closed();
        let dropped = match peer.push(message, len, token) {
            Pushed::Queued => None,
            Pushed::Dropped(message) => {
                trace!(peer = %peer_id.fmt_short(), "send queue full, drop the message");
                state.metrics.msgs_dropped.inc();
                Some(message)
            }
            Pushed::Overflow => {
                warn!(peer = %peer_id.fmt_short(), "send queue full with a control message, abort the peer");
                peer.abort();
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
            peer.on_dial();
        }
        // An open queue without a connection relies on the dial: it either
        // supplies the connection, or its failure disconnects the peer.
        debug_assert!(
            peer.conn_count() > 0 || peer.queue.is_closed() || state.dialer.is_dialing(peer_id),
            "open queue without a connection or a dial"
        );
        dropped
    }

    fn connect(&mut self, peer_id: PublicKey) {
        let state = &mut *self.state;
        let peer = state.peers.get_or_insert(peer_id);
        self.opened_queue |= peer.queue.is_closed();
        peer.queue.open();
        if peer.conn_count() > 0 {
            state.follow_ups.push_back(InEvent::PeerConnected(peer_id));
        } else if !state.dialer.is_dialing(peer_id) {
            debug!(peer = %peer_id.fmt_short(), "start to dial");
            state.dialer.queue_dial(peer_id);
        }
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

    fn filter(&mut self, message: proto::FilterContext<'_, PublicKey>) -> Option<proto::Verdict> {
        self.state.filter.as_ref().map(|filter| (filter.0)(message))
    }
}
