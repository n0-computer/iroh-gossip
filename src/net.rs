//! Networking for the `iroh-gossip` protocol

use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
    time::Duration,
};

use bytes::Bytes;
use iroh::{
    endpoint::{Connection, Side},
    protocol::{AcceptError, ProtocolHandler},
    Endpoint, EndpointAddr, EndpointId, PublicKey, RelayUrl, Watcher,
};
use irpc::WithChannels;
use n0_error::{e, stack_error};
use n0_future::{
    task::{self, AbortOnDropHandle, JoinSet},
    time::{self, Instant, Interval, MissedTickBehavior, Sleep},
    FuturesUnordered, Stream, StreamExt as _,
};
use rand::{rngs::StdRng, SeedableRng};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, error_span, trace, warn, Instrument};

use self::{
    address_lookup::GossipAddressLookup,
    queue::{Pushed, QueueLease, SendQueue},
    util::{RecvLoop, SendLoop, Timers},
};
use crate::{
    api::{self, Command, Event, GossipApi, RpcMessage},
    metrics::Metrics,
    proto::{self, HyparviewConfig, PeerData, PlumtreeConfig, Scope, TopicId},
};

mod address_lookup;
mod queue;
mod util;

/// ALPN protocol name
pub const GOSSIP_ALPN: &[u8] = b"/iroh-gossip/1";

/// Channel capacity for the ToActor message queue (single)
const TO_ACTOR_CAP: usize = 64;
/// Channel capacity for the InEvent message queue (single)
const IN_EVENT_CAP: usize = 1024;
/// Channel capacity for broadcast subscriber event queue (one per topic)
const TOPIC_EVENT_CAP: usize = 256;

/// Events emitted from the gossip protocol
pub type ProtoEvent = proto::Event<PublicKey>;
/// Commands for the gossip protocol
pub type ProtoCommand = proto::Command<PublicKey>;

type InEvent = proto::InEvent<PublicKey>;
type OutEvent = proto::OutEvent<PublicKey>;
type Timer = proto::Timer<PublicKey>;
type ProtoMessage = proto::Message<PublicKey>;

/// Publish and subscribe on gossiping topics.
///
/// Each topic is a separate broadcast tree with separate memberships.
///
/// A topic has to be joined before you can publish or subscribe on the topic.
/// To join the swarm for a topic, you have to know the [`PublicKey`] of at least one peer that also joined the topic.
///
/// Messages published on the swarm will be delivered to all peers that joined the swarm for that
/// topic. You will also be relaying (gossiping) messages published by other peers.
///
/// With the default settings, the protocol will maintain up to 5 peer connections per topic.
///
/// Even though the [`Gossip`] is created from a [`Endpoint`], it does not accept connections
/// itself. You should run an accept loop on the [`Endpoint`] yourself, check the ALPN protocol of incoming
/// connections, and if the ALPN protocol equals [`GOSSIP_ALPN`], forward the connection to the
/// gossip actor through [Self::handle_connection].
///
/// The gossip actor will, however, initiate new connections to other peers by itself.
#[derive(Debug, Clone)]
pub struct Gossip {
    pub(crate) inner: Arc<Inner>,
}

impl std::ops::Deref for Gossip {
    type Target = GossipApi;
    fn deref(&self) -> &Self::Target {
        &self.inner.api
    }
}

#[derive(Debug)]
enum LocalActorMessage {
    HandleConnection(Connection),
    Shutdown { reply: oneshot::Sender<()> },
}

#[allow(missing_docs)]
#[stack_error(derive, add_meta)]
#[non_exhaustive]
pub enum Error {
    ActorDropped {},
}

impl<T> From<mpsc::error::SendError<T>> for Error {
    fn from(_value: mpsc::error::SendError<T>) -> Self {
        e!(Error::ActorDropped)
    }
}
impl From<oneshot::error::RecvError> for Error {
    fn from(_value: oneshot::error::RecvError) -> Self {
        e!(Error::ActorDropped)
    }
}

#[derive(Debug)]
pub(crate) struct Inner {
    api: GossipApi,
    local_tx: mpsc::Sender<LocalActorMessage>,
    _actor_handle: AbortOnDropHandle<()>,
    max_message_size: usize,
    metrics: Arc<Metrics>,
}

impl ProtocolHandler for Gossip {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        self.handle_connection(connection)
            .await
            .map_err(AcceptError::from_err)?;
        Ok(())
    }

    async fn shutdown(&self) {
        if let Err(err) = self.shutdown().await {
            warn!("error while shutting down gossip: {err:#}");
        }
    }
}

/// Builder to configure and construct [`Gossip`].
#[derive(Debug, Clone)]
pub struct Builder {
    config: proto::Config,
    alpn: Option<Bytes>,
    send_queue: QueueOptions,
}

impl Builder {
    /// Sets the maximum message size in bytes.
    ///
    /// By default this is `4096` bytes. The limit applies to the encoded message,
    /// which is about 40 bytes longer than its content. [`GossipSender::broadcast`]
    /// refuses a message over the limit.
    ///
    /// Use the same limit on every node of a swarm. A node drops a message over
    /// its own limit, and the peers that sent it do not learn of it.
    ///
    /// [`GossipSender::broadcast`]: crate::api::GossipSender::broadcast
    pub fn max_message_size(mut self, size: usize) -> Self {
        self.config.max_message_size = size;
        self
    }

    /// Set the membership configuration.
    pub fn membership_config(mut self, config: HyparviewConfig) -> Self {
        self.config.membership = config;
        self
    }

    /// Set the broadcast configuration.
    pub fn broadcast_config(mut self, config: PlumtreeConfig) -> Self {
        self.config.broadcast = config;
        self
    }

    /// Set the ALPN this gossip instance uses.
    ///
    /// It has to be the same for all peers in the network. If you set a custom ALPN,
    /// you have to use the same ALPN when registering the [`Gossip`] in on a iroh
    /// router with [`RouterBuilder::accept`].
    ///
    /// [`RouterBuilder::accept`]: iroh::protocol::RouterBuilder::accept
    pub fn alpn(mut self, alpn: impl AsRef<[u8]>) -> Self {
        self.alpn = Some(alpn.as_ref().to_vec().into());
        self
    }

    /// Sets the options of the queue of messages to each peer.
    pub fn send_queue(mut self, options: QueueOptions) -> Self {
        self.send_queue = options;
        self
    }

    /// Spawn a gossip actor and get a handle for it
    pub fn spawn(self, endpoint: Endpoint) -> Gossip {
        let metrics = Arc::new(Metrics::default());
        let address_lookup = GossipAddressLookup::default();

        // `Endpoint::address_lookup` returns `Err` when the endpoint is closed.
        // In that case, the gossip actor will close too very soon for other reasons,
        // so it's fine if we only add our `GossipAddressLookup` for the non-closed
        // case. The alternative would be to return a `Result` from `spawn`,
        // but as long as this is the only direct error case, it seem unwarranted.
        if let Ok(endpoint_addr_lookup) = endpoint.address_lookup().as_ref() {
            endpoint_addr_lookup.add(address_lookup.clone());
        }
        let (mut actor, rpc_tx, local_tx) = Actor::new(
            endpoint,
            self.config,
            metrics.clone(),
            self.alpn,
            address_lookup,
        );
        actor.send_queue_size =
            queue::queue_size(self.send_queue.size, actor.state.max_message_size());
        let me = actor.endpoint.id().fmt_short();
        let max_message_size = actor.state.max_message_size();

        let actor_handle = task::spawn(actor.run().instrument(error_span!("gossip", %me)));

        let api = GossipApi::local(rpc_tx, max_message_size);

        Gossip {
            inner: Inner {
                api,
                local_tx,
                _actor_handle: AbortOnDropHandle::new(actor_handle),
                max_message_size,
                metrics,
            }
            .into(),
        }
    }
}

/// Options of the send queue to each peer, for [`Builder::send_queue`].
///
/// Gossip keeps one send queue per peer, shared between all topics. The queue
/// holds up to [`Self::size`] bytes of messages on their way to that peer.
/// When the queue becomes full, new data and announcement messages to that peer
/// will be dropped to not hold up the whole swarm. If this peer cannot keep
/// up with the volume of messages to be sent or relayed, the following
/// may happen:
///
/// - A broadcast to the peer is dropped, and [`Metrics::msgs_dropped`] counts
///   it. Gossip announces the broadcast to the peer again, and the peer
///   fetches it while it is still cached, 30 s by default. A broadcast to
///   direct neighbors is never repaired.
/// - A control message to the peer closes the connection, and
///   [`Metrics::peers_send_queue_full`] counts it. Both sides find new
///   neighbors.
/// - Once the queues to all peers that a topic pushes to are half full of data,
///   [`GossipSender::broadcast`] on that topic waits for room, up to 100 ms
///   per message. Relayed traffic and other topics fill the same queues, so
///   this slows a publisher that sends little as well.
///
/// Independent of the size:
///
/// - Messages to a slow peer arrive late. The queue bounds bytes, not time,
///   and QUIC buffers more on top.
/// - A peer that takes no data for 60 s loses its connection, as does one
///   that shares more topics with this node than its stream limit allows,
///   100 by default.
///
/// [`GossipSender::broadcast`]: crate::api::GossipSender::broadcast
#[derive(Debug, Clone, Copy)]
pub struct QueueOptions {
    size: usize,
}

impl Default for QueueOptions {
    fn default() -> Self {
        Self {
            size: queue::QUEUE_SIZE,
        }
    }
}

impl QueueOptions {
    /// Sets the size of the queue in bytes, 1 MiB by default.
    ///
    /// A larger queue absorbs larger bursts and longer stalls without a
    /// loss. It costs up to its size in memory for each peer, and it lets
    /// more old data wait for a slow one. A message takes its encoded size
    /// and about 130 bytes more. Whatever the setting, the queue is at
    /// least 64 KiB, and fits eight messages of [`Builder::max_message_size`].
    pub fn size(mut self, size: usize) -> Self {
        self.size = size;
        self
    }
}

impl Gossip {
    /// Creates a default `Builder`, with the endpoint set.
    pub fn builder() -> Builder {
        Builder {
            config: Default::default(),
            alpn: None,
            send_queue: Default::default(),
        }
    }

    /// Listen on a noq endpoint for incoming RPC connections.
    #[cfg(feature = "rpc")]
    pub async fn listen(self, endpoint: noq::Endpoint) {
        self.inner.api.listen(endpoint).await
    }

    /// Get the maximum message size configured for this gossip actor.
    pub fn max_message_size(&self) -> usize {
        self.inner.max_message_size
    }

    /// Handle an incoming [`Connection`].
    ///
    /// The connection should be an accepted, server-side connection.
    /// Make sure to check the ALPN protocol yourself before passing the connection.
    pub async fn handle_connection(&self, conn: Connection) -> Result<(), Error> {
        self.inner
            .local_tx
            .send(LocalActorMessage::HandleConnection(conn))
            .await?;
        Ok(())
    }

    /// Shutdown the gossip instance.
    ///
    /// This leaves all topics, sending `Disconnect` messages to peers, and then
    /// stops the gossip actor loop and drops all state and connections. A gossip
    /// instance that is shut down already, or shutting down, returns `Ok` too.
    pub async fn shutdown(&self) -> Result<(), Error> {
        let (reply, reply_rx) = oneshot::channel();
        let shutdown = LocalActorMessage::Shutdown { reply };
        // The actor is gone or about to go, which is what the caller asks for.
        if self.inner.local_tx.send(shutdown).await.is_ok() {
            reply_rx.await.ok();
        }
        Ok(())
    }

    /// Returns the metrics tracked for this gossip instance.
    pub fn metrics(&self) -> &Arc<Metrics> {
        &self.inner.metrics
    }
}

/// Actor that sends and handles messages between the connection and main state loops
struct Actor {
    alpn: Bytes,
    /// Protocol state
    state: proto::State<PublicKey, StdRng>,
    /// The endpoint through which we dial peers
    endpoint: Endpoint,
    /// Dial machine to connect to peers
    dialer: Dialer,
    /// Input messages to the actor
    rpc_rx: mpsc::Receiver<RpcMessage>,
    local_rx: mpsc::Receiver<LocalActorMessage>,
    /// Sender for the state input (cloned into the connection loops)
    in_event_tx: mpsc::Sender<InEvent>,
    /// Input events to the state (emitted from the connection loops)
    in_event_rx: mpsc::Receiver<InEvent>,
    /// Queued timers
    timers: Timers<Timer>,
    /// Map of topics to their state.
    topics: HashMap<TopicId, TopicState>,
    /// Map of peers to their state.
    peers: HashMap<EndpointId, PeerState>,
    /// Sweeps idle peers after their retention period.
    peer_cleanup: Interval,
    /// Receives subscription commands and applies broadcast backpressure.
    command_rx: SubscriptionCommands,
    /// Internal queue of topic to close because all handles were dropped.
    quit_queue: VecDeque<TopicId>,
    /// Tasks for the connection loops, to keep track of panics.
    connection_tasks: JoinSet<(EndpointId, Connection, Result<(), ConnectionLoopError>)>,
    metrics: Arc<Metrics>,
    topic_event_forwarders: JoinSet<TopicId>,
    address_lookup: GossipAddressLookup,
    /// The size of the queue to each peer, see [`QueueOptions::size`].
    send_queue_size: usize,
    /// Events that the handling of an event caused, handled after it.
    follow_ups: VecDeque<InEvent>,
}

impl Actor {
    fn new(
        endpoint: Endpoint,
        config: proto::Config,
        metrics: Arc<Metrics>,
        alpn: Option<Bytes>,
        address_lookup: GossipAddressLookup,
    ) -> (
        Self,
        mpsc::Sender<RpcMessage>,
        mpsc::Sender<LocalActorMessage>,
    ) {
        let peer_id = endpoint.id();
        let dialer = Dialer::new(endpoint.clone());
        let state = proto::State::new(
            peer_id,
            Default::default(),
            config,
            rand::rngs::StdRng::from_rng(&mut rand::rng()),
        );
        let send_queue_size = queue::queue_size(queue::QUEUE_SIZE, state.max_message_size());
        let (rpc_tx, rpc_rx) = mpsc::channel(TO_ACTOR_CAP);
        let (local_tx, local_rx) = mpsc::channel(16);
        let (in_event_tx, in_event_rx) = mpsc::channel(IN_EVENT_CAP);
        let mut peer_cleanup = time::interval(PEER_RETENTION);
        peer_cleanup.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let actor = Actor {
            alpn: alpn.unwrap_or_else(|| GOSSIP_ALPN.to_vec().into()),
            endpoint,
            state,
            dialer,
            rpc_rx,
            in_event_rx,
            in_event_tx,
            timers: Timers::new(),
            command_rx: SubscriptionCommands::default(),
            peers: Default::default(),
            peer_cleanup,
            topics: Default::default(),
            quit_queue: Default::default(),
            connection_tasks: Default::default(),
            metrics,
            local_rx,
            topic_event_forwarders: Default::default(),
            address_lookup,
            send_queue_size,
            follow_ups: Default::default(),
        };

        (actor, rpc_tx, local_tx)
    }

    pub async fn run(mut self) {
        let mut addr_update_stream = self.setup();

        let mut i = 0;
        while self.event_loop(&mut addr_update_stream, i).await {
            i += 1;
        }
    }

    /// Handles a message from the [`Gossip`] handle, and returns `false` to stop the actor.
    fn handle_local_message(&mut self, msg: Option<LocalActorMessage>) -> bool {
        match msg {
            Some(LocalActorMessage::Shutdown { reply }) => {
                debug!("received shutdown message, quit all topics");
                self.quit_queue.extend(self.topics.keys().copied());
                self.process_quit_queue();
                debug!("all topics quit, stop gossip actor");
                reply.send(()).ok();
                false
            }
            Some(LocalActorMessage::HandleConnection(conn)) => {
                self.handle_connection(conn);
                true
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
        self.handle_addr_update(initial_addr);
        addr_update_stream
    }

    /// One event loop processing step.
    ///
    /// Returns `false` when the actor should stop.
    async fn event_loop(
        &mut self,
        addr_updates: &mut (impl Stream<Item = EndpointAddr> + Send + Unpin),
        i: usize,
    ) -> bool {
        self.metrics.actor_tick_main.inc();
        // A shutdown skips the other branches. It waits only for the local messages
        // queued before it. Each tick still runs the select below, so local messages
        // starve no other branch.
        if let Ok(msg) = self.local_rx.try_recv() {
            if !self.handle_local_message(Some(msg)) {
                return false;
            }
        }
        // Not biased: a publisher that always has a command ready would starve the
        // branches after it, such as received messages, timers and closed connections.
        tokio::select! {
            msg = self.local_rx.recv() => {
                if !self.handle_local_message(msg) {
                    return false;
                }
            }
            msg = self.rpc_rx.recv() => {
                trace!(?i, "tick: to_actor_rx");
                self.metrics.actor_tick_rx.inc();
                match msg {
                    Some(msg) => {
                        self.handle_rpc_msg(msg, Instant::now()).await;
                    }
                    None => {
                        debug!("all gossip handles dropped, stop gossip actor");
                        return false;
                    }
                }
            },
            Some((topic, command)) = self.command_rx.next(|topic| {
                all_eager_peers_busy(&self.state, &self.peers, topic)
            }), if !self.command_rx.is_empty() => {
                trace!(?i, "tick: command_rx");
                self.handle_command(topic, command);
            },
            _ = self.peer_cleanup.tick(), if !self.peers.is_empty() => {
                self.forget_peers(Instant::now());
            },
            Some(new_address) = addr_updates.next() => {
                trace!(?i, "tick: new_address");
                self.metrics.actor_tick_endpoint.inc();
                self.handle_addr_update(new_address);
            }
            (peer_id, res) = self.dialer.next_conn() => {
                trace!(?i, "tick: dialer");
                self.metrics.actor_tick_dialer.inc();
                match res {
                    Some(Ok(conn)) => {
                        debug!(peer = %peer_id.fmt_short(), "dial successful");
                        self.metrics.actor_tick_dialer_success.inc();
                        self.handle_connection(conn);
                    }
                    Some(Err(err)) => {
                        warn!(peer = %peer_id.fmt_short(), "dial failed: {err}");
                        self.metrics.actor_tick_dialer_failure.inc();
                        self.handle_dial_failed(peer_id);
                    }
                    None => {
                        warn!(peer = %peer_id.fmt_short(), "dial disconnected");
                        self.metrics.actor_tick_dialer_failure.inc();
                    }
                }
            }
            event = self.in_event_rx.recv() => {
                trace!(?i, "tick: in_event_rx");
                self.metrics.actor_tick_in_event_rx.inc();
                let event = event.expect("unreachable: in_event_tx is never dropped before receiver");
                self.handle_in_event(event, Instant::now());
            }
            _ = self.timers.wait_next() => {
                trace!(?i, "tick: timers");
                self.metrics.actor_tick_timers.inc();
                let now = Instant::now();
                while let Some((_instant, timer)) = self.timers.pop_before(now) {
                    self.handle_in_event(InEvent::TimerExpired(timer), now);
                }
            }
            Some(res) = self.connection_tasks.join_next(), if !self.connection_tasks.is_empty() => {
                trace!(?i, "tick: connection_tasks");
                let Some((peer_id, conn, result)) = task_output(res) else {
                    debug!("connection task cancelled, the runtime shuts down");
                    return false;
                };
                self.handle_connection_task_finished(peer_id, conn, result);
            }
            Some(res) = self.topic_event_forwarders.join_next(), if !self.topic_event_forwarders.is_empty() => {
                let Some(topic_id) = task_output(res) else {
                    debug!("topic task cancelled, the runtime shuts down");
                    return false;
                };
                if let Some(state) = self.topics.get_mut(&topic_id) {
                    if !state.still_needed() {
                        self.quit_queue.push_back(topic_id);
                        self.process_quit_queue();
                    }
                }
            }
        }

        true
    }

    fn handle_addr_update(&mut self, endpoint_addr: EndpointAddr) {
        // let peer_data = our_peer_data(&self.endpoint, current_addresses);
        let peer_data = encode_peer_data(&endpoint_addr.into());
        self.handle_in_event(InEvent::UpdatePeerData(peer_data), Instant::now());
    }

    fn handle_command(&mut self, topic: TopicId, command: Option<Command>) {
        debug!(?topic, ?command, "handle command");
        let Some(state) = self.topics.get_mut(&topic) else {
            // TODO: unreachable?
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
                state.command_receiver_count = state
                    .command_receiver_count
                    .checked_sub(1)
                    .expect("finished command receiver must have been registered");
                if !state.still_needed() {
                    self.quit_queue.push_back(topic);
                    self.process_quit_queue();
                }
            }
        }
    }

    fn handle_connection(&mut self, conn: Connection) {
        let peer_id = conn.remote_id();
        let side = conn.side();
        let size = self.send_queue_size;
        let peer = self
            .peers
            .entry(peer_id)
            .or_insert_with(|| PeerState::new(size));
        peer.set_wanted(self.state.uses_peer(&peer_id));
        let queue_lease = peer.handle_connection(side, conn.stable_id());

        let aborted = peer.aborted.clone();
        let wanted = peer.wanted.subscribe();
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
                    in_event_tx,
                    max_message_size,
                    wanted,
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

    fn handle_dial_failed(&mut self, peer_id: EndpointId) {
        // An open queue with a writer is already delivering. Anything else,
        // including a queue that is only draining, still reports the peer gone.
        if !self
            .peers
            .get(&peer_id)
            .is_some_and(PeerState::has_active_writer)
        {
            self.handle_in_event(InEvent::PeerDisconnected(peer_id), Instant::now());
        }
    }

    /// Removes the peers whose retention ended.
    fn forget_peers(&mut self, now: Instant) {
        // A pending dial may still need the capability learned on an earlier connection.
        self.peers
            .retain(|id, peer| self.dialer.is_pending(*id) || !peer.can_forget(now));
    }

    #[tracing::instrument(name = "conn", skip_all, fields(peer = %peer_id.fmt_short()))]
    fn handle_connection_task_finished(
        &mut self,
        peer_id: EndpointId,
        conn: Connection,
        task_result: Result<(), ConnectionLoopError>,
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
        let error = task_result.err();
        debug!(%reason, ?error, "connection closed");
        let now = Instant::now();
        let disconnected = self
            .peers
            .get_mut(&peer_id)
            .expect("unfinished connection must keep its peer state")
            .connection_finished(conn.stable_id(), now);
        if disconnected {
            debug!("active send connection closed, mark peer as disconnected");
            self.handle_in_event(InEvent::PeerDisconnected(peer_id), now);
        }
    }

    async fn handle_rpc_msg(&mut self, msg: RpcMessage, now: Instant) {
        trace!("handle to_actor  {msg:?}");
        match msg {
            RpcMessage::Join(msg) => {
                let WithChannels {
                    inner,
                    rx,
                    tx,
                    // TODO(frando): make use of span?
                    span: _,
                } = msg;
                let api::JoinRequest {
                    topic_id,
                    bootstrap,
                } = inner;
                let TopicState {
                    event_sender,
                    command_receiver_count,
                } = self.topics.entry(topic_id).or_default();
                let mut sender_dead = false;
                for neighbor in self.state.neighbors(&topic_id) {
                    if let Err(_err) = tx.try_send(Event::NeighborUp(*neighbor)).await {
                        sender_dead = true;
                        break;
                    }
                }

                if !sender_dead {
                    let subscriber_fut = topic_subscriber_loop(tx, event_sender.subscribe());
                    let fut = async move {
                        subscriber_fut.await;
                        topic_id
                    };
                    self.topic_event_forwarders
                        .spawn(fut.instrument(tracing::Span::current()));
                }
                self.command_rx.insert(topic_id, rx.into_stream());
                *command_receiver_count += 1;

                self.handle_in_event(
                    InEvent::Command(
                        topic_id,
                        ProtoCommand::Join(bootstrap.into_iter().collect()),
                    ),
                    now,
                );
            }
            RpcMessage::MaxContentSize(msg) => {
                let max_message_size = self.state.max_message_size();
                let max_content = proto::topic::max_content_size(max_message_size);
                msg.tx.send(max_content).await.ok();
            }
        }
    }

    fn handle_in_event(&mut self, event: InEvent, now: Instant) {
        self.handle_in_event_inner(event, now);
        self.process_quit_queue();
    }

    fn process_quit_queue(&mut self) {
        while let Some(topic_id) = self.quit_queue.pop_front() {
            self.handle_in_event_inner(
                InEvent::Command(topic_id, ProtoCommand::Quit),
                Instant::now(),
            );
            if self.topics.remove(&topic_id).is_some() {
                tracing::debug!(%topic_id, "publishers and subscribers gone; unsubscribing");
            }
        }
    }

    fn handle_in_event_inner(&mut self, event: InEvent, now: Instant) {
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
        // The peers whose use by the topics may change, so their flag in `wanted` too.
        let mut touched = Vec::new();
        // Peers that overflowed in this batch. The follow-up disconnect drops what
        // we would queue for them, so we neither reopen their queue nor dial them.
        let mut aborted = Vec::new();
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
                    if peer.needs_dial() && !self.dialer.is_pending(peer_id) {
                        debug!(peer = %peer_id.fmt_short(), "start to dial");
                        self.dialer.queue_dial(peer_id, self.alpn.clone());
                    }
                    // An open queue without a writer relies on the dial: it either
                    // supplies the writer, or its failure disconnects the peer.
                    debug_assert!(
                        peer.writer.is_some()
                            || peer.queue.is_closing()
                            || self.dialer.is_pending(peer_id),
                        "open queue without a writer or a dial"
                    );
                }
                OutEvent::EmitEvent(topic_id, event) => {
                    let Some(state) = self.topics.get_mut(&topic_id) else {
                        // TODO: unreachable?
                        warn!(?topic_id, "gossip state emitted event for unknown topic");
                        continue;
                    };
                    let TopicState { event_sender, .. } = state;
                    event_sender.send(event).ok();
                    if !state.still_needed() {
                        self.quit_queue.push_back(topic_id);
                    }
                }
                OutEvent::ScheduleTimer(delay, timer) => {
                    self.timers.insert(now + delay, timer);
                }
                OutEvent::DisconnectPeer(peer_id) => {
                    debug!(peer=%peer_id.fmt_short(), "gossip state indicates disconnect: drain queue");
                    touched.push(peer_id);
                    if let Some(peer) = self.peers.get_mut(&peer_id) {
                        peer.close_queue();
                    }
                }
                OutEvent::PeerData(endpoint_id, data) => match decode_peer_data(&data) {
                    Err(err) => warn!("Failed to decode {data:?} from {endpoint_id}: {err}"),
                    Ok(info) => {
                        debug!(peer = ?endpoint_id, "add known addrs: {info:?}");
                        let mut endpoint_addr = EndpointAddr::new(endpoint_id);
                        for addr in info.direct_addresses {
                            endpoint_addr = endpoint_addr.with_ip_addr(addr);
                        }
                        if let Some(relay_url) = info.relay_url {
                            endpoint_addr = endpoint_addr.with_relay_url(relay_url);
                        }

                        self.address_lookup.add(endpoint_addr);
                    }
                },
            }
        }
        for peer in touched {
            if let Some(state) = self.peers.get(&peer) {
                state.set_wanted(self.state.uses_peer(&peer));
            }
        }
    }

    #[cfg(test)]
    fn joined(&self, topic: &TopicId) -> bool {
        self.state.neighbors(topic).next().is_some()
    }
}

