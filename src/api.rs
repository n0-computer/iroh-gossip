//! Public API for using iroh-gossip
//!
//! The API is usable both locally and over RPC.

use std::{
    collections::{BTreeSet, HashSet},
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use iroh_base::NodeId;
use irpc::{channel::mpsc, rpc_requests, Client};
use n0_future::{Stream, StreamExt, TryStreamExt};
use nested_enum_utils::common_fields;
use serde::{Deserialize, Serialize};
use snafu::Snafu;

use crate::proto::{DeliveryScope, TopicId};

/// Default channel capacity for topic subscription channels (one per topic)
const TOPIC_EVENTS_DEFAULT_CAP: usize = 2048;
/// Channel capacity for topic command send channels.
const TOPIC_COMMANDS_CAP: usize = 64;

/// Input messages for the gossip actor.
#[rpc_requests(message = RpcMessage, rpc_feature = "rpc")]
#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum Request {
    #[rpc(tx=mpsc::Sender<Event>, rx=mpsc::Receiver<Command>)]
    Join(JoinRequest),
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct JoinRequest {
    pub topic_id: TopicId,
    pub bootstrap: BTreeSet<NodeId>,
}

#[allow(missing_docs)]
#[common_fields({
    backtrace: Option<snafu::Backtrace>,
    #[snafu(implicit)]
    span_trace: n0_snafu::SpanTrace,
})]
#[derive(Debug, Snafu)]
#[non_exhaustive]
pub enum ApiError {
    #[snafu(transparent)]
    Rpc { source: irpc::Error },
    /// The gossip topic was closed.
    #[snafu(display("topic closed"))]
    Closed {},
}

impl From<irpc::channel::SendError> for ApiError {
    fn from(value: irpc::channel::SendError) -> Self {
        irpc::Error::from(value).into()
    }
}

impl From<irpc::channel::mpsc::RecvError> for ApiError {
    fn from(value: irpc::channel::mpsc::RecvError) -> Self {
        irpc::Error::from(value).into()
    }
}

impl From<irpc::channel::oneshot::RecvError> for ApiError {
    fn from(value: irpc::channel::oneshot::RecvError) -> Self {
        irpc::Error::from(value).into()
    }
}

/// API to control a [`Gossip`] instance.
///
/// This has methods to subscribe and join gossip topics, which return handles to publish
/// and receive messages on topics.
///
/// [`Gossip`] derefs to [`GossipApi`], so all functions on [`GossipApi`] are directly callable
/// from [`Gossip`].
///
/// Additionally, a [`GossipApi`] can be created by connecting to an RPC server. See [`Gossip::listen`]
/// and [`GossipApi::connect`] (*requires the `rpc` feature).
///
/// [`Gossip`]: crate::net::Gossip
/// [`Gossip::listen`]: crate::net::Gossip::listen
#[derive(Debug, Clone)]
pub struct GossipApi {
    client: Client<Request>,
}

impl GossipApi {
    #[cfg(feature = "net")]
    pub(crate) fn local(tx: tokio::sync::mpsc::Sender<RpcMessage>) -> Self {
        let local = irpc::LocalSender::<Request>::from(tx);
        Self {
            client: local.into(),
        }
    }

    /// Connect to a remote as a RPC client.
    #[cfg(feature = "rpc")]
    pub fn connect(endpoint: quinn::Endpoint, addr: std::net::SocketAddr) -> Self {
        let inner = irpc::Client::quinn(endpoint, addr);
        Self { client: inner }
    }

    /// Listen on a quinn endpoint for incoming RPC connections.
    #[cfg(all(feature = "rpc", feature = "net"))]
    pub(crate) async fn listen(&self, endpoint: quinn::Endpoint) {
        use irpc::rpc::{listen, RemoteService};

        let local = self
            .client
            .as_local()
            .expect("cannot listen on remote client");
        let handler = Request::remote_handler(local);

        listen::<Request>(endpoint, handler).await
    }

    /// Join a gossip topic with options.
    ///
    /// Returns a [`GossipTopic`] instantly. To wait for at least one connection to be established,
    /// you can await [`GossipTopic::joined`].
    ///
    /// Messages will be queued until a first connection is available. If the internal channel becomes full,
    /// the oldest messages will be dropped from the channel.
    pub async fn subscribe_with_opts(
        &self,
        topic_id: TopicId,
        opts: JoinOptions,
    ) -> Result<GossipTopic, ApiError> {
        let req = JoinRequest {
            topic_id,
            bootstrap: opts.bootstrap,
        };
        let (tx, rx) = self
            .client
            .bidi_streaming(req, TOPIC_COMMANDS_CAP, opts.subscription_capacity)
            .await?;
        Ok(GossipTopic::new(tx, rx))
    }

