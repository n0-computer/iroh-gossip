//! Networking for the `iroh-gossip` protocol

use std::{
    collections::{hash_map, BTreeSet, HashMap, VecDeque},
    sync::Arc,
};

use bytes::Bytes;
use iroh::{
    endpoint::{ConnectError, Connection},
    protocol::{AcceptError, ProtocolHandler},
    Endpoint, NodeId, Watcher,
};
use irpc::{
    channel::{self, RecvError},
    WithChannels,
};
use n0_future::{
    boxed::BoxStream,
    task::{self, AbortOnDropHandle},
    time::Instant,
    IterExt, MergeUnbounded, Stream, StreamExt,
};
use n0_watcher::{Direct, Watchable};
use rand::rngs::StdRng;
use snafu::Snafu;
use tokio::{sync::mpsc, task::JoinSet};
use tracing::{debug, error, info, instrument, trace, warn};

use crate::{
    api::{self, GossipApi},
    proto::{self, Config, HyparviewConfig, PeerData, PlumtreeConfig, TopicId},
};

use self::dialer::Dialer;
use self::net_proto::GossipMessage;
use self::util::{AddrInfo, IrohRemoteConnection, Timers};

mod dialer;
mod util;

/// ALPN protocol name
pub const GOSSIP_ALPN: &[u8] = b"/iroh-gossip/1";

/// Name used for logging when new node addresses are added from gossip.
const DISCOVERY_PROVENANCE: &str = "gossip";

type State = proto::topic::State<NodeId, StdRng>;
type ProtoMessage = proto::topic::Message<NodeId>;
type ProtoMessageOpt = Option<ProtoMessage>;
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
        Gossip::new(endpoint, Some(self.config), self.alpn)
        // let metrics = Arc::new(Metrics::default());
        // let (actor, rpc_tx, local_tx) =
        //     Actor::new(endpoint, self.config, metrics.clone(), self.alpn);
        // let me = actor.endpoint.node_id().fmt_short();
        // let max_message_size = actor.state.max_message_size();

        // let actor_handle = task::spawn(actor.run().instrument(error_span!("gossip", %me)));

        // let api = GossipApi::local(rpc_tx);

        // Gossip {
        //     inner: Inner {
        //         api,
        //         local_tx,
        //         _actor_handle: AbortOnDropHandle::new(actor_handle),
        //         max_message_size,
        //         metrics,
        //     }
        //     .into(),
        // }
    }
}

mod net_proto {
    use irpc::{channel::mpsc, rpc_requests};
    use serde::{Deserialize, Serialize};

    use crate::proto::TopicId;

    pub type Client = irpc::Client<Request>;

    #[derive(Debug, Serialize, Deserialize, Clone)]
    #[non_exhaustive]
    pub struct JoinRequest {
        pub topic_id: TopicId,
    }

    #[rpc_requests(message = GossipMessage)]
    #[derive(Debug, Serialize, Deserialize)]
    pub enum Request {
        #[rpc(tx=mpsc::Sender<super::ProtoMessageOpt>, rx=mpsc::Receiver<super::ProtoMessageOpt>)]
        Join(JoinRequest),
    }
}

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
#[derive(Debug, Clone, derive_more::Deref)]
pub struct Gossip {
    incoming_tx: mpsc::Sender<Incoming>,
    #[deref]
    api: GossipApi,
    _task: Arc<AbortOnDropHandle<()>>,
    max_message_size: usize,
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
            .send(Incoming::Accepted(remote, connection))
            .await
            .map_err(|_| AcceptError::from_err(ActorDiedError))?;
        Ok(())
    }

    fn new(endpoint: Endpoint, config: Option<Config>, alpn: Option<Bytes>) -> Self {
        let (api_tx, api_rx) = tokio::sync::mpsc::channel(16);
        let (topic_to_actor_tx, topic_to_actor_rx) = tokio::sync::mpsc::channel(16);
        let (incoming_tx, incoming_rx) = tokio::sync::mpsc::channel(16);

        let config = config.unwrap_or_default();
        let max_message_size = config.max_message_size;
        let me = endpoint.node_id();
        let me_data = endpoint
            .node_addr()
            .get()
            .map(|addr| AddrInfo::from(addr).encode());
        let actor = Actor {
            me,
            incoming_rx,
            incoming_tx: incoming_tx.clone(),
            config,
            api_rx,
            topic_to_actor_rx,
            topic_to_actor_tx,
            dialer: Dialer::new(endpoint),
            me_data: Watchable::new(me_data),
            alpn: alpn.unwrap_or_else(|| crate::ALPN.to_vec().into()),

            topics: Default::default(),
            pending_dials: Default::default(),
            remotes: Default::default(),
            connection_tasks: Default::default(),
        };
        let task = n0_future::task::spawn(actor.run());

        Self {
            incoming_tx,
            max_message_size,
            api: GossipApi::local(api_tx),
            _task: Arc::new(AbortOnDropHandle::new(task)),
        }
    }
}

