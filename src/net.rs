//! Networking for the `iroh-gossip` protocol

#[cfg(test)]
use std::sync::atomic::AtomicBool;
use std::{
    collections::{hash_map, BTreeSet, HashMap, HashSet, VecDeque},
    ops::{ControlFlow, DerefMut},
    sync::{Arc, Mutex},
    task::{ready, Poll},
    time::Duration,
};

use bytes::Bytes;
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
    Endpoint, EndpointAddr, EndpointId,
};
use irpc::{
    channel::{self, mpsc::RecvError},
    WithChannels,
};
use n0_error::{anyerr, stack_error};
use n0_future::{
    boxed::BoxFuture,
    stream::Boxed as BoxStream,
    task::{self, AbortOnDropHandle},
    time::Instant,
    FuturesUnordered, MergeUnbounded, Stream, StreamExt,
};
use n0_watcher::{Direct, Watchable, Watcher};
use rand::rngs::StdRng;
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinSet,
};
use tracing::{debug, error_span, instrument, trace, warn, Instrument};

use self::{
    address_lookup::GossipAddressLookup,
    connection_pool::{ConnectionPool, ConnectionRef},
    util::{AddrInfo, Timers},
};
use crate::{
    api::{self, GossipApi},
    metrics::Metrics,
    net::net_proto::{GossipReceiver, GossipSender},
    proto::{self, Config, HyparviewConfig, PeerData, PlumtreeConfig, TopicId},
};

mod address_lookup;
mod connection_pool;
mod net_proto;
mod util;

/// ALPN protocol name
pub const GOSSIP_ALPN: &[u8] = b"/iroh-gossip/1";

type InEvent = proto::topic::InEvent<EndpointId>;
type OutEvent = proto::topic::OutEvent<EndpointId>;
type Timer = proto::topic::Timer<EndpointId>;
pub(super) type ProtoMessage = proto::topic::Message<EndpointId>;
type ProtoEvent = proto::topic::Event<EndpointId>;
type State = proto::topic::State<EndpointId, StdRng>;
type Command = proto::topic::Command<EndpointId>;

/// Publish and subscribe on gossiping topics.
///
/// Each topic is a separate broadcast tree with separate memberships.
/// A topic has to be joined before you can publish or subscribe on the topic.
/// To join the swarm for a topic, you have to know the [`EndpointId`] of at least one peer that also joined the topic.
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

#[derive(Debug)]
struct Inner {
    api: GossipApi,
    pool: ConnectionPool,
    _actor_handle: AbortOnDropHandle<()>,
    max_message_size: usize,
    metrics: Arc<Metrics>,
}

impl ProtocolHandler for Gossip {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        self.handle_connection(connection)
            .await
            .map_err(|err| AcceptError::from_err(anyerr!(err)))?;
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
    pub async fn handle_connection(&self, connection: Connection) -> Result<(), ActorStoppedError> {
        self.0
            .pool
            .handle_connection(connection)
            .await
            .map_err(|_| ActorStoppedError::new())?;
        Ok(())
    }

    /// Returns the metrics tracked for this gossip instance.
    pub fn metrics(&self) -> &Arc<Metrics> {
        &self.0.metrics
    }

    #[tracing::instrument("gossip", parent=None, skip_all, fields(me=%endpoint.id().fmt_short()))]
    fn new(endpoint: Endpoint, config: Config, alpn: Option<Bytes>) -> Self {
        let metrics = Arc::new(Metrics::default());
        let max_message_size = config.max_message_size;
        let (api_tx, pool, actor) = Actor::new(endpoint, config, alpn, metrics.clone());
        let actor_task = task::spawn(actor.run().instrument(tracing::Span::current()));

        Self(Arc::new(Inner {
            max_message_size,
            api: GossipApi::local(api_tx),
            pool,
            metrics,
            _actor_handle: AbortOnDropHandle::new(actor_task),
        }))
    }

    #[cfg(test)]
    fn new_with_actor(endpoint: Endpoint, config: Config, alpn: Option<Bytes>) -> (Self, Actor) {
        let metrics = Arc::new(Metrics::default());
        let max_message_size = config.max_message_size;
        let (api_tx, pool, actor) = Actor::new(endpoint, config, alpn, metrics.clone());
        let handle = Self(Arc::new(Inner {
            pool,
            max_message_size,
            api: GossipApi::local(api_tx),
            metrics,
            _actor_handle: AbortOnDropHandle::new(task::spawn(std::future::pending())),
        }));
        (handle, actor)
    }
}