    /// Join a gossip topic with the default options and wait for at least one active connection.
    pub async fn subscribe_and_join(
        &self,
        topic_id: TopicId,
        bootstrap: Vec<NodeId>,
    ) -> Result<GossipTopic, ApiError> {
        let mut sub = self
            .subscribe_with_opts(topic_id, JoinOptions::with_bootstrap(bootstrap))
            .await?;
        sub.joined().await?;
        Ok(sub)
    }

    /// Join a gossip topic with the default options.
    ///
    /// Note that this will not wait for any bootstrap node to be available.
    /// To ensure the topic is connected to at least one node, use [`GossipTopic::joined`]
    /// or [`Self::subscribe_and_join`]
    pub async fn subscribe(
        &self,
        topic_id: TopicId,
        bootstrap: Vec<NodeId>,
    ) -> Result<GossipTopic, ApiError> {
        let sub = self
            .subscribe_with_opts(topic_id, JoinOptions::with_bootstrap(bootstrap))
            .await?;

        Ok(sub)
    }
}

/// Sender for a gossip topic.
#[derive(Debug, Clone)]
pub struct GossipSender(mpsc::Sender<Command>);

impl GossipSender {
    pub(crate) fn new(sender: mpsc::Sender<Command>) -> Self {
        Self(sender)
    }

    /// Broadcasts a message to all nodes.
    pub async fn broadcast(&self, message: Bytes) -> Result<(), ApiError> {
        self.send(Command::Broadcast(message)).await?;
        Ok(())
    }

    /// Broadcasts a message to our direct neighbors.
    pub async fn broadcast_neighbors(&self, message: Bytes) -> Result<(), ApiError> {
        self.send(Command::BroadcastNeighbors(message)).await?;
        Ok(())
    }

    /// Joins a set of peers.
    pub async fn join_peers(&self, peers: Vec<NodeId>) -> Result<(), ApiError> {
        self.send(Command::JoinPeers(peers)).await?;
        Ok(())
    }

    async fn send(&self, command: Command) -> Result<(), irpc::channel::SendError> {
        self.0.send(command).await?;
        Ok(())
    }
}

/// Subscribed gossip topic.
///
/// This handle is a [`Stream`] of [`Event`]s from the topic, and can be used to send messages.
///
/// Once the [`GossipTopic`] is dropped, the network actor will leave the gossip topic.
///
/// It may be split into sender and receiver parts with [`Self::split`]. In this case, the topic will
/// be left once both the [`GossipSender`] and [`GossipReceiver`] halves are dropped.
#[derive(Debug)]
pub struct GossipTopic {
    sender: GossipSender,
    receiver: GossipReceiver,
}

impl GossipTopic {
    pub(crate) fn new(sender: mpsc::Sender<Command>, receiver: mpsc::Receiver<Event>) -> Self {
        let sender = GossipSender::new(sender);
        Self {
            sender,
            receiver: GossipReceiver::new(receiver),
        }
    }

    /// Splits `self` into [`GossipSender`] and [`GossipReceiver`] parts.
    pub fn split(self) -> (GossipSender, GossipReceiver) {
        (self.sender, self.receiver)
    }

    /// Sends a message to all peers.
    pub async fn broadcast(&mut self, message: Bytes) -> Result<(), ApiError> {
        self.sender.broadcast(message).await
    }

    /// Sends a message to our direct neighbors in the swarm.
    pub async fn broadcast_neighbors(&mut self, message: Bytes) -> Result<(), ApiError> {
        self.sender.broadcast_neighbors(message).await
    }

    /// Waits until we are connected to at least one node.
    ///
    /// See [`GossipReceiver::joined`] for details.
    pub async fn joined(&mut self) -> Result<(), ApiError> {
        self.receiver.joined().await
    }

    /// Returns `true` if we are connected to at least one node.
    pub fn is_joined(&self) -> bool {
        self.receiver.is_joined()
    }
}

impl Stream for GossipTopic {
    type Item = Result<Event, ApiError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.receiver).poll_next(cx)
    }
}

