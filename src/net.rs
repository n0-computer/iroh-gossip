//! Networking for the `iroh-gossip` protocol

#[cfg(test)]
use std::sync::atomic::AtomicBool;
use std::{
    collections::{hash_map, BTreeSet, HashMap, HashSet, VecDeque},
    ops::DerefMut,
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use iroh::{
    endpoint::{ConnectError, Connection},
    protocol::{AcceptError, ProtocolHandler},
    Endpoint, NodeAddr, NodeId, Watcher,
};
use irpc::{
    channel::{self, mpsc::RecvError},
    WithChannels,
};
use n0_future::{
    stream::Boxed as BoxStream,
    task::{self, AbortOnDropHandle},
    time::Instant,
    MergeUnbounded, Stream, StreamExt,
};
use n0_watcher::{Direct, Watchable};
use rand::rngs::StdRng;
use snafu::Snafu;
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinSet,
};
use tracing::{debug, error_span, info, instrument, trace, warn, Instrument};

use self::{
    dialer::Dialer,
    discovery::GossipDiscovery,
    net_proto::GossipMessage,
    util::{AddrInfo, ConnectionCounter, Guarded, IrohRemoteConnection, Timers},
};
use crate::{
    api::{self, GossipApi},
    metrics::{inc, Metrics},
    net::util::accept_stream,
    proto::{self, Config, HyparviewConfig, PeerData, PlumtreeConfig, TopicId},
};

mod dialer;
mod discovery;
mod util;

/// ALPN protocol name
pub const GOSSIP_ALPN: &[u8] = b"/iroh-gossip/1";

type InEvent = proto::topic::InEvent<NodeId>;
type OutEvent = proto::topic::OutEvent<NodeId>;
type Timer = proto::topic::Timer<NodeId>;
type ProtoMessage = proto::topic::Message<NodeId>;
type ProtoEvent = proto::topic::Event<NodeId>;
type State = proto::topic::State<NodeId, StdRng>;
type Command = proto::topic::Command<NodeId>;

/// Publish and subscribe on gossiping topics.
///
/// Each topic is a separate broadcast tree with separate memberships.
/// A topic has to be joined before you can publish or subscribe on the topic.
/// To join the swarm for a topic, you have to know the [`NodeId`] of at least one peer that also joined the topic.
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
pub struct Gossip(Arc<Inner>);

impl std::ops::Deref for Gossip {
    type Target = GossipApi;
    fn deref(&self) -> &Self::Target {
        &self.0.api
    }
}

#[derive(derive_more::Debug)]
enum LocalActorMessage {
    #[debug("HandleConnection({})", _0.fmt_short())]
    HandleConnection(NodeId, Connection),
    #[debug("Connect({}, {})", _0.fmt_short(), _1.fmt_short())]
    Connect(NodeId, TopicId),
    #[debug("SetPeerData({}, {})", _0.fmt_short(), _1.as_bytes().len())]
    SetPeerData(NodeId, PeerData),
}

#[derive(Debug)]
struct Inner {
    api: GossipApi,
    local_tx: mpsc::Sender<LocalActorMessage>,
    _actor_handle: AbortOnDropHandle<()>,
    max_message_size: usize,
    metrics: Arc<Metrics>,
}

impl ProtocolHandler for Gossip {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote = connection.remote_node_id()?;
        self.handle_connection(remote, connection)
            .await
            .map_err(AcceptError::from_err)?;
        Ok(())
    }

    async fn shutdown(&self) {
        // TODO: Graceful shutdown?
    }
}

/// Builder to configure and construct [`Gossip`].
#[derive(Debug, Clone)]
pub struct Builder {
    config: proto::Config,
    alpn: Option<Bytes>,
}

impl Builder {
    /// Sets the maximum message size in bytes.
    /// By default this is `4096` bytes.
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

    /// Spawn a gossip actor and get a handle for it
    pub fn spawn(self, endpoint: Endpoint) -> Gossip {
        Gossip::new(endpoint, self.config, self.alpn)
    }
}

impl Gossip {
    /// Creates a default `Builder`, with the endpoint set.
    pub fn builder() -> Builder {
        Builder {
            config: Default::default(),
            alpn: None,
        }
    }

    /// Listen on a quinn endpoint for incoming RPC connections.
    #[cfg(feature = "rpc")]
    pub async fn listen(self, endpoint: quinn::Endpoint) {
        self.0.api.listen(endpoint).await
    }

    /// Get the maximum message size configured for this gossip actor.
    pub fn max_message_size(&self) -> usize {
        self.0.max_message_size
    }

    /// Handle an incoming [`Connection`].
    ///
    /// Make sure to check the ALPN protocol yourself before passing the connection.
    pub async fn handle_connection(
        &self,
        remote: NodeId,
        connection: Connection,
    ) -> Result<(), ActorStoppedError> {
        self.0
            .local_tx
            .send(LocalActorMessage::HandleConnection(remote, connection))
            .await
            .map_err(|_| ActorStoppedSnafu.build())?;
        Ok(())
    }

    /// Returns the metrics tracked for this gossip instance.
    pub fn metrics(&self) -> &Arc<Metrics> {
        &self.0.metrics
    }

    fn new(endpoint: Endpoint, config: Config, alpn: Option<Bytes>) -> Self {
        let metrics = Arc::new(Metrics::default());
        let max_message_size = config.max_message_size;
        let me = endpoint.node_id();
        let (api_tx, local_tx, actor) = Actor::new(endpoint, config, alpn, metrics.clone());
        let actor_task = task::spawn(
            actor
                .run()
                .instrument(error_span!("gossip", me=%me.fmt_short())),
        );

        Self(Arc::new(Inner {
            local_tx,
            max_message_size,
            api: GossipApi::local(api_tx),
            metrics,
            _actor_handle: AbortOnDropHandle::new(actor_task),
        }))
    }

    #[cfg(test)]
    fn new_with_actor(endpoint: Endpoint, config: Config, alpn: Option<Bytes>) -> (Self, Actor) {
        let metrics = Arc::new(Metrics::default());
        let max_message_size = config.max_message_size;
        let (api_tx, local_tx, actor) = Actor::new(endpoint, config, alpn, metrics.clone());
        let handle = Self(Arc::new(Inner {
            local_tx,
            max_message_size,
            api: GossipApi::local(api_tx),
            metrics,
            _actor_handle: AbortOnDropHandle::new(task::spawn(std::future::pending())),
        }));
        (handle, actor)
    }
}

mod net_proto {
    use irpc::{channel::mpsc, rpc_requests};
    use serde::{Deserialize, Serialize};

    use crate::proto::TopicId;

    #[derive(Debug, Serialize, Deserialize, Clone)]
    #[non_exhaustive]
    pub struct JoinRequest {
        pub topic_id: TopicId,
    }

