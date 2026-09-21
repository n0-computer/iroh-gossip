//! Networking for the `iroh-gossip` protocol

#[cfg(test)]
use std::sync::atomic::AtomicBool;
use std::{
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    ops::ControlFlow,
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
    Endpoint, EndpointAddr, EndpointId,
};
use iroh_util::connection_pool::{self, ConnectionHandle, ConnectionPool, Guarded};
use irpc::{
    channel::{self, mpsc::RecvError},
    WithChannels,
};
use n0_error::{anyerr, stack_error};
use n0_future::{
    stream::Boxed as BoxStream,
    task::{self, AbortOnDropHandle, JoinSet},
    time::Instant,
    MergeUnbounded, Stream, StreamExt,
};
use n0_watcher::{Watchable, Watcher};
use rand::rngs::StdRng;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, error_span, trace, warn, Instrument};

use self::{
    address_lookup::GossipAddressLookup,
    util::{AddrInfo, Timers},
};
use crate::{
    api::{self, GossipApi},
    metrics::Metrics,
    net::net_proto::{GossipReceiver, GossipSender},
    proto::{self, Config, HyparviewConfig, PeerData, PlumtreeConfig, TopicId},
};

mod address_lookup;
mod net_proto;
mod util;

/// How long a connection nothing uses is kept open.
///
/// Applies to superseded connections too, so tests that watch for a connection
/// being closed from under a peer have to outlast it. Short under test for that
/// reason.
const CONN_IDLE_TIMEOUT: Duration = if cfg!(test) {
    Duration::from_secs(1)
} else {
    Duration::from_secs(10)
};

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
    local_tx: mpsc::Sender<LocalMessage>,
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

    /// Leaves every topic and stops the gossip actor.
    ///
    /// Each topic tells its neighbors it is leaving, as it does when its last
    /// subscriber goes away. Subscriptions end, and later API calls fail.
    /// Resolves once the gossip actor has stopped, so that a router closing the
    /// endpoint afterwards does not cut the `Disconnect`s off.
    async fn shutdown(&self) {
        let (reply, done) = oneshot::channel();
        let shutdown = LocalMessage::Shutdown(reply);
        self.0.local_tx.send(shutdown).await.ok();
        // Resolves when the actor replies, or right away if it is already gone
        // and `reply` was dropped with the failed send.
        done.await.ok();
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

    /// Listen on a noq endpoint for incoming RPC connections.
    #[cfg(feature = "rpc")]
    pub async fn listen(self, endpoint: noq::Endpoint) {
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
        self.0.metrics.peers_accepted.inc();
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
        let (api_tx, local_tx, pool, actor) = Actor::new(endpoint, config, alpn, metrics.clone());
        let actor_task = task::spawn(actor.run().instrument(tracing::Span::current()));

        Self(Arc::new(Inner {
            max_message_size,
            api: GossipApi::local(api_tx),
            pool,
            local_tx,
            metrics,
            _actor_handle: AbortOnDropHandle::new(actor_task),
        }))
    }

    #[cfg(test)]
    fn new_with_actor(endpoint: Endpoint, config: Config, alpn: Option<Bytes>) -> (Self, Actor) {
        let metrics = Arc::new(Metrics::default());
        let max_message_size = config.max_message_size;
        let (api_tx, local_tx, pool, actor) = Actor::new(endpoint, config, alpn, metrics.clone());
        let handle = Self(Arc::new(Inner {
            pool,
            local_tx,
            max_message_size,
            api: GossipApi::local(api_tx),
            metrics,
            _actor_handle: AbortOnDropHandle::new(task::spawn(std::future::pending())),
        }));
        (handle, actor)
    }
}

/// What reaches the gossip actor from inside this crate, as opposed to the API.
#[derive(Debug)]
// Nearly every message is a stream. Boxing it to shrink a variant sent once
// per lifetime would cost an allocation per stream instead.
#[allow(clippy::large_enum_variant)]
enum LocalMessage {
    /// A stream a peer opened, from an accept loop.
    RemoteStream {
        remote: EndpointId,
        stream: RemoteStream,
    },
    /// Leave every topic and stop, then reply. See [`Gossip::shutdown`].
    Shutdown(oneshot::Sender<()>),
}

/// Error emitted when the gossip actor stopped.
#[stack_error(derive)]
pub struct ActorStoppedError;

#[derive(Debug, strum::Display)]
enum TopicMessage {
    ApiJoin(ApiJoinRequest),
    RemoteStream {
        remote: EndpointId,
        stream: RemoteStream,
    },
}

/// A stream a peer opened to us, holding its connection in use.
///
/// The peer may keep sending on a connection we have superseded, so the stream,
/// not our choice of connection, decides how long the connection stays open.
type RemoteStream = Guarded<GossipReceiver>;

type ApiJoinRequest = WithChannels<api::JoinRequest, api::Request>;
type ApiRecvStream = BoxStream<Result<api::Command, RecvError>>;
type RemoteRecvStream = BoxStream<(EndpointId, n0_error::Result<Option<ProtoMessage>>)>;

/// The topic actors and everything waiting for one.
///
/// Owned by the gossip [`Actor`] alone, and the only place that sends to a
/// topic actor. That single ownership is what the closing protocol relies on;
/// see [`TopicMap::send`] and [`TopicMap::reap`].
#[derive(Debug, Default)]
struct TopicMap {
    topics: HashMap<TopicId, TopicEntry>,
    tasks: JoinSet<TopicExit>,
    /// Streams that arrived for a topic that is not joined, keyed by topic.
    ///
    /// Two peers joining the same topic at once each open a stream before the
    /// other has processed its own local join, so this is expected rather than
    /// an error. Handed to the topic actor if the topic is ever joined, and
    /// capped by [`MAX_PENDING_STREAMS`] because a peer can ask about topics we
    /// never join.
    parked: HashMap<TopicId, Vec<(EndpointId, RemoteStream)>>,
}

/// How many streams to hold for topics that are not joined.
const MAX_PENDING_STREAMS: usize = 32;

#[derive(Debug)]
enum TopicEntry {
    Running(TopicHandle),
    /// The actor closed its inbox and is quitting. Messages for the topic wait
    /// here until it has been reaped, so that a successor only starts once the
    /// predecessor's `Disconnect`s are out.
    Quitting(Vec<TopicMessage>),
}

/// What a topic actor hands back when it stops.
#[derive(Debug)]
struct TopicExit {
    topic_id: TopicId,
    /// Messages that were in its inbox when it closed it.
    leftovers: Vec<TopicMessage>,
}

impl TopicMap {
    /// Delivers `msg` to the actor for `topic_id`, starting one for a join if
    /// there is none.
    ///
    /// Nothing sent here is lost. A topic actor stops by closing its inbox and
    /// draining it (see [`TopicActor::run`]), so a send either lands before the
    /// close, and comes back from the actor as a leftover, or fails and returns
    /// the message. In both cases [`Self::reap`] gets it.
    async fn send(&mut self, shared: &Arc<Shared>, topic_id: TopicId, msg: TopicMessage) {
        match self.topics.get_mut(&topic_id) {
            Some(TopicEntry::Running(handle)) => {
                if let Err(mpsc::error::SendError(msg)) = handle.tx.send(msg).await {
                    debug!(topic=%topic_id.fmt_short(), "topic actor is quitting, holding message");
                    self.topics
                        .insert(topic_id, TopicEntry::Quitting(vec![msg]));
                }
            }
            Some(TopicEntry::Quitting(buffer)) => buffer.push(msg),
            None => self.dispatch(shared, topic_id, vec![msg]),
        }
    }

