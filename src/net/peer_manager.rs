//! Peer connection management for gossip networking.
//!
//! This module provides [`PeerManager`], which handles the lifecycle of peer connections
//! for the gossip protocol. It manages dialing to peers, accepting incoming connections,
//! and opening topic-specific streams on those connections.

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use bytes::Bytes;
use iroh::{
    endpoint::{ConnectError, Connection},
    Endpoint, EndpointId,
};
use irpc::channel;
use n0_future::{stream::Boxed as BoxStream, MergeUnbounded, StreamExt};
use tokio::task::JoinSet;
use tracing::{debug, error_span, instrument, warn, Instrument};

use super::{
    dialer::Dialer,
    util::{accept_stream, CloseGuard, Guarded, IrohRemoteConnection},
    ProtoMessage,
};
use crate::proto::TopicId;

/// Direction of a connection (who initiated it).
#[derive(Debug, Copy, Clone)]
pub(crate) enum Direction {
    /// We dialed the remote.
    Dial,
    /// The remote connected to us.
    Accept,
}

/// State for a connected remote peer.
#[derive(Clone)]
struct ConnectionState {
    endpoint_id: EndpointId,
    conn_id: usize,
    client: irpc::Client<net_proto::Request>,
    #[allow(dead_code)]
    direction: Direction,
    guard: CloseGuard,
}

impl ConnectionState {
    fn new(endpoint_id: EndpointId, connection: Connection, direction: Direction) -> Self {
        let conn_id = connection.stable_id();
        let irpc_conn = IrohRemoteConnection::new(connection);
        let client = irpc::Client::boxed(irpc_conn);
        let guard = CloseGuard::new();
        ConnectionState {
            client,
            direction,
            conn_id,
            guard,
            endpoint_id,
        }
    }

    fn same_connection(&self, conn: &Connection) -> bool {
        self.conn_id == conn.stable_id()
    }
}

/// Network protocol messages for gossip.
pub(crate) mod net_proto {
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
        #[rpc(tx=mpsc::Sender<super::super::ProtoMessage>, rx=mpsc::Receiver<super::super::ProtoMessage>)]
        Join(JoinRequest),
    }
}

pub(crate) use self::net_proto::GossipMessage;

/// Stream of incoming topic join requests from all connected peers.
// TODO(frando): This uses MergeUnbounded which grows as connections are added.
// Streams from closed connections will error out but remain in the merge until polled.
// Consider using a bounded alternative or periodically cleaning up errored streams.
pub(crate) type IncomingRequestsStream = MergeUnbounded<
    BoxStream<(
        EndpointId,
        std::io::Result<Guarded<net_proto::GossipMessage>>,
    )>,
>;

/// Events emitted by the peer manager.
#[derive(derive_more::Debug)]
pub(crate) enum PeerManagerEvent {
    /// A topic connection is ready (either from dialing or accepting).
    TopicReady {
        #[debug("{}", remote.fmt_short())]
        remote: EndpointId,
        #[debug("{}", remote.fmt_short())]
        topic: TopicId,
        #[debug(skip)]
        channels: TopicChannels,
    },
    /// Connecting to a remote for a topic failed.
    ///
    /// Note: This is emitted when the dial fails or when opening a topic stream fails.
    /// The underlying connection may still be valid for other topics.
    ConnectFailed {
        /// The remote peer that failed to connect.
        #[debug("{}", remote.fmt_short())]
        remote: EndpointId,
        /// The topics that were waiting for this connection.
        #[debug("{:?}", topics.iter().map(|x| x.fmt_short()).collect::<Vec<_>>())]
        topics: HashSet<TopicId>,
    },
}

type TopicRequestResult = (EndpointId, TopicId, irpc::Result<TopicChannels>);

#[derive(Debug)]
pub(crate) struct TopicChannels {
    /// Sender for messages to the remote.
    pub(crate) tx: Guarded<channel::mpsc::Sender<ProtoMessage>>,
    /// Receiver for messages to the remote.
    pub(crate) rx: Guarded<channel::mpsc::Receiver<ProtoMessage>>,
}

/// Manages peer connections for the gossip protocol.
///
/// This struct handles dialing to peers, accepting incoming connections,
/// and managing the lifecycle of connections. It is not an actor itself,
/// but is driven by the main gossip actor via [`connect_topic`](Self::connect_topic)
/// and [`poll_next`](Self::poll_next).
pub(crate) struct PeerManager {
    pub(super) endpoint: Endpoint,
    alpn: Bytes,
    dialer: Dialer,
    /// Topics waiting for a dial to complete, keyed by remote endpoint.
    requested_topics: HashMap<EndpointId, HashSet<TopicId>>,
    /// Active connections, keyed by remote endpoint.
    // TODO(frando): When a new connection arrives for an existing remote, we overwrite
    // the old state. This could leave orphaned streams in `incoming_topic_requests`.
    // Consider explicitly closing the old connection or tracking multiple connections per remote.
    connections: HashMap<EndpointId, ConnectionState>,
    /// Tasks waiting for connections to close (for cleanup).
    connection_close_tasks: JoinSet<(EndpointId, Connection)>,
    /// Merged stream of incoming topic join requests from all connections.
    incoming_topic_requests: IncomingRequestsStream,
    /// Tasks for opening topic streams on already-connected remotes.
    pending_topic_requests: JoinSet<TopicRequestResult>,
}