    #[rpc_requests(message = GossipMessage)]
    #[derive(Debug, Serialize, Deserialize)]
    pub enum Request {
        #[rpc(tx=mpsc::Sender<super::ProtoMessage>, rx=mpsc::Receiver<super::ProtoMessage>)]
        Join(JoinRequest),
    }
}

/// Error emitted when the gossip actor stopped.
#[derive(Debug, Snafu)]
pub struct ActorStoppedError;

#[derive(strum::Display)]
enum ActorToTopic {
    Api(ApiJoinRequest),
    Connected {
        remote: NodeId,
        tx: Guarded<channel::mpsc::Sender<ProtoMessage>>,
        rx: Guarded<channel::mpsc::Receiver<ProtoMessage>>,
    },
    ConnectionFailed(NodeId),
}

type ApiJoinRequest = WithChannels<api::JoinRequest, api::Request>;
type ApiRecvStream = BoxStream<Result<api::Command, RecvError>>;
type RemoteRecvStream = BoxStream<(NodeId, Result<Option<ProtoMessage>, RecvError>)>;
type AcceptRemoteRequestsStream =
    MergeUnbounded<BoxStream<(NodeId, std::io::Result<Guarded<net_proto::GossipMessage>>)>>;

struct Actor {
    me: NodeId,
    endpoint: Endpoint,
    alpn: Bytes,
    config: Config,
    local_rx: mpsc::Receiver<LocalActorMessage>,
    local_tx: mpsc::Sender<LocalActorMessage>,
    api_rx: mpsc::Receiver<api::RpcMessage>,
    topics: HashMap<TopicId, TopicHandle>,
    pending_remotes_with_topics: HashMap<NodeId, HashSet<TopicId>>,
    topic_tasks: JoinSet<TopicActor>,
    remotes: HashMap<NodeId, RemoteState>,
    close_connections: JoinSet<(NodeId, Connection)>,
    dialer: Dialer,
    our_peer_data: n0_watcher::Watchable<PeerData>,
    metrics: Arc<Metrics>,
    node_addr_updates: BoxStream<NodeAddr>,
    accepting: AcceptRemoteRequestsStream,
    discovery: GossipDiscovery,
}

impl Actor {
    fn new(
        endpoint: Endpoint,
        config: Config,
        alpn: Option<Bytes>,
        metrics: Arc<Metrics>,
    ) -> (
        mpsc::Sender<api::RpcMessage>,
        mpsc::Sender<LocalActorMessage>,
        Self,
    ) {
        let (api_tx, api_rx) = tokio::sync::mpsc::channel(16);
        let (local_tx, local_rx) = tokio::sync::mpsc::channel(16);

        let me = endpoint.node_id();
        let node_addr_updates = endpoint.watch_node_addr().stream();
        let discovery = GossipDiscovery::default();
        endpoint.discovery().add(discovery.clone());
        let initial_peer_data = AddrInfo::from(endpoint.node_addr()).encode();
        // let peer_data = endpoint
        //     .watch_node_addr()
        //     .map(|addr| AddrInfo::from(addr).encode())
        //     .unwrap();
        (
            api_tx,
            local_tx.clone(),
            Actor {
                endpoint,
                me,
                config,
                api_rx,
                local_tx,
                local_rx,
                node_addr_updates: Box::pin(node_addr_updates),
                dialer: Dialer::default(),
                our_peer_data: Watchable::new(initial_peer_data),
                alpn: alpn.unwrap_or_else(|| crate::ALPN.to_vec().into()),
                metrics: metrics.clone(),
                topics: Default::default(),
                pending_remotes_with_topics: Default::default(),
                remotes: Default::default(),
                close_connections: JoinSet::new(),
                topic_tasks: JoinSet::new(),
                accepting: Default::default(),
                discovery,
            },
        )
    }

    async fn run(mut self) {
        while self.tick().await {}
    }

    #[cfg(test)]
    #[instrument("gossip", skip_all, fields(me=%self.me.fmt_short()))]
    pub(crate) async fn finish(self) {
        self.run().await
    }

    #[cfg(test)]
    #[instrument("gossip", skip_all, fields(me=%self.me.fmt_short()))]
    pub(crate) async fn steps(&mut self, n: usize) -> Result<(), ActorStoppedError> {
        for _ in 0..n {
            if !self.tick().await {
                return Err(ActorStoppedError);
            }
        }
        Ok(())
    }

    async fn tick(&mut self) -> bool {
        trace!("wait for tick");
        self.metrics.actor_tick_main.inc();
        tokio::select! {
            addr = self.node_addr_updates.next() => {
                trace!("tick: node_addr_update");
                match addr {
                    None => {
                        warn!("address stream returned None - endpoint has shut down");
                        false
                    }
                    Some(addr) => {
                        let data = AddrInfo::from(addr).encode();
                        self.our_peer_data.set(data).ok();
                        true
                    }
                }
            }
            Some(msg) = self.local_rx.recv() => {
                trace!("tick: local_rx {msg:?}");
                match msg {
                    LocalActorMessage::HandleConnection(node_id, connection) => {
                        self.handle_remote_connection(node_id, Ok(connection), Direction::Accept).await;
                    }
                    LocalActorMessage::Connect(node_id, topic_id) => {
                        self.connect(node_id, topic_id);
                    }
                    LocalActorMessage::SetPeerData(node_id, data) => {
                        match AddrInfo::decode(&data) {
                            Err(err) => warn!(remote=%node_id.fmt_short(), ?err, len=data.inner().len(), "Failed to decode peer data"),
                            Ok(info) => {
                                debug!(peer = ?node_id, "add known addrs: {info:?}");
                                let node_addr = info.into_node_addr(node_id);
                                self.discovery.add(node_addr);
                            }
                        }
                    }
                }
                true
            }
            Some((node_id, res)) = self.dialer.next(), if !self.dialer.is_empty() => {
                trace!(remote=%node_id.fmt_short(), ok=res.is_ok(), "tick: dialed");
                self.handle_remote_connection(node_id, res, Direction::Dial).await;
                true
            }
            Some((node_id, res)) = self.accepting.next(), if !self.accepting.is_empty() => {
                trace!(remote=%node_id.fmt_short(), res=?res.as_ref().map(|_| ()), "tick: accepting");
                match res {
                    Ok(request) => self.handle_remote_message(node_id, request).await,
                    Err(reason) => {
                        debug!(remote=%node_id.fmt_short(), ?reason, "accept loop for remote closed");
                    }
                }
                true
            }
            msg = self.api_rx.recv() => {
                trace!(some=msg.is_some(), "tick: api_rx");
                match msg {
                    Some(msg) => {
                        self.handle_api_message(msg).await;
                        true
                    }
                    None => {
                        trace!("all api senders dropped, stop actor");
                        false
                    }
                }
            }
            Some(res) = self.close_connections.join_next(), if !self.close_connections.is_empty() => {
                let (node_id, connection) = res.expect("connection task panicked");
                trace!(remote=%node_id.fmt_short(), "tick: connection closed");
                if let Some(state) = self.remotes.get(&node_id) {
                    if state.same_connection(&connection) {
                        self.remotes.remove(&node_id);
                    }
                }
                true
            }
            Some(actor) = self.topic_tasks.join_next(), if !self.topic_tasks.is_empty() => {
                let actor = actor.expect("topic actor task panicked");
                trace!(topic=%actor.topic_id.fmt_short(), "tick: topic actor finished");
                self.topics.remove(&actor.topic_id);
                true
            }
            else => unreachable!("reached else arm, but all fallible cases should be handled"),
        }
    }

