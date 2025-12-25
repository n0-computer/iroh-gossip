//! Implementation of the Plumtree epidemic broadcast tree protocol
//!
//! The implementation is based on [this paper][paper] by Joao Leitao, Jose Pereira, Luıs Rodrigues
//! and the [example implementation][impl] by Bartosz Sypytkowski
//!
//! [paper]: https://asc.di.fct.unl.pt/~jleitao/pdf/srds07-leitao.pdf
//! [impl]: https://gist.github.com/Horusiath/84fac596101b197da0546d1697580d99

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    hash::Hash,
};

use bytes::Bytes;
use derive_more::{Add, From, Sub};
use n0_future::time::{Duration, Instant};
use postcard::experimental::max_size::MaxSize;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::{
    util::{idbytes_impls, TimeBoundCache},
    PeerIdentity, IO,
};

/// A message identifier, which is the message content's blake3 hash.
#[derive(Serialize, Deserialize, Clone, Hash, Copy, PartialEq, Eq, MaxSize)]
pub struct MessageId([u8; 32]);
idbytes_impls!(MessageId, "MessageId");

impl MessageId {
    /// Create a `[MessageId]` by hashing the message content.
    ///
    /// This hashes the input with [`blake3::hash`].
    pub fn from_content(message: &[u8]) -> Self {
        Self::from(blake3::hash(message))
    }
}

/// Events Plumtree is informed of from the peer sampling service and IO layer.
#[derive(Debug)]
pub enum InEvent<PI> {
    /// A [`Message`] was received from the peer.
    RecvMessage(PI, Message),
    /// Broadcast the contained payload to the given scope.
    Broadcast(Bytes, Scope),
    /// A timer has expired.
    TimerExpired(Timer),
    /// New member `PI` has joined the topic.
    NeighborUp(PI),
    /// Peer `PI` has disconnected from the topic.
    NeighborDown(PI),
}

/// Events Plumtree emits.
#[derive(Debug, PartialEq, Eq)]
pub enum OutEvent<PI> {
    /// Ask the IO layer to send a [`Message`] to peer `PI`.
    SendMessage(PI, Message),
    /// Schedule a [`Timer`].
    ScheduleTimer(Duration, Timer),
    /// Emit an [`Event`] to the application.
    EmitEvent(Event<PI>),
}

/// Kinds of timers Plumtree needs to schedule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Timer {
    /// Request the content for [`MessageId`] by sending [`Message::Graft`].
    ///
    /// The message will be sent to a peer that sent us an [`Message::IHave`] for this [`MessageId`],
    /// which will send us the message content in reply and also move the peer into the eager set.
    /// Will be a no-op if the message for [`MessageId`] was already received from another peer by now.
    SendGraft(MessageId),
    /// Dispatch the [`Message::IHave`] in our lazy push queue.
    DispatchLazyPush,
    /// Evict the message cache
    EvictCache,
}

/// Event emitted by the [`State`] to the application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event<PI> {
    /// A new gossip message was received.
    Received(GossipEvent<PI>),
}

#[derive(Clone, derive_more::Debug, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct GossipEvent<PI> {
    /// The content of the gossip message.
    #[debug("<{}b>", content.len())]
    pub content: Bytes,
    /// The peer that we received the gossip message from. Note that this is not the peer that
    /// originally broadcasted the message, but the peer before us in the gossiping path.
    pub delivered_from: PI,
    /// The broadcast scope of the message.
    pub scope: DeliveryScope,
}

impl<PI> GossipEvent<PI> {
    fn from_message(message: &Gossip, from: PI) -> Self {
        Self {
            content: message.content.clone(),
            scope: message.scope,
            delivered_from: from,
        }
    }
}

/// Number of delivery hops a message has taken.
#[derive(
    From,
    Add,
    Sub,
    Serialize,
    Deserialize,
    Eq,
    PartialEq,
    PartialOrd,
    Ord,
    Clone,
    Copy,
    Debug,
    Hash,
    MaxSize,
)]
pub struct Round(u16);

impl Round {
    pub fn next(&self) -> Round {
        Round(self.0 + 1)
    }
}

/// Messages that we can send and receive from peers within the topic.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Message {
    /// When receiving Gossip, emit as event and forward full message to eager peer and (after a
    /// delay) message IDs to lazy peers.
    Gossip(Gossip),
    /// When receiving Prune, move the peer from the eager to the lazy set.
    Prune,
    /// When receiving Graft, move the peer to the eager set and send the full content for the
    /// included message ID.
    Graft(Graft),
    /// When receiving IHave, do nothing initially, and request the messages for the included
    /// message IDs after some time if they aren't pushed eagerly to us.
    IHave(Vec<IHave>),
}

/// Payload messages transmitted by the protocol.
#[derive(Serialize, Deserialize, Clone, derive_more::Debug, PartialEq, Eq)]
pub struct Gossip {
    /// Id of the message.
    id: MessageId,
    /// Message contents.
    #[debug("<{}b>", content.len())]
    content: Bytes,
    /// Scope to broadcast to.
    scope: DeliveryScope,
}

impl Gossip {
    fn round(&self) -> Option<Round> {
        match self.scope {
            DeliveryScope::Swarm(round) => Some(round),
            DeliveryScope::Neighbors => None,
        }
    }
}

/// The scope to deliver the message to.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Copy)]
pub enum DeliveryScope {
    /// This message was received from the swarm, with a distance (in hops) travelled from the
    /// original broadcaster.
    Swarm(Round),
    /// This message was received from a direct neighbor that broadcasted the message to neighbors
    /// only.
    Neighbors,
}

