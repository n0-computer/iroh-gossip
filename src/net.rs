//! Networking for the `iroh-gossip` protocol

use std::{
    collections::{hash_map::Entry, BTreeSet, HashMap, HashSet, VecDeque},
    net::SocketAddr,
    pin::Pin,
    sync::{atomic::AtomicUsize, Arc},
    task::{Context, Poll},
};

use anyhow::Context as _;
use bytes::BytesMut;
use futures_concurrency::stream::{stream_group, StreamGroup};
use futures_util::FutureExt as _;
use iroh::{
    endpoint::{Connection, DirectAddr},
    protocol::ProtocolHandler,
    Endpoint, NodeAddr, NodeId, PublicKey, RelayUrl,
};
use iroh_metrics::inc;
use n0_future::{
    boxed::BoxFuture,
    task::{self, AbortOnDropHandle, JoinSet},
    time::Instant,
    Stream, StreamExt as _, TryFutureExt as _,
};
use rand::rngs::StdRng;
use rand_core::SeedableRng;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, error_span, trace, warn, Instrument};

use self::util::{read_message, write_message, Timers};
use crate::{
    metrics::Metrics,
    proto::{self, HyparviewConfig, PeerData, PlumtreeConfig, Scope, TopicId},
};

mod handles;
pub mod util;

pub use self::handles::{
    Command, CommandStream, Event, GossipEvent, GossipReceiver, GossipSender, GossipTopic,
    JoinOptions, Message,
};

/// ALPN protocol name
pub const GOSSIP_ALPN: &[u8] = b"/iroh-gossip/0";
/// Default channel capacity for topic subscription channels (one per topic)
const TOPIC_EVENTS_DEFAULT_CAP: usize = 2048;
/// Default channel capacity for topic subscription channels (one per topic)
const TOPIC_COMMANDS_DEFAULT_CAP: usize = 2048;
/// Channel capacity for the send queue (one per connection)
const SEND_QUEUE_CAP: usize = 64;
/// Channel capacity for the ToActor message queue (single)
const TO_ACTOR_CAP: usize = 64;
/// Channel capacity for the InEvent message queue (single)
const IN_EVENT_CAP: usize = 1024;
/// Name used for logging when new node addresses are added from gossip.
const SOURCE_NAME: &str = "gossip";

/// Events emitted from the gossip protocol
pub type ProtoEvent = proto::Event<PublicKey>;
/// Commands for the gossip protocol
pub type ProtoCommand = proto::Command<PublicKey>;