    /// Handles a topic actor that stopped.
    ///
    /// Its leftovers and whatever was held for it while it quit go to a
    /// successor if they include a join, and are parked otherwise.
    fn reap(&mut self, shared: &Arc<Shared>, exit: TopicExit) {
        let TopicExit {
            topic_id,
            mut leftovers,
        } = exit;
        trace!(topic=%topic_id.fmt_short(), leftovers = leftovers.len(), "topic actor finished");
        if let Some(TopicEntry::Quitting(held)) = self.topics.remove(&topic_id) {
            leftovers.extend(held);
        }
        if !leftovers.is_empty() {
            self.dispatch(shared, topic_id, leftovers);
        }
    }

    /// Starts an actor for `msgs` if they include a join, and parks their
    /// streams otherwise. A stream alone never creates topic state.
    fn dispatch(&mut self, shared: &Arc<Shared>, topic_id: TopicId, msgs: Vec<TopicMessage>) {
        debug_assert!(
            !self.topics.contains_key(&topic_id),
            "a topic actor started while another was still around"
        );
        if msgs
            .iter()
            .any(|msg| matches!(msg, TopicMessage::ApiJoin(_)))
        {
            let parked = self.parked.remove(&topic_id).unwrap_or_default();
            let initial = msgs.into_iter().chain(
                parked
                    .into_iter()
                    .map(|(remote, stream)| TopicMessage::RemoteStream { remote, stream }),
            );
            let (handle, actor) = TopicHandle::new(topic_id, shared.clone());
            self.topics.insert(topic_id, TopicEntry::Running(handle));
            self.tasks.spawn(
                actor
                    .run(initial.collect())
                    .instrument(error_span!("topic", topic=%topic_id.fmt_short())),
            );
            return;
        }
        for msg in msgs {
            let TopicMessage::RemoteStream { remote, stream } = msg else {
                unreachable!("checked above");
            };
            if self.parked.values().map(Vec::len).sum::<usize>() >= MAX_PENDING_STREAMS {
                debug!(topic=%topic_id.fmt_short(), "dropping stream: too many parked");
                continue;
            }
            debug!(topic=%topic_id.fmt_short(), "parking stream for an unjoined topic");
            self.parked
                .entry(topic_id)
                .or_default()
                .push((remote, stream));
        }
    }

    /// Stops every topic actor and waits until they have left their topics.
    ///
    /// Dropping an actor's handle is the signal: its inbox closes, and it leaves
    /// the topic as it would with no subscribers left. Nothing is started in its
    /// place, so joins held here or left over in its inbox are dropped, which
    /// ends those subscriptions.
    async fn shut_down(&mut self) {
        debug!(topics = self.tasks.len(), "shutting down");
        self.topics.clear();
        while let Some(res) = self.tasks.join_next().await {
            res.expect("topic actor task panicked");
        }
    }

    /// Number of streams held for topics that are not joined.
    #[cfg(test)]
    fn parked_len(&self) -> usize {
        self.parked.values().map(Vec::len).sum()
    }

    /// Number of topics with an entry, running or quitting.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.topics.len()
    }

    /// Whether the actor for `topic_id` is running and still accepts messages.
    #[cfg(test)]
    fn is_running(&self, topic_id: &TopicId) -> bool {
        matches!(
            self.topics.get(topic_id),
            Some(TopicEntry::Running(handle)) if !handle.tx.is_closed()
        )
    }

    /// Whether the actor for `topic_id` has seen a neighbor come up.
    #[cfg(test)]
    fn joined(&self, topic_id: &TopicId) -> Option<bool> {
        match self.topics.get(topic_id)? {
            TopicEntry::Running(handle) => Some(handle.joined()),
            TopicEntry::Quitting(_) => None,
        }
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
    local_rx: mpsc::Receiver<LocalMessage>,
    endpoint_addr_updates: BoxStream<EndpointAddr>,
}

impl Actor {
    fn new(
        endpoint: Endpoint,
        config: Config,
        alpn: Option<Bytes>,
        metrics: Arc<Metrics>,
    ) -> (
        mpsc::Sender<api::RpcMessage>,
        mpsc::Sender<LocalMessage>,
        ConnectionPool,
        Self,
    ) {
        let (api_tx, api_rx) = tokio::sync::mpsc::channel(16);

        let me = endpoint.id();

        let endpoint_addr_updates = endpoint.watch_addr().stream();
        let address_lookup = GossipAddressLookup::default();

        // `Endpoint::address_lookup` returns `Err` when the endpoint is closed.
        // In that case, the gossip actor will close too very soon for other reasons,
        // so it's fine if we only add our `GossipAddressLookup` for the non-closed
        // case. The alternative would be to return a `Result` from `spawn`,
        // but as long as this is the only direct error case, it seem unwarranted.
        if let Ok(endpoint_addr_lookup) = endpoint.address_lookup().as_ref() {
            endpoint_addr_lookup.add(address_lookup.clone());
        }
        let initial_peer_data = AddrInfo::from(endpoint.addr()).encode();

        let alpn = alpn.unwrap_or_else(|| crate::ALPN.to_vec().into());

        let max_message_size = config.max_message_size;
        let (local_tx, local_rx) = mpsc::channel(16);
        let mut options = connection_pool::Options::default().with_on_connected({
            let local_tx = local_tx.clone();
            move |_ep, conn| {
                let local_tx = local_tx.clone();
                Box::pin(async move {
                    task::spawn(accept_loop(local_tx, conn, max_message_size));
                    Ok(())
                })
            }
        });
        options.connect_timeout = Duration::from_secs(10);
        options.idle_timeout = CONN_IDLE_TIMEOUT;
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
            local_tx,
            pool,
            Actor {
                #[cfg(test)]
                endpoint,
                shared,
                api_rx,
                local_rx,
                endpoint_addr_updates: Box::pin(endpoint_addr_updates),
                topics: TopicMap::default(),
            },
        )
    }

    async fn run(mut self) {
        while let ControlFlow::Continue(()) = self.tick().await {}
    }

    #[cfg(test)]
    #[tracing::instrument("gossip", skip_all, fields(me=%self.shared.me.fmt_short()))]
    pub(crate) async fn finish(self) {
        self.run().await
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
            Some(msg) = self.local_rx.recv() => match msg {
                LocalMessage::RemoteStream { remote, stream } => {
                    trace!(remote=%remote.fmt_short(), "tick: remote stream");
                    let topic_id = stream.topic_id();
                    let msg = TopicMessage::RemoteStream { remote, stream };
                    self.topics.send(&self.shared, topic_id, msg).await;
                    ControlFlow::Continue(())
                }
                LocalMessage::Shutdown(reply) => {
                    self.topics.shut_down().await;
                    reply.send(()).ok();
                    ControlFlow::Break(())
                }
            },
            Some(exit) = self.topics.tasks.join_next(), if !self.topics.tasks.is_empty() => {
                let exit = exit.expect("topic actor task panicked");
                self.topics.reap(&self.shared, exit);
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
        self.topics
            .send(&self.shared, topic_id, TopicMessage::ApiJoin(msg))
            .await;
    }
}

/// Accepts the streams a peer opens on `conn` and hands them to the gossip
/// actor, which routes them to the topic.
async fn accept_loop(
    actor: mpsc::Sender<LocalMessage>,
    conn: ConnectionHandle,
    max_message_size: usize,
) {
    let remote = conn.remote_id();
    loop {
        let stream = match GossipReceiver::accept(&conn, max_message_size).await {
            Ok(Some(stream)) => conn.guard(stream),
            _ => break,
        };
        if actor
            .send(LocalMessage::RemoteStream { remote, stream })
            .await
            .is_err()
        {
            break;
        }
    }
}