impl DeliveryScope {
    /// Whether this message was directly received from its publisher.
    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Neighbors | Self::Swarm(Round(0)))
    }
}

/// The broadcast scope of a gossip message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Copy)]
pub enum Scope {
    /// The message is broadcast to all peers in the swarm.
    Swarm,
    /// The message is broadcast only to the immediate neighbors of a peer.
    Neighbors,
}

impl Gossip {
    /// Get a clone of this `Gossip` message and increase the delivery round by 1.
    pub fn next_round(&self) -> Option<Gossip> {
        match self.scope {
            DeliveryScope::Neighbors => None,
            DeliveryScope::Swarm(round) => Some(Gossip {
                id: self.id,
                content: self.content.clone(),
                scope: DeliveryScope::Swarm(round.next()),
            }),
        }
    }

    /// Validate that the message id is the blake3 hash of the message content.
    pub fn validate(&self) -> bool {
        let expected = MessageId::from_content(&self.content);
        expected == self.id
    }
}

/// Control message to inform peers we have a message without transmitting the whole payload.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, MaxSize)]
pub struct IHave {
    /// Id of the message.
    pub(crate) id: MessageId,
    /// Delivery round of the message.
    pub(crate) round: Round,
}

/// Control message to signal a peer that they have been moved to the eager set, and to ask the
/// peer to do the same with this node.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Graft {
    /// Message id that triggers the graft, if any.
    /// On receiving a graft, the payload message must be sent in reply if a message id is set.
    id: Option<MessageId>,
    /// Delivery round of the [`Message::IHave`] that triggered this Graft message.
    round: Round,
}

/// Controls when to prune peers from eager to lazy push sets.
///
/// The plumtree protocol optimizes bandwidth by moving peers that send duplicate messages
/// from eager (full payload) to lazy (message IDs only). In small swarms or burst
/// scenarios, this can hurt performance.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PruneMode {
    /// Prune immediately on first duplicate message (default).
    /// Best for large swarms with stable topology.
    Immediate,
    /// Prune after receiving N duplicate messages from same peer.
    /// Best for burst scenarios or small swarms. Recommended: 3-10.
    AfterDuplicates(usize),
    /// Never prune, keep all peers in eager set.
    /// Use for very small swarms (2-3 nodes) or when reliability > bandwidth.
    Disabled,
}

impl Default for PruneMode {
    fn default() -> Self {
        // Default to pruning after 3 duplicates for better handling of burst scenarios.
        Self::AfterDuplicates(3)
    }
}

/// Configuration for the gossip broadcast layer.
///
/// Currently, the expectation is that the configuration is the same for all peers in the
/// network (as recommended in the paper).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// When receiving an `IHave` message, this timeout is registered. If the message for the
    /// `IHave` was not received once the timeout is expired, a `Graft` message is sent to the
    /// peer that sent us the `IHave` to request the message payload.
    ///
    /// The plumtree paper notes:
    /// > The timeout value is a protocol parameter that should be configured considering the
    /// > diameter of the overlay and a target maximum recovery latency, defined by the application
    /// > requirements. (p.8)
    pub graft_timeout_1: Duration,
    /// This timeout is registered when sending a `Graft` message. If a reply has not been
    /// received once the timeout expires, we send another `Graft` message to the next peer that
    /// sent us an `IHave` for this message.
    ///
    /// The plumtree paper notes:
    /// > This second timeout value should be smaller that the first, in the order of an average
    /// > round trip time to a neighbor.
    pub graft_timeout_2: Duration,
    /// Timeout after which `IHave` messages are pushed to peers.
    pub dispatch_timeout: Duration,
    /// The protocol performs a tree optimization, which promotes lazy peers to eager peers if the
    /// `Message::IHave` messages received from them have a lower number of hops from the
    /// message's origin as the `InEvent::Broadcast` messages received from our eager peers. This
    /// parameter is the number of hops that the lazy peers must be closer to the origin than our
    /// eager peers to be promoted to become an eager peer.
    pub optimization_threshold: Round,

    /// Duration for which to keep gossip messages in the internal message cache.
    ///
    /// Messages broadcast from this node or received from other nodes are kept in an internal
    /// cache for this duration before being evicted. If this is too low, other nodes will not be
    /// able to retrieve messages once they need them. If this is high, the cache will grow.
    ///
    /// Should be at least around several round trip times to peers.
    pub message_cache_retention: Duration,

    /// Duration for which to keep the `MessageId`s for received messages.
    ///
    /// Should be at least as long as [`Self::message_cache_retention`], usually will be longer to
    /// not accidentally receive messages multiple times.
    pub message_id_retention: Duration,

    /// How often the internal caches will be checked for expired items.
    pub cache_evict_interval: Duration,

    /// Controls when peers are pruned from eager to lazy push sets.
    ///
    /// - [`PruneMode::Immediate`]: Current behavior, prune on first duplicate
    /// - [`PruneMode::AfterDuplicates`]: More tolerant, prune after n duplicates
    /// - [`PruneMode::Disabled`]: Never prune, useful for 2-3 node swarms
    pub prune_mode: PruneMode,

    /// Cooldown period after promoting a peer to eager set.
    ///
    /// During this period, the peer will not be pruned on duplicate receipt.
    /// Prevents aggressive pruning during rapid message bursts.
    pub prune_cooldown: Duration,

    /// Minimum number of eager peers to maintain.
    ///
    /// Pruning will be skipped if it would reduce eager peers below this threshold.
    /// Provides a safety floor to prevent mesh fragmentation.
    pub min_eager_peers: usize,
}