type InEvent = proto::InEvent<PublicKey>;
type OutEvent = proto::OutEvent<PublicKey>;
type Timer = proto::Timer<PublicKey>;
type ProtoMessage = proto::Message<PublicKey>;

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
    /// Writing a message to the network
    #[error("write {0}")]
    Write(#[from] util::WriteError),
    /// Reading a message from the network
    #[error("read {0}")]
    Read(#[from] util::ReadError),
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
    #[cfg(feature = "rpc")]
    pub(crate) rpc_handler: Arc<std::sync::OnceLock<crate::rpc::RpcHandler>>,
}

#[derive(Debug)]
pub(crate) struct Inner {
    to_actor_tx: mpsc::Sender<ToActor>,
    _actor_handle: AbortOnDropHandle<()>,
    max_message_size: usize,
    /// Next [`ReceiverId`] to be assigned when a receiver is registered for a topic.
    next_receiver_id: AtomicUsize,
}

impl ProtocolHandler for Gossip {
    fn accept(&self, conn: Connection) -> BoxFuture<anyhow::Result<()>> {
        let inner = self.inner.clone();
        Box::pin(async move {
            inner.handle_connection(conn).await?;
            Ok(())
        })
    }
}

/// Builder to configure and construct [`Gossip`].
#[derive(Debug, Clone)]
pub struct Builder {
    config: proto::Config,
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

    /// Spawn a gossip actor and get a handle for it
    pub async fn spawn(self, endpoint: Endpoint) -> Result<Gossip, Error> {
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

        let (actor, to_actor_tx) = Actor::new(endpoint, self.config, &addr.into());
        let me = actor.endpoint.node_id().fmt_short();
        let max_message_size = actor.state.max_message_size();

        let actor_handle = task::spawn(
            async move {
                if let Err(err) = actor.run().await {
                    warn!("gossip actor closed with error: {err:?}");
                }
            }
            .instrument(error_span!("gossip", %me)),
        );

        Ok(Gossip {
            inner: Inner {
                to_actor_tx,
                _actor_handle: AbortOnDropHandle::new(actor_handle),
                max_message_size,
                next_receiver_id: Default::default(),
            }
            .into(),
            #[cfg(feature = "rpc")]
            rpc_handler: Default::default(),
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

    /// Get the maximum message size configured for this gossip actor.
    pub fn max_message_size(&self) -> usize {
        self.inner.max_message_size
    }

    /// Handle an incoming [`Connection`].
    ///
    /// Make sure to check the ALPN protocol yourself before passing the connection.
    pub async fn handle_connection(&self, conn: Connection) -> Result<(), Error> {
        self.inner.handle_connection(conn).await
    }

    /// Join a gossip topic with the default options and wait for at least one active connection.
    pub async fn subscribe_and_join(
        &self,
        topic_id: TopicId,
        bootstrap: Vec<NodeId>,
    ) -> Result<GossipTopic, Error> {
        let mut sub = self.subscribe_with_opts(topic_id, JoinOptions::with_bootstrap(bootstrap));
        sub.joined().await?;
        Ok(sub)
    }

    /// Join a gossip topic with the default options.
    ///
    /// Note that this will not wait for any bootstrap node to be available. To ensure the topic is connected to at least one node, use [`GossipTopic::joined`] or [`Gossip::subscribe_and_join`]
    pub fn subscribe(
        &self,
        topic_id: TopicId,
        bootstrap: Vec<NodeId>,
    ) -> Result<GossipTopic, Error> {
        let sub = self.subscribe_with_opts(topic_id, JoinOptions::with_bootstrap(bootstrap));

        Ok(sub)
    }

    /// Join a gossip topic with options.
    ///
    /// Returns a [`GossipTopic`] instantly. To wait for at least one connection to be established,
    /// you can await [`GossipTopic::joined`].
    ///
    /// Messages will be queued until a first connection is available. If the internal channel becomes full,
    /// the oldest messages will be dropped from the channel.
    pub fn subscribe_with_opts(&self, topic_id: TopicId, opts: JoinOptions) -> GossipTopic {
        let (command_tx, command_rx) = async_channel::bounded(TOPIC_COMMANDS_DEFAULT_CAP);
        let command_rx: CommandStream = Box::pin(command_rx);
        let event_rx = self.subscribe_with_stream(topic_id, opts, command_rx);
        GossipTopic::new(command_tx, event_rx)
    }

    /// Join a gossip topic with options and an externally-created update stream.
    ///
    /// This method differs from [`Self::subscribe_with_opts`] by letting you pass in a `updates` command stream yourself
    /// instead of using a channel created for you.
    ///
    /// It returns a stream of events. If you want to wait for the topic to become active, wait for
    /// the [`GossipEvent::Joined`] event.
    pub fn subscribe_with_stream(
        &self,
        topic_id: TopicId,
        options: JoinOptions,
        updates: CommandStream,
    ) -> EventStream {
        self.inner.subscribe_with_stream(topic_id, options, updates)
    }
}

impl Inner {
    pub fn subscribe_with_stream(
        &self,
        topic_id: TopicId,
        options: JoinOptions,
        updates: CommandStream,
    ) -> EventStream {
        let (event_tx, event_rx) = async_channel::bounded(options.subscription_capacity);
        let to_actor_tx = self.to_actor_tx.clone();
        let receiver_id = ReceiverId(
            self.next_receiver_id
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        );
        let channels = SubscriberChannels {
            receiver_id,
            command_rx: updates,
            event_tx,
        };
        // We spawn a task to send the subscribe action to the actor, because we want the send to
        // succeed even if the returned stream is dropped right away without being polled, because
        // it is legit to keep only the `updates` stream and drop the event stream. This situation
        // is handled fine within the actor, but we have to make sure that the message reaches the
        // actor.
        let task = task::spawn(async move {
            to_actor_tx
                .send(ToActor::Join {
                    topic_id,
                    bootstrap: options.bootstrap,
                    channels,
                })
                .await
                .map_err(Error::from)
        });
        let stream = async move {
            task.await??;
            Ok(event_rx)
        }
        .try_flatten_stream();
        EventStream {
            inner: Box::pin(stream),
            to_actor_tx: self.to_actor_tx.clone(),
            topic: topic_id,
            receiver_id,
        }
    }

    async fn send(&self, event: ToActor) -> Result<(), Error> {
        self.to_actor_tx.send(event).await?;
        Ok(())
    }

    async fn handle_connection(&self, conn: Connection) -> Result<(), Error> {
        let node_id = conn.remote_node_id()?;
        self.send(ToActor::HandleConnection(node_id, ConnOrigin::Accept, conn))
            .await?;
        Ok(())
    }
}

/// Stream of events for a topic.
#[derive(derive_more::Debug)]
pub struct EventStream {
    /// The actual stream polled to return [`Event`]s to the application.
    #[debug("Stream")]
    inner: Pin<Box<dyn Stream<Item = Result<Event, Error>> + Send + Sync + 'static>>,

    /// Channel to the actor task.
    ///
    /// This is used to handle the receiver being dropped. When all receiver and publishers are
    /// gone the topic will be unsubscribed.
    to_actor_tx: mpsc::Sender<ToActor>,
    /// The topic for which this stream is reporting events.
    ///
    /// This is sent on drop to the actor to handle the receiver going away.
    topic: TopicId,
    /// An Id identifying this specific receiver.
    ///
    /// This is sent on drop to the actor to handle the receiver going away.
    receiver_id: ReceiverId,
}

impl Stream for EventStream {
    type Item = Result<Event, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_next(cx)
    }
}

impl Drop for EventStream {
    fn drop(&mut self) {
        // NOTE: unexpectedly, this works without a tokio runtime, so we leverage that to avoid yet
        // another spawned task
        if let Err(e) = self.to_actor_tx.try_send(ToActor::ReceiverGone {
            topic: self.topic,
            receiver_id: self.receiver_id,
        }) {
            match e {
                mpsc::error::TrySendError::Full(msg) => {
                    // if we can't immediately inform then try to spawn a task that handles it
                    if let Ok(handle) = tokio::runtime::Handle::try_current() {
                        let to_actor_tx = self.to_actor_tx.clone();
                        handle.spawn(async move {
                            let _ = to_actor_tx.send(msg).await;
                        });
                    } else {
                        // full but no runtime oh no
                    }
                }
                mpsc::error::TrySendError::Closed(_) => {
                    // we are probably shutting down so ignore the error
                }
            }
        }
    }
}

/// Input messages for the gossip [`Actor`].
#[derive(derive_more::Debug)]
enum ToActor {
    /// Handle a new QUIC connection, either from accept (external to the actor) or from connect
    /// (happens internally in the actor).
    HandleConnection(PublicKey, ConnOrigin, #[debug("Connection")] Connection),
    Join {
        topic_id: TopicId,
        bootstrap: BTreeSet<NodeId>,
        channels: SubscriberChannels,
    },
    ReceiverGone {
        topic: TopicId,
        receiver_id: ReceiverId,
    },
}

/// Actor that sends and handles messages between the connection and main state loops
struct Actor {
    /// Protocol state
    state: proto::State<PublicKey, StdRng>,
    /// The endpoint through which we dial peers
    endpoint: Endpoint,
    /// Dial machine to connect to peers
    dialer: Dialer,
    /// Input messages to the actor
    to_actor_rx: mpsc::Receiver<ToActor>,
    /// Sender for the state input (cloned into the connection loops)
    in_event_tx: mpsc::Sender<InEvent>,
    /// Input events to the state (emitted from the connection loops)
    in_event_rx: mpsc::Receiver<InEvent>,
    /// Queued timers
    timers: Timers<Timer>,
    /// Map of topics to their state.
    topics: HashMap<TopicId, TopicState>,
    /// Map of peers to their state.
    peers: HashMap<NodeId, PeerState>,
    /// Stream of commands from topic handles.
    command_rx: stream_group::Keyed<TopicCommandStream>,
    /// Internal queue of topic to close because all handles were dropped.
    quit_queue: VecDeque<TopicId>,
    /// Tasks for the connection loops, to keep track of panics.
    connection_tasks: JoinSet<(NodeId, Connection, anyhow::Result<()>)>,
}

impl Actor {
    fn new(
        endpoint: Endpoint,
        config: proto::Config,
        my_addr: &AddrInfo,
    ) -> (Self, mpsc::Sender<ToActor>) {
        let peer_id = endpoint.node_id();
        let dialer = Dialer::new(endpoint.clone());
        let state = proto::State::new(
            peer_id,
            encode_peer_data(my_addr).unwrap(),
            config,
            rand::rngs::StdRng::from_entropy(),
        );
        let (to_actor_tx, to_actor_rx) = mpsc::channel(TO_ACTOR_CAP);
        let (in_event_tx, in_event_rx) = mpsc::channel(IN_EVENT_CAP);

        let actor = Actor {
            endpoint,
            state,
            dialer,
            to_actor_rx,
            in_event_rx,
            in_event_tx,
            timers: Timers::new(),
            command_rx: StreamGroup::new().keyed(),
            peers: Default::default(),
            topics: Default::default(),
            quit_queue: Default::default(),
            connection_tasks: Default::default(),
        };

        (actor, to_actor_tx)
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

        self.handle_addr_update(&current_addresses).await?;
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
        inc!(Metrics, actor_tick_main);
        tokio::select! {
            biased;
            msg = self.to_actor_rx.recv() => {
                trace!(?i, "tick: to_actor_rx");
                inc!(Metrics, actor_tick_rx);
                match msg {
                    Some(msg) => self.handle_to_actor_msg(msg, Instant::now()).await?,
                    None => {
                        debug!("all gossip handles dropped, stop gossip actor");
                        return Ok(None)
                    }
                }
            },
            Some((key, (topic, command))) = self.command_rx.next(), if !self.command_rx.is_empty() => {
                trace!(?i, "tick: command_rx");
                self.handle_command(topic, key, command).await?;
            },
            Some(new_addresses) = direct_addresses_stream.next() => {
                trace!(?i, "tick: new_endpoints");
                inc!(Metrics, actor_tick_endpoint);
                *current_addresses = new_addresses;
                self.handle_addr_update(current_addresses).await?;
            }
            Some(_relay_url) = home_relay_stream.next() => {
                trace!(?i, "tick: new_home_relay");
                self.handle_addr_update(current_addresses).await?;
            }
            (peer_id, res) = self.dialer.next_conn() => {
                trace!(?i, "tick: dialer");
                inc!(Metrics, actor_tick_dialer);
                match res {
                    Some(Ok(conn)) => {
                        debug!(peer = %peer_id.fmt_short(), "dial successful");
                        inc!(Metrics, actor_tick_dialer_success);
                        self.handle_connection(peer_id, ConnOrigin::Dial, conn);
                    }
                    Some(Err(err)) => {
                        warn!(peer = %peer_id.fmt_short(), "dial failed: {err}");
                        inc!(Metrics, actor_tick_dialer_failure);
                    }
                    None => {
                        warn!(peer = %peer_id.fmt_short(), "dial disconnected");
                        inc!(Metrics, actor_tick_dialer_failure);
                    }
                }
            }
            event = self.in_event_rx.recv() => {
                trace!(?i, "tick: in_event_rx");
                inc!(Metrics, actor_tick_in_event_rx);
                let event = event.expect("unreachable: in_event_tx is never dropped before receiver");
                self.handle_in_event(event, Instant::now()).await?;
            }
            drain = self.timers.wait_and_drain() => {
                trace!(?i, "tick: timers");
                inc!(Metrics, actor_tick_timers);
                let now = Instant::now();
                for (_instant, timer) in drain {
                    self.handle_in_event(InEvent::TimerExpired(timer), now).await?;
                }
            }
            Some(res) = self.connection_tasks.join_next(), if !self.connection_tasks.is_empty() => {
                trace!(?i, "tick: connection_tasks");
                let (peer_id, conn, result) = res.expect("connection task panicked");
                self.handle_connection_task_finished(peer_id, conn, result).await?;
            }
        }

        Ok(Some(()))
    }

    async fn handle_addr_update(
        &mut self,
        current_addresses: &BTreeSet<DirectAddr>,
    ) -> Result<(), Error> {
        let peer_data = our_peer_data(&self.endpoint, current_addresses)?;
        self.handle_in_event(InEvent::UpdatePeerData(peer_data), Instant::now())
            .await
    }

    async fn handle_command(
        &mut self,
        topic: TopicId,
        key: stream_group::Key,
        command: Option<Command>,
    ) -> Result<(), Error> {
        debug!(?topic, ?key, ?command, "handle command");
        let Some(state) = self.topics.get_mut(&topic) else {
            // TODO: unreachable?
            warn!("received command for unknown topic");
            return Ok(());
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
                self.handle_in_event(proto::InEvent::Command(topic, command), Instant::now())
                    .await?;
            }
            None => {
                state.command_rx_keys.remove(&key);
                if !state.still_needed() {
                    self.quit_queue.push_back(topic);
                    self.process_quit_queue().await?;
                }
            }
        }
        Ok(())
    }

    fn handle_connection(&mut self, peer_id: NodeId, origin: ConnOrigin, conn: Connection) {
        let (send_tx, send_rx) = mpsc::channel(SEND_QUEUE_CAP);
        let conn_id = conn.stable_id();

        let queue = match self.peers.entry(peer_id) {
            Entry::Occupied(mut entry) => entry.get_mut().accept_conn(send_tx, conn_id),
            Entry::Vacant(entry) => {
                entry.insert(PeerState::Active {
                    active_send_tx: send_tx,
                    active_conn_id: conn_id,
                    other_conns: Vec::new(),
                });
                Vec::new()
            }
        };

        let max_message_size = self.state.max_message_size();
        let in_event_tx = self.in_event_tx.clone();

        // Spawn a task for this connection
        self.connection_tasks.spawn(
            async move {
                let res = connection_loop(
                    peer_id,
                    &conn,
                    origin,
                    send_rx,
                    &in_event_tx,
                    max_message_size,
                    queue,
                )
                .await;
                (peer_id, conn, res)
            }
            .instrument(error_span!("conn", peer = %peer_id.fmt_short())),
        );
    }

    #[tracing::instrument(name = "conn", skip_all, fields(peer = %peer_id.fmt_short()))]
    async fn handle_connection_task_finished(
        &mut self,
        peer_id: NodeId,
        conn: Connection,
        task_result: anyhow::Result<()>,
    ) -> Result<(), Error> {
        if conn.close_reason().is_none() {
            conn.close(0u32.into(), b"close from disconnect");
        }
        let reason = conn.close_reason().expect("just closed");
        let error = task_result.err();
        debug!(%reason, ?error, "connection closed");
        if let Some(PeerState::Active {
            active_conn_id,
            other_conns,
            ..
        }) = self.peers.get_mut(&peer_id)
        {
            if conn.stable_id() == *active_conn_id {
                debug!("active send connection closed, mark peer as disconnected");
                self.handle_in_event(InEvent::PeerDisconnected(peer_id), Instant::now())
                    .await?;
            } else {
                other_conns.retain(|x| *x != conn.stable_id());
                debug!("remaining {} other connections", other_conns.len() + 1);
            }
        } else {
            debug!("peer already marked as disconnected");
        }
        Ok(())
    }

    async fn handle_to_actor_msg(&mut self, msg: ToActor, now: Instant) -> Result<(), Error> {
        trace!("handle to_actor  {msg:?}");
        match msg {
            ToActor::HandleConnection(peer_id, origin, conn) => {
                self.handle_connection(peer_id, origin, conn)
            }
            ToActor::Join {
                topic_id,
                bootstrap,
                channels,
            } => {
                let state = self.topics.entry(topic_id).or_default();
                let TopicState {
                    neighbors,
                    event_senders,
                    command_rx_keys,
                    joined,
                } = state;
                if *joined {
                    let neighbors = neighbors.iter().copied().collect();
                    channels
                        .event_tx
                        .try_send(Ok(Event::Gossip(GossipEvent::Joined(neighbors))))
                        .ok();
                }

                event_senders.push(channels.receiver_id, channels.event_tx);
                let command_rx = TopicCommandStream::new(topic_id, channels.command_rx);
                let key = self.command_rx.insert(command_rx);
                command_rx_keys.insert(key);

                self.handle_in_event(
                    InEvent::Command(
                        topic_id,
                        ProtoCommand::Join(bootstrap.into_iter().collect()),
                    ),
                    now,
                )
                .await?;
            }
            ToActor::ReceiverGone { topic, receiver_id } => {
                self.handle_receiver_gone(topic, receiver_id).await?;
            }
        }
        Ok(())
    }

    async fn handle_in_event(&mut self, event: InEvent, now: Instant) -> Result<(), Error> {
        self.handle_in_event_inner(event, now).await?;
        self.process_quit_queue().await?;
        Ok(())
    }

    async fn process_quit_queue(&mut self) -> Result<(), Error> {
        while let Some(topic_id) = self.quit_queue.pop_front() {
            self.handle_in_event_inner(
                InEvent::Command(topic_id, ProtoCommand::Quit),
                Instant::now(),
            )
            .await?;
            if self.topics.remove(&topic_id).is_some() {
                tracing::debug!(%topic_id, "publishers and subscribers gone; unsubscribing");
            }
        }
        Ok(())
    }

    async fn handle_in_event_inner(&mut self, event: InEvent, now: Instant) -> Result<(), Error> {
        if matches!(event, InEvent::TimerExpired(_)) {
            trace!(?event, "handle in_event");
        } else {
            debug!(?event, "handle in_event");
        };
        let out = self.state.handle(event, now);
        for event in out {
            if matches!(event, OutEvent::ScheduleTimer(_, _)) {
                trace!(?event, "handle out_event");
            } else {
                debug!(?event, "handle out_event");
            };
            match event {
                OutEvent::SendMessage(peer_id, message) => {
                    let state = self.peers.entry(peer_id).or_default();
                    match state {
                        PeerState::Active { active_send_tx, .. } => {
                            if let Err(_err) = active_send_tx.send(message).await {
                                // Removing the peer is handled by the in_event PeerDisconnected sent
                                // in [`Self::handle_connection_task_finished`].
                                warn!(
                                    peer = %peer_id.fmt_short(),
                                    "failed to send: connection task send loop terminated",
                                );
                            }
                        }
                        PeerState::Pending { queue } => {
                            if queue.is_empty() {
                                debug!(peer = %peer_id.fmt_short(), "start to dial");
                                self.dialer.queue_dial(peer_id, GOSSIP_ALPN);
                            }
                            queue.push(message);
                        }
                    }
                }
                OutEvent::EmitEvent(topic_id, event) => {
                    let Some(state) = self.topics.get_mut(&topic_id) else {
                        // TODO: unreachable?
                        warn!(?topic_id, "gossip state emitted event for unknown topic");
                        continue;
                    };
                    let TopicState {
                        joined,
                        neighbors,
                        event_senders,
                        ..
                    } = state;
                    let event = if let ProtoEvent::NeighborUp(neighbor) = event {
                        neighbors.insert(neighbor);
                        if !*joined {
                            *joined = true;
                            GossipEvent::Joined(vec![neighbor])
                        } else {
                            GossipEvent::NeighborUp(neighbor)
                        }
                    } else {
                        event.into()
                    };
                    event_senders.send(&event);
                    if !state.still_needed() {
                        self.quit_queue.push_back(topic_id);
                    }
                }
                OutEvent::ScheduleTimer(delay, timer) => {
                    self.timers.insert(now + delay, timer);
                }
                OutEvent::DisconnectPeer(peer_id) => {
                    // signal disconnection by dropping the senders to the connection
                    debug!(peer=%peer_id.fmt_short(), "gossip state indicates disconnect: drop peer");
                    self.peers.remove(&peer_id);
                }
                OutEvent::PeerData(node_id, data) => match decode_peer_data(&data) {
                    Err(err) => warn!("Failed to decode {data:?} from {node_id}: {err}"),
                    Ok(info) => {
                        debug!(peer = ?node_id, "add known addrs: {info:?}");
                        let node_addr = NodeAddr {
                            node_id,
                            relay_url: info.relay_url,
                            direct_addresses: info.direct_addresses,
                        };
                        if let Err(err) = self
                            .endpoint
                            .add_node_addr_with_source(node_addr, SOURCE_NAME)
                        {
                            debug!(peer = ?node_id, "add known failed: {err:?}");
                        }
                    }
                },
            }
        }
        Ok(())
    }

    async fn handle_receiver_gone(
        &mut self,
        topic: TopicId,
        receiver_id: ReceiverId,
    ) -> Result<(), Error> {
        if let Some(state) = self.topics.get_mut(&topic) {
            state.event_senders.remove(&receiver_id);
            if !state.still_needed() {
                self.quit_queue.push_back(topic);
                self.process_quit_queue().await?;
            }
        } else {
            // topic should not have been dropped without all receivers being dropped first
            warn!(%topic, "receiver gone for missing topic");
        };

        Ok(())
    }
}

type ConnId = usize;

#[derive(Debug)]
enum PeerState {
    Pending {
        queue: Vec<ProtoMessage>,
    },
    Active {
        active_send_tx: mpsc::Sender<ProtoMessage>,
        active_conn_id: ConnId,
        other_conns: Vec<ConnId>,
    },
}

impl PeerState {
    fn accept_conn(
        &mut self,
        send_tx: mpsc::Sender<ProtoMessage>,
        conn_id: ConnId,
    ) -> Vec<ProtoMessage> {
        match self {
            PeerState::Pending { queue } => {
                let queue = std::mem::take(queue);
                *self = PeerState::Active {
                    active_send_tx: send_tx,
                    active_conn_id: conn_id,
                    other_conns: Vec::new(),
                };
                queue
            }
            PeerState::Active {
                active_send_tx,
                active_conn_id,
                other_conns,
            } => {
                // We already have an active connection. We keep the old connection intact,
                // but only use the new connection for sending from now on.
                // By dropping the `send_tx` of the old connection, the send loop part of
                // the `connection_loop` of the old connection will terminate, which will also
                // notify the peer that the old connection may be dropped.
                other_conns.push(*active_conn_id);
                *active_send_tx = send_tx;
                *active_conn_id = conn_id;
                Vec::new()
            }
        }
    }
}

impl Default for PeerState {
    fn default() -> Self {
        PeerState::Pending { queue: Vec::new() }
    }
}

#[derive(Debug, Default)]
struct TopicState {
    joined: bool,
    neighbors: BTreeSet<NodeId>,
    /// Sender side to report events to a [`GossipReceiver`].
    ///
    /// This is the internal counter part of gossip's subscribe public API.
    event_senders: EventSenders,
    /// Keys identifying [`GossipSender`]s.
    ///
    /// This represents the receiver side of gossip's publish public API.
    command_rx_keys: HashSet<stream_group::Key>,
}

impl TopicState {
    /// Check if the topic still has any publisher or subscriber.
    fn still_needed(&self) -> bool {
        !self.event_senders.is_empty() || !self.command_rx_keys.is_empty()
    }
}

/// Whether a connection is initiated by us (Dial) or by the remote peer (Accept)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnOrigin {
    Accept,
    Dial,
}
#[derive(derive_more::Debug)]
struct SubscriberChannels {
    /// Id for the receiver counter part of [`Self::event_tx`].
    receiver_id: ReceiverId,
    event_tx: async_channel::Sender<Result<Event, Error>>,
    #[debug("CommandStream")]
    command_rx: CommandStream,
}

async fn connection_loop(
    from: PublicKey,
    conn: &Connection,
    origin: ConnOrigin,
    mut send_rx: mpsc::Receiver<ProtoMessage>,
    in_event_tx: &mpsc::Sender<InEvent>,
    max_message_size: usize,
    queue: Vec<ProtoMessage>,
) -> anyhow::Result<()> {
    let (mut send, mut recv) = match origin {
        ConnOrigin::Accept => conn.accept_bi().await?,
        ConnOrigin::Dial => conn.open_bi().await?,
    };
    debug!(?origin, "connection established");
    let mut send_buf = BytesMut::new();
    let mut recv_buf = BytesMut::new();

    let send_loop = async {
        for msg in queue {
            write_message(&mut send, &mut send_buf, &msg, max_message_size).await?;
        }
        while let Some(msg) = send_rx.recv().await {
            write_message(&mut send, &mut send_buf, &msg, max_message_size).await?;
        }
        // notify the other node no more data will be sent
        let _ = send.finish();
        // wait for the other node to ack all the sent data
        let _ = send.stopped().await;
        anyhow::Ok(())
    };

    let recv_loop = async {
        loop {
            let msg = read_message(&mut recv, &mut recv_buf, max_message_size).await?;

            match msg {
                None => break,
                Some(msg) => in_event_tx.send(InEvent::RecvMessage(from, msg)).await?,
            }
        }
        anyhow::Ok(())
    };

    let res = tokio::join!(send_loop, recv_loop);
    res.0.context("send_loop").and(res.1.context("recv_loop"))
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

fn encode_peer_data(info: &AddrInfo) -> Result<PeerData, Error> {
    let bytes = postcard::to_stdvec(info)?;
    if bytes.is_empty() {
        return Err(Error::EmptyPeerData);
    }

    Ok(PeerData::new(bytes))
}

fn decode_peer_data(peer_data: &PeerData) -> Result<AddrInfo, Error> {
    let bytes = peer_data.as_bytes();
    if bytes.is_empty() {
        return Ok(AddrInfo::default());
    }
    let info = postcard::from_bytes(bytes)?;
    Ok(info)
}

#[derive(Debug, Default)]
struct EventSenders {
    /// Channels to communicate [`Event`] to [`EventStream`]s.
    ///
    /// This is indexed by receiver id. The boolean indicates a lagged channel ([`Event::Lagged`]).
    senders: HashMap<ReceiverId, (async_channel::Sender<Result<Event, Error>>, bool)>,
}

/// Id for a gossip receiver.
///
/// This is assigned to each [`EventStream`] obtained by the application.
#[derive(derive_more::Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Copy)]
struct ReceiverId(usize);

impl EventSenders {
    fn is_empty(&self) -> bool {
        self.senders.is_empty()
    }

    fn push(&mut self, id: ReceiverId, sender: async_channel::Sender<Result<Event, Error>>) {
        self.senders.insert(id, (sender, false));
    }

    /// Send an event to all subscribers.
    ///
    /// This will not wait until the sink is full, but send a `Lagged` response if the sink is almost full.
    fn send(&mut self, event: &GossipEvent) {
        let mut remove = Vec::new();
        for (&id, (send, lagged)) in self.senders.iter_mut() {
            // If the stream is disconnected, we don't need to send to it.
            if send.is_closed() {
                remove.push(id);
                continue;
            }

            // Check if the send buffer is almost full, and send a lagged response if it is.
            let cap = send.capacity().expect("we only use bounded channels");
            let event = if send.len() >= cap - 1 {
                if *lagged {
                    continue;
                }
                *lagged = true;
                Event::Lagged
            } else {
                *lagged = false;
                Event::Gossip(event.clone())
            };

            if let Err(async_channel::TrySendError::Closed(_)) = send.try_send(Ok(event)) {
                remove.push(id);
            }
        }

        for id in remove.into_iter() {
            self.senders.remove(&id);
        }
    }

    /// Removes a sender based on the corresponding receiver's id.
    fn remove(&mut self, id: &ReceiverId) {
        self.senders.remove(id);
    }
}

#[derive(derive_more::Debug)]
struct TopicCommandStream {
    topic_id: TopicId,
    #[debug("CommandStream")]
    stream: CommandStream,
    closed: bool,
}

impl TopicCommandStream {
    fn new(topic_id: TopicId, stream: CommandStream) -> Self {
        Self {
            topic_id,
            stream,
            closed: false,
        }
    }
}

impl Stream for TopicCommandStream {
    type Item = (TopicId, Option<Command>);
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.closed {
            return Poll::Ready(None);
        }
        match Pin::new(&mut self.stream).poll_next(cx) {
            Poll::Ready(Some(item)) => Poll::Ready(Some((self.topic_id, Some(item)))),
            Poll::Ready(None) => {
                self.closed = true;
                Poll::Ready(Some((self.topic_id, None)))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

fn our_peer_data(
    endpoint: &Endpoint,
    direct_addresses: &BTreeSet<DirectAddr>,
) -> Result<PeerData, Error> {
    encode_peer_data(&AddrInfo {
        relay_url: endpoint.home_relay().get().ok().flatten(),
        direct_addresses: direct_addresses.iter().map(|x| x.addr).collect(),
    })
}

#[derive(Debug)]
struct Dialer {
    endpoint: Endpoint,
    pending: JoinSet<(NodeId, Option<Result<Connection, Error>>)>,
    pending_dials: HashMap<NodeId, CancellationToken>,
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

    /// Starts to dial a node by [`NodeId`].
    fn queue_dial(&mut self, node_id: NodeId, alpn: &'static [u8]) {
        if self.is_pending(node_id) {
            return;
        }
        let cancel = CancellationToken::new();
        self.pending_dials.insert(node_id, cancel.clone());
        let endpoint = self.endpoint.clone();
        self.pending.spawn(async move {
            let res = tokio::select! {
                biased;
                _ = cancel.cancelled() => None,
                res = endpoint.connect(node_id, alpn) => Some(res.map_err(Error::from)),
            };
            (node_id, res)
        });
    }

    /// Checks if a node is currently being dialed.
    fn is_pending(&self, node: NodeId) -> bool {
        self.pending_dials.contains_key(&node)
    }

    /// Waits for the next dial operation to complete.
    /// `None` means disconnected
    async fn next_conn(&mut self) -> (NodeId, Option<Result<Connection, Error>>) {
        match self.pending_dials.is_empty() {
            false => {
                let (node_id, res) = loop {
                    match self.pending.join_next().await {
                        Some(Ok((node_id, res))) => {
                            self.pending_dials.remove(&node_id);
                            break (node_id, res);
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

                (node_id, res)
            }
            true => std::future::pending().await,
        }
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use bytes::Bytes;
    use futures_concurrency::future::TryJoin;
    use iroh::{protocol::Router, RelayMap, RelayMode, SecretKey};
    use rand::Rng;
    use tokio::{spawn, time::timeout};
    use tokio_util::sync::CancellationToken;
    use tracing::{info, instrument};
    use tracing_test::traced_test;

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

            let (actor, to_actor_tx) = Actor::new(endpoint, config, &my_addr);
            let max_message_size = actor.state.max_message_size();

            let _actor_handle =
                AbortOnDropHandle::new(task::spawn(futures_lite::future::pending()));
            let gossip = Self {
                inner: Inner {
                    to_actor_tx,
                    _actor_handle,
                    max_message_size,
                    next_receiver_id: Default::default(),
                }
                .into(),
                #[cfg(feature = "rpc")]
                rpc_handler: Default::default(),
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

    async fn create_endpoint(
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

        let (sink1, _stream1) = sub1.split();

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
                if let Event::Gossip(GossipEvent::Received(msg)) = ev {
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
                if let Event::Gossip(GossipEvent::Received(msg)) = ev {
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
    ///   dropped
    // NOTE: this is a regression test.
    #[tokio::test]
    #[traced_test]
    async fn subscription_cleanup() -> testresult::TestResult {
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
                        Event::Gossip(gm) => match gm {
                            GossipEvent::Received(_) => unreachable!("test does not send messages"),
                            other => tracing::debug!(?other, "gs event"),
                        },
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
        actor.steps(3).await?; // handle our subscribe;
                               // get peer connection;
                               // receive the other peer's information for a NeighborUp
        let state = actor.topics.get(&topic).expect("get registered topic");
        assert!(state.joined);

        // signal the second subscribe, we should remain subscribed
        tx.send(()).await?;
        actor.steps(3).await?; // subscribe; first receiver gone; first sender gone
        let state = actor.topics.get(&topic).expect("get registered topic");
        assert!(state.joined);

        // signal to drop the second handle, the topic should no longer be subscribed
        tx.send(()).await?;
        actor.steps(2).await?; // second receiver gone; second sender gone
        assert!(!actor.topics.contains_key(&topic));

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
    #[tokio::test]
    #[traced_test]
    async fn can_reconnect() -> testresult::TestResult {
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

        let ct2 = ct.child_token();
        // channel used to signal the second gossip instance to advance the test
        let (tx, mut rx) = mpsc::channel::<()>(1);
        let addr1 = NodeAddr::new(node_id1).with_relay_url(relay_url.clone());
        ep2.add_node_addr(addr1)?;
        let go2_task = async move {
            let mut sub = go2.subscribe(topic, Vec::new())?;
            sub.joined().await?;

            rx.recv().await.expect("signal to unsubscribe");
            tracing::info!("unsubscribing");
            drop(sub);

            rx.recv().await.expect("signal to subscribe again");
            tracing::info!("resubscribing");
            let mut sub = go2.subscribe(topic, vec![node_id1])?;

            sub.joined().await?;
            tracing::info!("subscription successful!");

            ct2.cancelled().await;

            anyhow::Ok(())
        }
        .instrument(tracing::debug_span!("node_2", %node_id2));
        let go2_handle = task::spawn(go2_task);

        let addr2 = NodeAddr::new(node_id2).with_relay_url(relay_url);
        ep1.add_node_addr(addr2)?;

        let mut sub = go1.subscribe(topic, vec![node_id2])?;
        // wait for subscribed notification
        sub.joined().await?;

        // signal node_2 to unsubscribe
        tx.send(()).await?;

        // we should receive a Neighbor down event
        let conn_timeout = Duration::from_millis(500);
        let ev = timeout(conn_timeout, sub.try_next()).await??;
        assert_eq!(ev, Some(Event::Gossip(GossipEvent::NeighborDown(node_id2))));
        tracing::info!("node 2 left");

        // signal node_2 to subscribe again
        tx.send(()).await?;

        let conn_timeout = Duration::from_millis(500);
        let ev = timeout(conn_timeout, sub.try_next()).await??;
        assert_eq!(ev, Some(Event::Gossip(GossipEvent::NeighborUp(node_id2))));
        tracing::info!("node 2 rejoined!");

        // cleanup and ensure everything went as expected
        ct.cancel();
        let wait = Duration::from_secs(2);
        timeout(wait, ep1_handle).await???;
        timeout(wait, ep2_handle).await???;
        timeout(wait, go2_handle).await???;

        testresult::TestResult::Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn can_die_and_reconnect() -> testresult::TestResult {
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
                .spawn()
                .await?;
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
            let topic = gossip.subscribe_and_join(topic_id, bootstrap).await?;
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
                    if let Event::Gossip(GossipEvent::Received(message)) = event {
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
