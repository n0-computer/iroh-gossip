//! Networking for the `iroh-gossip` protocol

use std::{
    collections::{hash_map::Entry, BTreeSet, HashMap, HashSet, VecDeque},
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures_concurrency::stream::{stream_group, StreamGroup};
use futures_util::FutureExt as _;
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
    Endpoint, EndpointAddr, EndpointId, PublicKey, RelayUrl, Watcher,
};
use irpc::WithChannels;
use n0_error::{e, stack_error};
use n0_future::{
    task::{self, AbortOnDropHandle, JoinSet},
    time::Instant,
    Stream, StreamExt as _,
};
use rand::{rngs::StdRng, SeedableRng};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, error_span, trace, warn, Instrument};

use self::{
    discovery::GossipDiscovery,
    util::{RecvLoop, SendLoop, Timers},
};
use crate::{
    api::{self, Command, Event, GossipApi, RpcMessage},
    metrics::Metrics,
    proto::{self, HyparviewConfig, PeerData, PlumtreeConfig, Scope, TopicId},
};

mod discovery;
mod util;

/// ALPN protocol name
pub const GOSSIP_ALPN: &[u8] = b"/iroh-gossip/1";

/// Channel capacity for the send queue (one per connection)
const SEND_QUEUE_CAP: usize = 256;
/// Channel capacity for the ToActor message queue (single)
const TO_ACTOR_CAP: usize = 64;
/// Channel capacity for broadcast subscriber event queue (one per topic).
/// This should match or exceed TOPIC_EVENTS_DEFAULT_CAP in api.rs
const TOPIC_EVENT_CAP: usize = 2048;

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
        let metrics = Arc::new(Metrics::default());
        let discovery = GossipDiscovery::default();
        endpoint.discovery().add(discovery.clone());
        let (actor, rpc_tx, local_tx) =
            Actor::new(endpoint, self.config, metrics.clone(), self.alpn, discovery);
        let me = actor.endpoint.id().fmt_short();
        let max_message_size = actor.state.max_message_size();

        let actor_handle = task::spawn(actor.run().instrument(error_span!("gossip", %me)));

        let api = GossipApi::local(rpc_tx);

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
    pub async fn shutdown(&self) -> Result<(), Error> {
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

/// Channel capacity for received gossip messages
const MSG_RX_CAP: usize = 2048;
/// Max messages to drain per event loop tick (prevents starvation of other events)
const MAX_MSGS_PER_TICK: usize = 64;

/// Actor that sends and handles messages between the connection and main state loops
struct Actor {
    /// Protocol state
    state: proto::State<PublicKey, StdRng>,
    /// The endpoint through which we dial peers
    endpoint: Endpoint,
    /// Handle to send dial requests to the dialer task
    dialer: DialerHandle,
    /// Receiver for successful dial connections from dialer task
    dial_success_rx: mpsc::Receiver<(EndpointId, Connection)>,
    /// Input messages to the actor
    rpc_rx: mpsc::Receiver<RpcMessage>,
    local_rx: mpsc::Receiver<LocalActorMessage>,
    /// Sender for the state input (cloned into the connection loops)
    msg_tx: mpsc::Sender<(EndpointId, ProtoMessage)>,
    /// Input events to the state (emitted from the connection loops)
    msg_rx: mpsc::Receiver<(EndpointId, ProtoMessage)>,
    /// Queued timers
    timers: Timers<Timer>,
    /// Map of topics to their state.
    topics: HashMap<TopicId, TopicState>,
    /// Map of peers to their state.
    peers: HashMap<EndpointId, PeerState>,
    /// Stream of commands from topic handles.
    command_rx: stream_group::Keyed<TopicCommandStream>,
    /// Internal queue of topic to close because all handles were dropped.
    quit_queue: VecDeque<TopicId>,
    /// Tasks for the connection loops, to keep track of panics.
    connection_tasks: JoinSet<(EndpointId, Connection, Result<(), ConnectionLoopError>)>,
    metrics: Arc<Metrics>,
    topic_event_forwarders: JoinSet<TopicId>,
    discovery: GossipDiscovery,
    /// Cancellation token for graceful shutdown of background tasks
    cancel: CancellationToken,
}

impl Actor {
    fn new(
        endpoint: Endpoint,
        config: proto::Config,
        metrics: Arc<Metrics>,
        alpn: Option<Bytes>,
        discovery: GossipDiscovery,
    ) -> (
        Self,
        mpsc::Sender<RpcMessage>,
        mpsc::Sender<LocalActorMessage>,
    ) {
        let peer_id = endpoint.id();
        let alpn = alpn.unwrap_or_else(|| GOSSIP_ALPN.to_vec().into());
        let cancel = CancellationToken::new();

        let (dialer, dial_success_rx) = DialerTask::spawn(endpoint.clone(), alpn, cancel.clone());

        let state = proto::State::new(
            peer_id,
            Default::default(),
            config,
            rand::rngs::StdRng::from_rng(&mut rand::rng()),
        );
        let (rpc_tx, rpc_rx) = mpsc::channel(TO_ACTOR_CAP);
        let (local_tx, local_rx) = mpsc::channel(16);
        let (msg_tx, msg_rx) = mpsc::channel(MSG_RX_CAP);

        let actor = Actor {
            endpoint,
            state,
            dialer,
            dial_success_rx,
            rpc_rx,
            msg_tx,
            msg_rx,
            timers: Timers::new(),
            command_rx: StreamGroup::new().keyed(),
            peers: Default::default(),
            topics: Default::default(),
            quit_queue: Default::default(),
            connection_tasks: Default::default(),
            metrics,
            local_rx,
            topic_event_forwarders: Default::default(),
            discovery,
            cancel,
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
    /// None is returned when no further processing should be performed.
    async fn event_loop(
        &mut self,
        addr_updates: &mut (impl Stream<Item = EndpointAddr> + Send + Unpin),
        i: usize,
    ) -> bool {
        self.metrics.actor_tick_main.inc();
        tokio::select! {
            biased;
            conn = self.local_rx.recv() => {
                match conn {
                    Some(LocalActorMessage::Shutdown { reply }) => {
                        debug!("received shutdown message, quit all topics");
                        self.cancel.cancel();
                        self.quit_queue.extend(self.topics.keys().copied());
                        self.process_quit_queue();
                        debug!("all topics quit, stop gossip actor");
                        reply.send(()).ok();
                        return false;
                    },
                    Some(LocalActorMessage::HandleConnection(conn)) => {
                        self.handle_connection(conn.remote_id(), ConnOrigin::Accept, conn);
                    }
                    None => {
                        debug!("all gossip handles dropped, stop gossip actor");
                        self.cancel.cancel();
                        return false;
                    }
                }
            }
            Some((peer_id, msg)) = self.msg_rx.recv() => {
                trace!(?i, "tick: msg_rx");
                self.metrics.actor_tick_in_event_rx.inc();
                let now = Instant::now();
                self.handle_in_event(InEvent::RecvMessage(peer_id, msg), now);
                // Drain additional messages up to limit to reduce context switches
                for _ in 1..MAX_MSGS_PER_TICK {
                    match self.msg_rx.try_recv() {
                        Ok((peer_id, msg)) => {
                            self.handle_in_event(InEvent::RecvMessage(peer_id, msg), now);
                        }
                        Err(_) => break,
                    }
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
                        self.cancel.cancel();
                        return false;
                    }
                }
            },
            _ = self.timers.wait_next() => {
                trace!(?i, "tick: timers");
                self.metrics.actor_tick_timers.inc();
                let now = Instant::now();
                while let Some((_instant, timer)) = self.timers.pop_before(now) {
                    self.handle_in_event(InEvent::TimerExpired(timer), now);
                }
            }
            Some((key, (topic, command))) = self.command_rx.next(), if !self.command_rx.is_empty() => {
                trace!(?i, "tick: command_rx");
                self.handle_command(topic, key, command);
            },
            Some(new_address) = addr_updates.next() => {
                trace!(?i, "tick: new_address");
                self.metrics.actor_tick_endpoint.inc();
                self.handle_addr_update(new_address);
            }
            Some((peer_id, conn)) = self.dial_success_rx.recv() => {
                debug!(peer = %peer_id.fmt_short(), "dial successful");
                self.metrics.actor_tick_dialer.inc();
                self.metrics.actor_tick_dialer_success.inc();
                self.handle_connection(peer_id, ConnOrigin::Dial, conn);
            }
            Some(res) = self.connection_tasks.join_next(), if !self.connection_tasks.is_empty() => {
                trace!(?i, "tick: connection_tasks");
                let (peer_id, conn, result) = res.expect("connection task panicked");
                self.handle_connection_task_finished(peer_id, conn, result).await;
            }
            Some(res) = self.topic_event_forwarders.join_next(), if !self.topic_event_forwarders.is_empty() => {
                let topic_id = res.expect("topic event forwarder panicked");
                if let Some(state) = self.topics.get_mut(&topic_id) {
                    let cmd_keys = state.command_rx_keys.len();
                    let recv_count = state.event_sender.receiver_count();
                    let needed = state.still_needed();
                    debug!(%topic_id, %cmd_keys, %recv_count, %needed, "topic_event_forwarder finished");
                    if !needed {
                        self.quit_queue.push_back(topic_id);
                        self.process_quit_queue();
                    }
                }
            }
        }

        true
    }

    fn handle_addr_update(&mut self, endpoint_addr: EndpointAddr) {
        let peer_data = encode_peer_data(&endpoint_addr.into());
        self.handle_in_event(InEvent::UpdatePeerData(peer_data), Instant::now());
    }

    fn handle_command(&mut self, topic: TopicId, key: stream_group::Key, command: Option<Command>) {
        debug!(?topic, ?key, ?command, "handle command");
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
                state.command_rx_keys.remove(&key);
                let cmd_keys = state.command_rx_keys.len();
                let recv_count = state.event_sender.receiver_count();
                let needed = state.still_needed();
                debug!(%topic, %cmd_keys, %recv_count, %needed, "command stream ended");
                if !needed {
                    self.quit_queue.push_back(topic);
                    self.process_quit_queue();
                }
            }
        }
    }

    fn handle_connection(&mut self, peer_id: EndpointId, origin: ConnOrigin, conn: Connection) {
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
        let msg_tx = self.msg_tx.clone();

        // Spawn a task for this connection
        self.connection_tasks.spawn(
            async move {
                let res = connection_loop(
                    peer_id,
                    conn.clone(),
                    origin,
                    send_rx,
                    msg_tx,
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
        peer_id: EndpointId,
        conn: Connection,
        task_result: Result<(), ConnectionLoopError>,
    ) {
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
                self.handle_in_event(InEvent::PeerDisconnected(peer_id), Instant::now());
            } else {
                other_conns.retain(|x| *x != conn.stable_id());
                debug!("remaining {} other connections", other_conns.len() + 1);
            }
        } else {
            debug!("peer already marked as disconnected");
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
                    neighbors,
                    event_sender,
                    command_rx_keys,
                    ..
                } = self.topics.entry(topic_id).or_default();
                let mut sender_dead = false;
                if !neighbors.is_empty() {
                    for neighbor in neighbors.iter() {
                        if let Err(_err) = tx.try_send(Event::NeighborUp(*neighbor)).await {
                            sender_dead = true;
                            break;
                        }
                    }
                }

                if !sender_dead {
                    let receiver = event_sender.subscribe();
                    let recv_count = event_sender.receiver_count();
                    debug!(%topic_id, %recv_count, "created topic subscriber");
                    let fut = topic_subscriber_loop(tx, receiver).map(move |_| topic_id);
                    self.topic_event_forwarders
                        .spawn(fut.instrument(tracing::Span::current()));
                }
                let command_rx = TopicCommandStream::new(topic_id, Box::pin(rx.into_stream()));
                let key = self.command_rx.insert(command_rx);
                command_rx_keys.insert(key);

                self.handle_in_event(
                    InEvent::Command(
                        topic_id,
                        ProtoCommand::Join(bootstrap.into_iter().collect()),
                    ),
                    now,
                );
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
                    let state = self.peers.entry(peer_id).or_default();
                    match state {
                        PeerState::Active { active_send_tx, .. } => {
                            use tokio::sync::mpsc::error::TrySendError;
                            match active_send_tx.try_send(message) {
                                Ok(()) => {}
                                Err(TrySendError::Full(_)) => {
                                    debug!(peer = %peer_id.fmt_short(), "send queue full, dropping message");
                                }
                                Err(TrySendError::Closed(_)) => {
                                    // Removing the peer is handled by the in_event PeerDisconnected sent
                                    // in [`Self::handle_connection_task_finished`].
                                    warn!(
                                        peer = %peer_id.fmt_short(),
                                        "failed to send: connection task send loop terminated",
                                    );
                                }
                            }
                        }
                        PeerState::Pending { queue } => {
                            if queue.is_empty() {
                                debug!(peer = %peer_id.fmt_short(), "start to dial");
                                self.dialer.queue_dial(peer_id);
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
                        neighbors,
                        event_sender,
                        ..
                    } = state;
                    match &event {
                        ProtoEvent::NeighborUp(neighbor) => {
                            neighbors.insert(*neighbor);
                        }
                        ProtoEvent::NeighborDown(neighbor) => {
                            neighbors.remove(neighbor);
                        }
                        _ => {}
                    }
                    match event_sender.send(event) {
                        Ok(0) => {
                            debug!(%topic_id, "event sent but no receivers");
                        }
                        Ok(_n) => {}
                        Err(err) => {
                            debug!(%topic_id, ?err, "event dropped: send failed");
                        }
                    }

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

                        self.discovery.add(endpoint_addr);
                    }
                },
            }
        }
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

#[derive(Debug)]
struct TopicState {
    neighbors: BTreeSet<EndpointId>,
    event_sender: broadcast::Sender<ProtoEvent>,
    /// Keys identifying command receivers in [`Actor::command_rx`].
    ///
    /// This represents the receiver side of gossip's publish public API.
    command_rx_keys: HashSet<stream_group::Key>,
}

impl Default for TopicState {
    fn default() -> Self {
        let (event_sender, _) = broadcast::channel(TOPIC_EVENT_CAP);
        Self {
            neighbors: Default::default(),
            command_rx_keys: Default::default(),
            event_sender,
        }
    }
}

impl TopicState {
    /// Check if the topic still has any publisher or subscriber.
    fn still_needed(&self) -> bool {
        // Keep topic alive if either senders or receivers exist.
        !self.command_rx_keys.is_empty() || self.event_sender.receiver_count() > 0
    }

    #[cfg(test)]
    fn joined(&self) -> bool {
        !self.neighbors.is_empty()
    }
}

/// Whether a connection is initiated by us (Dial) or by the remote peer (Accept)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnOrigin {
    Accept,
    Dial,
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
    Read {
        source: self::util::ReadError,
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

async fn connection_loop(
    from: PublicKey,
    conn: Connection,
    origin: ConnOrigin,
    send_rx: mpsc::Receiver<ProtoMessage>,
    msg_tx: mpsc::Sender<(EndpointId, ProtoMessage)>,
    max_message_size: usize,
    queue: Vec<ProtoMessage>,
) -> Result<(), ConnectionLoopError> {
    debug!(?origin, "connection established");

    let mut send_loop = SendLoop::new(conn.clone(), send_rx, max_message_size);
    let mut recv_loop = RecvLoop::new(from, conn, msg_tx, max_message_size);

    let send_fut = send_loop.run(queue).instrument(error_span!("send"));
    let recv_fut = recv_loop.run().instrument(error_span!("recv"));

    let (send_res, recv_res) = tokio::join!(send_fut, recv_fut);
    send_res?;
    recv_res?;
    Ok(())
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
                    Err(broadcast::error::RecvError::Closed) => {
                        debug!("topic_subscriber_loop: broadcast closed");
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        debug!(n, "topic_subscriber_loop: lagged, missed messages");
                        Event::Lagged
                    }
                    Ok(event) => event.into(),
                };
                if sender.send(event).await.is_err() {
                    debug!("topic_subscriber_loop: send failed, receiver closed");
                    break;
                }
            }
            _ = sender.closed() => {
                debug!("topic_subscriber_loop: receiver closed");
                break;
            }
        }
    }
}

