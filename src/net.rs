//! Networking for the `iroh-gossip` protocol

use std::{
    collections::{BTreeSet, HashMap},
    net::SocketAddr,
    sync::Arc,
};

use futures_util::FutureExt;
use iroh::{
    endpoint::{Connection, DirectAddr},
    protocol::ProtocolHandler,
    watchable::Watchable,
    Endpoint, NodeAddr, NodeId, RelayUrl,
};
use irpc::WithChannels;
use n0_future::{
    boxed::BoxFuture,
    task::{self, AbortOnDropHandle},
    Stream, StreamExt as _,
};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinSet,
};
use tracing::{debug, error_span, trace, warn, Instrument};

use crate::{
    api::{self, GossipApi, RpcMessage},
    metrics::Metrics,
    proto::{HyparviewConfig, PeerData, PlumtreeConfig, TopicId},
};

use self::connections::ConnectionPool;
use self::topic::{Config, SubscribeChannels, ToTopic, TopicHandle};

mod connections;
mod topic;
mod util;

/// ALPN protocol name
pub const GOSSIP_ALPN: &[u8] = b"/iroh-gossip/0";

/// Channel capacity for the ToActor message queue (single)
const TO_ACTOR_CAP: usize = 64;

/// Net related errors
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The gossip actor is closed, and cannot receive new messages
    #[error("Actor closed")]
    ActorClosed,
    /// First event received that was not `Joined`
    #[error("Joined event to be the first event received")]
    UnexpectedEvent,
    /// The gossip message receiver closed
    #[error("Receiver closed")]
    ReceiverClosed,
    /// Ser/De error
    #[error("Ser/De {0}")]
    SerDe(#[from] postcard::Error),
    /// Tried to construct empty peer data
    #[error("empty peer data")]
    EmptyPeerData,
    // /// Writing a message to the network
    // #[error("write {0}")]
    // Write(#[from] util_old::WriteError),
    // /// Reading a message from the network
    // #[error("read {0}")]
    // Read(#[from] util_old::ReadError),
    /// A watchable disconnected.
    #[error(transparent)]
    WatchableDisconnected(#[from] iroh::watchable::Disconnected),
    /// Iroh connection error
    #[error(transparent)]
    IrohConnection(#[from] iroh::endpoint::ConnectionError),
    /// Errors coming from `iroh`
    #[error(transparent)]
    Iroh(#[from] anyhow::Error),
    /// Task join failure
    #[error("join")]
    Join(#[from] task::JoinError),
    /// Failed to send a request.
    #[error(transparent)]
    RpcSend(#[from] irpc::channel::SendError),
    /// Failed to receive a response.
    #[error(transparent)]
    RpcRecv(#[from] irpc::channel::RecvError),
}

impl<T> From<async_channel::SendError<T>> for Error {
    fn from(_value: async_channel::SendError<T>) -> Self {
        Error::ActorClosed
    }
}

impl<T> From<mpsc::error::SendError<T>> for Error {
    fn from(_value: mpsc::error::SendError<T>) -> Self {
        Error::ActorClosed
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

#[derive(Debug)]
pub(crate) struct Inner {
    api: GossipApi,
    local_tx: mpsc::Sender<LocalActorMessage>,
    _actor_handle: AbortOnDropHandle<()>,
    max_message_size: usize,
    metrics: Arc<Metrics>,
}

impl ProtocolHandler for Gossip {
    fn accept(&self, conn: Connection) -> BoxFuture<anyhow::Result<()>> {
        let this = self.clone();
        Box::pin(async move {
            this.handle_connection(conn).await?;
            Ok(())
        })
    }

    fn shutdown(&self) -> BoxFuture<()> {
        let this = self.clone();
        Box::pin(async move {
            if let Err(err) = this.shutdown().await {
                warn!("error while shutting down gossip: {err:#}");
            }
        })
    }
}

/// Builder to configure and construct [`Gossip`].
#[derive(Debug, Clone)]
pub struct Builder {
    config: topic::Config,
}

impl Builder {
    /// Sets the maximum message size in bytes.
    /// By default this is `4096` bytes.
    pub fn max_message_size(mut self, size: usize) -> Self {
        self.config.proto.max_message_size = size;
        self
    }

    /// Set the membership configuration.
    pub fn membership_config(mut self, config: HyparviewConfig) -> Self {
        self.config.proto.membership = config;
        self
    }

    /// Set the broadcast configuration.
    pub fn broadcast_config(mut self, config: PlumtreeConfig) -> Self {
        self.config.proto.broadcast = config;
        self
    }

    /// Spawn a gossip actor and get a handle for it
    pub async fn spawn(self, endpoint: Endpoint) -> Result<Gossip, Error> {
        let metrics = Arc::new(Metrics::default());

        // We want to wait for our endpoint to be addressable by other nodes before launching gossip,
        // because otherwise our Join messages, which will be forwarded into the swarm through a random
        // walk, might not include an address to talk back to us.
        // `Endpoint::node_addr` always waits for direct addresses to be available, which never completes
        // when running as WASM in browser. Therefore, we instead race the futures that wait for the direct
        // addresses or the home relay to be initialized, and construct our node address from that.
        // TODO: Make `Endpoint` provide a more straightforward API for that.
        let addr = {
            n0_future::future::race(
                endpoint.direct_addresses().initialized().map(|_| ()),
                endpoint.home_relay().initialized().map(|_| ()),
            )
            .await;
            let addrs = endpoint
                .direct_addresses()
                .get()
                .expect("endpoint alive")
                .unwrap_or_default()
                .into_iter()
                .map(|x| x.addr);
            let home_relay = endpoint.home_relay().get().expect("endpoint alive");
            NodeAddr::from_parts(endpoint.node_id(), home_relay, addrs)
        };

        let me = endpoint.node_id().fmt_short();
        let max_message_size = self.config.proto.max_message_size;
        let (actor, rpc_tx, local_tx) = Actor::new(
            endpoint,
            Arc::new(self.config),
            metrics.clone(),
            &addr.into(),
        );

        let actor_handle = task::spawn(
            async move {
                if let Err(err) = actor.run().await {
                    warn!("gossip actor closed with error: {err:?}");
                }
            }
            .instrument(error_span!("gossip", %me)),
        );

        let api = GossipApi::local(rpc_tx);

        Ok(Gossip {
            inner: Inner {
                api,
                local_tx,
                _actor_handle: AbortOnDropHandle::new(actor_handle),
                max_message_size,
                metrics,
            }
            .into(),
        })
    }
}

impl Gossip {
    /// Creates a default `Builder`, with the endpoint set.
    pub fn builder() -> Builder {
        Builder {
            config: Default::default(),
        }
    }

    /// Listen on a quinn endpoint for incoming RPC connections.
    #[cfg(feature = "rpc")]
    pub async fn listen(self, endpoint: quinn::Endpoint) {
        self.inner.api.listen(endpoint).await
    }

    /// Get the maximum message size configured for this gossip actor.
    pub fn max_message_size(&self) -> usize {
        self.inner.max_message_size
    }

    /// Handle an incoming [`Connection`].
    ///
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
    /// stops the gossip actor loop and drops all state and connections.
    pub async fn shutdown(&self) -> anyhow::Result<()> {
        let (reply, reply_rx) = oneshot::channel();
        self.inner
            .local_tx
            .send(LocalActorMessage::Shutdown { reply })
            .await?;
        reply_rx.await?;
        Ok(())
    }

    /// Returns the metrics tracked for this gossip instance.
    pub fn metrics(&self) -> &Arc<Metrics> {
        &self.inner.metrics
    }
}

/// Actor that sends and handles messages between the connection and main state loops
struct Actor {
    endpoint: Endpoint,
    rpc_rx: mpsc::Receiver<RpcMessage>,
    local_rx: mpsc::Receiver<LocalActorMessage>,
    connection_pool: ConnectionPool,
    topic_handles: HashMap<TopicId, TopicHandle>,
    topic_tasks: tokio::task::JoinSet<self::topic::Topic>,
    metrics: Arc<Metrics>,
    config: Arc<Config>,
    our_peer_data: Watchable<PeerData>,
}

impl Actor {
    fn new(
        endpoint: Endpoint,
        config: Arc<Config>,
        metrics: Arc<Metrics>,
        my_addr: &AddrInfo,
    ) -> (
        Self,
        mpsc::Sender<RpcMessage>,
        mpsc::Sender<LocalActorMessage>,
    ) {
        let (rpc_tx, rpc_rx) = mpsc::channel(TO_ACTOR_CAP);
        let (local_tx, local_rx) = mpsc::channel(16);
        // let (from_topic_tx, from_topic_rx) = mpsc::channel(16);

        let connection_pool = ConnectionPool::new(endpoint.clone());
        let our_peer_data = Watchable::new(my_addr.encode());

        let actor = Actor {
            endpoint,
            rpc_rx,
            local_rx,
            config,
            topic_handles: Default::default(),
            connection_pool,
            metrics,
            topic_tasks: JoinSet::default(),
            // from_topic_tx,
            // from_topic_rx,
            our_peer_data,
        };

        (actor, rpc_tx, local_tx)
    }

    pub async fn run(mut self) -> Result<(), Error> {
        let (mut current_addresses, mut home_relay_stream, mut direct_addresses_stream) =
            self.setup().await?;

        let mut i = 0;
        while let Some(()) = self
            .event_loop(
                &mut current_addresses,
                &mut home_relay_stream,
                &mut direct_addresses_stream,
                i,
            )
            .await?
        {
            i += 1;
        }
        Ok(())
    }

    /// Performs the initial actor setup to run the [`Actor::event_loop`].
    ///
    /// This updates our current address and return it. It also returns the home relay stream and
    /// direct addr stream.
    async fn setup(
        &mut self,
    ) -> Result<
        (
            BTreeSet<DirectAddr>,
            impl Stream<Item = iroh::RelayUrl> + Unpin,
            impl Stream<Item = BTreeSet<DirectAddr>> + Unpin,
        ),
        Error,
    > {
        // Watch for changes in direct addresses to update our peer data.
        let direct_addresses_stream = self.endpoint.direct_addresses().stream().filter_map(|i| i);
        // Watch for changes of our home relay to update our peer data.
        let home_relay_stream = self.endpoint.home_relay().stream().filter_map(|i| i);
        // With each gossip message we provide addressing information to reach our node.
        let current_addresses = self.endpoint.direct_addresses().get()?.unwrap_or_default();

        // self.handle_addr_update(&current_addresses).await?;
        Ok((
            current_addresses,
            home_relay_stream,
            direct_addresses_stream,
        ))
    }

    /// One event loop processing step.
    ///
    /// None is returned when no further processing should be performed.
    async fn event_loop(
        &mut self,
        current_addresses: &mut BTreeSet<DirectAddr>,
        home_relay_stream: &mut (impl Stream<Item = iroh::RelayUrl> + Unpin),
        direct_addresses_stream: &mut (impl Stream<Item = BTreeSet<DirectAddr>> + Unpin),
        i: usize,
    ) -> Result<Option<()>, Error> {
        trace!("tick {i}: wait");
        self.metrics.actor_tick_main.inc();
        tokio::select! {
            biased;
            conn = self.local_rx.recv() => {
                match conn {
                    Some(LocalActorMessage::Shutdown { reply }) => {
                        debug!("received shutdown message, quit actor");
                        reply.send(()).ok();
                        return Ok(None)
                    },
                    Some(LocalActorMessage::HandleConnection(conn)) => {
                        self.connection_pool.handle_connection(conn);
                    }
                    None => {
                        debug!("all gossip handles dropped, stop gossip actor");
                        return Ok(None)
                    }
                }
            }
            msg = self.rpc_rx.recv() => {
                trace!(?i, "tick: to_actor_rx");
                self.metrics.actor_tick_rx.inc();
                match msg {
                    Some(msg) => self.handle_rpc_msg(msg).await,
                    None => {
                        debug!("all gossip handles dropped, stop gossip actor");
                        return Ok(None)
                    }
                }
            },
            Some(res) = self.topic_tasks.join_next(), if !self.topic_tasks.is_empty() => {
                let state = res.expect("topic task panicked");
                self.topic_closed(state).await;
            }
            Some(new_addresses) = direct_addresses_stream.next() => {
                trace!(?i, "tick: new_endpoints");
                self.metrics.actor_tick_endpoint.inc();
                *current_addresses = new_addresses;
                self.handle_addr_update(current_addresses);
            }
            Some(_relay_url) = home_relay_stream.next() => {
                trace!(?i, "tick: new_home_relay");
                self.handle_addr_update(current_addresses);
            }
        }

        Ok(Some(()))
    }

    fn handle_addr_update(&mut self, current_addresses: &BTreeSet<DirectAddr>) {
        let peer_data = AddrInfo::from_endpoint(&self.endpoint, current_addresses).encode();
        let _ = self.our_peer_data.set(peer_data);
    }

    async fn handle_rpc_msg(&mut self, msg: RpcMessage) {
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
                let api::JoinRequest { topic_id } = inner;
                let channels = (tx, rx);
                self.subscribe(topic_id, channels).await;
            }
        }
    }

    async fn subscribe(&mut self, topic_id: TopicId, channels: SubscribeChannels) {
        let me = self.endpoint.node_id();
        let topic_handle = self.topic_handles.entry(topic_id).or_insert_with(|| {
            let endpoint = self.endpoint.clone();
            let peer_data_fn = Arc::new(move |node_id, peer_data: PeerData| {
                if let Some(node_addr) = AddrInfo::decode_to_node_addr(&peer_data, node_id) {
                    endpoint.add_node_addr(node_addr).ok();
                }
            });
            TopicHandle::spawn_into(
                &mut self.topic_tasks,
                me,
                topic_id,
                self.config.clone(),
                peer_data_fn,
                self.our_peer_data.watch(),
                self.connection_pool.clone(),
            )
        });
        topic_handle.send(ToTopic::Subscribe(channels)).await;
    }

    async fn topic_closed(&mut self, state: self::topic::Topic) {
        let topic_id = state.id();
        tracing::info!(topic=%topic_id.fmt_short(), "topic closed");
        if let Some(handle) = self.topic_handles.remove(&topic_id) {
            if let Some(new_handle) = handle
                .maybe_respawn_into(&mut self.topic_tasks, state)
                .await
            {
                tracing::info!(topic=%topic_id.fmt_short(), "topic respawned");
                self.topic_handles.insert(topic_id, new_handle);
            } else {
                tracing::info!(topic=%topic_id.fmt_short(), "topic closed and dropped");
            }
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
struct AddrInfo {
    relay_url: Option<RelayUrl>,
    direct_addresses: BTreeSet<SocketAddr>,
}

impl From<NodeAddr> for AddrInfo {
    fn from(
        NodeAddr {
            relay_url,
            direct_addresses,
            ..
        }: NodeAddr,
    ) -> Self {
        Self {
            relay_url,
            direct_addresses,
        }
    }
}

impl AddrInfo {
    fn encode(&self) -> PeerData {
        let bytes = postcard::to_stdvec(self).expect("serialization may not fail");
        PeerData::new(bytes)
    }

    fn decode(peer_data: &PeerData) -> Result<Self, Error> {
        let bytes = peer_data.as_bytes();
        if bytes.is_empty() {
            return Ok(AddrInfo::default());
        }
        let info = postcard::from_bytes(bytes)?;
        Ok(info)
    }

    fn to_node_addr(self, node_id: NodeId) -> NodeAddr {
        NodeAddr::from_parts(node_id, self.relay_url, self.direct_addresses)
    }

    fn decode_to_node_addr(peer_data: &PeerData, node_id: NodeId) -> Option<NodeAddr> {
        AddrInfo::decode(peer_data)
            .map(|addr_info| addr_info.to_node_addr(node_id))
            .ok()
    }

    fn from_endpoint(endpoint: &Endpoint, direct_addresses: &BTreeSet<DirectAddr>) -> Self {
        AddrInfo {
            relay_url: endpoint.home_relay().get().ok().flatten(),
            direct_addresses: direct_addresses.iter().map(|x| x.addr).collect(),
        }
    }
}

#[cfg(test)]
pub(crate) mod test {
    use std::time::Duration;

    use bytes::Bytes;
    use futures_concurrency::future::TryJoin;
    use iroh::{protocol::Router, RelayMap, RelayMode, SecretKey};
    use rand::{Rng, SeedableRng};
    use tokio::{spawn, time::timeout};
    use tokio_util::sync::CancellationToken;
    use tracing::{info, instrument};
    use tracing_test::traced_test;

    use crate::{api::Event, proto};

    use super::*;

    struct ManualActorLoop {
        actor: Actor,
        current_addresses: BTreeSet<DirectAddr>,
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

    type EndpointHandle = tokio::task::JoinHandle<Result<(), Error>>;

    impl ManualActorLoop {
        #[instrument(skip_all, fields(me = %actor.endpoint.node_id().fmt_short()))]
        async fn new(mut actor: Actor) -> Result<Self, Error> {
            let (current_addresses, _, _) = actor.setup().await?;
            let test_rig = Self {
                actor,
                current_addresses,
                step: 0,
            };

            Ok(test_rig)
        }

        #[instrument(skip_all, fields(me = %self.endpoint.node_id().fmt_short()))]
        async fn step(&mut self) -> Result<Option<()>, Error> {
            let ManualActorLoop {
                actor,
                current_addresses,
                step,
            } = self;
            *step += 1;
            // ignore updates that change our published address. This gives us better control over
            // events since the endpoint it no longer emitting changes
            let home_relay_stream = &mut futures_lite::stream::pending();
            let direct_addresses_stream = &mut futures_lite::stream::pending();
            actor
                .event_loop(
                    current_addresses,
                    home_relay_stream,
                    direct_addresses_stream,
                    *step,
                )
                .await
        }

        async fn steps(&mut self, n: usize) -> Result<(), Error> {
            for _ in 0..n {
                self.step().await?;
            }
            Ok(())
        }

        async fn finish(mut self) -> Result<(), Error> {
            while self.step().await?.is_some() {}
            Ok(())
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
            rng: &mut rand_chacha::ChaCha12Rng,
            config: proto::Config,
            relay_map: RelayMap,
            cancel: &CancellationToken,
        ) -> Result<(Self, Actor, EndpointHandle), Error> {
            let my_addr = AddrInfo {
                relay_url: relay_map.nodes().next().map(|relay| relay.url.clone()),
                direct_addresses: Default::default(),
            };
            let endpoint = create_endpoint(rng, relay_map).await?;
            let metrics = Arc::new(Metrics::default());

            let max_message_size = config.max_message_size;
            let mut tconfig = Config::default();
            tconfig.proto = config;
            let (actor, to_actor_tx, conn_tx) =
                Actor::new(endpoint, Arc::new(tconfig), metrics.clone(), &my_addr);

            let _actor_handle =
                AbortOnDropHandle::new(task::spawn(futures_lite::future::pending()));
            let gossip = Self {
                inner: Inner {
                    api: GossipApi::local(to_actor_tx),
                    local_tx: conn_tx,
                    _actor_handle,
                    max_message_size,
                    metrics,
                }
                .into(),
            };

            let endpoing_task = task::spawn(endpoint_loop(
                actor.endpoint.clone(),
                gossip.clone(),
                cancel.child_token(),
            ));

            Ok((gossip, actor, endpoing_task))
        }

        /// Crates a new testing gossip instance with the normal actor loop.
        async fn t_new(
            rng: &mut rand_chacha::ChaCha12Rng,
            config: proto::Config,
            relay_map: RelayMap,
            cancel: &CancellationToken,
        ) -> Result<(Self, Endpoint, EndpointHandle, impl Drop), Error> {
            let (g, actor, ep_handle) =
                Gossip::t_new_with_actor(rng, config, relay_map, cancel).await?;
            let ep = actor.endpoint.clone();
            let me = ep.node_id().fmt_short();
            let actor_handle = task::spawn(
                async move {
                    if let Err(err) = actor.run().await {
                        warn!("gossip actor closed with error: {err:?}");
                    }
                }
                .instrument(tracing::error_span!("gossip", %me)),
            );
            Ok((g, ep, ep_handle, AbortOnDropHandle::new(actor_handle)))
        }
    }

    pub(crate) async fn create_endpoint(
        rng: &mut rand_chacha::ChaCha12Rng,
        relay_map: RelayMap,
    ) -> Result<Endpoint, Error> {
        let ep = Endpoint::builder()
            .secret_key(SecretKey::generate(rng))
            .alpns(vec![GOSSIP_ALPN.to_vec()])
            .relay_mode(RelayMode::Custom(relay_map))
            .insecure_skip_relay_cert_verify(true)
            .bind()
            .await?;

        ep.home_relay().initialized().await?;
        Ok(ep)
    }

    async fn endpoint_loop(
        endpoint: Endpoint,
        gossip: Gossip,
        cancel: CancellationToken,
    ) -> Result<(), Error> {
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
                        gossip.handle_connection(connecting.await?).await?
                    }
                }
            }
        }
        endpoint.close().await;
        Ok(())
    }

    #[tokio::test]
    // #[traced_test]
    async fn gossip_net_smoke() {
        tracing_subscriber::fmt::try_init().ok();
        let mut rng = rand_chacha::ChaCha12Rng::seed_from_u64(1);
        let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();

        let ep1 = create_endpoint(&mut rng, relay_map.clone()).await.unwrap();
        let ep2 = create_endpoint(&mut rng, relay_map.clone()).await.unwrap();
        let ep3 = create_endpoint(&mut rng, relay_map.clone()).await.unwrap();

        let go1 = Gossip::builder().spawn(ep1.clone()).await.unwrap();
        let go2 = Gossip::builder().spawn(ep2.clone()).await.unwrap();
        let go3 = Gossip::builder().spawn(ep3.clone()).await.unwrap();
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

        let (mut sink1, _stream1) = sub1.split();

        let len = 2;

        // publish messages on node1
        let pub1 = spawn(async move {
            for i in 0..len {
                let message = format!("hi{}", i);
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
    // #[traced_test]
    async fn subscription_cleanup() -> testresult::TestResult {
        tracing_subscriber::fmt::try_init().ok();
        let rng = &mut rand_chacha::ChaCha12Rng::seed_from_u64(1);
        let ct = CancellationToken::new();
        let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();

        // create the first node with a manual actor loop
        let (go1, actor, ep1_handle) =
            Gossip::t_new_with_actor(rng, Default::default(), relay_map.clone(), &ct).await?;
        let mut actor = ManualActorLoop::new(actor).await?;

        // create the second node with the usual actor loop
        let (go2, ep2, ep2_handle, _test_actor_handle) =
            Gossip::t_new(rng, Default::default(), relay_map, &ct).await?;

        let node_id1 = actor.endpoint.node_id();
        let node_id2 = ep2.node_id();
        tracing::info!(
            node_1 = node_id1.fmt_short(),
            node_2 = node_id2.fmt_short(),
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
                anyhow::Ok(())
            };

            tokio::select! {
                _ = ct2.cancelled() => Ok(()),
                res = subscribe_fut => res,
            }
        }
        .instrument(tracing::debug_span!("node_2", %node_id2));
        let go2_handle = task::spawn(go2_task);

        // first node
        let addr2 = NodeAddr::new(node_id2).with_relay_url(relay_url);
        actor.endpoint.add_node_addr(addr2)?;
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
            drop(go1);

            anyhow::Ok(())
        }
        .instrument(tracing::debug_span!("node_1", %node_id1));
        let go1_handle = task::spawn(go1_task);

        // advance and check that the topic is now subscribed
        tracing::info!("now wait subscribe 1");
        actor.steps(1).await?; // handle our subscribe;
        tracing::info!("subscribe1 done, now sleep and wait join");

        // TODO(Frando): Remove timeout. Added because the topics don't have manual actor loops yet.
        tokio::time::sleep(Duration::from_millis(2000)).await;
        tracing::info!("sleep done, should be joined");
        let state = actor
            .topic_handles
            .get(&topic)
            .expect("get registered topic");
        assert!(state.joined());

        // signal the second subscribe, we should remain subscribed
        tx.send(()).await?;
        tracing::info!("now wait subscribe 2");
        actor.steps(1).await?; // subscribe
        tracing::info!("subscribe2 done, now sleep and wait join");

        // TODO(Frando): Remove timeout. Added because the topics don't have manual actor loops yet.
        tokio::time::sleep(Duration::from_millis(200)).await;
        tracing::info!("sleep done, should be joined");
        let state = actor
            .topic_handles
            .get(&topic)
            .expect("get registered topic");
        assert!(state.joined());

        // signal to drop the second handle, the topic should no longer be subscribed
        tx.send(()).await?;
        actor.steps(1).await?; // topic closed

        assert!(!actor.topic_handles.contains_key(&topic));

        // cleanup and ensure everything went as expected
        ct.cancel();
        let wait = Duration::from_secs(2);
        timeout(wait, ep1_handle).await???;
        timeout(wait, ep2_handle).await???;
        timeout(wait, go1_handle).await???;
        timeout(wait, go2_handle).await???;
        timeout(wait, actor.finish()).await??;

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

            anyhow::Ok(())
        }
        .instrument(tracing::debug_span!("node_2", %node_id2));
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
        let conn_timeout = Duration::from_millis(500);
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
        timeout(wait, ep1_handle).await???;
        timeout(wait, ep2_handle).await???;

        testresult::TestResult::Ok(())
    }

    #[tokio::test]
    // #[traced_test]
    async fn can_die_and_reconnect() -> testresult::TestResult {
        tracing_subscriber::fmt::try_init().ok();
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
        ) -> anyhow::Result<(Router, Gossip)> {
            let ep = Endpoint::builder()
                .secret_key(secret_key)
                .relay_mode(RelayMode::Custom(relay_map))
                .insecure_skip_relay_cert_verify(true)
                .bind()
                .await?;
            let gossip = Gossip::builder().spawn(ep.clone()).await?;
            let router = Router::builder(ep.clone())
                .accept(GOSSIP_ALPN, gossip.clone())
                .spawn();
            Ok((router, gossip))
        }

        /// Spawns a gossip node, and broadcasts a single message, then sleep until cancelled externally.
        async fn broadcast_once(
            secret_key: SecretKey,
            relay_map: RelayMap,
            bootstrap_addr: NodeAddr,
            topic_id: TopicId,
            message: String,
        ) -> anyhow::Result<()> {
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
        let topic_id = TopicId::from_bytes(rng.gen());

        // spawn a gossip node, send the node's address on addr_tx,
        // then wait to receive `count` messages, and terminate.
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();
        let (msgs_recv_tx, mut msgs_recv_rx) = tokio::sync::mpsc::channel(3);
        let recv_task = tokio::task::spawn({
            let relay_map = relay_map.clone();
            let secret_key = SecretKey::generate(&mut rng);
            async move {
                let (router, gossip) = spawn_gossip(secret_key, relay_map).await?;
                let addr = router.endpoint().node_addr().await?;
                info!(node_id = %addr.node_id.fmt_short(), "recv node spawned");
                addr_tx.send(addr).unwrap();
                let mut topic = gossip.subscribe_and_join(topic_id, vec![]).await?;
                while let Some(event) = topic.try_next().await.unwrap() {
                    if let Event::Received(message) = event {
                        let message = std::str::from_utf8(&message.content)?.to_string();
                        msgs_recv_tx.send(message).await?;
                    }
                }
                anyhow::Ok(())
            }
        });

        let node0_addr = addr_rx.await?;
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
        let msg = timeout(max_wait, msgs_recv_rx.recv()).await?.unwrap();
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
        let msg = timeout(max_wait, msgs_recv_rx.recv()).await?.unwrap();
        assert_eq!(&msg, "msg2");
        info!("kill broadcast node");
        cancel.cancel();

        info!("kill recv node");
        recv_task.abort();
        assert!(join_handle_1.join().unwrap().is_none());
        assert!(join_handle_2.join().unwrap().is_none());

        Ok(())
    }
}