enum Incoming {
    Accepted(NodeId, Connection),
    Request(NodeId, GossipMessage),
}

enum ActorToTopic {
    Api(ApiJoinRequest),
    RemoteConnected(
        NodeId,
        channel::mpsc::Sender<ProtoMessageOpt>,
        channel::mpsc::Receiver<ProtoMessageOpt>,
    ),
}

enum TopicToActor {
    Request(NodeId, TopicId),
    PeerData(NodeId, PeerData),
}

type ApiJoinRequest = WithChannels<api::JoinRequest, api::Request>;
type RemoteJoinRequest = WithChannels<net_proto::JoinRequest, net_proto::Request>;

type ApiRecvStream = BoxStream<Result<api::Command, channel::RecvError>>;
type RemoteRecvStream = BoxStream<(NodeId, Result<ProtoMessageOpt, channel::RecvError>)>;
// type RemoteRecvStream = BoxStream<(NodeId, Result<Option<Message>, ReadError>)>;

struct Actor {
    me: NodeId,
    alpn: Bytes,
    config: Config,
    incoming_rx: mpsc::Receiver<Incoming>,
    incoming_tx: mpsc::Sender<Incoming>,
    topic_to_actor_tx: mpsc::Sender<TopicToActor>,
    topic_to_actor_rx: mpsc::Receiver<TopicToActor>,
    api_rx: mpsc::Receiver<crate::api::RpcMessage>,
    topics: HashMap<TopicId, TopicHandle>,
    remotes: HashMap<NodeId, RemoteClient>,
    pending_dials: HashMap<NodeId, Vec<TopicId>>,
    dialer: Dialer,
    me_data: n0_watcher::Watchable<Option<PeerData>>,
    connection_tasks: JoinSet<()>,
}