impl Default for Config {
    /// Sensible defaults for the plumtree configuration
    //
    // TODO: Find out what good defaults are for the three timeouts here. Current numbers are
    // guesses that need validation. The paper does not have concrete recommendations for these
    // numbers.
    fn default() -> Self {
        Self {
            // Graft timeout: when we receive IHave, wait for this duration before sending Graft.
            // Higher value provides more tolerance for unstable connections under load. Lower
            // value provides faster recovery of missing messages.
            graft_timeout_1: Duration::from_millis(80),

            // Second graft timeout for retry.
            graft_timeout_2: Duration::from_millis(40),

            // Again, paper does not tell a recommended number here. Likely should be quite small,
            // as to not delay messages without need. This would also be the time frame in which
            // `IHave`s are aggregated to save on packets.
            //
            // Eartstar dispatches immediately from my reading.
            dispatch_timeout: Duration::from_millis(5),

            // This number comes from experiment settings the plumtree paper (p. 12)
            optimization_threshold: Round(7),

            // This is a certainly-high-enough value for usual operation.
            message_cache_retention: Duration::from_secs(30),
            message_id_retention: Duration::from_secs(90),
            cache_evict_interval: Duration::from_secs(1),

            // Default to pruning after 3 duplicates for better handling of burst scenarios.
            prune_mode: PruneMode::default(),
            // 1s cooldown prevents cascade pruning under load
            prune_cooldown: Duration::from_secs(1),
            // Keep at least 2 eager peers to prevent mesh fragmentation
            min_eager_peers: 2,
        }
    }
}

/// Stats about this topic's plumtree.
#[derive(Debug, Default, Clone)]
pub struct Stats {
    /// Number of payload messages received so far.
    ///
    /// See [`Message::Gossip`].
    pub payload_messages_received: u64,
    /// Number of control messages received so far.
    ///
    /// See [`Message::Prune`], [`Message::Graft`], [`Message::IHave`].
    pub control_messages_received: u64,
    /// Max round seen so far.
    pub max_last_delivery_hop: u16,
}

/// State of the plumtree.
#[derive(Debug)]
pub struct State<PI> {
    /// Our address.
    me: PI,
    /// Configuration for this plumtree.
    config: Config,

    /// Set of peers used for payload exchange.
    pub(crate) eager_push_peers: BTreeSet<PI>,
    /// Set of peers used for control message exchange.
    pub(crate) lazy_push_peers: BTreeSet<PI>,

    lazy_push_queue: BTreeMap<PI, Vec<IHave>>,

    /// Messages for which a [`MessageId`] has been seen via a [`Message::IHave`] but we have not
    /// yet received the full payload. For each, we store the peers that have claimed to have this
    /// message.
    missing_messages: HashMap<MessageId, VecDeque<(PI, Round)>>,
    /// Messages for which the full payload has been seen.
    received_messages: TimeBoundCache<MessageId, ()>,
    /// Payloads of received messages.
    cache: TimeBoundCache<MessageId, Gossip>,
    /// Messages that we originated via broadcast() - don't prune peers for relaying these back.
    originated_messages: TimeBoundCache<MessageId, ()>,

    /// Message ids for which a [`Timer::SendGraft`] has been scheduled.
    graft_timer_scheduled: HashSet<MessageId>,
    /// Whether a [`Timer::DispatchLazyPush`] has been scheduled.
    dispatch_timer_scheduled: bool,

    /// Set to false after the first message is received. Used for initial timer scheduling.
    init: bool,

    /// [`Stats`] of this plumtree.
    pub(crate) stats: Stats,

    max_message_size: usize,

    /// Tracks duplicate message counts per peer for AfterDuplicates prune mode.
    duplicate_counts: HashMap<PI, usize>,
    /// Track when peers were last promoted to eager set for prune cooldown.
    eager_promotion_times: HashMap<PI, Instant>,
}

impl<PI: PeerIdentity> State<PI> {
    /// Initialize the [`State`] of a plumtree.
    pub fn new(me: PI, config: Config, max_message_size: usize) -> Self {
        Self {
            me,
            eager_push_peers: Default::default(),
            lazy_push_peers: Default::default(),
            lazy_push_queue: Default::default(),
            config,
            missing_messages: Default::default(),
            received_messages: Default::default(),
            graft_timer_scheduled: Default::default(),
            dispatch_timer_scheduled: false,
            cache: Default::default(),
            originated_messages: Default::default(),
            init: false,
            stats: Default::default(),
            max_message_size,
            duplicate_counts: Default::default(),
            eager_promotion_times: Default::default(),
        }
    }

    /// Handle an [`InEvent`].
    pub fn handle(&mut self, event: InEvent<PI>, now: Instant, io: &mut impl IO<PI>) {
        if !self.init {
            self.init = true;
            self.on_evict_cache_timer(now, io)
        }
        match event {
            InEvent::RecvMessage(from, message) => self.handle_message(from, message, now, io),
            InEvent::Broadcast(data, scope) => self.broadcast(data, scope, now, io),
            InEvent::NeighborUp(peer) => self.on_neighbor_up(peer, now),
            InEvent::NeighborDown(peer) => self.on_neighbor_down(peer),
            InEvent::TimerExpired(timer) => match timer {
                Timer::DispatchLazyPush => self.on_dispatch_timer(io),
                Timer::SendGraft(id) => {
                    self.on_send_graft_timer(id, now, io);
                }
                Timer::EvictCache => self.on_evict_cache_timer(now, io),
            },
        }
    }