impl PeerManager {
    /// Creates a new peer manager.
    pub(crate) fn new(endpoint: Endpoint, alpn: Bytes) -> Self {
        Self {
            endpoint,
            alpn,
            dialer: Dialer::default(),
            requested_topics: Default::default(),
            connections: Default::default(),
            connection_close_tasks: JoinSet::new(),
            incoming_topic_requests: Default::default(),
            pending_topic_requests: JoinSet::new(),
        }
    }

    /// Request to connect to a remote peer for a specific topic.
    ///
    /// If already connected to the peer, this will open a new topic stream.
    /// Otherwise, it will initiate a dial to the peer.
    ///
    /// The result will be available via [`Self::poll_next`].
    pub(crate) fn connect_topic(&mut self, remote: EndpointId, topic: TopicId) {
        if let Some(state) = self.connections.get(&remote) {
            // Already connected, spawn task to open topic stream
            let state = state.clone();
            self.pending_topic_requests.spawn(
                async move {
                    let result = open_topic_stream(&state, topic).await;
                    (state.endpoint_id, topic, result)
                }
                .instrument(
                    error_span!("open_topic", remote=%remote.fmt_short(), topic=%topic.fmt_short()),
                ),
            );
        } else {
            // Not connected, queue a dial
            self.dialer
                .queue_dial(&self.endpoint, remote, self.alpn.clone());
            self.requested_topics
                .entry(remote)
                .or_default()
                .insert(topic);
        }
    }

    /// Poll for the next peer manager event.
    ///
    /// Returns `None` if the peer manager has no more work to do (all streams closed).
    ///
    /// # Cancellation Safety
    ///
    /// This method is cancellation-safe: all select branches perform only synchronous
    /// operations after the future resolves.
    pub(crate) async fn next(&mut self) -> Option<PeerManagerEvent> {
        loop {
            let event = tokio::select! {
                // Handle completed dial operations
                Some((remote, res)) = self.dialer.next(), if !self.dialer.is_empty() => {
                    self.on_dial_ready(remote, res)
                }
                // Handle incoming topic join requests from connected peers
                Some((remote, res)) = self.incoming_topic_requests.next(), if !self.incoming_topic_requests.is_empty() => {
                    self.on_incoming_topic_request(remote, res)
                }
                // Handle completed topic open tasks
                Some(res) = self.pending_topic_requests.join_next(), if !self.pending_topic_requests.is_empty() => {
                    let (remote, topic, result) = res.expect("open_topic task panicked");
                    Some(self.on_topic_request_ready(remote, topic, result))
                }
                // Handle closed connections (cleanup only, no event emitted)
                Some(res) = self.connection_close_tasks.join_next(), if !self.connection_close_tasks.is_empty() => {
                    let (remote, connection) = res.expect("connection close task panicked");
                    self.on_connection_closed(remote, &connection);
                    None
                }
                else => return None,
            };
            if let Some(event) = event {
                return Some(event);
            }
        }
    }

    /// Handle an incoming connection from a remote peer.
    ///
    /// This should be called when the main actor receives an incoming connection
    /// (e.g., via the protocol handler).
    #[instrument("accept_connection", skip_all, fields(remote=%remote.fmt_short()))]
    pub(crate) fn handle_connection(&mut self, remote: EndpointId, connection: Connection) {
        // TODO(frando): Should we check if we already have a connection to this remote?
        // Currently we just overwrite, which may not be the desired behavior.
        self.register_connection(remote, connection, Direction::Accept);
    }

    fn on_dial_ready(
        &mut self,
        remote: EndpointId,
        res: Result<Connection, ConnectError>,
    ) -> Option<PeerManagerEvent> {
        match res {
            Ok(connection) => {
                self.register_connection(remote, connection, Direction::Dial);
                // Topic streams will be opened and returned via pending_topic_opens
                None
            }
            Err(err) => {
                debug!(?err, "Dial failed");
                let topics = self.requested_topics.remove(&remote).unwrap_or_default();
                Some(PeerManagerEvent::ConnectFailed { remote, topics })
            }
        }
    }