/// Error emitted when the gossip actor stopped.
#[stack_error(derive)]
pub struct ActorStoppedError;

#[derive(strum::Display)]
enum TopicMessage {
    ApiJoin(ApiJoinRequest),
    RemoteConnected {
        remote: EndpointId,
        stream: GossipReceiver,
    },
}

type ApiJoinRequest = WithChannels<api::JoinRequest, api::Request>;
type ApiRecvStream = BoxStream<Result<api::Command, RecvError>>;
type RemoteRecvStream = BoxStream<(EndpointId, n0_error::Result<Option<ProtoMessage>>)>;

#[derive(Debug, Default, Clone)]
struct TopicMap(Arc<Mutex<TopicMapInner>>);

#[derive(Debug, Default)]
struct TopicMapInner {
    topics: HashMap<TopicId, TopicHandle>,
    tasks: JoinSet<TopicActor>,
}

impl TopicMap {
    fn get_or_init(&self, topic_id: TopicId, shared: &Arc<Shared>) -> TopicHandle {
        let mut inner = self.0.lock().expect("poisoned");
        match inner.topics.entry(topic_id) {
            hash_map::Entry::Occupied(entry) => entry.get().clone(),
            hash_map::Entry::Vacant(entry) => {
                let (handle, actor) = TopicHandle::new(topic_id, shared.clone());
                let topic = entry.insert(handle).clone();
                inner.tasks.spawn(
                    actor
                        .run()
                        .instrument(error_span!("topic", topic=%topic_id.fmt_short())),
                );
                topic
            }
        }
    }

    fn get(&self, topic_id: &TopicId) -> Option<TopicHandle> {
        let inner = self.0.lock().expect("poisoned");
        inner.topics.get(topic_id).cloned()
    }

    async fn join_next(&self) -> TopicId {
        std::future::poll_fn(|cx| {
            let mut inner = self.0.lock().expect("poisoned");
            if inner.tasks.is_empty() {
                Poll::Pending
            } else {
                let res = ready!(inner.tasks.poll_join_next(cx)).expect("task map is not empty");
                let actor = res.expect("topic actortask panicked");
                trace!(topic=%actor.topic_id.fmt_short(), "tick: topic actor finished");
                inner.topics.remove(&actor.topic_id);
                Poll::Ready(actor.topic_id)
            }
        })
        .await
    }
}

struct Shared {
    me: EndpointId,
    config: Config,
    our_peer_data: n0_watcher::Watchable<PeerData>,
    metrics: Arc<Metrics>,
    address_lookup: GossipAddressLookup,
    pool: ConnectionPool,
}

struct Actor {
    #[cfg(test)]
    endpoint: Endpoint,
    shared: Arc<Shared>,
    topics: TopicMap,
    api_rx: mpsc::Receiver<api::RpcMessage>,
    endpoint_addr_updates: BoxStream<EndpointAddr>,
}

impl Actor {
    fn new(
        endpoint: Endpoint,
        config: Config,
        alpn: Option<Bytes>,
        metrics: Arc<Metrics>,
    ) -> (mpsc::Sender<api::RpcMessage>, ConnectionPool, Self) {
        let (api_tx, api_rx) = tokio::sync::mpsc::channel(16);

        let me = endpoint.id();

        let endpoint_addr_updates = endpoint.watch_addr().stream();
        let address_lookup = GossipAddressLookup::default();
        endpoint.address_lookup().add(address_lookup.clone());
        let initial_peer_data = AddrInfo::from(endpoint.addr()).encode();

        let alpn = alpn.unwrap_or_else(|| crate::ALPN.to_vec().into());

        let topics = TopicMap::default();
        let mut options = connection_pool::Options::default().with_on_connected({
            let topics = topics.clone();
            move |_ep, conn| {
                let topics = topics.clone();
                Box::pin(async move {
                    task::spawn(accept_loop(topics, conn));
                    Ok(())
                })
            }
        });
        options.connect_timeout = Duration::from_secs(10);
        options.idle_timeout = Duration::from_secs(10);
        let pool = ConnectionPool::new(endpoint.clone(), &alpn, options);

        let shared = Arc::new(Shared {
            me,
            config,
            our_peer_data: Watchable::new(initial_peer_data),
            metrics: metrics.clone(),
            address_lookup,
            pool: pool.clone(),
        });

        (
            api_tx,
            pool,
            Actor {
                #[cfg(test)]
                endpoint,
                shared,
                api_rx,
                endpoint_addr_updates: Box::pin(endpoint_addr_updates),
                topics,
            },
        )
    }

