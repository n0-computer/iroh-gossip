//! Public API for using iroh-gossip
//!
//! The API is usable both locally and over RPC.

use std::{
    collections::{BTreeSet, HashSet},
    pin::Pin,
    sync::{Arc, OnceLock},
    task::{Context, Poll},
};

use bytes::Bytes;
use iroh_base::EndpointId;
use irpc::{
    channel::{mpsc, oneshot},
    rpc_requests, Client,
};
use n0_error::{e, stack_error};
use n0_future::{Stream, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};

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
    /// Asks for the largest message content that a broadcast takes.
    #[rpc(tx=oneshot::Sender<usize>)]
    MaxContentSize(MaxContentSizeRequest),
}

/// The request for the size limit of the server, see [`GossipSender::max_content_size`].
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct MaxContentSizeRequest;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct JoinRequest {
    pub topic_id: TopicId,
    pub bootstrap: BTreeSet<EndpointId>,
}

#[allow(missing_docs)]
#[stack_error(derive, add_meta, from_sources)]
#[non_exhaustive]
pub enum ApiError {
    #[error(transparent)]
    Rpc { source: irpc::Error },
    /// The gossip topic was closed.
    #[error("topic closed")]
    Closed,
    /// The message content is over the limit, see [`GossipSender::max_content_size`].
    #[error("message of {size} bytes is over the limit of {max_content} bytes")]
    TooLarge {
        /// The size of the content.
        size: usize,
        /// The largest content that a broadcast takes.
        max_content: usize,
    },
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
    /// The largest content that a broadcast takes, once known.
    ///
    /// An RPC client asks its server on its first subscribe.
    max_content: Arc<OnceLock<usize>>,
}

impl GossipApi {
    #[cfg(feature = "net")]
    pub(crate) fn local(
        tx: tokio::sync::mpsc::Sender<RpcMessage>,
        max_message_size: usize,
    ) -> Self {
        let local = irpc::LocalSender::<Request>::from(tx);
        let max_content = crate::proto::topic::max_content_size(max_message_size);
        Self {
            client: local.into(),
            max_content: Arc::new(OnceLock::from(max_content)),
        }
    }

    /// Connect to a remote as a RPC client.
    #[cfg(feature = "rpc")]
    pub fn connect(endpoint: noq::Endpoint, addr: std::net::SocketAddr) -> Self {
        let inner = irpc::Client::noq(endpoint, addr);
        Self {
            client: inner,
            max_content: Default::default(),
        }
    }

    /// Listen on a noq endpoint for incoming RPC connections.
    #[cfg(all(feature = "rpc", feature = "net"))]
    pub(crate) async fn listen(&self, endpoint: noq::Endpoint) {
        use irpc::rpc::{listen, RemoteService};

        let local = self
            .client
            .as_local()
            .expect("cannot listen on remote client");
        let handler = Request::remote_handler(local);

        listen::<Request>(endpoint, handler).await
    }

    /// Returns the largest content that a broadcast takes, and asks the server once.
    ///
    /// A server older than 0.102.0 does not know the request, see
    /// [`GossipSender::max_content_size`]. Such a server stops serving the
    /// connection on it, so we ask only once, as the first request, and keep
    /// the fallback too.
    async fn max_content_size(&self) -> usize {
        if let Some(max_content) = self.max_content.get() {
            return *max_content;
        }
        let max_content = match self.client.rpc(MaxContentSizeRequest).await {
            Ok(max_content) => max_content,
            Err(err) => {
                tracing::debug!("the server reported no size limit: {err:#}");
                usize::MAX
            }
        };
        *self.max_content.get_or_init(|| max_content)
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
        let max_content = self.max_content_size().await;
        let req = JoinRequest {
            topic_id,
            bootstrap: opts.bootstrap,
        };
        let (tx, rx) = self
            .client
            .bidi_streaming(req, TOPIC_COMMANDS_CAP, opts.subscription_capacity)
            .await?;
        Ok(GossipTopic::new(tx, rx, max_content))
    }

    /// Join a gossip topic with the default options and wait for at least one active connection.
    pub async fn subscribe_and_join(
        &self,
        topic_id: TopicId,
        bootstrap: Vec<EndpointId>,
    ) -> Result<GossipTopic, ApiError> {
        let mut sub = self
            .subscribe_with_opts(topic_id, JoinOptions::with_bootstrap(bootstrap))
            .await?;
        sub.joined().await?;
        Ok(sub)
    }