    #[cfg(test)]
    fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    fn drain_pending_dials(
        &mut self,
        remote: &NodeId,
    ) -> impl Iterator<Item = (TopicId, &TopicHandle)> {
        self.pending_remotes_with_topics
            .remove(remote)
            .into_iter()
            .flatten()
            .flat_map(|topic_id| self.topics.get(&topic_id).map(|handle| (topic_id, handle)))
    }

    fn connect(&mut self, remote: NodeId, topic_id: TopicId) {
        let Some(handle) = self.topics.get(&topic_id) else {
            return;
        };
        if let Some(state) = self.remotes.get(&remote) {
            let tx = handle.tx.clone();
            let state = state.clone();
            // TODO: Track task?
            task::spawn(async move {
                let msg = state.open_topic(topic_id).await;
                tx.send(msg).await.ok();
            });
        } else {
            self.dialer
                .queue_dial(&self.endpoint, remote, self.alpn.clone());
            self.pending_remotes_with_topics
                .entry(remote)
                .or_default()
                .insert(topic_id);
        }
    }

    #[instrument("connection", skip_all, fields(remote=%remote.fmt_short()))]
    async fn handle_remote_connection(
        &mut self,
        remote: NodeId,
        res: Result<Connection, ConnectError>,
        direction: Direction,
    ) {
        match (res.as_ref(), direction) {
            (Ok(_), Direction::Dial) => inc(&self.metrics.peers_dialed_success),
            (Err(_), Direction::Dial) => inc(&self.metrics.peers_dialed_failure),
            (Ok(_), Direction::Accept) => inc(&self.metrics.peers_accepted),
            (Err(_), Direction::Accept) => {}
        }
        let connection = match res {
            Err(err) => {
                debug!(?err, "Connection failed");
                for (_, handle) in self.drain_pending_dials(&remote) {
                    handle
                        .send(ActorToTopic::ConnectionFailed(remote))
                        .await
                        .ok();
                }
                return;
            }
            Ok(connection) => connection,
        };

        let state = RemoteState::new(remote, connection.clone(), direction);

        // Open requests for pending topics.
        for (topic_id, handle) in self.drain_pending_dials(&remote) {
            let tx = handle.tx.clone();
            let state = state.clone();
            task::spawn(
                async move {
                    let msg = state.open_topic(topic_id).await;
                    tx.send(msg).await.ok();
                }
                .instrument(tracing::Span::current()),
            );
        }

        // Read incoming requests.
        let counter = state.counter.clone();
        self.accepting.push(Box::pin(
            accept_stream::<net_proto::Request>(connection.clone())
                .map(move |req| (remote, req.map(|r| counter.guard(r)))),
        ));

        // Close on idle (if dialed) or await close (if accepted).
        let counter = state.counter.clone();
        let fut = async move {
            match direction {
                Direction::Dial => {
                    counter.idle_for(Duration::from_millis(500)).await;
                    info!("close connection (from dial): unused");
                    connection.close(1u32.into(), b"idle");
                }
                Direction::Accept => {
                    let reason = connection.closed().await;
                    info!(?reason, "connection closed (from accept)")
                }
            }
            (remote, connection)
        };
        self.close_connections
            .spawn(fut.instrument(error_span!("conn", remote=%remote.fmt_short())));

        self.remotes.insert(remote, state);
    }

    #[instrument("request", skip_all, fields(remote=%remote.fmt_short()))]
    async fn handle_remote_message(&mut self, remote: NodeId, request: Guarded<GossipMessage>) {
        let (request, guard) = request.split();
        let (topic_id, request) = match request {
            GossipMessage::Join(req) => (req.inner.topic_id, req),
        };
        if let Some(topic) = self.topics.get(&topic_id) {
            if let Err(_err) = topic
                .send(ActorToTopic::Connected {
                    remote,
                    tx: Guarded::new(request.tx, guard.clone()),
                    rx: Guarded::new(request.rx, guard.clone()),
                })
                .await
            {
                warn!(topic=%topic_id.fmt_short(), "Topic actor dead");
            }
        } else {
            debug!(topic=%topic_id.fmt_short(), "ignore request: unknown topic");
        }
    }

    async fn handle_api_message(&mut self, msg: api::RpcMessage) {
        let (topic_id, msg) = match msg {
            api::RpcMessage::Join(msg) => (msg.inner.topic_id, msg),
        };
        let topic = self.topics.entry(topic_id).or_insert_with(|| {
            let (handle, actor) = TopicHandle::new(
                self.me,
                topic_id,
                self.config.clone(),
                self.local_tx.clone(),
                self.our_peer_data.watch(),
                self.metrics.clone(),
            );
            self.topic_tasks.spawn(
                actor
                    .run()
                    .instrument(error_span!("topic", topic=%topic_id.fmt_short())),
            );
            handle
        });
        if topic.send(ActorToTopic::Api(msg)).await.is_err() {
            warn!(topic=%topic_id.fmt_short(), "Topic actor dead");
        }
    }
}

#[derive(Clone)]
struct RemoteState {
    node_id: NodeId,
    conn_id: usize,
    client: irpc::Client<net_proto::Request>,
    #[allow(dead_code)]
    direction: Direction,
    counter: ConnectionCounter,
}

impl RemoteState {
    fn new(node_id: NodeId, connection: Connection, direction: Direction) -> Self {
        let conn_id = connection.stable_id();
        let irpc_conn = IrohRemoteConnection::new(connection);
        let client = irpc::Client::boxed(irpc_conn);
        let counter = ConnectionCounter::new();
        RemoteState {
            client,
            direction,
            conn_id,
            counter,
            node_id,
        }
    }

    fn same_connection(&self, conn: &Connection) -> bool {
        self.conn_id == conn.stable_id()
    }