    async fn run(mut self) {
        loop {
            match self.tick().await {
                ControlFlow::Continue(()) => {}
                ControlFlow::Break(()) => break,
            }
        }
    }

    #[cfg(test)]
    #[instrument("gossip", skip_all, fields(me=%self.shared.me.fmt_short()))]
    pub(crate) async fn finish(self) {
        self.run().await
    }

    #[cfg(test)]
    #[instrument("gossip", skip_all, fields(me=%self.shared.me.fmt_short()))]
    pub(crate) async fn steps(&mut self, n: usize) -> Result<(), ActorStoppedError> {
        for _ in 0..n {
            if self.tick().await == ControlFlow::Break(()) {
                return Err(ActorStoppedError);
            }
        }
        Ok(())
    }

    async fn tick(&mut self) -> ControlFlow<(), ()> {
        self.shared.metrics.actor_tick_main.inc();
        tokio::select! {
            addr = self.endpoint_addr_updates.next() => {
                trace!("tick: endpoint_addr_update");
                match addr {
                    None => {
                        warn!("address stream returned None - endpoint has shut down");
                        ControlFlow::Break(())
                    }
                    Some(addr) => {
                        let data = AddrInfo::from(addr).encode();
                        self.shared.our_peer_data.set(data).ok();
                        ControlFlow::Continue(())
                    }
                }
            }
            msg = self.api_rx.recv() => {
                trace!(some=msg.is_some(), "tick: api_rx");
                match msg {
                    Some(msg) => {
                        self.handle_api_message(msg).await;
                        ControlFlow::Continue(())
                    }
                    None => {
                        trace!("all api senders dropped, stop actor");
                        ControlFlow::Break(())
                    }
                }
            }
            topic_id = self.topics.join_next() => {
                trace!(%topic_id, "topic actor stopped");
                ControlFlow::Continue(())
            }
            else => unreachable!("reached else arm, but all fallible cases should be handled"),
        }
    }

    #[cfg(test)]
    fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    async fn handle_api_message(&mut self, msg: api::RpcMessage) {
        let (topic_id, msg) = match msg {
            api::RpcMessage::Join(msg) => (msg.inner.topic_id, msg),
        };
        let topic = self.topics.get_or_init(topic_id, &self.shared);
        if topic.send(TopicMessage::ApiJoin(msg)).await.is_err() {
            warn!(topic=%topic_id.fmt_short(), "Topic actor dead");
        }
    }
}

async fn accept_loop(topics: TopicMap, conn: Connection) {
    loop {
        let stream = match GossipReceiver::accept(&conn).await {
            Ok(Some(stream)) => stream,
            _ => break,
        };

        let Some(topic) = topics.get(&stream.topic_id()) else {
            continue;
        };
        let msg = TopicMessage::RemoteConnected {
            remote: conn.remote_id(),
            stream,
        };
        topic.send(msg).await.ok();
    }
}

#[derive(Debug, Clone)]
struct TopicHandle {
    tx: mpsc::Sender<TopicMessage>,
    #[cfg(test)]
    joined: Arc<AtomicBool>,
}

impl TopicHandle {
    fn new(topic_id: TopicId, shared: Arc<Shared>) -> (Self, TopicActor) {
        let (tx, rx) = mpsc::channel(16);
        // TODO: peer_data
        let state = State::new(shared.me, None, shared.config.clone());
        #[cfg(test)]
        let joined = Arc::new(AtomicBool::new(false));
        let peer_data = shared.our_peer_data.watch();
        let (forward_event_tx, _) = broadcast::channel(512);
        let actor = TopicActor {
            topic_id,
            shared,
            state,
            rx,
            peer_data,
            api_send_tx: forward_event_tx,
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
            api_send_tasks: Default::default(),
            connecting: Default::default(),
        };
        let handle = Self {
            tx,
            #[cfg(test)]
            joined,
        };
        (handle, actor)
    }

