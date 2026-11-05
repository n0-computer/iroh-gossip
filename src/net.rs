//! Networking for the `iroh-gossip` protocol

use std::{collections::BTreeSet, sync::Arc};

use bytes::Bytes;
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
    Endpoint, EndpointId, PublicKey,
};
use n0_error::e;
use n0_future::task::{self, AbortOnDropHandle};
use tokio::sync::{mpsc, oneshot};
use tracing::{error_span, warn, Instrument};

pub use self::auth::AuthContext;
use self::{
    actor::{Actor, ActorConfig},
    address_lookup::GossipAddressLookup,
    auth::TopicAuth,
};
use crate::{
    api::{Command, Event, GossipTopic, JoinOptions},
    metrics::Metrics,
    proto::{self, HyparviewConfig, PlumtreeConfig, TopicId},
    Error,
};

mod actor;
mod address_lookup;
mod auth;
mod conn;
mod dialer;
mod peers;
mod queue;
mod subscriptions;
mod wire;

/// ALPN protocol name
pub const GOSSIP_ALPN: &[u8] = b"iroh-gossip/2-canary.5";

type ProtoEvent = proto::Event<PublicKey>;
type ProtoCommand = proto::Command<PublicKey>;

/// A received message, as a [`Builder::message_filter`] sees it.
pub type FilterContext<'a> = proto::FilterContext<'a, PublicKey>;
pub use proto::Verdict;