/// Receiver for gossip events on a topic.
///
/// This is a [`Stream`] of [`Event`]s emitted from the topic.
#[derive(derive_more::Debug)]
pub struct GossipReceiver {
    #[debug("BoxStream")]
    stream: Pin<Box<dyn Stream<Item = Result<Event, ApiError>> + Send + Sync + 'static>>,
    neighbors: HashSet<NodeId>,
}

impl GossipReceiver {
    pub(crate) fn new(events_rx: mpsc::Receiver<Event>) -> Self {
        let stream = events_rx.into_stream().map_err(ApiError::from);
        let stream = Box::pin(stream);
        Self {
            stream,
            neighbors: Default::default(),
        }
    }

    /// Lists our current direct neighbors.
    pub fn neighbors(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.neighbors.iter().copied()
    }

    /// Waits until we are connected to at least one node.
    ///
    /// Progresses the event stream to the first [`Event::NeighborUp`] event.
    ///
    /// Note that this consumes this initial `NeighborUp` event. If you want to track
    /// neighbors, use [`Self::neighbors`] after awaiting [`Self::joined`], and then
    /// continue to track `NeighborUp` events on the event stream.
    pub async fn joined(&mut self) -> Result<(), ApiError> {
        while !self.is_joined() {
            let _event = self.next().await.ok_or(ClosedSnafu.build())??;
        }
        Ok(())
    }

    /// Returns `true` if we are connected to at least one node.
    pub fn is_joined(&self) -> bool {
        !self.neighbors.is_empty()
    }
}

impl Stream for GossipReceiver {
    type Item = Result<Event, ApiError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let item = std::task::ready!(Pin::new(&mut self.stream).poll_next(cx));
        if let Some(Ok(item)) = &item {
            match item {
                Event::NeighborUp(node_id) => {
                    self.neighbors.insert(*node_id);
                }
                Event::NeighborDown(node_id) => {
                    self.neighbors.remove(node_id);
                }
                _ => {}
            }
        }
        Poll::Ready(item)
    }
}

/// Events emitted from a gossip topic.
///
/// These are the events emitted from a [`GossipReceiver`].
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Event {
    /// We have a new, direct neighbor in the swarm membership layer for this topic.
    NeighborUp(NodeId),
    /// We dropped direct neighbor in the swarm membership layer for this topic.
    NeighborDown(NodeId),
    /// We received a gossip message for this topic.
    Received(Message),
    /// We missed some messages because our [`GossipReceiver`] was not progressing fast enough.
    Lagged,
}

impl From<crate::proto::Event<NodeId>> for Event {
    fn from(event: crate::proto::Event<NodeId>) -> Self {
        match event {
            crate::proto::Event::NeighborUp(node_id) => Self::NeighborUp(node_id),
            crate::proto::Event::NeighborDown(node_id) => Self::NeighborDown(node_id),
            crate::proto::Event::Received(message) => Self::Received(Message {
                content: message.content,
                scope: message.scope,
                delivered_from: message.delivered_from,
            }),
        }
    }
}

/// A gossip message
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, derive_more::Debug, Serialize, Deserialize)]
pub struct Message {
    /// The content of the message
    #[debug("Bytes({})", self.content.len())]
    pub content: Bytes,
    /// The scope of the message.
    /// This tells us if the message is from a direct neighbor or actual gossip.
    pub scope: DeliveryScope,
    /// The node that delivered the message. This is not the same as the original author.
    pub delivered_from: NodeId,
}