    async fn open_topic(&self, topic_id: TopicId) -> ActorToTopic {
        let guard = self.counter.get_one();
        let req = net_proto::JoinRequest { topic_id };
        match self.client.bidi_streaming(req.clone(), 64, 64).await {
            Ok((tx, rx)) => ActorToTopic::Connected {
                remote: self.node_id,
                tx: Guarded::new(tx, guard.clone()),
                rx: Guarded::new(rx, guard),
            },
            Err(err) => {
                warn!(?topic_id, ?err, "failed to open stream with remote");
                ActorToTopic::ConnectionFailed(self.node_id)
            }
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum Direction {
    Dial,
    Accept,
}

struct TopicHandle {
    tx: mpsc::Sender<ActorToTopic>,
    #[cfg(test)]
    joined: Arc<AtomicBool>,
}

impl TopicHandle {
    fn new(
        me: NodeId,
        topic_id: TopicId,
        config: proto::Config,
        to_actor_tx: mpsc::Sender<LocalActorMessage>,
        peer_data: Direct<PeerData>,
        metrics: Arc<Metrics>,
    ) -> (Self, TopicActor) {
        let (tx, rx) = mpsc::channel(16);
        // TODO: peer_data
        let state = State::new(me, None, config);
        #[cfg(test)]
        let joined = Arc::new(AtomicBool::new(false));
        let (forward_event_tx, _) = broadcast::channel(512);
        let actor = TopicActor {
            topic_id,
            state,
            actor_rx: rx,
            to_actor_tx,
            peer_data,
            forward_event_tx,
            metrics,
            init: false,
            #[cfg(test)]
            joined: joined.clone(),
            timers: Default::default(),
            neighbors: Default::default(),
            out_events: Default::default(),
            api_receivers: Default::default(),
            remote_senders: Default::default(),
            remote_receivers: Default::default(),
            drop_peers_queue: Default::default(),
            forward_event_tasks: Default::default(),
        };
        let handle = Self {
            tx,
            #[cfg(test)]
            joined,
        };
        (handle, actor)
    }

    async fn send(&self, msg: ActorToTopic) -> Result<(), mpsc::error::SendError<ActorToTopic>> {
        self.tx.send(msg).await
    }

    #[cfg(test)]
    fn joined(&self) -> bool {
        self.joined.load(std::sync::atomic::Ordering::Relaxed)
    }
}

struct TopicActor {
    topic_id: TopicId,
    to_actor_tx: mpsc::Sender<LocalActorMessage>,
    state: State,
    actor_rx: mpsc::Receiver<ActorToTopic>,
    timers: Timers<Timer>,
    neighbors: BTreeSet<NodeId>,
    peer_data: Direct<PeerData>,
    out_events: VecDeque<OutEvent>,
    api_receivers: MergeUnbounded<ApiRecvStream>,
    remote_senders: HashMap<NodeId, MaybeSender>,
    remote_receivers: MergeUnbounded<RemoteRecvStream>,
    forward_event_tx: broadcast::Sender<ProtoEvent>,
    forward_event_tasks: JoinSet<()>,
    #[cfg(test)]
    joined: Arc<AtomicBool>,
    init: bool,
    drop_peers_queue: HashSet<NodeId>,
    metrics: Arc<Metrics>,
}

impl TopicActor {
    pub async fn run(mut self) -> Self {
        self.metrics.topics_joined.inc();
        let peer_data = self.peer_data.clone().stream();
        tokio::pin!(peer_data);
        loop {
            tokio::select! {
                Some(msg) = self.actor_rx.recv() => {
                    trace!("tick: actor_rx {msg}");
                    self.handle_actor_message(msg).await;
                },
                Some(cmd) = self.api_receivers.next(), if !self.api_receivers.is_empty() => {
                    self.handle_api_command(cmd).await;
                }
                Some((remote, message)) = self.remote_receivers.next(), if !self.remote_receivers.is_empty() => {
                    trace!(remote=%remote.fmt_short(), msg=?message, "tick: remote_rx");
                    self.handle_remote_message(remote, message).await;
                }
                Some(data) = peer_data.next() => {
                    self.handle_in_event(InEvent::UpdatePeerData(data)).await;
                }
                _ = self.timers.wait_next() => {
                    let now = Instant::now();
                    while let Some((_instant, timer)) = self.timers.pop_before(now) {
                        self.handle_in_event(InEvent::TimerExpired(timer)).await;
                    }
                }
                _ = self.forward_event_tasks.join_next(), if !self.forward_event_tasks.is_empty() => {}
                else => break,
            }

            if !self.drop_peers_queue.is_empty() {
                let now = Instant::now();
                for peer in self.drop_peers_queue.drain() {
                    self.out_events
                        .extend(self.state.handle(InEvent::PeerDisconnected(peer), now));
                }
                self.process_out_events(now).await;
            }

            if self.to_actor_tx.is_closed() {
                warn!("Channel to main actor closed: abort topic loop");
                break;
            }
            if self.init && self.api_receivers.is_empty() && self.forward_event_tasks.is_empty() {
                debug!("Closing topic: All API subscribers dropped");
                break;
            }
        }
        self.metrics.topics_quit.inc();
        self
    }

    async fn handle_actor_message(&mut self, msg: ActorToTopic) {
        match msg {
            ActorToTopic::Connected { remote, rx, tx } => {
                self.remote_receivers
                    .push(Box::pin(into_stream(rx).map(move |msg| (remote, msg))));
                let sender = self.remote_senders.entry(remote).or_default();
                if let Err(err) = sender.init(tx).await {
                    warn!("Remote failed while pushing queued messages: {err:?}");
                }
            }
            ActorToTopic::Api(req) => {
                self.init = true;
                let WithChannels { inner, tx, rx, .. } = req;
                let initial_neighbors = self.neighbors.clone().into_iter();
                self.forward_event_tasks.spawn(
                    forward_events(tx, self.forward_event_tx.subscribe(), initial_neighbors)
                        .instrument(tracing::Span::current()),
                );
                self.api_receivers.push(Box::pin(into_stream2(rx)));
                self.handle_in_event(InEvent::Command(Command::Join(
                    inner.bootstrap.into_iter().collect(),
                )))
                .await;
            }
            ActorToTopic::ConnectionFailed(node_id) => {
                self.handle_in_event(InEvent::PeerDisconnected(node_id))
                    .await
            }
        }
    }

    async fn handle_remote_message(
        &mut self,
        remote: NodeId,
        message: Result<Option<ProtoMessage>, RecvError>,
    ) {
        let event = match message {
            Ok(Some(message)) => InEvent::RecvMessage(remote, message),
            Ok(None) => {
                debug!(remote=%remote.fmt_short(), "Recv stream from remote closed");
                InEvent::PeerDisconnected(remote)
            }
            Err(err) => {
                warn!(remote=%remote.fmt_short(), ?err, "Recv stream from remote failed");
                InEvent::PeerDisconnected(remote)
            }
        };
        self.handle_in_event(event).await;
    }

    async fn handle_api_command(&mut self, command: Result<api::Command, RecvError>) {
        let Ok(command) = command else {
            return;
        };
        trace!("tick: api command {command}");
        self.handle_in_event(InEvent::Command(command.into())).await;
    }

    async fn handle_in_event(&mut self, event: InEvent) {
        trace!("tick: in event {event:?}");
        let now = Instant::now();
        self.metrics.track_in_event(&event);
        self.out_events.extend(self.state.handle(event, now));
        self.process_out_events(now).await;
    }

    async fn process_out_events(&mut self, now: Instant) {
        while let Some(event) = self.out_events.pop_front() {
            trace!("tick: out event {event:?}");
            self.metrics.track_out_event(&event);
            match event {
                OutEvent::SendMessage(node_id, message) => {
                    self.send(node_id, message).await;
                }
                OutEvent::EmitEvent(event) => {
                    self.handle_event(event);
                }
                OutEvent::ScheduleTimer(delay, timer) => {
                    self.timers.insert(now + delay, timer);
                }
                OutEvent::DisconnectPeer(node_id) => {
                    self.remote_senders.remove(&node_id);
                }
                OutEvent::PeerData(node_id, peer_data) => {
                    self.to_actor_tx
                        .send(LocalActorMessage::SetPeerData(node_id, peer_data))
                        .await
                        .ok();
                }
            }
        }
    }

    #[instrument(skip_all, fields(remote=%remote.fmt_short()))]
    async fn send(&mut self, remote: NodeId, message: ProtoMessage) {
        let sender = match self.remote_senders.entry(remote) {
            hash_map::Entry::Occupied(entry) => entry.into_mut(),
            hash_map::Entry::Vacant(entry) => {
                debug!("requesting new connection");
                self.to_actor_tx
                    .send(LocalActorMessage::Connect(remote, self.topic_id))
                    .await
                    .ok();
                entry.insert(Default::default())
            }
        };
        if let Err(err) = sender.send(message).await {
            warn!(?err, remote=%remote.fmt_short(), "failed to send message");
            self.drop_peers_queue.insert(remote);
        }
    }

    fn handle_event(&mut self, event: ProtoEvent) {
        match &event {
            ProtoEvent::NeighborUp(n) => {
                #[cfg(test)]
                self.joined
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                self.neighbors.insert(*n);
            }
            ProtoEvent::NeighborDown(n) => {
                self.neighbors.remove(n);
            }
            ProtoEvent::Received(_) => {}
        }
        self.forward_event_tx.send(event).ok();
    }
}

async fn forward_events(
    tx: channel::mpsc::Sender<api::Event>,
    mut sub: broadcast::Receiver<ProtoEvent>,
    initial_neighbors: impl Iterator<Item = NodeId>,
) {
    for neighbor in initial_neighbors {
        if let Err(_err) = tx.send(api::Event::NeighborUp(neighbor)).await {
            break;
        }
    }
    loop {
        let event = tokio::select! {
            biased;
            event = sub.recv() => event,
            _ = tx.closed() => break
        };
        let event: api::Event = match event {
            Ok(event) => event.into(),
            Err(broadcast::error::RecvError::Lagged(_)) => api::Event::Lagged,
            Err(broadcast::error::RecvError::Closed) => break,
        };
        if let Err(_err) = tx.send(event).await {
            break;
        }
    }
}

#[derive(Debug)]
enum MaybeSender {
    Active(Guarded<channel::mpsc::Sender<ProtoMessage>>),
    Pending(Vec<ProtoMessage>),
}

impl MaybeSender {
    async fn send(&mut self, message: ProtoMessage) -> Result<(), channel::SendError> {
        match self {
            Self::Active(sender) => sender.send(message).await,
            Self::Pending(messages) => {
                messages.push(message);
                Ok(())
            }
        }
    }

    async fn init(
        &mut self,
        sender: Guarded<channel::mpsc::Sender<ProtoMessage>>,
    ) -> Result<(), channel::SendError> {
        debug!("Initializing new sender");
        *self = match self {
            Self::Active(_old) => {
                debug!("Dropping old sender");
                Self::Active(sender)
            }
            Self::Pending(queue) => {
                debug!("Sending {} queued messages", queue.len());
                for msg in queue.drain(..) {
                    sender.send(msg).await?;
                }
                Self::Active(sender)
            }
        };
        Ok(())
    }
}

impl Default for MaybeSender {
    fn default() -> Self {
        Self::Pending(Vec::new())
    }
}

// TODO: Upstream to irpc: This differs from Receiver::into_stream: it returns
// None after the first error, whereas upstream would loop on the error
fn into_stream<T: irpc::RpcMessage>(
    receiver: impl DerefMut<Target = channel::mpsc::Receiver<T>> + Send + Sync + 'static,
) -> impl Stream<Item = Result<Option<T>, RecvError>> + Send + Sync + 'static {
    n0_future::stream::unfold(Some(receiver), |recv| async move {
        let mut recv = recv?;
        let res = recv.recv().await;
        match res {
            Err(err) => Some((Err(err), None)),
            Ok(Some(res)) => Some((Ok(Some(res)), Some(recv))),
            Ok(None) => Some((Ok(None), None)),
        }
    })
}

fn into_stream2<T: irpc::RpcMessage>(
    receiver: channel::mpsc::Receiver<T>,
) -> impl Stream<Item = Result<T, RecvError>> + Send + Sync + 'static {
    n0_future::stream::unfold(Some(receiver), |recv| async move {
        let mut recv = recv?;
        match recv.recv().await {
            Err(err) => Some((Err(err), None)),
            Ok(Some(res)) => Some((Ok(res), Some(recv))),
            Ok(None) => None,
        }
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{future::Future, time::Duration};

    use bytes::Bytes;
    use futures_concurrency::future::TryJoin;
    use iroh::{
        discovery::static_provider::StaticProvider, endpoint::BindError, protocol::Router,
        NodeAddr, RelayMap, RelayMode, SecretKey,
    };
    use n0_snafu::{Result, ResultExt};
    use rand::{CryptoRng, Rng, SeedableRng};
    use tokio::{spawn, time::timeout};
    use tokio_util::sync::CancellationToken;
    use tracing::info;
    use tracing_test::traced_test;

    use super::*;
    use crate::{
        api::{ApiError, Event, GossipReceiver, GossipSender},
        ALPN,
    };

    impl Gossip {
        pub(super) async fn t_new(
            rng: &mut rand_chacha::ChaCha12Rng,
            config: proto::Config,
            relay_map: RelayMap,
            cancel: &CancellationToken,
        ) -> n0_snafu::Result<(Self, Endpoint, impl Future<Output = ()>, impl Drop)> {
            let (gossip, actor, ep_handle) =
                Gossip::t_new_with_actor(rng, config, relay_map, cancel).await?;
            let ep = actor.endpoint().clone();
            let me = ep.node_id().fmt_short();
            let actor_handle =
                task::spawn(actor.run().instrument(tracing::error_span!("gossip", %me)));
            Ok((gossip, ep, ep_handle, AbortOnDropHandle::new(actor_handle)))
        }
        pub(super) async fn t_new_with_actor(
            rng: &mut rand_chacha::ChaCha12Rng,
            config: proto::Config,
            relay_map: RelayMap,
            cancel: &CancellationToken,
        ) -> n0_snafu::Result<(Self, Actor, impl Future<Output = ()>)> {
            let endpoint = Endpoint::builder()
                .secret_key(SecretKey::generate(rng))
                .relay_mode(RelayMode::Custom(relay_map))
                .insecure_skip_relay_cert_verify(true)
                .bind()
                .await?;

            endpoint.online().await;
            let (gossip, mut actor) = Gossip::new_with_actor(endpoint.clone(), config, None);
            actor.node_addr_updates = Box::pin(n0_future::stream::pending());
            let router = Router::builder(endpoint)
                .accept(GOSSIP_ALPN, gossip.clone())
                .spawn();
            let cancel = cancel.clone();
            let router_task = tokio::task::spawn(async move {
                cancel.cancelled().await;
                router.shutdown().await.ok();
                drop(router);
            });
            let router_fut = async move {
                router_task.await.expect("router task panicked");
            };
            Ok((gossip, actor, router_fut))
        }
    }

    pub(crate) async fn create_endpoint(
        rng: &mut rand_chacha::ChaCha12Rng,
        relay_map: RelayMap,
        static_provider: Option<StaticProvider>,
    ) -> Result<Endpoint, BindError> {
        let ep = Endpoint::builder()
            .secret_key(SecretKey::generate(rng))
            .alpns(vec![ALPN.to_vec()])
            .relay_mode(RelayMode::Custom(relay_map))
            .insecure_skip_relay_cert_verify(true)
            .bind()
            .await?;

        if let Some(static_provider) = static_provider {
            ep.discovery().add(static_provider);
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
                        let connection = connecting.await.e()?;
                        let remote_node_id = connection.remote_node_id()?;
                        gossip.handle_connection(remote_node_id, connection).await?
                    }
                }
            }
        }
        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn gossip_net_smoke() {
        let mut rng = rand_chacha::ChaCha12Rng::seed_from_u64(1);
        let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();

        let static_provider = StaticProvider::new();

        let ep1 = create_endpoint(&mut rng, relay_map.clone(), Some(static_provider.clone()))
            .await
            .unwrap();
        let ep2 = create_endpoint(&mut rng, relay_map.clone(), Some(static_provider.clone()))
            .await
            .unwrap();
        let ep3 = create_endpoint(&mut rng, relay_map.clone(), Some(static_provider.clone()))
            .await
            .unwrap();

        let go1 = Gossip::builder().spawn(ep1.clone());
        let go2 = Gossip::builder().spawn(ep2.clone());
        let go3 = Gossip::builder().spawn(ep3.clone());
        debug!("peer1 {:?}", ep1.node_id());
        debug!("peer2 {:?}", ep2.node_id());
        debug!("peer3 {:?}", ep3.node_id());
        let pi1 = ep1.node_id();
        let pi2 = ep2.node_id();

        let cancel = CancellationToken::new();
        let tasks = [
            spawn(endpoint_loop(ep1.clone(), go1.clone(), cancel.clone())),
            spawn(endpoint_loop(ep2.clone(), go2.clone(), cancel.clone())),
            spawn(endpoint_loop(ep3.clone(), go3.clone(), cancel.clone())),
        ];

        debug!("----- adding peers  ----- ");
        let topic: TopicId = blake3::hash(b"foobar").into();

        let addr1 = NodeAddr::new(pi1).with_relay_url(relay_url.clone());
        let addr2 = NodeAddr::new(pi2).with_relay_url(relay_url);
        static_provider.add_node_info(addr1.clone());
        static_provider.add_node_info(addr2.clone());

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

        // publish messages on node1
        let pub1 = spawn(async move {
            for i in 0..len {
                let message = format!("hi{i}");
                info!("go1 broadcast: {message:?}");
                sink1.broadcast(message.into_bytes().into()).await.unwrap();
                tokio::time::sleep(Duration::from_micros(1)).await;
            }
        });

        // wait for messages on node2
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

        // wait for messages on node3
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

        let expected: Vec<Bytes> = (0..len)
            .map(|i| Bytes::from(format!("hi{i}").into_bytes()))
            .collect();
        assert_eq!(recv2, expected);
        assert_eq!(recv3, expected);

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
    /// - Subscribe both nodes to the same topic. The first node will subscribe twice and connect
    ///   to the second node. The second node will subscribe without bootstrap.
    /// - Ensure that the first node removes the subscription iff all topic handles have been
    ///   dropped.
    // NOTE: this is a regression test.
    #[tokio::test]
    #[traced_test]
    async fn subscription_cleanup() -> Result {
        let rng = &mut rand_chacha::ChaCha12Rng::seed_from_u64(1);
        let ct = CancellationToken::new();
        let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();

        // create the first node with a manual actor loop
        let (go1, mut actor, ep1_handle) =
            Gossip::t_new_with_actor(rng, Default::default(), relay_map.clone(), &ct).await?;

        // create the second node with the usual actor loop
        let (go2, ep2, ep2_handle, _test_actor_handle) =
            Gossip::t_new(rng, Default::default(), relay_map, &ct).await?;

        let node_id1 = actor.endpoint().node_id();
        let node_id2 = ep2.node_id();
        tracing::info!(
            node_1 = %node_id1.fmt_short(),
            node_2 = %node_id2.fmt_short(),
            "nodes ready"
        );

        let topic: TopicId = blake3::hash(b"subscription_cleanup").into();
        tracing::info!(%topic, "joining");

        // create the tasks for each gossip instance:
        // - second node subscribes once without bootstrap and listens to events
        // - first node subscribes twice with the second node as bootstrap. This is done on command
        //   from the main task (this)

        // second node
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
                Ok::<_, n0_snafu::Error>(())
            };

            tokio::select! {
                _ = ct2.cancelled() => Ok(()),
                res = subscribe_fut => res,
            }
        }
        .instrument(tracing::debug_span!("node_2", node_id2=%node_id2.fmt_short()));
        let go2_handle = task::spawn(go2_task);

        // first node
        let addr2 = NodeAddr::new(node_id2).with_relay_url(relay_url);
        let static_provider = StaticProvider::new();
        static_provider.add_node_info(addr2);
        actor.endpoint().discovery().add(static_provider);
        // we use a channel to signal advancing steps to the task
        let (tx, mut rx) = mpsc::channel::<()>(1);
        let ct1 = ct.clone();
        let go1_task = async move {
            // first subscribe is done immediately
            tracing::info!("subscribing the first time");
            let sub_1a = go1.subscribe_and_join(topic, vec![node_id2]).await?;

            // wait for signal to subscribe a second time
            rx.recv().await.expect("signal for second subscribe");
            tracing::info!("subscribing a second time");
            let sub_1b = go1.subscribe_and_join(topic, vec![node_id2]).await?;
            drop(sub_1a);

            // wait for signal to drop the second handle as well
            rx.recv().await.expect("signal for second subscribe");
            tracing::info!("dropping all handles");
            drop(sub_1b);

            // wait for cancellation
            ct1.cancelled().await;
            drop(go1);

            Ok::<_, n0_snafu::Error>(())
        }
        .instrument(tracing::debug_span!("node_1", node_id1 = %node_id1.fmt_short()));
        let go1_handle = task::spawn(go1_task);

        // advance and check that the topic is now subscribed
        actor.steps(4).await?; // api_rx subscribe;
                               // internal_rx connection request (from topic actor);
                               // dialer connected;
                               // internal_rx update peer data (from topic actor);
        tracing::info!("subscribe and join done, should be joined");
        let state = actor.topics.get(&topic).expect("get registered topic");
        assert!(state.joined());

        // signal the second subscribe, we should remain subscribed
        tx.send(()).await.e()?;
        actor.steps(1).await?; // api_rx subscribe;
        let state = actor.topics.get(&topic).expect("get registered topic");
        assert!(state.joined());

        // signal to drop the second handle, the topic should no longer be subscribed
        tx.send(()).await.e()?;
        actor.steps(1).await?; // topic task finished

        assert!(!actor.topics.contains_key(&topic));

        // cleanup and ensure everything went as expected
        ct.cancel();
        let wait = Duration::from_secs(5);
        timeout(wait, ep1_handle).await.e()?;
        timeout(wait, ep2_handle).await.e()?;
        timeout(wait, go1_handle).await.e()?.e()??;
        timeout(wait, go2_handle).await.e()?.e()??;
        timeout(wait, actor.finish()).await.e()?;

        Ok(())
    }