/// The [`TopicMap`]'s end of a topic actor's inbox.
#[derive(Debug)]
struct TopicHandle {
    tx: mpsc::Sender<TopicMessage>,
    #[cfg(test)]
    joined: Arc<AtomicBool>,
}

impl TopicHandle {
    fn new(topic_id: TopicId, shared: Arc<Shared>) -> (Self, TopicActor) {
        let (tx, rx) = mpsc::channel(16);
        let state = State::new(shared.me, None, shared.config.clone());
        #[cfg(test)]
        let joined = Arc::new(AtomicBool::new(false));
        let peer_data = Box::pin(shared.our_peer_data.watch().stream());
        let actor = TopicActor {
            topic_id,
            shared,
            state,
            rx,
            peer_data,
            #[cfg(test)]
            joined: joined.clone(),
            timers: Default::default(),
            neighbors: Default::default(),
            out_events: Default::default(),
            subscribers: Subscribers::default(),
            senders: Default::default(),
            remote_receivers: Default::default(),
            drop_peers_queue: Default::default(),
        };
        let handle = Self {
            tx,
            #[cfg(test)]
            joined,
        };
        (handle, actor)
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
    drop_peers_queue: HashSet<EndpointId>,
    #[cfg(test)]
    joined: Arc<AtomicBool>,

    // -- senders and receivers
    peer_data: BoxStream<PeerData>,
    rx: mpsc::Receiver<TopicMessage>,
    subscribers: Subscribers,
    senders: PeerSenders,
    remote_receivers: MergeUnbounded<RemoteRecvStream>,
}

impl TopicActor {
    /// Runs the actor until the topic has no subscribers left.
    ///
    /// `initial` is handled before anything else. Registering a stream does not
    /// read from it, so its order relative to the joins does not matter: every
    /// join in `initial` is processed before the first message is read.
    ///
    /// Then leaves the topic through [`Self::leave`].
    async fn run(mut self, initial: Vec<TopicMessage>) -> TopicExit {
        self.shared.metrics.topics_joined.inc();
        for msg in initial {
            self.handle_actor_message(msg);
        }
        while let ControlFlow::Continue(()) = self.tick().await {}
        self.leave().await
    }