/// How long we keep the state of an idle peer.
///
/// A dial that completes late still finds the `handles_close` flag of the peer.
// TODO(frando): Remove the retention once the wire format no longer needs the flag.
const PEER_RETENTION: Duration = RETIRE_LIMIT;

/// The [`Connection::stable_id`] of a connection.
type ConnId = usize;

/// What we know of a peer: its queue, and its connections.
///
/// The queue always exists. An open queue waits for messages. A closed queue
/// goes out, and then its writer ends. A closed, empty queue is idle, and the
/// peer needs no writer and no dial.
///
/// The state outlives the last connection by [`PEER_RETENTION`], so that a
/// peer we want again keeps its flags.
#[derive(Debug)]
struct PeerState {
    queue: SendQueue,
    /// Closes every connection of the peer, also receive-only ones, and revokes every lease.
    ///
    /// An abort cancels it, and a fresh token takes its place for later connections.
    aborted: CancellationToken,
    /// The connection that writes the queue, or `None` until a dial takes it.
    ///
    /// A replaced writer finishes its current message first. The queue lease
    /// keeps the two in order. Reopening clears this slot, but an older lease
    /// may still write until the replacement dial takes over.
    writer: Option<ConnId>,
    /// Connection tasks that the actor has not yet seen finish.
    connections: usize,
    /// When [`Actor::forget_peers`] may remove the idle peer.
    forget_at: Option<Instant>,
    /// Whether a topic uses the peer, which keeps its connections open.
    wanted: watch::Sender<bool>,
    /// Whether the peer sent [`util::StreamHeader::handles_close`].
    ///
    /// It belongs to the peer, not to one connection. So a connection on which
    /// neither side sends closes too, as after two dials cross.
    handles_close: watch::Sender<bool>,
}

impl PeerState {
    /// Creates the state of a peer with a closed, empty queue.
    ///
    /// A message that we send, or an accepted connection, opens the queue. A
    /// dial that completes after we forgot the peer thus has nothing to write.
    fn new(queue_size: usize) -> Self {
        let aborted = CancellationToken::new();
        let queue = SendQueue::with_parent(queue_size, aborted.clone());
        queue.set_closing(true);
        Self {
            queue,
            aborted,
            writer: None,
            connections: 0,
            forget_at: None,
            wanted: watch::channel(false).0,
            handles_close: watch::channel(false).0,
        }
    }

    /// Returns whether this peer needs a new dial because it has an open queue but no writer.
    fn needs_dial(&self) -> bool {
        self.writer.is_none() && !self.queue.is_closing()
    }

    /// Whether a connection writes the open queue.
    fn has_active_writer(&self) -> bool {
        self.writer.is_some() && !self.queue.is_closing()
    }

    /// Sets the `wanted` flag, which wakes the connections only on a change.
    fn set_wanted(&self, wanted: bool) {
        self.wanted
            .send_if_modified(|current| std::mem::replace(current, wanted) != wanted);
    }

    /// Opens the queue, which keeps its messages and its byte limit.
    fn open_queue(&mut self) -> &SendQueue {
        self.forget_at = None;
        if self.queue.is_closing() {
            self.queue.set_closing(false);
            // The writer of the closed queue may have ended. A pending dial takes over.
            self.writer = None;
        }
        &self.queue
    }

    /// Queues a message without waiting, and opens the queue.
    ///
    /// Waiting for room would stop the actor, and with it every topic, for as
    /// long as this one peer does not read. On [`Pushed::Overflow`], the caller
    /// aborts the peer.
    fn send(&mut self, message: ProtoMessage) -> Pushed {
        self.open_queue().push(message)
    }

    /// Closes the queue: the writer sends what is left, and then ends.
    fn close_queue(&mut self) {
        self.queue.set_closing(true);
    }

    /// Puts a closed, empty queue in place of the queue.
    ///
    /// A writer of the old queue still sends what it holds, and then ends. The
    /// new queue keeps the abort token, so a later abort still closes that writer.
    fn discard_queue(&mut self, now: Instant) {
        self.queue = SendQueue::with_parent(self.queue.size(), self.aborted.clone());
        self.queue.set_closing(true);
        self.writer = None;
        self.retain_if_idle(now);
    }

    /// Discards the queue, and closes every connection of the peer.
    ///
    /// The topics cannot work with a peer that misses a control message. So no
    /// writer sends the queue with the gap, also not one that a handover replaced.
    /// Receive-only connections close too, so the remote writer learns of the abort.
    fn abort(&mut self, now: Instant) {
        self.aborted.cancel();
        self.aborted = CancellationToken::new();
        self.discard_queue(now);
    }

    /// Counts a new connection, and returns its lease if it writes the queue.
    fn handle_connection(&mut self, side: Side, id: ConnId) -> Option<QueueLease> {
        self.connections += 1;
        self.forget_at = None;
        // A dial writes only if no connection does. An accepted one always takes over.
        if side == Side::Client && self.writer.is_some() {
            None
        } else {
            self.writer = Some(id);
            // An accepted connection reopens a closed queue. A dial sends what is
            // left of it, and then ends.
            if side == Side::Server {
                self.queue.set_closing(false);
            }
            Some(self.queue.lease())
        }
    }

    /// Counts a finished connection, and returns whether to report a disconnect.
    ///
    /// It is when the writer of an open queue ended, even if other connections
    /// remain. The caller must immediately process `PeerDisconnected`, which
    /// discards the queue before invoking the protocol. A closed queue is
    /// discarded here instead, since its writer ended intentionally.
    fn connection_finished(&mut self, id: ConnId, now: Instant) -> bool {
        self.connections = self
            .connections
            .checked_sub(1)
            .expect("finished connection must have been registered");
        if self.writer != Some(id) {
            self.retain_if_idle(now);
            false
        } else if self.queue.is_closing() {
            self.discard_queue(now);
            false
        } else {
            true
        }
    }

    /// Starts retention once the queue is closed and empty and all connections
    /// finished. An open queue still needs a writer, even when empty.
    /// A later call does not extend retention.
    fn retain_if_idle(&mut self, now: Instant) {
        if self.queue.is_closing() && self.queue.is_empty() && self.connections == 0 {
            self.forget_at.get_or_insert(now + PEER_RETENTION);
        }
    }

    /// Whether the retention of the peer ended at `now`.
    fn can_forget(&self, now: Instant) -> bool {
        self.forget_at.is_some_and(|deadline| deadline <= now)
    }
}

#[derive(Debug)]
struct TopicState {
    event_sender: broadcast::Sender<ProtoEvent>,
    /// Counts subscription command receivers, including those waiting to publish.
    ///
    /// A receiver remains counted until the actor processes its end.
    command_receiver_count: usize,
}

impl Default for TopicState {
    fn default() -> Self {
        let (event_sender, _) = broadcast::channel(TOPIC_EVENT_CAP);
        Self {
            command_receiver_count: 0,
            event_sender,
        }
    }
}

/// How often a waiting broadcast checks the current queue pressure again.
const BROADCAST_RECHECK_INTERVAL: Duration = Duration::from_millis(10);

/// How long one broadcast may wait for queue pressure to ease.
///
/// This bounds admission waiting, not API channel backlog or delivery time.
const BROADCAST_WAIT_LIMIT: Duration = Duration::from_millis(100);

/// Returns whether all eager targets have busy data queues.
///
/// No targets, or one target with room, lets a local broadcast proceed. The
/// receiver calls this before admitting a broadcast and when retrying one.
/// Pressure can come from local publications, relayed traffic, or slow receivers.
fn all_eager_peers_busy(
    state: &proto::State<PublicKey, StdRng>,
    peers: &HashMap<EndpointId, PeerState>,
    topic: TopicId,
) -> bool {
    let mut eager_peers = state.eager_peers(&topic).peekable();
    eager_peers.peek().is_some()
        && eager_peers.all(|peer| peers.get(peer).is_some_and(|peer| peer.queue.is_busy()))
}

impl TopicState {
    /// Check if the topic still has any publisher or subscriber.
    fn still_needed(&self) -> bool {
        // Keep topic alive if either senders or receivers exist.
        // Using || prevents topic closure when senders are dropped while receivers listen.
        self.command_receiver_count != 0 || self.event_sender.receiver_count() > 0
    }
}

#[allow(missing_docs)]
#[stack_error(derive, add_meta, from_sources, std_sources)]
#[non_exhaustive]
enum ConnectionLoopError {
    #[error(transparent)]
    Write {
        source: self::util::WriteError,
    },
    #[error(transparent)]
    Connection {
        #[error(std_err)]
        source: iroh::endpoint::ConnectionError,
    },
    ActorDropped {},
}

impl<T> From<mpsc::error::SendError<T>> for ConnectionLoopError {
    fn from(_value: mpsc::error::SendError<T>) -> Self {
        e!(ConnectionLoopError::ActorDropped)
    }
}

/// Returns a finished task's output, or `None` if the runtime cancelled the task.
///
/// A task cancelled at runtime shutdown is no bug, so it must not panic the actor (#140).
/// A panic in the task panics the caller with the same payload.
fn task_output<T>(res: Result<T, task::JoinError>) -> Option<T> {
    match res {
        Ok(output) => Some(output),
        Err(err) if err.is_cancelled() => None,
        #[cfg(not(target_family = "wasm"))]
        Err(err) => std::panic::resume_unwind(err.into_panic()),
        #[cfg(target_family = "wasm")]
        Err(err) => panic!("task failed: {err}"),
    }
}

/// How long a connection stays once no topic uses its peer and our sends ended.
///
/// The peer reads what we sent last, its messages in flight still arrive, and a
/// peer we want again keeps the connection.
const IDLE_GRACE: Duration = Duration::from_secs(5);

/// How long a connection stays at most once no topic uses its peer.
///
/// It bounds the wait for sends that do not end, as the peer does not read.
const RETIRE_LIMIT: Duration = Duration::from_secs(20);

/// Returns once no topic used the peer for [`IDLE_GRACE`] after our sends ended.
///
/// Sends that do not end delay it up to [`RETIRE_LIMIT`] after the peer became unused.
async fn retire(mut wanted: watch::Receiver<bool>, mut sending: watch::Receiver<bool>) {
    let (mut wanted_open, mut sending_open) = (true, true);
    let mut limit = None;
    loop {
        let unwanted = !*wanted.borrow_and_update();
        let sending_now = *sending.borrow_and_update();
        let deadline = if unwanted {
            let limit = *limit.get_or_insert_with(|| Instant::now() + RETIRE_LIMIT);
            match sending_now {
                true => Some(limit),
                false => Some(limit.min(Instant::now() + IDLE_GRACE)),
            }
        } else {
            limit = None;
            None
        };
        tokio::select! {
            // A change that is ready at the deadline wins, and the grace starts over.
            biased;
            // Any change starts a new unused period, also one that read false again,
            // as the peer was wanted in between.
            res = wanted.changed(), if wanted_open => {
                wanted_open = res.is_ok();
                limit = None;
            }
            res = sending.changed(), if sending_open => sending_open = res.is_ok(),
            _ = n0_future::time::sleep_until(deadline.unwrap_or_else(Instant::now)), if deadline.is_some() => return,
            // Neither can change, and we want the peer.
            else => std::future::pending().await,
        }
    }
}