    /// Get access to the [`Stats`] of the plumtree.
    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    /// Handle receiving a [`Message`].
    fn handle_message(&mut self, sender: PI, message: Message, now: Instant, io: &mut impl IO<PI>) {
        if matches!(message, Message::Gossip(_)) {
            self.stats.payload_messages_received += 1;
        } else {
            self.stats.control_messages_received += 1;
        }
        match message {
            Message::Gossip(details) => self.on_gossip(sender, details, now, io),
            Message::Prune => self.on_prune(sender),
            Message::IHave(details) => self.on_ihave(sender, details, io),
            Message::Graft(details) => self.on_graft(sender, details, now, io),
        }
    }

    /// Dispatches messages from lazy queue over to lazy peers.
    fn on_dispatch_timer(&mut self, io: &mut impl IO<PI>) {
        let chunk_size = self.max_message_size
            // Space for discriminator
            - 1
            // Space for length prefix
            - 2;
        let chunk_len = chunk_size / IHave::POSTCARD_MAX_SIZE;
        while let Some((peer, list)) = self.lazy_push_queue.pop_first() {
            for chunk in list.chunks(chunk_len) {
                io.push(OutEvent::SendMessage(peer, Message::IHave(chunk.to_vec())));
            }
        }

        self.dispatch_timer_scheduled = false;
    }

    /// Send a gossip message.
    ///
    /// Will be pushed in full to eager peers.
    /// Pushing the message id to the lazy peers is delayed by a timer.
    fn broadcast(&mut self, content: Bytes, scope: Scope, now: Instant, io: &mut impl IO<PI>) {
        let id = MessageId::from_content(&content);
        let scope = match scope {
            Scope::Neighbors => DeliveryScope::Neighbors,
            Scope::Swarm => DeliveryScope::Swarm(Round(0)),
        };
        let message = Gossip { id, content, scope };
        let me = self.me;
        if let DeliveryScope::Swarm(_) = scope {
            self.received_messages
                .insert(id, (), now + self.config.message_id_retention);
            // Track that we originated this message - don't prune peers for relaying it back
            self.originated_messages
                .insert(id, (), now + self.config.message_id_retention);
            self.cache.insert(
                id,
                message.clone(),
                now + self.config.message_cache_retention,
            );
            self.lazy_push(message.clone(), &me, io);
        }

        self.eager_push(message.clone(), &me, io);
    }

    /// Handle receiving a [`Message::Gossip`].
    fn on_gossip(&mut self, sender: PI, message: Gossip, now: Instant, io: &mut impl IO<PI>) {
        // Validate that the message id is the blake3 hash of the message content.
        if !message.validate() {
            // TODO: Do we want to take any measures against the sender if we received a message
            // with a spoofed message id?
            warn!(
                peer = ?sender,
                "Received a message with spoofed message id ({})", message.id
            );
            return;
        }

        // if we already received this message: consider pruning based on config
        if self.received_messages.contains_key(&message.id) {
            debug!("duplicate from {:?}, msg={}", sender, message.id);
            self.maybe_prune_on_duplicate(sender, &message.id, now, io);
        // otherwise store the message, emit to application and forward to peers
        } else {
            // Reset duplicate count on successful message delivery
            self.duplicate_counts.remove(&sender);
            // Refresh cooldown for active eager peers - prevents timing-sensitive pruning
            if self.eager_push_peers.contains(&sender) {
                self.eager_promotion_times.insert(sender, now);
            }
            if let DeliveryScope::Swarm(prev_round) = message.scope {
                // insert the message in the list of received messages
                self.received_messages.insert(
                    message.id,
                    (),
                    now + self.config.message_id_retention,
                );
                // increase the round for forwarding the message, and add to cache
                // to reply to Graft messages later
                // TODO: add callback/event to application to get missing messages that were received before?
                let message = message.next_round().expect("just checked");

                self.cache.insert(
                    message.id,
                    message.clone(),
                    now + self.config.message_cache_retention,
                );
                // push the message to our peers
                self.eager_push(message.clone(), &sender, io);
                self.lazy_push(message.clone(), &sender, io);
                // cleanup places where we track missing messages
                self.graft_timer_scheduled.remove(&message.id);
                let previous_ihaves = self.missing_messages.remove(&message.id);
                // do the optimization step from the paper
                if let Some(previous_ihaves) = previous_ihaves {
                    self.optimize_tree(&sender, &message, previous_ihaves, now, io);
                }
                self.stats.max_last_delivery_hop =
                    self.stats.max_last_delivery_hop.max(prev_round.0);
            }

            // emit event to application
            io.push(OutEvent::EmitEvent(Event::Received(
                GossipEvent::from_message(&message, sender),
            )));
        }
    }

    /// Optimize the tree by pruning the `sender` of a [`Message::Gossip`] if we previously
    /// received a [`Message::IHave`] for the same message with a much lower number of delivery
    /// hops from the original broadcaster of the message.
    ///
    /// See [Config::optimization_threshold].
    fn optimize_tree(
        &mut self,
        gossip_sender: &PI,
        message: &Gossip,
        previous_ihaves: VecDeque<(PI, Round)>,
        now: Instant,
        io: &mut impl IO<PI>,
    ) {
        let round = message.round().expect("only called for swarm messages");
        let best_ihave = previous_ihaves
            .iter()
            .min_by(|(_a_peer, a_round), (_b_peer, b_round)| a_round.cmp(b_round))
            .copied();

        if let Some((ihave_peer, ihave_round)) = best_ihave {
            if (ihave_round < round) && (round - ihave_round) >= self.config.optimization_threshold
            {
                // Graft the sender of the IHave, but only if it's not already eager.
                if !self.eager_push_peers.contains(&ihave_peer) {
                    let message = Message::Graft(Graft {
                        id: None,
                        round: ihave_round,
                    });
                    self.add_eager_at(ihave_peer, now);
                    io.push(OutEvent::SendMessage(ihave_peer, message));
                }
                // Prune the sender of the Gossip.
                // Only prune immediately in Immediate mode; AfterDuplicates uses duplicate path.
                if matches!(self.config.prune_mode, PruneMode::Immediate)
                    && !self.is_in_prune_cooldown(gossip_sender, now)
                    && self.eager_push_peers.len() > self.config.min_eager_peers
                {
                    self.add_lazy(*gossip_sender);
                    io.push(OutEvent::SendMessage(*gossip_sender, Message::Prune));
                }
            }
        }
    }