    async fn send(&self, msg: TopicMessage) -> Result<(), mpsc::error::SendError<TopicMessage>> {
        self.tx.send(msg).await
    }

    #[cfg(test)]
    fn joined(&self) -> bool {
        self.joined.load(std::sync::atomic::Ordering::Relaxed)
    }
}

struct TopicActor {
    topic_id: TopicId,
    shared: Arc<Shared>,

    // -- state
    state: State,
    timers: Timers<Timer>,
    neighbors: BTreeSet<EndpointId>,
    out_events: VecDeque<OutEvent>,
    init: bool,
    drop_peers_queue: HashSet<EndpointId>,
    #[cfg(test)]
    joined: Arc<AtomicBool>,

    // -- senders and receivers
    peer_data: Direct<PeerData>,
    rx: mpsc::Receiver<TopicMessage>,
    remote_senders: HashMap<EndpointId, MaybeSender>,
    remote_receivers: MergeUnbounded<RemoteRecvStream>,
    api_receivers: MergeUnbounded<ApiRecvStream>,
    api_send_tx: broadcast::Sender<ProtoEvent>,
    api_send_tasks: JoinSet<()>,
    connecting: FuturesUnordered<BoxFuture<(EndpointId, n0_error::Result<Guarded<GossipSender>>)>>,
}

impl TopicActor {
    pub async fn run(mut self) -> Self {
        self.shared.metrics.topics_joined.inc();
        let peer_data = self.peer_data.clone().stream();
        tokio::pin!(peer_data);
        loop {
            trace!("wait for tick");
            tokio::select! {
                Some(msg) = self.rx.recv() => {
                    trace!("tick: actor_rx {msg}");
                    self.handle_actor_message(msg).await;
                },
                Some(conn) = self.connecting.next(), if !self.connecting.is_empty() => {
                    self.handle_connected(conn).await;
                }
                Some(message) = self.api_receivers.next(), if !self.api_receivers.is_empty() => {
                    let message = match message {
                        Ok(message) => message,
                        Err(err) => {
                            trace!("tick: api receiver closed {err:#}");
                            continue;
                        }
                    };
                    trace!("tick: api message {message}");
                    self.handle_in_event(InEvent::Command(message.into())).await;
                }
                Some((remote, message)) = self.remote_receivers.next(), if !self.remote_receivers.is_empty() => {
                    trace!(remote=%remote.fmt_short(), msg=?message, "tick: remote_rx");
                    self.handle_remote_message(remote, message).await;
                }
                Some(data) = peer_data.next() => {
                    trace!("tick: peer_data");
                    self.handle_in_event(InEvent::UpdatePeerData(data)).await;
                }
                _ = self.timers.wait_next() => {
                    trace!("tick: timers");
                    let now = Instant::now();
                    while let Some((_instant, timer)) = self.timers.pop_before(now) {
                        self.handle_in_event(InEvent::TimerExpired(timer)).await;
                    }
                }
                _ = self.api_send_tasks.join_next(), if !self.api_send_tasks.is_empty() => {
                    trace!("tick: api sender finished");
                }
                else => break,
            }

            if !self.drop_peers_queue.is_empty() {
                trace!(len = self.drop_peers_queue.len(), "process peer drop queue");
                let now = Instant::now();
                for peer in self.drop_peers_queue.drain() {
                    self.out_events
                        .extend(self.state.handle(InEvent::PeerDisconnected(peer), now));
                }
                self.process_out_events(now).await;
            }

            if self.init && self.api_receivers.is_empty() && self.api_send_tasks.is_empty() {
                debug!("Closing topic: All API subscribers dropped");
                self.handle_in_event(InEvent::Command(Command::Quit)).await;
                debug!("Topic closed");
                break;
            }
        }
        self.shared.metrics.topics_quit.inc();
        self
    }

    async fn handle_connected(
        &mut self,
        (remote, tx): (EndpointId, n0_error::Result<Guarded<GossipSender>>),
    ) {
        match tx {
            Ok(tx) => {
                let sender = self.remote_senders.entry(remote).or_default();
                if let Err(err) = sender.init(tx).await {
                    warn!("Remote failed while pushing queued messages: {err:?}");
                }
            }
            Err(_err) => {
                self.handle_in_event(InEvent::PeerDisconnected(remote))
                    .await
            }
        }
    }