    /// Waits for the next event and handles it.
    ///
    /// Returns `Break` once the actor should leave the topic: it has no
    /// subscribers left, or the gossip actor let go of it.
    async fn tick(&mut self) -> ControlFlow<()> {
        tokio::select! {
            msg = self.rx.recv() => match msg {
                Some(msg) => {
                    trace!("tick: actor_rx {msg}");
                    self.handle_actor_message(msg);
                }
                // The owner let go of us: gossip is shutting down.
                None => {
                    debug!("closing topic: gossip is shutting down");
                    return ControlFlow::Break(());
                }
            },
            command = self.subscribers.next() => {
                if let Some(command) = command {
                    trace!("tick: api message {command}");
                    self.handle_in_event(InEvent::Command(command.into()));
                }
            }
            Some((remote, message)) = self.remote_receivers.next(), if !self.remote_receivers.is_empty() => {
                trace!(remote=%remote.fmt_short(), msg=?message, "tick: recv from remote");
                self.handle_remote_message(remote, message);
            }
            Some(data) = self.peer_data.next() => {
                trace!("tick: peer_data");
                self.handle_in_event(InEvent::UpdatePeerData(data));
            }
            _ = self.timers.wait_next() => {
                trace!("tick: timers");
                let now = Instant::now();
                while let Some((_instant, timer)) = self.timers.pop_before(now) {
                    self.handle_in_event(InEvent::TimerExpired(timer));
                }
            }
            (remote, id, exit) = self.senders.next_exit() => {
                self.handle_sender_exit(remote, id, exit);
            }
            else => return ControlFlow::Break(()),
        }

        if !self.drop_peers_queue.is_empty() {
            trace!(len = self.drop_peers_queue.len(), "process peer drop queue");
            let now = Instant::now();
            for peer in self.drop_peers_queue.drain() {
                self.out_events
                    .extend(self.state.handle(InEvent::PeerDisconnected(peer), now));
            }
            self.process_out_events(now);
        }

        if self.subscribers.is_empty() {
            debug!("closing topic: all subscribers dropped");
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    }

    /// Leaves the topic and hands back what was left in the inbox.
    ///
    /// Closes the inbox and drains it until `recv` returns `None`, which tokio
    /// only does once no permit taken before the close is still outstanding, so
    /// every message sent to the actor is either handled or returned. Then runs
    /// the protocol's `Quit` and waits for the send tasks to write what is
    /// queued -- the `Disconnect`s `Quit` just produced -- so they are out before
    /// a successor can start. Each task gets at most `DRAIN_TIMEOUT` once its
    /// sender is dropped.
    async fn leave(mut self) -> TopicExit {
        self.rx.close();
        let mut leftovers = Vec::new();
        while let Some(msg) = self.rx.recv().await {
            leftovers.push(msg);
        }
        self.handle_in_event(InEvent::Command(Command::Quit));
        self.senders.drain().await;
        self.shared.metrics.topics_quit.inc();
        debug!(leftovers = leftovers.len(), "topic closed");
        TopicExit {
            topic_id: self.topic_id,
            leftovers,
        }
    }

    /// Handles a peer's send task ending.
    ///
    /// Only the current task for the peer counts. A task we already replaced or
    /// dropped also ends, and that is ordinary: a dial that lost to a newer one,
    /// or a stream that finished and was acknowledged after we stopped using it.
    /// Acting on those would take down the sender that replaced them.
    ///
    /// The current task's entry is removed before the protocol hears of the
    /// disconnect, so that whatever the protocol sends in response -- a retried
    /// join, say -- starts a fresh task rather than queueing behind a dead one.
    fn handle_sender_exit(&mut self, remote: EndpointId, id: SenderId, exit: SenderExit) {
        if !self.senders.remove_if_current(remote, id) {
            trace!(remote=%remote.fmt_short(), ?exit, "replaced sender ended");
            return;
        }
        let remote_id = remote.fmt_short();
        match exit {
            SenderExit::DialFailed(err) => {
                debug!(remote=%remote_id, ?err, "dial failed, drop peer")
            }
            SenderExit::WriteFailed(err) => {
                debug!(remote=%remote_id, ?err, "write failed, drop peer")
            }
            SenderExit::Stopped => debug!(remote=%remote_id, "peer stopped reading, drop peer"),
            // Only reachable once the queue is closed, which the current sender's is not.
            SenderExit::Finished | SenderExit::DrainTimedOut => {
                debug!(remote=%remote_id, ?exit, "sender ended, drop peer")
            }
        }
        self.drop_peers_queue.insert(remote);
    }

    fn handle_actor_message(&mut self, msg: TopicMessage) {
        match msg {
            TopicMessage::RemoteStream { remote, stream } => {
                self.register_remote_stream(remote, stream);
            }
            TopicMessage::ApiJoin(req) => {
                let WithChannels { inner, tx, rx, .. } = req;
                self.subscribers.add(tx, rx, self.neighbors.clone());
                self.handle_in_event(InEvent::Command(Command::Join(
                    inner.bootstrap.into_iter().collect(),
                )));
            }
        }
    }

    /// Starts reading a stream the peer opened.
    ///
    /// Leaves our sender alone even if it is on a different connection: two
    /// peers need not agree on which connection is current, and each keeps the
    /// other's connection open for as long as it has a stream on it.
    fn register_remote_stream(&mut self, remote: EndpointId, stream: RemoteStream) {
        debug!(remote=%remote.fmt_short(), "remote stream opened");
        self.remote_receivers
            .push(Box::pin(into_stream(stream).map(move |msg| (remote, msg))));
    }

    fn handle_remote_message(
        &mut self,
        remote: EndpointId,
        message: n0_error::Result<Option<ProtoMessage>>,
    ) {
        // A stream ending is not the peer going away. The peer finishes a stream
        // when it moves its sender to another connection, and says so at the
        // protocol level when it actually leaves. A connection that is lost
        // outright shows up on our sender instead, through `sender_stopped`.
        match message {
            Ok(Some(message)) => self.handle_in_event(InEvent::RecvMessage(remote, message)),
            Ok(None) => debug!(remote=%remote.fmt_short(), "remote stream finished"),
            Err(error) => debug!(remote=%remote.fmt_short(), ?error, "remote stream failed"),
        }
    }

    fn handle_in_event(&mut self, event: InEvent) {
        trace!("in_event {event:?}");
        let now = Instant::now();
        self.shared.metrics.track_in_event(&event);
        self.out_events.extend(self.state.handle(event, now));
        self.process_out_events(now);
    }

    fn process_out_events(&mut self, now: Instant) {
        while let Some(event) = self.out_events.pop_front() {
            trace!("out_event {event:?}");
            self.shared.metrics.track_out_event(&event);
            match event {
                OutEvent::SendMessage(remote, message) => {
                    if !self
                        .senders
                        .send(&self.shared, self.topic_id, remote, message)
                    {
                        self.drop_peers_queue.insert(remote);
                    }
                }
                OutEvent::EmitEvent(event) => {
                    self.handle_event(event);
                }
                OutEvent::ScheduleTimer(delay, timer) => {
                    self.timers.insert(now + delay, timer);
                }
                // Dropping the sender lets its task write what is still queued,
                // such as the protocol's `Disconnect`, within `DRAIN_TIMEOUT`.
                OutEvent::DisconnectPeer(endpoint_id) => self.senders.remove(&endpoint_id),
                OutEvent::PeerData(endpoint_id, peer_data) => {
                    self.shared
                        .address_lookup
                        .add_peer_data(endpoint_id, peer_data);
                }
            }
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
        self.subscribers.emit(event);
    }
}

async fn connect(
    shared: &Shared,
    remote: EndpointId,
    topic: TopicId,
) -> n0_error::Result<Guarded<GossipSender>> {
    let res = async {
        let conn = shared.pool.get_or_connect(remote).await?;
        let tx = GossipSender::init(&conn, topic, shared.config.max_message_size).await?;
        n0_error::Ok(conn.guard(tx))
    }
    .await;
    match &res {
        Ok(_) => shared.metrics.peers_dialed_success.inc(),
        Err(_) => shared.metrics.peers_dialed_failure.inc(),
    };
    res
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

/// Identifies one send task a topic actor started, among all it ever starts
/// for any peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SenderId(u64);

/// How many messages may wait for one peer's send task before the peer counts
/// as not keeping up.
const SEND_QUEUE_CAP: usize = 64;

/// How long a peer's send task may keep writing once the topic has let go of
/// it -- long enough to deliver a queued `Disconnect`, short enough that a peer
/// that stopped reading cannot keep the task around.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

/// A topic actor's handle on one peer's send task.
///
/// Dropping it closes the queue and starts the task's `DRAIN_TIMEOUT`.
#[derive(Debug)]
struct PeerSender {
    id: SenderId,
    queue: mpsc::Sender<ProtoMessage>,
    /// Dropped with the handle, which tells the task to wrap up.
    _closing: oneshot::Sender<()>,
}

/// A topic actor's send tasks, one per peer it currently sends to.
#[derive(Debug, Default)]
struct PeerSenders {
    current: HashMap<EndpointId, PeerSender>,
    tasks: JoinSet<(EndpointId, SenderId, SenderExit)>,
    next_id: u64,
}

impl PeerSenders {
    /// Queues `message` for `remote`, starting its send task if there is none.
    ///
    /// Never waits, since waiting on one slow peer would stall the whole topic,
    /// and through the gossip actor's sends to it, every other topic too.
    /// Returns `false`, and forgets the sender, if the peer has let its queue
    /// fill up or its task has ended; the peer then counts as disconnected.
    fn send(
        &mut self,
        shared: &Arc<Shared>,
        topic: TopicId,
        remote: EndpointId,
        message: ProtoMessage,
    ) -> bool {
        let sender = self.current.entry(remote).or_insert_with(|| {
            let id = SenderId(self.next_id);
            self.next_id += 1;
            let (queue, rx) = mpsc::channel(SEND_QUEUE_CAP);
            let (closing, closed) = oneshot::channel();
            self.tasks.spawn(
                run_sender(shared.clone(), remote, topic, id, rx, closed)
                    .instrument(error_span!("send", remote=%remote.fmt_short())),
            );
            PeerSender {
                id,
                queue,
                _closing: closing,
            }
        });
        let Err(err) = sender.queue.try_send(message) else {
            return true;
        };
        match err {
            mpsc::error::TrySendError::Full(_) => {
                warn!(remote=%remote.fmt_short(), "peer is not keeping up, dropping it")
            }
            // The task ended and its exit is on the way; act on it now.
            mpsc::error::TrySendError::Closed(_) => {
                debug!(remote=%remote.fmt_short(), "send task ended, dropping peer")
            }
        }
        self.current.remove(&remote);
        false
    }

    /// Lets go of `remote`'s send task, which delivers what is still queued
    /// within `DRAIN_TIMEOUT`.
    fn remove(&mut self, remote: &EndpointId) {
        self.current.remove(remote);
    }

    /// Lets go of `remote`'s send task if `id` is the current one, and returns
    /// whether it was.
    fn remove_if_current(&mut self, remote: EndpointId, id: SenderId) -> bool {
        let current = matches!(self.current.get(&remote), Some(sender) if sender.id == id);
        if current {
            self.current.remove(&remote);
        }
        current
    }

    /// Waits for a send task to end. Pending while there are none.
    async fn next_exit(&mut self) -> (EndpointId, SenderId, SenderExit) {
        loop {
            match self.tasks.join_next().await {
                Some(res) => {
                    if let Some(exit) = join_result(res) {
                        return exit;
                    }
                }
                None => std::future::pending().await,
            }
        }
    }

    /// Lets go of every send task and waits for them to deliver what is queued.
    async fn drain(&mut self) {
        self.current.clear();
        while let Some(res) = self.tasks.join_next().await {
            join_result(res);
        }
    }
}

/// The local subscribers of a topic.
struct Subscribers {
    /// Commands from every subscriber.
    commands: MergeUnbounded<ApiRecvStream>,
    /// Events for every subscriber, each forwarded by its own task.
    events: broadcast::Sender<ProtoEvent>,
    forwarders: JoinSet<()>,
}

impl Default for Subscribers {
    fn default() -> Self {
        Self {
            commands: Default::default(),
            events: broadcast::channel(512).0,
            forwarders: Default::default(),
        }
    }
}

impl Subscribers {
    /// Adds a subscriber, telling it about the neighbors we already have.
    fn add(
        &mut self,
        events: channel::mpsc::Sender<api::Event>,
        commands: channel::mpsc::Receiver<api::Command>,
        neighbors: BTreeSet<EndpointId>,
    ) {
        self.forwarders.spawn(
            forward_events(events, self.events.subscribe(), neighbors.into_iter())
                .instrument(tracing::Span::current()),
        );
        self.commands.push(Box::pin(into_stream2(commands)));
    }

    fn is_empty(&self) -> bool {
        self.commands.is_empty() && self.forwarders.is_empty()
    }

    fn emit(&self, event: ProtoEvent) {
        self.events.send(event).ok();
    }

    /// Waits for a command from a subscriber.
    ///
    /// Resolves to `None` when a subscriber went away instead, so the caller can
    /// check whether any are left. Pending while there are none.
    async fn next(&mut self) -> Option<api::Command> {
        tokio::select! {
            Some(command) = self.commands.next(), if !self.commands.is_empty() => command.ok(),
            _ = self.forwarders.join_next(), if !self.forwarders.is_empty() => None,
            else => std::future::pending().await,
        }
    }
}

/// Why a peer's send task ended.
#[derive(Debug)]
enum SenderExit {
    /// Its queue was closed and everything in it was written.
    Finished,
    /// It could not reach the peer.
    DialFailed(n0_error::AnyError),
    /// Writing failed.
    WriteFailed(n0_error::AnyError),
    /// The peer stopped reading the stream, or the connection was lost.
    Stopped,
    /// It was still writing when its `DRAIN_TIMEOUT` ran out.
    DrainTimedOut,
}

/// Delivers queued messages to one peer, in order, on a task of its own.
///
/// Runs until the queue is closed and drained, or until the peer cannot be
/// written to. Once the topic drops its [`PeerSender`], it gets `DRAIN_TIMEOUT`
/// to finish.
async fn run_sender(
    shared: Arc<Shared>,
    remote: EndpointId,
    topic: TopicId,
    id: SenderId,
    mut queue: mpsc::Receiver<ProtoMessage>,
    closing: oneshot::Receiver<()>,
) -> (EndpointId, SenderId, SenderExit) {
    let drain_timeout = async {
        closing.await.ok();
        n0_future::time::sleep(DRAIN_TIMEOUT).await;
    };
    let exit = tokio::select! {
        exit = deliver(&shared, remote, topic, &mut queue) => exit,
        _ = drain_timeout => SenderExit::DrainTimedOut,
    };
    (remote, id, exit)
}

async fn deliver(
    shared: &Shared,
    remote: EndpointId,
    topic: TopicId,
    queue: &mut mpsc::Receiver<ProtoMessage>,
) -> SenderExit {
    let mut sender = match connect(shared, remote, topic).await {
        Ok(sender) => sender,
        Err(err) => return SenderExit::DialFailed(err),
    };
    let mut stopped = Box::pin(sender.closed());
    loop {
        let message = tokio::select! {
            message = queue.recv() => match message {
                Some(message) => message,
                None => return SenderExit::Finished,
            },
            _ = &mut stopped => return SenderExit::Stopped,
        };
        // Follow the peer to its current connection. Deciding by our own pool's
        // view, rather than by which connection the peer's streams arrive on,
        // keeps two peers that disagree about the current connection from
        // moving each other's senders back and forth forever.
        if sender.connection().is_superseded() {
            debug!("sender on a superseded connection, moving");
            sender = match connect(shared, remote, topic).await {
                Ok(sender) => sender,
                Err(err) => return SenderExit::DialFailed(err),
            };
            stopped = Box::pin(sender.closed());
        }
        if let Err(err) = sender.send(&message).await {
            return SenderExit::WriteFailed(err);
        }
    }
}

/// Unpacks a finished send task, re-raising a panic.
///
/// Returns `None` for a task that was aborted, which only happens when the
/// topic actor itself is dropped.
fn join_result(
    res: Result<(EndpointId, SenderId, SenderExit), task::JoinError>,
) -> Option<(EndpointId, SenderId, SenderExit)> {
    match res {
        Ok(exit) => Some(exit),
        Err(err) => match err.try_into_panic() {
            Ok(panic) => std::panic::resume_unwind(panic),
            Err(_) => None,
        },
    }
}

fn into_stream(
    receiver: RemoteStream,
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
        address_lookup::memory::MemoryLookup,
        endpoint::{presets, BindError},
        protocol::Router,
        tls::CaTlsConfig,
        RelayMap, RelayMode, SecretKey,
    };
    use n0_error::{ensure_any, AnyError, Result, StdResultExt};
    use n0_tracing_test::traced_test;
    use rand::{CryptoRng, RngExt, SeedableRng};
    use tokio::{spawn, time::timeout};
    use tokio_util::sync::CancellationToken;
    use tracing::info;

    use super::*;
    use crate::{
        api::{ApiError, Event, GossipReceiver, GossipSender},
        ALPN,
    };

    /// How long a [`ManualActor`] waits for the next unit of work before
    /// concluding there is none.
    ///
    /// Also the window in which the tasks the actor spawned -- topic actors,
    /// accept loops -- get to run, since the actor is only driven while a test
    /// awaits it.
    const SETTLE: Duration = Duration::from_millis(50);

    /// How long [`ManualActor::until`] keeps trying.
    const PATIENCE: Duration = Duration::from_secs(10);

    /// A gossip [`Actor`] driven by the test instead of by a task.
    ///
    /// Stepping the actor by hand is what makes the ordering between the actor,
    /// its topic actors and the API observable. Prefer [`Self::until`] over
    /// counting steps: a test that asks for more steps than there is work
    /// blocks forever, and one that asks for fewer leaves work behind, and
    /// neither count is something a test should have to know.
    #[derive(derive_more::Deref, derive_more::DerefMut)]
    pub(super) struct ManualActor(Actor);

    impl ManualActor {
        /// Handles one unit of work. Returns `false` if the actor stopped.
        ///
        /// Returns `true` without doing anything if nothing was ready within
        /// [`SETTLE`].
        async fn step(&mut self) -> bool {
            match n0_future::time::timeout(SETTLE, self.0.tick()).await {
                Ok(ControlFlow::Continue(())) | Err(_) => true,
                Ok(ControlFlow::Break(())) => false,
            }
        }

        /// Lets the tasks the actor spawned run, with the actor itself paused.
        ///
        /// Some orderings are only observable this way. Stepping the actor lets
        /// it reap a finished topic actor, so a test about the window *before*
        /// that reaping must not step it.
        async fn pause(&self) {
            n0_future::time::sleep(SETTLE).await;
        }

        /// Steps until nothing more is ready.
        async fn settle(&mut self) {
            while n0_future::time::timeout(SETTLE, self.0.tick())
                .await
                .is_ok_and(|flow| flow.is_continue())
            {}
        }

        /// Steps until `cond` holds.
        ///
        /// `what` is used in the failure message, phrased as the thing that did
        /// not happen: "the topic actor to start".
        async fn until(&mut self, what: &str, mut cond: impl FnMut(&Actor) -> bool) -> Result {
            let deadline = Instant::now() + PATIENCE;
            while !cond(&self.0) {
                ensure_any!(Instant::now() < deadline, "timed out waiting for {what}");
                ensure_any!(self.step().await, "actor stopped while waiting for {what}");
            }
            Ok(())
        }

        async fn finish(self) {
            self.0.finish().await
        }
    }

    /// A topic actor driven by hand, with one peer it can reach.
    ///
    /// The topic actor belongs to no [`TopicMap`] and no task runs it, so tests
    /// can feed it dial results directly.
    struct DialFixture {
        topic: TopicActor,
        topic_id: TopicId,
        me: EndpointId,
        peer: Gossip,
        peer_id: EndpointId,
        /// Kept so the topic actor's inbox stays open: a closed one means stop.
        _handle: TopicHandle,
        _actor: ManualActor,
        _peer_router: Router,
    }

    impl DialFixture {
        async fn new(relay_map: RelayMap, ct: &CancellationToken) -> Result<Self> {
            let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
            let (_gossip, actor, _router) =
                Gossip::t_new_with_actor(rng, Default::default(), relay_map.clone(), ct).await?;
            let (peer, peer_router) = spawn_node(rng, relay_map, []).await?;
            let peer_addr = peer_router.endpoint().addr();
            let lookup = MemoryLookup::new();
            lookup.add_endpoint_info(peer_addr.clone());
            actor.endpoint().address_lookup()?.add(lookup);

            let topic_id = TopicId::from([5u8; 32]);
            let (handle, topic) = TopicHandle::new(topic_id, actor.shared.clone());
            Ok(Self {
                topic,
                topic_id,
                me: actor.endpoint().id(),
                peer,
                peer_id: peer_addr.id,
                _handle: handle,
                _actor: actor,
                _peer_router: peer_router,
            })
        }

        /// The id of the peer's current send task, if there is one.
        fn sender_id(&self) -> Option<SenderId> {
            self.topic.senders.current.get(&self.peer_id).map(|s| s.id)
        }

        /// Sends the protocol's join to the peer, which starts a send task.
        fn join_peer(&mut self) {
            self.topic
                .handle_in_event(InEvent::Command(Command::Join(vec![self.peer_id])));
        }
    }

    /// Spawns a gossip instance on its own endpoint, with a router accepting on
    /// [`GOSSIP_ALPN`] and `reachable` resolvable.
    async fn spawn_node(
        rng: &mut rand::rngs::ChaCha12Rng,
        relay_map: RelayMap,
        reachable: impl IntoIterator<Item = EndpointAddr>,
    ) -> Result<(Gossip, Router)> {
        let lookup = MemoryLookup::new();
        for addr in reachable {
            lookup.add_endpoint_info(addr);
        }
        let endpoint = create_endpoint(rng, relay_map, Some(lookup)).await?;
        let gossip = Gossip::builder().spawn(endpoint.clone());
        let router = Router::builder(endpoint)
            .accept(GOSSIP_ALPN, gossip.clone())
            .spawn();
        Ok((gossip, router))
    }

    impl Gossip {
        pub(super) async fn t_new<'a>(
            rng: &mut rand::rngs::ChaCha12Rng,
            config: proto::Config,
            relay_map: RelayMap,
            cancel: &'a CancellationToken,
        ) -> n0_error::Result<(
            Self,
            Endpoint,
            impl Future<Output = ()> + use<'a>,
            impl Drop + use<>,
        )> {
            let (gossip, actor, ep_handle) =
                Gossip::t_new_with_actor(rng, config, relay_map, cancel).await?;
            let ep = actor.endpoint().clone();
            let me = ep.id().fmt_short();
            let actor_handle = task::spawn(
                actor
                    .0
                    .run()
                    .instrument(tracing::error_span!("gossip", %me)),
            );
            Ok((gossip, ep, ep_handle, AbortOnDropHandle::new(actor_handle)))
        }
        pub(super) async fn t_new_with_actor<'a>(
            rng: &mut rand::rngs::ChaCha12Rng,
            config: proto::Config,
            relay_map: RelayMap,
            cancel: &'a CancellationToken,
        ) -> n0_error::Result<(Self, ManualActor, impl Future<Output = ()> + use<'a>)> {
            let endpoint = Endpoint::builder(presets::Minimal)
                .relay_mode(RelayMode::Custom(relay_map))
                .secret_key(SecretKey::from_bytes(&rng.random()))
                .ca_tls_config(CaTlsConfig::insecure_skip_verify())
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
            Ok((gossip, ManualActor(actor), router_fut))
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
    /// - Subscribe both nodes to the same topic. The first node will subscribe twice and connect
    ///   to the second node. The second node will subscribe without bootstrap.
    /// - Ensure that the first node removes the subscription iff all topic handles have been
    ///   dropped.
    // NOTE: this is a regression test.
    #[tokio::test]
    #[traced_test]
    async fn subscription_cleanup() -> Result {
        let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
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
        actor.endpoint().address_lookup()?.add(memory_lookup);
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
        actor
            .until("the topic to be joined", |actor| {
                actor.topics.joined(&topic) == Some(true)
            })
            .await?;
        go1_joined_rx.recv().await.unwrap();

        // signal the second subscribe, we should remain subscribed
        go1_resubscribe_tx
            .send(())
            .await
            .std_context("signal additional subscribe")?;
        actor.settle().await;
        assert_eq!(actor.topics.joined(&topic), Some(true));

        // signal to drop the second handle, the topic should no longer be subscribed
        go1_resubscribe_tx
            .send(())
            .await
            .std_context("signal drop handles")?;
        actor
            .until("the topic to be dropped", |actor| {
                actor.topics.joined(&topic).is_none()
            })
            .await?;

        // cleanup and ensure everything went as expected
        ct.cancel();
        let wait = Duration::from_secs(4);
        // Shutting down endpoint 1's router waits for its gossip actor to stop,
        // and nothing steps that actor but us, so run the two side by side.
        let (ep1_res, ()) = tokio::join!(timeout(wait, ep1_handle), actor.finish());
        ep1_res.std_context("wait endpoint1 task")?;
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

        Ok(())
    }

    /// A join that arrives while the topic's actor is quitting is served by its
    /// successor, and the successor starts only once the old actor is gone.
    ///
    /// The old actor has closed its inbox but not been reaped yet, so the send
    /// fails and the map holds the join. Calling `handle_api_message` directly,
    /// rather than stepping the actor, is what orders the join before the reap:
    /// `select!` would pick either.
    #[tokio::test]
    #[traced_test]
    async fn join_racing_topic_shutdown_is_served() -> Result {
        let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
        let ct = CancellationToken::new();
        let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
        let (gossip, mut actor, _router) =
            Gossip::t_new_with_actor(rng, Default::default(), relay_map, &ct).await?;
        let topic_id = TopicId::from([1u8; 32]);

        let topic = gossip.subscribe(topic_id, vec![]).await?;
        actor
            .until("the topic actor to start", |actor| {
                actor.topics.is_running(&topic_id)
            })
            .await?;

        // The only subscriber leaves, and the actor closes its inbox and stops.
        drop(topic);
        actor.pause().await;
        assert!(!actor.topics.is_running(&topic_id));
        assert_eq!(
            actor.topics.tasks.len(),
            1,
            "the old actor is not reaped yet"
        );

        let _topic = gossip.subscribe(topic_id, vec![]).await?;
        let join = actor.api_rx.recv().await.expect("api channel open");
        actor.handle_api_message(join).await;
        assert_eq!(
            actor.topics.tasks.len(),
            1,
            "a successor started before the old actor was reaped"
        );

        actor
            .until("the held join to start a successor", |actor| {
                actor.topics.is_running(&topic_id)
            })
            .await?;
        ct.cancel();
        Ok(())
    }

    /// A join left in a stopping actor's inbox starts a successor.
    #[tokio::test]
    #[traced_test]
    async fn leftover_join_starts_a_successor() -> Result {
        let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
        let ct = CancellationToken::new();
        let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
        let (gossip, mut actor, _router) =
            Gossip::t_new_with_actor(rng, Default::default(), relay_map, &ct).await?;
        let topic_id = TopicId::from([2u8; 32]);

        let _topic = gossip.subscribe(topic_id, vec![]).await?;
        let api::RpcMessage::Join(join) = actor.api_rx.recv().await.expect("api channel open");
        let exit = TopicExit {
            topic_id,
            leftovers: vec![TopicMessage::ApiJoin(join)],
        };
        let shared = actor.shared.clone();
        actor.topics.reap(&shared, exit);

        assert!(
            actor.topics.is_running(&topic_id),
            "the leftover join was dropped"
        );
        ct.cancel();
        Ok(())
    }

    /// A stream for a topic we have not joined must be parked, not reset.
    ///
    /// Two peers joining the same topic at once each open a stream before the
    /// other has processed its own join. Resetting the stream fails the
    /// sender's `GossipSender`, which drops us as a peer just as we were about
    /// to become interested -- and a node bootstrapping off a single seed may
    /// then never join at all.
    #[tokio::test]
    #[traced_test]
    async fn streams_for_unjoined_topics_are_parked() -> Result {
        let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
        let ct = CancellationToken::new();
        let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();

        // Our main actor is stepped by hand, so we have not joined the topic
        // when the stream arrives.
        let (gossip, mut actor, _router) =
            Gossip::t_new_with_actor(rng, Default::default(), relay_map.clone(), &ct).await?;
        let our_addr = actor.endpoint().addr();
        let topic_id = TopicId::from([3u8; 32]);

        // The other side joins the topic with us as its bootstrap peer, which
        // opens a stream for the topic.
        let (sender, _sender_router) = spawn_node(rng, relay_map, [our_addr.clone()]).await?;
        let _sender_topic = sender.subscribe(topic_id, vec![our_addr.id]).await?;

        actor
            .until("a stream to be parked", |actor| {
                actor.topics.parked_len() == 1
            })
            .await?;
        assert_eq!(actor.topics.len(), 0, "the topic was joined on our behalf");

        // Joining now has to pick the parked stream up.
        let _topic = gossip.subscribe(topic_id, vec![]).await?;
        actor
            .until("the parked stream to be used", |actor| {
                actor.topics.parked_len() == 0
            })
            .await?;
        assert_eq!(actor.topics.len(), 1, "the topic actor did not start");

        ct.cancel();
        Ok(())
    }

    /// Two peers that dial each other at once keep a working connection.
    ///
    /// Each side keeps the connection it saw last as its primary, and the two
    /// may disagree. Neither side may close the one the other side uses.
    #[tokio::test]
    #[traced_test]
    async fn concurrent_dials_keep_both_connections() -> Result {
        let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
        let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
        let memory_lookup = MemoryLookup::new();
        let ep1 = create_endpoint(rng, relay_map.clone(), Some(memory_lookup.clone())).await?;
        let ep2 = create_endpoint(rng, relay_map, Some(memory_lookup.clone())).await?;
        let (ep1_id, ep2_id) = (ep1.id(), ep2.id());
        for id in [ep1_id, ep2_id] {
            memory_lookup
                .add_endpoint_info(EndpointAddr::new(id).with_relay_url(relay_url.clone()));
        }
        let go1 = Gossip::builder().spawn(ep1.clone());
        let go2 = Gossip::builder().spawn(ep2.clone());
        let cancel = CancellationToken::new();
        let _loops = [
            AbortOnDropHandle::new(spawn(endpoint_loop(ep1, go1.clone(), cancel.clone()))),
            AbortOnDropHandle::new(spawn(endpoint_loop(ep2, go2.clone(), cancel.clone()))),
        ];

        let topic: TopicId = blake3::hash(b"concurrent_dials").into();
        let [mut t1, mut t2] = [
            go1.subscribe_and_join(topic, vec![ep2_id]),
            go2.subscribe_and_join(topic, vec![ep1_id]),
        ]
        .try_join()
        .await?;

        // The join resolves on `NeighborUp`, before either side has had a chance
        // to supersede a connection. Watch past the idle timeout, after which an
        // unused superseded connection is closed, for the neighbor to be lost.
        let fallout = timeout(CONN_IDLE_TIMEOUT + Duration::from_secs(2), async {
            loop {
                let event = tokio::select! {
                    event = t1.try_next() => event,
                    event = t2.try_next() => event,
                };
                match event {
                    Ok(Some(Event::NeighborDown(_))) => return "a side lost its neighbor",
                    Ok(Some(_)) => {}
                    _ => return "a topic stream ended",
                }
            }
        })
        .await;
        if let Ok(what) = fallout {
            panic!("{what} after concurrent dials");
        }
        cancel.cancel();
        Ok(())
    }

    /// A peer whose connection was closed after it left dials again to rejoin.
    #[tokio::test]
    #[traced_test]
    async fn rejoin_after_idle_close_redials() -> Result {
        let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
        let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
        let memory_lookup = MemoryLookup::new();
        let ep1 = create_endpoint(rng, relay_map.clone(), None).await?;
        let ep2 = create_endpoint(rng, relay_map, Some(memory_lookup.clone())).await?;
        let ep1_id = ep1.id();
        memory_lookup.add_endpoint_info(EndpointAddr::new(ep1_id).with_relay_url(relay_url));
        let go1 = Gossip::builder().spawn(ep1.clone());
        let go2 = Gossip::builder().spawn(ep2.clone());

        // Accept for `go1` and hand every connection to the test as well.
        let (conn_tx, mut conn_rx) = mpsc::channel(2);
        let accept_go1 = go1.clone();
        let _accept = AbortOnDropHandle::new(spawn(async move {
            while let Some(incoming) = ep1.accept().await {
                let conn = incoming.await.expect("accept failed");
                conn_tx.send(conn.clone()).await.ok();
                accept_go1
                    .handle_connection(conn)
                    .await
                    .expect("handle connection");
            }
        }));

        let topic: TopicId = blake3::hash(b"rejoin_after_idle_close").into();
        let _t1 = go1.subscribe(topic, vec![]).await?;
        let t2 = go2.subscribe_and_join(topic, vec![ep1_id]).await?;
        let conn1 = conn_rx.recv().await.expect("first connection");

        // Leaving the topic disconnects the peer on both sides, and the connection
        // closes once it has been unused for the idle timeout.
        drop(t2);
        timeout(CONN_IDLE_TIMEOUT + Duration::from_secs(2), conn1.closed())
            .await
            .std_context("connection was not closed once unused")?;

        go2.subscribe_and_join(topic, vec![ep1_id])
            .await
            .std_context("rejoin")?;
        let conn2 = conn_rx.recv().await.expect("second connection");
        assert_ne!(conn1.stable_id(), conn2.stable_id());
        Ok(())
    }

    /// A join whose dial failed goes out again on a fresh dial.
    ///
    /// The protocol retries a join when the connection closes before a reply,
    /// but `send` only dials for a vacant entry, so the retry reaches the peer
    /// only if the failed dial left nothing behind. This is what
    /// `join_during_peer_close_is_retried` covers on `main`: a join written to a
    /// connection the peer just closed.
    #[tokio::test]
    #[traced_test]
    async fn failed_dial_is_retried() -> Result {
        let ct = CancellationToken::new();
        let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
        let mut f = DialFixture::new(relay_map, &ct).await?;
        let mut peer_topic = f.peer.subscribe(f.topic_id, vec![]).await?;

        f.join_peer();
        // Fail the dial the way a connection closing under the join would.
        let id = f.sender_id().expect("a send task started");
        f.topic.senders.tasks.abort_all();
        let err = anyerr!("connection closed before the join was written");
        f.topic
            .handle_sender_exit(f.peer_id, id, SenderExit::DialFailed(err));
        assert_eq!(f.sender_id(), None, "the failed task was kept");

        // The next event lets the actor tell the protocol, which retries the join
        // on a fresh send task. Which event comes first does not matter, and nor
        // does the `Break` it returns for an actor without subscribers.
        let _ = f.topic.tick().await;
        let me = f.me;
        timeout(Duration::from_secs(10), async {
            loop {
                match peer_topic.try_next().await {
                    Ok(Some(Event::NeighborUp(id))) if id == me => return,
                    Ok(Some(_)) => {}
                    other => panic!("peer topic ended: {other:?}"),
                }
            }
        })
        .await
        .std_context("the peer never received the retried join")?;
        ct.cancel();
        Ok(())
    }

    /// A send task that was replaced ending must leave its replacement alone.
    ///
    /// The old task's end is ordinary -- it finishes its stream once dropped --
    /// and used to be matched to the current sender by connection, which took
    /// down a replacement on the same connection.
    #[tokio::test]
    #[traced_test]
    async fn replaced_sender_ending_keeps_its_replacement() -> Result {
        let ct = CancellationToken::new();
        let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
        let mut f = DialFixture::new(relay_map, &ct).await?;

        f.join_peer();
        let first = f.sender_id().expect("a send task started");
        // The protocol drops the peer, and later talks to it again.
        f.topic.senders.current.remove(&f.peer_id);
        f.join_peer();
        let second = f.sender_id().expect("a send task started");
        assert_ne!(first, second);

        let (remote, id, exit) =
            timeout(Duration::from_secs(10), f.topic.senders.tasks.join_next())
                .await
                .std_context("the dropped task never ended")?
                .and_then(join_result)
                .expect("a send task ended");
        assert_eq!(id, first, "the replacement ended first: {exit:?}");
        f.topic.handle_sender_exit(remote, id, exit);
        assert_eq!(
            f.sender_id(),
            Some(second),
            "a replaced sender's end dropped its replacement"
        );
        ct.cancel();
        Ok(())
    }

    /// A dial failing for a replaced send task must leave the current one alone.
    #[tokio::test]
    #[traced_test]
    async fn stale_dial_failure_keeps_current_sender() -> Result {
        let ct = CancellationToken::new();
        let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
        let mut f = DialFixture::new(relay_map, &ct).await?;

        f.join_peer();
        let first = f.sender_id().expect("a send task started");
        f.topic.senders.current.remove(&f.peer_id);
        f.join_peer();
        let second = f.sender_id().expect("a send task started");

        let err = anyerr!("an older dial failed");
        f.topic
            .handle_sender_exit(f.peer_id, first, SenderExit::DialFailed(err));
        assert_eq!(
            f.sender_id(),
            Some(second),
            "a stale dial failure dropped a live sender"
        );
        ct.cancel();
        Ok(())
    }

    /// A send task the topic let go of delivers what was queued and ends,
    /// rather than holding its connection open.
    #[tokio::test]
    #[traced_test]
    async fn dropped_sender_task_finishes() -> Result {
        let ct = CancellationToken::new();
        let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
        let mut f = DialFixture::new(relay_map, &ct).await?;

        f.join_peer();
        f.topic.senders.current.remove(&f.peer_id);
        let (_, _, exit) = timeout(DRAIN_TIMEOUT * 2, f.topic.senders.tasks.join_next())
            .await
            .std_context("the dropped task never ended")?
            .and_then(join_result)
            .expect("a send task ended");
        assert!(
            matches!(exit, SenderExit::Finished),
            "expected the queue to be delivered, got {exit:?}"
        );
        ct.cancel();
        Ok(())
    }

    /// Leaving a topic delivers what is queued for its peers before it returns.
    ///
    /// That is how a topic's `Disconnect`s reach its neighbors before a
    /// successor can start, and before shutdown lets the endpoint close.
    #[tokio::test]
    #[traced_test]
    async fn leaving_delivers_queued_messages() -> Result {
        let ct = CancellationToken::new();
        let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
        let mut f = DialFixture::new(relay_map, &ct).await?;
        let mut peer_topic = f.peer.subscribe(f.topic_id, vec![]).await?;

        // A join is queued for the peer, whose send task is still dialing.
        f.join_peer();
        let me = f.me;
        timeout(DRAIN_TIMEOUT * 2, f.topic.leave())
            .await
            .std_context("leaving did not finish")?;

        timeout(Duration::from_secs(5), async {
            loop {
                match peer_topic.try_next().await {
                    Ok(Some(Event::NeighborUp(id))) if id == me => return,
                    Ok(Some(_)) => {}
                    other => panic!("peer topic ended: {other:?}"),
                }
            }
        })
        .await
        .std_context("the queued join was not delivered")?;
        ct.cancel();
        Ok(())
    }

    /// A peer that stops taking messages is dropped instead of stalling the
    /// topic.
    ///
    /// Sending used to write to the peer's stream from the topic actor itself,
    /// so a peer that stopped reading blocked the whole topic -- and, through
    /// the gossip actor's sends to it, every other topic too.
    #[tokio::test]
    #[traced_test]
    async fn slow_peer_is_dropped_instead_of_stalling_the_topic() -> Result {
        let ct = CancellationToken::new();
        let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
        let mut f = DialFixture::new(relay_map, &ct).await?;

        // A send task that never takes anything off its queue.
        let (queue, _unread) = mpsc::channel(SEND_QUEUE_CAP);
        let (closing, _closed) = oneshot::channel();
        let stuck = PeerSender {
            id: SenderId(u64::MAX),
            queue,
            _closing: closing,
        };
        f.topic.senders.current.insert(f.peer_id, stuck);

        // Every join sends the peer one message; one more than fits.
        for _ in 0..=SEND_QUEUE_CAP {
            f.join_peer();
        }
        assert_eq!(f.sender_id(), None, "the stuck sender was kept");
        assert!(
            f.topic.drop_peers_queue.contains(&f.peer_id),
            "the protocol was not told"
        );
        ct.cancel();
        Ok(())
    }

    /// Shutting gossip down ends subscriptions and tells neighbors it left.
    ///
    /// `ProtocolHandler::shutdown` used to do nothing. A router closes the
    /// endpoint right after it, so peers only noticed once the connection
    /// dropped, and never got the `Disconnect` that tells them not to keep us as
    /// a candidate to reconnect to.
    #[tokio::test]
    #[traced_test]
    async fn shutdown_ends_subscriptions_and_tells_neighbors() -> Result {
        let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
        let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
        let topic_id = TopicId::from([9u8; 32]);
        let (a, a_router) = spawn_node(rng, relay_map.clone(), []).await?;
        let a_addr = a_router.endpoint().addr();
        let (b, _b_router) = spawn_node(rng, relay_map, [a_addr.clone()]).await?;
        let mut a_topic = a.subscribe(topic_id, vec![]).await?;
        let mut b_topic = b.subscribe_and_join(topic_id, vec![a_addr.id]).await?;

        timeout(Duration::from_secs(10), ProtocolHandler::shutdown(&a))
            .await
            .std_context("shutdown did not finish")?;

        // Once nothing uses the connection, the pool closes it after
        // `CONN_IDLE_TIMEOUT`, and the neighbor would hear of that too. Hearing
        // sooner means it was told, not that the connection timed out.
        let a_id = a_addr.id;
        timeout(CONN_IDLE_TIMEOUT / 2, async {
            loop {
                match b_topic.try_next().await {
                    Ok(Some(Event::NeighborDown(id))) if id == a_id => return,
                    Ok(Some(_)) => {}
                    other => panic!("the neighbor's topic ended: {other:?}"),
                }
            }
        })
        .await
        .std_context("the neighbor did not hear that we left")?;

        let ended = timeout(Duration::from_secs(5), async {
            while let Ok(Some(_)) = a_topic.try_next().await {}
        })
        .await;
        assert!(ended.is_ok(), "a subscription outlived shutdown");

        assert!(
            a.subscribe(topic_id, vec![]).await.is_err(),
            "subscribed after shutdown"
        );
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
            tracing::info!("resubscribe ok");
            Ok::<_, ApiError>(())
        }
        .instrument(tracing::debug_span!("endpoint_2", id=%endpoint_id2.fmt_short()));

        let go2_handle = task::spawn(go2_task);

        let addr2 = EndpointAddr::new(endpoint_id2).with_relay_url(relay_url);
        memory_lookup.add_endpoint_info(addr2);
        ep1.address_lookup()?.add(memory_lookup);

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
            let ep = Endpoint::builder(presets::Minimal)
                .relay_mode(RelayMode::Custom(relay_map))
                .secret_key(secret_key)
                .ca_tls_config(CaTlsConfig::insecure_skip_verify())
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
}