    fn on_topic_request_ready(
        &mut self,
        remote: EndpointId,
        topic: TopicId,
        result: irpc::Result<TopicChannels>,
    ) -> PeerManagerEvent {
        match result {
            Ok(channels) => PeerManagerEvent::TopicReady {
                remote,
                topic,
                channels,
            },
            Err(_) => {
                // TODO(frando): When opening a topic stream fails, we emit ConnectFailed,
                // but the connection itself may still be valid. The caller should be aware
                // that this is a topic-level failure, not a connection-level failure.
                let topics = HashSet::from_iter([topic]);
                PeerManagerEvent::ConnectFailed { remote, topics }
            }
        }
    }

    fn on_connection_closed(&mut self, remote: EndpointId, connection: &Connection) {
        if let Some(state) = self.connections.get(&remote) {
            if state.same_connection(connection) {
                self.connections.remove(&remote);
            }
            // If it's a different connection, the old one was already replaced,
            // so we don't need to do anything.
        }
    }

    /// Register a new connection (from either dial or accept).
    ///
    /// This method is entirely synchronous - all async work is spawned into tasks.
    fn register_connection(
        &mut self,
        remote: EndpointId,
        connection: Connection,
        direction: Direction,
    ) {
        let state = ConnectionState::new(remote, connection.clone(), direction);

        // Open streams for any topics that were waiting for this connection
        let pending_topics = self.requested_topics.remove(&remote).unwrap_or_default();
        for topic in pending_topics {
            let state = state.clone();
            self.pending_topic_requests.spawn(
                async move {
                    let result = open_topic_stream(&state, topic).await;
                    (remote, topic, result)
                }
                .instrument(
                    error_span!("open_topic", remote=%remote.fmt_short(), topic=%topic.fmt_short()),
                ),
            );
        }

        // Set up stream for incoming topic join requests from this connection
        let guard = state.guard.clone();
        self.incoming_topic_requests.push(Box::pin(
            accept_stream::<net_proto::Request>(connection.clone())
                .map(move |req| (remote, req.map(|r| guard.guard(r)))),
        ));

        // Spawn task to handle connection lifecycle (idle timeout or close detection)
        let counter = state.guard.clone();
        let close_fut = async move {
            match direction {
                Direction::Dial => {
                    // For dialed connections, close after idle timeout
                    // TODO(frando): Make this timeout configurable?
                    counter.idle_for(Duration::from_millis(1000)).await;
                    debug!("closing idle dialed connection");
                    connection.close(1u32.into(), b"idle");
                }
                Direction::Accept => {
                    // For accepted connections, wait for the remote to close
                    let reason = connection.closed().await;
                    debug!(?reason, "accepted connection closed by remote");
                }
            }
            (remote, connection)
        };
        self.connection_close_tasks
            .spawn(close_fut.instrument(error_span!("conn_close", remote=%remote.fmt_short())));

        self.connections.insert(remote, state);
    }

    fn on_incoming_topic_request(
        &mut self,
        remote: EndpointId,
        res: std::io::Result<Guarded<GossipMessage>>,
    ) -> Option<PeerManagerEvent> {
        match res {
            Ok(request) => {
                let (request, guard) = request.split();
                match request {
                    GossipMessage::Join(req) => {
                        let topic = req.inner.topic_id;
                        Some(PeerManagerEvent::TopicReady {
                            remote,
                            topic,
                            channels: TopicChannels {
                                tx: Guarded::new(req.tx, guard.clone()),
                                rx: Guarded::new(req.rx, guard),
                            },
                        })
                    }
                }
            }
            Err(reason) => {
                debug!(remote=%remote.fmt_short(), ?reason, "incoming request stream closed");
                None
            }
        }
    }

    /// Check if there's an existing connection to a remote.
    #[allow(dead_code)]
    pub(crate) fn has_connection(&self, remote: &EndpointId) -> bool {
        self.connections.contains_key(remote)
    }

    /// Returns `true` if there is no pending work.
    ///
    /// When empty, [`Self::next`] will return `None` immediately.
    pub(crate) fn is_empty(&self) -> bool {
        self.dialer.is_empty()
            && self.incoming_topic_requests.is_empty()
            && self.pending_topic_requests.is_empty()
            && self.connection_close_tasks.is_empty()
    }
}

/// Open a topic stream on an existing connection.
async fn open_topic_stream(
    state: &ConnectionState,
    topic: TopicId,
) -> Result<TopicChannels, irpc::Error> {
    let guard = state.guard.get_one();
    let req = net_proto::JoinRequest { topic_id: topic };
    match state.client.bidi_streaming(req, 64, 64).await {
        Ok((tx, rx)) => Ok(TopicChannels {
            tx: Guarded::new(tx, guard.clone()),
            rx: Guarded::new(rx, guard),
        }),
        Err(err) => {
            warn!(?topic, ?err, "failed to open topic stream");
            Err(err)
        }
    }
}