    async fn handle_actor_message(&mut self, msg: TopicMessage) {
        match msg {
            TopicMessage::RemoteConnected { remote, stream } => {
                // Replace our sender if this a new connection.
                if let Some(old) = self.remote_senders.get_mut(&remote) {
                    if !old.is_same_conn(&stream) {
                        if let Ok(conn) = self.shared.pool.get_or_connect(remote).await {
                            if let Ok(tx) = GossipSender::init(&conn, self.topic_id).await {
                                let tx = Guarded::new(tx, conn);
                                if let Err(err) = old.init(tx).await {
                                    warn!("Remote failed while pushing queued messages: {err:?}");
                                }
                            }
                        }
                    }
                }
                // We keep old receivers to fully drain them and just add our new receiver.
                self.remote_receivers
                    .push(Box::pin(into_stream(stream).map(move |msg| (remote, msg))));
            }
            TopicMessage::ApiJoin(req) => {
                self.init = true;
                let WithChannels { inner, tx, rx, .. } = req;
                let initial_neighbors = self.neighbors.clone().into_iter();
                self.api_send_tasks.spawn(
                    forward_events(tx, self.api_send_tx.subscribe(), initial_neighbors)
                        .instrument(tracing::Span::current()),
                );
                self.api_receivers.push(Box::pin(into_stream2(rx)));
                self.handle_in_event(InEvent::Command(Command::Join(
                    inner.bootstrap.into_iter().collect(),
                )))
                .await;
            }
        }
    }