    /// Test that nodes can reconnect to each other.
    ///
    /// This test will create two nodes subscribed to the same topic. The second node will
    /// unsubscribe and then resubscribe and connection between the nodes should succeed both
    /// times.
    // NOTE: This is a regression test
    #[tokio::test(flavor = "multi_thread")]
    #[traced_test]
    async fn can_reconnect() -> Result {
        let rng = &mut rand_chacha::ChaCha12Rng::seed_from_u64(1);
        let ct = CancellationToken::new();
        let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();

        let (go1, ep1, ep1_handle, _test_actor_handle1) =
            Gossip::t_new(rng, Default::default(), relay_map.clone(), &ct).await?;

        let (go2, ep2, ep2_handle, _test_actor_handle2) =
            Gossip::t_new(rng, Default::default(), relay_map, &ct).await?;

        let node_id1 = ep1.node_id();
        let node_id2 = ep2.node_id();
        tracing::info!(
            node_1 = %node_id1.fmt_short(),
            node_2 = %node_id2.fmt_short(),
            "nodes ready"
        );

        let topic: TopicId = blake3::hash(b"can_reconnect").into();
        tracing::info!(%topic, "joining");

        // channel used to signal the second gossip instance to advance the test
        let (tx, mut rx) = mpsc::channel::<()>(1);
        let addr1 = NodeAddr::new(node_id1).with_relay_url(relay_url.clone());
        let static_provider = StaticProvider::new();
        static_provider.add_node_info(addr1);
        ep2.discovery().add(static_provider.clone());
        let go2_task = async move {
            let mut sub = go2.subscribe(topic, Vec::new()).await?;
            sub.joined().await?;

            rx.recv().await.expect("signal to unsubscribe");
            tracing::info!("unsubscribing");
            drop(sub);

            rx.recv().await.expect("signal to subscribe again");
            tracing::info!("resubscribing");
            let mut sub = go2.subscribe(topic, vec![node_id1]).await?;

            sub.joined().await?;

            Result::<_, ApiError>::Ok(())
        }
        .instrument(tracing::debug_span!("node_2", node_id2=%node_id2.fmt_short()));
        let go2_handle = task::spawn(go2_task);

        let addr2 = NodeAddr::new(node_id2).with_relay_url(relay_url);
        static_provider.add_node_info(addr2);
        ep1.discovery().add(static_provider);

        let mut sub = go1.subscribe(topic, vec![node_id2]).await?;
        // wait for subscribed notification
        sub.joined().await?;
        info!("go1 joined");

        // signal node_2 to unsubscribe
        tx.send(()).await.e()?;

        info!("wait for neighbor down");
        // we should receive a Neighbor down event
        let conn_timeout = Duration::from_millis(1000);
        let ev = timeout(conn_timeout, sub.try_next()).await.e()??;
        assert_eq!(ev, Some(Event::NeighborDown(node_id2)));
        tracing::info!("node 2 left");

        // signal node_2 to subscribe again
        tx.send(()).await.e()?;

        info!("wait for neighbor up");
        let conn_timeout = Duration::from_millis(1000);
        let ev = timeout(conn_timeout, sub.try_next()).await.e()??;
        assert_eq!(ev, Some(Event::NeighborUp(node_id2)));
        tracing::info!("node 2 rejoined!");

        // wait for go2 to also be rejoined, then the task terminates
        let wait = Duration::from_secs(2);
        timeout(wait, go2_handle).await.e()?.e()??;
        ct.cancel();
        // cleanup and ensure everything went as expected
        timeout(wait, ep1_handle).await.e()?;
        timeout(wait, ep2_handle).await.e()?;

        Ok(())
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
            let ep = Endpoint::builder()
                .secret_key(secret_key)
                .relay_mode(RelayMode::Custom(relay_map))
                .insecure_skip_relay_cert_verify(true)
                .bind()
                .await?;
            let gossip = Gossip::builder().spawn(ep.clone());
            let router = Router::builder(ep).accept(ALPN, gossip.clone()).spawn();
            Ok((router, gossip))
        }