/// A stream of commands for a gossip subscription.
type BoxedCommandReceiver =
    n0_future::stream::Boxed<Result<Command, irpc::channel::mpsc::RecvError>>;

#[derive(derive_more::Debug)]
struct TopicCommandStream {
    topic_id: TopicId,
    #[debug("CommandStream")]
    stream: BoxedCommandReceiver,
    closed: bool,
}

impl TopicCommandStream {
    fn new(topic_id: TopicId, stream: BoxedCommandReceiver) -> Self {
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
            Poll::Ready(Some(Ok(item))) => Poll::Ready(Some((self.topic_id, Some(item)))),
            Poll::Ready(None) => {
                debug!(topic_id = %self.topic_id, "command stream closed: sender dropped");
                self.closed = true;
                Poll::Ready(Some((self.topic_id, None)))
            }
            Poll::Ready(Some(Err(err))) => {
                warn!(topic_id = %self.topic_id, ?err, "command stream error");
                self.closed = true;
                Poll::Ready(Some((self.topic_id, None)))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Maximum concurrent dial attempts - needs to be high enough to establish mesh quickly
const MAX_CONCURRENT_DIALS: usize = 8;
/// Maximum retry attempts for failed dials
const MAX_DIAL_RETRIES: usize = 2;
/// Delay between dial retries
const DIAL_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(300);
/// Dial timeout
const DIAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
/// Channel capacity for dial requests
const DIAL_REQUEST_CAP: usize = 64;

#[derive(Debug, Clone)]
struct DialerHandle {
    request_tx: mpsc::Sender<EndpointId>,
}

impl DialerHandle {
    fn queue_dial(&self, endpoint_id: EndpointId) {
        // Non-blocking send - if channel is full, dial request is dropped
        if self.request_tx.try_send(endpoint_id).is_err() {
            trace!(peer = %endpoint_id.fmt_short(), "dial request dropped (queue full)");
        }
    }
}

/// Dialer runs as a separate task, completely isolated from main actor
struct DialerTask {
    endpoint: Endpoint,
    alpn: Bytes,
    request_rx: mpsc::Receiver<EndpointId>,
    success_tx: mpsc::Sender<(EndpointId, Connection)>,
    pending: JoinSet<(EndpointId, Option<Connection>)>,
    pending_dials: HashMap<EndpointId, CancellationToken>,
    cancel: CancellationToken,
}

impl DialerTask {
    /// Create a new dialer for a [`Endpoint`]
    fn spawn(
        endpoint: Endpoint,
        alpn: Bytes,
        cancel: CancellationToken,
    ) -> (DialerHandle, mpsc::Receiver<(EndpointId, Connection)>) {
        let (request_tx, request_rx) = mpsc::channel(DIAL_REQUEST_CAP);
        let (success_tx, success_rx) = mpsc::channel(32);

        let task = Self {
            endpoint,
            alpn,
            request_rx,
            success_tx,
            pending: Default::default(),
            pending_dials: Default::default(),
            cancel,
        };

        tokio::spawn(task.run().instrument(error_span!("dialer")));

        (DialerHandle { request_tx }, success_rx)
    }

    async fn run(mut self) {
        loop {
            tokio::select! {
                biased;
                _ = self.cancel.cancelled() => {
                    debug!("dialer shutting down");
                    break;
                }
                Some(endpoint_id) = self.request_rx.recv() => {
                    self.start_dial(endpoint_id);
                }
                Some(result) = self.pending.join_next(), if !self.pending.is_empty() => {
                    match result {
                        Ok((endpoint_id, Some(conn))) => {
                            self.pending_dials.remove(&endpoint_id);
                            if self.success_tx.send((endpoint_id, conn)).await.is_err() {
                                debug!("dialer success channel closed");
                                break;
                            }
                        }
                        Ok((endpoint_id, None)) => {
                            self.pending_dials.remove(&endpoint_id);
                        }
                        Err(e) => {
                            error!("dial task panicked: {:?}", e);
                        }
                    }
                }
            }
        }
    }

    /// Starts to dial a endpoint by [`EndpointId`].
    fn start_dial(&mut self, endpoint_id: EndpointId) {
        if self.pending_dials.contains_key(&endpoint_id) {
            return;
        }

        let delay = if self.pending_dials.len() >= MAX_CONCURRENT_DIALS {
            std::time::Duration::from_secs(1)
        } else {
            std::time::Duration::ZERO
        };

        let cancel = CancellationToken::new();
        self.pending_dials.insert(endpoint_id, cancel.clone());
        let endpoint = self.endpoint.clone();
        let alpn = self.alpn.clone();
        let parent_cancel = self.cancel.clone();

        self.pending.spawn(async move {
            if !delay.is_zero() {
                n0_future::time::sleep(delay).await;
            }

            for attempt in 0..MAX_DIAL_RETRIES {
                if cancel.is_cancelled() || parent_cancel.is_cancelled() {
                    return (endpoint_id, None);
                }

                let res = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return (endpoint_id, None),
                    _ = parent_cancel.cancelled() => return (endpoint_id, None),
                    res = n0_future::time::timeout(DIAL_TIMEOUT, endpoint.connect(endpoint_id, &alpn)) => res,
                };

                match res {
                    Ok(Ok(conn)) => {
                        debug!(peer = %endpoint_id.fmt_short(), "dial successful");
                        return (endpoint_id, Some(conn));
                    }
                    Ok(Err(err)) => {
                        if attempt < MAX_DIAL_RETRIES - 1 {
                            trace!(peer = %endpoint_id.fmt_short(), attempt, "dial failed, retrying: {err}");
                            n0_future::time::sleep(DIAL_RETRY_DELAY).await;
                        } else {
                            debug!(peer = %endpoint_id.fmt_short(), "dial failed after retries: {err}");
                        }
                    }
                    Err(_elapsed) => {
                        if attempt < MAX_DIAL_RETRIES - 1 {
                            trace!(peer = %endpoint_id.fmt_short(), attempt, "dial timeout, retrying");
                            n0_future::time::sleep(DIAL_RETRY_DELAY).await;
                        } else {
                            debug!(peer = %endpoint_id.fmt_short(), "dial timeout after retries");
                        }
                    }
                }
            }
            (endpoint_id, None)
        });
    }
}

#[cfg(test)]
pub(crate) mod test {
    use std::time::Duration;

    use bytes::Bytes;
    use futures_concurrency::future::TryJoin;
    use iroh::{
        discovery::static_provider::StaticProvider, endpoint::BindError, protocol::Router,
        EndpointAddr, RelayMap, RelayMode, SecretKey,
    };
    use n0_error::{AnyError, Result, StdResultExt};
    use rand::{CryptoRng, Rng};
    use tokio::{spawn, time::timeout};
    use tokio_util::sync::CancellationToken;
    use tracing::{info, instrument};
    use tracing_test::traced_test;

    use super::*;
    use crate::api::{ApiError, GossipReceiver, GossipSender, GossipTopic};

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
            let addr_update_stream = &mut futures_lite::stream::pending();
            actor.event_loop(addr_update_stream, *step).await
        }

        async fn steps(&mut self, n: usize) {
            for _ in 0..n {
                self.step().await;
            }
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
            rng: &mut rand_chacha::ChaCha12Rng,
            config: proto::Config,
            relay_map: RelayMap,
            cancel: &CancellationToken,
        ) -> Result<(Self, Actor, EndpointHandle), BindError> {
            let endpoint = create_endpoint(rng, relay_map, None).await?;
            let metrics = Arc::new(Metrics::default());
            let discovery = GossipDiscovery::default();
            endpoint.discovery().add(discovery.clone());

            let (actor, to_actor_tx, conn_tx) =
                Actor::new(endpoint, config, metrics.clone(), None, discovery);
            let max_message_size = actor.state.max_message_size();

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

            let endpoint_task = task::spawn(endpoint_loop(
                actor.endpoint.clone(),
                gossip.clone(),
                cancel.child_token(),
            ));

            Ok((gossip, actor, endpoint_task))
        }

        /// Crates a new testing gossip instance with the normal actor loop.
        async fn t_new(
            rng: &mut rand_chacha::ChaCha12Rng,
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
        rng: &mut rand_chacha::ChaCha12Rng,
        relay_map: RelayMap,
        static_provider: Option<StaticProvider>,
    ) -> Result<Endpoint, BindError> {
        let ep = Endpoint::empty_builder(RelayMode::Custom(relay_map))
            .secret_key(SecretKey::generate(rng))
            .alpns(vec![GOSSIP_ALPN.to_vec()])
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
        let mut pending_conns = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => break,
                // Handle completed connection handshakes
                Some(res) = pending_conns.join_next(), if !pending_conns.is_empty() => {
                    match res {
                        Ok(Ok(conn)) => {
                            if let Err(e) = gossip.handle_connection(conn).await {
                                debug!("handle_connection error: {e:#}");
                            }
                        }
                        Ok(Err(e)) => debug!("connection handshake failed: {e:#}"),
                        Err(e) => debug!("connection task panicked: {e:#}"),
                    }
                }
                // Accept new incoming connections
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
                        // Spawn handshake so we can continue accepting
                        pending_conns.spawn(async move {
                            connecting.await.std_context("await incoming connection")
                        });
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
        static_provider.add_endpoint_info(addr1.clone());
        static_provider.add_endpoint_info(addr2.clone());

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
    /// - Subscribe both endpoints to the same topic. The first endpoint will subscribe twice and connect
    ///   to the second endpoint. The second endpoint will subscribe without bootstrap.
    /// - Ensure that the first endpoint removes the subscription iff all topic handles have been
    ///   dropped
    // NOTE: this is a regression test.
    #[tokio::test]
    #[traced_test]
    async fn subscription_cleanup() -> Result {
        let rng = &mut rand_chacha::ChaCha12Rng::seed_from_u64(1);
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
        let static_provider = StaticProvider::new();
        static_provider.add_endpoint_info(addr2);
        actor.endpoint.discovery().add(static_provider);
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
        actor.steps(3).await; // handle our subscribe;
                              // get peer connection;
                              // receive the other peer's information for a NeighborUp
        let state = actor.topics.get(&topic).expect("get registered topic");
        assert!(state.joined());

        // signal the second subscribe, we should remain subscribed
        tx.send(())
            .await
            .std_context("signal additional subscribe")?;
        actor.steps(3).await; // subscribe; first receiver gone; first sender gone
        let state = actor.topics.get(&topic).expect("get registered topic");
        assert!(state.joined());

        // signal to drop the second handle, the topic should no longer be subscribed
        tx.send(()).await.std_context("signal drop handles")?;
        actor.steps(2).await; // second receiver gone; second sender gone
        assert!(!actor.topics.contains_key(&topic));

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

    /// Test that endpoints can reconnect to each other.
    ///
    /// This test will create two endpoints subscribed to the same topic. The second endpoint will
    /// unsubscribe and then resubscribe and connection between the endpoints should succeed both
    /// times.
    // NOTE: This is a regression test
    #[tokio::test]
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

        let ct2 = ct.child_token();
        // channel used to signal the second gossip instance to advance the test
        let (tx, mut rx) = mpsc::channel::<()>(1);
        let addr1 = EndpointAddr::new(endpoint_id1).with_relay_url(relay_url.clone());
        let static_provider = StaticProvider::new();
        static_provider.add_endpoint_info(addr1);
        ep2.discovery().add(static_provider.clone());
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
        static_provider.add_endpoint_info(addr2);
        ep1.discovery().add(static_provider);

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
            let ep = Endpoint::empty_builder(RelayMode::Custom(relay_map))
                .secret_key(secret_key)
                .insecure_skip_relay_cert_verify(true)
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
            let static_provider = StaticProvider::new();
            static_provider.add_endpoint_info(bootstrap_addr);
            router.endpoint().discovery().add(static_provider);
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
        let static_provider = StaticProvider::new();
        static_provider.add_endpoint_info(addr1);
        router2.endpoint().discovery().add(static_provider);

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
    async fn gossip_rely_on_gossip_discovery() -> n0_error::Result<()> {
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

        // spawn 3 endpoints without relay or discovery
        let (n1, r1, _g1, _tx1, mut rx1) = spawn(rng).await?;
        let (n2, r2, _g2, tx2, mut rx2) = spawn(rng).await?;
        let (n3, r3, _g3, tx3, mut rx3) = spawn(rng).await?;

        println!("endpoints {:?}", [n1, n2, n3]);

        // create a static discovery that has only endpoint 1 addr info set
        let addr1 = r1.endpoint().addr();
        let disco = StaticProvider::new();
        disco.add_endpoint_info(addr1);

        // add addr info of endpoint1 to endpoint2 and join endpoint1
        r2.endpoint().discovery().add(disco.clone());
        tx2.join_peers(vec![n1]).await?;

        // await join endpoint2 -> nodde1
        timeout(Duration::from_secs(3), rx1.joined())
            .await
            .std_context("wait rx1 join")??;
        timeout(Duration::from_secs(3), rx2.joined())
            .await
            .std_context("wait rx2 join")??;

        // add addr info of endpoint1 to endpoint3 and join endpoint1
        r3.endpoint().discovery().add(disco.clone());
        tx3.join_peers(vec![n1]).await?;

        // await join at endpoint3: n1 and n2
        // n2 only works because because we use gossip discovery!
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
        let static_provider = StaticProvider::new();
        static_provider.add_endpoint_info(addr1);
        router2.endpoint().discovery().add(static_provider);

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

        // Node 1 drops its sender
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

    struct TestMesh {
        endpoints: Vec<Endpoint>,
        subs: Vec<GossipTopic>,
        cancel: CancellationToken,
        topic: TopicId,
        tasks: Vec<n0_future::task::JoinHandle<Result<(), AnyError>>>,
        rng: rand_chacha::ChaCha12Rng,
        relay_map: iroh::RelayMap,
        relay_url: iroh::RelayUrl,
        static_provider: StaticProvider,
    }

    impl TestMesh {
        async fn new(nodes: usize, topic_byte: u8) -> Result<Self> {
            let mut rng = rand_chacha::ChaCha12Rng::seed_from_u64(1);
            let (relay_map, relay_url, _relay_guard) =
                iroh::test_utils::run_relay_server().await.unwrap();
            let static_provider = StaticProvider::new();
            let cancel = CancellationToken::new();

            let mut endpoints = Vec::new();
            for _ in 0..nodes {
                let ep =
                    create_endpoint(&mut rng, relay_map.clone(), Some(static_provider.clone()))
                        .await?;
                endpoints.push(ep);
            }

            for ep in &endpoints {
                let addr = EndpointAddr::new(ep.id()).with_relay_url(relay_url.clone());
                static_provider.add_endpoint_info(addr);
            }

            let mut gossips = Vec::new();
            let mut tasks = Vec::new();
            for ep in &endpoints {
                let go = Gossip::builder().spawn(ep.clone());
                tasks.push(spawn(endpoint_loop(ep.clone(), go.clone(), cancel.clone())));
                gossips.push(go);
            }

            let topic = TopicId::from_bytes([topic_byte; 32]);

            let mut sub_futures = Vec::new();
            for (i, go) in gossips.iter().enumerate() {
                let bootstrap = if i == 0 {
                    vec![]
                } else {
                    vec![endpoints[0].id()]
                };
                sub_futures.push(go.subscribe_and_join(topic, bootstrap));
            }
            let subs: Vec<_> = futures_concurrency::future::TryJoin::try_join(sub_futures).await?;

            // Allow mesh to stabilize - nodes need time for HyParView/PlumTree formation
            tokio::time::sleep(Duration::from_millis(500)).await;

            std::mem::forget(_relay_guard);

            Ok(Self {
                endpoints,
                subs,
                cancel,
                topic,
                tasks,
                rng,
                relay_map,
                relay_url,
                static_provider,
            })
        }

        async fn add_node(&mut self) -> Result<GossipTopic> {
            let ep = create_endpoint(
                &mut self.rng,
                self.relay_map.clone(),
                Some(self.static_provider.clone()),
            )
            .await?;
            let addr = EndpointAddr::new(ep.id()).with_relay_url(self.relay_url.clone());
            self.static_provider.add_endpoint_info(addr);

            let go = Gossip::builder().spawn(ep.clone());
            self.tasks.push(spawn(endpoint_loop(
                ep.clone(),
                go.clone(),
                self.cancel.clone(),
            )));

            let sub = go
                .subscribe_and_join(self.topic, vec![self.endpoints[0].id()])
                .await?;
            self.endpoints.push(ep);
            info!("New node joined mesh");
            Ok(sub)
        }

        fn start_connection_flood(&self, sleep_interval: Duration) {
            let n = self.endpoints.len();
            for (i, j) in (0..n).flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j)))
            {
                let ep = self.endpoints[i].clone();
                let target = self.endpoints[j].id();
                let cancel = self.cancel.clone();
                spawn(async move {
                    while !cancel.is_cancelled() {
                        let _ = ep.connect(target, b"flood").await;
                        tokio::time::sleep(sleep_interval).await;
                    }
                });
            }
            info!("Started {} connection stress tasks", n * (n - 1));
        }

        async fn shutdown(self) {
            self.cancel.cancel();
            for task in self.tasks {
                task.await.ok();
            }
        }

        fn drain_as_senders(
            &mut self,
        ) -> (Vec<GossipSender>, Vec<n0_future::task::JoinHandle<()>>) {
            self.subs
                .drain(..)
                .map(|s| {
                    let (tx, mut rx) = s.split();
                    let drain = spawn(async move { while rx.next().await.is_some() {} });
                    (tx, drain)
                })
                .unzip()
        }

        async fn run_delivery_test(
            &self,
            senders: Vec<GossipSender>,
            mut receivers: Vec<GossipReceiver>,
            cfg: DeliveryTestConfig,
        ) -> Result<()> {
            let num_receivers = receivers.len();
            let expected_per_receiver = cfg.expected_per_receiver;
            let msg_delay = cfg.msg_delay;
            let msgs_per_node = cfg.msgs_per_node;

            let send_handles: Vec<_> = senders
                .iter()
                .enumerate()
                .map(|(idx, tx)| {
                    let tx = tx.clone();
                    spawn(async move {
                        for i in 0..msgs_per_node {
                            let content = format!("n{idx}m{i}");
                            let _ = tx.broadcast(content.into_bytes().into()).await;
                            tokio::time::sleep(msg_delay).await;
                        }
                    })
                })
                .collect();

            let recv_deadline = cfg.recv_deadline;
            let recv_handles: Vec<_> = receivers
                .drain(..)
                .map(|mut rx| {
                    spawn(async move {
                        let mut count = 0usize;
                        let deadline = n0_future::time::Instant::now() + recv_deadline;
                        while n0_future::time::Instant::now() < deadline {
                            match timeout(Duration::from_millis(100), rx.next()).await {
                                Ok(Some(Ok(Event::Received(_)))) => {
                                    count += 1;
                                    if count >= expected_per_receiver {
                                        break;
                                    }
                                }
                                Ok(Some(Ok(_))) => {}
                                Ok(Some(Err(_))) | Ok(None) => break,
                                Err(_) => {}
                            }
                        }
                        count
                    })
                })
                .collect();

            for h in send_handles {
                let _ = h.await;
            }
            tokio::time::sleep(cfg.delivery_wait).await;
            drop(senders);

            let mut received_counts = Vec::new();
            for h in recv_handles {
                received_counts.push(h.await.std_context("recv task")?);
            }

            self.cancel.cancel();

            let total_received: usize = received_counts.iter().sum();
            let total_expected = num_receivers * expected_per_receiver;
            let threshold = (total_expected * cfg.threshold_percent) / 100;

            info!(
                total_received,
                total_expected, threshold, "Delivery results"
            );

            assert!(
                total_received >= threshold,
                "Delivery failed: {total_received}/{threshold} (expected {total_expected})"
            );

            Ok(())
        }
    }

    struct DeliveryTestConfig {
        msgs_per_node: usize,
        expected_per_receiver: usize,
        msg_delay: Duration,
        recv_deadline: Duration,
        delivery_wait: Duration,
        threshold_percent: usize,
    }

    #[tokio::test]
    #[traced_test]
    async fn test_broadcast_4_nodes() -> Result {
        let mut mesh = TestMesh::new(4, 2).await?;

        let (senders, receivers): (Vec<_>, Vec<_>) = mesh.subs.drain(..).map(|s| s.split()).unzip();

        let cfg = DeliveryTestConfig {
            msgs_per_node: 50,
            expected_per_receiver: 3 * 50,
            msg_delay: Duration::from_millis(5),
            recv_deadline: Duration::from_secs(10),
            delivery_wait: Duration::from_millis(200),
            threshold_percent: 95,
        };

        mesh.run_delivery_test(senders, receivers, cfg).await?;
        mesh.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn test_broadcast_10_nodes() -> Result {
        let mut mesh = TestMesh::new(10, 47).await?;

        let (senders, receivers): (Vec<_>, Vec<_>) = mesh.subs.drain(..).map(|s| s.split()).unzip();

        let cfg = DeliveryTestConfig {
            msgs_per_node: 20,
            expected_per_receiver: 9 * 20,
            msg_delay: Duration::from_millis(5),
            recv_deadline: Duration::from_secs(30),
            delivery_wait: Duration::from_millis(200),
            threshold_percent: 95,
        };

        mesh.run_delivery_test(senders, receivers, cfg).await?;
        mesh.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn test_late_joiner_with_connection_stress() -> Result {
        let mut mesh = TestMesh::new(20, 9).await?;
        mesh.start_connection_flood(Duration::from_millis(50));
        let sub_late = mesh.add_node().await?;
        tokio::time::sleep(Duration::from_millis(200)).await;

        let (senders, drains) = mesh.drain_as_senders();
        let (tx_late, rx_late) = sub_late.split();

        let cfg = DeliveryTestConfig {
            msgs_per_node: 50,
            expected_per_receiver: senders.len() * 50,
            msg_delay: Duration::from_millis(1),
            // reduce to 20s to get flakiness due to the late joiner having a hard time catching up
            recv_deadline: Duration::from_secs(60),
            delivery_wait: Duration::from_millis(200),
            threshold_percent: 95,
        };

        mesh.run_delivery_test(senders, vec![rx_late], cfg).await?;
        drop(tx_late);
        drop(drains);
        mesh.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn test_late_joiner() -> Result {
        let mut mesh = TestMesh::new(20, 10).await?;
        let sub_late = mesh.add_node().await?;
        tokio::time::sleep(Duration::from_millis(200)).await;

        let (senders, drains) = mesh.drain_as_senders();
        let (tx_late, rx_late) = sub_late.split();

        let cfg = DeliveryTestConfig {
            msgs_per_node: 50,
            expected_per_receiver: senders.len() * 50,
            msg_delay: Duration::from_millis(1),
            recv_deadline: Duration::from_secs(30),
            delivery_wait: Duration::from_millis(200),
            threshold_percent: 95,
        };

        mesh.run_delivery_test(senders, vec![rx_late], cfg).await?;
        drop(tx_late);
        drop(drains);
        mesh.shutdown().await;
        Ok(())
    }
}