    /// Join a gossip topic with the default options.
    ///
    /// Note that this will not wait for any bootstrap endpoint to be available.
    /// To ensure the topic is connected to at least one endpoint, use [`GossipTopic::joined`]
    /// or [`Self::subscribe_and_join`]
    pub async fn subscribe(
        &self,
        topic_id: TopicId,
        bootstrap: Vec<EndpointId>,
    ) -> Result<GossipTopic, ApiError> {
        let sub = self
            .subscribe_with_opts(topic_id, JoinOptions::with_bootstrap(bootstrap))
            .await?;

        Ok(sub)
    }
}

/// Sender for a gossip topic.
///
/// The commands of a topic keep their order. While the queues to all
/// neighbors of the topic are half full with data, they wait: a broadcast, a
/// join of more peers, and the leave when the last handle drops. See the known
/// limits of `QueueOptions` in the `net` module.
#[derive(Debug, Clone)]
pub struct GossipSender {
    sender: mpsc::Sender<Command>,
    max_content: usize,
}

impl GossipSender {
    pub(crate) fn new(sender: mpsc::Sender<Command>, max_content: usize) -> Self {
        Self {
            sender,
            max_content,
        }
    }

    /// Returns the largest message content that [`Self::broadcast`] takes.
    ///
    /// It is [`Builder::max_message_size`] less the encoding, about 40 bytes,
    /// and holds for [`Self::broadcast_neighbors`] too. Over a remote RPC
    /// connection, it is the limit of the server. If that server is older than
    /// iroh-gossip 0.102.0, it is `usize::MAX`.
    ///
    /// [`Builder::max_message_size`]: crate::net::Builder::max_message_size
    pub fn max_content_size(&self) -> usize {
        self.max_content
    }

    /// Returns an error if the message content is over the limit.
    fn check_size(&self, message: &Bytes) -> Result<(), ApiError> {
        if message.len() > self.max_content {
            return Err(e!(ApiError::TooLarge {
                size: message.len(),
                max_content: self.max_content
            }));
        }
        Ok(())
    }

    /// Broadcasts a message to all endpoints.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::TooLarge`] if the content is over
    /// [`Self::max_content_size`].
    pub async fn broadcast(&self, message: Bytes) -> Result<(), ApiError> {
        self.check_size(&message)?;
        self.send(Command::Broadcast(message)).await?;
        Ok(())
    }

    /// Broadcasts a message to our direct neighbors.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::TooLarge`] as [`Self::broadcast`] does.
    pub async fn broadcast_neighbors(&self, message: Bytes) -> Result<(), ApiError> {
        self.check_size(&message)?;
        self.send(Command::BroadcastNeighbors(message)).await?;
        Ok(())
    }

    /// Joins a set of peers.
    pub async fn join_peers(&self, peers: Vec<EndpointId>) -> Result<(), ApiError> {
        self.send(Command::JoinPeers(peers)).await?;
        Ok(())
    }

    async fn send(&self, command: Command) -> Result<(), irpc::channel::SendError> {
        self.sender.send(command).await?;
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
    pub(crate) fn new(
        sender: mpsc::Sender<Command>,
        receiver: mpsc::Receiver<Event>,
        max_content: usize,
    ) -> Self {
        let sender = GossipSender::new(sender, max_content);
        Self {
            sender,
            receiver: GossipReceiver::new(receiver),
        }
    }

    /// Returns the largest message content that [`Self::broadcast`] takes.
    ///
    /// See [`GossipSender::max_content_size`].
    pub fn max_content_size(&self) -> usize {
        self.sender.max_content_size()
    }

    /// Splits `self` into [`GossipSender`] and [`GossipReceiver`] parts.
    pub fn split(self) -> (GossipSender, GossipReceiver) {
        (self.sender, self.receiver)
    }

    /// Sends a message to all peers.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::TooLarge`] as [`GossipSender::broadcast`] does.
    pub async fn broadcast(&mut self, message: Bytes) -> Result<(), ApiError> {
        self.sender.broadcast(message).await
    }

    /// Sends a message to our direct neighbors in the swarm.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::TooLarge`] as [`GossipSender::broadcast`] does.
    pub async fn broadcast_neighbors(&mut self, message: Bytes) -> Result<(), ApiError> {
        self.sender.broadcast_neighbors(message).await
    }

    /// Lists our current direct neighbors.
    pub fn neighbors(&self) -> impl Iterator<Item = EndpointId> + '_ {
        self.receiver.neighbors()
    }