type FilterFn = dyn Fn(FilterContext<'_>) -> Verdict + Send + Sync;

/// The filter of received messages that the application set.
#[derive(Clone, derive_more::Debug)]
#[debug("MessageFilter")]
struct MessageFilter(Arc<FilterFn>);

type InEvent = proto::InEvent<PublicKey>;
type Timer = proto::Timer<PublicKey>;
type ProtoMessage = proto::Message<PublicKey>;

/// Channel capacity for topic command send channels.
const TOPIC_COMMANDS_CAP: usize = 64;

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
/// With the default settings, a node has up to 5 active neighbors per topic (the default active view).
///
/// Even though the [`Gossip`] is created from a [`Endpoint`], it does not accept connections
/// itself. You should run an accept loop on the [`Endpoint`] yourself, check the ALPN protocol of incoming
/// connections, and if the ALPN protocol equals [`GOSSIP_ALPN`], forward the connection to the
/// gossip actor through [Self::handle_connection].
///
/// The gossip actor will, however, initiate new connections to other peers by itself.
///
/// Either side retires a connection after 10 seconds without a stream in either
/// direction. Gossip opens a control stream on each connection, which
/// is the only bidirectional stream it opens. Do not use a connection that you passed to
/// [`Self::handle_connection`] for other traffic.
///
/// The actor stops when you drop the last clone of [`Gossip`]. Keep a clone for as long as you
/// need the topics. Call [`Self::shutdown`] to leave the topics cleanly first.
#[derive(Debug, Clone)]
pub struct Gossip {
    inner: Arc<Inner>,
}

/// A message from the [`Gossip`] handle to the actor.
#[derive(Debug)]
enum ToActor {
    Join(Join),
    Connection(Connection),
    Shutdown { reply: oneshot::Sender<()> },
}

/// A request from [`Gossip::subscribe_with_opts`] to the actor to join a topic.
#[derive(derive_more::Debug)]
struct Join {
    topic_id: TopicId,
    bootstrap: BTreeSet<EndpointId>,
    #[debug("{}", if auth_token.is_some() { "Some(..)" } else { "None" })]
    auth_token: Option<Bytes>,
    events_tx: mpsc::Sender<Event>,
    commands_rx: mpsc::Receiver<Command>,
    /// The actor answers once it registered the topic and applied the join.
    registered: oneshot::Sender<()>,
}

#[derive(Debug)]
struct Inner {
    to_actor: mpsc::Sender<ToActor>,
    _actor_handle: AbortOnDropHandle<()>,
    max_message_size: usize,
    /// The largest content that a broadcast takes.
    max_content: usize,
    /// The largest auth token that fits in a stream header.
    max_token: usize,
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
    config: ActorConfig,
}

impl Builder {
    /// Sets the maximum message size in bytes.
    ///
    /// By default this is `4096` bytes. The limit applies to the encoded message,
    /// which is about 15 bytes longer than its content, 14 at the default size.
    /// [`GossipSender::broadcast`] refuses a message over the limit.
    ///
    /// Use the same limit on every node of a swarm. A node drops a message over
    /// its own limit, and the peers that sent it do not learn of it.
    ///
    /// [`GossipSender::broadcast`]: crate::api::GossipSender::broadcast
    pub fn max_message_size(mut self, size: usize) -> Self {
        self.config.proto.max_message_size = size;
        self
    }

    /// Sets the membership configuration.
    ///
    /// The defaults suit most apps. Use the same values on every node of a swarm.
    /// We have not tested how a swarm behaves with different values, so test your setup
    /// first if you mix them.
    pub fn membership_config(mut self, config: HyparviewConfig) -> Self {
        self.config.proto.membership = config;
        self
    }

    /// Sets the broadcast configuration.
    ///
    /// The defaults suit most apps. Use the same values on every node of a swarm.
    /// We have not tested how a swarm behaves with different values, so test your setup
    /// first if you mix them.
    pub fn broadcast_config(mut self, config: PlumtreeConfig) -> Self {
        self.config.proto.broadcast = config;
        self
    }

    /// Sets the ALPN this gossip instance uses.
    ///
    /// It has to be the same for all peers in the network. If you set a custom ALPN,
    /// you have to use the same ALPN when registering the [`Gossip`] on an iroh
    /// router with [`RouterBuilder::accept`].
    ///
    /// [`RouterBuilder::accept`]: iroh::protocol::RouterBuilder::accept
    pub fn alpn(mut self, alpn: impl AsRef<[u8]>) -> Self {
        self.config.alpn = alpn.as_ref().to_vec().into();
        self
    }

    /// Sets the options of the queue of messages to each peer.
    pub fn send_queue(mut self, options: QueueOptions) -> Self {
        self.config.send_queue = options;
        self
    }

    /// Sets a filter that decides whether each received message is delivered and forwarded.
    ///
    /// The filter applies to all topics of this [`Gossip`]. It returns a [`Verdict`]:
    /// [`Verdict::Accept`] delivers the message to the local subscribers and forwards it
    /// to peers, as without a filter. [`Verdict::Reject`] does neither, so the message does
    /// not spread beyond this node. Use it to drop messages that are invalid, obsolete, or
    /// expired before they cost the swarm bandwidth.
    ///
    /// - The filter runs once per message id and topic, also if several peers send the
    ///   message. A rejected message counts as seen for the
    ///   [message id retention], so the filter does not run on a later copy. The node does
    ///   not cache a rejected message, so a peer that asks this node for it with a graft gets
    ///   nothing.
    /// - The node does not forward a message that a peer sent to its neighbors only. For
    ///   such a message, the filter decides only the delivery. A later copy gets the first
    ///   verdict.
    /// - Messages that this node broadcasts do not pass the filter.
    /// - The filter runs on the task of the gossip actor and delays all topics while it
    ///   runs. Keep it short and do not block. Check the cheap things first. If it needs
    ///   state, keep it behind a lock.
    /// - A rejection is final for this node: it does not deliver or forward the message
    ///   later, also if the filter would now accept it.
    ///
    /// Gossip does not authenticate the message content. A filter that checks authorship has to
    /// verify a signature in the content.
    ///
    /// [message id retention]: PlumtreeConfig::message_id_retention
    pub fn message_filter(
        mut self,
        filter: impl Fn(FilterContext<'_>) -> Verdict + Send + Sync + 'static,
    ) -> Self {
        self.config.filter = Some(MessageFilter(Arc::new(filter)));
        self
    }

    /// Sets a verifier for the auth tokens of peers.
    ///
    /// A peer sends its token for a topic in the header of each stream that it opens for
    /// the topic, see [`JoinOptions::auth_token`]. The verifier gets an [`AuthContext`] with
    /// the topic, the authenticated id of the peer and the token, if the peer sent one. It
    /// runs once per stream, before the node reads a message from it. After it rejects a
    /// stream, the connection stops each later stream of the topic without a call. It
    /// returns a [`Verdict`]:
    ///
    /// - [`Verdict::Accept`] reads the stream.
    /// - [`Verdict::Reject`] stops the stream and reads nothing from it. The node does not
    ///   process or forward any message of the peer on this topic, not even a membership message.
    ///   It drops the peer from the topic, so it stops sending to the peer too. The peer sees
    ///   [`Event::Refused`] on its topic. It sends no message of the topic to us on this
    ///   connection again. Other topics on the connection do not change.
    ///
    /// The verifier applies to all topics of this [`Gossip`], also to topics that this
    /// node has not joined, so it can set a policy for each topic. Without a verifier,
    /// the node accepts every stream.
    ///
    /// - A token is opaque to gossip. To bind a token to its owner, include the endpoint id
    ///   in it and compare it with the id the verifier gets.
    /// - The verifier runs on the task of the connection, not on the actor. A slow one
    ///   delays only the streams of that connection. It still must not block a thread.
    /// - A panic in the verifier stops the gossip actor.
    /// - The node never forwards a token. Every member must be able to check the token of every
    ///   other member.
    ///
    /// [`JoinOptions::auth_token`]: crate::api::JoinOptions::auth_token
    /// [`Event::Refused`]: crate::api::Event::Refused
    pub fn topic_auth(
        mut self,
        verify: impl Fn(AuthContext<'_>) -> Verdict + Send + Sync + 'static,
    ) -> Self {
        self.config.topic_auth = TopicAuth::new(verify);
        self
    }

    /// Sets how long a connection stays open without any stream, before this node retires it.
    ///
    /// The time runs while no stream is open on the connection in either direction, and no
    /// message waits to go out. A connection to a neighbor keeps its streams open, so it
    /// does not retire. To retire, the node finishes its streams and sends `GOAWAY`. A
    /// later message to the peer opens a new connection. Either side can retire a
    /// connection, so the shorter time of the two nodes applies.
    ///
    /// The default is 10 seconds. A shorter time closes unused connections sooner. A
    /// longer one saves dials to peers that the node uses now and then.
    pub fn retire_after(mut self, after: std::time::Duration) -> Self {
        self.config.retire_after = after;
        self
    }

    /// Spawns a gossip actor, and returns its handle.
    ///
    /// This has to run inside a tokio runtime. The actor stops when you drop the last clone of
    /// the returned [`Gossip`].
    pub fn spawn(self, endpoint: Endpoint) -> Gossip {
        let metrics = Arc::new(Metrics::default());
        let address_lookup = GossipAddressLookup::default();

        // `Endpoint::address_lookup` returns `Err` when the endpoint is closed.
        // In that case, the gossip actor will close too very soon for other reasons,
        // so it's fine if we only add our `GossipAddressLookup` for the non-closed
        // case. The alternative would be to return a `Result` from `spawn`,
        // but as long as this is the only direct error case, it seems unwarranted.
        if let Ok(endpoint_addr_lookup) = endpoint.address_lookup().as_ref() {
            endpoint_addr_lookup.add(address_lookup.clone());
        }
        let (actor, to_actor) = Actor::new(endpoint, self.config, metrics.clone(), address_lookup);
        let me = actor.endpoint.id().fmt_short();
        let max_message_size = actor.proto.max_message_size();

        let actor_handle = task::spawn(actor.run().instrument(error_span!("gossip", %me)));

        Gossip::new(
            to_actor,
            AbortOnDropHandle::new(actor_handle),
            max_message_size,
            metrics,
        )
    }
}

/// Options of the send queue to each peer, for [`Builder::send_queue`].
///
/// Gossip keeps one send queue per peer, shared between all topics. The queue
/// holds up to [`Self::size`] bytes of messages on their way to that peer.
/// When the queue becomes full, new data and announcement messages to that peer
/// will be dropped to not hold up the whole swarm. If this peer cannot keep
/// up with the volume of messages to be sent or relayed, the following
/// may happen:
///
/// - A broadcast to the peer is dropped, and [`Metrics::msgs_dropped`] counts
///   it. Gossip announces the broadcast to the peer again, and the peer
///   fetches it while it is still cached, 30 s by default. A broadcast to
///   direct neighbors is never repaired.
/// - A control message to the peer closes the connection, and
///   [`Metrics::peers_send_queue_full`] counts it. Both sides find new
///   neighbors.
///
/// Independent of the size:
///
/// - Messages to a slow peer arrive late. The queue bounds bytes, not time,
///   and QUIC buffers more on top.
/// - A peer that takes no data for 60 s loses its connection, as does one
///   that shares more topics with this node than its stream limit allows,
///   100 by default.
#[derive(Debug, Clone, Copy)]
pub struct QueueOptions {
    size: usize,
}

impl Default for QueueOptions {
    fn default() -> Self {
        Self {
            size: queue::QUEUE_SIZE,
        }
    }
}

impl QueueOptions {
    /// Sets the size of the queue in bytes, 1 MiB by default.
    ///
    /// A larger queue absorbs larger bursts and longer stalls without a
    /// loss. It costs up to its size in memory for each peer, and it lets
    /// more old data wait for a slow one. A message takes its encoded size
    /// and about 130 bytes more. Whatever the setting, the queue is at
    /// least 64 KiB, and fits eight messages of [`Builder::max_message_size`].
    pub fn size(mut self, size: usize) -> Self {
        self.size = size;
        self
    }
}

impl Gossip {
    /// Creates a handle for the actor task in `actor_handle`.
    fn new(
        to_actor: mpsc::Sender<ToActor>,
        actor_handle: AbortOnDropHandle<()>,
        max_message_size: usize,
        metrics: Arc<Metrics>,
    ) -> Self {
        let inner = Inner {
            to_actor,
            _actor_handle: actor_handle,
            max_message_size,
            max_content: proto::topic::max_content_size(max_message_size),
            max_token: wire::MAX_TOKEN_LEN,
            metrics,
        };
        Self {
            inner: Arc::new(inner),
        }
    }

    /// Creates a default [`Builder`].
    pub fn builder() -> Builder {
        Builder {
            config: Default::default(),
        }
    }

    /// Joins a gossip topic with options.
    ///
    /// Returns a [`GossipTopic`] once the node has registered the topic, so it accepts a join
    /// from a peer from then on. It does not wait for a neighbor. To wait for the first
    /// neighbor, await [`GossipTopic::joined`].
    ///
    /// See [`JoinOptions`] for available options.
    ///
    /// A broadcast reaches only the neighbors that exist when the node sends it. Await
    /// [`GossipTopic::joined`] before the first broadcast. With no bootstrap peers, the node
    /// waits until another node joins it.
    ///
    /// The node leaves the topic when you drop all handles of the topic.
    ///
    /// A [`JoinOptions::subscription_capacity`] of 0 counts as 1.
    ///
    /// # Errors
    ///
    /// - [`Error::TokenTooLarge`] if the auth token does not fit in a stream header.
    /// - [`Error::Closed`] if the gossip actor stopped.
    pub async fn subscribe_with_opts(
        &self,
        topic_id: TopicId,
        opts: JoinOptions,
    ) -> Result<GossipTopic, Error> {
        if let Some(token) = &opts.auth_token {
            if token.len() > self.inner.max_token {
                return Err(e!(Error::TokenTooLarge {
                    size: token.len(),
                    max_token: self.inner.max_token
                }));
            }
        }
        let (commands_tx, commands_rx) = mpsc::channel(TOPIC_COMMANDS_CAP);
        let (events_tx, events_rx) = mpsc::channel(opts.subscription_capacity.max(1));
        let (registered_tx, registered_rx) = oneshot::channel();
        self.inner
            .to_actor
            .send(ToActor::Join(Join {
                topic_id,
                bootstrap: opts.bootstrap,
                auth_token: opts.auth_token,
                events_tx,
                commands_rx,
                registered: registered_tx,
            }))
            .await
            .map_err(|_| e!(Error::Closed))?;
        registered_rx.await.map_err(|_| e!(Error::Closed))?;
        Ok(GossipTopic::new(
            commands_tx,
            events_rx,
            self.inner.max_content,
        ))
    }

    /// Joins a gossip topic with the default options, and waits for the first neighbor.
    ///
    /// With an empty `bootstrap` list, or one that holds only our own id, the call waits until
    /// a peer joins us. It has no timeout for that wait.
    ///
    /// # Errors
    ///
    /// - [`Error::JoinFailed`] if the join got no neighbor from its bootstrap peers, see
    ///   [`GossipReceiver::joined`]. The call then drops the topic handle. Thus the node leaves
    ///   the topic, unless another handle for the topic exists.
    /// - [`Error::Closed`] if the gossip actor stopped.
    ///
    /// [`GossipReceiver::joined`]: crate::api::GossipReceiver::joined
    pub async fn subscribe_and_join(
        &self,
        topic_id: TopicId,
        bootstrap: Vec<EndpointId>,
    ) -> Result<GossipTopic, Error> {
        let mut sub = self
            .subscribe_with_opts(topic_id, JoinOptions::with_bootstrap(bootstrap))
            .await?;
        sub.joined().await?;
        Ok(sub)
    }

    /// Joins a gossip topic with the default options.
    ///
    /// It does not wait for a bootstrap peer. To wait for the first neighbor, use
    /// [`GossipTopic::joined`] or [`Self::subscribe_and_join`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Closed`] if the gossip actor stopped.
    pub async fn subscribe(
        &self,
        topic_id: TopicId,
        bootstrap: Vec<EndpointId>,
    ) -> Result<GossipTopic, Error> {
        self.subscribe_with_opts(topic_id, JoinOptions::with_bootstrap(bootstrap))
            .await
    }

    /// Get the maximum message size configured for this gossip actor.
    ///
    /// This is the size of the encoded message. For the largest content that you can broadcast,
    /// see [`GossipSender::max_content_size`].
    ///
    /// [`GossipSender::max_content_size`]: crate::api::GossipSender::max_content_size
    pub fn max_message_size(&self) -> usize {
        self.inner.max_message_size
    }

    /// Handles an incoming [`Connection`].
    ///
    /// The connection should be an accepted, server-side connection.
    /// Make sure to check the ALPN protocol yourself before passing the connection.
    ///
    /// Gossip owns the connection from now on, and retires it once it is idle.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Closed`] if the gossip actor stopped.
    pub async fn handle_connection(&self, conn: Connection) -> Result<(), Error> {
        self.inner
            .to_actor
            .send(ToActor::Connection(conn))
            .await
            .map_err(|_| e!(Error::Closed))?;
        Ok(())
    }

    /// Shutdown the gossip instance.
    ///
    /// It leaves all topics and sends `Close` to the peers. Then it stops the actor, and
    /// drops all state and connections. A gossip
    /// instance that is shut down already, or shutting down, returns `Ok` too.
    pub async fn shutdown(&self) -> Result<(), Error> {
        let (reply, reply_rx) = oneshot::channel();
        let shutdown = ToActor::Shutdown { reply };
        // The actor is gone or about to go, which is what the caller asks for.
        if self.inner.to_actor.send(shutdown).await.is_ok() {
            reply_rx.await.ok();
        }
        Ok(())
    }

    /// Returns the metrics tracked for this gossip instance.
    pub fn metrics(&self) -> &Arc<Metrics> {
        &self.inner.metrics
    }
}
