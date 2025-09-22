//! Networking for the `iroh-gossip` protocol

#[cfg(test)]
use std::sync::atomic::AtomicBool;
use std::{
    collections::{hash_map, BTreeSet, HashMap, HashSet, VecDeque},
    ops::DerefMut,
    sync::{atomic::AtomicUsize, Arc},
};

use bytes::Bytes;
use futures_util::future::Remote;
use iroh::{
    endpoint::{ConnectError, Connection},
    protocol::{AcceptError, ProtocolHandler},
    Endpoint, NodeAddr, NodeId, Watcher,
};
use irpc::{
    channel::{self, RecvError},
    WithChannels,
};
use n0_future::{
    boxed::{BoxFuture, BoxStream},
    task::{self, AbortOnDropHandle},
    time::Instant,
    FuturesUnordered, IterExt, MergeUnbounded, Stream, StreamExt,
};
use n0_watcher::{Direct, Watchable};
use rand::rngs::StdRng;
use snafu::Snafu;
use tokio::{
    sync::{broadcast, mpsc, Notify},
    task::JoinSet,
};
use tracing::{debug, error, error_span, instrument, trace, warn, Instrument};

use crate::{
    api::{self, GossipApi},
    metrics::Metrics,
    proto::{
        self,
        util::{ConnectionCounter, OneConnection},
        Config, HyparviewConfig, PeerData, PlumtreeConfig, TopicId,
    },
};

use self::dialer::Dialer;
use self::net_proto::GossipMessage;
use self::util::{AddrInfo, IrohRemoteConnection, Timers};

mod connection_pool;
mod dialer;
mod util;

/// ALPN protocol name
pub const GOSSIP_ALPN: &[u8] = b"/iroh-gossip/1";

/// Name used for logging when new node addresses are added from gossip.
const DISCOVERY_PROVENANCE: &str = "gossip";

type State = proto::topic::State<NodeId, StdRng>;
type ProtoMessage = proto::topic::Message<NodeId>;
// type ProtoMessageOpt = Option<ProtoMessage>;
// type Event = proto::topic::Event<NodeId>;
type Timer = proto::topic::Timer<NodeId>;
type ProtoEvent = proto::topic::Event<NodeId>;
type OutEvent = proto::topic::OutEvent<NodeId>;
type InEvent = proto::topic::InEvent<NodeId>;
type Command = proto::topic::Command<NodeId>;

type RemoteClient = irpc::Client<net_proto::Request>;

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

/// Publish and subscribe on gossiping topics.
///
/// Each topic is a separate broadcast tree with separate memberships.
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
#[derive(Debug, Clone, derive_more::Deref)]
pub struct Gossip {
    incoming_tx: mpsc::Sender<Incoming>,
    #[deref]
    api: GossipApi,
    metrics: Arc<Metrics>,
    _actor_task: Arc<AbortOnDropHandle<()>>,
    max_message_size: usize,
}

impl Drop for Gossip {
    fn drop(&mut self) {
        // let backtrace = snafu::Backtrace::new();
        // tracing::info!(?backtrace, "GOSSIP DROPPED");
        tracing::info!("GOSSIP DROPPED");
    }
}

impl ProtocolHandler for Gossip {
    async fn accept(&self, connection: Connection) -> Result<(), iroh::protocol::AcceptError> {
        let remote = connection.remote_node_id()?;
        self.handle_connection(remote, connection).await?;
        Ok(())
    }

    async fn shutdown(&self) {
        // TODO: Graceful shutdown?
    }
}

#[derive(Debug, Snafu)]
struct ActorDiedError;

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
        self.api.listen(endpoint).await
    }

    /// Get the maximum message size configured for this gossip actor.
    pub fn max_message_size(&self) -> usize {
        self.max_message_size
    }

    /// Handle an incoming [`Connection`].
    ///
    /// Make sure to check the ALPN protocol yourself before passing the connection.
    pub async fn handle_connection(
        &self,
        remote: NodeId,
        connection: Connection,
    ) -> Result<(), iroh::protocol::AcceptError> {
        self.incoming_tx
            .send(Incoming::HandleConnection(remote, connection))
            .await
            .map_err(|_| AcceptError::from_err(ActorDiedError))?;
        Ok(())
    }

    /// Returns the metrics tracked for this gossip instance.
    pub fn metrics(&self) -> &Arc<Metrics> {
        &self.metrics
    }

    fn new(endpoint: Endpoint, config: Config, alpn: Option<Bytes>) -> Self {
        let metrics = Arc::new(Metrics::default());
        let max_message_size = config.max_message_size;
        let me = endpoint.node_id();
        let (api_tx, incoming_tx, actor) = Actor::new(endpoint, config, alpn, metrics.clone());
        let actor_task = task::spawn(
            actor
                .run()
                .instrument(error_span!("gossip", me=%me.fmt_short())),
        );

        Self {
            incoming_tx,
            max_message_size,
            api: GossipApi::local(api_tx),
            metrics,
            _actor_task: Arc::new(AbortOnDropHandle::new(actor_task)),
        }
    }

    #[cfg(test)]
    fn new_with_actor(endpoint: Endpoint, config: Config, alpn: Option<Bytes>) -> (Self, Actor) {
        let metrics = Arc::new(Metrics::default());
        let max_message_size = config.max_message_size;
        let (api_tx, incoming_tx, actor) = Actor::new(endpoint, config, alpn, metrics.clone());

        let handle = Self {
            incoming_tx,
            max_message_size,
            api: GossipApi::local(api_tx),
            metrics,
            _actor_task: Arc::new(AbortOnDropHandle::new(task::spawn(std::future::pending()))),
        };
        (handle, actor)
    }
}