/// Runs the I/O of a connection, and closes it on an abort or when no topic uses the peer.
///
/// Without a lease, the connection only receives. Every connection observes
/// the peer abort signal, independently of whether it writes.
// Each argument is state the loops share. A struct would only wrap them.
#[allow(clippy::too_many_arguments)]
async fn connection_loop(
    from: PublicKey,
    conn: Connection,
    aborted: CancellationToken,
    queue_lease: Option<QueueLease>,
    in_event_tx: mpsc::Sender<InEvent>,
    max_message_size: usize,
    wanted: watch::Receiver<bool>,
    handles_close: watch::Sender<bool>,
) -> Result<(), ConnectionLoopError> {
    debug!(side = ?conn.side(), "connection established");
    let mut handles_close_rx = handles_close.subscribe();

    let send_loop = queue_lease.map(|lease| SendLoop::new(conn.clone(), lease, max_message_size));
    // Receive-only connections have no send work delaying their idle grace.
    let sending = send_loop
        .as_ref()
        .map(SendLoop::sending)
        .unwrap_or_else(|| watch::channel(false).1);
    let mut recv_loop = RecvLoop::new(
        from,
        conn.clone(),
        in_event_tx,
        max_message_size,
        handles_close,
    );

    let io = async {
        let recv_fut = recv_loop.run().instrument(error_span!("recv"));
        tokio::pin!(recv_fut);
        if let Some(send_loop) = send_loop {
            tokio::select! {
                res = send_loop.run().instrument(error_span!("send")) => res?,
                _ = &mut recv_fut => return Ok(()),
            }
        }
        // We are done sending. A peer we want may still send on this connection,
        // as two peers that dialed each other can each use a different one.
        recv_fut.await;
        Ok(())
    };
    let retirement = async {
        // An old node would lose its later messages to us after a close, see
        // `StreamHeader::handles_close`.
        if handles_close_rx.wait_for(|known| *known).await.is_err() {
            std::future::pending::<()>().await;
        }
        retire(wanted, sending).await;
    };
    tokio::select! {
        biased;
        // All connections close, including receive-only ones, so the peer
        // learns it on the connection that it uses. A
        // node up to 0.101 may then lose its later messages to us (#155). It
        // missed a control message, so its views differ from ours anyway.
        _ = aborted.cancelled() => {
            warn!("the peer is aborted, close the connection");
            conn.close(util::CLOSE_QUEUE_FULL.into(), b"send queue full");
            Ok(())
        }
        res = io => res,
        _ = retirement => {
            debug!("no topic uses the peer, close");
            Ok(())
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
struct AddrInfo {
    relay_url: Option<RelayUrl>,
    direct_addresses: BTreeSet<SocketAddr>,
}

impl From<EndpointAddr> for AddrInfo {
    fn from(endpoint_addr: EndpointAddr) -> Self {
        Self {
            relay_url: endpoint_addr.relay_urls().next().cloned(),
            direct_addresses: endpoint_addr.ip_addrs().cloned().collect(),
        }
    }
}

fn encode_peer_data(info: &AddrInfo) -> PeerData {
    let bytes = postcard::to_stdvec(info).expect("serializing AddrInfo may not fail");
    PeerData::new(bytes)
}

fn decode_peer_data(peer_data: &PeerData) -> Result<AddrInfo, postcard::Error> {
    let bytes = peer_data.as_bytes();
    if bytes.is_empty() {
        return Ok(AddrInfo::default());
    }
    let info = postcard::from_bytes(bytes)?;
    Ok(info)
}

async fn topic_subscriber_loop(
    sender: irpc::channel::mpsc::Sender<Event>,
    mut topic_events: broadcast::Receiver<ProtoEvent>,
) {
    loop {
        tokio::select! {
           biased;
           msg = topic_events.recv() => {
               let event = match msg {
                   Err(broadcast::error::RecvError::Closed) => break,
                   Err(broadcast::error::RecvError::Lagged(_)) => Event::Lagged,
                   Ok(event) => event.into(),
               };
               if sender.send(event).await.is_err() {
                   break;
               }
           }
           _ = sender.closed() => break,
        }
    }
}

/// Receives subscription commands, delaying broadcasts while their targets are busy.
///
/// A held broadcast keeps ownership of its receiver, so later commands on that
/// subscription cannot pass it. Other subscriptions continue independently.
/// Operations use pooled storage; only a held broadcast allocates a retry timer.
#[derive(Debug, Default)]
struct SubscriptionCommands {
    pending: FuturesUnordered<RecvFut>,
}

impl SubscriptionCommands {
    /// Registers a subscription's command receiver.
    fn insert(
        &mut self,
        topic: TopicId,
        stream: impl Stream<Item = Result<Command, irpc::channel::mpsc::RecvError>> + Send + 'static,
    ) {
        let receiver = CommandReceiver {
            topic,
            stream: Box::pin(stream),
            wait: None,
        };
        self.pending.push(receiver.recv_next());
    }

    /// Returns whether all subscription receivers have ended.
    fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Returns the next command or the end of one subscription receiver.
    ///
    /// `topic_is_blocked(topic)` checks the actor's current queues before
    /// broadcast admission. It is never called for other commands or after a
    /// held broadcast's deadline. Each admitted command returns to the actor
    /// before another is checked, so its queue changes affect the next check.
    ///
    /// `Some((topic, None))` reports one receiver ending; `None` means no
    /// receivers remain. Cancelling this future preserves receivers, commands,
    /// and deadlines: every unfinished receiver is put back before another await.
    async fn next(
        &mut self,
        topic_is_blocked: impl Fn(TopicId) -> bool,
    ) -> Option<(TopicId, Option<Command>)> {
        while let Some((receiver, item)) = self.pending.next().await {
            let topic = receiver.topic;
            let Some(item) = item else {
                return Some((topic, None));
            };
            let now = Instant::now();
            if item.is_broadcast() && !receiver.wait_expired(now) && topic_is_blocked(topic) {
                self.pending.push(receiver.retry(item, now));
                continue;
            } else {
                self.pending.push(receiver.recv_next());
                return Some((topic, Some(item)));
            }
        }
        None
    }
}

/// Keeps a subscription's stream, current command, and admission timing together.
#[derive(derive_more::Debug)]
struct CommandReceiver {
    topic: TopicId,
    #[debug("CommandStream")]
    stream: n0_future::stream::Boxed<Result<Command, irpc::channel::mpsc::RecvError>>,
    wait: Option<(Instant, Pin<Box<Sleep>>, Option<Command>)>,
}

impl CommandReceiver {
    fn wait_expired(&self, now: Instant) -> bool {
        match self.wait.as_ref() {
            None => false,
            Some((deadline, _timer, _)) => now >= *deadline,
        }
    }

    fn recv_next(mut self) -> RecvFut {
        self.wait = None;
        RecvFut(Some(self))
    }

    fn retry(mut self, item: Command, now: Instant) -> RecvFut {
        let (deadline, timer) = if let Some((deadline, mut timer, _)) = self.wait.take() {
            let next_retry = (now + BROADCAST_RECHECK_INTERVAL).min(deadline);
            timer.as_mut().reset(next_retry);
            (deadline, timer)
        } else {
            let deadline = now + BROADCAST_WAIT_LIMIT;
            let next_retry = (now + BROADCAST_RECHECK_INTERVAL).min(deadline);
            let timer = Box::pin(time::sleep_until(next_retry));
            (deadline, timer)
        };
        self.wait = Some((deadline, timer, Some(item)));
        RecvFut(Some(self))
    }
}

#[derive(Debug, Default)]
struct RecvFut(Option<CommandReceiver>);

impl Future for RecvFut {
    type Output = (CommandReceiver, Option<Command>);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let receiver = this.0.as_mut().expect("RecvFut polled after completion");
        let item = if let Some((_deadline, timer, item)) = &mut receiver.wait {
            ready!(timer.as_mut().poll(cx));
            item.take()
        } else {
            ready!(receiver.stream.as_mut().poll_next(cx)).and_then(Result::ok)
        };
        let receiver = this.0.take().expect("checked above");
        Poll::Ready((receiver, item))
    }
}

#[derive(Debug)]
struct Dialer {
    endpoint: Endpoint,
    pending: JoinSet<(
        EndpointId,
        Option<Result<Connection, iroh::endpoint::ConnectError>>,
    )>,
    pending_dials: HashMap<EndpointId, CancellationToken>,
}

impl Dialer {
    /// Create a new dialer for a [`Endpoint`]
    fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            pending: Default::default(),
            pending_dials: Default::default(),
        }
    }

    /// Starts to dial a endpoint by [`EndpointId`].
    fn queue_dial(&mut self, endpoint_id: EndpointId, alpn: Bytes) {
        if self.is_pending(endpoint_id) {
            return;
        }
        let cancel = CancellationToken::new();
        self.pending_dials.insert(endpoint_id, cancel.clone());
        let endpoint = self.endpoint.clone();
        self.pending.spawn(
            async move {
                let res = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => None,
                    res = endpoint.connect(endpoint_id, &alpn) => Some(res),
                };
                (endpoint_id, res)
            }
            .instrument(tracing::Span::current()),
        );
    }

    /// Checks if a endpoint is currently being dialed.
    fn is_pending(&self, endpoint: EndpointId) -> bool {
        self.pending_dials.contains_key(&endpoint)
    }

    /// Waits for the next dial operation to complete.
    /// `None` means disconnected
    async fn next_conn(
        &mut self,
    ) -> (
        EndpointId,
        Option<Result<Connection, iroh::endpoint::ConnectError>>,
    ) {
        match self.pending_dials.is_empty() {
            false => {
                let (endpoint_id, res) = loop {
                    match self.pending.join_next().await {
                        Some(Ok((endpoint_id, res))) => {
                            self.pending_dials.remove(&endpoint_id);
                            break (endpoint_id, res);
                        }
                        Some(Err(e)) => {
                            error!("next conn error: {:?}", e);
                        }
                        None => {
                            error!("no more pending conns available");
                            std::future::pending().await
                        }
                    }
                };

                (endpoint_id, res)
            }
            true => std::future::pending().await,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{any::Any, collections::HashSet, time::Duration};

    use bytes::{Bytes, BytesMut};
    use futures_concurrency::future::TryJoin;
    use iroh::{
        address_lookup::memory::MemoryLookup,
        endpoint::{presets, BindError, QuicTransportConfig, VarInt},
        protocol::Router,
        tls::CaTlsConfig,
        RelayMap, RelayMode, SecretKey,
    };
    use n0_error::{AnyError, Result, StdResultExt};
    use n0_tracing_test::traced_test;
    use rand::{CryptoRng, RngExt};
    use tokio::{spawn, time::timeout};
    use tokio_util::sync::CancellationToken;
    use tracing::{info, instrument};

    use super::*;
    use crate::api::{ApiError, GossipReceiver, GossipSender};

    struct ManualActorLoop {
        actor: Actor,
        step: usize,
    }

    impl std::ops::Deref for ManualActorLoop {
        type Target = Actor;

        fn deref(&self) -> &Self::Target {
            &self.actor
        }
    }

    impl std::ops::DerefMut for ManualActorLoop {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.actor
        }
    }

    type EndpointHandle = tokio::task::JoinHandle<Result<()>>;

    impl ManualActorLoop {
        #[instrument(skip_all, fields(me = %actor.endpoint.id().fmt_short()))]
        async fn new(mut actor: Actor) -> Self {
            let _ = actor.setup();
            Self { actor, step: 0 }
        }

        #[instrument(skip_all, fields(me = %self.endpoint.id().fmt_short()))]
        async fn step(&mut self) -> bool {
            let ManualActorLoop { actor, step } = self;
            *step += 1;
            // ignore updates that change our published address. This gives us better control over
            // events since the endpoint it no longer emitting changes
            let addr_update_stream = &mut n0_future::stream::pending();
            actor.event_loop(addr_update_stream, *step).await
        }

        /// Steps until `done` holds, at most `max` times, and returns whether it holds.
        ///
        /// The actor picks its branches in no fixed order, so a count of steps can
        /// miss an event.
        async fn steps_until(&mut self, max: usize, done: impl Fn(&Actor) -> bool) -> bool {
            for _ in 0..max {
                if done(&self.actor) {
                    return true;
                }
                self.step().await;
            }
            done(&self.actor)
        }

        async fn finish(mut self) {
            while self.step().await {}
        }
    }

    impl Gossip {
        /// Creates a testing gossip instance and its actor without spawning it.
        ///
        /// This creates the endpoint and spawns the endpoint loop as well. The handle for the
        /// endpoing task is returned along the gossip instance and actor. Since the actor is not
        /// actually spawned as [`Builder::spawn`] would, the gossip instance will have a
        /// handle to a dummy task instead.
        async fn t_new_with_actor(
            rng: &mut rand::rngs::ChaCha12Rng,
            config: proto::Config,
            relay_map: RelayMap,
            cancel: &CancellationToken,
        ) -> Result<(Self, Actor, EndpointHandle), BindError> {
            let endpoint = create_endpoint(rng, relay_map, None).await?;
            let metrics = Arc::new(Metrics::default());
            let address_lookup = GossipAddressLookup::default();
            endpoint
                .address_lookup()
                .expect("endpoint is not closed")
                .add(address_lookup.clone());

            let (actor, to_actor_tx, conn_tx) =
                Actor::new(endpoint, config, metrics.clone(), None, address_lookup);
            let max_message_size = actor.state.max_message_size();

            let _actor_handle = AbortOnDropHandle::new(task::spawn(n0_future::future::pending()));
            let gossip = Self {
                inner: Inner {
                    api: GossipApi::local(to_actor_tx, max_message_size),
                    local_tx: conn_tx,
                    _actor_handle,
                    max_message_size,
                    metrics,
                }
                .into(),
            };

            let endpoint_task = task::spawn(endpoint_loop(
                actor.endpoint.clone(),
                gossip.clone(),
                cancel.child_token(),
            ));

            Ok((gossip, actor, endpoint_task))
        }

        /// Crates a new testing gossip instance with the normal actor loop.
        async fn t_new(
            rng: &mut rand::rngs::ChaCha12Rng,
            config: proto::Config,
            relay_map: RelayMap,
            cancel: &CancellationToken,
        ) -> Result<(Self, Endpoint, EndpointHandle, impl Drop + use<>), BindError> {
            let (g, actor, ep_handle) =
                Gossip::t_new_with_actor(rng, config, relay_map, cancel).await?;
            let ep = actor.endpoint.clone();
            let me = ep.id().fmt_short();
            let actor_handle =
                task::spawn(actor.run().instrument(tracing::error_span!("gossip", %me)));
            Ok((g, ep, ep_handle, AbortOnDropHandle::new(actor_handle)))
        }
    }

    pub(crate) async fn create_endpoint(
        rng: &mut rand::rngs::ChaCha12Rng,
        relay_map: RelayMap,
        memory_lookup: Option<MemoryLookup>,
    ) -> Result<Endpoint, BindError> {
        let ep = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Custom(relay_map))
            .secret_key(SecretKey::from_bytes(&rng.random()))
            .alpns(vec![GOSSIP_ALPN.to_vec()])
            .ca_tls_config(CaTlsConfig::insecure_skip_verify())
            .bind()
            .await?;

        if let Some(memory_lookup) = memory_lookup {
            ep.address_lookup()
                .expect("endpoint is not closed")
                .add(memory_lookup);
        }
        ep.online().await;
        Ok(ep)
    }

    async fn endpoint_loop(
        endpoint: Endpoint,
        gossip: Gossip,
        cancel: CancellationToken,
    ) -> Result<()> {
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => break,
                incoming = endpoint.accept() => match incoming {
                    None => break,
                    Some(incoming) => {
                        let connecting = match incoming.accept() {
                            Ok(connecting) => connecting,
                            Err(err) => {
                                warn!("incoming connection failed: {err:#}");
                                // we can carry on in these cases:
                                // this can be caused by retransmitted datagrams
                                continue;
                            }
                        };
                        let connection = connecting
                            .await
                            .std_context("await incoming connection")?;
                        gossip.handle_connection(connection).await?
                    }
                }
            }
        }
        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn gossip_net_smoke() {
        let mut rng = rand::rngs::ChaCha12Rng::seed_from_u64(1);
        let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();

        let memory_lookup = MemoryLookup::new();

        let ep1 = create_endpoint(&mut rng, relay_map.clone(), Some(memory_lookup.clone()))
            .await
            .unwrap();
        let ep2 = create_endpoint(&mut rng, relay_map.clone(), Some(memory_lookup.clone()))
            .await
            .unwrap();
        let ep3 = create_endpoint(&mut rng, relay_map.clone(), Some(memory_lookup.clone()))
            .await
            .unwrap();

        let go1 = Gossip::builder().spawn(ep1.clone());
        let go2 = Gossip::builder().spawn(ep2.clone());
        let go3 = Gossip::builder().spawn(ep3.clone());
        debug!("peer1 {:?}", ep1.id());
        debug!("peer2 {:?}", ep2.id());
        debug!("peer3 {:?}", ep3.id());
        let pi1 = ep1.id();
        let pi2 = ep2.id();

        let cancel = CancellationToken::new();
        let tasks = [
            spawn(endpoint_loop(ep1.clone(), go1.clone(), cancel.clone())),
            spawn(endpoint_loop(ep2.clone(), go2.clone(), cancel.clone())),
            spawn(endpoint_loop(ep3.clone(), go3.clone(), cancel.clone())),
        ];

        debug!("----- adding peers  ----- ");
        let topic: TopicId = blake3::hash(b"foobar").into();

        let addr1 = EndpointAddr::new(pi1).with_relay_url(relay_url.clone());
        let addr2 = EndpointAddr::new(pi2).with_relay_url(relay_url);
        memory_lookup.add_endpoint_info(addr1.clone());
        memory_lookup.add_endpoint_info(addr2.clone());

        debug!("----- joining  ----- ");
        // join the topics and wait for the connection to succeed
        let [sub1, mut sub2, mut sub3] = [
            go1.subscribe_and_join(topic, vec![]),
            go2.subscribe_and_join(topic, vec![pi1]),
            go3.subscribe_and_join(topic, vec![pi2]),
        ]
        .try_join()
        .await
        .unwrap();

        let (sink1, _stream1) = sub1.split();

        let len = 2;

        // publish messages on endpoint1
        let pub1 = spawn(async move {
            for i in 0..len {
                let message = format!("hi{i}");
                info!("go1 broadcast: {message:?}");
                sink1.broadcast(message.into_bytes().into()).await.unwrap();
                tokio::time::sleep(Duration::from_micros(1)).await;
            }
        });

        // wait for messages on endpoint2
        let sub2 = spawn(async move {
            let mut recv = vec![];
            loop {
                let ev = sub2.next().await.unwrap().unwrap();
                info!("go2 event: {ev:?}");
                if let Event::Received(msg) = ev {
                    recv.push(msg.content);
                }
                if recv.len() == len {
                    return recv;
                }
            }
        });

        // wait for messages on endpoint3
        let sub3 = spawn(async move {
            let mut recv = vec![];
            loop {
                let ev = sub3.next().await.unwrap().unwrap();
                info!("go3 event: {ev:?}");
                if let Event::Received(msg) = ev {
                    recv.push(msg.content);
                }
                if recv.len() == len {
                    return recv;
                }
            }
        });

        timeout(Duration::from_secs(10), pub1)
            .await
            .unwrap()
            .unwrap();
        let recv2 = timeout(Duration::from_secs(10), sub2)
            .await
            .unwrap()
            .unwrap();
        let recv3 = timeout(Duration::from_secs(10), sub3)
            .await
            .unwrap()
            .unwrap();

        // We assert the received messages, but not their order.
        // While commonly they will be received in-order, for go3 it may happen
        // that the second message arrives before the first one, because it managed to
        // forward-join go1 before the second message is published.
        let expected: HashSet<Bytes> = (0..len)
            .map(|i| Bytes::from(format!("hi{i}").into_bytes()))
            .collect();
        assert_eq!(HashSet::from_iter(recv2), expected);
        assert_eq!(HashSet::from_iter(recv3), expected);

        cancel.cancel();
        for t in tasks {
            timeout(Duration::from_secs(10), t)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        }
    }

    /// Test that when a gossip topic is no longer needed it's actually unsubscribed.
    ///
    /// This test will:
    /// - Create two endpoints, the first using manual event loop.
    /// - Subscribe both endpoints to the same topic. The first endpoint will subscribe twice and connect
    ///   to the second endpoint. The second endpoint will subscribe without bootstrap.
    /// - Ensure that the first endpoint removes the subscription iff all topic handles have been
    ///   dropped
    // NOTE: this is a regression test.
    #[tokio::test]
    #[traced_test]
    async fn subscription_cleanup() -> Result {
        let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
        let ct = CancellationToken::new();
        let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();

        // create the first endpoint with a manual actor loop
        let (go1, actor, ep1_handle) =
            Gossip::t_new_with_actor(rng, Default::default(), relay_map.clone(), &ct).await?;
        let mut actor = ManualActorLoop::new(actor).await;

        // create the second endpoint with the usual actor loop
        let (go2, ep2, ep2_handle, _test_actor_handle) =
            Gossip::t_new(rng, Default::default(), relay_map, &ct).await?;

        let endpoint_id1 = actor.endpoint.id();
        let endpoint_id2 = ep2.id();
        tracing::info!(
            endpoint_1 = %endpoint_id1.fmt_short(),
            endpoint_2 = %endpoint_id2.fmt_short(),
            "endpoints ready"
        );

        let topic: TopicId = blake3::hash(b"subscription_cleanup").into();
        tracing::info!(%topic, "joining");

        // create the tasks for each gossip instance:
        // - second endpoint subscribes once without bootstrap and listens to events
        // - first endpoint subscribes twice with the second endpoint as bootstrap. This is done on command
        //   from the main task (this)

        // second endpoint
        let ct2 = ct.clone();
        let go2_task = async move {
            let (_pub_tx, mut sub_rx) = go2.subscribe_and_join(topic, vec![]).await?.split();

            let subscribe_fut = async {
                while let Some(ev) = sub_rx.try_next().await? {
                    match ev {
                        Event::Lagged => tracing::debug!("missed some messages :("),
                        Event::Received(_) => unreachable!("test does not send messages"),
                        other => tracing::debug!(?other, "gs event"),
                    }
                }

                tracing::debug!("subscribe stream ended");
                Ok::<_, AnyError>(())
            };

            tokio::select! {
                _ = ct2.cancelled() => Ok(()),
                res = subscribe_fut => res,
            }
        }
        .instrument(tracing::debug_span!("endpoint_2", %endpoint_id2));
        let go2_handle = task::spawn(go2_task);

        // first endpoint
        let addr2 = EndpointAddr::new(endpoint_id2).with_relay_url(relay_url);
        let memory_lookup = MemoryLookup::new();
        memory_lookup.add_endpoint_info(addr2);
        actor.endpoint.address_lookup()?.add(memory_lookup);
        // we use a channel to signal advancing steps to the task
        let (tx, mut rx) = mpsc::channel::<()>(1);
        let ct1 = ct.clone();
        let go1_task = async move {
            // first subscribe is done immediately
            tracing::info!("subscribing the first time");
            let sub_1a = go1.subscribe_and_join(topic, vec![endpoint_id2]).await?;

            // wait for signal to subscribe a second time
            rx.recv().await.expect("signal for second subscribe");
            tracing::info!("subscribing a second time");
            let sub_1b = go1.subscribe_and_join(topic, vec![endpoint_id2]).await?;
            drop(sub_1a);

            // wait for signal to drop the second handle as well
            rx.recv().await.expect("signal for second subscribe");
            tracing::info!("dropping all handles");
            drop(sub_1b);

            // wait for cancellation
            ct1.cancelled().await;
            drop(go1);

            Ok::<_, AnyError>(())
        }
        .instrument(tracing::debug_span!("endpoint_1", %endpoint_id1));
        let go1_handle = task::spawn(go1_task);

        // advance and check that the topic is now subscribed
        let commands = |actor: &Actor| {
            actor
                .topics
                .get(&topic)
                .map_or(0, |state| state.command_receiver_count)
        };
        // Our subscribe, the connection, and the NeighborUp from the peer's information.
        let joined = |actor: &Actor| actor.joined(&topic);
        assert!(actor.steps_until(20, joined).await, "not joined");

        // signal the second subscribe, we should remain subscribed
        tx.send(())
            .await
            .std_context("signal additional subscribe")?;
        assert!(actor.steps_until(20, |a| commands(a) == 2).await);
        // The first handle goes.
        assert!(actor.steps_until(20, |a| commands(a) == 1).await);
        assert!(actor.joined(&topic));

        // signal to drop the second handle, the topic should no longer be subscribed
        tx.send(()).await.std_context("signal drop handles")?;
        assert!(
            actor
                .steps_until(20, |a| !a.topics.contains_key(&topic))
                .await,
            "the topic is still subscribed"
        );

        // cleanup and ensure everything went as expected
        ct.cancel();
        let wait = Duration::from_secs(2);
        timeout(wait, ep1_handle)
            .await
            .std_context("wait endpoint1 task")?
            .std_context("join endpoint1 task")??;
        timeout(wait, ep2_handle)
            .await
            .std_context("wait endpoint2 task")?
            .std_context("join endpoint2 task")??;
        timeout(wait, go1_handle)
            .await
            .std_context("wait gossip1 task")?
            .std_context("join gossip1 task")??;
        timeout(wait, go2_handle)
            .await
            .std_context("wait gossip2 task")?
            .std_context("join gossip2 task")??;
        timeout(wait, actor.finish())
            .await
            .std_context("wait actor finish")?;

        Ok(())
    }

    /// A cancelled task gives `None`, and a finished one its output.
    #[tokio::test]
    async fn task_output_of_cancelled_and_finished_tasks() {
        let task = task::spawn(std::future::pending::<u32>());
        task.abort();
        assert!(task_output(task.await).is_none());
        assert_eq!(task_output(task::spawn(async { 42u32 }).await), Some(42));
    }

    /// A panic in a task panics the caller with the same payload.
    #[cfg(not(target_family = "wasm"))]
    #[tokio::test]
    #[should_panic(expected = "original child panic")]
    async fn task_output_passes_a_panic_on() {
        task_output(task::spawn(async { panic!("original child panic") }).await);
    }

    /// A message of the topic layer, as it goes over the wire.
    type TopicMessage = proto::topic::Message<PublicKey>;

    /// The frame size limit when tests read or write raw streams.
    const TEST_FRAME_LIMIT: usize = 1 << 16;

    /// How long a connection that we stopped sending on waits before it closes.
    const IDLE_GRACE: Duration = Duration::from_secs(5);

    /// Two endpoints on one test relay, the second reachable by its address.
    struct TestPair {
        ep1: Endpoint,
        ep2: Endpoint,
        ep1_addr: EndpointAddr,
        ep2_addr: EndpointAddr,
        _relay: Box<dyn Any + Send>,
    }

    impl TestPair {
        async fn new(seed: u64) -> Result<Self> {
            let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(seed);
            let (relay_map, relay_url, relay) = iroh::test_utils::run_relay_server()
                .await
                .std_context("relay")?;
            let ep1 = create_endpoint(rng, relay_map.clone(), None).await?;
            let ep2 = create_endpoint(rng, relay_map, None).await?;
            let ep1_addr = EndpointAddr::new(ep1.id()).with_relay_url(relay_url.clone());
            let ep2_addr = EndpointAddr::new(ep2.id()).with_relay_url(relay_url);
            Ok(Self {
                ep1,
                ep2,
                ep1_addr,
                ep2_addr,
                _relay: Box::new(relay),
            })
        }

        /// Connects the first endpoint to the second, returning both sides.
        async fn connect(&self) -> Result<(Connection, Connection)> {
            let ours = async {
                self.ep1
                    .connect(self.ep2_addr.clone(), GOSSIP_ALPN)
                    .await
                    .std_context("connect")
            };
            let theirs = async {
                let incoming = self.ep2.accept().await.std_context("endpoint closed")?;
                incoming.await.std_context("accept")
            };
            (ours, theirs).try_join().await
        }

        /// Connects the second endpoint to the first, returning the first's side first.
        async fn connect_back(&self) -> Result<(Connection, Connection)> {
            let ours = async {
                let incoming = self.ep1.accept().await.std_context("endpoint closed")?;
                incoming.await.std_context("accept")
            };
            let theirs = async {
                self.ep2
                    .connect(self.ep1_addr.clone(), GOSSIP_ALPN)
                    .await
                    .std_context("connect")
            };
            (ours, theirs).try_join().await
        }
    }

    /// Opens a stream for `topic` and writes `messages` to it, leaving it open.
    async fn open_stream(
        conn: &Connection,
        topic: TopicId,
        messages: &[proto::topic::Message<PublicKey>],
    ) -> Result<iroh::endpoint::SendStream> {
        let mut stream = conn.open_uni().await.std_context("open stream")?;
        let mut buffer = Vec::new();
        util::StreamHeader::new(topic)
            .write(&mut stream, &mut buffer, TEST_FRAME_LIMIT)
            .await
            .std_context("write header")?;
        for message in messages {
            util::write_frame(&mut stream, message, &mut buffer, TEST_FRAME_LIMIT)
                .await
                .std_context("write frame")?;
        }
        Ok(stream)
    }

    /// Accepts the peer's next stream and reads it to its end.
    async fn read_next_stream(conn: &Connection) -> Result<(TopicId, Vec<TopicMessage>)> {
        let mut stream = conn.accept_uni().await.std_context("accept stream")?;
        let mut buffer = BytesMut::new();
        let header = util::StreamHeader::read(&mut stream, &mut buffer, TEST_FRAME_LIMIT)
            .await
            .std_context("read header")?;
        let mut messages = Vec::new();
        while let Some(message) = util::read_frame(&mut stream, &mut buffer, TEST_FRAME_LIMIT)
            .await
            .std_context("read frame")?
        {
            messages.push(message);
        }
        Ok((header.topic_id, messages))
    }

    /// Returns a `Disconnect` for `topic`.
    fn disconnect_message(topic: TopicId) -> ProtoMessage {
        ProtoMessage {
            topic,
            message: TopicMessage::test_disconnect(),
        }
    }

    /// Accepts the peer's next stream and reads its first message.
    async fn read_next_message(conn: &Connection) -> Result<(TopicId, TopicMessage)> {
        let mut stream = conn.accept_uni().await.std_context("accept stream")?;
        let mut buffer = BytesMut::new();
        let header = util::StreamHeader::read(&mut stream, &mut buffer, TEST_FRAME_LIMIT)
            .await
            .std_context("read header")?;
        let message = util::read_frame(&mut stream, &mut buffer, TEST_FRAME_LIMIT)
            .await
            .std_context("read frame")?
            .std_context("the stream ended without a message")?;
        Ok((header.topic_id, message))
    }

    /// Builds an actor without running its event loop.
    async fn t_actor() -> Result<Actor, BindError> {
        let endpoint = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .bind()
            .await?;
        let metrics = Arc::new(Metrics::default());
        let (actor, _rpc_tx, _local_tx) = Actor::new(
            endpoint,
            Default::default(),
            metrics,
            None,
            Default::default(),
        );
        Ok(actor)
    }

    /// Sets the peer's flag in `wanted`, as the actor does when a topic starts or stops using it.
    fn set_wanted(actor: &Actor, peer: EndpointId, wanted: bool) {
        actor
            .peers
            .get(&peer)
            .expect("the peer has a connection")
            .set_wanted(wanted);
    }

    /// Waits until `conn` closed, and checks that our side closed it with `code`.
    async fn closed_with(conn: &Connection, code: u32) -> Result {
        let reason = timeout(Duration::from_secs(2), conn.closed())
            .await
            .std_context("the connection stayed open")?;
        assert!(
            matches!(
                &reason,
                iroh::endpoint::ConnectionError::ApplicationClosed(close)
                    if close.error_code == VarInt::from_u32(code)
            ),
            "{reason:?}"
        );
        Ok(())
    }

    /// Returns an unset flag of [`util::StreamHeader::handles_close`] for one connection.
    fn close_flag() -> watch::Sender<bool> {
        watch::channel(false).0
    }

    /// Runs `retire` on paused time, applies `steps` at their offsets, and returns when it ended.
    ///
    /// A step sets `wanted` and `sending`. `None` means `retire` did not end within a minute.
    async fn retire_ends_after(
        wanted: bool,
        sending: bool,
        steps: &[(u64, bool, bool)],
    ) -> Option<Duration> {
        let (wanted_tx, wanted) = watch::channel(wanted);
        let (sending_tx, sending) = watch::channel(sending);
        let start = Instant::now();
        let mut retire = std::pin::pin!(retire(wanted, sending));
        for (at, wanted, sending) in steps {
            let at = start + Duration::from_secs(*at);
            tokio::select! {
                _ = &mut retire => return Some(start.elapsed()),
                _ = n0_future::time::sleep_until(at) => {}
            }
            wanted_tx.send_replace(*wanted);
            sending_tx.send_replace(*sending);
        }
        timeout(Duration::from_secs(60), retire)
            .await
            .ok()
            .map(|_| start.elapsed())
    }

    /// The grace starts once no topic uses the peer and our sends ended.
    #[tokio::test(start_paused = true)]
    async fn retire_waits_for_our_sends_to_end() {
        assert_eq!(retire_ends_after(false, false, &[]).await, Some(IDLE_GRACE));
        assert_eq!(
            retire_ends_after(false, true, &[(3, false, false)]).await,
            Some(Duration::from_secs(3) + IDLE_GRACE)
        );
        // A send in the grace starts it over.
        assert_eq!(
            retire_ends_after(false, false, &[(4, false, true), (5, false, false)]).await,
            Some(Duration::from_secs(5) + IDLE_GRACE)
        );
    }

    /// Sends that do not end delay the close up to the limit.
    #[tokio::test(start_paused = true)]
    async fn retire_limits_the_wait_for_sends() {
        assert_eq!(
            retire_ends_after(false, true, &[]).await,
            Some(RETIRE_LIMIT)
        );
        // The limit counts from when the peer became unused.
        assert_eq!(
            retire_ends_after(true, true, &[(10, false, true)]).await,
            Some(Duration::from_secs(10) + RETIRE_LIMIT)
        );
    }

    /// A peer wanted and unused again before `retire` runs gets a new limit and grace.
    #[tokio::test(start_paused = true)]
    async fn retire_starts_over_after_an_unseen_revival() {
        let (wanted_tx, wanted) = watch::channel(false);
        let (sending_tx, sending) = watch::channel(true);
        let start = Instant::now();
        let mut retire = std::pin::pin!(retire(wanted, sending));
        assert!(timeout(Duration::from_secs(19), &mut retire).await.is_err());

        // All three land before `retire` runs again.
        wanted_tx.send_replace(true);
        wanted_tx.send_replace(false);
        sending_tx.send_replace(false);
        retire.await;
        assert_eq!(start.elapsed(), Duration::from_secs(19) + IDLE_GRACE);
    }

    /// A peer we want again stops the grace.
    #[tokio::test(start_paused = true)]
    async fn retire_stops_while_the_peer_is_wanted() {
        assert_eq!(
            retire_ends_after(false, false, &[(2, true, false)]).await,
            None
        );
        assert_eq!(
            retire_ends_after(false, false, &[(2, true, false), (3, false, false)]).await,
            Some(Duration::from_secs(3) + IDLE_GRACE)
        );
    }

    /// A stopped writer's completion must not remove messages queued after reopening.
    #[tokio::test]
    async fn reopened_queue_survives_its_previous_writer() {
        let now = Instant::now();
        let topic = [42; 32].into();
        let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
        let first = peer.handle_connection(Side::Server, 1).unwrap();
        peer.close_queue();
        assert!(!first.lock().await.next().await, "the writer must stop");

        let message = join_message(topic, 0);
        assert!(matches!(peer.send(message.clone()), Pushed::Queued));
        assert!(peer.needs_dial(), "reopening must request a new writer");
        assert!(!peer.connection_finished(1, now));
        assert!(!peer.can_forget(now + PEER_RETENTION));

        let next = peer
            .handle_connection(Side::Client, 2)
            .expect("awaiting writer");
        assert!(first.revoked().is_cancelled());
        assert!(!peer.needs_dial());
        let mut reader = next.lock().await;
        assert!(reader.next().await);
        assert_eq!(reader.current().unwrap().message.topic, message.topic);
        assert_eq!(reader.current().unwrap().message.message, message.message);
    }

    /// The old writer can drain a reopened queue before the actor decides to dial.
    #[tokio::test]
    async fn reopened_empty_queue_still_needs_a_dial() {
        let now = Instant::now();
        let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
        let old = peer.handle_connection(Side::Server, 1).unwrap();
        peer.close_queue();
        assert!(matches!(
            peer.send(join_message([43; 32].into(), 0)),
            Pushed::Queued
        ));

        // The previous lease is still valid. Model it running between the
        // actor's push and its decision to schedule a replacement dial.
        let mut reader = old.lock().await;
        assert!(reader.next().await);
        reader.advance();
        drop(reader);

        assert!(peer.queue.is_empty());
        assert!(peer.needs_dial(), "an empty open queue still needs a dial");
        assert!(!peer.connection_finished(1, now));
        assert!(peer.forget_at.is_none(), "the reopened peer is not idle");
        assert!(!peer.can_forget(now + PEER_RETENTION));
        let _next = peer
            .handle_connection(Side::Client, 2)
            .expect("the replacement dial must take the queue");
        assert!(old.revoked().is_cancelled());
        assert!(!peer.needs_dial());
    }

    /// A completed dial writes only a queue that has no writer.
    #[test]
    fn completed_dial_leaves_an_existing_writer() {
        let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
        let first = peer.handle_connection(Side::Server, 1).unwrap();
        let second = peer.handle_connection(Side::Server, 2).unwrap();
        let late = peer.handle_connection(Side::Client, 3);
        assert!(first.revoked().is_cancelled());
        assert!(!second.revoked().is_cancelled());
        assert!(late.is_none(), "the dial must not replace the writer");

        let now = Instant::now();
        assert!(!peer.connection_finished(1, now));
        assert!(!peer.connection_finished(3, now));
        assert!(peer.connection_finished(2, now));
        peer.discard_queue(now); // The actor processes PeerDisconnected immediately.
        assert!(is_idle(&peer));

        let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
        peer.close_queue();
        let first = peer.handle_connection(Side::Server, 1).unwrap();
        let second = peer.handle_connection(Side::Server, 2).unwrap();
        let late = peer.handle_connection(Side::Client, 3);
        assert!(first.revoked().is_cancelled());
        assert!(!second.revoked().is_cancelled());
        assert!(
            late.is_none(),
            "the dial of a closed peer must only receive"
        );
        assert!(!peer.connection_finished(1, now));
        assert!(!peer.connection_finished(3, now));
        assert!(peer.connection_finished(2, now));
        peer.discard_queue(now);
        assert!(is_idle(&peer));
        assert!(peer.can_forget(now + PEER_RETENTION));
    }

    /// A disconnect drops the queue, but its writer still sends what it holds.
    #[tokio::test]
    async fn discarded_queue_goes_out_on_its_writer() {
        let now = Instant::now();
        let topic = [7; 32].into();
        let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
        let lease = peer.handle_connection(Side::Server, 1).unwrap();
        let aborted = peer.aborted.clone();
        let message = join_message(topic, 0);
        assert!(matches!(peer.send(message.clone()), Pushed::Queued));
        peer.discard_queue(now);
        assert!(is_idle(&peer));
        assert!(!aborted.is_cancelled());

        let mut reader = lease.lock().await;
        assert!(reader.next().await);
        assert_eq!(reader.current().unwrap().message.message, message.message);
        reader.advance();
        assert!(!reader.next().await, "the old queue must end");
    }

    /// A failed dial does not close the connection that drains a closed queue.
    #[tokio::test]
    async fn failed_dial_leaves_the_draining_writer() -> Result {
        let mut actor = t_actor().await?;
        let peer_id = SecretKey::generate().public();
        let mut state = open_peer(queue::MIN_QUEUE_SIZE);
        let _lease = state.handle_connection(Side::Server, 1).unwrap();
        let aborted = state.aborted.clone();
        state.send(join_message([8; 32].into(), 0));
        state.close_queue();
        actor.peers.insert(peer_id, state);
        actor.handle_dial_failed(peer_id);
        assert!(is_idle(&actor.peers[&peer_id]));
        assert!(!aborted.is_cancelled(), "the writer was aborted");
        Ok(())
    }

    /// The end of the writer of an open queue disconnects the peer, also if a dial still receives.
    #[test]
    fn lost_writer_disconnects_while_a_dial_receives() {
        let now = Instant::now();
        let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
        let _writer = peer.handle_connection(Side::Server, 1).unwrap();
        assert!(peer.handle_connection(Side::Client, 2).is_none());
        assert!(peer.connection_finished(1, now));
        peer.discard_queue(now);
        assert!(is_idle(&peer));
        assert!(!peer.needs_dial());
        assert!(!peer.connection_finished(2, now));
        assert!(peer.can_forget(now + PEER_RETENTION));
    }

    /// The end of the writer of a closed queue drops the rest of it, and is no disconnect.
    #[test]
    fn lost_writer_of_a_closed_queue_drops_it() {
        let now = Instant::now();
        let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
        let _writer = peer.handle_connection(Side::Server, 1).unwrap();
        peer.send(join_message([9; 32].into(), 0));
        peer.close_queue();
        assert!(!peer.connection_finished(1, now));
        assert!(is_idle(&peer));
        assert!(!peer.needs_dial(), "no dial for a closed peer");
    }

    /// A message that does not fit still reopens a closed queue, which then needs a dial.
    #[test]
    fn dropped_message_reopens_a_closed_queue() {
        let topic = [10; 32].into();
        let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
        let mut i = 0;
        while matches!(peer.send(numbered(topic, i)), Pushed::Queued) {
            i += 1;
        }
        peer.close_queue();
        assert!(!peer.needs_dial(), "a closed queue needs no dial");
        assert!(matches!(peer.send(numbered(topic, i)), Pushed::Dropped(_)));
        assert!(!peer.queue.is_closing());
        assert!(peer.needs_dial(), "the reopened queue has no writer");
    }

    /// Queue disposal cannot reset the signals still used by older connections.
    #[test]
    fn peer_signals_survive_queue_replacement() {
        let now = Instant::now();
        let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
        let old = peer.handle_connection(Side::Server, 1).unwrap();
        let old_aborted = peer.aborted.clone();
        let mut wanted = peer.wanted.subscribe();
        let handles_close = peer.handles_close.subscribe();
        peer.handles_close.send_replace(true);
        peer.set_wanted(true);

        // Ordinary replacement lets the old queue drain, but all existing
        // connections must still observe a later overflow of the new queue.
        peer.discard_queue(now);
        assert!(!old_aborted.is_cancelled());
        assert!(!old.revoked().is_cancelled());
        peer.open_queue();
        let current = peer.handle_connection(Side::Client, 2).unwrap();
        assert!(peer.handle_connection(Side::Client, 3).is_none());
        let receiving_aborted = peer.aborted.clone();
        peer.abort(now);
        assert!(old_aborted.is_cancelled());
        assert!(old.revoked().is_cancelled());
        assert!(receiving_aborted.is_cancelled());
        assert!(current.revoked().is_cancelled());
        assert!(!peer.can_forget(now + PEER_RETENTION));

        peer.open_queue();
        let new = peer
            .handle_connection(Side::Client, 4)
            .expect("awaiting writer");
        let new_aborted = peer.aborted.clone();
        assert!(*wanted.borrow_and_update());
        assert!(*handles_close.borrow());
        assert!(!new_aborted.is_cancelled());
        assert!(!new.revoked().is_cancelled());
        assert!(
            !peer.connection_finished(1, now),
            "old completion must leave the new queue intact"
        );
        assert!(!peer.connection_finished(2, now));
        assert!(!peer.connection_finished(3, now));
        assert_eq!(peer.writer, Some(4));
        assert!(!new_aborted.is_cancelled());
        assert!(!peer.needs_dial());
    }

    /// Retention starts after the last processed completion, independently of the close flag.
    #[test]
    fn peer_retention_waits_for_all_connections_and_the_queue() {
        for handles_close in [false, true] {
            let now = Instant::now();
            let later = now + PEER_RETENTION;
            let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
            let _first = peer.handle_connection(Side::Server, 1).unwrap();
            let _second = peer.handle_connection(Side::Server, 2).unwrap();
            peer.handles_close.send_replace(handles_close);
            peer.abort(now);
            assert!(!peer.connection_finished(1, now));
            assert!(!peer.can_forget(later));
            assert!(!peer.connection_finished(2, later));
            assert!(!peer.can_forget(later + PEER_RETENTION - Duration::from_nanos(1)));
            assert!(peer.can_forget(later + PEER_RETENTION));

            // A new queue cancels retention, even without a running connection.
            peer.open_queue();
            assert!(!peer.can_forget(later + PEER_RETENTION));
            peer.abort(later);
            peer.abort(later + Duration::from_secs(1));
            assert!(
                peer.can_forget(later + PEER_RETENTION),
                "repeated disposal must not prolong retention"
            );
        }
    }

    /// A failed dial after a disconnect drops the queue it kept.
    #[tokio::test]
    async fn failed_retired_dial_releases_its_queue() -> Result {
        let mut actor = t_actor().await?;
        let peer = SecretKey::generate().public();
        actor
            .peers
            .insert(peer, pending_with(vec![join_message([1; 32].into(), 0)]));
        actor
            .peers
            .get_mut(&peer)
            .expect("known peer")
            .close_queue();
        actor.handle_dial_failed(peer);
        assert!(is_idle(&actor.peers[&peer]));
        Ok(())
    }

    /// A failed dial keeps the state of a peer that reached us on its own connection.
    ///
    /// Its connection still works, so dropping the state would lose the peer.
    #[tokio::test]
    #[traced_test]
    async fn dial_failure_keeps_active_peer() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect_back().await?;
        let peer_id = pair.ep2.id();
        let endpoint = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .bind()
            .await?;
        let metrics = Arc::new(Metrics::default());
        // The senders keep the event loop running.
        let (mut actor, _rpc_tx, _local_tx) = Actor::new(
            endpoint,
            Default::default(),
            metrics.clone(),
            None,
            Default::default(),
        );
        actor.handle_connection(conn);
        // Our endpoint has no address for the peer, so the dial fails.
        actor.dialer.queue_dial(peer_id, actor.alpn.clone());

        let addr_updates = &mut n0_future::stream::pending();
        timeout(Duration::from_secs(5), async {
            while metrics.actor_tick_dialer_failure.get() == 0 {
                actor.event_loop(addr_updates, 0).await;
            }
        })
        .await
        .std_context("the dial did not fail")?;
        assert!(matches!(
            send_state(&actor, peer_id),
            Some((Some(_), false))
        ));
        assert!(peer_conn.close_reason().is_none(), "the connection closed");
        Ok(())
    }

    /// Opens a stream with our header, as a new node does on each stream.
    ///
    /// The header tells the other side that the peer handles a close.
    async fn show_that_it_handles_a_close(conn: &Connection, topic: TopicId) {
        let mut stream = conn.open_uni().await.expect("open stream");
        util::StreamHeader::new(topic)
            .write(&mut stream, &mut Vec::new(), TEST_FRAME_LIMIT)
            .await
            .expect("write header");
        stream.finish().expect("finish");
    }

    /// The stream header that nodes from 0.90 to 0.101 write and read.
    ///
    /// Their `StreamHeader` and `read_frame` are the same in every release from
    /// 0.90 to 0.101, and their `read_lp` and `write_frame` are the same as ours
    /// since 0.95. So `util::read_frame` and `util::write_frame` of this struct
    /// read and write a header as those nodes do.
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct OldStreamHeader {
        topic_id: TopicId,
    }

    /// Old and new nodes read the stream headers of each other on a stream, and the frames after them.
    #[tokio::test]
    #[traced_test]
    async fn stream_headers_cross_versions_on_a_stream() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let topic: TopicId = blake3::hash(b"versions").into();
        let join = join_message(topic, 0).message;
        let encoded_join = postcard::to_stdvec(&join).std_context("encode")?;
        let mut buffer = Vec::new();
        let mut read_buffer = BytesMut::new();

        // We write, an old node reads.
        let mut stream = conn.open_uni().await.std_context("open stream")?;
        util::StreamHeader::new(topic)
            .write(&mut stream, &mut buffer, TEST_FRAME_LIMIT)
            .await
            .std_context("write header")?;
        util::write_frame(&mut stream, &join, &mut buffer, TEST_FRAME_LIMIT)
            .await
            .std_context("write frame")?;
        stream.finish().std_context("finish")?;
        let mut recv = peer_conn.accept_uni().await.std_context("accept stream")?;
        let header: OldStreamHeader =
            util::read_frame(&mut recv, &mut read_buffer, TEST_FRAME_LIMIT)
                .await
                .std_context("old node reads header")?
                .expect("a header");
        assert_eq!(header.topic_id, topic);
        let frame = util::read_lp(&mut recv, &mut read_buffer, TEST_FRAME_LIMIT)
            .await
            .std_context("old node reads frame")?
            .expect("a frame");
        assert_eq!(&frame[..], &encoded_join[..], "the frame after the header");

        // An old node writes, we read.
        let mut stream = peer_conn.open_uni().await.std_context("open stream")?;
        util::write_frame(
            &mut stream,
            &OldStreamHeader { topic_id: topic },
            &mut buffer,
            TEST_FRAME_LIMIT,
        )
        .await
        .std_context("write old header")?;
        util::write_frame(&mut stream, &join, &mut buffer, TEST_FRAME_LIMIT)
            .await
            .std_context("write frame")?;
        stream.finish().std_context("finish")?;
        let mut recv = conn.accept_uni().await.std_context("accept stream")?;
        let header = util::StreamHeader::read(&mut recv, &mut read_buffer, TEST_FRAME_LIMIT)
            .await
            .std_context("read old header")?;
        assert_eq!(header.topic_id, topic);
        assert!(!header.handles_close);
        let frame = util::read_lp(&mut recv, &mut read_buffer, TEST_FRAME_LIMIT)
            .await
            .std_context("read frame")?
            .expect("a frame");
        assert_eq!(
            &frame[..],
            &encoded_join[..],
            "the frame after the old header"
        );
        Ok(())
    }

    /// An unused connection to a peer whose header has no flags stays open.
    ///
    /// Nodes up to 0.101 send the topic alone. Such a node would lose its later
    /// messages to us if we closed a connection that is no neighbor link of it.
    #[tokio::test]
    #[traced_test]
    async fn unused_connection_from_an_old_peer_stays() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect_back().await?;
        let peer_id = pair.ep2.id();
        let topic: TopicId = blake3::hash(b"old_peer").into();
        let mut actor = t_actor().await?;
        actor.handle_connection(conn);
        let mut stream = peer_conn.open_uni().await.std_context("open stream")?;
        let mut buffer = Vec::new();
        util::write_frame(&mut stream, &topic, &mut buffer, TEST_FRAME_LIMIT)
            .await
            .std_context("write header")?;
        let join = join_message(topic, 0).message;
        util::write_frame(&mut stream, &join, &mut buffer, TEST_FRAME_LIMIT)
            .await
            .std_context("write frame")?;
        let event = timeout(Duration::from_secs(5), actor.in_event_rx.recv())
            .await
            .std_context("the message did not arrive")?;
        assert!(
            matches!(event, Some(InEvent::RecvMessage(peer, _)) if peer == peer_id),
            "{event:?}"
        );

        tokio::time::sleep(IDLE_GRACE + Duration::from_secs(1)).await;
        assert!(
            actor.connection_tasks.try_join_next().is_none(),
            "the connection closed"
        );
        assert!(peer_conn.close_reason().is_none(), "the connection closed");
        Ok(())
    }

    /// Pushes `count` payloads of `len` bytes for `topic` into a large queue.
    fn queue_payloads(topic: TopicId, count: usize, len: usize) -> QueueLease {
        let mut queue = SendQueue::new(64 << 20);
        for _ in 0..count {
            let message = ProtoMessage {
                topic,
                message: TopicMessage::test_gossip(len),
            };
            assert!(matches!(queue.push(message), Pushed::Queued));
        }
        queue.lease()
    }

    /// A write that makes no progress for the write timeout closes the connection.
    ///
    /// The peer never reads, so flow control blocks every write once the
    /// receive window is full. The writer never ended.
    #[tokio::test]
    #[traced_test]
    async fn stalled_write_closes_the_connection() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let topic: TopicId = blake3::hash(b"stalled").into();
        // More than the windows of the peer, which reads nothing.
        let queue_lease = queue_payloads(topic, 200, 60_000);
        let send_loop = SendLoop::new(conn, queue_lease, TEST_FRAME_LIMIT);
        let result = timeout(util::WRITE_TIMEOUT * 5, send_loop.run())
            .await
            .std_context("the writer never ended")?;
        assert!(
            matches!(result, Err(util::WriteError::Stalled { .. })),
            "{result:?}"
        );
        closed_with(&peer_conn, util::CLOSE_STALLED).await
    }

    /// A peer that reads one frame slower than the write timeout keeps the connection.
    ///
    /// The timeout counts the time without progress, not the time of a frame.
    #[tokio::test]
    #[traced_test]
    async fn slow_reader_keeps_the_connection() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let topic: TopicId = blake3::hash(b"slow").into();
        // At 64 KiB per 300 ms, the frame takes about 6.5 s, over the write timeout.
        let (count, len) = (1, 1_400_000);
        let queue_lease = queue_payloads(topic, count, len);
        let send_loop = SendLoop::new(conn, queue_lease, 4 << 20);
        let send = AbortOnDropHandle::new(spawn(async move { send_loop.run().await }));

        let start = Instant::now();
        let mut stream = peer_conn.accept_uni().await.std_context("accept")?;
        let mut chunk = vec![0u8; 64 * 1024];
        let mut read = 0;
        while read < count * len {
            tokio::time::sleep(Duration::from_millis(300)).await;
            read += stream
                .read(&mut chunk)
                .await
                .std_context("read")?
                .std_context("the stream ended early")?;
        }
        assert!(
            start.elapsed() > util::WRITE_TIMEOUT,
            "the reader was not slow"
        );
        let result = timeout(Duration::from_secs(5), send)
            .await
            .std_context("the writer did not end")?
            .std_context("send task")?;
        assert!(result.is_ok(), "{result:?}");
        Ok(())
    }

    /// A peer at its stream limit loses the connection after the open timeout.
    ///
    /// It accepts no stream, so no stream of ours ends, and the peer never
    /// raises its stream limit.
    #[tokio::test]
    #[traced_test]
    async fn stream_limit_closes_the_connection() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let mut queue = SendQueue::new(64 << 20);
        for i in 0..1000u32 {
            let topic: TopicId = blake3::hash(&i.to_be_bytes()).into();
            assert!(matches!(queue.push(join_message(topic, 0)), Pushed::Queued));
        }
        let send_loop = SendLoop::new(conn, queue.lease(), TEST_FRAME_LIMIT);
        let result = timeout(util::OPEN_TIMEOUT * 5, send_loop.run())
            .await
            .std_context("the writer never ended")?;
        assert!(
            matches!(result, Err(util::WriteError::OpenStalled { .. })),
            "{result:?}"
        );
        closed_with(&peer_conn, util::CLOSE_STALLED).await
    }

    /// A peer whose queue has no room for a control message loses its connection (#47).
    ///
    /// The actor waited for room in the queue of a peer that did not read. While it
    /// waited, no topic made progress, no timer fired, and the peer stayed.
    #[tokio::test]
    #[traced_test]
    async fn full_control_queue_aborts_the_peer_without_waiting() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let peer = pair.ep2.id();
        let mut actor = t_actor().await?;
        let mut state = open_peer(queue::MIN_QUEUE_SIZE);
        let queue_lease = state
            .handle_connection(Side::Server, conn.stable_id())
            .expect("an accepted connection takes the lease");
        let aborted = state.aborted.clone();
        actor.peers.insert(peer, state);
        let (in_event_tx, _in_event_rx) = mpsc::channel(16);
        // The runtime has one thread, so the writer takes nothing before the queue is full.
        let _conn_loop = AbortOnDropHandle::new(spawn(connection_loop(
            peer,
            conn,
            aborted,
            Some(queue_lease),
            in_event_tx,
            TEST_FRAME_LIMIT,
            watch::channel(true).1,
            close_flag(),
        )));

        join_until_full(&mut actor, peer);
        assert_eq!(actor.metrics.peers_send_queue_full.get(), 1);
        // More control messages to the closing connection fail nothing new.
        let topic: TopicId = blake3::hash(b"one more").into();
        let join = InEvent::Command(topic, ProtoCommand::Join(vec![peer]));
        actor.handle_in_event(join, Instant::now());
        assert_eq!(actor.metrics.peers_send_queue_full.get(), 1);
        timeout(Duration::from_secs(2), peer_conn.closed())
            .await
            .std_context("the connection to the full peer stayed open")?;
        Ok(())
    }

    /// Crossed dials can leave the remote writer on our receive-only connection.
    #[tokio::test(flavor = "current_thread")]
    #[traced_test]
    async fn control_overflow_closes_both_crossed_connections() -> Result {
        let pair = TestPair::new(1).await?;
        let (accepted, peer_accepted) = pair.connect_back().await?;
        let (dialed, peer_dialed) = pair.connect().await?;
        let peer = pair.ep2.id();
        let writer = accepted.stable_id();
        let mut actor = t_actor().await?;
        actor.send_queue_size = queue::MIN_QUEUE_SIZE;
        actor.handle_connection(accepted);
        actor.handle_connection(dialed);
        assert_eq!(actor.peers[&peer].writer, Some(writer));
        assert_eq!(actor.peers[&peer].connections, 2);
        set_wanted(&actor, peer, true);

        // Neither task can drain the queue while this synchronous helper runs.
        join_until_full(&mut actor, peer);
        assert_eq!(actor.metrics.peers_send_queue_full.get(), 1);
        set_wanted(&actor, peer, true);

        // An abort must close both, regardless of their leases or idle retirement.
        closed_with(&peer_accepted, util::CLOSE_QUEUE_FULL).await?;
        closed_with(&peer_dialed, util::CLOSE_QUEUE_FULL).await?;
        Ok(())
    }

    /// A control overflow during a handover closes the stuck old connection, not the new one (a cost of the design).
    ///
    /// The old connection does not read, so its writer keeps the queue for up
    /// to the handover limit, and control fills the queue meanwhile. Today the
    /// overflow aborts the peer, and closes the new and healthy connection too.
    #[tokio::test(flavor = "current_thread")]
    #[traced_test]
    #[ignore = "a cost of the design, see SendQueue"]
    async fn overflow_during_a_handover_keeps_the_new_connection() -> Result {
        let pair = TestPair::new(1).await?;
        let (old, peer_old) = pair.connect_back().await?;
        let (new, peer_new) = pair.connect_back().await?;
        let peer = pair.ep2.id();
        let topic: TopicId = blake3::hash(b"overflow during a handover").into();
        let mut actor = t_actor().await?;
        actor.send_queue_size = 8 << 20;
        actor.handle_connection(old);
        for _ in 0..1000 {
            let message = ProtoMessage {
                topic,
                message: TopicMessage::test_gossip(3000),
            };
            push_active(&actor, peer, message);
        }
        // Wait until flow control blocks the old writer.
        tokio::time::sleep(Duration::from_secs(1)).await;
        actor.handle_connection(new);

        // Large joins fill the queue before a writer runs again: nothing awaits.
        let data = PeerData::new(vec![0u8; 2000]);
        actor.handle_in_event(InEvent::UpdatePeerData(data), Instant::now());
        join_until_full(&mut actor, peer);
        assert_eq!(actor.metrics.peers_send_queue_full.get(), 1, "no overflow");
        timeout(Duration::from_secs(2), peer_old.closed())
            .await
            .std_context("the stuck old connection stayed open")?;
        assert!(
            peer_new.close_reason().is_none(),
            "the new connection closed"
        );
        Ok(())
    }

    /// A peer whose queue fills during the dial goes down, and its queue with it.
    #[tokio::test]
    #[traced_test]
    async fn full_queue_during_the_dial_drops_the_peer() -> Result {
        let mut actor = t_actor().await?;
        actor.send_queue_size = queue::MIN_QUEUE_SIZE;
        // The endpoint has no address for the peer, so the dial does not connect soon.
        let peer = SecretKey::generate().public();
        join_until_full(&mut actor, peer);
        assert_eq!(actor.metrics.peers_send_queue_full.get(), 1);
        assert!(
            !actor.state.uses_peer(&peer),
            "the protocol still uses the closed peer"
        );
        Ok(())
    }

    /// An inbound connection that no topic uses closes, even with a partial header.
    #[tokio::test]
    #[traced_test]
    async fn unwanted_connection_with_partial_header_closes() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect_back().await?;
        let mut actor = t_actor().await?;
        actor.handle_connection(conn);
        show_that_it_handles_a_close(&peer_conn, [0; 32].into()).await;
        let mut stream = peer_conn.open_uni().await.std_context("open stream")?;
        stream
            .write_all(&[0, 0, 0, 32, 0])
            .await
            .std_context("partial header")?;
        let result = timeout(
            IDLE_GRACE + Duration::from_secs(3),
            actor.connection_tasks.join_next(),
        )
        .await
        .std_context("the connection stayed open")?;
        assert!(result.expect("connection task").is_ok());
        timeout(Duration::from_secs(1), peer_conn.closed())
            .await
            .std_context("the peer did not see the close")?;
        Ok(())
    }

    /// A send loop whose connection closed lets go of the queue, for another connection of the peer.
    #[tokio::test]
    #[traced_test]
    async fn closed_send_loop_lets_go_of_the_queue() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let mut queue = SendQueue::new(queue::MIN_QUEUE_SIZE);
        let send = SendLoop::new(conn, queue.lease(), TEST_FRAME_LIMIT);
        peer_conn.close(0u32.into(), b"done");
        timeout(Duration::from_secs(3), send.run())
            .await
            .std_context("the send loop did not end")??;
        let message = join_message(TopicId::from_bytes([0; 32]), 0);
        assert!(matches!(queue.push(message), Pushed::Queued));
        let reader = queue
            .try_reader()
            .std_context("the queue is still locked")?;
        assert!(!reader.is_empty(), "the message did not stay queued");
        Ok(())
    }

    /// An aborted peer loses the connection that took its queue, also once its writer ended.
    ///
    /// The protocol closed the peer and reopened it before a new dial
    /// completed. The writer of the old connection sent the queue and ended,
    /// but the connection still reads from the peer, which must learn of the
    /// drop.
    #[tokio::test]
    #[traced_test]
    async fn aborted_peer_loses_the_connection_that_took_its_queue() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let peer_id = pair.ep2.id();
        let topic: TopicId = blake3::hash(b"reopened").into();
        let mut peers = HashMap::new();
        let mut state = open_peer(queue::MIN_QUEUE_SIZE);
        let queue_lease = state
            .handle_connection(Side::Server, conn.stable_id())
            .expect("an accepted connection takes the lease");
        let (in_event_tx, _in_event_rx) = mpsc::channel(16);
        let _conn_loop = AbortOnDropHandle::new(spawn(connection_loop(
            peer_id,
            conn,
            state.aborted.clone(),
            Some(queue_lease),
            in_event_tx,
            TEST_FRAME_LIMIT,
            watch::channel(true).1,
            close_flag(),
        )));
        assert!(matches!(
            state.queue.push(join_message(topic, 0)),
            Pushed::Queued
        ));
        timeout(Duration::from_secs(5), read_next_message(&peer_conn))
            .await
            .std_context("the queue did not go out")??;
        peers.insert(peer_id, state);
        peers.get_mut(&peer_id).expect("known peer").close_queue();
        // The writer lets go of the queue once it ended.
        timeout(Duration::from_secs(5), async {
            while peers[&peer_id].queue.try_reader().is_none() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .std_context("the writer did not end")?;
        peers.get_mut(&peer_id).expect("dropped").open_queue();

        peers
            .get_mut(&peer_id)
            .expect("known peer")
            .abort(Instant::now());
        assert!(is_idle(&peers[&peer_id]), "the queue stayed");
        closed_with(&peer_conn, util::CLOSE_QUEUE_FULL).await
    }

    /// A closed peer that the protocol reopens keeps its queue, and the dial delivers all of it.
    #[tokio::test]
    #[traced_test]
    async fn peer_wanted_again_during_the_dial_keeps_its_queue() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let peer_id = pair.ep2.id();
        let (before, after): (TopicId, TopicId) = (
            blake3::hash(b"before").into(),
            blake3::hash(b"after").into(),
        );
        let mut actor = t_actor().await?;
        actor
            .peers
            .insert(peer_id, pending_with(vec![join_message(before, 0)]));
        actor
            .peers
            .get_mut(&peer_id)
            .expect("known peer")
            .close_queue();
        let join = InEvent::Command(after, ProtoCommand::Join(vec![peer_id]));
        actor.handle_in_event(join, Instant::now());
        assert!(
            matches!(send_state(&actor, peer_id), Some((None, false))),
            "the peer is not pending again"
        );

        actor.handle_connection(conn);
        assert!(
            matches!(send_state(&actor, peer_id), Some((Some(_), false))),
            "the dial was not kept"
        );
        let mut topics = BTreeSet::new();
        for _ in 0..2 {
            let (topic, _) = timeout(Duration::from_secs(5), read_next_message(&peer_conn))
                .await
                .std_context("a queued message did not go out")??;
            topics.insert(topic);
        }
        assert_eq!(topics, BTreeSet::from([before, after]));
        Ok(())
    }

    /// Joins new topics with `peer` until its queue had no room for a control message.
    fn join_until_full(actor: &mut Actor, peer: EndpointId) {
        for i in 0..10_000u32 {
            let topic: TopicId = blake3::hash(&i.to_be_bytes()).into();
            let join = InEvent::Command(topic, ProtoCommand::Join(vec![peer]));
            actor.handle_in_event(join, Instant::now());
            if actor.metrics.peers_send_queue_full.get() > 0 {
                return;
            }
        }
    }

    /// Returns the queue of `peer_id`, which must have an active connection.
    fn active_queue(actor: &Actor, peer_id: EndpointId) -> &SendQueue {
        let peer = &actor.peers[&peer_id];
        if peer.queue.is_closing() || peer.writer.is_none() {
            panic!("not active: {:?}, writer: {:?}", peer.queue, peer.writer);
        }
        &peer.queue
    }

    /// Pushes `message` to the queue of the active peer `peer_id`.
    fn push_active(actor: &Actor, peer_id: EndpointId, message: ProtoMessage) {
        let pushed = active_queue(actor, peer_id).push(message);
        assert!(matches!(pushed, Pushed::Queued));
    }

    /// A replaced connection hands the rest of the queue to the new one, which keeps its order.
    ///
    /// The old writer never took the queue, so all messages go out on the new
    /// connection, in one stream and in the order of the queue.
    ///
    /// The runtime has one thread, so the old writer cannot take the queue before the test does.
    #[tokio::test(flavor = "current_thread")]
    #[traced_test]
    async fn replaced_connection_hands_over_its_queue() -> Result {
        let pair = TestPair::new(1).await?;
        let (old, peer_old) = pair.connect_back().await?;
        let (new, peer_new) = pair.connect_back().await?;
        let peer_id = pair.ep2.id();
        let topic: TopicId = blake3::hash(b"handed over").into();
        let mut actor = t_actor().await?;
        actor.handle_connection(old);
        // The old writer cannot take the queue before the new connection replaces it.
        let held = active_queue(&actor, peer_id)
            .try_reader()
            .std_context("the queue is locked")?;
        push_active(&actor, peer_id, join_message(topic, 1));
        actor.handle_connection(new);
        push_active(&actor, peer_id, join_message(topic, 2));
        drop(held);

        let read = async {
            let mut stream = peer_new.accept_uni().await.std_context("accept")?;
            let mut buffer = BytesMut::new();
            util::StreamHeader::read(&mut stream, &mut buffer, TEST_FRAME_LIMIT).await?;
            let mut sizes = Vec::new();
            for _ in 0..2 {
                let message: TopicMessage =
                    util::read_frame(&mut stream, &mut buffer, TEST_FRAME_LIMIT)
                        .await?
                        .std_context("the stream ended")?;
                sizes.push(postcard::experimental::serialized_size(&message).anyerr()?);
            }
            Ok::<_, AnyError>(sizes)
        };
        let sizes = timeout(Duration::from_secs(5), read)
            .await
            .std_context("the queue did not go out on the new connection")??;
        assert!(sizes[0] < sizes[1], "the messages lost their order");
        assert!(
            timeout(Duration::from_millis(500), peer_old.accept_uni())
                .await
                .is_err(),
            "the old connection sent a message"
        );
        Ok(())
    }

    /// Pushes `count` payloads of distinct sizes, which may fill the windows of a connection.
    fn push_many(actor: &Actor, peer_id: EndpointId, topic: TopicId, count: usize) {
        for i in 0..count {
            push_active(actor, peer_id, numbered(topic, i));
        }
    }

    /// Returns the payload number `i`, which differs from the others in its size.
    ///
    /// Up to 3000 of them fit the frame limit of the actor.
    fn numbered(topic: TopicId, i: usize) -> ProtoMessage {
        ProtoMessage {
            topic,
            message: TopicMessage::test_gossip(1000 + i),
        }
    }

    /// A replaced connection that cannot finish its message hands it to the new one, and loses nothing.
    ///
    /// The old connection does not read, so flow control blocks its writer in
    /// the middle of a message. The new connection writes that message first.
    /// The old stream ends after the part of it: the peer gets each message
    /// before it on the old connection, and that message once. The old
    /// connection stays open, as the peer may still send on it.
    #[tokio::test]
    #[traced_test]
    async fn stuck_replaced_connection_hands_over_its_queue() -> Result {
        let pair = TestPair::new(1).await?;
        let (old, peer_old) = pair.connect_back().await?;
        let (new, peer_new) = pair.connect_back().await?;
        let peer_id = pair.ep2.id();
        let topic: TopicId = blake3::hash(b"stuck").into();
        let mut actor = t_actor().await?;
        actor.send_queue_size = 64 << 20;
        actor.handle_connection(old);
        push_many(&actor, peer_id, topic, 3000);
        // Wait until flow control blocks the old writer.
        tokio::time::sleep(Duration::from_secs(1)).await;
        actor.handle_connection(new);

        let mut new_stream = timeout(util::HANDOVER_LIMIT * 3, peer_new.accept_uni())
            .await
            .std_context("the new connection sent nothing")?
            .std_context("accept")?;
        let mut new_buffer = BytesMut::new();
        let header =
            util::StreamHeader::read(&mut new_stream, &mut new_buffer, TEST_FRAME_LIMIT).await?;
        assert_eq!(header.topic_id, topic);
        let last = numbered(topic, 2999).message;
        let read_new = async {
            let mut messages = Vec::new();
            while messages.last() != Some(&last) {
                match util::read_frame(&mut new_stream, &mut new_buffer, TEST_FRAME_LIMIT).await {
                    Ok(Some(message)) => messages.push(message),
                    _ => break,
                }
            }
            messages
        };
        let new_messages = timeout(Duration::from_secs(10), read_new)
            .await
            .std_context("the new connection did not send the rest")?;
        let handed_over = (0..3000)
            .find(|&i| Some(&numbered(topic, i).message) == new_messages.first())
            .std_context("an unknown message")?;
        let rest: Vec<_> = (handed_over..3000)
            .map(|i| numbered(topic, i).message)
            .collect();
        assert!(
            new_messages == rest,
            "the new connection sent {} messages, not those from {handed_over}",
            new_messages.len()
        );

        let mut stream = peer_old.accept_uni().await.std_context("accept")?;
        let mut buffer = BytesMut::new();
        util::StreamHeader::read(&mut stream, &mut buffer, TEST_FRAME_LIMIT).await?;
        let read = async {
            let mut messages = Vec::new();
            // The part of the handed over message ends the stream with an error.
            while let Ok(Some(message)) =
                util::read_frame::<TopicMessage>(&mut stream, &mut buffer, TEST_FRAME_LIMIT).await
            {
                messages.push(message);
            }
            messages
        };
        let messages = timeout(Duration::from_secs(5), read)
            .await
            .std_context("the stuck stream did not end")?;
        let expected: Vec<_> = (0..handed_over)
            .map(|i| numbered(topic, i).message)
            .collect();
        assert!(
            messages == expected,
            "the old connection sent {} messages, not those before {handed_over}",
            messages.len()
        );
        assert!(
            peer_old.close_reason().is_none(),
            "the old connection closed"
        );
        Ok(())
    }

    /// A peer that we close and reopen keeps its queue, and the next connection writes the old messages first.
    ///
    /// The old connection does not read, so its writer still has the queue,
    /// which ends with a `Disconnect`. This checks the order of our writes
    /// only: the peer reads each stream on its own.
    #[tokio::test]
    #[traced_test]
    async fn dropped_peer_wanted_again_gets_the_old_queue_first() -> Result {
        let pair = TestPair::new(1).await?;
        let (old, _peer_old) = pair.connect_back().await?;
        let (new, peer_new) = pair.connect_back().await?;
        let peer_id = pair.ep2.id();
        let (left, joined): (TopicId, TopicId) =
            (blake3::hash(b"left").into(), blake3::hash(b"joined").into());
        let mut actor = t_actor().await?;
        actor.send_queue_size = 64 << 20;
        actor.handle_connection(old);
        push_many(&actor, peer_id, left, 3000);
        push_active(&actor, peer_id, disconnect_message(left));
        // Wait until flow control blocks the old writer.
        tokio::time::sleep(Duration::from_secs(1)).await;
        actor
            .peers
            .get_mut(&peer_id)
            .expect("known peer")
            .close_queue();
        let state = actor.peers.get_mut(&peer_id).expect("dropped");
        state.open_queue();
        let pushed = state.queue.push(join_message(joined, 0));
        assert!(matches!(pushed, Pushed::Queued));
        actor.handle_connection(new);

        let (topic, messages) = timeout(Duration::from_secs(10), read_next_stream(&peer_new))
            .await
            .std_context("the old queue did not go out")??;
        assert_eq!(topic, left, "the new queue went first");
        assert_eq!(messages.last(), Some(&TopicMessage::test_disconnect()));
        let (topic, _) = timeout(Duration::from_secs(5), read_next_message(&peer_new))
            .await
            .std_context("the new queue did not go out")??;
        assert_eq!(topic, joined);
        Ok(())
    }

    /// A peer that fails during a handover loses both connections at once.
    ///
    /// The old writer is stuck in a message and waits out the handover limit.
    /// The abort must not wait for it, nor for its streams to finish.
    #[tokio::test]
    #[traced_test]
    async fn aborted_peer_during_a_handover_loses_both_connections() -> Result {
        let pair = TestPair::new(1).await?;
        let (old, peer_old) = pair.connect_back().await?;
        let (new, peer_new) = pair.connect_back().await?;
        let peer_id = pair.ep2.id();
        let topic: TopicId = blake3::hash(b"failed handover").into();
        let mut actor = t_actor().await?;
        actor.send_queue_size = 64 << 20;
        actor.handle_connection(old);
        push_many(&actor, peer_id, topic, 3000);
        // Wait until flow control blocks the old writer.
        tokio::time::sleep(Duration::from_secs(1)).await;
        actor.handle_connection(new);

        actor
            .peers
            .get_mut(&peer_id)
            .expect("known peer")
            .abort(Instant::now());
        closed_with(&peer_old, util::CLOSE_QUEUE_FULL).await?;
        closed_with(&peer_new, util::CLOSE_QUEUE_FULL).await
    }

    /// A peer that we close and reopen keeps one byte limit for its old and new messages.
    #[tokio::test]
    #[traced_test]
    async fn closed_peer_keeps_its_byte_limit() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, _peer_conn) = pair.connect().await?;
        let peer_id = pair.ep2.id();
        let topic: TopicId = blake3::hash(b"one limit").into();
        let mut peers = HashMap::new();
        let mut state = open_peer(queue::MIN_QUEUE_SIZE);
        // Nothing takes from the queue, as from that of a peer that does not read.
        let _queue_lease = state
            .handle_connection(Side::Server, conn.stable_id())
            .expect("an accepted connection takes the lease");
        peers.insert(peer_id, state);
        let push = |peers: &mut HashMap<EndpointId, PeerState>| {
            let state = peers.get_mut(&peer_id).expect("known");
            state.queue.push(numbered(topic, 0))
        };
        while matches!(push(&mut peers), Pushed::Queued) {}

        peers.get_mut(&peer_id).expect("known peer").close_queue();
        peers.get_mut(&peer_id).expect("dropped").open_queue();
        assert!(
            matches!(push(&mut peers), Pushed::Dropped(_)),
            "the peer got a second limit"
        );
        Ok(())
    }

    /// A replaced connection that waits to open a stream hands the queue over at once.
    ///
    /// The peer allows one stream per connection, and the old connection used
    /// it, so its writer waits to open the stream of a second topic. Nothing
    /// of that stream went out, so the new connection takes the message at
    /// once, not after the open timeout.
    #[tokio::test]
    #[traced_test]
    async fn replaced_connection_waiting_to_open_hands_over_at_once() -> Result {
        let lookup = MemoryLookup::new();
        let ours = direct_endpoint(&lookup, QuicTransportConfig::builder().build()).await?;
        let one_stream = QuicTransportConfig::builder()
            .max_concurrent_uni_streams(VarInt::from_u32(1))
            .build();
        let theirs = direct_endpoint(&lookup, one_stream).await?;
        // The peer dials us, so the connection we keep is the server side and
        // takes the queue. The peer still limits the streams we open to it.
        let connect = || async {
            let dial = async {
                theirs
                    .connect(ours.id(), GOSSIP_ALPN)
                    .await
                    .std_context("connect")
            };
            let accept = async {
                let incoming = ours.accept().await.std_context("endpoint closed")?;
                incoming.await.std_context("accept")
            };
            (accept, dial).try_join().await
        };
        let (old, _peer_old) = connect().await?;
        let (new, peer_new) = connect().await?;
        let peer_id = theirs.id();
        let (first, second): (TopicId, TopicId) = (
            blake3::hash(b"first").into(),
            blake3::hash(b"second").into(),
        );
        let mut actor = t_actor().await?;
        actor.handle_connection(old);
        push_active(&actor, peer_id, join_message(first, 0));
        push_active(&actor, peer_id, join_message(second, 0));
        // Wait until the old writer opened the first stream and waits for the second.
        tokio::time::sleep(Duration::from_millis(500)).await;
        actor.handle_connection(new);

        let (topic, _) = timeout(Duration::from_secs(2), read_next_message(&peer_new))
            .await
            .std_context("the new connection waited for the open of the old one")??;
        assert_eq!(topic, second);
        Ok(())
    }

    /// A replaced connection keeps the message that it wrote, and new messages go on the new connection.
    #[tokio::test]
    #[traced_test]
    async fn replaced_connection_keeps_what_it_wrote() -> Result {
        let pair = TestPair::new(1).await?;
        let (old, peer_old) = pair.connect_back().await?;
        let (new, peer_new) = pair.connect_back().await?;
        let peer_id = pair.ep2.id();
        let (t_old, t_new): (TopicId, TopicId) =
            (blake3::hash(b"old").into(), blake3::hash(b"new").into());
        let mut actor = t_actor().await?;
        actor.handle_connection(old);
        push_active(&actor, peer_id, join_message(t_old, 0));
        let (topic, _) = timeout(Duration::from_secs(5), read_next_message(&peer_old))
            .await
            .std_context("the old connection sent nothing")??;
        assert_eq!(topic, t_old);

        actor.handle_connection(new);
        push_active(&actor, peer_id, join_message(t_new, 0));
        let (topic, _) = timeout(Duration::from_secs(5), read_next_message(&peer_new))
            .await
            .std_context("the new message did not go out")??;
        assert_eq!(topic, t_new);
        Ok(())
    }

    /// A connection from a peer we disconnected during a dial becomes its active one.
    ///
    /// The dial's queue goes out on it. After the queue, we drop only a connection
    /// we dialed, as one the peer opened is one it is about to use.
    #[tokio::test]
    #[traced_test]
    async fn inbound_during_disconnected_dial_is_kept() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect_back().await?;
        let peer_id = pair.ep2.id();
        let topic: TopicId = blake3::hash(b"inbound_during_disconnected_dial").into();
        let mut actor = t_actor().await?;
        actor
            .peers
            .insert(peer_id, pending_with(vec![join_message(topic, 0)]));
        actor
            .peers
            .get_mut(&peer_id)
            .expect("known peer")
            .close_queue();

        actor.handle_connection(conn);

        assert!(
            matches!(send_state(&actor, peer_id), Some((Some(_), false))),
            "the peer's connection was not kept"
        );
        let (_, message) = timeout(Duration::from_secs(5), read_next_message(&peer_conn))
            .await
            .std_context("the queue did not go out")??;
        assert!(!message.is_disconnect(), "the queued join is missing");
        Ok(())
    }

    /// A replaced connection stays open and delivers while we want the peer.
    ///
    /// Two peers that dial each other can each keep a different connection, so
    /// the peer may send on the one we replaced. Once we close the peer, both
    /// connections close after the grace, even with the peer's stream open.
    #[tokio::test]
    #[traced_test]
    async fn replaced_connection_stays_while_the_peer_is_wanted() -> Result {
        let pair = TestPair::new(1).await?;
        let (first, peer_first) = pair.connect_back().await?;
        let (second, peer_second) = pair.connect_back().await?;
        let peer_id = pair.ep2.id();
        let topic: TopicId = blake3::hash(b"replaced_connection_stays").into();
        let mut actor = t_actor().await?;
        actor.handle_connection(first);
        actor.handle_connection(second);
        set_wanted(&actor, peer_id, true);

        show_that_it_handles_a_close(&peer_second, topic).await;
        let mut stream = peer_first.open_uni().await.std_context("open stream")?;
        let mut buffer = Vec::new();
        util::StreamHeader::new(topic)
            .write(&mut stream, &mut buffer, TEST_FRAME_LIMIT)
            .await
            .std_context("write header")?;
        let join = join_message(topic, 0).message;
        util::write_frame(&mut stream, &join, &mut buffer, TEST_FRAME_LIMIT)
            .await
            .std_context("write frame")?;
        tokio::time::sleep(IDLE_GRACE + Duration::from_secs(1)).await;
        util::write_frame(&mut stream, &join, &mut buffer, TEST_FRAME_LIMIT)
            .await
            .std_context("write after the grace")?;
        for _ in 0..2 {
            let event = timeout(Duration::from_secs(5), actor.in_event_rx.recv())
                .await
                .std_context("a message did not arrive")?;
            assert!(
                matches!(event, Some(InEvent::RecvMessage(peer, _)) if peer == peer_id),
                "{event:?}"
            );
        }
        assert!(
            actor.connection_tasks.try_join_next().is_none(),
            "a connection closed"
        );
        assert!(
            peer_first.close_reason().is_none(),
            "the replaced connection closed"
        );

        actor
            .peers
            .get_mut(&peer_id)
            .expect("known peer")
            .close_queue();
        set_wanted(&actor, peer_id, false);
        for _ in 0..2 {
            let ended = timeout(
                IDLE_GRACE + Duration::from_secs(3),
                actor.connection_tasks.join_next(),
            )
            .await
            .std_context("a connection stayed open")?;
            let (peer, conn, res) = ended.expect("two tasks").expect("no panic");
            actor.handle_connection_task_finished(peer, conn, res);
        }
        timeout(Duration::from_secs(1), peer_first.closed())
            .await
            .std_context("the peer did not see the close")?;
        Ok(())
    }

    /// A peer we want again keeps the connection that outlived its old state.
    #[tokio::test]
    #[traced_test]
    async fn peer_wanted_again_keeps_its_connection() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect_back().await?;
        let peer_id = pair.ep2.id();
        let mut actor = t_actor().await?;
        actor.handle_connection(conn);
        set_wanted(&actor, peer_id, true);

        actor
            .peers
            .get_mut(&peer_id)
            .expect("known peer")
            .close_queue();
        set_wanted(&actor, peer_id, false);
        // The grace is running when we want the peer again.
        tokio::time::sleep(Duration::from_secs(2)).await;
        actor
            .peers
            .entry(peer_id)
            .or_insert_with(|| pending_with(vec![]));
        set_wanted(&actor, peer_id, true);

        tokio::time::sleep(IDLE_GRACE + Duration::from_secs(1)).await;
        assert!(
            actor.connection_tasks.try_join_next().is_none(),
            "the connection closed"
        );
        assert!(peer_conn.close_reason().is_none(), "the connection closed");
        Ok(())
    }

    /// A dial that completes after the peer's state is gone closes.
    ///
    /// The peer's state goes when its other connection closes or the protocol
    /// drops it. Nothing wants the dialed connection, so it closes after the
    /// grace, even if the peer opens a stream on it.
    #[tokio::test]
    #[traced_test]
    async fn dial_for_a_peer_without_state_closes() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let peer_id = pair.ep2.id();
        let topic: TopicId = blake3::hash(b"dial_without_state").into();
        let mut actor = t_actor().await?;

        actor.handle_connection(conn);
        let _stream = open_stream(&peer_conn, topic, &[join_message(topic, 0).message]).await?;

        assert!(is_idle(&actor.peers[&peer_id]), "the dial got a queue");
        let ended = timeout(
            IDLE_GRACE + Duration::from_secs(3),
            actor.connection_tasks.join_next(),
        )
        .await
        .std_context("the connection stayed open")?;
        let (peer, conn, res) = ended.expect("one task").expect("no panic");
        actor.handle_connection_task_finished(peer, conn, res);
        timeout(Duration::from_secs(1), peer_conn.closed())
            .await
            .std_context("the peer did not see the close")?;
        Ok(())
    }

    /// A late dial on which the peer never sends closes.
    ///
    /// Two peers that dial each other at once can each end up with a
    /// connection on which neither side sends. The peer showed on its other
    /// connection that it handles a close, and that connection ends before
    /// the dial runs.
    #[tokio::test]
    #[traced_test]
    async fn silent_late_dial_closes() -> Result {
        let pair = TestPair::new(1).await?;
        let (accepted, peer_accepted) = pair.connect_back().await?;
        let (dialed, peer_dialed) = pair.connect().await?;
        let peer_id = pair.ep2.id();
        let topic: TopicId = blake3::hash(b"silent_late_dial").into();
        let mut actor = t_actor().await?;

        actor.handle_connection(accepted);
        show_that_it_handles_a_close(&peer_accepted, topic).await;
        actor
            .peers
            .get_mut(&peer_id)
            .expect("known peer")
            .close_queue();
        let ended = timeout(
            IDLE_GRACE + Duration::from_secs(3),
            actor.connection_tasks.join_next(),
        )
        .await
        .std_context("the accepted connection stayed open")?;
        let (peer, conn, res) = ended.expect("one task").expect("no panic");

        // The test runtime runs the dial task only at the next await.
        actor.handle_connection(dialed);
        actor.handle_connection_task_finished(peer, conn, res);
        assert!(
            actor.peers.contains_key(&peer_id),
            "the flag went while the dial holds it"
        );

        let ended = timeout(
            IDLE_GRACE + Duration::from_secs(3),
            actor.connection_tasks.join_next(),
        )
        .await
        .std_context("the dial stayed open")?;
        let (peer, conn, res) = ended.expect("one task").expect("no panic");
        actor.handle_connection_task_finished(peer, conn, res);
        timeout(Duration::from_secs(1), peer_dialed.closed())
            .await
            .std_context("the peer did not see the close")?;
        actor.forget_peers(Instant::now() + PEER_RETENTION);
        assert!(
            !actor.peers.contains_key(&peer_id),
            "the flag outlived its time"
        );
        Ok(())
    }

    /// A silent late dial closes, also when the other connection of the peer ended before it.
    ///
    /// The flag of the peer outlives its last connection for a while.
    #[tokio::test]
    #[traced_test]
    async fn silent_dial_after_the_last_connection_closes() -> Result {
        let pair = TestPair::new(1).await?;
        let (accepted, peer_accepted) = pair.connect_back().await?;
        let (dialed, peer_dialed) = pair.connect().await?;
        let peer_id = pair.ep2.id();
        let topic: TopicId = blake3::hash(b"silent_dial_after_the_last").into();
        let mut actor = t_actor().await?;

        actor.handle_connection(accepted);
        show_that_it_handles_a_close(&peer_accepted, topic).await;
        actor
            .peers
            .get_mut(&peer_id)
            .expect("known peer")
            .close_queue();
        let ended = timeout(
            IDLE_GRACE + Duration::from_secs(3),
            actor.connection_tasks.join_next(),
        )
        .await
        .std_context("the accepted connection stayed open")?;
        let (peer, conn, res) = ended.expect("one task").expect("no panic");
        actor.handle_connection_task_finished(peer, conn, res);
        assert!(
            actor.peers.contains_key(&peer_id),
            "the flag went with the last connection"
        );

        actor.handle_connection(dialed);
        let ended = timeout(
            IDLE_GRACE + Duration::from_secs(3),
            actor.connection_tasks.join_next(),
        )
        .await
        .std_context("the dial stayed open")?;
        let (peer, conn, res) = ended.expect("one task").expect("no panic");
        actor.handle_connection_task_finished(peer, conn, res);
        timeout(Duration::from_secs(1), peer_dialed.closed())
            .await
            .std_context("the peer did not see the close")?;
        Ok(())
    }

    /// An idle peer expires after retention even if it never set handles_close.
    #[tokio::test]
    #[traced_test]
    async fn peer_with_unset_close_flag_expires() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect_back().await?;
        let peer_id = pair.ep2.id();
        let mut actor = t_actor().await?;

        actor.handle_connection(conn);
        peer_conn.close(0u32.into(), b"done");
        let ended = timeout(Duration::from_secs(3), actor.connection_tasks.join_next())
            .await
            .std_context("the connection stayed open")?;
        let (peer, conn, res) = ended.expect("one task").expect("no panic");
        actor.handle_connection_task_finished(peer, conn, res);
        assert!(actor.peers.contains_key(&peer_id), "retain the idle peer");
        actor.forget_peers(Instant::now() + PEER_RETENTION);
        assert!(
            !actor.peers.contains_key(&peer_id),
            "the idle peer did not expire"
        );
        Ok(())
    }

    /// A late dial stays open if we want the peer again within the grace.
    ///
    /// The peer may take the dial as its active connection. Closing it would
    /// then drop us on the side of the peer.
    #[tokio::test]
    #[traced_test]
    async fn late_dial_of_a_peer_wanted_again_stays() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let peer_id = pair.ep2.id();
        let mut actor = t_actor().await?;

        actor.handle_connection(conn);
        assert!(is_idle(&actor.peers[&peer_id]), "the dial got a queue");
        set_wanted(&actor, peer_id, true);

        tokio::time::sleep(IDLE_GRACE + Duration::from_secs(1)).await;
        assert!(
            actor.connection_tasks.try_join_next().is_none(),
            "the connection closed"
        );
        assert!(peer_conn.close_reason().is_none(), "the connection closed");
        Ok(())
    }

    /// A dial that the peer's connection overtook after a disconnect does not replace it.
    ///
    /// The peer's connection took the queue and is the active one. The dial
    /// stays while we want the peer, as the peer may send on it, and closes
    /// once we do not.
    #[tokio::test]
    #[traced_test]
    async fn overtaken_dial_of_a_closed_peer_follows_the_flag() -> Result {
        let pair = TestPair::new(1).await?;
        let (dialed, dialed_theirs) = pair.connect().await?;
        let (inbound, _inbound_theirs) = pair.connect_back().await?;
        let peer_id = pair.ep2.id();
        let topic: TopicId = blake3::hash(b"overtaken_dial").into();
        let mut actor = t_actor().await?;
        actor.peers.insert(peer_id, pending_with(vec![]));
        actor
            .peers
            .get_mut(&peer_id)
            .expect("known peer")
            .close_queue();

        let inbound_id = inbound.stable_id();
        actor.handle_connection(inbound);
        actor.handle_connection(dialed);
        set_wanted(&actor, peer_id, true);
        let _stream = open_stream(&dialed_theirs, topic, &[join_message(topic, 0).message]).await?;

        assert!(
            matches!(
                send_state(&actor, peer_id),
                Some((Some(writer), false)) if writer == inbound_id
            ),
            "the dial replaced the peer's connection"
        );
        tokio::time::sleep(IDLE_GRACE + Duration::from_secs(1)).await;
        assert!(
            dialed_theirs.close_reason().is_none(),
            "the dial closed while we want the peer"
        );

        actor
            .peers
            .get_mut(&peer_id)
            .expect("known peer")
            .close_queue();
        set_wanted(&actor, peer_id, false);
        timeout(IDLE_GRACE + Duration::from_secs(3), dialed_theirs.closed())
            .await
            .std_context("the dial stayed open")?;
        Ok(())
    }

    /// A dial for a disconnected peer delivers the queue, then closes.
    #[tokio::test]
    #[traced_test]
    async fn disconnected_pending_dial_flushes_then_closes() -> Result {
        let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
        let ct = CancellationToken::new();
        let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
        let (_go1, actor, ep1_handle) =
            Gossip::t_new_with_actor(rng, Default::default(), relay_map.clone(), &ct).await?;
        let _ep1_handle = AbortOnDropHandle::new(ep1_handle);
        let mut actor = ManualActorLoop::new(actor).await;
        let topic: TopicId = blake3::hash(b"pending_dial_flushes").into();

        // The peer reads one message, then waits for us to close.
        let ep2 = create_endpoint(rng, relay_map, None).await?;
        let ep2_id = ep2.id();
        let ep2_addr = EndpointAddr::new(ep2_id).with_relay_url(relay_url);
        let peer = spawn(async move {
            let incoming = ep2.accept().await.expect("endpoint closed");
            let conn = incoming.await.expect("accept failed");
            let mut stream = conn.accept_uni().await.expect("accept_uni");
            let mut buf = bytes::BytesMut::new();
            let header = util::StreamHeader::read(&mut stream, &mut buf, 1024)
                .await
                .expect("header");
            let message: Option<proto::topic::Message<PublicKey>> =
                util::read_frame(&mut stream, &mut buf, 1024)
                    .await
                    .expect("frame");
            show_that_it_handles_a_close(&conn, header.topic_id).await;
            conn.closed().await;
            (header.topic_id, message.is_some())
        });
        let peer = AbortOnDropHandle::new(peer);

        let conn = actor
            .endpoint
            .connect(ep2_addr, GOSSIP_ALPN)
            .await
            .std_context("connect")?;
        actor
            .peers
            .insert(ep2_id, pending_with(vec![join_message(topic, 0)]));
        actor
            .peers
            .get_mut(&ep2_id)
            .expect("known peer")
            .close_queue();
        actor.handle_connection(conn);
        assert!(
            matches!(send_state(&actor, ep2_id), Some((Some(_), true))),
            "the connection became the peer's active one"
        );

        let mut peer = peer;
        let delivered = timeout(IDLE_GRACE * 2 + Duration::from_secs(3), async {
            loop {
                tokio::select! {
                    _ = actor.step() => {}
                    res = &mut peer => return res.expect("peer task"),
                }
            }
        })
        .await
        .std_context("the connection was not closed")?;
        assert_eq!(
            delivered,
            (topic, true),
            "the queued message did not arrive"
        );
        Ok(())
    }

    /// A `Disconnect` ends its stream, and a rejoin goes on a new stream.
    ///
    /// The ended stream frees a stream for the peer's limit on open streams.
    #[tokio::test]
    #[traced_test]
    async fn disconnect_ends_its_stream() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let topic: TopicId = blake3::hash(b"disconnect_ends_its_stream").into();
        let messages = vec![
            join_message(topic, 0),
            disconnect_message(topic),
            join_message(topic, 0),
        ];
        let mut queue = SendQueue::new(queue::MIN_QUEUE_SIZE);
        for message in messages {
            assert!(matches!(queue.push(message), Pushed::Queued));
        }
        let send_loop = SendLoop::new(conn, queue.lease(), 1024);
        drop(queue);
        let _send = AbortOnDropHandle::new(spawn(async move { send_loop.run().await }));

        let (_, first) = timeout(Duration::from_secs(5), read_next_stream(&peer_conn))
            .await
            .std_context("the first stream did not end")??;
        let (_, second) = timeout(Duration::from_secs(5), read_next_stream(&peer_conn))
            .await
            .std_context("the second stream did not end")??;

        let first: Vec<bool> = first.iter().map(|m| m.is_disconnect()).collect();
        let second: Vec<bool> = second.iter().map(|m| m.is_disconnect()).collect();
        assert_eq!(first, vec![false, true], "first stream, by `is_disconnect`");
        assert_eq!(second, vec![false], "second stream, by `is_disconnect`");
        Ok(())
    }

    /// Returns the writer of the queue of `peer_id`, and whether the queue is draining.
    fn send_state(actor: &Actor, peer_id: EndpointId) -> Option<(Option<ConnId>, bool)> {
        let peer = actor.peers.get(&peer_id)?;
        Some((peer.writer, peer.queue.is_closing()))
    }

    /// Whether the peer has no messages and its queue is closed.
    fn is_idle(peer: &PeerState) -> bool {
        peer.queue.is_closing() && peer.queue.is_empty()
    }

    /// Returns the state of a peer with an open, empty queue of `size` bytes.
    fn open_peer(size: usize) -> PeerState {
        let mut peer = PeerState::new(size);
        peer.open_queue();
        peer
    }

    /// Returns the state of a peer that we dial, with `messages` in its queue.
    fn pending_with(messages: Vec<ProtoMessage>) -> PeerState {
        let state = open_peer(queue::QUEUE_SIZE);
        for message in messages {
            assert!(matches!(state.queue.push(message), Pushed::Queued));
        }
        state
    }

    /// Returns a join for `topic` carrying `data_len` bytes of peer data.
    ///
    /// The protocol state builds it, as the message types are private to `proto`.
    fn join_message(topic: TopicId, data_len: usize) -> ProtoMessage {
        let me = SecretKey::generate().public();
        let peer = SecretKey::generate().public();
        let mut state = proto::State::new(
            me,
            PeerData::new(vec![0u8; data_len]),
            Default::default(),
            StdRng::seed_from_u64(1),
        );
        let join = InEvent::Command(topic, proto::Command::Join(vec![peer]));
        let message = state
            .handle(join, Instant::now(), None)
            .find_map(|event| match event {
                proto::OutEvent::SendMessage(_, message) => Some(message),
                _ => None,
            });
        message.expect("a join sends a message")
    }

    /// A stream that the peer stops is dropped, and the connection stays.
    ///
    /// The peer stops a stream that it cannot read, for example a frame over its
    /// size limit. The next message of the topic goes out on a new stream.
    #[tokio::test]
    #[traced_test]
    async fn stopped_stream_keeps_the_connection() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let peer_id = pair.ep2.id();
        let topic: TopicId = blake3::hash(b"stopped_stream").into();

        // The peer stops the first stream we open, and reads the next one.
        let peer = AbortOnDropHandle::new(spawn(async move {
            let mut first = peer_conn.accept_uni().await.expect("accept_uni");
            first.stop(0u32.into()).expect("stop");
            let mut next = peer_conn.accept_uni().await.expect("accept_uni");
            let mut buffer = BytesMut::new();
            let header = util::StreamHeader::read(&mut next, &mut buffer, TEST_FRAME_LIMIT)
                .await
                .expect("read header");
            let message: Option<TopicMessage> =
                util::read_frame(&mut next, &mut buffer, TEST_FRAME_LIMIT)
                    .await
                    .expect("read frame");
            (header.topic_id, message.is_some(), peer_conn)
        }));
        let mut queue = SendQueue::new(queue::MIN_QUEUE_SIZE);
        let queue_lease = queue.lease();
        let aborted = CancellationToken::new();
        let _feed = AbortOnDropHandle::new(spawn(async move {
            loop {
                queue.push(join_message(topic, 0));
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }));
        let (in_event_tx, _in_event_rx) = mpsc::channel(16);
        let _conn_loop = AbortOnDropHandle::new(spawn(connection_loop(
            peer_id,
            conn,
            aborted,
            Some(queue_lease),
            in_event_tx,
            TEST_FRAME_LIMIT,
            watch::channel(true).1,
            close_flag(),
        )));

        let (read_topic, read_message, peer_conn) = timeout(Duration::from_secs(5), peer)
            .await
            .std_context("no message on a new stream")?
            .std_context("peer task")?;
        assert_eq!(read_topic, topic);
        assert!(read_message, "the new stream had no message");
        assert!(peer_conn.close_reason().is_none(), "the connection closed");
        Ok(())
    }

    /// A receive loop that ended ends the connection loop despite a live send loop.
    ///
    /// The receive loop ends once the actor takes no more messages. A bad
    /// stream header does not end it: it drops only its stream.
    #[tokio::test]
    #[traced_test]
    async fn ended_receive_loop_ends_connection_loop() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let topic: TopicId = blake3::hash(b"ended_receive_loop").into();
        let join = join_message(topic, 0).message;
        let _stream = open_stream(&peer_conn, topic, &[join]).await?;

        // The queue stays alive, so only the receive loop can end the connection.
        let mut queue = SendQueue::new(queue::MIN_QUEUE_SIZE);
        let (in_event_tx, in_event_rx) = mpsc::channel(16);
        drop(in_event_rx);
        timeout(
            Duration::from_secs(5),
            connection_loop(
                pair.ep2.id(),
                conn,
                CancellationToken::new(),
                Some(queue.lease()),
                in_event_tx,
                TEST_FRAME_LIMIT,
                watch::channel(true).1,
                close_flag(),
            ),
        )
        .await
        .std_context("connection loop still runs after the receive loop ended")??;
        Ok(())
    }

    /// The connection a shuffle reply opened closes once the origin reads the reply.
    ///
    /// The replying node is usually not a neighbor of the shuffle's origin. The
    /// origin drops it after it reads the reply. That does not close the
    /// connection while the reply stream is open, so the replying node has to
    /// drop the origin as well.
    #[tokio::test]
    #[traced_test]
    async fn shuffle_reply_connection_closes() -> Result {
        let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
        let ct = CancellationToken::new();
        let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
        let memory_lookup = MemoryLookup::new();
        let (_go1, actor, ep1_handle) =
            Gossip::t_new_with_actor(rng, Default::default(), relay_map.clone(), &ct).await?;
        let _ep1_handle = AbortOnDropHandle::new(ep1_handle);
        let mut actor = ManualActorLoop::new(actor).await;
        actor
            .endpoint
            .address_lookup()
            .expect("endpoint is not closed")
            .add(memory_lookup.clone());
        let ep1_addr = EndpointAddr::new(actor.endpoint.id()).with_relay_url(relay_url.clone());
        let topic: TopicId = blake3::hash(b"shuffle_reply_closes").into();
        actor.handle_in_event(
            InEvent::Command(topic, proto::Command::Join(vec![])),
            Instant::now(),
        );

        // The origin of the shuffle reads the reply, then waits for the close.
        let origin = create_endpoint(rng, relay_map.clone(), None).await?;
        let origin_id = origin.id();
        memory_lookup.add_endpoint_info(EndpointAddr::new(origin_id).with_relay_url(relay_url));
        let mut origin_task = AbortOnDropHandle::new(spawn(async move {
            let incoming = origin.accept().await.expect("endpoint closed");
            let conn = incoming.await.expect("accept failed");
            let mut stream = conn.accept_uni().await.expect("accept_uni");
            let mut buf = bytes::BytesMut::new();
            let header = util::StreamHeader::read(&mut stream, &mut buf, 1024)
                .await
                .expect("header");
            let reply: Option<proto::topic::Message<PublicKey>> =
                util::read_frame(&mut stream, &mut buf, 1024)
                    .await
                    .expect("frame");
            show_that_it_handles_a_close(&conn, header.topic_id).await;
            conn.closed().await;
            reply.is_some()
        }));

        // A third node forwards the shuffle to us with its walk at an end.
        let ep3 = create_endpoint(rng, relay_map, None).await?;
        let conn = ep3
            .connect(ep1_addr, GOSSIP_ALPN)
            .await
            .std_context("connect")?;
        let mut stream = conn.open_uni().await.std_context("open_uni")?;
        let mut buf = Vec::new();
        util::StreamHeader::new(topic)
            .write(&mut stream, &mut buf, 1024)
            .await
            .std_context("header")?;
        let shuffle = proto::topic::Message::test_shuffle(origin_id, 0);
        util::write_frame(&mut stream, &shuffle, &mut buf, 1024)
            .await
            .std_context("frame")?;

        let replied = timeout(IDLE_GRACE * 2 + Duration::from_secs(5), async {
            loop {
                tokio::select! {
                    _ = actor.step() => {}
                    res = &mut origin_task => return res.expect("origin task"),
                }
            }
        })
        .await
        .std_context("the reply connection was not closed")?;
        assert!(replied, "the shuffle reply did not arrive");
        assert!(
            actor.peers.get(&origin_id).is_none_or(is_idle),
            "the origin's queue was kept"
        );
        Ok(())
    }

    /// A stream with a header that does not decode does not end the other streams.
    #[tokio::test]
    #[traced_test]
    async fn bad_header_drops_only_its_stream() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let (in_event_tx, mut in_event_rx) = mpsc::channel(4);
        let mut recv_loop =
            RecvLoop::new(peer_conn.remote_id(), conn, in_event_tx, 1024, close_flag());
        let _recv = AbortOnDropHandle::new(spawn(async move { recv_loop.run().await }));

        // A complete frame of three bytes, too short for a topic id.
        let mut bad = peer_conn.open_uni().await.std_context("open stream")?;
        bad.write_all(&[0, 0, 0, 3, 1, 2, 3])
            .await
            .std_context("write bad header")?;
        bad.finish().std_context("finish")?;
        let topic: TopicId = [1; 32].into();
        let _stream = open_stream(&peer_conn, topic, &[join_message(topic, 0).message]).await?;

        let event = timeout(Duration::from_secs(2), in_event_rx.recv())
            .await
            .std_context("the bad header ended the receive loop")?;
        assert!(matches!(event, Some(InEvent::RecvMessage(..))), "{event:?}");
        Ok(())
    }

    /// A stream whose header was incomplete at the close still delivers its message.
    ///
    /// The loop picks its next branch at random, so a single round can miss the bug.
    #[tokio::test]
    #[traced_test]
    async fn pending_header_is_read_after_a_close() -> Result {
        for round in 0..4 {
            let pair = TestPair::new(1).await?;
            let (conn, peer_conn) = pair.connect().await?;
            let (in_event_tx, mut in_event_rx) = mpsc::channel(2);
            let mut recv_loop = RecvLoop::new(
                peer_conn.remote_id(),
                conn.clone(),
                in_event_tx,
                1024,
                close_flag(),
            );
            // We poll it by hand, so that it sees the rest of the header only after the close.
            let run = recv_loop.run();
            tokio::pin!(run);

            let topic: TopicId = [1; 32].into();
            let mut partial = peer_conn.open_uni().await.std_context("open stream")?;
            partial
                .write_all(&[0, 0])
                .await
                .std_context("write partial header")?;
            // The loop accepts streams in order, so this message means it has the partial one.
            let mut stream =
                open_stream(&peer_conn, topic, &[join_message(topic, 0).message]).await?;
            stream.finish().std_context("finish")?;
            tokio::select! {
                res = &mut run => panic!("the recv loop ended: {res:?}"),
                event = in_event_rx.recv() => {
                    assert!(matches!(event, Some(InEvent::RecvMessage(..))), "{event:?}");
                }
            }
            // Let the loop see the end of the full stream, so that only the header remains.
            stream.stopped().await.std_context("stopped")?;
            let res = n0_future::future::poll_once(&mut run).await;
            assert!(res.is_none(), "the recv loop ended: {res:?}");

            partial
                .write_all(&[0, 32])
                .await
                .std_context("write header length")?;
            partial
                .write_all(&[1; 32])
                .await
                .std_context("write topic")?;
            let mut buffer = Vec::new();
            util::write_frame(
                &mut partial,
                &join_message(topic, 0).message,
                &mut buffer,
                1024,
            )
            .await
            .std_context("write frame")?;
            partial.finish().std_context("finish")?;
            partial.stopped().await.std_context("stopped")?;
            peer_conn.close(0u32.into(), b"done");
            conn.closed().await;

            timeout(Duration::from_secs(2), &mut run)
                .await
                .std_context("the recv loop did not end")?;
            let event = in_event_rx.try_recv();
            assert!(
                matches!(event, Ok(InEvent::RecvMessage(..))),
                "round {round}: {event:?}"
            );
        }
        Ok(())
    }

    /// A peer with too many partial headers waits until one of them is complete.
    #[tokio::test]
    #[traced_test]
    async fn partial_headers_are_limited() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let (in_event_tx, mut in_event_rx) = mpsc::channel(4);
        let mut recv_loop =
            RecvLoop::new(peer_conn.remote_id(), conn, in_event_tx, 1024, close_flag());
        let _recv = AbortOnDropHandle::new(spawn(async move { recv_loop.run().await }));

        let mut partials = Vec::new();
        for _ in 0..util::MAX_PENDING_HEADERS {
            let mut stream = peer_conn.open_uni().await.std_context("open stream")?;
            stream
                .write_all(&[0, 0])
                .await
                .std_context("write partial header")?;
            partials.push(stream);
        }
        let topic: TopicId = [1; 32].into();
        let _stream = open_stream(&peer_conn, topic, &[join_message(topic, 0).message]).await?;
        let event = timeout(Duration::from_millis(500), in_event_rx.recv()).await;
        assert!(event.is_err(), "the loop read more headers than its limit");

        let partial = &mut partials[0];
        partial
            .write_all(&[0, 32])
            .await
            .std_context("write header length")?;
        partial
            .write_all(&[1; 32])
            .await
            .std_context("write topic")?;
        let event = timeout(Duration::from_secs(2), in_event_rx.recv())
            .await
            .std_context("a complete header did not free its place")?;
        assert!(matches!(event, Some(InEvent::RecvMessage(..))), "{event:?}");
        Ok(())
    }

    /// A close handles the messages queued before it, not those that arrive meanwhile.
    ///
    /// Other connections can refill the queue as fast as the actor handles it.
    #[tokio::test(flavor = "multi_thread")]
    #[traced_test]
    async fn close_does_not_wait_for_an_empty_queue() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, _peer_conn) = pair.connect_back().await?;
        let (mut actor, _rpc_tx, _local_tx) = Actor::new(
            pair.ep1.clone(),
            Default::default(),
            Default::default(),
            None,
            Default::default(),
        );
        conn.close(0u32.into(), b"done");
        actor.handle_connection(conn);
        let (peer, conn, result) = actor
            .connection_tasks
            .join_next()
            .await
            .expect("one task")
            .expect("no panic");

        // Another connection: it sends ten full queues, as fast as the actor takes them.
        let in_event_tx = actor.in_event_tx.clone();
        let other = SecretKey::generate().public();
        let producer = AbortOnDropHandle::new(spawn(async move {
            for _ in 0..10 * IN_EVENT_CAP {
                in_event_tx
                    .send(InEvent::PeerDisconnected(other))
                    .await
                    .expect("actor alive");
            }
        }));
        while actor.in_event_rx.len() < IN_EVENT_CAP {
            tokio::task::yield_now().await;
        }
        actor.handle_connection_task_finished(peer, conn, result);
        assert!(
            !producer.is_finished(),
            "the close waited for an empty queue"
        );
        Ok(())
    }

    /// `broadcast` refuses a message that does not fit a frame at every hop.
    #[tokio::test]
    #[traced_test]
    async fn broadcast_refuses_an_oversized_message() -> Result {
        const MAX: usize = 1024;
        let endpoint = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .bind()
            .await?;
        let gossip = Gossip::builder().max_message_size(MAX).spawn(endpoint);
        let topic = gossip.subscribe([1; 32].into(), vec![]).await?;
        let (sender, _receiver) = topic.split();
        // The limit is the largest content that fits a frame at every hop.
        let fits = (0..MAX)
            .rev()
            .find(|len| proto::topic::broadcast_fits(*len, Scope::Swarm, MAX))
            .expect("some content fits");
        assert_eq!(sender.max_content_size(), fits);

        sender.broadcast(vec![0; fits].into()).await?;
        let refused = sender.broadcast(vec![0; fits + 1].into()).await;
        assert!(
            matches!(refused, Err(ApiError::TooLarge { size, max_content, .. }) if size == fits + 1 && max_content == fits),
            "{refused:?}"
        );
        sender.broadcast_neighbors(vec![0; fits].into()).await?;
        let refused = sender.broadcast_neighbors(vec![0; fits + 1].into()).await;
        assert!(
            matches!(refused, Err(ApiError::TooLarge { .. })),
            "{refused:?}"
        );
        Ok(())
    }

    /// Test that endpoints can reconnect to each other.
    ///
    /// This test will create two endpoints subscribed to the same topic. The second endpoint will
    /// unsubscribe and then resubscribe and connection between the endpoints should succeed both
    /// times.
    // NOTE: This is a regression test
    #[tokio::test]
    #[traced_test]
    async fn can_reconnect() -> Result {
        let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
        let ct = CancellationToken::new();
        let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();

        let (go1, ep1, ep1_handle, _test_actor_handle1) =
            Gossip::t_new(rng, Default::default(), relay_map.clone(), &ct).await?;

        let (go2, ep2, ep2_handle, _test_actor_handle2) =
            Gossip::t_new(rng, Default::default(), relay_map, &ct).await?;

        let endpoint_id1 = ep1.id();
        let endpoint_id2 = ep2.id();
        tracing::info!(
            endpoint_1 = %endpoint_id1.fmt_short(),
            endpoint_2 = %endpoint_id2.fmt_short(),
            "endpoints ready"
        );

        let topic: TopicId = blake3::hash(b"can_reconnect").into();
        tracing::info!(%topic, "joining");

        let ct2 = ct.child_token();
        // channel used to signal the second gossip instance to advance the test
        let (tx, mut rx) = mpsc::channel::<()>(1);
        let addr1 = EndpointAddr::new(endpoint_id1).with_relay_url(relay_url.clone());
        let memory_lookup = MemoryLookup::new();
        memory_lookup.add_endpoint_info(addr1);
        ep2.address_lookup()?.add(memory_lookup.clone());
        let go2_task = async move {
            let mut sub = go2.subscribe(topic, Vec::new()).await?;
            sub.joined().await?;

            rx.recv().await.expect("signal to unsubscribe");
            tracing::info!("unsubscribing");
            drop(sub);

            rx.recv().await.expect("signal to subscribe again");
            tracing::info!("resubscribing");
            let mut sub = go2.subscribe(topic, vec![endpoint_id1]).await?;

            sub.joined().await?;
            tracing::info!("subscription successful!");

            ct2.cancelled().await;

            Ok::<_, ApiError>(())
        }
        .instrument(tracing::debug_span!("endpoint_2", %endpoint_id2));
        let go2_handle = task::spawn(go2_task);

        let addr2 = EndpointAddr::new(endpoint_id2).with_relay_url(relay_url);
        memory_lookup.add_endpoint_info(addr2);
        ep1.address_lookup()?.add(memory_lookup);

        let mut sub = go1.subscribe(topic, vec![endpoint_id2]).await?;
        // wait for subscribed notification
        sub.joined().await?;

        // signal endpoint_2 to unsubscribe
        tx.send(()).await.std_context("signal unsubscribe")?;

        // we should receive a Neighbor down event
        let conn_timeout = Duration::from_millis(500);
        let ev = timeout(conn_timeout, sub.try_next())
            .await
            .std_context("wait neighbor down")??;
        assert_eq!(ev, Some(Event::NeighborDown(endpoint_id2)));
        tracing::info!("endpoint 2 left");

        // signal endpoint_2 to subscribe again
        tx.send(()).await.std_context("signal resubscribe")?;

        let conn_timeout = Duration::from_millis(500);
        let ev = timeout(conn_timeout, sub.try_next())
            .await
            .std_context("wait neighbor up")??;
        assert_eq!(ev, Some(Event::NeighborUp(endpoint_id2)));
        tracing::info!("endpoint 2 rejoined!");

        // cleanup and ensure everything went as expected
        ct.cancel();
        let wait = Duration::from_secs(2);
        timeout(wait, ep1_handle)
            .await
            .std_context("wait endpoint1 task")?
            .std_context("join endpoint1 task")??;
        timeout(wait, ep2_handle)
            .await
            .std_context("wait endpoint2 task")?
            .std_context("join endpoint2 task")??;
        timeout(wait, go2_handle)
            .await
            .std_context("wait gossip2 task")?
            .std_context("join gossip2 task")??;

        Result::Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn can_die_and_reconnect() -> Result {
        /// Runs a future in a separate runtime on a separate thread, cancelling everything
        /// abruptly once `cancel` is invoked.
        fn run_in_thread<T: Send + 'static>(
            cancel: CancellationToken,
            fut: impl std::future::Future<Output = T> + Send + 'static,
        ) -> std::thread::JoinHandle<Option<T>> {
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                rt.block_on(async move { cancel.run_until_cancelled(fut).await })
            })
        }

        /// Spawns a new endpoint and gossip instance.
        async fn spawn_gossip(
            secret_key: SecretKey,
            relay_map: RelayMap,
        ) -> Result<(Router, Gossip), BindError> {
            let ep = Endpoint::builder(presets::Minimal)
                .relay_mode(RelayMode::Custom(relay_map))
                .secret_key(secret_key)
                .ca_tls_config(CaTlsConfig::insecure_skip_verify())
                .bind()
                .await?;
            let gossip = Gossip::builder().spawn(ep.clone());
            let router = Router::builder(ep)
                .accept(GOSSIP_ALPN, gossip.clone())
                .spawn();
            Ok((router, gossip))
        }

        /// Spawns a gossip endpoint, and broadcasts a single message, then sleep until cancelled externally.
        async fn broadcast_once(
            secret_key: SecretKey,
            relay_map: RelayMap,
            bootstrap_addr: EndpointAddr,
            topic_id: TopicId,
            message: String,
        ) -> Result {
            let (router, gossip) = spawn_gossip(secret_key, relay_map).await?;
            info!(endpoint_id = %router.endpoint().id().fmt_short(), "broadcast endpoint spawned");
            let bootstrap = vec![bootstrap_addr.id];
            let memory_lookup = MemoryLookup::new();
            memory_lookup.add_endpoint_info(bootstrap_addr);
            router.endpoint().address_lookup()?.add(memory_lookup);
            let mut topic = gossip.subscribe_and_join(topic_id, bootstrap).await?;
            topic.broadcast(message.as_bytes().to_vec().into()).await?;
            std::future::pending::<()>().await;
            Ok(())
        }

        let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
        let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
        let topic_id = TopicId::from_bytes(rng.random());

        // spawn a gossip endpoint, send the endpoint's address on addr_tx,
        // then wait to receive `count` messages, and terminate.
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();
        let (msgs_recv_tx, mut msgs_recv_rx) = tokio::sync::mpsc::channel(3);
        let recv_task = tokio::task::spawn({
            let relay_map = relay_map.clone();
            let secret_key = SecretKey::from_bytes(&rng.random());
            async move {
                let (router, gossip) = spawn_gossip(secret_key, relay_map).await?;
                // wait for the relay to be set. iroh currently has issues when trying
                // to immediately reconnect with changed direct addresses, but when the
                // relay path is available it works.
                // See https://github.com/n0-computer/iroh/pull/3372
                router.endpoint().online().await;
                let addr = router.endpoint().addr();
                info!(endpoint_id = %addr.id.fmt_short(), "recv endpoint spawned");
                addr_tx.send(addr).unwrap();
                let mut topic = gossip.subscribe_and_join(topic_id, vec![]).await?;
                while let Some(event) = topic.try_next().await.unwrap() {
                    if let Event::Received(message) = event {
                        let message = std::str::from_utf8(&message.content)
                            .std_context("decode broadcast message")?
                            .to_string();
                        msgs_recv_tx
                            .send(message)
                            .await
                            .std_context("forward received message")?;
                    }
                }
                Ok::<_, AnyError>(())
            }
        });

        let endpoint0_addr = addr_rx.await.std_context("receive endpoint address")?;
        let max_wait = Duration::from_secs(5);

        // spawn a endpoint, send a message, and then abruptly terminate the endpoint ungracefully
        // after the message was received on our receiver endpoint.
        let cancel = CancellationToken::new();
        let secret = SecretKey::from_bytes(&rng.random());
        let join_handle_1 = run_in_thread(
            cancel.clone(),
            broadcast_once(
                secret.clone(),
                relay_map.clone(),
                endpoint0_addr.clone(),
                topic_id,
                "msg1".to_string(),
            ),
        );
        // assert that we received the message on the receiver endpoint.
        let msg = timeout(max_wait, msgs_recv_rx.recv())
            .await
            .std_context("wait for first broadcast")?
            .std_context("receiver dropped channel")?;
        assert_eq!(&msg, "msg1");
        info!("kill broadcast endpoint");
        cancel.cancel();

        // spawns the endpoint again with the same endpoint id, and send another message
        let cancel = CancellationToken::new();
        let join_handle_2 = run_in_thread(
            cancel.clone(),
            broadcast_once(
                secret.clone(),
                relay_map.clone(),
                endpoint0_addr.clone(),
                topic_id,
                "msg2".to_string(),
            ),
        );
        // assert that we received the message on the receiver endpoint.
        // this means that the reconnect with the same endpoint id worked.
        let msg = timeout(max_wait, msgs_recv_rx.recv())
            .await
            .std_context("wait for second broadcast")?
            .std_context("receiver dropped channel")?;
        assert_eq!(&msg, "msg2");
        info!("kill broadcast endpoint");
        cancel.cancel();

        info!("kill recv endpoint");
        recv_task.abort();
        assert!(join_handle_1.join().unwrap().is_none());
        assert!(join_handle_2.join().unwrap().is_none());

        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn gossip_change_alpn() -> n0_error::Result<()> {
        let alpn = b"my-gossip-alpn";
        let topic_id = TopicId::from([0u8; 32]);

        let ep1 = Endpoint::bind(presets::Minimal).await?;
        let ep2 = Endpoint::bind(presets::Minimal).await?;
        let gossip1 = Gossip::builder().alpn(alpn).spawn(ep1.clone());
        let gossip2 = Gossip::builder().alpn(alpn).spawn(ep2.clone());
        let router1 = Router::builder(ep1).accept(alpn, gossip1.clone()).spawn();
        let router2 = Router::builder(ep2).accept(alpn, gossip2.clone()).spawn();

        let addr1 = router1.endpoint().addr();
        let id1 = addr1.id;
        let memory_lookup = MemoryLookup::new();
        memory_lookup.add_endpoint_info(addr1);
        router2.endpoint().address_lookup()?.add(memory_lookup);

        let mut topic1 = gossip1.subscribe(topic_id, vec![]).await?;
        let mut topic2 = gossip2.subscribe(topic_id, vec![id1]).await?;

        timeout(Duration::from_secs(3), topic1.joined())
            .await
            .std_context("wait topic1 join")??;
        timeout(Duration::from_secs(3), topic2.joined())
            .await
            .std_context("wait topic2 join")??;
        router1.shutdown().await.std_context("shutdown router1")?;
        router2.shutdown().await.std_context("shutdown router2")?;
        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn gossip_rely_on_gossip_address_lookup() -> n0_error::Result<()> {
        let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);

        async fn spawn(
            rng: &mut impl CryptoRng,
        ) -> n0_error::Result<(EndpointId, Router, Gossip, GossipSender, GossipReceiver)> {
            let topic_id = TopicId::from([0u8; 32]);
            let ep = Endpoint::builder(presets::Minimal)
                .secret_key(SecretKey::from_bytes(&rng.random()))
                .bind()
                .await?;
            let endpoint_id = ep.id();
            let gossip = Gossip::builder().spawn(ep.clone());
            let router = Router::builder(ep)
                .accept(GOSSIP_ALPN, gossip.clone())
                .spawn();
            let topic = gossip.subscribe(topic_id, vec![]).await?;
            let (sender, receiver) = topic.split();
            Ok((endpoint_id, router, gossip, sender, receiver))
        }

        // spawn 3 endpoints without relay or address lookup
        let (n1, r1, _g1, _tx1, mut rx1) = spawn(rng).await?;
        let (n2, r2, _g2, tx2, mut rx2) = spawn(rng).await?;
        let (n3, r3, _g3, tx3, mut rx3) = spawn(rng).await?;

        println!("endpoints {:?}", [n1, n2, n3]);

        // create a mem lookup that has only endpoint 1 addr info set
        let addr1 = r1.endpoint().addr();
        let lookup = MemoryLookup::new();
        lookup.add_endpoint_info(addr1);

        // add addr info of endpoint1 to endpoint2 and join endpoint1
        r2.endpoint().address_lookup()?.add(lookup.clone());
        tx2.join_peers(vec![n1]).await?;

        // await join endpoint2 -> nodde1
        timeout(Duration::from_secs(3), rx1.joined())
            .await
            .std_context("wait rx1 join")??;
        timeout(Duration::from_secs(3), rx2.joined())
            .await
            .std_context("wait rx2 join")??;

        // add addr info of endpoint1 to endpoint3 and join endpoint1
        r3.endpoint().address_lookup()?.add(lookup.clone());
        tx3.join_peers(vec![n1]).await?;

        // await join at endpoint3: n1 and n2
        // n2 only works because because we use gossip address lookup!
        let ev = timeout(Duration::from_secs(3), rx3.next())
            .await
            .std_context("wait rx3 first neighbor")?;
        assert!(matches!(ev, Some(Ok(Event::NeighborUp(_)))));
        let ev = timeout(Duration::from_secs(3), rx3.next())
            .await
            .std_context("wait rx3 second neighbor")?;
        assert!(matches!(ev, Some(Ok(Event::NeighborUp(_)))));

        assert_eq!(sorted(rx3.neighbors()), sorted([n1, n2]));

        let ev = timeout(Duration::from_secs(3), rx2.next())
            .await
            .std_context("wait rx2 neighbor")?;
        assert!(matches!(ev, Some(Ok(Event::NeighborUp(n))) if n == n3));

        let ev = timeout(Duration::from_secs(3), rx1.next())
            .await
            .std_context("wait rx1 neighbor")?;
        assert!(matches!(ev, Some(Ok(Event::NeighborUp(n))) if n == n3));

        tokio::try_join!(r1.shutdown(), r2.shutdown(), r3.shutdown())
            .std_context("shutdown routers")?;
        Ok(())
    }

    fn sorted<T: Ord>(input: impl IntoIterator<Item = T>) -> Vec<T> {
        let mut out: Vec<_> = input.into_iter().collect();
        out.sort();
        out
    }

    /// Test that dropping sender doesn't close topic while receiver is still listening.
    ///
    /// This is a common footgun: users split a GossipTopic, drop the sender early,
    /// and expect the receiver to keep working. With the bug (using && in still_needed),
    /// the topic closes immediately when sender is dropped.
    #[tokio::test]
    #[traced_test]
    async fn topic_stays_alive_after_sender_drop() -> n0_error::Result<()> {
        let topic_id = TopicId::from([99u8; 32]);

        let ep1 = Endpoint::bind(presets::Minimal).await?;
        let ep2 = Endpoint::bind(presets::Minimal).await?;
        let gossip1 = Gossip::builder().spawn(ep1.clone());
        let gossip2 = Gossip::builder().spawn(ep2.clone());
        let router1 = Router::builder(ep1)
            .accept(crate::ALPN, gossip1.clone())
            .spawn();
        let router2 = Router::builder(ep2)
            .accept(crate::ALPN, gossip2.clone())
            .spawn();

        let addr1 = router1.endpoint().addr();
        let id1 = addr1.id;
        let mem_lookup = MemoryLookup::new();
        mem_lookup.add_endpoint_info(addr1);
        router2.endpoint().address_lookup()?.add(mem_lookup);

        let topic1 = gossip1.subscribe(topic_id, vec![]).await?;
        let topic2 = gossip2.subscribe(topic_id, vec![id1]).await?;

        let (tx1, mut rx1) = topic1.split();
        let (tx2, mut rx2) = topic2.split();

        // Wait for mesh to form
        timeout(Duration::from_secs(3), rx1.joined())
            .await
            .std_context("wait rx1 join")??;
        timeout(Duration::from_secs(3), rx2.joined())
            .await
            .std_context("wait rx2 join")??;

        // Node 1 drops its sender - simulating the footgun where user drops sender early
        drop(tx1);

        // Node 2 sends a message - receiver on node 1 should still get it
        tx2.broadcast(b"hello from node2".to_vec().into()).await?;

        // Node 1's receiver should still work and receive the message
        let event = timeout(Duration::from_secs(3), rx1.next())
            .await
            .std_context("wait for message on rx1")?;

        match event {
            Some(Ok(Event::Received(msg))) => {
                assert_eq!(&msg.content[..], b"hello from node2");
            }
            other => panic!("expected Received event, got {:?}", other),
        }

        drop(tx2);
        drop(rx1);
        drop(rx2);
        router1.shutdown().await.std_context("shutdown router1")?;
        router2.shutdown().await.std_context("shutdown router2")?;
        Ok(())
    }

    /// A message that a connection delivered before its close is handled before the close.
    ///
    /// The receive loop queues the message before its task ends, but the actor can
    /// see the end first. A join from the peer then made it our neighbor after its
    /// connection was gone.
    #[tokio::test]
    #[traced_test]
    async fn message_before_a_close_is_handled_first() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, _peer_conn) = pair.connect_back().await?;
        let peer = conn.remote_id();
        let (mut actor, _rpc_tx, _local_tx) = Actor::new(
            pair.ep1.clone(),
            Default::default(),
            Default::default(),
            None,
            Default::default(),
        );
        let topic = [1; 32].into();
        let state = TopicState::default();
        let mut events = state.event_sender.subscribe();
        actor.topics.insert(topic, state);
        actor.handle_in_event(
            InEvent::Command(topic, ProtoCommand::Join(vec![])),
            Instant::now(),
        );
        actor
            .in_event_tx
            .send(InEvent::RecvMessage(peer, join_message(topic, 0)))
            .await
            .std_context("queue join")?;
        conn.close(0u32.into(), b"done");
        actor.handle_connection(conn);
        let (peer, conn, result) = actor
            .connection_tasks
            .join_next()
            .await
            .expect("one task")
            .expect("no panic");
        actor.handle_connection_task_finished(peer, conn, result);

        assert!(
            !actor.state.has_active_peers(&topic),
            "the join made a neighbor after its close"
        );
        let up = events.try_recv().std_context("no event")?;
        let down = events.try_recv().std_context("no second event")?;
        assert!(
            matches!(up, ProtoEvent::NeighborUp(p) if p == peer),
            "{up:?}"
        );
        assert!(
            matches!(down, ProtoEvent::NeighborDown(p) if p == peer),
            "{down:?}"
        );
        Ok(())
    }

    /// A stream with an incomplete header does not block the other streams.
    #[tokio::test]
    #[traced_test]
    async fn partial_header_does_not_block_other_streams() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let (in_event_tx, mut in_event_rx) = mpsc::channel(4);
        let mut recv_loop =
            RecvLoop::new(peer_conn.remote_id(), conn, in_event_tx, 1024, close_flag());
        let _recv = AbortOnDropHandle::new(spawn(async move { recv_loop.run().await }));

        let mut partial = peer_conn.open_uni().await.std_context("open stream")?;
        partial
            .write_all(&[0, 0, 0, 32, 0])
            .await
            .std_context("write partial header")?;
        let topic: TopicId = [1; 32].into();
        let _stream = open_stream(&peer_conn, topic, &[join_message(topic, 0).message]).await?;

        let event = timeout(Duration::from_secs(2), in_event_rx.recv())
            .await
            .std_context("the partial header blocked the other stream")?;
        assert!(matches!(event, Some(InEvent::RecvMessage(..))), "{event:?}");
        Ok(())
    }

    /// Holds a broadcast without letting subsequent commands pass it.
    ///
    /// Other commands and EOF do not consult pressure. Cancelling `next`
    /// cannot lose the command.
    #[tokio::test(start_paused = true)]
    async fn admission_wait_preserves_command_order() -> Result {
        let topic = [1; 32].into();
        let mut commands = SubscriptionCommands::default();
        commands.insert(
            topic,
            n0_future::stream::iter(vec![
                Ok(Command::JoinPeers(vec![])),
                Ok(Command::Broadcast(Bytes::from_static(b"held"))),
                Ok(Command::JoinPeers(vec![])),
                Ok(Command::BroadcastNeighbors(Bytes::from_static(b"bounded"))),
            ]),
        );
        let next = commands
            .next(|_| panic!("a join must not check pressure"))
            .await;
        assert!(matches!(next, Some((_, Some(Command::JoinPeers(_))))));

        let soon = BROADCAST_WAIT_LIMIT / 2;
        assert!(timeout(soon, commands.next(|_| true)).await.is_err());
        let next = timeout(BROADCAST_RECHECK_INTERVAL * 2, commands.next(|_| false))
            .await
            .std_context("pressure eased but the broadcast did not resume")?;
        assert!(matches!(next, Some((_, Some(Command::Broadcast(_))))));
        let next = commands
            .next(|_| panic!("a join must not check pressure"))
            .await;
        assert!(matches!(next, Some((_, Some(Command::JoinPeers(_))))));

        let start = Instant::now();
        let next = timeout(BROADCAST_WAIT_LIMIT * 2, commands.next(|_| true))
            .await
            .std_context("the broadcast waited past its deadline")?;
        assert!(matches!(
            next,
            Some((_, Some(Command::BroadcastNeighbors(_))))
        ));
        assert_eq!(start.elapsed(), BROADCAST_WAIT_LIMIT);
        assert!(matches!(
            commands
                .next(|_| panic!("EOF must not check pressure"))
                .await,
            Some((_, None))
        ));
        assert!(commands.is_empty());
        Ok(())
    }

    /// A fixed deadline survives cancelled actor selects and repeated rechecks.
    #[tokio::test(start_paused = true)]
    async fn admission_deadline_survives_cancellation() -> Result {
        let mut commands = SubscriptionCommands::default();
        commands.insert(
            [1; 32].into(),
            n0_future::stream::iter(vec![Ok(Command::Broadcast(Bytes::from_static(b"bounded")))]),
        );
        let start = Instant::now();
        for _ in 0..3 {
            assert!(timeout(BROADCAST_WAIT_LIMIT / 4, commands.next(|_| true))
                .await
                .is_err());
        }
        let next = timeout(BROADCAST_WAIT_LIMIT, commands.next(|_| true))
            .await
            .std_context("the deadline was extended")?;
        assert!(matches!(next, Some((_, Some(Command::Broadcast(_))))));
        assert_eq!(start.elapsed(), BROADCAST_WAIT_LIMIT);
        Ok(())
    }

    /// Every waiting receiver retries independently when pressure eases.
    #[tokio::test(start_paused = true)]
    async fn admission_resumes_every_publisher() -> Result {
        let topic = [1; 32].into();
        let mut commands = SubscriptionCommands::default();
        for name in [b"first".as_slice(), b"second".as_slice()] {
            commands.insert(
                topic,
                n0_future::stream::iter(vec![Ok(Command::Broadcast(Bytes::copy_from_slice(name)))]),
            );
        }
        assert!(timeout(BROADCAST_WAIT_LIMIT / 2, commands.next(|_| true))
            .await
            .is_err());
        let start = Instant::now();
        let mut received = BTreeSet::new();
        while received.len() < 2 {
            let next = timeout(BROADCAST_RECHECK_INTERVAL * 2, commands.next(|_| false))
                .await
                .std_context("a publisher did not resume")?;
            if let Some((_, Some(Command::Broadcast(message)))) = next {
                received.insert(message);
            } else {
                assert!(matches!(next, Some((_, None))));
            }
        }
        assert_eq!(received.len(), 2);
        assert!(start.elapsed() <= BROADCAST_RECHECK_INTERVAL * 2);
        Ok(())
    }

    /// Admitting one publisher can consume the room another publisher needs.
    #[tokio::test(start_paused = true)]
    async fn admission_rechecks_between_publishers() -> Result {
        let topic = [1; 32].into();
        let mut commands = SubscriptionCommands::default();
        for _ in 0..2 {
            commands.insert(
                topic,
                n0_future::stream::iter(vec![Ok(Command::Broadcast(Bytes::from_static(
                    b"waiting",
                )))]),
            );
        }
        let start = Instant::now();
        assert!(timeout(BROADCAST_WAIT_LIMIT / 2, commands.next(|_| true))
            .await
            .is_err());
        let first = commands.next(|_| false).await;
        assert!(matches!(first, Some((_, Some(Command::Broadcast(_))))));

        // Model the first broadcast filling the queues again. EOF from its
        // receiver may arrive first, but the second broadcast must still wait.
        loop {
            let next = commands.next(|_| true).await;
            if matches!(next, Some((_, Some(Command::Broadcast(_))))) {
                break;
            }
            assert!(matches!(next, Some((_, None))));
        }
        assert_eq!(start.elapsed(), BROADCAST_WAIT_LIMIT);
        Ok(())
    }

    /// Waiting on one subscription never stalls another topic's commands.
    #[tokio::test(start_paused = true)]
    async fn admission_leaves_other_topics_ready() -> Result {
        let blocked = [1; 32].into();
        let healthy = [2; 32].into();
        let mut commands = SubscriptionCommands::default();
        commands.insert(
            blocked,
            n0_future::stream::iter(vec![Ok(Command::Broadcast(Bytes::from_static(b"waiting")))]),
        );
        commands.insert(
            healthy,
            n0_future::stream::iter(vec![
                Ok(Command::JoinPeers(vec![])),
                Ok(Command::Broadcast(Bytes::from_static(b"ready"))),
            ]),
        );
        let start = Instant::now();
        let next = commands.next(|topic| topic == blocked).await;
        assert!(matches!(next, Some((topic, Some(Command::JoinPeers(_)))) if topic == healthy));
        let next = commands.next(|topic| topic == blocked).await;
        assert!(matches!(next, Some((topic, Some(Command::Broadcast(_)))) if topic == healthy));
        assert_eq!(start.elapsed(), Duration::ZERO);
        Ok(())
    }

    /// Admission uses current eager targets, including before the first broadcast.
    ///
    /// No network: the peers join by received messages, and have active states
    /// that nothing writes.
    #[tokio::test]
    #[traced_test]
    async fn admission_follows_current_eager_peers() -> Result {
        let topic: TopicId = blake3::hash(b"eager").into();
        let mut actor = t_actor().await?;
        actor.send_queue_size = queue::MIN_QUEUE_SIZE;
        actor.topics.insert(topic, TopicState::default());
        // A subscriber keeps the topic, which the actor quits otherwise.
        let _events = actor.topics[&topic].event_sender.subscribe();
        actor.handle_in_event(
            InEvent::Command(topic, ProtoCommand::Join(vec![])),
            Instant::now(),
        );
        let mut peers = Vec::new();
        for id in 0..2usize {
            let peer = SecretKey::generate().public();
            let mut state = open_peer(queue::MIN_QUEUE_SIZE);
            let _lease = state
                .handle_connection(Side::Server, id)
                .expect("an accepted connection takes the lease");
            actor.peers.insert(peer, state);
            let join = join_message(topic, 0);
            actor.handle_in_event(InEvent::RecvMessage(peer, join), Instant::now());
            peers.push(peer);
        }
        let [full, other] = peers[..] else {
            unreachable!()
        };
        assert_eq!(
            actor.state.eager_peers(&topic).count(),
            2,
            "both joined eager"
        );
        let fill = |actor: &Actor, peer| {
            let queue = &actor.peers[&peer].queue;
            while !queue.is_busy() {
                assert!(matches!(queue.push(numbered(topic, 0)), Pushed::Queued));
            }
        };
        let backed_up = |actor: &Actor| all_eager_peers_busy(&actor.state, &actor.peers, topic);

        fill(&actor, full);
        assert!(
            !backed_up(&actor),
            "one eager peer with room must keep the gate open"
        );

        // A prune makes the other peer lazy, so only the full one counts.
        let prune = ProtoMessage {
            topic,
            message: TopicMessage::test_prune(),
        };
        actor.handle_in_event(InEvent::RecvMessage(other, prune), Instant::now());
        assert!(
            backed_up(&actor),
            "a lazy peer with room must not keep the gate open"
        );

        actor.command_rx.insert(
            topic,
            n0_future::stream::iter(vec![Ok(Command::Broadcast(Bytes::from_static(b"first")))]),
        );
        actor.topics.get_mut(&topic).unwrap().command_receiver_count += 1;
        assert!(
            timeout(
                BROADCAST_RECHECK_INTERVAL * 2,
                actor
                    .command_rx
                    .next(|topic| { all_eager_peers_busy(&actor.state, &actor.peers, topic) })
            )
            .await
            .is_err(),
            "the first broadcast must wait before protocol processing"
        );
        actor
            .peers
            .get_mut(&full)
            .unwrap()
            .discard_queue(Instant::now());
        assert!(!backed_up(&actor), "a replacement queue has room");
        let next = timeout(
            BROADCAST_RECHECK_INTERVAL * 2,
            actor
                .command_rx
                .next(|topic| all_eager_peers_busy(&actor.state, &actor.peers, topic)),
        )
        .await
        .std_context("queue replacement did not resume admission")?;
        assert!(matches!(next, Some((_, Some(Command::Broadcast(_))))));

        actor.handle_in_event(InEvent::PeerDisconnected(full), Instant::now());
        assert!(
            !backed_up(&actor),
            "the gate stayed closed without eager peers"
        );
        Ok(())
    }

    /// Joins a topic with a publisher that always has a command ready.
    fn publish_forever(actor: &mut Actor) {
        let topic = [1; 32].into();
        actor.topics.insert(topic, TopicState::default());
        actor.handle_in_event(
            InEvent::Command(topic, ProtoCommand::Join(vec![])),
            Instant::now(),
        );
        let commands = n0_future::stream::iter(std::iter::repeat_with(|| {
            Ok(Command::Broadcast(Bytes::new()))
        }));
        actor.command_rx.insert(topic, commands);
        actor
            .topics
            .get_mut(&topic)
            .expect("inserted above")
            .command_receiver_count += 1;
    }

    /// A publisher that always has a command ready does not starve a finished connection.
    #[tokio::test]
    #[traced_test]
    async fn busy_publisher_does_not_starve_finished_connections() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, _peer_conn) = pair.connect_back().await?;
        let (mut actor, _rpc_tx, _local_tx) = Actor::new(
            pair.ep1.clone(),
            Default::default(),
            Default::default(),
            None,
            Default::default(),
        );
        publish_forever(&mut actor);
        conn.close(0u32.into(), b"done");
        actor.handle_connection(conn);
        let mut addresses = n0_future::stream::pending();
        for i in 0..256 {
            actor.event_loop(&mut addresses, i).await;
            tokio::task::yield_now().await;
            if actor.connection_tasks.is_empty() {
                return Ok(());
            }
        }
        panic!("publishing starved the finished connection");
    }

    /// A shutdown comes before the commands of a busy publisher.
    ///
    /// Without a priority, the actor picks its next branch at random, so a single
    /// round can miss the bug.
    #[tokio::test]
    #[traced_test]
    async fn shutdown_comes_before_a_busy_publisher() -> Result {
        let pair = TestPair::new(1).await?;
        for round in 0..8 {
            let (mut actor, _rpc_tx, local_tx) = Actor::new(
                pair.ep1.clone(),
                Default::default(),
                Default::default(),
                None,
                Default::default(),
            );
            publish_forever(&mut actor);
            let (reply, mut reply_rx) = oneshot::channel();
            local_tx
                .send(LocalActorMessage::Shutdown { reply })
                .await
                .std_context("send shutdown")?;
            let mut addresses = n0_future::stream::pending();
            assert!(
                !actor.event_loop(&mut addresses, 0).await,
                "round {round}: the actor did not stop"
            );
            reply_rx.try_recv().std_context("no shutdown reply")?;
        }
        Ok(())
    }

    /// A frame one byte over the size limit is refused on read.
    #[tokio::test]
    #[traced_test]
    async fn frame_over_the_size_limit_is_refused() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let topic: TopicId = blake3::hash(b"frame_over_the_size_limit").into();
        let message = join_message(topic, 64).message;
        let size = postcard::experimental::serialized_size(&message).std_context("size")?;

        let mut stream = conn.open_uni().await.std_context("open stream")?;
        util::write_frame(&mut stream, &message, &mut Vec::new(), size + 1)
            .await
            .std_context("write")?;
        stream.finish().std_context("finish")?;

        let mut stream = peer_conn.accept_uni().await.std_context("accept")?;
        let read: Result<Option<TopicMessage>, _> =
            util::read_frame(&mut stream, &mut BytesMut::new(), size - 1).await;
        assert!(
            matches!(read, Err(util::ReadError::TooLarge { .. })),
            "{read:?}"
        );
        Ok(())
    }

    /// A frame shorter than the size limit goes through, and one of the limit does not.
    ///
    /// Nodes up to 0.101 read a frame of the limit, but refuse to write one, so
    /// they could not forward it.
    #[tokio::test]
    #[traced_test]
    async fn frame_shorter_than_the_size_limit_goes_through() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let topic: TopicId = blake3::hash(b"frame_shorter_than_the_size_limit").into();
        let message = join_message(topic, 64).message;
        let size = postcard::experimental::serialized_size(&message).std_context("size")?;

        let mut stream = conn.open_uni().await.std_context("open stream")?;
        let mut buffer = Vec::new();
        let refused = util::write_frame(&mut stream, &message, &mut buffer, size).await;
        assert!(
            matches!(refused, Err(util::WriteError::TooLarge { .. })),
            "{refused:?}"
        );
        util::write_frame(&mut stream, &message, &mut buffer, size + 1)
            .await
            .std_context("write")?;
        stream.finish().std_context("finish")?;

        let mut stream = peer_conn.accept_uni().await.std_context("accept")?;
        let read: Option<TopicMessage> = util::read_frame(&mut stream, &mut BytesMut::new(), size)
            .await
            .std_context("read")?;
        assert!(read.is_some(), "the frame did not arrive");
        Ok(())
    }

    /// The send task drops a message over the size limit, and the connection stays.
    #[tokio::test]
    #[traced_test]
    async fn oversized_message_is_dropped() -> Result {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let peer_id = pair.ep2.id();
        let topic: TopicId = blake3::hash(b"oversized_message").into();
        let mut queue = SendQueue::new(queue::MIN_QUEUE_SIZE);
        let (in_event_tx, _in_event_rx) = mpsc::channel(16);
        let task = AbortOnDropHandle::new(spawn(connection_loop(
            peer_id,
            conn,
            CancellationToken::new(),
            Some(queue.lease()),
            in_event_tx,
            1024,
            watch::channel(true).1,
            close_flag(),
        )));

        assert!(matches!(
            queue.push(join_message(topic, 4096)),
            Pushed::Queued
        ));
        assert!(matches!(
            queue.push(join_message(topic, 16)),
            Pushed::Queued
        ));

        let (_, message) = timeout(Duration::from_secs(5), read_next_message(&peer_conn))
            .await
            .std_context("nothing arrived after the oversized message")??;
        let size = postcard::experimental::serialized_size(&message).std_context("size")?;
        assert!(size < 1024, "the oversized message went out");
        assert!(!task.is_finished(), "the connection ended");
        Ok(())
    }

    /// How long a slow peer test waits for something that should happen promptly.
    const PROMPT: Duration = Duration::from_secs(10);

    /// A gossip node that connects only directly, with its router.
    struct DirectNode {
        router: Router,
        gossip: Gossip,
    }

    impl DirectNode {
        /// Spawns gossip from `builder` on an endpoint with `transport`, and adds its address to `lookup`.
        async fn spawn(
            lookup: &MemoryLookup,
            builder: Builder,
            transport: QuicTransportConfig,
        ) -> Result<Self> {
            let endpoint = direct_endpoint(lookup, transport).await?;
            let gossip = builder.spawn(endpoint.clone());
            let router = Router::builder(endpoint)
                .accept(GOSSIP_ALPN, gossip.clone())
                .spawn();
            Ok(Self { router, gossip })
        }

        fn id(&self) -> EndpointId {
            self.router.endpoint().id()
        }
    }

    /// Binds an endpoint without a relay, and adds its address to `lookup`.
    async fn direct_endpoint(
        lookup: &MemoryLookup,
        transport: QuicTransportConfig,
    ) -> Result<Endpoint> {
        let endpoint = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .alpns(vec![GOSSIP_ALPN.to_vec()])
            .transport_config(transport)
            .bind()
            .await?;
        endpoint.address_lookup()?.add(lookup.clone());
        lookup.set_endpoint_info(endpoint.addr());
        Ok(endpoint)
    }

    /// Returns a transport with a receive window of `window` bytes per stream, and twice that per connection.
    fn small_windows(window: u32) -> QuicTransportConfig {
        QuicTransportConfig::builder()
            .stream_receive_window(VarInt::from_u32(window))
            .receive_window(VarInt::from_u32(window * 2))
            .build()
    }

    /// Waits until `receiver` reports `peer` as a neighbor.
    async fn neighbor_up(receiver: &mut GossipReceiver, peer: EndpointId) -> Result {
        timeout(PROMPT, async {
            while let Some(event) = receiver.try_next().await? {
                if matches!(event, api::Event::NeighborUp(up) if up == peer) {
                    return Ok(());
                }
            }
            Err(n0_error::anyerr!("the subscription ended"))
        })
        .await
        .std_context("the peer did not become a neighbor")?
    }

    /// Waits until `receiver` got a message with `content`, and returns the contents it got.
    async fn receive_until(receiver: &mut GossipReceiver, content: &[u8]) -> Result<Vec<Bytes>> {
        let mut received = Vec::new();
        while let Some(event) = receiver.try_next().await? {
            if let api::Event::Received(message) = event {
                let done = message.content == content;
                received.push(message.content);
                if done {
                    return Ok(received);
                }
            }
        }
        Err(n0_error::anyerr!("the subscription ended"))
    }

    /// A healthy peer that shares more topics with us than its stream limit keeps its neighbors (a cost of the design).
    ///
    /// v1 keeps a stream per topic open while the topic uses the peer. Over the
    /// peer's limit of concurrent streams, the open of the next stream waits for
    /// good, and the open timeout closes the connection. The topics rejoin, and
    /// it closes again. The limit of iroh is 100 streams by default.
    #[tokio::test(flavor = "multi_thread")]
    #[traced_test]
    #[ignore = "a cost of the design: one stream per topic, see SendQueue"]
    async fn topics_over_the_stream_limit_keep_their_neighbors() -> Result {
        let lookup = MemoryLookup::new();
        let limited = || {
            QuicTransportConfig::builder()
                .max_concurrent_uni_streams(VarInt::from_u32(4))
                .build()
        };
        let a = DirectNode::spawn(&lookup, Gossip::builder(), limited()).await?;
        let b = DirectNode::spawn(&lookup, Gossip::builder(), limited()).await?;
        let mut topics = Vec::new();
        for i in 0..8u32 {
            let topic: TopicId = blake3::hash(&i.to_be_bytes()).into();
            topics.push(a.gossip.subscribe(topic, vec![]).await?);
            topics.push(b.gossip.subscribe(topic, vec![a.id()]).await?);
        }
        let joined = async {
            for topic in topics.iter_mut() {
                topic.joined().await?;
            }
            Ok::<_, AnyError>(())
        };
        timeout(PROMPT, joined)
            .await
            .std_context("the topics did not join")??;
        // Long enough for the open timeout of a stream over the limit.
        tokio::time::sleep(util::OPEN_TIMEOUT + Duration::from_secs(2)).await;
        let down = a.gossip.metrics().neighbor_down.get() + b.gossip.metrics().neighbor_down.get();
        assert_eq!(down, 0, "neighbors went down");
        Ok(())
    }

    /// A publisher whose every eager peer has a full queue waits, loses nothing, and goes on later.
    ///
    /// The only neighbor, an eager peer, reads nothing, as when our own uplink
    /// is the limit for all peers. Without the wait, the queue drops the
    /// broadcasts. Once the peer reads again, the gate opens.
    #[tokio::test(flavor = "multi_thread")]
    #[traced_test]
    async fn publisher_waits_while_all_neighbors_are_full() -> Result {
        let lookup = MemoryLookup::new();
        let a = DirectNode::spawn(&lookup, Gossip::builder(), small_windows(16 * 1024)).await?;
        let topic: TopicId = blake3::hash(b"all full").into();
        let (a_tx, mut a_rx) = a.gossip.subscribe(topic, vec![]).await?.split();

        // B joins on its own connection, and then reads nothing.
        let b = direct_endpoint(&lookup, small_windows(16 * 1024)).await?;
        let conn = b.connect(a.id(), GOSSIP_ALPN).await?;
        let _join = open_stream(&conn, topic, &[join_message(topic, 0).message]).await?;
        neighbor_up(&mut a_rx, b.id()).await?;

        // The gate never opens while B reads nothing, so a waiting broadcast
        // takes the whole limit.
        let mut waited = false;
        for i in 0..2000u32 {
            let mut message = vec![7u8; 3000];
            message[..4].copy_from_slice(&i.to_be_bytes());
            let start = Instant::now();
            timeout(PROMPT, a_tx.broadcast(message.into()))
                .await
                .std_context("the publisher waited past the limit")??;
            if start.elapsed() >= BROADCAST_WAIT_LIMIT {
                waited = true;
                break;
            }
        }
        assert!(waited, "the publisher never waited");

        // B reads again, so the gate opens, and the publisher goes on.
        const LAST: &[u8] = b"after 99";
        let (seen_tx, seen_rx) = tokio::sync::oneshot::channel();
        let reader = conn.clone();
        let _read = AbortOnDropHandle::new(spawn(async move {
            let mut stream = reader.accept_uni().await.expect("the topic stream");
            // The end of the previous chunk, as the marker may span two.
            let mut seen = Vec::new();
            while let Ok(Some(chunk)) = stream.read_chunk(usize::MAX).await {
                seen.drain(..seen.len().saturating_sub(LAST.len()));
                seen.extend_from_slice(&chunk);
                if seen.windows(LAST.len()).any(|w| w == LAST) {
                    seen_tx.send(()).ok();
                    break;
                }
            }
        }));
        // Far less than a limit per broadcast, so the gate opened rather than timed out.
        timeout(BROADCAST_WAIT_LIMIT * 20, async {
            for i in 0..100u32 {
                a_tx.broadcast(Bytes::from(format!("after {i}"))).await?;
            }
            Ok::<_, AnyError>(())
        })
        .await
        .std_context("the publisher still waits after B read again")??;
        timeout(Duration::from_secs(10), seen_rx)
            .await
            .std_context("B did not get the last broadcast")?
            .std_context("B stopped reading")?;
        assert!(conn.close_reason().is_none(), "the connection closed");
        assert_eq!(a.gossip.metrics().msgs_dropped.get(), 0, "messages dropped");
        conn.close(0u32.into(), b"done");
        Ok(())
    }

    /// A publisher gets its broadcasts out while relayed traffic keeps its neighbors full.
    ///
    /// B reads slowly, and is the only eager peer of A on `t1`. C floods `t2`,
    /// and A relays it to B, so the queue to B stays over half full. Relays
    /// never wait, so the gate alone stays closed for as long as C sends.
    #[tokio::test(flavor = "multi_thread")]
    #[traced_test]
    async fn relayed_traffic_does_not_starve_the_publisher() -> Result {
        let lookup = MemoryLookup::new();
        let default = || QuicTransportConfig::builder().build();
        let a = DirectNode::spawn(&lookup, Gossip::builder(), default()).await?;
        let c = DirectNode::spawn(&lookup, Gossip::builder(), default()).await?;
        let (t1, t2): (TopicId, TopicId) = (
            blake3::hash(b"starved").into(),
            blake3::hash(b"flood").into(),
        );
        let (a1, mut a1_rx) = a.gossip.subscribe(t1, vec![]).await?.split();
        let (_a2, _a2_rx) = a.gossip.subscribe(t2, vec![]).await?.split();

        // B joins both topics on its own connection, and reads slowly.
        let b = direct_endpoint(&lookup, small_windows(16 * 1024)).await?;
        let conn = b.connect(a.id(), GOSSIP_ALPN).await?;
        let _join1 = open_stream(&conn, t1, &[join_message(t1, 0).message]).await?;
        let _join2 = open_stream(&conn, t2, &[join_message(t2, 0).message]).await?;
        neighbor_up(&mut a1_rx, b.id()).await?;
        let reader = conn.clone();
        let _read = AbortOnDropHandle::new(spawn(async move {
            let mut tasks = JoinSet::new();
            while let Ok(mut stream) = reader.accept_uni().await {
                tasks.spawn(async move {
                    while let Ok(Some(_)) = stream.read_chunk(4096).await {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                });
            }
        }));

        let (c2, _c2_rx) = c.gossip.subscribe_and_join(t2, vec![a.id()]).await?.split();
        let _flood = AbortOnDropHandle::new(spawn(async move {
            for i in 0u64.. {
                let mut message = vec![1u8; 1000];
                message[..8].copy_from_slice(&i.to_be_bytes());
                if c2.broadcast(message.into()).await.is_err() {
                    break;
                }
            }
        }));
        // The relayed data fills the queue to B up to its limit.
        let metrics = a.gossip.metrics().clone();
        timeout(PROMPT, async {
            while metrics.msgs_dropped.get() == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .std_context("the queue to B never filled")?;

        // More than the command channel holds, so the publisher waits for the actor.
        for i in 0..100u32 {
            timeout(PROMPT, a1.broadcast(Bytes::from(format!("own {i}"))))
                .await
                .std_context("the publisher starved")??;
        }
        assert!(conn.close_reason().is_none(), "the connection closed");
        conn.close(0u32.into(), b"done");
        Ok(())
    }

    /// A peer that reads nothing fills its queue, and the other peers still get each message (#47).
    ///
    /// A neighbor of the publisher keeps its connection open but never reads,
    /// so the queue to it fills and drops payloads. A healthy neighbor gets the
    /// later messages on the same topic and on a second one, and the publisher
    /// shuts down in time.
    #[tokio::test(flavor = "multi_thread")]
    #[traced_test]
    async fn blocked_peer_does_not_stall_the_others() -> Result {
        let lookup = MemoryLookup::new();
        let default = || QuicTransportConfig::builder().build();
        let a = DirectNode::spawn(&lookup, Gossip::builder(), default()).await?;
        let c = DirectNode::spawn(&lookup, Gossip::builder(), default()).await?;
        let (t1, t2): (TopicId, TopicId) = (
            blake3::hash(b"blocked 1").into(),
            blake3::hash(b"blocked 2").into(),
        );
        let (a1, mut a1_rx) = a.gossip.subscribe(t1, vec![]).await?.split();
        let (a2, _a2_rx) = a.gossip.subscribe(t2, vec![]).await?.split();
        let (_c1_tx, mut c1) = c.gossip.subscribe_and_join(t1, vec![a.id()]).await?.split();
        let (_c2_tx, mut c2) = c.gossip.subscribe_and_join(t2, vec![a.id()]).await?.split();

        // B joins on its own connection, and then reads nothing.
        let b = direct_endpoint(&lookup, small_windows(16 * 1024)).await?;
        let conn = b.connect(a.id(), GOSSIP_ALPN).await?;
        let _join = open_stream(&conn, t1, &[join_message(t1, 0).message]).await?;
        neighbor_up(&mut a1_rx, b.id()).await?;

        let metrics = a.gossip.metrics().clone();
        let payload = Bytes::from(vec![7u8; 3000]);
        timeout(PROMPT, async {
            let mut i = 0u32;
            while metrics.msgs_dropped.get() == 0 {
                let mut message = payload.to_vec();
                message.extend_from_slice(&i.to_be_bytes());
                a1.broadcast(message.into()).await?;
                i += 1;
            }
            Ok::<_, AnyError>(())
        })
        .await
        .std_context("the queue to B did not fill")??;
        // Checked now: B reads nothing, so the write timeout closes it later.
        assert!(conn.close_reason().is_none(), "B lost its connection");

        a1.broadcast(Bytes::from_static(b"after")).await?;
        a2.broadcast(Bytes::from_static(b"other topic")).await?;
        timeout(PROMPT, receive_until(&mut c1, b"after"))
            .await
            .std_context("C missed a message on the topic of B")??;
        timeout(PROMPT, receive_until(&mut c2, b"other topic"))
            .await
            .std_context("C missed a message on the other topic")??;
        assert_eq!(
            metrics.peers_send_queue_full.get(),
            0,
            "B lost its connection"
        );

        timeout(Duration::from_secs(5), a.gossip.shutdown())
            .await
            .std_context("B blocked the shutdown")??;
        conn.close(0u32.into(), b"done");
        Ok(())
    }

    /// A slow peer gets the payloads that its full queue dropped, once it catches up.
    ///
    /// A relays the broadcasts of C to B. The queue to B is small and B's
    /// windows are small, so a burst drops payloads. A relays and publishes
    /// nothing, so it never waits. A announces the dropped payloads to B again,
    /// and B requests them.
    #[tokio::test(flavor = "multi_thread")]
    #[traced_test]
    async fn slow_peer_gets_the_dropped_messages() -> Result {
        let lookup = MemoryLookup::new();
        let small_queue = Gossip::builder().send_queue(QueueOptions::default().size(0));
        let default = || QuicTransportConfig::builder().build();
        let a = DirectNode::spawn(&lookup, small_queue, default()).await?;
        let b = DirectNode::spawn(&lookup, Gossip::builder(), small_windows(8 * 1024)).await?;
        let topic: TopicId = blake3::hash(b"slow peer").into();
        let (_a_tx, mut a_rx) = a.gossip.subscribe(topic, vec![]).await?.split();
        let (_b_tx, mut b_rx) = b
            .gossip
            .subscribe_and_join(topic, vec![a.id()])
            .await?
            .split();

        // C publishes on its own connection, and reads every stream of A at once.
        let c = direct_endpoint(&lookup, default()).await?;
        let c_conn = c.connect(a.id(), GOSSIP_ALPN).await?;
        let mut publish = open_stream(&c_conn, topic, &[join_message(topic, 0).message]).await?;
        let reader = c_conn.clone();
        let _read = AbortOnDropHandle::new(spawn(async move {
            while let Ok(mut stream) = reader.accept_uni().await {
                spawn(async move { stream.read_to_end(usize::MAX).await.ok() });
            }
        }));
        timeout(PROMPT, async {
            while let Some(event) = a_rx.try_next().await? {
                if matches!(event, api::Event::NeighborUp(peer) if peer == c.id()) {
                    return Ok(());
                }
            }
            Err(n0_error::anyerr!("the subscription ended"))
        })
        .await
        .std_context("C did not become a neighbor")??;

        // B reads all along, so that its subscription does not lag. The channel
        // holds more than all bursts.
        let (got_tx, mut got_rx) = mpsc::channel(8192);
        let _receive = AbortOnDropHandle::new(spawn(async move {
            while let Ok(Some(event)) = b_rx.try_next().await {
                if let api::Event::Received(message) = event {
                    got_tx.send(message.content).await.ok();
                }
            }
        }));

        // A burst of 60 overflows the queue to B, which holds about 16 payloads.
        let metrics = a.gossip.metrics().clone();
        let mut sent = BTreeSet::new();
        let mut buffer = Vec::new();
        for burst in 0..40u32 {
            for i in 0..60u32 {
                let mut content = vec![7u8; 3000];
                content.extend_from_slice(&burst.to_be_bytes());
                content.extend_from_slice(&i.to_be_bytes());
                let content = Bytes::from(content);
                sent.insert(content.clone());
                let message = TopicMessage::test_payload(content);
                util::write_frame(&mut publish, &message, &mut buffer, TEST_FRAME_LIMIT)
                    .await
                    .std_context("publish")?;
            }
            if metrics.msgs_dropped.get() > 0 {
                break;
            }
            // Let the queue drain, so that each burst starts empty.
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        assert!(metrics.msgs_dropped.get() > 0, "no burst filled the queue");

        let mut received = BTreeSet::new();
        timeout(Duration::from_secs(25), async {
            while received.len() < sent.len() {
                match got_rx.recv().await {
                    Some(content) => {
                        received.insert(content);
                    }
                    None => break,
                }
            }
        })
        .await
        .with_std_context(|_| {
            format!(
                "B got {} of {} payloads, {} dropped",
                received.len(),
                sent.len(),
                metrics.msgs_dropped.get()
            )
        })?;
        assert_eq!(received, sent);
        Ok(())
    }
}