        /// Spawns a gossip node, and broadcasts a single message, then sleep until cancelled externally.
        async fn broadcast_once(
            secret_key: SecretKey,
            relay_map: RelayMap,
            bootstrap_addr: NodeAddr,
            topic_id: TopicId,
            message: String,
        ) -> Result {
            let (router, gossip) = spawn_gossip(secret_key, relay_map).await?;
            info!(node_id = %router.endpoint().node_id().fmt_short(), "broadcast node spawned");
            let bootstrap = vec![bootstrap_addr.node_id];
            let static_provider = StaticProvider::new();
            static_provider.add_node_info(bootstrap_addr);
            router.endpoint().discovery().add(static_provider);
            let mut topic = gossip.subscribe_and_join(topic_id, bootstrap).await?;
            topic.broadcast(message.as_bytes().to_vec().into()).await?;
            std::future::pending::<()>().await;
            Ok(())
        }

        let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
        let mut rng = &mut rand_chacha::ChaCha12Rng::seed_from_u64(1);
        let topic_id = TopicId::from_bytes(rng.random());

        // spawn a gossip node, send the node's address on addr_tx,
        // then wait to receive `count` messages, and terminate.
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();
        let (msgs_recv_tx, mut msgs_recv_rx) = tokio::sync::mpsc::channel(3);
        let recv_task = tokio::task::spawn({
            let relay_map = relay_map.clone();
            let secret_key = SecretKey::generate(&mut rng);
            async move {
                let (router, gossip) = spawn_gossip(secret_key, relay_map).await?;
                // wait for the relay to be set. iroh currently has issues when trying
                // to immediately reconnect with changed direct addresses, but when the
                // relay path is available it works.
                // See https://github.com/n0-computer/iroh/pull/3372
                router.endpoint().online().await;
                let addr = router.endpoint().node_addr();
                info!(node_id = %addr.node_id.fmt_short(), "recv node spawned");
                addr_tx.send(addr).unwrap();
                let mut topic = gossip.subscribe_and_join(topic_id, vec![]).await?;
                while let Some(event) = topic.try_next().await.unwrap() {
                    if let Event::Received(message) = event {
                        let message = std::str::from_utf8(&message.content).e()?.to_string();
                        msgs_recv_tx.send(message).await.e()?;
                    }
                }
                Result::<_, n0_snafu::Error>::Ok(())
            }
        });