impl Actor {
    pub async fn run(mut self) {
        let node_addr_stream = self.dialer.endpoint().node_addr().stream();
        tokio::pin!(node_addr_stream);
        loop {
            tokio::select! {
                Some(addr) = node_addr_stream.next() => {
                    let data = addr.map(|addr| AddrInfo::from(addr).encode());
                    self.me_data.set(data);
                }
                Some(msg) = self.incoming_rx.recv() => {
                    match msg {
                        Incoming::Accepted(node_id, connection) => {
                            self.handle_remote_connection(node_id, Ok(connection)).await;
                        }
                        Incoming::Request(node_id, request) => {
                            self.handle_remote_request(node_id, request).await;
                        }
                    }
                }
                Some(msg) = self.topic_to_actor_rx.recv() => {
                    match msg {
                        TopicToActor::Request(node_id, topic_id) => {
                            self.dialer.queue_dial(node_id, self.alpn.clone());
                            self.pending_dials.entry(node_id).or_default().push(topic_id);
                        }
                        TopicToActor::PeerData(node_id, data) => {
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
                    }
                }
                (node_id, res) = self.dialer.next_conn() => {
                    self.handle_remote_connection(node_id, res).await;
                }
                Some(msg) = self.api_rx.recv() => {
                    self.handle_api_message(msg).await;
                }
                _ = self.connection_tasks.join_next(), if !self.connection_tasks.is_empty() => {}
                else => break,
            }
        }
    }

    fn endpoint(&self) -> &Endpoint {
        self.dialer.endpoint()
    }

    async fn connection_task(
        remote: NodeId,
        connection: Connection,
        incoming_tx: mpsc::Sender<Incoming>,
    ) {
        loop {
            match irpc_iroh::read_request::<net_proto::Request>(&connection).await {
                Err(err) => {
                    // TODO: err
                    tracing::info!("connection task terminated: {err:?}");
                    break;
                }
                Ok(None) => break,
                Ok(Some(request)) => {
                    if let Err(_) = incoming_tx.send(Incoming::Request(remote, request)).await {
                        break;
                    }
                }
            }
        }
    }

    #[instrument("connection", skip_all, fields(remote=%remote.fmt_short()))]
    async fn handle_remote_connection(
        &mut self,
        remote: NodeId,
        res: Result<Connection, ConnectError>,
    ) {
        let connection = match res {
            Ok(c) => c,
            Err(err) => {
                // TODO: Emit PeerDisconnected to topics.
                debug!(?err, "Connection failed");
                return;
            }
        };
        let client = irpc::Client::<net_proto::Request>::boxed(IrohRemoteConnection::new(
            connection.clone(),
        ));
        self.remotes.insert(remote, client.clone());
        self.connection_tasks.spawn(Self::connection_task(
            remote,
            connection,
            self.incoming_tx.clone(),
        ));
        // TODO: Don't block main loop.
        for (topic_id, handle) in self
            .pending_dials
            .remove(&remote)
            .into_iter()
            .flatten()
            .flat_map(|topic_id| self.topics.get(&topic_id).map(|handle| (topic_id, handle)))
        {
            let req = net_proto::JoinRequest { topic_id };
            let (tx, rx) = match client.bidi_streaming(req.clone(), 64, 64).await {
                Ok(pair) => pair,
                Err(err) => {
                    warn!(?topic_id, ?err, "failed to open stream with remote");
                    return;
                }
            };
            if let Err(_) = handle
                .send(ActorToTopic::RemoteConnected(remote, tx, rx))
                .await
            {
                warn!(topic=%topic_id.fmt_short(), "Topic actor dead");
            }
        }
    }

    #[instrument("request", skip_all, fields(remote=%remote.fmt_short()))]
    async fn handle_remote_request(&mut self, remote: NodeId, request: GossipMessage) {
        let (topic_id, msg) = match request {
            GossipMessage::Join(req) => (req.inner.topic_id, req),
        };
        if let Some(topic) = self.topics.get(&topic_id) {
            if let Err(_err) = topic
                .send(ActorToTopic::RemoteConnected(remote, msg.tx, msg.rx))
                .await
            {
                warn!(topic=%topic_id.fmt_short(), "Topic actor dead");
            }
        } else {
            debug!(topic=%topic_id.fmt_short(), "ignore request: unknown topic");
        }
    }

    #[instrument("api", skip_all)]
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
        let actor = TopicActor {
            topic_id,
            state,
            rx,
            to_actor_tx,
            peer_data,
            timers: Default::default(),
            neighbors: Default::default(),
            out_events: Default::default(),
            api_senders: Default::default(),
            api_receivers: Default::default(),
            remote_senders: Default::default(),
            remote_receivers: Default::default(),
        };
        let task = task::spawn(actor.run());
        Self {
            tx,
            _task: AbortOnDropHandle::new(task),
        }
    }

    async fn send(&self, msg: ActorToTopic) -> Result<(), mpsc::error::SendError<ActorToTopic>> {
        self.tx.send(msg).await
    }
}