/// Command for a gossip topic.
#[derive(Serialize, Deserialize, derive_more::Debug, Clone)]
pub enum Command {
    /// Broadcasts a message to all nodes in the swarm.
    Broadcast(#[debug("Bytes({})", _0.len())] Bytes),
    /// Broadcasts a message to all direct neighbors.
    BroadcastNeighbors(#[debug("Bytes({})", _0.len())] Bytes),
    /// Connects to a set of peers.
    JoinPeers(Vec<NodeId>),
}

/// Options for joining a gossip topic.
#[derive(Serialize, Deserialize, Debug)]
pub struct JoinOptions {
    /// The initial bootstrap nodes.
    pub bootstrap: BTreeSet<NodeId>,
    /// The maximum number of messages that can be buffered in a subscription.
    ///
    /// If this limit is reached, the subscriber will receive a `Lagged` response,
    /// the message will be dropped, and the subscriber will be closed.
    ///
    /// This is to prevent a single slow subscriber from blocking the dispatch loop.
    /// If a subscriber is lagging, it should be closed and re-opened.
    pub subscription_capacity: usize,
}

impl JoinOptions {
    /// Creates [`JoinOptions`] with the provided bootstrap nodes and the default subscription
    /// capacity.
    pub fn with_bootstrap(nodes: impl IntoIterator<Item = NodeId>) -> Self {
        Self {
            bootstrap: nodes.into_iter().collect(),
            subscription_capacity: TOPIC_EVENTS_DEFAULT_CAP,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::api::GossipTopic;

    #[cfg(all(feature = "rpc", feature = "net"))]
    #[tokio::test]
    #[tracing_test::traced_test]
    async fn test_rpc() -> n0_snafu::Result {
        use iroh::{discovery::static_provider::StaticProvider, protocol::Router, RelayMap};
        use n0_future::{time::Duration, StreamExt};
        use n0_snafu::{Error, Result, ResultExt};
        use rand_chacha::rand_core::SeedableRng;

        use crate::{
            api::{Event, GossipApi},
            net::{test::create_endpoint, Gossip},
            proto::TopicId,
            ALPN,
        };

        let mut rng = rand_chacha::ChaCha12Rng::seed_from_u64(1);
        let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();

        async fn create_gossip_endpoint(
            rng: &mut rand_chacha::ChaCha12Rng,
            relay_map: RelayMap,
        ) -> Result<(Router, Gossip)> {
            let endpoint = create_endpoint(rng, relay_map, None).await?;
            let gossip = Gossip::builder().spawn(endpoint.clone());
            let router = Router::builder(endpoint)
                .accept(ALPN, gossip.clone())
                .spawn();
            Ok((router, gossip))
        }

        let topic_id = TopicId::from_bytes([0u8; 32]);

        // create our gossip node
        let (router, gossip) = create_gossip_endpoint(&mut rng, relay_map.clone()).await?;

        // create a second node so that we can test actually joining
        let (node2_id, node2_addr, node2_task) = {
            let (router, gossip) = create_gossip_endpoint(&mut rng, relay_map.clone()).await?;
            let node_addr = router.endpoint().node_addr();
            let node_id = router.endpoint().node_id();
            let task = tokio::task::spawn(async move {
                let mut topic = gossip.subscribe_and_join(topic_id, vec![]).await?;
                topic.broadcast(b"hello".to_vec().into()).await?;
                Result::<_, Error>::Ok(router)
            });
            (node_id, node_addr, task)
        };

        // create static provider to add node addr manually
        let static_provider = StaticProvider::new();
        static_provider.add_node_info(node2_addr);

        router.endpoint().discovery().add(static_provider);

        // expose the gossip node over RPC
        let (rpc_server_endpoint, rpc_server_cert) =
            irpc::util::make_server_endpoint("127.0.0.1:0".parse().unwrap())
                .map_err(Error::anyhow)?;
        let rpc_server_addr = rpc_server_endpoint.local_addr().e()?;
        let rpc_server_task = tokio::task::spawn(async move {
            gossip.listen(rpc_server_endpoint).await;
        });

        // connect to the RPC node with a new client
        let rpc_client_endpoint =
            irpc::util::make_client_endpoint("127.0.0.1:0".parse().unwrap(), &[&rpc_server_cert])
                .map_err(Error::anyhow)?;
        let rpc_client = GossipApi::connect(rpc_client_endpoint, rpc_server_addr);

        // join via RPC
        let recv = async move {
            let mut topic = rpc_client
                .subscribe_and_join(topic_id, vec![node2_id])
                .await?;
            // wait for a message
            while let Some(event) = topic.try_next().await? {
                match event {
                    Event::Received(message) => {
                        assert_eq!(&message.content[..], b"hello");
                        break;
                    }
                    Event::Lagged => panic!("unexpected lagged event"),
                    _ => {}
                }
            }
            Result::<_, Error>::Ok(())
        };

        // timeout to not hang in case of failure
        tokio::time::timeout(Duration::from_secs(10), recv)
            .await
            .e()??;

        // shutdown
        rpc_server_task.abort();
        router.shutdown().await.e()?;
        let router2 = node2_task.await.e()??;
        router2.shutdown().await.e()?;
        Ok(())
    }

    #[test]
    fn ensure_gossip_topic_is_sync() {
        #[allow(unused)]
        fn get() -> GossipTopic {
            unimplemented!()
        }
        #[allow(unused)]
        fn check(_t: impl Sync) {}
        #[allow(unused)]
        fn foo() {
            check(get());
        }
    }
}