    /// Handle receiving a [`Message::Prune`].
    fn on_prune(&mut self, sender: PI) {
        self.add_lazy(sender);
    }

    /// Handle receiving a [`Message::IHave`].
    ///
    /// > When a node receives a IHAVE message, it simply marks the corresponding message as
    /// > missing It then starts a timer, with a predefined timeout value, and waits for the missing
    /// > message to be received via eager push before the timer expires. The timeout value is a
    /// > protocol parameter that should be configured considering the diameter of the overlay and a
    /// > target maximum recovery latency, defined by the application requirements. This is a
    /// > parameter that should be statically configured at deployment time. (p8)
    fn on_ihave(&mut self, sender: PI, ihaves: Vec<IHave>, io: &mut impl IO<PI>) {
        for ihave in ihaves {
            if !self.received_messages.contains_key(&ihave.id) {
                self.missing_messages
                    .entry(ihave.id)
                    .or_default()
                    .push_back((sender, ihave.round));

                if !self.graft_timer_scheduled.contains(&ihave.id) {
                    self.graft_timer_scheduled.insert(ihave.id);
                    io.push(OutEvent::ScheduleTimer(
                        self.config.graft_timeout_1,
                        Timer::SendGraft(ihave.id),
                    ));
                }
            }
        }
    }

    /// A scheduled [`Timer::SendGraft`] has reached it's deadline.
    fn on_send_graft_timer(&mut self, id: MessageId, now: Instant, io: &mut impl IO<PI>) {
        self.graft_timer_scheduled.remove(&id);
        // if the message was received before the timer ran out, there is no need to request it
        // again
        if self.received_messages.contains_key(&id) {
            return;
        }
        // get the first peer that advertised this message
        let entry = self
            .missing_messages
            .get_mut(&id)
            .and_then(|entries| entries.pop_front());
        if let Some((peer, round)) = entry {
            self.add_eager_at(peer, now);
            let message = Message::Graft(Graft {
                id: Some(id),
                round,
            });
            io.push(OutEvent::SendMessage(peer, message));

            // "when a GRAFT message is sent, another timer is started to expire after a certain timeout,
            // to ensure that the message will be requested to another neighbor if it is not received
            // meanwhile. This second timeout value should be smaller that the first, in the order of
            // an average round trip time to a neighbor." (p9)
            io.push(OutEvent::ScheduleTimer(
                self.config.graft_timeout_2,
                Timer::SendGraft(id),
            ));
        }
    }

    /// Handle receiving a [`Message::Graft`].
    fn on_graft(&mut self, sender: PI, details: Graft, now: Instant, io: &mut impl IO<PI>) {
        self.add_eager_at(sender, now);
        if let Some(id) = details.id {
            if let Some(message) = self.cache.get(&id) {
                io.push(OutEvent::SendMessage(
                    sender,
                    Message::Gossip(message.clone()),
                ));
            } else {
                debug!(?id, peer=?sender, "on_graft failed to graft: message not in cache");
            }
        }
    }

    /// Handle a [`InEvent::NeighborUp`] when a peer joins the topic.
    fn on_neighbor_up(&mut self, peer: PI, now: Instant) {
        self.add_eager_at(peer, now);
    }

    /// Handle a [`InEvent::NeighborDown`] when a peer leaves the topic.
    /// > When a neighbor is detected to leave the overlay, it is simple removed from the
    /// > membership. Furthermore, the record of IHAVE messages sent from failed members is deleted
    /// > from the missing history. (p9)
    fn on_neighbor_down(&mut self, peer: PI) {
        self.missing_messages.retain(|_message_id, ihaves| {
            ihaves.retain(|(ihave_peer, _round)| *ihave_peer != peer);
            !ihaves.is_empty()
        });
        self.eager_push_peers.remove(&peer);
        self.lazy_push_peers.remove(&peer);
        self.duplicate_counts.remove(&peer);
        self.eager_promotion_times.remove(&peer);
    }

    fn on_evict_cache_timer(&mut self, now: Instant, io: &mut impl IO<PI>) {
        self.cache.expire_until(now);
        self.originated_messages.expire_until(now);
        self.expire_promotion_times(now);
        io.push(OutEvent::ScheduleTimer(
            self.config.cache_evict_interval,
            Timer::EvictCache,
        ));
    }

    /// Moves peer into eager set with timestamp for cooldown tracking.
    fn add_eager_at(&mut self, peer: PI, now: Instant) {
        self.lazy_push_peers.remove(&peer);
        if self.eager_push_peers.insert(peer) {
            self.eager_promotion_times.insert(peer, now);
        }
    }