#[derive(derive_more::Debug)]
enum Incoming {
    #[debug("HandleConnection({})", _0.fmt_short())]
    HandleConnection(NodeId, Connection),
}

#[derive(strum::Display)]
enum ActorToTopic {
    Api(ApiJoinRequest),
    RemoteEstablished {
        remote: NodeId,
        tx: Guarded<channel::mpsc::Sender<ProtoMessage>>,
        rx: Guarded<channel::mpsc::Receiver<ProtoMessage>>,
    },
    DialFailed(NodeId),
}

#[derive(strum::Display)]
enum TopicToActor {
    EstablishRemote(NodeId, TopicId),
    SetPeerData(NodeId, PeerData),
    Closed(TopicId),
}

type ApiJoinRequest = WithChannels<api::JoinRequest, api::Request>;
type RemoteJoinRequest = WithChannels<net_proto::JoinRequest, net_proto::Request>;

type ApiRecvStream = BoxStream<Result<api::Command, channel::RecvError>>;
type RemoteRecvStream = BoxStream<(NodeId, Result<Option<ProtoMessage>, channel::RecvError>)>;
// type RemoteRecvStream = BoxStream<(NodeId, Result<Option<Message>, ReadError>)>;

struct Actor {
    me: NodeId,
    alpn: Bytes,
    config: Config,
    incoming_rx: mpsc::Receiver<Incoming>,
    topic_to_actor_tx: mpsc::Sender<TopicToActor>,
    topic_to_actor_rx: mpsc::Receiver<TopicToActor>,
    api_rx: mpsc::Receiver<api::RpcMessage>,
    topics: HashMap<TopicId, TopicHandle>,
    remotes: HashMap<NodeId, RemoteState>,
    conn_tasks: JoinSet<()>,
    pending_dials: HashMap<NodeId, Vec<TopicId>>,
    dialer: Dialer,
    me_data: n0_watcher::Watchable<Option<PeerData>>,
    metrics: Arc<Metrics>,
    node_addr_stream: BoxStream<Option<NodeAddr>>,
    remote_requests: MergeUnbounded<
        BoxStream<(
            NodeId,
            std::io::Result<(net_proto::GossipMessage, OneConnection)>,
        )>,
    >,
}

struct RemoteState {
    client: irpc::Client<net_proto::Request>,
    direction: Direction,
}

#[derive(Debug, Copy, Clone)]
enum Direction {
    Dial,
    Accept,
}