        let node0_addr = addr_rx.await.e()?;
        let max_wait = Duration::from_secs(5);

        // spawn a node, send a message, and then abruptly terminate the node ungracefully
        // after the message was received on our receiver node.
        let cancel = CancellationToken::new();
        let secret = SecretKey::generate(&mut rng);
        let join_handle_1 = run_in_thread(
            cancel.clone(),
            broadcast_once(
                secret.clone(),
                relay_map.clone(),
                node0_addr.clone(),
                topic_id,
                "msg1".to_string(),
            ),
        );
        // assert that we received the message on the receiver node.
        let msg = timeout(max_wait, msgs_recv_rx.recv()).await.e()?.unwrap();
        assert_eq!(&msg, "msg1");
        info!("kill broadcast node");
        cancel.cancel();

        // spawns the node again with the same node id, and send another message
        let cancel = CancellationToken::new();
        let join_handle_2 = run_in_thread(
            cancel.clone(),
            broadcast_once(
                secret.clone(),
                relay_map.clone(),
                node0_addr.clone(),
                topic_id,
                "msg2".to_string(),
            ),
        );
        // assert that we received the message on the receiver node.
        // this means that the reconnect with the same node id worked.
        let msg = timeout(max_wait, msgs_recv_rx.recv()).await.e()?.unwrap();
        assert_eq!(&msg, "msg2");
        info!("kill broadcast node");
        cancel.cancel();