    /// Moves peer into lazy set and cleans up tracking state.
    fn add_lazy(&mut self, peer: PI) {
        self.eager_push_peers.remove(&peer);
        self.lazy_push_peers.insert(peer);
        self.duplicate_counts.remove(&peer);
        self.eager_promotion_times.remove(&peer);
    }

    /// Check if peer is within prune cooldown period.
    fn is_in_prune_cooldown(&self, peer: &PI, now: Instant) -> bool {
        self.eager_promotion_times
            .get(peer)
            .map(|promotion_time| {
                let elapsed = now.saturating_duration_since(*promotion_time);
                elapsed < self.config.prune_cooldown
            })
            .unwrap_or(false)
    }

    /// Remove expired promotion time entries.
    fn expire_promotion_times(&mut self, now: Instant) {
        self.eager_promotion_times.retain(|_peer, promotion_time| {
            let elapsed = now.saturating_duration_since(*promotion_time);
            elapsed < self.config.prune_cooldown
        });
    }

    /// Handle duplicate message receipt - decide whether to prune based on config.
    fn maybe_prune_on_duplicate(
        &mut self,
        sender: PI,
        message_id: &MessageId,
        now: Instant,
        io: &mut impl IO<PI>,
    ) {
        // Never prune for relays of our own messages
        if self.originated_messages.contains_key(message_id) {
            debug!("skip prune: originated message");
            return;
        }

        // Don't prune during cooldown period
        if self.is_in_prune_cooldown(&sender, now) {
            debug!("skip prune: cooldown");
            return;
        }

        // Don't prune below minimum eager peers
        if self.eager_push_peers.len() <= self.config.min_eager_peers {
            debug!("skip prune: min_eager_peers");
            return;
        }

        // Check prune mode
        match self.config.prune_mode {
            PruneMode::Immediate => {
                debug!("PRUNE: immediate mode");
                self.add_lazy(sender);
                io.push(OutEvent::SendMessage(sender, Message::Prune));
            }
            PruneMode::AfterDuplicates(threshold) => {
                let count = self.duplicate_counts.entry(sender).or_insert(0);
                *count += 1;
                debug!("duplicate count: {}/{}", count, threshold);
                if *count >= threshold {
                    debug!("PRUNE: after {} duplicates", count);
                    self.add_lazy(sender);
                    io.push(OutEvent::SendMessage(sender, Message::Prune));
                }
            }
            PruneMode::Disabled => {
                // Don't prune
            }
        }
    }

    /// Immediately sends message to eager peers.
    fn eager_push(&mut self, gossip: Gossip, sender: &PI, io: &mut impl IO<PI>) {
        for peer in self
            .eager_push_peers
            .iter()
            .filter(|peer| **peer != self.me && *peer != sender)
        {
            io.push(OutEvent::SendMessage(
                *peer,
                Message::Gossip(gossip.clone()),
            ));
        }
    }