impl Actor {
    fn new(
        endpoint: Endpoint,
        config: Config,
        alpn: Option<Bytes>,
        metrics: Arc<Metrics>,
    ) -> (mpsc::Sender<api::RpcMessage>, mpsc::Sender<Incoming>, Self) {
        let (api_tx, api_rx) = tokio::sync::mpsc::channel(16);
        let (topic_to_actor_tx, topic_to_actor_rx) = tokio::sync::mpsc::channel(16);
        let (incoming_tx, incoming_rx) = tokio::sync::mpsc::channel(16);

        let me = endpoint.node_id();
        let me_data = endpoint
            .node_addr()
            .get()
            .map(|addr| AddrInfo::from(addr).encode());
        let node_addr_stream = endpoint.node_addr().stream();
        (
            api_tx,
            incoming_tx,
            Actor {
                me,
                incoming_rx,
                config,
                api_rx,
                topic_to_actor_rx,
                topic_to_actor_tx,
                node_addr_stream: Box::pin(node_addr_stream),
                dialer: Dialer::new(endpoint),
                me_data: Watchable::new(me_data),
                alpn: alpn.unwrap_or_else(|| crate::ALPN.to_vec().into()),
                metrics: metrics.clone(),
                topics: Default::default(),
                pending_dials: Default::default(),
                remotes: Default::default(),
                conn_tasks: JoinSet::new(),
                remote_requests: Default::default(),
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
    pub(crate) async fn steps(&mut self, n: usize) -> Result<(), ActorDiedError> {
        for _ in 0..n {
            if !self.tick().await {
                return Err(ActorDiedError);
            }
        }
        Ok(())
    }

    async fn tick(&mut self) -> bool {
        trace!("wait for tick");
        tokio::select! {
            addr = self.node_addr_stream.next() => {
                trace!("tick: node_addr_stream");
                match addr {
                    None => {
                        warn!("address stream returned None - endpoint has shut down");
                        false
                    }
                    Some(addr) => {
                        let data = addr.map(|addr| AddrInfo::from(addr).encode());
                        let _ = self.me_data.set(data);
                        true
                    }
                }
            }
            msg = self.incoming_rx.recv() => {
                trace!("tick: incoming_rx {msg:?}");
                match msg {
                    Some(Incoming::HandleConnection(node_id, connection)) => {
                        self.handle_remote_connection(node_id, Ok(connection), Direction::Accept).await;
                        true
                    }
                    None => {
                        trace!("all incoming senders dropped, stop actor");
                        false
                    }
                }
            }
            Some(msg) = self.topic_to_actor_rx.recv() => {
                trace!("tick: topic_to_actor_rx {msg}");
                match msg {
                    TopicToActor::EstablishRemote(node_id, topic_id) => {
                        self.dialer.queue_dial(node_id, self.alpn.clone());
                        self.pending_dials.entry(node_id).or_default().push(topic_id);
                    }
                    TopicToActor::SetPeerData(node_id, data) => {
                        match AddrInfo::decode(&data) {
                            Err(err) => warn!(remote=%node_id.fmt_short(), ?err, len=data.inner().len(), "Failed to decode peer data"),
                            Ok(info) => {
                                debug!(peer = ?node_id, "add known addrs: {info:?}");
                                let node_addr = info.to_node_addr(node_id);
                                if let Err(err) = self
                                    .endpoint()
                                    .add_node_addr_with_source(node_addr, DISCOVERY_PROVENANCE)
                                {
                                    debug!(remote=%node_id.fmt_short(), "add addr failed: {err:?}");
                                }
                            }
                        }
                    }
                    TopicToActor::Closed(topic_id) => {
                        self.topics.remove(&topic_id);
                    }
                }
                true
            }
            (node_id, res) = self.dialer.next_conn() => {
                trace!(remote=%node_id.fmt_short(), ok=res.is_ok(), "tick: dialer");
                self.handle_remote_connection(node_id, res, Direction::Dial).await;
                true
            }
            Some((node_id, res)) = self.remote_requests.next(), if !self.remote_requests.is_empty() => {
                trace!(remote=%node_id.fmt_short(), res=?res.as_ref().map(|_| ()), "tick: remote_requests");
                match res {
                    Ok((request, guard)) => self.handle_remote_message(node_id, request, guard).await,
                    Err(_) => {
                        // TODO: remove from self.peers, but only if it is still the same connection
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
            else => unreachable!("reached else arm, but all fallible cases should be handled"),
        }
    }

    fn endpoint(&self) -> &Endpoint {
        self.dialer.endpoint()
    }

    fn pending_dials_by(
        &mut self,
        remote: &NodeId,
    ) -> impl Iterator<Item = (TopicId, &TopicHandle)> {
        self.pending_dials
            .remove(remote)
            .into_iter()
            .flatten()
            .flat_map(|topic_id| self.topics.get(&topic_id).map(|handle| (topic_id, handle)))
    }

    #[instrument("connection", skip_all, fields(remote=%remote.fmt_short()))]
    async fn handle_remote_connection(
        &mut self,
        remote: NodeId,
        res: Result<Connection, ConnectError>,
        direction: Direction,
    ) {
        // Read incoming requests.
        let res = match res {
            Ok(connection) => {
                let irpc_conn = IrohRemoteConnection::new(connection.clone());
                let client = irpc::Client::boxed(irpc_conn.clone());
                let counter = ConnectionCounter::new();
                let guard = counter.get_one();
                if matches!(direction, Direction::Dial) {
                    self.conn_tasks.spawn({
                        let counter = counter.clone();
                        async move {
                            let stream = counter.idle_stream();
                            tokio::pin!(stream);
                            loop {
                                let _ = stream.next().await;
                                if counter.is_idle() {
                                    connection.close(1u32.into(), b"idle");
                                    break;
                                }
                            }
                        }
                    });
                }

                // TODO we keep dead clients around.
                let state = RemoteState {
                    client: client.clone(),
                    direction,
                };
                self.remotes.insert(remote, state);
                self.remote_requests.push(Box::pin(
                    irpc_conn
                        .into_request_stream::<net_proto::Request>()
                        .map(move |req| (remote, req.map(|r| (r, counter.get_one())))),
                ));
                Ok((client, guard))
            }
            Err(err) => {
                debug!(?err, "Connection failed");
                Err(())
            }
        };

        // Inform topics that wanted a stream to that node.
        for (topic_id, handle) in self.pending_dials_by(&remote) {
            let Ok(permit) = handle.tx.clone().reserve_owned().await else {
                warn!(topic=%topic_id.fmt_short(), "Topic actor dead");
                continue;
            };
            let client = res.clone();
            task::spawn(async move {
                let msg = match client {
                    Err(()) => ActorToTopic::DialFailed(remote),
                    Ok((client, guard)) => {
                        let req = net_proto::JoinRequest { topic_id };
                        match client.bidi_streaming(req.clone(), 64, 64).await {
                            Ok((tx, rx)) => ActorToTopic::RemoteEstablished {
                                remote,
                                tx: Guarded::new(tx, guard.clone()),
                                rx: Guarded::new(rx, guard),
                            },
                            Err(err) => {
                                warn!(?topic_id, ?err, "failed to open stream with remote");
                                ActorToTopic::DialFailed(remote)
                            }
                        }
                    }
                };
                permit.send(msg);
            });
        }
    }

    #[instrument("request", skip_all, fields(remote=%remote.fmt_short()))]
    async fn handle_remote_message(
        &mut self,
        remote: NodeId,
        request: GossipMessage,
        guard: OneConnection,
    ) {
        let (topic_id, req) = match request {
            GossipMessage::Join(req) => (req.inner.topic_id, req),
        };
        if let Some(topic) = self.topics.get(&topic_id) {
            if let Err(_err) = topic
                .send(ActorToTopic::RemoteEstablished {
                    remote,
                    tx: Guarded::new(req.tx, guard.clone()),
                    rx: Guarded::new(req.rx, guard.clone()),
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
            TopicHandle::new(
                self.me,
                topic_id,
                self.config.clone(),
                self.topic_to_actor_tx.clone(),
                self.me_data.watch(),
            )
        });
        if let Err(_) = topic.send(ActorToTopic::Api(msg)).await {
            error!(topic=%topic_id.fmt_short(), "Topic actor dead");
        }
    }
}

struct TopicHandle {
    tx: mpsc::Sender<ActorToTopic>,
    #[cfg(test)]
    joined: Arc<AtomicBool>,
    _task: AbortOnDropHandle<()>,
}

impl TopicHandle {
    fn new(
        me: NodeId,
        topic_id: TopicId,
        config: proto::Config,
        to_actor_tx: mpsc::Sender<TopicToActor>,
        peer_data: Direct<Option<PeerData>>,
    ) -> Self {
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
            timers: Default::default(),
            neighbors: Default::default(),
            out_events: Default::default(),
            api_receivers: Default::default(),
            remote_senders: Default::default(),
            remote_receivers: Default::default(),
            drop_peers_queue: Default::default(),
            forward_event_tasks: JoinSet::new(),
            forward_event_tx,
            init: false,
            #[cfg(test)]
            joined: joined.clone(),
        };
        let task = task::spawn(
            actor
                .run()
                .instrument(error_span!("topic", topic=%topic_id.fmt_short())),
        );
        Self {
            tx,
            _task: AbortOnDropHandle::new(task),
            #[cfg(test)]
            joined,
        }
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
    to_actor_tx: mpsc::Sender<TopicToActor>,
    state: State,
    actor_rx: mpsc::Receiver<ActorToTopic>,
    timers: Timers<Timer>,
    neighbors: BTreeSet<NodeId>,
    peer_data: Direct<Option<PeerData>>,
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
}

impl TopicActor {
    pub async fn run(mut self) {
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
                    if let Some(data) = data {
                        self.handle_in_event(InEvent::UpdatePeerData(data)).await;
                    }
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
        self.to_actor_tx
            .send(TopicToActor::Closed(self.topic_id))
            .await
            .ok();
    }

    async fn handle_actor_message(&mut self, msg: ActorToTopic) {
        match msg {
            ActorToTopic::RemoteEstablished { remote, rx, tx } => {
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
            ActorToTopic::DialFailed(node_id) => {
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
        match message {
            Ok(Some(message)) => {
                self.handle_in_event(InEvent::RecvMessage(remote, message))
                    .await;
            }
            Ok(None) => {
                debug!(remote=%remote.fmt_short(), "Recv stream from remote closed");
                self.handle_in_event(InEvent::PeerDisconnected(remote))
                    .await;
            }
            Err(err) => {
                warn!(remote=%remote.fmt_short(), ?err, "Recv stream from remote failed");
                self.handle_in_event(InEvent::PeerDisconnected(remote))
                    .await;
            }
        }
    }

    async fn handle_api_command(&mut self, command: Result<api::Command, channel::RecvError>) {
        let Ok(command) = command else {
            // TODO: Do we have to do anything if we failed to receive, i.e. an API subscriber is dead?
            // Leave topic if no one is left.
            return;
        };
        trace!("tick: api command {command}");
        self.handle_in_event(InEvent::Command(command.into())).await;
    }

    async fn handle_in_event(&mut self, event: InEvent) {
        trace!("tick: in event {event:?}");
        let now = Instant::now();
        self.out_events.extend(self.state.handle(event, now));
        self.process_out_events(now).await;
    }
    async fn process_out_events(&mut self, now: Instant) {
        while let Some(event) = self.out_events.pop_front() {
            trace!("tick: out event {event:?}");
            match event {
                OutEvent::SendMessage(node_id, message) => {
                    self.send(node_id, message).await;
                }
                OutEvent::EmitEvent(event) => {
                    self.handle_event(event).await;
                }
                OutEvent::ScheduleTimer(delay, timer) => {
                    self.timers.insert(now + delay, timer);
                }
                OutEvent::DisconnectPeer(node_id) => {
                    self.remote_senders.remove(&node_id);
                    // TODO: Abort receive channel?
                }
                OutEvent::PeerData(node_id, peer_data) => {
                    self.to_actor_tx
                        .send(TopicToActor::SetPeerData(node_id, peer_data))
                        .await
                        .ok();
                }
            }
        }
    }

    // TODO: Error handling
    #[instrument(skip_all, fields(remote=%remote.fmt_short()))]
    async fn send(&mut self, remote: NodeId, message: ProtoMessage) {
        let sender = match self.remote_senders.entry(remote) {
            hash_map::Entry::Occupied(entry) => entry.into_mut(),
            hash_map::Entry::Vacant(entry) => {
                debug!("requesting new connection");
                self.to_actor_tx
                    .send(TopicToActor::EstablishRemote(remote, self.topic_id))
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

    async fn handle_event(&mut self, event: ProtoEvent) {
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
            ProtoEvent::Received(_gossip_event) => {}
        }
        self.forward_event_tx.send(event).ok();
        // let event = api::Event::from(event);

        // // This is an async send + retain (remove failed senders)
        // // TODO: check perf and optimize if needed.
        // // TODO: Add timeout to not let slow receivers stall everything.
        // let to_remove = self
        //     .api_senders
        //     .iter()
        //     .enumerate()
        //     .map(async |(i, tx)| tx.send(event.clone()).await.err().map(|_| i))
        //     .join_all()
        //     .await
        //     .into_iter()
        //     .flatten();
        // for i in to_remove.rev() {
        //     self.api_senders.remove(i);
        // }
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
    tracing::info!("CLOSING EVENT FWD TASK");
}

#[derive(derive_more::Deref, derive_more::DerefMut, Debug)]
struct Guarded<T> {
    #[deref]
    #[deref_mut]
    inner: T,
    _guard: OneConnection,
}

impl<T> Guarded<T> {
    fn new(inner: T, guard: OneConnection) -> Self {
        Self {
            inner,
            _guard: guard,
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
    // receiver: channel::mpsc::Receiver<T>,
    receiver: impl DerefMut<Target = channel::mpsc::Receiver<T>> + Send + Sync + 'static,
) -> impl Stream<Item = Result<Option<T>, RecvError>> + Send + Sync + 'static {
    n0_future::stream::unfold(Some(receiver), |recv| async move {
        let mut recv = recv?;
        let res = recv.recv().await;
        debug!("RECV {res:?}");
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
mod tests {
    use std::{future::Future, time::Duration};

    use iroh::{protocol::Router, RelayMap, RelayMode, SecretKey, Watcher};
    use n0_snafu::ResultExt;
    use tokio::time::timeout;
    use tokio_util::sync::CancellationToken;
    use tracing_test::traced_test;

    use super::*;

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

            endpoint.home_relay().initialized().await;
            let (gossip, mut actor) = Gossip::new_with_actor(endpoint.clone(), config, None);
            actor.node_addr_stream = Box::pin(n0_future::stream::pending());
            let router = Router::builder(endpoint)
                .accept(GOSSIP_ALPN, gossip.clone())
                .spawn();
            let cancel = cancel.clone();
            let router_task = tokio::task::spawn(async move {
                cancel.cancelled().await;
                router.shutdown().await.ok();
                drop(router);
                tracing::info!("ROUTER DROPPED");
            });
            let router_fut = async move {
                router_task.await.expect("router task panicked");
            };
            Ok((gossip, actor, router_fut))
        }
    }

    #[tokio::test]
    #[traced_test]
    async fn gossip_smokenew() -> n0_snafu::Result<()> {
        // tracing_subscriber::fmt::try_init().ok();
        let topic_id = TopicId::from([0u8; 32]);

        let ep1 = Endpoint::builder().bind().await?;
        let ep2 = Endpoint::builder().bind().await?;
        let gossip1 = Gossip::builder().spawn(ep1.clone());
        let gossip2 = Gossip::builder().spawn(ep2.clone());
        let router1 = Router::builder(ep1)
            .accept(crate::ALPN, gossip1.clone())
            .spawn();
        let router2 = Router::builder(ep2)
            .accept(crate::ALPN, gossip2.clone())
            .spawn();

        let addr1 = router1.endpoint().node_addr().initialized().await;
        let id1 = addr1.node_id;
        router2.endpoint().add_node_addr(addr1)?;

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
}

#[cfg(test)]
pub(crate) mod test {
    use std::time::Duration;

    use bytes::Bytes;
    use futures_concurrency::future::TryJoin;
    use iroh::{endpoint::BindError, protocol::Router, NodeAddr, RelayMap, RelayMode, SecretKey};
    use n0_snafu::{Result, ResultExt};
    use rand::{Rng, SeedableRng};
    use tokio::{spawn, time::timeout};
    use tokio_util::sync::CancellationToken;
    use tracing::info;
    use tracing_test::traced_test;

    use super::*;
    use crate::{api::Event, ALPN};

    pub(crate) async fn create_endpoint(
        rng: &mut rand_chacha::ChaCha12Rng,
        relay_map: RelayMap,
    ) -> Result<Endpoint, BindError> {
        let ep = Endpoint::builder()
            .secret_key(SecretKey::generate(rng))
            .alpns(vec![ALPN.to_vec()])
            .relay_mode(RelayMode::Custom(relay_map))
            .insecure_skip_relay_cert_verify(true)
            .bind()
            .await?;

        ep.home_relay().initialized().await;
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

        let ep1 = create_endpoint(&mut rng, relay_map.clone()).await.unwrap();
        let ep2 = create_endpoint(&mut rng, relay_map.clone()).await.unwrap();
        let ep3 = create_endpoint(&mut rng, relay_map.clone()).await.unwrap();

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
        ep2.add_node_addr(addr1.clone()).unwrap();
        ep3.add_node_addr(addr2).unwrap();

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
    ///   droppetopicd
    // NOTE: this is a regression test.
    #[tokio::test]
    #[traced_test]
    async fn subscription_cleanup() -> testresult::TestResult {
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
                        other @ _ => tracing::debug!(?other, "gs event"),
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
        actor.endpoint().add_node_addr(addr2)?;
        // we use a channel to signal advancing steps to the task
        let (tx, mut rx) = mpsc::channel::<()>(1);
        let ct1 = ct.clone();
        let go1_task = async move {
            // first subscribe is done immediately
            tracing::info!("subscribing the first time");
            let sub_1a = go1.subscribe_and_join(topic, vec![node_id2]).await?;
            tracing::info!("subscribed!");

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
            tracing::info!("go1 dropped!");
            drop(go1);

            Ok::<_, n0_snafu::Error>(())
        }
        .instrument(tracing::debug_span!("node_1", node_id1 = %node_id1.fmt_short()));
        let go1_handle = task::spawn(go1_task);

        // advance and check that the topic is now subscribed
        tracing::info!("now wait subscribe 1");
        actor.steps(4).await?; // api_rx subscribe; topic_rx connection request; dialer connected; topic_rx update peer data
        tracing::info!("subscribe and join done, should be joined");
        let state = actor.topics.get(&topic).expect("get registered topic");
        assert!(state.joined());

        // signal the second subscribe, we should remain subscribed
        tx.send(()).await?;
        tracing::info!("now wait subscribe 2");
        actor.steps(1).await?; // api_subscribe
        let state = actor.topics.get(&topic).expect("get registered topic");
        assert!(state.joined());

        // signal to drop the second handle, the topic should no longer be subscribed
        tx.send(()).await?;
        actor.steps(1).await?; // topic closed

        assert!(!actor.topics.contains_key(&topic));

        // cleanup and ensure everything went as expected
        ct.cancel();
        let wait = Duration::from_secs(5);
        timeout(wait, ep1_handle).await?;
        timeout(wait, ep2_handle).await?;
        timeout(wait, go1_handle).await???;
        timeout(wait, go2_handle).await???;
        timeout(wait, actor.finish()).await?;

        testresult::TestResult::Ok(())
    }

    /// Test that nodes can reconnect to each other.
    ///
    /// This test will create two nodes subscribed to the same topic. The second node will
    /// unsubscribe and then resubscribe and connection between the nodes should succeed both
    /// times.
    // NOTE: This is a regression test
    #[tokio::test(flavor = "multi_thread")]
    // #[traced_test]
    async fn can_reconnect() -> testresult::TestResult {
        tracing_subscriber::fmt::try_init().ok();
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
            node_1 = node_id1.fmt_short(),
            node_2 = node_id2.fmt_short(),
            "nodes ready"
        );

        let topic: TopicId = blake3::hash(b"can_reconnect").into();
        tracing::info!(%topic, "joining");

        // channel used to signal the second gossip instance to advance the test
        let (tx, mut rx) = mpsc::channel::<()>(1);
        let addr1 = NodeAddr::new(node_id1).with_relay_url(relay_url.clone());
        ep2.add_node_addr(addr1)?;
        let go2_task = async move {
            info!("go2 sub1 subscribe");
            let mut sub = go2.subscribe(topic, Vec::new()).await?;
            sub.joined().await?;
            info!("go2 sub1 joined");

            rx.recv().await.expect("signal to unsubscribe");
            tracing::info!("go2 sub1 drop");
            drop(sub);

            rx.recv().await.expect("signal to subscribe again");
            tracing::info!("go2 sub2 subscribe");
            let mut sub = go2.subscribe(topic, vec![node_id1]).await?;

            sub.joined().await?;
            tracing::info!("go2 sub2 joined!");

            Ok::<_, n0_snafu::Error>(())
        }
        .instrument(tracing::debug_span!("node_2", node_id2=%node_id2.fmt_short()));
        let go2_handle = task::spawn(go2_task);

        let addr2 = NodeAddr::new(node_id2).with_relay_url(relay_url);
        ep1.add_node_addr(addr2)?;

        let mut sub = go1.subscribe(topic, vec![node_id2]).await?;
        // wait for subscribed notification
        sub.joined().await?;
        info!("go1 joined");

        // signal node_2 to unsubscribe
        tx.send(()).await?;

        info!("wait for neighbor down");
        // we should receive a Neighbor down event
        let conn_timeout = Duration::from_millis(2000);
        let ev = timeout(conn_timeout, sub.try_next()).await??;
        assert_eq!(ev, Some(Event::NeighborDown(node_id2)));
        tracing::info!("node 2 left");

        // signal node_2 to subscribe again
        tx.send(()).await?;

        info!("wait for neighbor up");
        let conn_timeout = Duration::from_millis(500);
        let ev = timeout(conn_timeout, sub.try_next()).await??;
        assert_eq!(ev, Some(Event::NeighborUp(node_id2)));
        tracing::info!("node 2 rejoined!");

        // wait for go2 to also be rejoined, then the task terminates
        let wait = Duration::from_secs(2);
        timeout(wait, go2_handle).await???;
        ct.cancel();
        // cleanup and ensure everything went as expected
        timeout(wait, ep1_handle).await?;
        timeout(wait, ep2_handle).await?;

        testresult::TestResult::Ok(())
    }

    // /// Test that when a gossip topic is no longer needed it's actually unsubscribed.
    // ///
    // /// This test will:
    // /// - Create two endpoints, the first using manual event loop.
    // /// - Subscribe both nodes to the same topic. The first node will subscribe twice and connect
    // ///   to the second node. The second node will subscribe without bootstrap.
    // /// - Ensure that the first node removes the subscription iff all topic handles have been
    // ///   dropped
    // // NOTE: this is a regression test.
    // #[tokio::test]
    // #[traced_test]
    // async fn subscription_cleanup() -> Result {
    //     let rng = &mut rand_chacha::ChaCha12Rng::seed_from_u64(1);
    //     let ct = CancellationToken::new();
    //     let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();

    //     // create the first node with a manual actor loop
    //     let (go1, actor, ep1_handle) =
    //         Gossip::t_new_with_actor(rng, Default::default(), relay_map.clone(), &ct).await?;
    //     let mut actor = ManualActorLoop::new(actor).await;

    //     // create the second node with the usual actor loop
    //     let (go2, ep2, ep2_handle, _test_actor_handle) =
    //         Gossip::t_new(rng, Default::default(), relay_map, &ct).await?;

    //     let node_id1 = actor.endpoint.node_id();
    //     let node_id2 = ep2.node_id();
    //     tracing::info!(
    //         node_1 = node_id1.fmt_short(),
    //         node_2 = node_id2.fmt_short(),
    //         "nodes ready"
    //     );

    //     let topic: TopicId = blake3::hash(b"subscription_cleanup").into();
    //     tracing::info!(%topic, "joining");

    //     // create the tasks for each gossip instance:
    //     // - second node subscribes once without bootstrap and listens to events
    //     // - first node subscribes twice with the second node as bootstrap. This is done on command
    //     //   from the main task (this)

    //     // second node
    //     let ct2 = ct.clone();
    //     let go2_task = async move {
    //         let (_pub_tx, mut sub_rx) = go2.subscribe_and_join(topic, vec![]).await?.split();

    //         let subscribe_fut = async {
    //             while let Some(ev) = sub_rx.try_next().await? {
    //                 match ev {
    //                     Event::Lagged => tracing::debug!("missed some messages :("),
    //                     Event::Received(_) => unreachable!("test does not send messages"),
    //                     other => tracing::debug!(?other, "gs event"),
    //                 }
    //             }

    //             tracing::debug!("subscribe stream ended");
    //             Result::<_, n0_snafu::Error>::Ok(())
    //         };

    //         tokio::select! {
    //             _ = ct2.cancelled() => Ok(()),
    //             res = subscribe_fut => res,
    //         }
    //     }
    //     .instrument(tracing::debug_span!("node_2", %node_id2));
    //     let go2_handle = task::spawn(go2_task);

    //     // first node
    //     let addr2 = NodeAddr::new(node_id2).with_relay_url(relay_url);
    //     actor.endpoint.add_node_addr(addr2)?;
    //     // we use a channel to signal advancing steps to the task
    //     let (tx, mut rx) = mpsc::channel::<()>(1);
    //     let ct1 = ct.clone();
    //     let go1_task = async move {
    //         // first subscribe is done immediately
    //         tracing::info!("subscribing the first time");
    //         let sub_1a = go1.subscribe_and_join(topic, vec![node_id2]).await?;

    //         // wait for signal to subscribe a second time
    //         rx.recv().await.expect("signal for second subscribe");
    //         tracing::info!("subscribing a second time");
    //         let sub_1b = go1.subscribe_and_join(topic, vec![node_id2]).await?;
    //         drop(sub_1a);

    //         // wait for signal to drop the second handle as well
    //         rx.recv().await.expect("signal for second subscribe");
    //         tracing::info!("dropping all handles");
    //         drop(sub_1b);

    //         // wait for cancellation
    //         ct1.cancelled().await;
    //         drop(go1);

    //         Result::<_, n0_snafu::Error>::Ok(())
    //     }
    //     .instrument(tracing::debug_span!("node_1", %node_id1));
    //     let go1_handle = task::spawn(go1_task);

    //     // advance and check that the topic is now subscribed
    //     actor.steps(3).await; // handle our subscribe;
    //                           // get peer connection;
    //                           // receive the other peer's information for a NeighborUp
    //     let state = actor.topics.get(&topic).expect("get registered topic");
    //     assert!(state.joined());

    //     // signal the second subscribe, we should remain subscribed
    //     tx.send(()).await.e()?;
    //     actor.steps(3).await; // subscribe; first receiver gone; first sender gone
    //     let state = actor.topics.get(&topic).expect("get registered topic");
    //     assert!(state.joined());

    //     // signal to drop the second handle, the topic should no longer be subscribed
    //     tx.send(()).await.e()?;
    //     actor.steps(2).await; // second receiver gone; second sender gone
    //     assert!(!actor.topics.contains_key(&topic));

    //     // cleanup and ensure everything went as expected
    //     ct.cancel();
    //     let wait = Duration::from_secs(2);
    //     timeout(wait, ep1_handle).await.e()?.e()??;
    //     timeout(wait, ep2_handle).await.e()?.e()??;
    //     timeout(wait, go1_handle).await.e()?.e()??;
    //     timeout(wait, go2_handle).await.e()?.e()??;
    //     timeout(wait, actor.finish()).await.e()?;

    //     Ok(())
    // }

    // /// Test that nodes can reconnect to each other.
    // ///
    // /// This test will create two nodes subscribed to the same topic. The second node will
    // /// unsubscribe and then resubscribe and connection between the nodes should succeed both
    // /// times.
    // // NOTE: This is a regression test
    // #[tokio::test]
    // #[traced_test]
    // async fn can_reconnect() -> Result {
    //     let rng = &mut rand_chacha::ChaCha12Rng::seed_from_u64(1);
    //     let ct = CancellationToken::new();
    //     let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();

    //     let (go1, ep1, ep1_handle, _test_actor_handle1) =
    //         Gossip::t_new(rng, Default::default(), relay_map.clone(), &ct).await?;

    //     let (go2, ep2, ep2_handle, _test_actor_handle2) =
    //         Gossip::t_new(rng, Default::default(), relay_map, &ct).await?;

    //     let node_id1 = ep1.node_id();
    //     let node_id2 = ep2.node_id();
    //     tracing::info!(
    //         node_1 = node_id1.fmt_short(),
    //         node_2 = node_id2.fmt_short(),
    //         "nodes ready"
    //     );

    //     let topic: TopicId = blake3::hash(b"can_reconnect").into();
    //     tracing::info!(%topic, "joining");

    //     let ct2 = ct.child_token();
    //     // channel used to signal the second gossip instance to advance the test
    //     let (tx, mut rx) = mpsc::channel::<()>(1);
    //     let addr1 = NodeAddr::new(node_id1).with_relay_url(relay_url.clone());
    //     ep2.add_node_addr(addr1)?;
    //     let go2_task = async move {
    //         let mut sub = go2.subscribe(topic, Vec::new()).await?;
    //         sub.joined().await?;

    //         rx.recv().await.expect("signal to unsubscribe");
    //         tracing::info!("unsubscribing");
    //         drop(sub);

    //         rx.recv().await.expect("signal to subscribe again");
    //         tracing::info!("resubscribing");
    //         let mut sub = go2.subscribe(topic, vec![node_id1]).await?;

    //         sub.joined().await?;
    //         tracing::info!("subscription successful!");

    //         ct2.cancelled().await;

    //         Result::<_, ApiError>::Ok(())
    //     }
    //     .instrument(tracing::debug_span!("node_2", %node_id2));
    //     let go2_handle = task::spawn(go2_task);

    //     let addr2 = NodeAddr::new(node_id2).with_relay_url(relay_url);
    //     ep1.add_node_addr(addr2)?;

    //     let mut sub = go1.subscribe(topic, vec![node_id2]).await?;
    //     // wait for subscribed notification
    //     sub.joined().await?;

    //     // signal node_2 to unsubscribe
    //     tx.send(()).await.e()?;

    //     // we should receive a Neighbor down event
    //     let conn_timeout = Duration::from_millis(500);
    //     let ev = timeout(conn_timeout, sub.try_next()).await.e()??;
    //     assert_eq!(ev, Some(Event::NeighborDown(node_id2)));
    //     tracing::info!("node 2 left");

    //     // signal node_2 to subscribe again
    //     tx.send(()).await.e()?;

    //     let conn_timeout = Duration::from_millis(500);
    //     let ev = timeout(conn_timeout, sub.try_next()).await.e()??;
    //     assert_eq!(ev, Some(Event::NeighborUp(node_id2)));
    //     tracing::info!("node 2 rejoined!");

    //     // cleanup and ensure everything went as expected
    //     ct.cancel();
    //     let wait = Duration::from_secs(2);
    //     timeout(wait, ep1_handle).await.e()?.e()??;
    //     timeout(wait, ep2_handle).await.e()?.e()??;
    //     timeout(wait, go2_handle).await.e()?.e()??;

    //     Result::Ok(())
    // }

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
            router.endpoint().add_node_addr(bootstrap_addr)?;
            let mut topic = gossip.subscribe_and_join(topic_id, bootstrap).await?;
            topic.broadcast(message.as_bytes().to_vec().into()).await?;
            std::future::pending::<()>().await;
            Ok(())
        }

        let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
        let mut rng = &mut rand_chacha::ChaCha12Rng::seed_from_u64(1);
        let topic_id = TopicId::from_bytes(rng.r#gen());

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
                router.endpoint().home_relay().initialized().await;
                let addr = router.endpoint().node_addr().initialized().await;
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

        let addr1 = router1.endpoint().node_addr().initialized().await;
        let id1 = addr1.node_id;
        router2.endpoint().add_node_addr(addr1)?;

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
}