struct TopicActor {
    topic_id: TopicId,
    to_actor_tx: mpsc::Sender<TopicToActor>,
    state: State,
    rx: mpsc::Receiver<ActorToTopic>,
    timers: Timers<Timer>,
    neighbors: BTreeSet<NodeId>,
    peer_data: Direct<Option<PeerData>>,
    out_events: VecDeque<OutEvent>,
    api_senders: Vec<channel::mpsc::Sender<api::Event>>,
    api_receivers: MergeUnbounded<ApiRecvStream>,
    remote_senders: HashMap<NodeId, MaybeSender>,
    remote_receivers: MergeUnbounded<RemoteRecvStream>,
}

impl TopicActor {
    pub async fn run(mut self) {
        let peer_data = self.peer_data.clone().stream();
        tokio::pin!(peer_data);
        loop {
            tokio::select! {
                Some(msg) = self.rx.recv() => {
                    self.handle_message(msg).await;
                },
                Some(cmd) = self.api_receivers.next(), if !self.api_receivers.is_empty() => {
                    self.handle_api_command(cmd).await;
                }
                Some((remote, message)) = self.remote_receivers.next(), if !self.remote_receivers.is_empty() => {
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
                else => break,
            }
        }
    }

    async fn handle_message(&mut self, msg: ActorToTopic) {
        match msg {
            ActorToTopic::RemoteConnected(remote, tx, rx) => {
                self.remote_receivers
                    .push(Box::pin(into_stream(rx).map(move |msg| (remote, msg))));
                let sender = self.remote_senders.entry(remote).or_default();
                if let Err(err) = sender.init(tx).await {
                    warn!("Remote failed while pushing queued messages: {err:?}");
                }
            }
            ActorToTopic::Api(req) => {
                let WithChannels { inner, tx, rx, .. } = req;
                self.api_senders.push(tx);
                self.api_receivers.push(Box::pin(rx.into_stream()));
                self.handle_in_event(InEvent::Command(Command::Join(
                    inner.bootstrap.into_iter().collect(),
                )))
                .await;
            }
        }
    }

    async fn handle_remote_message(
        &mut self,
        remote: NodeId,
        message: Result<ProtoMessageOpt, RecvError>,
    ) {
        match message {
            Ok(Some(message)) => {
                self.handle_in_event(InEvent::RecvMessage(remote, message))
                    .await;
            }
            Err(err) => {
                warn!(remote=%remote.fmt_short(), ?err, "Recv stream from remote failed");
                self.handle_in_event(InEvent::PeerDisconnected(remote))
                    .await;
            }
            Ok(None) => {
                warn!(remote=%remote.fmt_short(), "Recv stream from remote closed");
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
        self.handle_in_event(InEvent::Command(command.into())).await;
    }

    async fn handle_in_event(&mut self, event: InEvent) {
        trace!("tick: in event {event:?}");
        let now = Instant::now();
        self.out_events.extend(self.state.handle(event, now));

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
                    // if let Some(peer) = self.peers.get_mut(&node_id) {
                    //     debug!(remote=%node_id.fmt_short(), "disable sender");
                    //     peer.sender = MaybeSender::None;
                    //     for handle in peer.recv_tasks.drain(..) {
                    //         handle.abort();
                    //     }
                    // }
                }
                OutEvent::PeerData(node_id, peer_data) => {
                    self.to_actor_tx
                        .send(TopicToActor::PeerData(node_id, peer_data))
                        .await;
                    // (self.on_peer_data)(node_id, peer_data),
                }
            }
        }
    }

    // TODO: Error handling
    async fn send(&mut self, node_id: NodeId, message: ProtoMessage) {
        let sender = match self.remote_senders.entry(node_id) {
            hash_map::Entry::Occupied(entry) => entry.into_mut(),
            hash_map::Entry::Vacant(entry) => {
                self.to_actor_tx
                    .send(TopicToActor::Request(node_id, self.topic_id))
                    .await;
                entry.insert(Default::default())
            }
        };
        if let Err(err) = sender.send(message).await {
            warn!(?err, remote=%node_id.fmt_short(), "failed to send message")
            // TODO: Remove peer.
        }
    }