    async fn handle_remote_message(
        &mut self,
        remote: EndpointId,
        message: n0_error::Result<Option<ProtoMessage>>,
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

    async fn handle_in_event(&mut self, event: InEvent) {
        trace!("in_event {event:?}");
        let now = Instant::now();
        self.shared.metrics.track_in_event(&event);
        self.out_events.extend(self.state.handle(event, now));
        self.process_out_events(now).await;
    }

    async fn process_out_events(&mut self, now: Instant) {
        while let Some(event) = self.out_events.pop_front() {
            trace!("out_event {event:?}");
            self.shared.metrics.track_out_event(&event);
            match event {
                OutEvent::SendMessage(endpoint_id, message) => {
                    self.send(endpoint_id, message).await;
                }
                OutEvent::EmitEvent(event) => {
                    self.handle_event(event);
                }
                OutEvent::ScheduleTimer(delay, timer) => {
                    self.timers.insert(now + delay, timer);
                }
                OutEvent::DisconnectPeer(endpoint_id) => {
                    self.remote_senders.remove(&endpoint_id);
                }
                OutEvent::PeerData(endpoint_id, peer_data) => {
                    self.shared
                        .address_lookup
                        .add_peer_data(endpoint_id, peer_data);
                }
            }
        }
    }

    #[instrument(skip_all, fields(remote=%remote.fmt_short()))]
    async fn send(&mut self, remote: EndpointId, message: ProtoMessage) {
        let sender = match self.remote_senders.entry(remote) {
            hash_map::Entry::Occupied(entry) => entry.into_mut(),
            hash_map::Entry::Vacant(entry) => {
                debug!("requesting new connection");
                let pool = self.shared.pool.clone();
                let topic = self.topic_id;
                self.connecting.push(Box::pin(async move {
                    (remote, connect(pool, remote, topic).await)
                }));
                entry.insert(Default::default())
            }
        };
        if let Err(err) = sender.send(message).await {
            warn!(remote=%remote.fmt_short(), "failed to send message: {err:#}");
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
        self.api_send_tx.send(event).ok();
    }
}

async fn connect(
    pool: ConnectionPool,
    remote: EndpointId,
    topic: TopicId,
) -> n0_error::Result<Guarded<GossipSender>> {
    let conn = pool.get_or_connect(remote).await?;
    let tx = GossipSender::init(&conn, topic).await?;
    let tx = Guarded::new(tx, conn.clone());
    Ok(tx)
}

async fn forward_events(
    tx: channel::mpsc::Sender<api::Event>,
    mut sub: broadcast::Receiver<ProtoEvent>,
    initial_neighbors: impl Iterator<Item = EndpointId>,
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

#[derive(Debug, derive_more::Deref)]
struct Guarded<T> {
    #[deref]
    value: T,
    conn: ConnectionRef,
}

impl<T> Guarded<T> {
    fn new(value: T, conn: ConnectionRef) -> Self {
        Self { value, conn }
    }

    fn conn(&self) -> &ConnectionRef {
        &self.conn
    }
}

impl<T> DerefMut for Guarded<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

#[derive(Debug)]
enum MaybeSender {
    Active(Guarded<GossipSender>),
    Pending(Vec<ProtoMessage>),
}

impl MaybeSender {
    fn is_same_conn(&self, recv: &GossipReceiver) -> bool {
        match self {
            MaybeSender::Active(guarded) => recv.is_same_conn(guarded.conn()),
            MaybeSender::Pending(_) => false,
        }
    }

    async fn send(&mut self, message: ProtoMessage) -> n0_error::Result<()> {
        match self {
            Self::Active(sender) => sender.send(message).await,
            Self::Pending(messages) => {
                messages.push(message);
                Ok(())
            }
        }
    }

    async fn init(&mut self, mut sender: Guarded<GossipSender>) -> n0_error::Result<()> {
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

fn into_stream(
    receiver: GossipReceiver,
) -> impl Stream<Item = n0_error::Result<Option<ProtoMessage>>> + Send + Sync + 'static {
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
        address_lookup::memory::MemoryLookup, endpoint::BindError, protocol::Router, EndpointAddr,
        RelayMap, RelayMode, SecretKey,
    };
    use n0_error::{AnyError, Result, StdResultExt};
    use n0_tracing_test::traced_test;
    use rand::{CryptoRng, Rng, SeedableRng};
    use tokio::{spawn, time::timeout};
    use tokio_util::sync::CancellationToken;
    use tracing::info;

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
        ) -> n0_error::Result<(Self, Endpoint, impl Future<Output = ()>, impl Drop)> {
            let (gossip, actor, ep_handle) =
                Gossip::t_new_with_actor(rng, config, relay_map, cancel).await?;
            let ep = actor.endpoint().clone();
            let me = ep.id().fmt_short();
            let actor_handle =
                task::spawn(actor.run().instrument(tracing::error_span!("gossip", %me)));
            Ok((gossip, ep, ep_handle, AbortOnDropHandle::new(actor_handle)))
        }
        pub(super) async fn t_new_with_actor(
            rng: &mut rand_chacha::ChaCha12Rng,
            config: proto::Config,
            relay_map: RelayMap,
            cancel: &CancellationToken,
        ) -> n0_error::Result<(Self, Actor, impl Future<Output = ()>)> {
            let endpoint = Endpoint::empty_builder(RelayMode::Custom(relay_map))
                .secret_key(SecretKey::generate(rng))
                .insecure_skip_relay_cert_verify(true)
                .bind()
                .await?;

            endpoint.online().await;
            let (gossip, mut actor) = Gossip::new_with_actor(endpoint.clone(), config, None);
            actor.endpoint_addr_updates = Box::pin(n0_future::stream::pending());
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
        memory_lookup: Option<MemoryLookup>,
    ) -> Result<Endpoint, BindError> {
        let ep = Endpoint::empty_builder(RelayMode::Custom(relay_map))
            .secret_key(SecretKey::generate(rng))
            .alpns(vec![GOSSIP_ALPN.to_vec()])
            .insecure_skip_relay_cert_verify(true)
            .bind()
            .await?;

        if let Some(memory_lookup) = memory_lookup {
            ep.address_lookup().add(memory_lookup);
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
        let mut rng = rand_chacha::ChaCha12Rng::seed_from_u64(1);
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

        // create the second endpoint with the usual actor loop
        let (go2, ep2, ep2_handle, _test_actor_handle) =
            Gossip::t_new(rng, Default::default(), relay_map, &ct).await?;

        let endpoint_id1 = actor.endpoint().id();
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
        .instrument(tracing::debug_span!("endpoint_2", id=%endpoint_id2.fmt_short()));
        let go2_handle = task::spawn(go2_task);

        // first endpoint
        let addr2 = EndpointAddr::new(endpoint_id2).with_relay_url(relay_url);
        let memory_lookup = MemoryLookup::new();
        memory_lookup.add_endpoint_info(addr2);
        actor.endpoint().address_lookup().add(memory_lookup);
        // we use a channel to signal advancing steps to the task
        let (go1_resubscribe_tx, mut go1_resubscribe_rx) = mpsc::channel::<()>(1);
        let (go1_joined_tx, mut go1_joined_rx) = mpsc::channel::<()>(1);
        let ct1 = ct.clone();
        let go1_task = async move {
            // first subscribe is done immediately
            tracing::info!("subscribing the first time");
            let sub_1a = go1.subscribe_and_join(topic, vec![endpoint_id2]).await?;
            tracing::info!("subscribed the first time");

            go1_joined_tx.send(()).await.unwrap();

            // wait for signal to subscribe a second time
            go1_resubscribe_rx
                .recv()
                .await
                .expect("signal for second subscribe");
            tracing::info!("subscribing a second time");
            let sub_1b = go1.subscribe_and_join(topic, vec![endpoint_id2]).await?;
            drop(sub_1a);

            // wait for signal to drop the second handle as well
            go1_resubscribe_rx
                .recv()
                .await
                .expect("signal for second subscribe");
            tracing::info!("dropping all handles");
            drop(sub_1b);

            // wait for cancellation
            ct1.cancelled().await;
            drop(go1);

            Ok::<_, AnyError>(())
        }
        .instrument(tracing::debug_span!("endpoint_1", id=%endpoint_id1.fmt_short()));
        let go1_handle = task::spawn(go1_task);

        // advance and check that the topic is now subscribed
        actor.steps(1).await?; // api_rx subscribe;
        go1_joined_rx.recv().await.unwrap();
        tracing::info!("subscribe and join done, should be joined");
        let state = actor.topics.get(&topic).expect("get registered topic");
        assert!(state.joined());

        // signal the second subscribe, we should remain subscribed
        go1_resubscribe_tx
            .send(())
            .await
            .std_context("signal additional subscribe")?;
        actor.steps(1).await?; // api_rx subscribe;
        let state = actor.topics.get(&topic).expect("get registered topic");
        assert!(state.joined());

        // signal to drop the second handle, the topic should no longer be subscribed
        go1_resubscribe_tx
            .send(())
            .await
            .std_context("signal drop handles")?;
        actor.steps(1).await?; // topic task finished
        assert!(actor.topics.get(&topic).is_none());

        // cleanup and ensure everything went as expected
        ct.cancel();
        let wait = Duration::from_secs(4);
        timeout(wait, ep1_handle)
            .await
            .std_context("wait endpoint1 task")?;
        timeout(wait, ep2_handle)
            .await
            .std_context("wait endpoint2 task")?;
        timeout(wait, go1_handle)
            .await
            .std_context("wait gossip1 task")?
            .std_context("join gossip1 task")??;
        timeout(wait, go2_handle)
            .await
            .std_context("wait gossip2 task")?
            .std_context("join gossip1 task")??;
        timeout(wait, actor.finish())
            .await
            .std_context("wait actor finish")?;

        Ok(())
    }

    /// Test that endpoints can reconnect to each other.
    ///
    /// This test will create two endpoints subscribed to the same topic. The second endpoint will
    /// unsubscribe and then resubscribe and connection between the endpoints should succeed both
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

        let endpoint_id1 = ep1.id();
        let endpoint_id2 = ep2.id();
        tracing::info!(
            endpoint_1 = %endpoint_id1.fmt_short(),
            endpoint_2 = %endpoint_id2.fmt_short(),
            "endpoints ready"
        );

        let topic: TopicId = blake3::hash(b"can_reconnect").into();
        tracing::info!(%topic, "joining");

        // channel used to signal the second gossip instance to advance the test
        let (tx, mut rx) = mpsc::channel::<()>(1);
        let addr1 = EndpointAddr::new(endpoint_id1).with_relay_url(relay_url.clone());
        let memory_lookup = MemoryLookup::new();
        memory_lookup.add_endpoint_info(addr1);
        ep2.address_lookup().add(memory_lookup.clone());
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
            tracing::info!("resubscribe ok");
            Ok::<_, ApiError>(())
        }
        .instrument(tracing::debug_span!("endpoint_2", id=%endpoint_id2.fmt_short()));

        let go2_handle = task::spawn(go2_task);

        let addr2 = EndpointAddr::new(endpoint_id2).with_relay_url(relay_url);
        memory_lookup.add_endpoint_info(addr2);
        ep1.address_lookup().add(memory_lookup);

        let mut sub = go1.subscribe(topic, vec![endpoint_id2]).await?;
        // wait for subscribed notification
        sub.joined().await?;
        info!("go1 joined");

        // signal endpoint_2 to unsubscribe
        tx.send(()).await.std_context("signal unsubscribe")?;

        info!("wait for neighbor down");
        // we should receive a Neighbor down event
        let conn_timeout = Duration::from_millis(2000);
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

        // wait for go2 to also be rejoined, then the task terminates
        let wait = Duration::from_secs(5);
        timeout(wait, go2_handle)
            .await
            .std_context("wait gossip2 task")?
            .std_context("join gossip2 task")??;
        ct.cancel();
        timeout(wait, ep1_handle)
            .await
            .std_context("wait endpoint1 task")?;
        timeout(wait, ep2_handle)
            .await
            .std_context("wait endpoint2 task")?;

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
            let ep = Endpoint::empty_builder(RelayMode::Custom(relay_map))
                .secret_key(secret_key)
                .insecure_skip_relay_cert_verify(true)
                .bind()
                .await?;
            let gossip = Gossip::builder().spawn(ep.clone());
            let router = Router::builder(ep).accept(ALPN, gossip.clone()).spawn();
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
            router.endpoint().address_lookup().add(memory_lookup);
            let mut topic = gossip.subscribe_and_join(topic_id, bootstrap).await?;
            topic.broadcast(message.as_bytes().to_vec().into()).await?;
            std::future::pending::<()>().await;
            Ok(())
        }

        let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
        let mut rng = &mut rand_chacha::ChaCha12Rng::seed_from_u64(1);
        let topic_id = TopicId::from_bytes(rng.random());

        // spawn a gossip endpoint, send the endpoint's address on addr_tx,
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
        let secret = SecretKey::generate(&mut rng);
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

        let ep1 = Endpoint::empty_builder(RelayMode::Disabled).bind().await?;
        let ep2 = Endpoint::empty_builder(RelayMode::Disabled).bind().await?;
        let gossip1 = Gossip::builder().alpn(alpn).spawn(ep1.clone());
        let gossip2 = Gossip::builder().alpn(alpn).spawn(ep2.clone());
        let router1 = Router::builder(ep1).accept(alpn, gossip1.clone()).spawn();
        let router2 = Router::builder(ep2).accept(alpn, gossip2.clone()).spawn();

        let addr1 = router1.endpoint().addr();
        let id1 = addr1.id;
        let memory_lookup = MemoryLookup::new();
        memory_lookup.add_endpoint_info(addr1);
        router2.endpoint().address_lookup().add(memory_lookup);

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
        let rng = &mut rand_chacha::ChaCha12Rng::seed_from_u64(1);

        async fn spawn(
            rng: &mut impl CryptoRng,
        ) -> n0_error::Result<(EndpointId, Router, Gossip, GossipSender, GossipReceiver)> {
            let topic_id = TopicId::from([0u8; 32]);
            let ep = Endpoint::empty_builder(RelayMode::Disabled)
                .secret_key(SecretKey::generate(rng))
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
        r2.endpoint().address_lookup().add(lookup.clone());
        tx2.join_peers(vec![n1]).await?;

        // await join endpoint2 -> nodde1
        timeout(Duration::from_secs(3), rx1.joined())
            .await
            .std_context("wait rx1 join")??;
        timeout(Duration::from_secs(3), rx2.joined())
            .await
            .std_context("wait rx2 join")??;

        // add addr info of endpoint1 to endpoint3 and join endpoint1
        r3.endpoint().address_lookup().add(lookup.clone());
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

        let ep1 = Endpoint::empty_builder(RelayMode::Disabled).bind().await?;
        let ep2 = Endpoint::empty_builder(RelayMode::Disabled).bind().await?;
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
        router2.endpoint().address_lookup().add(mem_lookup);

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
}