        info!("kill recv node");
        recv_task.abort();
        assert!(join_handle_1.join().unwrap().is_none());
        assert!(join_handle_2.join().unwrap().is_none());

        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn gossip_change_alpn() -> n0_snafu::Result<()> {
        let alpn = b"my-gossip-alpn";
        let topic_id = TopicId::from([0u8; 32]);

        let ep1 = Endpoint::builder().bind().await?;
        let ep2 = Endpoint::builder().bind().await?;
        let gossip1 = Gossip::builder().alpn(alpn).spawn(ep1.clone());
        let gossip2 = Gossip::builder().alpn(alpn).spawn(ep2.clone());
        let router1 = Router::builder(ep1).accept(alpn, gossip1.clone()).spawn();
        let router2 = Router::builder(ep2).accept(alpn, gossip2.clone()).spawn();

        let addr1 = router1.endpoint().node_addr();
        let id1 = addr1.node_id;
        let static_provider = StaticProvider::new();
        static_provider.add_node_info(addr1);
        router2.endpoint().discovery().add(static_provider);

        let mut topic1 = gossip1.subscribe(topic_id, vec![]).await?;
        let mut topic2 = gossip2.subscribe(topic_id, vec![id1]).await?;

        timeout(Duration::from_secs(3), topic1.joined())
            .await
            .e()??;
        timeout(Duration::from_secs(3), topic2.joined())
            .await
            .e()??;
        router1.shutdown().await.e()?;
        router2.shutdown().await.e()?;
        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn gossip_rely_on_gossip_discovery() -> n0_snafu::Result<()> {
        let rng = &mut rand_chacha::ChaCha12Rng::seed_from_u64(1);

        async fn spawn(
            rng: &mut impl CryptoRng,
        ) -> n0_snafu::Result<(NodeId, Router, Gossip, GossipSender, GossipReceiver)> {
            let topic_id = TopicId::from([0u8; 32]);
            let ep = Endpoint::builder()
                .secret_key(SecretKey::generate(rng))
                .relay_mode(RelayMode::Disabled)
                .bind()
                .await?;
            let node_id = ep.node_id();
            let gossip = Gossip::builder().spawn(ep.clone());
            let router = Router::builder(ep)
                .accept(GOSSIP_ALPN, gossip.clone())
                .spawn();
            let topic = gossip.subscribe(topic_id, vec![]).await?;
            let (sender, receiver) = topic.split();
            Ok((node_id, router, gossip, sender, receiver))
        }

        // spawn 3 nodes without relay or discovery
        let (n1, r1, _g1, _tx1, mut rx1) = spawn(rng).await?;
        let (n2, r2, _g2, tx2, mut rx2) = spawn(rng).await?;
        let (n3, r3, _g3, tx3, mut rx3) = spawn(rng).await?;

        println!("nodes {:?}", [n1, n2, n3]);

        // create a static discovery that has only node 1 addr info set
        let addr1 = r1.endpoint().node_addr();
        let disco = StaticProvider::new();
        disco.add_node_info(addr1);

        // add addr info of node1 to node2 and join node1
        r2.endpoint().discovery().add(disco.clone());
        tx2.join_peers(vec![n1]).await?;

        // await join node2 -> nodde1
        timeout(Duration::from_secs(3), rx1.joined()).await.e()??;
        timeout(Duration::from_secs(3), rx2.joined()).await.e()??;

        // add addr info of node1 to node3 and join node1
        r3.endpoint().discovery().add(disco.clone());
        tx3.join_peers(vec![n1]).await?;

        // await join at node3: n1 and n2
        // n2 only works because because we use gossip discovery!
        let ev = timeout(Duration::from_secs(3), rx3.next()).await.e()?;
        assert!(matches!(ev, Some(Ok(Event::NeighborUp(_)))));
        let ev = timeout(Duration::from_secs(3), rx3.next()).await.e()?;
        assert!(matches!(ev, Some(Ok(Event::NeighborUp(_)))));

        assert_eq!(sorted(rx3.neighbors()), sorted([n1, n2]));

        let ev = timeout(Duration::from_secs(3), rx2.next()).await.e()?;
        assert!(matches!(ev, Some(Ok(Event::NeighborUp(n))) if n == n3));

        let ev = timeout(Duration::from_secs(3), rx1.next()).await.e()?;
        assert!(matches!(ev, Some(Ok(Event::NeighborUp(n))) if n == n3));

        tokio::try_join!(r1.shutdown(), r2.shutdown(), r3.shutdown()).e()?;
        Ok(())
    }

    fn sorted<T: Ord>(input: impl IntoIterator<Item = T>) -> Vec<T> {
        let mut out: Vec<_> = input.into_iter().collect();
        out.sort();
        out
    }
}