    async fn handle_event(&mut self, event: ProtoEvent) {
        match &event {
            ProtoEvent::NeighborUp(n) => {
                self.neighbors.insert(*n);
            }
            ProtoEvent::NeighborDown(n) => {
                self.neighbors.remove(n);
            }
            ProtoEvent::Received(_gossip_event) => {}
        }
        let event = api::Event::from(event);

        // This is an async send + retain (remove failed senders)
        // TODO: check perf and optimize if needed.
        // TODO: Add timeout to not let slow receivers stall everything.
        let to_remove = self
            .api_senders
            .iter()
            .enumerate()
            .map(async |(i, tx)| tx.send(event.clone()).await.err().map(|_| i))
            .join_all()
            .await
            .into_iter()
            .flatten();
        for i in to_remove.rev() {
            self.api_senders.remove(i);
        }
    }
}

#[derive(Debug)]
enum MaybeSender {
    Active(channel::mpsc::Sender<ProtoMessageOpt>),
    Pending {
        queue: Vec<ProtoMessage>,
        closed: bool,
    },
    Closed,
}

impl MaybeSender {
    async fn send(&mut self, message: ProtoMessage) -> Result<(), channel::SendError> {
        match self {
            // TODO: Better error
            Self::Closed | Self::Pending { closed: true, .. } => {
                Err(channel::SendError::ReceiverClosed)
            }
            Self::Active(sender) => sender.send(Some(message)).await,
            Self::Pending {
                queue: messages, ..
            } => {
                messages.push(message);
                Ok(())
            }
        }
    }

    async fn close(&mut self) -> Result<(), channel::SendError> {
        match self {
            Self::Active(sender) => {
                sender.send(None).await?;
                *self = Self::Closed;
                Ok(())
            }
            Self::Pending { queue: _, closed } => {
                *closed = true;
                Ok(())
            }
            // TODO: Better error
            Self::Closed => Ok(()),
        }
    }

    async fn init(
        &mut self,
        sender: channel::mpsc::Sender<ProtoMessageOpt>,
    ) -> Result<(), channel::SendError> {
        *self = match self {
            Self::Active(old) => {
                old.send(None).await?;
                debug!("Dropping old sender");
                Self::Active(sender)
            }
            Self::Pending { queue, closed } => {
                for msg in queue.drain(..) {
                    sender.send(Some(msg)).await?;
                }
                if *closed {
                    sender.send(None).await?;
                    Self::Closed
                } else {
                    Self::Active(sender)
                }
            }
            Self::Closed => {
                info!("init called while sender is closed, reopen");
                Self::Active(sender)
            }
        };
        Ok(())
    }
}

impl Default for MaybeSender {
    fn default() -> Self {
        Self::Pending {
            queue: Vec::new(),
            closed: false,
        }
    }
}

fn into_stream(
    receiver: channel::mpsc::Receiver<ProtoMessageOpt>,
) -> impl Stream<Item = Result<ProtoMessageOpt, RecvError>> + Send + Sync + 'static {
    n0_future::stream::unfold(Some(receiver), |recv| async move {
        let mut recv = recv?;
        match recv.recv().await {
            Err(err) => Some((Err(err), None)),
            Ok(Some(Some(res))) => Some((Ok(Some(res)), Some(recv))),
            Ok(Some(None)) => Some((Ok(None), None)),
            Ok(None) => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use iroh::{protocol::Router, Watcher};
    use n0_snafu::ResultExt;
    use tokio::time::timeout;
    use tracing_test::traced_test;

    use super::*;
    #[tokio::test]
    #[traced_test]
    async fn gossip_smokenew() -> n0_snafu::Result<()> {
        tracing_subscriber::fmt::try_init().ok();
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
    use tracing::{info, instrument};
    use tracing_test::traced_test;

    use super::*;
    use crate::{
        api::{ApiError, Event},
        ALPN,
    };

    type EndpointHandle = tokio::task::JoinHandle<Result<()>>;

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