    /// Queue lazy message announcements into the queue that will be sent out as batched
    /// [`Message::IHave`] messages once the [`Timer::DispatchLazyPush`] timer is triggered.
    fn lazy_push(&mut self, gossip: Gossip, sender: &PI, io: &mut impl IO<PI>) {
        let Some(round) = gossip.round() else {
            return;
        };
        for peer in self.lazy_push_peers.iter().filter(|x| *x != sender) {
            self.lazy_push_queue.entry(*peer).or_default().push(IHave {
                id: gossip.id,
                round,
            });
        }
        if !self.dispatch_timer_scheduled {
            io.push(OutEvent::ScheduleTimer(
                self.config.dispatch_timeout,
                Timer::DispatchLazyPush,
            ));
            self.dispatch_timer_scheduled = true;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::proto::topic;

    fn has_prune_to<PI: PartialEq>(io: &VecDeque<topic::OutEvent<PI>>, peer: PI) -> bool {
        io.iter().any(|e| {
            matches!(e, topic::OutEvent::SendMessage(to, topic::Message::Gossip(Message::Prune)) if *to == peer)
        })
    }

    fn has_any_prune<PI>(io: &VecDeque<topic::OutEvent<PI>>) -> bool {
        io.iter().any(|e| {
            matches!(
                e,
                topic::OutEvent::SendMessage(_, topic::Message::Gossip(Message::Prune))
            )
        })
    }

    #[test]
    fn optimize_tree() {
        let mut io = VecDeque::new();
        // Use Immediate mode and disable min_eager_peers for this unit test
        let config = Config {
            prune_mode: PruneMode::Immediate,
            min_eager_peers: 0,
            ..Default::default()
        };
        let mut state = State::new(1, config.clone(), 1024);
        let now = Instant::now();

        // we receive an IHave message from peer 2
        // it has `round: 2` which means that the the peer that sent us the IHave was
        // two hops away from the original sender of the message
        let content: Bytes = b"hi".to_vec().into();
        let id = MessageId::from_content(&content);
        let event = InEvent::RecvMessage(
            2u32,
            Message::IHave(vec![IHave {
                id,
                round: Round(2),
            }]),
        );
        state.handle(event, now, &mut io);
        io.clear();
        // we then receive a `Gossip` message with the same `MessageId` from peer 3
        // the message has `round: 6`, which means it travelled 6 hops until it reached us
        // this is less hops than to peer 2, but not enough to trigger the optimization
        // because we use the default config which has `optimization_threshold: 7`
        let event = InEvent::RecvMessage(
            3,
            Message::Gossip(Gossip {
                id,
                content: content.clone(),
                scope: DeliveryScope::Swarm(Round(6)),
            }),
        );
        state.handle(event, now, &mut io);
        let expected = {
            // we expect a dispatch timer schedule and receive event, but no Graft or Prune
            // messages
            let mut io = VecDeque::new();
            io.push(OutEvent::ScheduleTimer(
                config.dispatch_timeout,
                Timer::DispatchLazyPush,
            ));
            io.push(OutEvent::EmitEvent(Event::Received(GossipEvent {
                content,
                delivered_from: 3,
                scope: DeliveryScope::Swarm(Round(6)),
            })));
            io
        };
        assert_eq!(io, expected);
        io.clear();

        // now we run the same flow again but this time peer 3 is 9 hops away from the message's
        // sender. message's sender. this will trigger the optimization:
        // peer 2 will be promoted to eager and peer 4 demoted to lazy

        let content: Bytes = b"hi2".to_vec().into();
        let id = MessageId::from_content(&content);
        let event = InEvent::RecvMessage(
            2u32,
            Message::IHave(vec![IHave {
                id,
                round: Round(2),
            }]),
        );
        state.handle(event, now, &mut io);
        io.clear();

        let event = InEvent::RecvMessage(
            3,
            Message::Gossip(Gossip {
                id,
                content: content.clone(),
                scope: DeliveryScope::Swarm(Round(9)),
            }),
        );
        state.handle(event, now, &mut io);
        let expected = {
            // this time we expect the Graft and Prune messages to be sent, performing the
            // optimization step
            let mut io = VecDeque::new();
            io.push(OutEvent::SendMessage(
                2,
                Message::Graft(Graft {
                    id: None,
                    round: Round(2),
                }),
            ));
            io.push(OutEvent::SendMessage(3, Message::Prune));
            io.push(OutEvent::EmitEvent(Event::Received(GossipEvent {
                content,
                delivered_from: 3,
                scope: DeliveryScope::Swarm(Round(9)),
            })));
            io
        };
        assert_eq!(io, expected);
    }

    #[test]
    fn spoofed_messages_are_ignored() {
        let config: Config = Default::default();
        let mut state = State::new(1, config.clone(), 1024);
        let now = Instant::now();

        // we recv a correct gossip message and expect the Received event to be emitted
        let content: Bytes = b"hello1".to_vec().into();
        let message = Message::Gossip(Gossip {
            content: content.clone(),
            id: MessageId::from_content(&content),
            scope: DeliveryScope::Swarm(Round(1)),
        });
        let mut io = VecDeque::new();
        state.handle(InEvent::RecvMessage(2, message), now, &mut io);
        let expected = {
            let mut io = VecDeque::new();
            io.push(OutEvent::ScheduleTimer(
                config.cache_evict_interval,
                Timer::EvictCache,
            ));
            io.push(OutEvent::ScheduleTimer(
                config.dispatch_timeout,
                Timer::DispatchLazyPush,
            ));
            io.push(OutEvent::EmitEvent(Event::Received(GossipEvent {
                content,
                delivered_from: 2,
                scope: DeliveryScope::Swarm(Round(1)),
            })));
            io
        };
        assert_eq!(io, expected);

        // now we recv with a spoofed id and expect no event to be emitted
        let content: Bytes = b"hello2".to_vec().into();
        let message = Message::Gossip(Gossip {
            content,
            id: MessageId::from_content(b"foo"),
            scope: DeliveryScope::Swarm(Round(1)),
        });
        let mut io = VecDeque::new();
        state.handle(InEvent::RecvMessage(2, message), now, &mut io);
        let expected = VecDeque::new();
        assert_eq!(io, expected);
    }

    #[test]
    fn cache_is_evicted() {
        let config: Config = Default::default();
        let mut state = State::new(1, config.clone(), 1024);
        let now = Instant::now();
        let content: Bytes = b"hello1".to_vec().into();
        let message = Message::Gossip(Gossip {
            content: content.clone(),
            id: MessageId::from_content(&content),
            scope: DeliveryScope::Swarm(Round(1)),
        });
        let mut io = VecDeque::new();
        state.handle(InEvent::RecvMessage(2, message), now, &mut io);
        assert_eq!(state.cache.len(), 1);

        let now = now + Duration::from_secs(1);
        state.handle(InEvent::TimerExpired(Timer::EvictCache), now, &mut io);
        assert_eq!(state.cache.len(), 1);

        let now = now + config.message_cache_retention;
        state.handle(InEvent::TimerExpired(Timer::EvictCache), now, &mut io);
        assert_eq!(state.cache.len(), 0);
    }

    #[test]
    fn no_prune_for_own_message_relay() {
        let config = Config::default();
        let mut state = State::new(1u32, config.clone(), 1024);
        let mut io = VecDeque::new();
        let now = Instant::now();

        state.handle(InEvent::NeighborUp(2), now, &mut io);
        io.clear();

        // Broadcast a message (this adds it to originated_messages)
        let content: Bytes = b"my_message".to_vec().into();
        state.handle(
            InEvent::Broadcast(content.clone(), Scope::Swarm),
            now,
            &mut io,
        );
        io.clear();

        // Simulate peer 2 relaying our message back to us (duplicate)
        let id = MessageId::from_content(&content);
        let message = Message::Gossip(Gossip {
            id,
            content: content.clone(),
            scope: DeliveryScope::Swarm(Round(1)),
        });
        state.handle(InEvent::RecvMessage(2, message), now, &mut io);

        assert!(
            !has_any_prune(&io),
            "Should not prune peer for relaying our own message"
        );

        // Peer 2 should still be in eager set
        assert!(state.eager_push_peers.contains(&2));
    }

    #[test]
    fn prune_cooldown_prevents_immediate_prune() {
        let config = Config {
            prune_mode: PruneMode::Immediate,
            prune_cooldown: Duration::from_millis(100),
            min_eager_peers: 0,
            ..Default::default()
        };
        let mut state = State::new(1u32, config.clone(), 1024);
        let mut io = VecDeque::new();
        let now = Instant::now();

        // Add peer 2 as eager neighbor
        state.handle(InEvent::NeighborUp(2), now, &mut io);
        io.clear();

        // Receive a message from peer 3 (not 2)
        let content: Bytes = b"msg1".to_vec().into();
        let id = MessageId::from_content(&content);
        state.handle(
            InEvent::RecvMessage(
                3,
                Message::Gossip(Gossip {
                    id,
                    content: content.clone(),
                    scope: DeliveryScope::Swarm(Round(1)),
                }),
            ),
            now,
            &mut io,
        );
        io.clear();

        // Now receive same message from peer 2 (duplicate) immediately
        state.handle(
            InEvent::RecvMessage(
                2,
                Message::Gossip(Gossip {
                    id,
                    content: content.clone(),
                    scope: DeliveryScope::Swarm(Round(2)),
                }),
            ),
            now,
            &mut io,
        );

        // Should NOT prune peer 2 because of cooldown (just added as neighbor)
        assert!(
            !has_prune_to(&io, 2),
            "Should not prune peer during cooldown period"
        );

        // After cooldown expires, should prune
        io.clear();
        let later = now + Duration::from_millis(150);

        // Receive another message from peer 3
        let content2: Bytes = b"msg2".to_vec().into();
        let id2 = MessageId::from_content(&content2);
        state.handle(
            InEvent::RecvMessage(
                3,
                Message::Gossip(Gossip {
                    id: id2,
                    content: content2.clone(),
                    scope: DeliveryScope::Swarm(Round(1)),
                }),
            ),
            later,
            &mut io,
        );
        io.clear();

        // Receive duplicate from peer 2 after cooldown
        state.handle(
            InEvent::RecvMessage(
                2,
                Message::Gossip(Gossip {
                    id: id2,
                    content: content2.clone(),
                    scope: DeliveryScope::Swarm(Round(2)),
                }),
            ),
            later,
            &mut io,
        );

        // Now should prune
        assert!(
            has_prune_to(&io, 2),
            "Should prune peer after cooldown expires"
        );
    }

    #[test]
    fn prune_mode_disabled() {
        let config = Config {
            prune_mode: PruneMode::Disabled,
            prune_cooldown: Duration::ZERO,
            ..Default::default()
        };
        let mut state = State::new(1u32, config.clone(), 1024);
        let mut io = VecDeque::new();
        let now = Instant::now();

        // Add peer 2 as eager neighbor
        let old_time = now - Duration::from_secs(10);
        state.handle(InEvent::NeighborUp(2), old_time, &mut io);
        io.clear();

        // Receive a message from peer 3
        let content: Bytes = b"test".to_vec().into();
        let id = MessageId::from_content(&content);
        state.handle(
            InEvent::RecvMessage(
                3,
                Message::Gossip(Gossip {
                    id,
                    content: content.clone(),
                    scope: DeliveryScope::Swarm(Round(1)),
                }),
            ),
            now,
            &mut io,
        );
        io.clear();

        // Receive duplicate from peer 2
        state.handle(
            InEvent::RecvMessage(
                2,
                Message::Gossip(Gossip {
                    id,
                    content: content.clone(),
                    scope: DeliveryScope::Swarm(Round(2)),
                }),
            ),
            now,
            &mut io,
        );

        // Should NOT prune because mode is Disabled
        assert!(
            !has_any_prune(&io),
            "PruneMode::Disabled should prevent pruning"
        );
        assert!(
            state.eager_push_peers.contains(&2),
            "Peer should remain eager"
        );
    }

    #[test]
    fn prune_mode_after_duplicates() {
        let config = Config {
            prune_mode: PruneMode::AfterDuplicates(3),
            prune_cooldown: Duration::ZERO,
            min_eager_peers: 0,
            ..Default::default()
        };
        let mut state = State::new(1u32, config.clone(), 1024);
        let mut io = VecDeque::new();
        let now = Instant::now();

        // Add peer 2 as eager neighbor
        let old_time = now - Duration::from_secs(10);
        state.handle(InEvent::NeighborUp(2), old_time, &mut io);
        io.clear();

        // Send 3 different messages, each will trigger a duplicate from peer 2
        for i in 0..3 {
            let content: Bytes = format!("msg{i}").into_bytes().into();
            let id = MessageId::from_content(&content);

            // First, receive from peer 3 (original)
            state.handle(
                InEvent::RecvMessage(
                    3,
                    Message::Gossip(Gossip {
                        id,
                        content: content.clone(),
                        scope: DeliveryScope::Swarm(Round(1)),
                    }),
                ),
                now,
                &mut io,
            );
            io.clear();

            // Then receive duplicate from peer 2
            state.handle(
                InEvent::RecvMessage(
                    2,
                    Message::Gossip(Gossip {
                        id,
                        content: content.clone(),
                        scope: DeliveryScope::Swarm(Round(2)),
                    }),
                ),
                now,
                &mut io,
            );

            if i < 2 {
                assert!(
                    !has_prune_to(&io, 2),
                    "Should not prune before threshold (dup {})",
                    i + 1
                );
            } else {
                assert!(
                    has_prune_to(&io, 2),
                    "Should prune on reaching threshold (dup {})",
                    i + 1
                );
            }
            io.clear();
        }
    }
}