    /// Waits until we are connected to at least one endpoint.
    ///
    /// See [`GossipReceiver::joined`] for details.
    pub async fn joined(&mut self) -> Result<(), ApiError> {
        self.receiver.joined().await
    }

    /// Returns `true` if we are connected to at least one endpoint.
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
    neighbors: HashSet<EndpointId>,
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
    pub fn neighbors(&self) -> impl Iterator<Item = EndpointId> + '_ {
        self.neighbors.iter().copied()
    }

    /// Waits until we are connected to at least one endpoint.
    ///
    /// Progresses the event stream to the first [`Event::NeighborUp`] event.
    ///
    /// Note that this consumes this initial `NeighborUp` event. If you want to track
    /// neighbors, use [`Self::neighbors`] after awaiting [`Self::joined`], and then
    /// continue to track `NeighborUp` events on the event stream.
    pub async fn joined(&mut self) -> Result<(), ApiError> {
        while !self.is_joined() {
            let _event = self.next().await.ok_or(e!(ApiError::Closed))??;
        }
        Ok(())
    }

    /// Returns `true` if we are connected to at least one endpoint.
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
                Event::NeighborUp(endpoint_id) => {
                    self.neighbors.insert(*endpoint_id);
                }
                Event::NeighborDown(endpoint_id) => {
                    self.neighbors.remove(endpoint_id);
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
#[non_exhaustive]
pub enum Event {
    /// We have a new, direct neighbor in the swarm membership layer for this topic.
    NeighborUp(EndpointId),
    /// We dropped direct neighbor in the swarm membership layer for this topic.
    NeighborDown(EndpointId),
    /// We received a gossip message for this topic.
    Received(Message),
    /// We missed some messages because our [`GossipReceiver`] was not progressing fast enough.
    Lagged,
}

impl From<crate::proto::Event<EndpointId>> for Event {
    fn from(event: crate::proto::Event<EndpointId>) -> Self {
        match event {
            crate::proto::Event::NeighborUp(endpoint_id) => Self::NeighborUp(endpoint_id),
            crate::proto::Event::NeighborDown(endpoint_id) => Self::NeighborDown(endpoint_id),
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
    /// The endpoint that delivered the message. This is not the same as the original author.
    pub delivered_from: EndpointId,
}

/// Command for a gossip topic.
#[derive(Serialize, Deserialize, derive_more::Debug, Clone)]
#[non_exhaustive]
pub enum Command {
    /// Broadcasts a message to all endpoints in the swarm.
    Broadcast(#[debug("Bytes({})", _0.len())] Bytes),
    /// Broadcasts a message to all direct neighbors.
    BroadcastNeighbors(#[debug("Bytes({})", _0.len())] Bytes),
    /// Connects to a set of peers.
    JoinPeers(Vec<EndpointId>),
}

/// Options for joining a gossip topic.
#[derive(Serialize, Deserialize, Debug)]
pub struct JoinOptions {
    /// The initial bootstrap endpoints.
    pub bootstrap: BTreeSet<EndpointId>,
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
    /// Creates [`JoinOptions`] with the provided bootstrap endpoints and the default subscription
    /// capacity.
    pub fn with_bootstrap(endpoints: impl IntoIterator<Item = EndpointId>) -> Self {
        Self {
            bootstrap: endpoints.into_iter().collect(),
            subscription_capacity: TOPIC_EVENTS_DEFAULT_CAP,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::api::GossipTopic;

    #[cfg(all(feature = "rpc", feature = "net"))]
    #[tokio::test]
    #[n0_tracing_test::traced_test]
    async fn test_rpc() -> n0_error::Result<()> {
        use iroh::{address_lookup::memory::MemoryLookup, protocol::Router, RelayMap};
        use n0_error::{AnyError, Result, StackResultExt, StdResultExt};
        use n0_future::{time::Duration, StreamExt};
        use rand::SeedableRng;

        use crate::{
            api::{Event, GossipApi},
            net::{tests::create_endpoint, Gossip},
            proto::TopicId,
            ALPN,
        };

        let mut rng = rand::rngs::ChaCha12Rng::seed_from_u64(1);
        let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();

        async fn create_gossip_endpoint(
            rng: &mut rand::rngs::ChaCha12Rng,
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

        // create our gossip endpoint
        let (router, gossip) = create_gossip_endpoint(&mut rng, relay_map.clone()).await?;

        // create a second endpoint so that we can test actually joining
        let (endpoint2_id, endpoint2_addr, endpoint2_task) = {
            let (router, gossip) = create_gossip_endpoint(&mut rng, relay_map.clone()).await?;
            let endpoint_addr = router.endpoint().addr();
            let endpoint_id = router.endpoint().id();
            let task = tokio::task::spawn(async move {
                let mut topic = gossip.subscribe_and_join(topic_id, vec![]).await?;
                topic.broadcast(b"hello".to_vec().into()).await?;
                Ok::<_, AnyError>(router)
            });
            (endpoint_id, endpoint_addr, task)
        };

        // create a memory lookup service to add endpoint addr manually
        let memory_lookup = MemoryLookup::new();
        memory_lookup.add_endpoint_info(endpoint2_addr);

        router.endpoint().address_lookup()?.add(memory_lookup);

        // The size limit of the server, which an RPC client asks for.
        let max_content = crate::proto::topic::max_content_size(gossip.max_message_size());

        // expose the gossip endpoint over RPC
        let (rpc_server_endpoint, rpc_server_cert) =
            irpc::util::make_server_endpoint("127.0.0.1:0".parse().unwrap())
                .context("make server endpoint")?;
        let rpc_server_addr = rpc_server_endpoint
            .local_addr()
            .std_context("resolve server addr")?;
        let rpc_server_task = tokio::task::spawn(async move {
            gossip.listen(rpc_server_endpoint).await;
        });

        // connect to the RPC endpoint with a new client
        let rpc_client_endpoint =
            irpc::util::make_client_endpoint("127.0.0.1:0".parse().unwrap(), &[&rpc_server_cert])
                .context("make client endpoint")?;
        let rpc_client = GossipApi::connect(rpc_client_endpoint, rpc_server_addr);

        // join via RPC
        let recv = async move {
            let mut topic = rpc_client
                .subscribe_and_join(topic_id, vec![endpoint2_id])
                .await?;
            // The client refuses an oversized message itself, as a local one does.
            assert_eq!(topic.max_content_size(), max_content);
            let refused = topic.broadcast(vec![0; max_content + 1].into()).await;
            assert!(
                matches!(refused, Err(super::ApiError::TooLarge { .. })),
                "{refused:?}"
            );
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
            Ok::<_, AnyError>(())
        };

        // timeout to not hang in case of failure
        tokio::time::timeout(Duration::from_secs(10), recv)
            .await
            .std_context("rpc recv timeout")??;

        // shutdown
        rpc_server_task.abort();
        router.shutdown().await.std_context("shutdown router")?;
        let router2 = endpoint2_task.await.std_context("join endpoint task")??;
        router2
            .shutdown()
            .await
            .std_context("shutdown second router")?;
        Ok(())
    }

    /// A server that does not answer the size request is asked once, and its client keeps `usize::MAX`.
    ///
    /// A server older than 0.102.0 stops serving the connection on that request,
    /// so a second request would cut off the topics on it.
    #[cfg(feature = "net")]
    #[tokio::test]
    async fn unanswered_size_request_is_asked_once() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<super::RpcMessage>(4);
        let api = super::GossipApi {
            client: irpc::LocalSender::<super::Request>::from(tx).into(),
            max_content: Default::default(),
        };
        // Drops each request unanswered.
        let asked = tokio::spawn(async move {
            let mut asked = 0;
            while let Some(message) = rx.recv().await {
                if matches!(message, super::RpcMessage::MaxContentSize(_)) {
                    asked += 1;
                }
            }
            asked
        });

        assert_eq!(api.max_content_size().await, usize::MAX);
        assert_eq!(api.max_content_size().await, usize::MAX);
        drop(api);
        assert_eq!(asked.await.expect("server task"), 1);
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
