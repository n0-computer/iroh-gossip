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
use indexmap::IndexMap;
use n0_future::time::{Duration, Instant};
use postcard::experimental::max_size::MaxSize;
use rustc_hash::FxBuildHasher;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::{
    topic::{self, Event, IO},
    util::{idbytes_impls, TimeBoundCache},
    PeerIdentity, Verdict,
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

    /// Returns the first 5 bytes as hex, a short form for logs.
    pub fn fmt_short(&self) -> String {
        data_encoding::HEXLOWER.encode(&self.as_bytes()[..5])
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
    /// Announce again the messages that a full send queue dropped.
    AnnounceDropped,
}

#[derive(Clone, derive_more::Debug, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct GossipEvent<PI> {
    /// The content of the gossip message.
    #[debug("<{}b>", content.len())]
    pub content: Bytes,
    /// The peer that delivered the message to us.
    ///
    /// This is the peer before us on the path of the message, not the peer that broadcast it.
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
    /// Returns the round to forward a message with, one hop higher.
    ///
    /// The round stops at its maximum. A peer can send any round.
    pub fn next(&self) -> Round {
        Round(self.0.saturating_add(1))
    }
}

/// Messages that we can send and receive from peers within the topic.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Message {
    /// Carries the content of a message.
    ///
    /// A receiver emits it as an event, forwards it to its eager peers, and later announces its id
    /// to its lazy peers.
    Gossip(Gossip),
    /// Moves the sender from the eager to the lazy set of the tree.
    Prune(TreeId),
    /// Moves the sender to the eager set of the tree, and requests the message it names.
    Graft(Graft),
    /// Announces the ids of messages that the sender has.
    ///
    /// A receiver waits, and requests a message with a `Graft` if no eager peer pushed it in time.
    IHave(Vec<IHave>),
}

#[cfg(test)]
impl Message {
    /// Returns a payload with `content`, as its publisher sends it.
    ///
    /// The network and simulator tests use it.
    pub(crate) fn test_payload(content: Bytes) -> Self {
        Message::Gossip(Gossip {
            id: MessageId::from_content(&content),
            content,
            scope: DeliveryScope::Swarm(Round(0)),
            tree: TreeId::default(),
        })
    }
}

#[cfg(all(test, feature = "net"))]
impl Message {
    /// Returns a payload of `len` bytes, as an eager push sends it, for the network tests.
    pub(crate) fn test_gossip(len: usize) -> Self {
        Self::test_payload(Bytes::from(vec![7u8; len]))
    }

    /// Returns an `IHave` of one message, for the network tests.
    pub(crate) fn test_ihave() -> Self {
        Message::IHave(vec![IHave {
            id: MessageId::from_content(b"announced"),
            round: Round(0),
            tree: TreeId::default(),
        }])
    }

    /// Returns a `Prune`, for the network tests.
    pub(crate) fn test_prune() -> Self {
        Message::Prune(TreeId::default())
    }
}

/// Identifies the broadcast tree a message travels on.
///
/// A sender uses the shared tree `TreeId::default()` for its first
/// [`Config::source_tree_after`] broadcasts. From the next one on, it uses a tree of
/// its own, named with [`Self::of`]. A duplicate of one tree's message only
/// prunes that tree. The origin names the tree, and every message of it carries
/// the id: receivers copy it and never compute it. Two origins whose ids collide
/// share a tree, which costs latency but not delivery.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, MaxSize)]
pub struct TreeId([u8; 8]);

impl TreeId {
    /// Returns the id of the tree that `origin` broadcasts on.
    ///
    /// The id is the first 8 bytes of the BLAKE3 hash of the encoded `origin`. An endpoint
    /// id encodes as its 32 bytes.
    pub fn of<PI: Serialize>(origin: &PI) -> Self {
        let encoded = postcard::to_stdvec(origin).expect("a peer id serializes");
        let hash = blake3::hash(&encoded);
        Self(hash.as_bytes()[..8].try_into().expect("8 of 32 bytes"))
    }
}

/// Payload messages transmitted by the protocol.
///
/// The id is not on the wire. A receiver hashes the content, see [`Gossip::new`].
#[derive(Clone, derive_more::Debug, PartialEq, Eq)]
pub struct Gossip {
    /// Id of the message, the hash of its content.
    pub(crate) id: MessageId,
    /// Message contents.
    #[debug("<{}b>", content.len())]
    pub(crate) content: Bytes,
    /// Scope to broadcast to.
    pub(crate) scope: DeliveryScope,
    /// Tree the message travels on. A message to the neighbors travels on none.
    pub(crate) tree: TreeId,
}

impl Serialize for Gossip {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        (&self.content, &self.scope, &self.tree).serialize(s)
    }
}

impl<'de> Deserialize<'de> for Gossip {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let (content, scope, tree) = Deserialize::deserialize(d)?;
        Ok(Self::new(content, scope, tree))
    }
}

impl Gossip {
    /// Returns a message with `content`, and its id as the hash of the content.
    pub(crate) fn new(content: Bytes, scope: DeliveryScope, tree: TreeId) -> Self {
        let id = MessageId::from_content(&content);
        Self {
            id,
            content,
            scope,
            tree,
        }
    }

    fn round(&self) -> Option<Round> {
        match self.scope {
            DeliveryScope::Swarm(round) => Some(round),
            DeliveryScope::Neighbors => None,
        }
    }

    /// Returns the announcement of this message, if it is a broadcast to the swarm.
    fn ihave(&self) -> Option<IHave> {
        let round = self.round()?;
        Some(IHave {
            id: self.id,
            round,
            tree: self.tree,
        })
    }
}

/// The scope to deliver the message to.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Copy)]
#[non_exhaustive]
pub enum DeliveryScope {
    /// The message comes from the swarm, with the number of hops from the broadcaster.
    Swarm(Round),
    /// The message comes from a direct neighbor that sent it to its neighbors only.
    Neighbors,
}

impl DeliveryScope {
    /// Whether this message was directly received from its publisher.
    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Neighbors | Self::Swarm(Round(0)))
    }
}

/// Returns the largest wire size of a [`Message::Gossip`] with `content_len` bytes of content.
///
/// A broadcast to the swarm is a type code, the content with its length prefix,
/// the round and the tree. Each hop adds one to the round, so the encoding grows on the
/// way. This assumes the largest round. A broadcast to neighbors has no round.
pub(crate) fn max_gossip_size(content_len: usize, scope: Scope) -> usize {
    let type_size = 1;
    let prefix_size = (usize::BITS - content_len.leading_zeros())
        .div_ceil(7)
        .max(1) as usize;
    let round_size = match scope {
        // A varint of `u16::MAX`, and the tree.
        Scope::Swarm => 3 + size_of::<TreeId>(),
        Scope::Neighbors => 0,
    };
    type_size + prefix_size + content_len + round_size
}

/// The broadcast scope of a gossip message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Copy)]
#[non_exhaustive]
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
                tree: self.tree,
            }),
        }
    }
}

/// Control message to inform peers we have a message without transmitting the whole payload.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, MaxSize)]
pub struct IHave {
    /// Id of the message.
    pub(crate) id: MessageId,
    /// Delivery round of the message.
    pub(crate) round: Round,
    /// Tree the message travels on.
    pub(crate) tree: TreeId,
}

/// Tells a peer that it is eager in a tree, and asks it to make us eager too.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Graft {
    /// The id of the message that the graft requests, if any.
    ///
    /// If the id is set, the receiver sends that message in reply.
    pub(crate) id: Option<MessageId>,
    /// Tree to graft the peer on.
    pub(crate) tree: TreeId,
}

/// Configuration for the gossip broadcast layer.
///
/// Currently, the expectation is that the configuration is the same for all peers in the
/// network (as recommended in the paper).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct Config {
    /// How long we wait for a message after its `IHave`, before we request it.
    ///
    /// When the timeout expires without the message, we send a `Graft` to the peer that sent the
    /// `IHave`.
    ///
    /// The plumtree paper notes:
    /// > The timeout value is a protocol parameter that should be configured considering the
    /// > diameter of the overlay and a target maximum recovery latency, defined by the application
    /// > requirements. (p.8)
    pub graft_timeout_1: Duration,
    /// How long we wait for the reply to a `Graft`, before we ask the next peer.
    ///
    /// When the timeout expires without a reply, we send a `Graft` to the next peer that sent us an
    /// `IHave` for the message.
    ///
    /// The plumtree paper notes:
    /// > This second timeout value should be smaller that the first, in the order of an average
    /// > round trip time to a neighbor.
    pub graft_timeout_2: Duration,
    /// Timeout after which `IHave` messages are pushed to peers.
    pub dispatch_timeout: Duration,
    /// How many hops closer to the origin a lazy peer must be, before it replaces an eager peer.
    ///
    /// The protocol promotes a lazy peer to eager if its `Message::IHave` messages come with fewer
    /// hops from the origin than the messages of our eager peers.
    pub optimization_threshold: Round,

    /// Duration for which to keep gossip messages in the internal message cache.
    ///
    /// Messages broadcast from this node or received from other nodes are kept in an internal
    /// cache for this duration before being evicted. If this is too low, other nodes will not be
    /// able to retrieve messages once they need them. If this is high, the cache will grow.
    ///
    /// Should be at least around several round trip times to peers.
    ///
    /// The cache, the message ids and the verdicts of neighbor messages are bounded by time
    /// only, not by count. Their size grows with the rate at which peers send messages, times
    /// the retention. A peer cannot send faster than its connection and the send queues of the
    /// swarm allow, and a message filter can drop what a peer sends before it enters the cache.
    /// A node that takes data from peers it does not trust needs a filter, or a shorter retention.
    pub message_cache_retention: Duration,

    /// Duration for which to keep the `MessageId`s for received messages.
    ///
    /// Should be at least as long as [`Self::message_cache_retention`], usually will be longer to
    /// not accidentally receive messages multiple times.
    pub message_id_retention: Duration,

    /// How often the internal caches will be checked for expired items.
    pub cache_evict_interval: Duration,

    /// How many of our own broadcasts go on the shared tree before we get a tree of our own.
    ///
    /// Our first `source_tree_after` broadcasts name the shared tree [`TreeId::default()`], which
    /// every node uses. From the next one on we name `TreeId::of(me)`. A node that first sees
    /// that tree starts it as a copy of its shared tree, so the switch costs no flood. The tree
    /// then diverges through the prunes and grafts of our traffic, like the paper's tree per
    /// sender. The count starts again after we have not broadcast for five minutes.
    ///
    /// `0` gives every sender its own tree from its first message, which has to flood because
    /// there is no shared tree to copy yet. A very high value gives one shared tree.
    pub source_tree_after: u32,
}

impl Default for Config {
    /// Returns the default plumtree configuration.
    ///
    /// The paper gives no numbers for the timeouts, so we chose them for typical
    /// latencies of tens of milliseconds.
    fn default() -> Self {
        Self {
            // Paper: "The timeout value is a protocol parameter that should be configured considering
            // the diameter of the overlay and a target maximum recovery latency, defined by the
            // application requirements. This is a parameter that should be statically configured
            // at deployment time." (p. 8)
            // In the paper it is a few roundtrips.
            graft_timeout_1: Duration::from_millis(80),

            // Paper: "This second timeout value should be smaller that the first, in the order of an
            // average round trip time to a neighbor." (p. 9)
            graft_timeout_2: Duration::from_millis(40),

            // Again, paper does not tell a recommended number here. Likely should be quite small,
            // as to not delay messages without need. This would also be the time frame in which
            // `IHave`s are aggregated to save on packets.
            dispatch_timeout: Duration::from_millis(5),

            // This number comes from experiment settings the plumtree paper (p. 12)
            optimization_threshold: Round(7),

            // This is a certainly-high-enough value for usual operation.
            message_cache_retention: Duration::from_secs(30),
            message_id_retention: Duration::from_secs(90),
            cache_evict_interval: Duration::from_secs(1),

            // The first message builds the shared tree, and a sender's second one already
            // names its own tree, copied from the shared one. Measured in the simulator: lower
            // redundancy than 3 when all nodes send, equal when few do. See `TreeId`.
            source_tree_after: 1,
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
    /// Most hops that a message took to reach us, counting the link it came over.
    ///
    /// A message from its source took one hop. Plumtree 2.3 calls the largest
    /// value over all recipients the last delivery hop.
    pub max_last_delivery_hop: u16,
}

/// The grafts we sent for one message.
#[derive(Debug)]
struct Grafts<PI> {
    /// The peers we grafted, with the hops they announced, which their replies take too.
    peers: Vec<(PI, Round)>,
    /// The sender of the first copy, and the hops it took, once it came.
    first: Option<(PI, Round)>,
    /// The grafted peer that replaced the sender of the first copy.
    kept: Option<PI>,
}

impl<PI: PartialEq> Grafts<PI> {
    /// Returns whether we grafted `peer`.
    fn contains(&self, peer: &PI) -> bool {
        self.peers.iter().any(|(grafted, _)| grafted == peer)
    }
}

/// The most dropped messages per peer that we keep to announce again.
const MAX_DROPPED_PER_PEER: usize = 256;
/// The most dropped messages per topic that we keep to announce again.
///
/// At a tick, the ids move to the queue of announcements, which has no cap.
const MAX_DROPPED: usize = 4096;
/// The most dropped messages that we announce to a peer again per [`Timer::AnnounceDropped`].
const DROPPED_PER_TICK: usize = 64;
/// The time between two [`Timer::AnnounceDropped`].
const ANNOUNCE_DROPPED_INTERVAL: Duration = Duration::from_secs(1);

/// The most trees we keep per topic. A tree past the cap replaces the least recently used one.
///
/// A tree takes about 1.4 KB with 32 byte peer ids and 11 neighbors, so the cap
/// bounds a topic at about 1.4 MB. A dropped tree starts again as a copy of the
/// shared tree.
const MAX_TREES: usize = 1024;
/// How long we keep a tree that no message travels on.
///
/// The shared tree stays. A new tree copies it, and without it a new tree has every
/// neighbor eager and floods.
const TREE_RETENTION: Duration = Duration::from_secs(300);

/// One node's part of a broadcast tree: which neighbors it pushes payloads to.
#[derive(Debug)]
pub(crate) struct Tree<PI> {
    /// Neighbors we push payloads to.
    pub(crate) eager: BTreeSet<PI>,
    /// Neighbors we only announce payloads to.
    pub(crate) lazy: BTreeSet<PI>,
    /// When a message last travelled on the tree.
    last_used: Instant,
    /// When we last pruned a neighbor from the tree, see [`State::count_duplicate`].
    last_prune: Option<Instant>,
    /// The message that created the tree, if one did.
    first_message: Option<MessageId>,
}

/// A message we only know from announcements.
#[derive(Debug)]
struct Missing<PI> {
    /// The peers that announced it, with the hops they announced.
    announcers: VecDeque<(PI, Round)>,
    /// The tree it travels on.
    tree: TreeId,
}

/// State of the plumtree.
#[derive(Debug)]
pub struct State<PI> {
    /// Our address.
    me: PI,
    /// How many broadcasts of our own went out since the tree we send on was last idle.
    sent: u32,
    /// When we last broadcast.
    last_sent: Option<Instant>,
    /// Configuration for this plumtree.
    config: Config,

    /// Our neighbors in the membership layer.
    pub(crate) neighbors: BTreeSet<PI>,
    /// Our part of every broadcast tree we know, at most [`MAX_TREES`].
    pub(crate) trees: HashMap<TreeId, Tree<PI>>,

    lazy_push_queue: BTreeMap<PI, Vec<IHave>>,
    /// Messages that a full queue to the peer dropped, to announce to the peer again.
    ///
    /// They go out with [`Timer::AnnounceDropped`], not at once. A graft reply that
    /// the full queue drops again would otherwise loop at the speed of a round trip.
    dropped: BTreeMap<PI, IndexMap<MessageId, (Round, TreeId), FxBuildHasher>>,

    /// Messages that a [`Message::IHave`] announced, and that we did not receive yet.
    ///
    /// For each, we keep the peers that announced it.
    missing_messages: HashMap<MessageId, Missing<PI>>,
    /// The messages whose payload we saw, with the time we first saw them.
    received_messages: TimeBoundCache<MessageId, Instant, FxBuildHasher>,
    /// The verdicts on messages to the neighbors, which `received_messages` does not hold.
    ///
    /// The state uses it only with a filter. A copy of such a message then gets the verdict
    /// of the first one, and does not run the filter again.
    neighbor_verdicts: TimeBoundCache<MessageId, Verdict, FxBuildHasher>,
    /// The grafts we sent for a message, see [`Self::on_late_graft_reply`].
    ///
    /// Entries expire `message_cache_retention` after the first graft. A later
    /// reply is a plain duplicate.
    grafted: TimeBoundCache<MessageId, Grafts<PI>>,
    /// Payloads of received messages.
    cache: TimeBoundCache<MessageId, Gossip, FxBuildHasher>,

    /// Message ids for which a [`Timer::SendGraft`] has been scheduled.
    graft_timer_scheduled: HashSet<MessageId>,
    /// Whether a [`Timer::DispatchLazyPush`] has been scheduled.
    dispatch_timer_scheduled: bool,
    /// Whether a [`Timer::AnnounceDropped`] has been scheduled.
    announce_dropped_scheduled: bool,

    /// Set to false after the first message is received. Used for initial timer scheduling.
    init: bool,

    /// [`Stats`] of this plumtree.
    pub(crate) stats: Stats,

    max_message_size: usize,
}

impl<PI: PeerIdentity> State<PI> {
    /// Initialize the [`State`] of a plumtree.
    pub fn new(me: PI, config: Config, max_message_size: usize) -> Self {
        Self {
            me,
            sent: 0,
            last_sent: None,
            neighbors: Default::default(),
            trees: Default::default(),
            lazy_push_queue: Default::default(),
            dropped: Default::default(),
            config,
            missing_messages: Default::default(),
            received_messages: TimeBoundCache::with_hasher(FxBuildHasher),
            neighbor_verdicts: TimeBoundCache::with_hasher(FxBuildHasher),
            grafted: Default::default(),
            graft_timer_scheduled: Default::default(),
            dispatch_timer_scheduled: false,
            announce_dropped_scheduled: false,
            cache: TimeBoundCache::with_hasher(FxBuildHasher),
            init: false,
            stats: Default::default(),
            max_message_size,
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
            InEvent::TimerExpired(timer) => match timer {
                Timer::DispatchLazyPush => self.on_dispatch_timer(io),
                Timer::SendGraft(id) => {
                    self.on_send_graft_timer(id, now, io);
                }
                Timer::EvictCache => self.on_evict_cache_timer(now, io),
                Timer::AnnounceDropped => self.on_announce_dropped_timer(io),
            },
        }

        #[cfg(test)]
        self.check_invariants();
    }

    /// Panics if the per-peer bookkeeping disagrees with the trees.
    ///
    /// Runs after every event in the unit tests. Announcers in
    /// `missing_messages` may be peers we do not hold yet: views converge in
    /// their own time, and a graft makes them eager.
    #[cfg(test)]
    fn check_invariants(&self) {
        assert!(self.trees.len() <= MAX_TREES);
        for tree in self.trees.values() {
            assert!(
                tree.eager.is_disjoint(&tree.lazy),
                "a peer is eager and lazy"
            );
            let all: BTreeSet<_> = tree.eager.union(&tree.lazy).copied().collect();
            assert_eq!(all, self.neighbors, "a tree does not hold the neighbors");
        }
        for peer in self.lazy_push_queue.keys().chain(self.dropped.keys()) {
            assert!(
                self.neighbors.contains(peer),
                "announcements queued for {peer:?}, which is no peer"
            );
        }
        for (id, missing) in self.missing_messages.iter() {
            assert!(
                !missing.announcers.is_empty(),
                "no announcer left for missing {id:?}"
            );
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
        // A message from a non-neighbor is a straggler from a former one. Deliver
        // its payload, but let it change no peer set.
        let from_peer = self.neighbors.contains(&sender);
        match message {
            Message::Gossip(details) => self.on_gossip(sender, from_peer, details, now, io),
            _ if !from_peer => {}
            Message::Prune(tree) => self.add_lazy(tree, sender, now),
            Message::IHave(details) => self.on_ihave(sender, details, io),
            Message::Graft(details) => self.on_graft(sender, details, now, io),
        }
    }

    /// Dispatches messages from lazy queue over to lazy peers.
    fn on_dispatch_timer(&mut self, io: &mut impl IO<PI>) {
        let chunk_size = self.max_message_size
            // Space for discriminator
            - 1
            // Space for the length of the list: 5 bytes hold any length that fits a frame
            - 5;
        let chunk_len = chunk_size / IHave::POSTCARD_MAX_SIZE;
        while let Some((peer, list)) = self.lazy_push_queue.pop_first() {
            if list.is_empty() {
                continue;
            }
            // A batch that fits one frame can hand its allocation to the outgoing message.
            if list.len() <= chunk_len {
                self.send_ihave(peer, list, io);
                continue;
            }
            for chunk in list.chunks(chunk_len) {
                self.send_ihave(peer, chunk.to_vec(), io);
            }
        }

        self.dispatch_timer_scheduled = false;
    }

    /// Send a gossip message.
    ///
    /// Will be pushed in full to eager peers.
    /// Pushing the message id to the lazy peers is delayed by a timer.
    fn broadcast(&mut self, content: Bytes, scope: Scope, now: Instant, io: &mut impl IO<PI>) {
        // A message over the limit, at any hop, cannot go out, so we neither cache
        // nor announce it.
        if max_gossip_size(content.len(), scope) > self.max_message_size {
            warn!(max = self.max_message_size, "broadcast too large, dropped");
            return;
        }
        let scope = match scope {
            Scope::Neighbors => DeliveryScope::Neighbors,
            Scope::Swarm => DeliveryScope::Swarm(Round(0)),
        };
        let tree_id = self.tree_for_broadcast(scope, now);
        let message = Gossip::new(content, scope, tree_id);
        let id = message.id;
        let me = self.me;
        if let DeliveryScope::Swarm(_) = scope {
            self.received_messages
                .insert(id, now, now + self.config.message_id_retention);
            self.cache.insert(
                id,
                message.clone(),
                now + self.config.message_cache_retention,
            );
            self.lazy_push(&message, &me, now, io);
        }

        self.eager_push(&message, &me, &[], now, io);
    }

    /// Handle receiving a [`Message::Gossip`].
    fn on_gossip(
        &mut self,
        sender: PI,
        from_peer: bool,
        message: Gossip,
        now: Instant,
        io: &mut impl IO<PI>,
    ) {
        // if we already received this message: move peer to lazy set
        // and notify peer about this.
        if self.received_messages.contains_key(&message.id) {
            debug!(hash = %message.id.fmt_short(), "Received duplicate message... ignoring");
            if message.round().is_none() || self.on_late_graft_reply(sender, &message, now, io) {
                return;
            }
            // A non-neighbor that pushes to us may still hold us as an eager
            // peer, as an older version does. The `Prune` stops its pushes.
            if !from_peer || self.count_duplicate(sender, &message, now) {
                self.add_lazy(message.tree, sender, now);
                io.send(sender, Message::Prune(message.tree));
            }
        // otherwise store the message, emit to application and forward to peers
        } else {
            let verdict = self.verdict(sender, &message, now, io);
            if let DeliveryScope::Swarm(prev_round) = message.scope {
                // insert the message in the list of received messages
                self.received_messages.insert(
                    message.id,
                    now,
                    now + self.config.message_id_retention,
                );
                if verdict == Verdict::Reject {
                    // It counts as seen, and nothing more. We do not cache it, forward it,
                    // or graft for it.
                    self.graft_timer_scheduled.remove(&message.id);
                    self.missing_messages.remove(&message.id);
                    return;
                }
                // increase the round for forwarding the message, and add to cache
                // to reply to Graft messages later
                // TODO: add callback/event to application to get missing messages that were received before?
                let message = message.next_round().expect("just checked");

                self.cache.insert(
                    message.id,
                    message.clone(),
                    now + self.config.message_cache_retention,
                );
                self.tree(message.tree, now, Some(message.id));
                // Push the message to our peers. A peer we grafted for it
                // has it already, and sends it to us.
                // A graft reply over no more hops can change the tree, see
                // `on_late_graft_reply`. The tree optimization would only see
                // the announcements left after the graft.
                let (grafted, graft_out) = match self.grafted.get_mut(&message.id) {
                    Some(grafts) => {
                        grafts.first = Some((sender, prev_round));
                        let out = grafts.peers.iter().any(|(_, round)| *round <= prev_round)
                            && !grafts.contains(&sender);
                        let peers = grafts.peers.iter().map(|(peer, _)| *peer).collect();
                        (peers, out)
                    }
                    None => (Vec::new(), false),
                };
                // A graft cannot make its peer eager in a tree that we do not know yet.
                for peer in &grafted {
                    self.add_eager(message.tree, *peer, now);
                }
                self.eager_push(&message, &sender, &grafted, now, io);
                self.lazy_push(&message, &sender, now, io);
                // cleanup places where we track missing messages
                self.graft_timer_scheduled.remove(&message.id);
                let previous_ihaves = self.missing_messages.remove(&message.id);
                // do the optimization step from the paper
                if let Some(missing) = previous_ihaves.filter(|_| from_peer && !graft_out) {
                    self.optimize_tree(&sender, &message, missing.announcers, now, io);
                }
                self.stats.max_last_delivery_hop = self
                    .stats
                    .max_last_delivery_hop
                    .max(prev_round.0.saturating_add(1));
            }

            // emit event to application
            if verdict == Verdict::Accept {
                io.emit(Event::Received(GossipEvent::from_message(&message, sender)));
            }
        }
    }

    /// Counts a duplicate from the neighbor `sender`, and returns whether to prune it.
    ///
    /// The first duplicate from an eager neighbor prunes it, as in the paper, unless the
    /// message was first seen less than a graft timeout after our last prune in its tree. That
    /// message was in flight during the prune. Its duplicate may come from the cycle we just
    /// cut, and a second cut would split the tree. A link that alternates
    /// between first copy and duplicate is cut at its first duplicate. The duplicates of the
    /// message that created the tree come from its flood and always prune.
    fn count_duplicate(&mut self, sender: PI, message: &Gossip, now: Instant) -> bool {
        let first_seen = self.received_messages.get(&message.id).copied();
        let hold = self.config.graft_timeout_1;
        // A copy can name any tree, as the id covers only the content. Thus a copy creates
        // no tree.
        let Some(tree) = self.trees.get_mut(&message.tree) else {
            return true;
        };
        tree.last_used = now;
        if !tree.eager.contains(&sender) {
            return true;
        }
        let in_flight = tree.first_message != Some(message.id)
            && matches!((tree.last_prune, first_seen), (Some(prune), Some(seen)) if seen < prune + hold);
        if !in_flight {
            tree.last_prune = Some(now);
        }
        !in_flight
    }

    /// Returns the tree to name for a broadcast of ours, see [`Config::source_tree_after`].
    fn tree_for_broadcast(&mut self, scope: DeliveryScope, now: Instant) -> TreeId {
        if scope == DeliveryScope::Neighbors {
            return TreeId::default();
        }
        if self
            .last_sent
            .is_some_and(|last| last + TREE_RETENTION <= now)
        {
            self.sent = 0;
        }
        self.last_sent = Some(now);
        let own = self.sent >= self.config.source_tree_after;
        self.sent = self.sent.saturating_add(1);
        if own {
            TreeId::of(&self.me)
        } else {
            TreeId::default()
        }
    }

    /// Returns the verdict on a new message, and asks the filter at most once per message id.
    ///
    /// The state does not deduplicate messages to the neighbors, so it keeps the first verdict
    /// for their copies. A later copy to the swarm also gets that verdict.
    fn verdict(
        &mut self,
        sender: PI,
        message: &Gossip,
        now: Instant,
        io: &mut impl IO<PI>,
    ) -> Verdict {
        if let Some(verdict) = self.neighbor_verdicts.get(&message.id) {
            return *verdict;
        }
        let Some(verdict) = io.filter(message, sender) else {
            return Verdict::Accept;
        };
        if message.scope == DeliveryScope::Neighbors {
            let expires = now + self.config.message_id_retention;
            self.neighbor_verdicts.insert(message.id, verdict, expires);
        }
        verdict
    }

    /// Handles a duplicate that a peer we grafted sends after the eager copy came.
    ///
    /// The peer announced the message more than a graft timeout before the
    /// eager copy arrived, so its path is faster. If its copy took no more
    /// hops, it stays eager, and the sender of the eager copy turns lazy.
    /// Otherwise the graft and its duplicate would repeat for each message.
    /// The peer is not below us, as it announced the message before we had it.
    /// The hop check keeps [`Self::optimize_tree`] from undoing the change on
    /// the next message. A reply to a retried graft is ignored.
    ///
    /// Returns whether it handled the duplicate.
    fn on_late_graft_reply(
        &mut self,
        sender: PI,
        message: &Gossip,
        now: Instant,
        io: &mut impl IO<PI>,
    ) -> bool {
        let Some(grafts) = self.grafted.get_mut(&message.id) else {
            return false;
        };
        let Some((first, first_round)) = grafts.first else {
            return false;
        };
        if !grafts.contains(&sender) {
            return false;
        }
        // A retry brings a second reply from a peer that already gave us the message.
        if first == sender || grafts.kept == Some(sender) {
            return true;
        }
        let no_more_hops = message.round().is_some_and(|round| round <= first_round);
        let both_eager = self
            .trees
            .get(&message.tree)
            .is_some_and(|tree| tree.eager.contains(&sender) && tree.eager.contains(&first));
        if !no_more_hops || grafts.contains(&first) || !both_eager {
            return false;
        }
        grafts.kept = Some(sender);
        debug!(peer = ?sender, slower = ?first, "keep a faster grafted peer");
        self.add_lazy(message.tree, first, now);
        io.send(first, Message::Prune(message.tree));
        true
    }

    /// Optimizes the tree: prunes the `sender` of a [`Message::Gossip`] if a faster path exists.
    ///
    /// A path is faster if we received a [`Message::IHave`] for the same message with far fewer
    /// hops from the broadcaster.
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
                let tree = message.tree;
                if !self
                    .tree(tree, now, Some(message.id))
                    .eager
                    .contains(&ihave_peer)
                {
                    let message = Message::Graft(Graft { id: None, tree });
                    self.add_eager(tree, ihave_peer, now);
                    io.send(ihave_peer, message);
                }
                // Prune the sender of the Gossip.
                self.add_lazy(tree, *gossip_sender, now);
                io.send(*gossip_sender, Message::Prune(tree));
            }
        }
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
                // Queue each peer once, as it answers every graft. A new
                // announcement after a graft is a retry, for example after a
                // dropped reply.
                let missing = self
                    .missing_messages
                    .entry(ihave.id)
                    .or_insert_with(|| Missing {
                        announcers: VecDeque::new(),
                        tree: ihave.tree,
                    });
                if !missing.announcers.iter().any(|(peer, _)| *peer == sender) {
                    missing.announcers.push_back((sender, ihave.round));
                }

                if !self.graft_timer_scheduled.contains(&ihave.id) {
                    self.graft_timer_scheduled.insert(ihave.id);
                    io.schedule(self.config.graft_timeout_1, Timer::SendGraft(ihave.id));
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
            .and_then(|missing| Some((missing.tree, missing.announcers.pop_front()?)));
        if self
            .missing_messages
            .get(&id)
            .is_some_and(|missing| missing.announcers.is_empty())
        {
            self.missing_messages.remove(&id);
        }
        if let Some((tree, (peer, round))) = entry {
            self.add_eager(tree, peer, now);
            match self.grafted.get_mut(&id) {
                Some(grafts) => match grafts.peers.iter_mut().find(|(p, _)| *p == peer) {
                    Some((_, known)) => *known = (*known).min(round),
                    None => grafts.peers.push((peer, round)),
                },
                None => {
                    let grafts = Grafts {
                        peers: vec![(peer, round)],
                        first: None,
                        kept: None,
                    };
                    let expires = now + self.config.message_cache_retention;
                    self.grafted.insert(id, grafts, expires);
                }
            }
            let message = Message::Graft(Graft { id: Some(id), tree });
            io.send(peer, message);

            // "when a GRAFT message is sent, another timer is started to expire after a certain timeout,
            // to ensure that the message will be requested to another neighbor if it is not received
            // meanwhile. This second timeout value should be smaller that the first, in the order of
            // an average round trip time to a neighbor." (p9)
            // A new announcement does not start a second chain of grafts meanwhile.
            self.graft_timer_scheduled.insert(id);
            io.schedule(self.config.graft_timeout_2, Timer::SendGraft(id));
        }
    }

    /// Handle receiving a [`Message::Graft`].
    fn on_graft(&mut self, sender: PI, details: Graft, now: Instant, io: &mut impl IO<PI>) {
        self.add_eager(details.tree, sender, now);
        if let Some(id) = details.id {
            if let Some(message) = self.cache.get(&id) {
                let ihave = message.ihave();
                if io.send(sender, Message::Gossip(message.clone())).is_some() {
                    self.note_dropped(sender, ihave, io);
                }
            } else {
                debug!(?id, peer=?sender, "on_graft failed to graft: message not in cache");
            }
        }
    }

    /// Makes a new neighbor an eager peer.
    ///
    /// The peer starts eager in every tree.
    pub(super) fn on_neighbor_up(&mut self, peer: PI) {
        self.neighbors.insert(peer);
        for tree in self.trees.values_mut() {
            tree.lazy.remove(&peer);
            tree.eager.insert(peer);
        }
    }

    /// Forgets a neighbor that left the topic.
    ///
    /// > When a neighbor is detected to leave the overlay, it is simple removed from the
    /// > membership. Furthermore, the record of IHAVE messages sent from failed members is deleted
    /// > from the missing history. (p9)
    pub(super) fn on_neighbor_down(&mut self, peer: PI) {
        self.missing_messages.retain(|_message_id, missing| {
            missing
                .announcers
                .retain(|(ihave_peer, _round)| *ihave_peer != peer);
            !missing.announcers.is_empty()
        });
        self.neighbors.remove(&peer);
        for tree in self.trees.values_mut() {
            tree.eager.remove(&peer);
            tree.lazy.remove(&peer);
        }
        // The dispatch timer would otherwise send these, and the network layer
        // would dial the peer to deliver them.
        self.lazy_push_queue.remove(&peer);
        self.dropped.remove(&peer);
        // A peer that comes back has lost its state, so it needs our pushes again.
        for grafts in self.grafted.values_mut() {
            grafts.peers.retain(|(grafted, _)| *grafted != peer);
        }
    }

    /// Sends an announcement, and notes it for later if the queue dropped it.
    fn send_ihave(&mut self, peer: PI, ihaves: Vec<IHave>, io: &mut impl IO<PI>) {
        if let Some(topic::Message::Gossip(Message::IHave(ihaves))) =
            io.send(peer, Message::IHave(ihaves))
        {
            self.note_dropped(peer, ihaves, io);
        }
    }

    /// Notes a dropped push or announcement, to announce it to the peer later.
    ///
    /// A dropped neighbor message is lost, as we do not cache it.
    fn note_dropped(
        &mut self,
        peer: PI,
        ihaves: impl IntoIterator<Item = IHave>,
        io: &mut impl IO<PI>,
    ) {
        if !self.neighbors.contains(&peer) {
            return;
        }
        let mut total: usize = self.dropped.values().map(IndexMap::len).sum();
        let ids = self.dropped.entry(peer).or_default();
        for IHave { id, round, tree } in ihaves {
            if !self.cache.contains_key(&id) || ids.contains_key(&id) {
                continue;
            }
            // Past the caps, the peer misses the message unless another peer has it.
            if ids.len() >= MAX_DROPPED_PER_PEER || total >= MAX_DROPPED {
                debug!("too many dropped messages to announce again, drop {id}");
                continue;
            }
            ids.insert(id, (round, tree));
            total += 1;
        }
        if ids.is_empty() {
            self.dropped.remove(&peer);
        }
        self.schedule_announce_dropped(io);
    }

    fn schedule_announce_dropped(&mut self, io: &mut impl IO<PI>) {
        if !self.dropped.is_empty() && !self.announce_dropped_scheduled {
            io.schedule(ANNOUNCE_DROPPED_INTERVAL, Timer::AnnounceDropped);
            self.announce_dropped_scheduled = true;
        }
    }

    /// Announces the oldest dropped messages per peer, so that one tick sends a bounded amount.
    fn on_announce_dropped_timer(&mut self, io: &mut impl IO<PI>) {
        self.announce_dropped_scheduled = false;
        for (peer, ids) in self.dropped.iter_mut() {
            // A peer cannot graft what we no longer cache.
            ids.retain(|id, _round| self.cache.contains_key(id));
            let ihaves = ids
                .drain(..ids.len().min(DROPPED_PER_TICK))
                .map(|(id, (round, tree))| IHave { id, round, tree });
            self.lazy_push_queue
                .entry(*peer)
                .or_default()
                .extend(ihaves);
        }
        self.dropped.retain(|_, ids| !ids.is_empty());
        self.lazy_push_queue.retain(|_, ihaves| !ihaves.is_empty());
        if !self.lazy_push_queue.is_empty() {
            self.schedule_dispatch(io);
        }
        self.schedule_announce_dropped(io);
    }

    fn on_evict_cache_timer(&mut self, now: Instant, io: &mut impl IO<PI>) {
        self.cache.expire_until(now);
        self.received_messages.expire_until(now);
        self.neighbor_verdicts.expire_until(now);
        self.grafted.expire_until(now);
        if let Some(shared) = self.trees.get_mut(&TreeId::default()) {
            shared.last_used = now;
        }
        self.trees
            .retain(|_, tree| tree.last_used + TREE_RETENTION > now);
        io.schedule(self.config.cache_evict_interval, Timer::EvictCache);
    }

    /// Returns our part of the tree `id`, and creates the tree if needed.
    ///
    /// A new tree is a copy of the shared tree, or has every neighbor eager if there is no
    /// shared tree.
    ///
    /// Past [`MAX_TREES`], the least recently used tree makes room.
    /// `first_message` is the message that creates the tree.
    fn tree(
        &mut self,
        id: TreeId,
        now: Instant,
        first_message: Option<MessageId>,
    ) -> &mut Tree<PI> {
        if !self.trees.contains_key(&id) && self.trees.len() >= MAX_TREES {
            let oldest = self
                .trees
                .iter()
                .filter(|(id, _)| **id != TreeId::default())
                .min_by_key(|(_, tree)| tree.last_used);
            if let Some(oldest) = oldest.map(|(id, _)| *id) {
                self.trees.remove(&oldest);
            }
        }
        if !self.trees.contains_key(&id) {
            // The copy makes the switch of a sender to its own tree cost no flood.
            let (eager, lazy) = match self.trees.get(&TreeId::default()) {
                Some(shared) => (shared.eager.clone(), shared.lazy.clone()),
                None => (self.neighbors.clone(), BTreeSet::new()),
            };
            let tree = Tree {
                eager,
                lazy,
                last_used: now,
                last_prune: None,
                first_message,
            };
            self.trees.insert(id, tree);
        }
        let tree = self.trees.get_mut(&id).expect("exists or just inserted");
        tree.last_used = now;
        tree
    }

    /// Moves a neighbor into the eager set of a known tree.
    ///
    /// It ignores a peer that is not a neighbor. A `Graft`, `Prune`, or
    /// `IHave` can still be in flight when the membership layer drops a peer,
    /// and handling it must not put the peer back: we handled its `NeighborDown`
    /// already, so nothing would remove the peer again.
    ///
    /// Only a payload creates a tree, see [`Self::tree`]. A peer could otherwise
    /// make up tree ids in small control messages, and evict the real trees.
    fn add_eager(&mut self, tree: TreeId, peer: PI, now: Instant) {
        if let Some(tree) = self.known_tree(tree, &peer, now) {
            tree.lazy.remove(&peer);
            tree.eager.insert(peer);
        }
    }

    /// Moves a neighbor into the lazy set of a known tree, see [`Self::add_eager`].
    fn add_lazy(&mut self, tree: TreeId, peer: PI, now: Instant) {
        if let Some(tree) = self.known_tree(tree, &peer, now) {
            tree.eager.remove(&peer);
            tree.lazy.insert(peer);
        }
    }

    /// Returns the tree `id` if `peer` is a neighbor and the tree exists or is the shared one.
    fn known_tree(&mut self, id: TreeId, peer: &PI, now: Instant) -> Option<&mut Tree<PI>> {
        if !self.neighbors.contains(peer) {
            return None;
        }
        if id == TreeId::default() {
            return Some(self.tree(id, now, None));
        }
        let tree = self.trees.get_mut(&id)?;
        tree.last_used = now;
        Some(tree)
    }

    /// Sends `gossip` at once to the eager peers of its tree.
    ///
    /// A message to the neighbors goes to all neighbors.
    /// A peer whose queue dropped the message gets it announced later.
    fn eager_push(
        &mut self,
        gossip: &Gossip,
        sender: &PI,
        skip: &[PI],
        now: Instant,
        io: &mut impl IO<PI>,
    ) {
        let me = self.me;
        let peers = match gossip.scope {
            DeliveryScope::Neighbors => &self.neighbors,
            DeliveryScope::Swarm(_) => &self.tree(gossip.tree, now, Some(gossip.id)).eager,
        };
        let mut dropped = Vec::new();
        for peer in peers
            .iter()
            .filter(|peer| **peer != me && *peer != sender && !skip.contains(peer))
        {
            if io.send(*peer, Message::Gossip(gossip.clone())).is_some() {
                dropped.push(*peer);
            }
        }
        if let Some(ihave) = gossip.ihave() {
            for peer in dropped {
                self.note_dropped(peer, [ihave.clone()], io);
            }
        }
    }

    /// Queues announcements for the lazy peers.
    ///
    /// They go out as batched [`Message::IHave`] messages when [`Timer::DispatchLazyPush`] fires.
    fn lazy_push(&mut self, gossip: &Gossip, sender: &PI, now: Instant, io: &mut impl IO<PI>) {
        let Some(ihave) = gossip.ihave() else {
            return;
        };
        self.tree(gossip.tree, now, Some(gossip.id));
        for peer in self.trees[&gossip.tree]
            .lazy
            .iter()
            .filter(|x| *x != sender)
        {
            self.lazy_push_queue
                .entry(*peer)
                .or_default()
                .push(ihave.clone());
        }
        self.schedule_dispatch(io);
    }

    fn schedule_dispatch(&mut self, io: &mut impl IO<PI>) {
        if !self.dispatch_timer_scheduled {
            io.schedule(self.config.dispatch_timeout, Timer::DispatchLazyPush);
            self.dispatch_timer_scheduled = true;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::proto::topic;

    /// The tree the tests send on.
    const T: TreeId = TreeId([0; 8]);

    /// Returns the eager peers of the tree [`T`].
    fn eager(state: &State<u32>) -> BTreeSet<u32> {
        state
            .trees
            .get(&T)
            .map(|tree| tree.eager.clone())
            .unwrap_or_default()
    }

    /// Returns the lazy peers of the tree [`T`].
    fn lazy(state: &State<u32>) -> BTreeSet<u32> {
        state
            .trees
            .get(&T)
            .map(|tree| tree.lazy.clone())
            .unwrap_or_default()
    }

    /// A late control frame of a former neighbor does not add it back to the broadcast view.
    ///
    /// Only a membership handshake does.
    #[test]
    fn late_prune_does_not_recreate_a_departed_peer() {
        let mut state = State::new(1u32, Config::default(), 1024);
        let now = Instant::now();
        let mut io = VecDeque::new();
        state.on_neighbor_up(2);
        state.on_neighbor_down(2);
        state.handle(InEvent::RecvMessage(2, Message::Prune(T)), now, &mut io);
        assert!(!eager(&state).contains(&2));
        assert!(!lazy(&state).contains(&2));
    }

    /// A round at its maximum from a peer goes out at its maximum.
    #[test]
    fn round_from_a_peer_does_not_overflow() {
        let mut state = State::new(1u32, Config::default(), 1024);
        state.on_neighbor_up(2);
        state.on_neighbor_up(3);
        let mut io = VecDeque::new();
        let message = Message::Gossip(gossip(b"x", u16::MAX));
        state.handle(InEvent::RecvMessage(2, message), Instant::now(), &mut io);
        let Some(Message::Gossip(out)) = pushed_to(&io, 3) else {
            panic!("no payload to 3");
        };
        assert_eq!(out.round(), Some(Round(u16::MAX)));
    }

    /// Returns a swarm-scope payload with `content`, as received after `round` hops.
    fn gossip(content: &'static [u8], round: u16) -> Gossip {
        let content = Bytes::from_static(content);
        Gossip {
            id: MessageId::from_content(&content),
            content,
            scope: DeliveryScope::Swarm(Round(round)),
            tree: T,
        }
    }

    /// Late frames of a departed neighbor change no peer set, and a reconnect restores it.
    ///
    /// A `Graft`, an `IHave`, or a duplicate payload can be in flight when the
    /// membership layer drops a peer. Before, each of them put the peer back
    /// into the eager or lazy set, and every later broadcast dialed it.
    #[test]
    fn late_frames_do_not_recreate_a_departed_peer() {
        let mut state = State::new(1u32, Config::default(), 1024);
        let now = Instant::now();
        let mut io = VecDeque::new();
        state.on_neighbor_up(2);
        state.on_neighbor_up(3);
        let seen = gossip(b"seen", 1);
        state.handle(
            InEvent::RecvMessage(3, Message::Gossip(seen.clone())),
            now,
            &mut io,
        );
        state.on_neighbor_down(2);
        let unknown = MessageId::from_content(b"unknown");
        let late = [
            Message::Graft(Graft {
                id: Some(seen.id),
                tree: T,
            }),
            Message::IHave(vec![IHave {
                id: unknown,
                round: Round(1),
                tree: T,
            }]),
            Message::Gossip(seen),
        ];
        for message in late {
            state.handle(InEvent::RecvMessage(2, message), now, &mut io);
        }
        assert!(!eager(&state).contains(&2));
        assert!(!lazy(&state).contains(&2));
        assert!(
            state.missing_messages.is_empty(),
            "a non-neighbor's IHave was kept"
        );

        state.on_neighbor_up(2);
        assert!(
            eager(&state).contains(&2),
            "a reconnect must restore the peer"
        );
    }

    /// A duplicate from a non-neighbor gets a `Prune`, and changes no peer set.
    ///
    /// An older node may still hold us as an eager peer after it dropped us. The
    /// `Prune` stops it from pushing every message to us in full.
    #[test]
    fn duplicate_from_a_non_neighbor_is_pruned_but_not_added() {
        let mut state = State::new(1u32, Config::default(), 1024);
        let now = Instant::now();
        let mut io = VecDeque::new();
        let message = gossip(b"dup", 1);
        state.handle(
            InEvent::RecvMessage(2, Message::Gossip(message.clone())),
            now,
            &mut io,
        );
        assert!(
            io.iter()
                .any(|event| matches!(event, crate::proto::topic::tests::OutEvent::EmitEvent(_))),
            "a non-neighbor's first payload is delivered"
        );
        io.clear();
        state.handle(
            InEvent::RecvMessage(2, Message::Gossip(message)),
            now,
            &mut io,
        );
        assert!(io.iter().any(|event| matches!(
            event,
            crate::proto::topic::tests::OutEvent::SendMessage(
                2,
                crate::proto::topic::Message::Gossip(Message::Prune(T))
            )
        )));
        assert!(eager(&state).is_empty() && lazy(&state).is_empty());
    }

    /// A late payload from a non-neighbor does not optimize the tree toward it.
    #[test]
    fn late_payload_from_a_non_neighbor_does_not_optimize_the_tree() {
        let mut state = State::new(1u32, Config::default(), 1024);
        let now = Instant::now();
        let mut io = VecDeque::new();
        state.on_neighbor_up(3);
        let message = gossip(b"late payload", 100);
        let ihave = Message::IHave(vec![IHave {
            id: message.id,
            round: Round(0),
            tree: T,
        }]);
        state.handle(InEvent::RecvMessage(3, ihave), now, &mut io);
        io.clear();
        state.handle(
            InEvent::RecvMessage(2, Message::Gossip(message)),
            now,
            &mut io,
        );
        assert!(!eager(&state).contains(&2) && !lazy(&state).contains(&2));
        assert!(
            !io.iter().any(|event| matches!(
                event,
                crate::proto::topic::tests::OutEvent::SendMessage(
                    2,
                    crate::proto::topic::Message::Gossip(Message::Prune(T))
                )
            )),
            "pruned a non-neighbor through the optimization: {io:?}"
        );
    }

    /// Returns the Plumtree messages in `io` to `peer`.
    fn sent_to(io: &VecDeque<topic::tests::OutEvent<u32>>, peer: u32) -> Vec<Message> {
        io.iter()
            .filter_map(|event| match event {
                topic::tests::OutEvent::SendMessage(to, topic::Message::Gossip(message))
                    if *to == peer =>
                {
                    Some(message.clone())
                }
                _ => None,
            })
            .collect()
    }

    /// Returns the push to `peer` that `io` contains.
    fn pushed_to(io: &VecDeque<topic::tests::OutEvent<u32>>, peer: u32) -> Option<Message> {
        sent_to(io, peer)
            .into_iter()
            .find(|message| matches!(message, Message::Gossip(_)))
    }

    /// Returns the ids that `io` announces to `peer`.
    fn announced_to(io: &VecDeque<topic::tests::OutEvent<u32>>, peer: u32) -> Vec<MessageId> {
        sent_to(io, peer)
            .into_iter()
            .filter_map(|message| match message {
                Message::IHave(ihaves) => Some(ihaves),
                _ => None,
            })
            .flatten()
            .map(|ihave| ihave.id)
            .collect()
    }

    /// Runs the timers that announce dropped messages, and returns the ids announced to `peer`.
    fn tick(state: &mut State<u32>, now: Instant, peer: u32) -> Vec<MessageId> {
        let mut io = VecDeque::new();
        state.handle(InEvent::TimerExpired(Timer::AnnounceDropped), now, &mut io);
        state.handle(InEvent::TimerExpired(Timer::DispatchLazyPush), now, &mut io);
        announced_to(&io, peer)
    }

    /// Returns whether `io` schedules a [`Timer::AnnounceDropped`].
    fn schedules_announce(io: &VecDeque<topic::tests::OutEvent<u32>>) -> bool {
        io.iter().any(|event| {
            matches!(
                event,
                topic::tests::OutEvent::ScheduleTimer(
                    _,
                    topic::Timer::Gossip(Timer::AnnounceDropped)
                )
            )
        })
    }

    /// The messages that the queue of a [`Dropping`] peer drops.
    #[derive(Clone, Copy)]
    enum Drops {
        Nothing,
        Pushes,
        Announcements,
    }

    /// A test IO that drops the messages that `Drops` names, as a full queue does.
    struct Dropping {
        out: VecDeque<topic::tests::OutEvent<u32>>,
        drops: Drops,
    }

    impl Dropping {
        fn new(drops: Drops) -> Self {
            Self {
                out: VecDeque::new(),
                drops,
            }
        }
    }

    impl IO<u32> for Dropping {
        fn send(
            &mut self,
            peer: u32,
            message: impl Into<topic::Message<u32>>,
        ) -> Option<topic::Message<u32>> {
            let message = message.into();
            let dropped = matches!(
                (self.drops, &message),
                (Drops::Pushes, topic::Message::Gossip(Message::Gossip(_)))
                    | (
                        Drops::Announcements,
                        topic::Message::Gossip(Message::IHave(_))
                    )
            );
            // Keep the message in `out`, so tests can look at what was dropped.
            self.out.send(peer, message.clone());
            dropped.then_some(message)
        }

        fn emit(&mut self, event: Event<u32>) {
            self.out.emit(event);
        }

        fn schedule(&mut self, delay: Duration, timer: impl Into<topic::Timer<u32>>) {
            self.out.schedule(delay, timer);
        }

        fn peer_data(&mut self, peer: u32, data: crate::proto::PeerData) {
            self.out.peer_data(peer, data);
        }

        fn connect(&mut self, peer: u32) {
            self.out.connect(peer);
        }
    }

    /// Returns a state with neighbor 2, and the IO after a broadcast of `content` that `drops` hit.
    fn broadcast_to_2(content: &[u8], scope: Scope, drops: Drops) -> (State<u32>, Dropping) {
        let mut io = Dropping::new(drops);
        let mut state = State::new(1u32, Config::default(), 1024);
        state.on_neighbor_up(2);
        let content = Bytes::copy_from_slice(content);
        state.handle(InEvent::Broadcast(content, scope), Instant::now(), &mut io);
        (state, io)
    }

    /// A push that the full queue to a peer dropped is announced to it with the next timer.
    ///
    /// The peer then requests it with a graft, as from any lazy peer. Not at
    /// once: a graft reply that the queue drops again would loop.
    #[test]
    fn dropped_push_is_announced_later() {
        let (mut state, mut io) = broadcast_to_2(b"dropped", Scope::Swarm, Drops::Pushes);
        let id = MessageId::from_content(b"dropped");
        let now = Instant::now();
        assert!(schedules_announce(&io.out), "no timer for the dropped push");
        state.handle(InEvent::TimerExpired(Timer::DispatchLazyPush), now, &mut io);
        assert!(announced_to(&io.out, 2).is_empty(), "announced at once");

        assert_eq!(tick(&mut state, now, 2), vec![id]);
        assert!(
            !state.announce_dropped_scheduled,
            "the timer runs on with nothing to announce"
        );
        // An announcement that went out is not repeated.
        assert!(tick(&mut state, now, 2).is_empty(), "announced twice");

        // A graft for it gets the payload from the cache, and the queue drops it.
        let mut io = Dropping::new(Drops::Pushes);
        let graft = Message::Graft(Graft {
            id: Some(id),
            tree: T,
        });
        state.handle(InEvent::RecvMessage(2, graft), now, &mut io);
        pushed_to(&io.out, 2).expect("the graft got no payload");

        // A dropped graft reply is announced again on the next tick, not at once.
        state.handle(InEvent::TimerExpired(Timer::DispatchLazyPush), now, &mut io);
        assert!(announced_to(&io.out, 2).is_empty(), "announced at once");
        assert_eq!(tick(&mut state, now, 2), vec![id]);
    }

    /// A dropped announcement goes out again with the next `Timer::AnnounceDropped`.
    #[test]
    fn dropped_announcement_is_announced_again() {
        let now = Instant::now();
        let mut io = Dropping::new(Drops::Announcements);
        let mut state = State::new(1u32, Config::default(), 1024);
        // Peer 2 is lazy, so the broadcast queues an announcement for it.
        state.on_neighbor_up(2);
        let prune = Message::Prune(TreeId::default());
        state.handle(InEvent::RecvMessage(2, prune), now, &mut io);
        let content = Bytes::from_static(b"dropped");
        state.handle(InEvent::Broadcast(content, Scope::Swarm), now, &mut io);
        state.handle(InEvent::TimerExpired(Timer::DispatchLazyPush), now, &mut io);
        let id = MessageId::from_content(b"dropped");
        assert_eq!(announced_to(&io.out, 2), vec![id], "no announcement sent");
        assert_eq!(tick(&mut state, now, 2), vec![id]);
    }

    /// A dropped neighbor message is lost, as we do not cache it.
    #[test]
    fn dropped_neighbor_message_is_not_announced() {
        let (mut state, _io) = broadcast_to_2(b"dropped", Scope::Neighbors, Drops::Pushes);
        assert!(tick(&mut state, Instant::now(), 2).is_empty());
    }

    /// A dropped push to a peer that went down, or that we no longer cache, is not announced.
    #[test]
    fn dropped_push_to_a_gone_peer_is_not_announced() {
        let mut io = Dropping::new(Drops::Pushes);
        let mut state = State::new(1u32, Config::default(), 1024);
        let now = Instant::now();
        state.on_neighbor_up(2);
        state.on_neighbor_up(3);
        state.handle(
            InEvent::Broadcast(b"dropped".to_vec().into(), Scope::Swarm),
            now,
            &mut io,
        );
        state.on_neighbor_down(2);

        let later = now + Config::default().message_cache_retention * 2;
        let mut io = VecDeque::new();
        state.handle(InEvent::TimerExpired(Timer::EvictCache), later, &mut io);
        state.handle(
            InEvent::TimerExpired(Timer::AnnounceDropped),
            later,
            &mut io,
        );
        state.handle(
            InEvent::TimerExpired(Timer::DispatchLazyPush),
            later,
            &mut io,
        );
        assert!(announced_to(&io, 2).is_empty(), "announced to a gone peer");
        assert!(
            announced_to(&io, 3).is_empty(),
            "announced a message we no longer cache"
        );
    }

    /// Repeated drops of a message take one entry, and the entries per peer and per tick have caps.
    #[test]
    fn dropped_messages_have_caps() {
        let mut io = Dropping::new(Drops::Pushes);
        let mut state = State::new(1u32, Config::default(), 1 << 20);
        let now = Instant::now();
        state.on_neighbor_up(2);
        for _ in 0..2 {
            for i in 0..MAX_DROPPED_PER_PEER + 10 {
                let content = Bytes::from(i.to_be_bytes().to_vec());
                state.handle(InEvent::Broadcast(content, Scope::Swarm), now, &mut io);
            }
        }
        assert_eq!(state.dropped[&2].len(), MAX_DROPPED_PER_PEER);

        let mut announced = 0;
        loop {
            let ids = tick(&mut state, now, 2);
            if ids.is_empty() {
                break;
            }
            assert!(ids.len() <= DROPPED_PER_TICK, "{} in one tick", ids.len());
            announced += ids.len();
        }
        assert_eq!(announced, MAX_DROPPED_PER_PEER);
    }

    /// Announcements queued for a neighbor that then goes down must not be sent.
    #[test]
    fn neighbor_down_drops_queued_ihaves() {
        let mut io = VecDeque::new();
        let mut state = State::new(1u32, Config::default(), 1024);
        let now = Instant::now();

        // Peer 2 joins and prunes us, which makes it a lazy peer, so broadcasting
        // queues an IHave for it rather than sending the content.
        state.on_neighbor_up(2);
        let prune = Message::Prune(TreeId::default());
        state.handle(InEvent::RecvMessage(2, prune), now, &mut io);
        let content: Bytes = b"hi".to_vec().into();
        state.handle(InEvent::Broadcast(content, Scope::Swarm), now, &mut io);
        assert!(state.lazy_push_queue.contains_key(&2));

        state.on_neighbor_down(2);
        io.clear();
        state.handle(InEvent::TimerExpired(Timer::DispatchLazyPush), now, &mut io);

        assert!(io.is_empty(), "sent to a peer that went down: {io:?}");
    }

    /// Returns an `IHave` of `id` after `round` hops.
    fn ihave(id: MessageId, round: u16) -> Message {
        Message::IHave(vec![IHave {
            id,
            round: Round(round),
            tree: T,
        }])
    }

    /// Adds the lazy peer `peer`, which announces `id` after `round` hops.
    fn announce(state: &mut State<u32>, peer: u32, id: MessageId, round: u16, now: Instant) {
        let mut io = VecDeque::new();
        state.on_neighbor_up(peer);
        state.handle(InEvent::RecvMessage(peer, Message::Prune(T)), now, &mut io);
        state.handle(InEvent::RecvMessage(peer, ihave(id, round)), now, &mut io);
    }

    /// Returns node 1 with the eager peer 2, and the peer 3 that it grafted for `id`.
    ///
    /// Peer 3 was lazy and announced the message after `round` hops.
    fn grafted_state(id: MessageId, round: u16, now: Instant) -> State<u32> {
        let mut state = State::new(1u32, Config::default(), 1024);
        let mut io = VecDeque::new();
        state.on_neighbor_up(2);
        announce(&mut state, 3, id, round, now);
        state.handle(InEvent::TimerExpired(Timer::SendGraft(id)), now, &mut io);
        assert!(matches!(sent_to(&io, 3)[..], [Message::Graft(_)]));
        state
    }

    /// A grafted peer whose late copy took no more hops replaces the peer of the eager copy.
    ///
    /// The graft fired while the eager copy still traveled a slower path. If
    /// the duplicate pruned the grafted peer, the graft and the prune would
    /// repeat for each message.
    #[test]
    fn late_graft_reply_over_no_more_hops_replaces_the_eager_peer() {
        let now = Instant::now();
        for hops in [1, 6] {
            let slow = gossip(b"shortcut", 6);
            let mut state = grafted_state(slow.id, hops, now);
            let mut io = VecDeque::new();
            state.handle(InEvent::RecvMessage(2, Message::Gossip(slow)), now, &mut io);
            assert!(
                sent_to(&io, 3).is_empty(),
                "pushed the payload to the peer that announced it"
            );

            io.clear();
            let fast = gossip(b"shortcut", hops);
            state.handle(InEvent::RecvMessage(3, Message::Gossip(fast)), now, &mut io);
            assert_eq!(sent_to(&io, 2), vec![Message::Prune(T)], "hops {hops}");
            assert!(sent_to(&io, 3).is_empty());
            assert!(eager(&state).contains(&3));
            assert!(lazy(&state).contains(&2));
            state.check_invariants();
        }
    }

    /// A grafted peer whose late copy took more hops is pruned, as any duplicate.
    #[test]
    fn late_graft_reply_over_a_longer_path_is_pruned() {
        let now = Instant::now();
        let first = gossip(b"detour", 2);
        let mut state = grafted_state(first.id, 5, now);
        let mut io = VecDeque::new();
        state.handle(
            InEvent::RecvMessage(2, Message::Gossip(first)),
            now,
            &mut io,
        );
        io.clear();
        let late = gossip(b"detour", 5);
        state.handle(InEvent::RecvMessage(3, Message::Gossip(late)), now, &mut io);
        assert_eq!(sent_to(&io, 3), vec![Message::Prune(T)]);
        assert!(sent_to(&io, 2).is_empty());
        assert!(eager(&state).contains(&2));
        assert!(lazy(&state).contains(&3));
    }

    /// Only the first late copy of a grafted peer replaces the eager peer.
    ///
    /// A second grafted peer, asked after the second graft timeout, is pruned
    /// as any duplicate, so a node keeps one eager peer for the path.
    #[test]
    fn second_late_graft_reply_is_pruned() {
        let now = Instant::now();
        let slow = gossip(b"twice", 6);
        let mut state = grafted_state(slow.id, 1, now);
        announce(&mut state, 4, slow.id, 2, now);
        let mut io = VecDeque::new();
        state.handle(
            InEvent::TimerExpired(Timer::SendGraft(slow.id)),
            now,
            &mut io,
        );
        assert!(eager(&state).contains(&4), "no second graft");

        state.handle(InEvent::RecvMessage(2, Message::Gossip(slow)), now, &mut io);
        let fast = gossip(b"twice", 1);
        state.handle(InEvent::RecvMessage(3, Message::Gossip(fast)), now, &mut io);
        io.clear();
        let second = gossip(b"twice", 2);
        state.handle(
            InEvent::RecvMessage(4, Message::Gossip(second)),
            now,
            &mut io,
        );
        assert_eq!(sent_to(&io, 4), vec![Message::Prune(T)]);
        assert!(eager(&state).contains(&3));
        assert!(lazy(&state).contains(&2) && lazy(&state).contains(&4));
    }

    /// Of two graft replies before any eager copy, the node prunes the later one.
    ///
    /// No eager copy was late, so the rule for late graft replies does not apply.
    #[test]
    fn later_of_two_graft_replies_is_pruned() {
        let now = Instant::now();
        let id = gossip(b"two replies", 1).id;
        let mut state = grafted_state(id, 1, now);
        announce(&mut state, 4, id, 1, now);
        let mut io = VecDeque::new();
        state.handle(InEvent::TimerExpired(Timer::SendGraft(id)), now, &mut io);
        let reply = gossip(b"two replies", 1);
        state.handle(
            InEvent::RecvMessage(3, Message::Gossip(reply.clone())),
            now,
            &mut io,
        );
        io.clear();
        state.handle(
            InEvent::RecvMessage(4, Message::Gossip(reply)),
            now,
            &mut io,
        );
        assert_eq!(sent_to(&io, 4), vec![Message::Prune(T)]);
        assert!(sent_to(&io, 3).is_empty());
        assert!(eager(&state).contains(&3));
        assert!(lazy(&state).contains(&4));
    }

    /// A peer that announced a message twice before the graft timer gets one graft.
    #[test]
    fn peer_announced_twice_before_the_graft_gets_one() {
        let now = Instant::now();
        let id = gossip(b"announced twice", 1).id;
        let mut state = State::new(1u32, Config::default(), 1024);
        let mut io = VecDeque::new();
        state.on_neighbor_up(3);
        state.handle(InEvent::RecvMessage(3, Message::Prune(T)), now, &mut io);
        let ihave = ihave(id, 1);
        state.handle(InEvent::RecvMessage(3, ihave.clone()), now, &mut io);
        state.handle(InEvent::RecvMessage(3, ihave), now, &mut io);
        io.clear();
        for _ in 0..2 {
            state.handle(InEvent::TimerExpired(Timer::SendGraft(id)), now, &mut io);
        }
        assert_eq!(sent_to(&io, 3).len(), 1, "grafts: {:?}", sent_to(&io, 3));
    }

    /// A kept peer that answers a retried graft stays eager.
    ///
    /// The second reply is our own retry, not a redundant path. If it pruned
    /// the peer, the node would lose both the eager peer and the kept one.
    #[test]
    fn retried_reply_of_a_kept_peer_is_ignored() {
        let now = Instant::now();
        let slow = gossip(b"retried", 6);
        let mut state = grafted_state(slow.id, 1, now);
        let mut io = VecDeque::new();
        state.handle(InEvent::RecvMessage(3, ihave(slow.id, 1)), now, &mut io);
        io.clear();
        state.handle(
            InEvent::TimerExpired(Timer::SendGraft(slow.id)),
            now,
            &mut io,
        );
        assert!(
            matches!(sent_to(&io, 3)[..], [Message::Graft(_)]),
            "no retry"
        );

        state.handle(InEvent::RecvMessage(2, Message::Gossip(slow)), now, &mut io);
        let fast = gossip(b"retried", 1);
        state.handle(
            InEvent::RecvMessage(3, Message::Gossip(fast.clone())),
            now,
            &mut io,
        );
        io.clear();
        state.handle(InEvent::RecvMessage(3, Message::Gossip(fast)), now, &mut io);
        assert!(sent_to(&io, 3).is_empty(), "pruned the kept peer");
        assert!(eager(&state).contains(&3));
        assert!(lazy(&state).contains(&2));
    }

    /// A message whose graft replies the source's queue drops reaches the receiver once it clears.
    ///
    /// The source announces the message again after each drop, and the
    /// receiver grafts it again. A second reply, as when a retry crosses a
    /// reply, prunes nothing.
    /// With two peers, no other source can hide a missing retry.
    #[test]
    fn dropped_graft_replies_are_requested_again() {
        let now = Instant::now();
        let (mut source, _io) = broadcast_to_2(b"dropped replies", Scope::Swarm, Drops::Pushes);
        let mut receiver = State::new(2u32, Config::default(), 1024);
        receiver.on_neighbor_up(1);
        let mut reply = None;
        for attempt in 0..3 {
            let id = tick(&mut source, now, 2);
            assert_eq!(id.len(), 1, "attempt {attempt}: no announcement");
            let ihave = Message::IHave(vec![IHave {
                id: id[0],
                round: Round(0),
                tree: T,
            }]);
            receiver.handle(InEvent::RecvMessage(1, ihave), now, &mut VecDeque::new());
            let mut io = VecDeque::new();
            receiver.handle(InEvent::TimerExpired(Timer::SendGraft(id[0])), now, &mut io);
            let graft = sent_to(&io, 1).pop().expect("a graft");
            assert!(
                matches!(graft, Message::Graft(_)),
                "attempt {attempt}: no graft"
            );
            // The queue drops the first two replies.
            let drops = if attempt < 2 {
                Drops::Pushes
            } else {
                Drops::Nothing
            };
            let mut io = Dropping::new(drops);
            source.handle(InEvent::RecvMessage(2, graft), now, &mut io);
            let answer = pushed_to(&io.out, 2).expect("a reply");
            if attempt < 2 {
                reply = Some(answer);
            } else {
                let mut io = VecDeque::new();
                receiver.handle(InEvent::RecvMessage(1, answer), now, &mut io);
                let received = io
                    .iter()
                    .filter(|event| matches!(event, topic::tests::OutEvent::EmitEvent(_)))
                    .count();
                assert_eq!(received, 1);
            }
        }
        // A second reply to the same peer's grafts.
        let mut io = VecDeque::new();
        receiver.handle(
            InEvent::RecvMessage(1, reply.expect("a dropped reply")),
            now,
            &mut io,
        );
        assert!(io.is_empty(), "late reply caused {io:?}");
        assert!(eager(&receiver).contains(&1));
    }

    /// While a graft for a message is out, the eager copy does not optimize the tree.
    ///
    /// The tree optimization would only see the announcements left after the
    /// graft. Here it would graft peer 4 and prune peer 2, and the reply of
    /// the grafted peer 3 would then find no eager peer to replace.
    #[test]
    fn graft_reply_decides_before_the_tree_optimization() {
        let now = Instant::now();
        let slow = gossip(b"optimize", 9);
        let mut state = grafted_state(slow.id, 1, now);
        announce(&mut state, 4, slow.id, 2, now);
        let mut io = VecDeque::new();
        state.handle(InEvent::RecvMessage(2, Message::Gossip(slow)), now, &mut io);
        assert!(sent_to(&io, 4).is_empty(), "optimized the tree");
        let fast = gossip(b"optimize", 1);
        state.handle(InEvent::RecvMessage(3, Message::Gossip(fast)), now, &mut io);
        assert_eq!(eager(&state), BTreeSet::from([3]));
        assert!(lazy(&state).contains(&2) && lazy(&state).contains(&4));
    }

    /// A graft whose reply cannot change the tree does not stop the tree optimization.
    ///
    /// Peer 3 announced over more hops than the eager copy took, so its reply
    /// gets pruned. The optimization still grafts peer 4, which announced over
    /// far fewer hops. Else no rule would ever pick peer 4.
    #[test]
    fn slow_graft_does_not_stop_the_tree_optimization() {
        let now = Instant::now();
        let slow = gossip(b"slow graft", 8);
        let mut state = grafted_state(slow.id, 9, now);
        announce(&mut state, 4, slow.id, 1, now);
        let mut io = VecDeque::new();
        state.handle(InEvent::RecvMessage(2, Message::Gossip(slow)), now, &mut io);
        assert!(
            matches!(sent_to(&io, 4)[..], [Message::Graft(_)]),
            "no optimization"
        );
        assert_eq!(sent_to(&io, 2), vec![Message::Prune(T)]);
    }

    /// A grafted peer that went down does not hold back the tree optimization.
    ///
    /// Its reply cannot come any more, so only the other announcers count.
    #[test]
    fn gone_grafted_peer_does_not_stop_the_tree_optimization() {
        let now = Instant::now();
        let slow = gossip(b"gone graft", 9);
        let mut state = grafted_state(slow.id, 1, now);
        announce(&mut state, 4, slow.id, 2, now);
        let mut io = VecDeque::new();
        state.on_neighbor_down(3);
        io.clear();
        state.handle(InEvent::RecvMessage(2, Message::Gossip(slow)), now, &mut io);
        assert!(
            matches!(sent_to(&io, 4)[..], [Message::Graft(_)]),
            "no optimization"
        );
        assert_eq!(sent_to(&io, 2), vec![Message::Prune(T)]);
    }

    /// A grafted peer that went down and came back gets the push of the eager copy.
    ///
    /// It lost its state, so it may not have the message any more.
    #[test]
    fn reconnected_grafted_peer_gets_the_push() {
        let now = Instant::now();
        let slow = gossip(b"reconnected", 6);
        let mut state = grafted_state(slow.id, 1, now);
        let mut io = VecDeque::new();
        state.on_neighbor_down(3);
        state.on_neighbor_up(3);
        io.clear();
        state.handle(InEvent::RecvMessage(2, Message::Gossip(slow)), now, &mut io);
        assert!(
            pushed_to(&io, 3).is_some(),
            "no push to the reconnected peer"
        );
    }

    /// A message that missed the eager path reaches us through an `IHave` and a `Graft`.
    ///
    /// A lazy peer announces the message. When the graft timer fires, we ask
    /// that peer for it with a `Graft`, and the peer becomes eager. The peer
    /// answers the `Graft` with the message, and makes us eager too.
    #[test]
    fn missed_gossip_is_repaired_by_graft() {
        let now = Instant::now();
        let content: Bytes = b"repair".to_vec().into();
        let id = MessageId::from_content(&content);

        let mut sender = State::new(2u32, Config::default(), 1024);
        let mut sender_io = VecDeque::new();
        sender.on_neighbor_up(1);
        sender.handle(
            InEvent::RecvMessage(1, Message::Prune(T)),
            now,
            &mut sender_io,
        );
        sender.handle(
            InEvent::Broadcast(content, Scope::Swarm),
            now,
            &mut sender_io,
        );
        sender_io.clear();

        let mut receiver = State::new(1u32, Config::default(), 1024);
        let mut io = VecDeque::new();
        receiver.on_neighbor_up(2);
        receiver.handle(InEvent::RecvMessage(2, Message::Prune(T)), now, &mut io);
        let ihave = ihave(id, 1);
        receiver.handle(InEvent::RecvMessage(2, ihave), now, &mut io);
        io.clear();
        receiver.handle(InEvent::TimerExpired(Timer::SendGraft(id)), now, &mut io);

        let graft = io
            .iter()
            .find_map(|event| match event {
                topic::tests::OutEvent::SendMessage(
                    2,
                    topic::Message::Gossip(Message::Graft(graft)),
                ) => Some(graft.clone()),
                _ => None,
            })
            .expect("no graft to the announcer");
        assert!(eager(&receiver).contains(&2), "the announcer is not eager");

        sender.handle(
            InEvent::RecvMessage(1, Message::Graft(graft)),
            now,
            &mut sender_io,
        );
        let answered = sender_io.iter().any(|event| {
            matches!(
                event,
                topic::tests::OutEvent::SendMessage(
                    1,
                    topic::Message::Gossip(Message::Gossip(gossip)),
                ) if gossip.id == id
            )
        });
        assert!(answered, "the graft was not answered: {sender_io:?}");
        assert!(
            eager(&sender).contains(&1),
            "the grafting peer is not eager"
        );
    }

    #[test]
    fn optimize_tree() {
        let mut io = VecDeque::new();
        let config: Config = Default::default();
        let mut state = State::new(1, config.clone(), 1024);
        let now = Instant::now();
        // Peers 2 and 3 are neighbors, and 2 pruned us, so it is lazy.
        state.on_neighbor_up(2);
        state.on_neighbor_up(3);
        state.handle(InEvent::RecvMessage(2, Message::Prune(T)), now, &mut io);
        io.clear();

        // we receive an IHave message from peer 2
        // it has `round: 2` which means that the the peer that sent us the IHave was
        // two hops away from the original sender of the message
        let content: Bytes = b"hi".to_vec().into();
        let id = MessageId::from_content(&content);
        let event = InEvent::RecvMessage(2u32, ihave(id, 2));
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
                tree: T,
            }),
        );
        state.handle(event, now, &mut io);
        let expected = {
            // we expect a dispatch timer schedule and receive event, but no Graft or Prune
            // messages
            let mut io = VecDeque::new();
            io.schedule(config.dispatch_timeout, Timer::DispatchLazyPush);
            io.emit(Event::Received(GossipEvent {
                content,
                delivered_from: 3,
                scope: DeliveryScope::Swarm(Round(6)),
            }));
            io
        };
        assert_eq!(io, expected);
        io.clear();

        // now we run the same flow again but this time peer 3 is 9 hops away from the message's
        // sender. message's sender. this will trigger the optimization:
        // peer 2 will be promoted to eager and peer 4 demoted to lazy

        let content: Bytes = b"hi2".to_vec().into();
        let id = MessageId::from_content(&content);
        let event = InEvent::RecvMessage(2u32, ihave(id, 2));
        state.handle(event, now, &mut io);
        io.clear();

        let event = InEvent::RecvMessage(
            3,
            Message::Gossip(Gossip {
                id,
                content: content.clone(),
                scope: DeliveryScope::Swarm(Round(9)),
                tree: T,
            }),
        );
        state.handle(event, now, &mut io);
        let expected = {
            // this time we expect the Graft and Prune messages to be sent, performing the
            // optimization step
            let mut io = VecDeque::new();
            io.send(2, Message::Graft(Graft { id: None, tree: T }));
            io.send(3, Message::Prune(T));
            io.emit(Event::Received(GossipEvent {
                content,
                delivered_from: 3,
                scope: DeliveryScope::Swarm(Round(9)),
            }));
            io
        };
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
            tree: T,
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

    /// A missing message leaves no entry once we asked its last announcer.
    #[test]
    fn last_graft_leaves_no_missing_entry() {
        let mut io = VecDeque::new();
        let mut state = State::new(1u32, Config::default(), 1024);
        let now = Instant::now();
        state.on_neighbor_up(2);
        let id = MessageId::from_content(b"hi");
        state.handle(InEvent::RecvMessage(2, ihave(id, 1)), now, &mut io);

        state.handle(InEvent::TimerExpired(Timer::SendGraft(id)), now, &mut io);

        assert!(state.missing_messages.is_empty());
    }

    /// A batch of announcements fits the size limit, also when its length takes 3 bytes.
    ///
    /// A list of 16384 entries or more has a length of 3 bytes, and an announcement
    /// of a round of 16384 or more takes 35 bytes.
    #[test]
    fn announcement_batch_fits_the_size_limit() {
        let max_message_size = 16_384 * IHave::POSTCARD_MAX_SIZE + 3;
        let mut state = State::new(1u32, Config::default(), max_message_size);
        let now = Instant::now();
        let mut io = VecDeque::new();
        state.on_neighbor_up(2);
        let ihaves = (0..20_000u32).map(|i| IHave {
            id: MessageId::from_content(&i.to_be_bytes()),
            round: Round(20_000),
            tree: T,
        });
        state.lazy_push_queue.insert(2, ihaves.collect());
        io.clear();
        state.handle(InEvent::TimerExpired(Timer::DispatchLazyPush), now, &mut io);

        let mut sent = 0;
        for event in io {
            if let topic::tests::OutEvent::SendMessage(2, topic::Message::Gossip(message)) = event {
                let size = postcard::experimental::serialized_size(&message).expect("encodes");
                assert!(size <= max_message_size, "{size} > {max_message_size}");
                sent += 1;
            }
        }
        assert!(sent >= 2, "the batch was not split");
    }

    /// The IO of a node with a filter that always gives `verdict`, and notes what it was asked.
    struct Filtering {
        out: VecDeque<topic::tests::OutEvent<u32>>,
        verdict: Verdict,
        asked: Vec<(MessageId, u32, DeliveryScope)>,
    }

    impl Filtering {
        fn new(verdict: Verdict) -> Self {
            Self {
                out: VecDeque::new(),
                verdict,
                asked: Vec::new(),
            }
        }
    }

    impl IO<u32> for Filtering {
        fn send(
            &mut self,
            peer: u32,
            message: impl Into<topic::Message<u32>>,
        ) -> Option<topic::Message<u32>> {
            self.out.send(peer, message)
        }

        fn emit(&mut self, event: Event<u32>) {
            self.out.emit(event);
        }

        fn schedule(&mut self, delay: Duration, timer: impl Into<topic::Timer<u32>>) {
            self.out.schedule(delay, timer);
        }

        fn peer_data(&mut self, peer: u32, data: crate::proto::PeerData) {
            self.out.peer_data(peer, data);
        }

        fn connect(&mut self, peer: u32) {
            self.out.connect(peer);
        }

        fn filter(&mut self, message: &Gossip, delivered_from: u32) -> Option<Verdict> {
            self.asked.push((message.id, delivered_from, message.scope));
            Some(self.verdict)
        }
    }

    /// Returns node 1 with the eager neighbors 2 and 3 and the lazy neighbor 4.
    fn filtered_state() -> State<u32> {
        let mut state = State::new(1u32, Config::default(), 1024);
        let mut io = VecDeque::new();
        for peer in [2, 3, 4] {
            state.on_neighbor_up(peer);
        }
        state.handle(
            InEvent::RecvMessage(4, Message::Prune(T)),
            Instant::now(),
            &mut io,
        );
        state
    }

    fn received(io: &Filtering) -> usize {
        io.out
            .iter()
            .filter(|event| matches!(event, topic::tests::OutEvent::EmitEvent(Event::Received(_))))
            .count()
    }

    /// An accepted message is delivered, cached, and forwarded to eager and lazy peers.
    #[test]
    fn filter_accept_changes_nothing() {
        let (mut state, now) = (filtered_state(), Instant::now());
        let mut io = Filtering::new(Verdict::Accept);
        let message = gossip(b"ok", 1);
        let id = message.id;
        state.handle(
            InEvent::RecvMessage(2, Message::Gossip(message)),
            now,
            &mut io,
        );

        assert_eq!(io.asked, vec![(id, 2, DeliveryScope::Swarm(Round(1)))]);
        assert_eq!(received(&io), 1);
        assert!(state.cache.contains_key(&id));
        assert!(state.lazy_push_queue.contains_key(&4));
        let pushed = io.out.iter().any(|event| {
            matches!(
                event,
                topic::tests::OutEvent::SendMessage(3, topic::Message::Gossip(_))
            )
        });
        assert!(pushed, "not forwarded: {:?}", io.out);
    }

    /// A rejected message is not delivered, cached, forwarded or announced.
    ///
    /// It counts as seen: copies and announcements are dropped without a second
    /// filter call, and a graft for it finds nothing.
    #[test]
    fn filter_reject_stops_the_message() {
        let (mut state, now) = (filtered_state(), Instant::now());
        let mut io = Filtering::new(Verdict::Reject);
        let message = gossip(b"bad", 1);
        let id = message.id;
        state.handle(
            InEvent::RecvMessage(2, Message::Gossip(message.clone())),
            now,
            &mut io,
        );

        assert_eq!(io.asked.len(), 1);
        assert!(io.out.is_empty(), "a reject does something: {:?}", io.out);
        assert!(state.cache.get(&id).is_none());
        assert!(state.lazy_push_queue.is_empty());
        assert!(state.received_messages.contains_key(&id));

        // A copy from 3 is a duplicate: the filter does not run, and 3 turns lazy.
        state.handle(
            InEvent::RecvMessage(3, Message::Gossip(message)),
            now,
            &mut io,
        );
        assert_eq!(io.asked.len(), 1);
        assert!(lazy(&state).contains(&3));
        io.out.clear();

        // An announcement of a seen id starts no graft.
        state.handle(InEvent::RecvMessage(4, ihave(id, 1)), now, &mut io);
        assert!(io.out.is_empty(), "announcement of a seen id: {:?}", io.out);

        // A graft for it gets no payload.
        let graft = Message::Graft(Graft {
            id: Some(id),
            tree: T,
        });
        state.handle(InEvent::RecvMessage(4, graft), now, &mut io);
        assert!(io.out.is_empty(), "graft was answered: {:?}", io.out);
        assert_eq!(io.asked.len(), 1);
    }

    /// A rejected message starts no graft, also after an earlier announcement.
    #[test]
    fn filter_reject_clears_missing() {
        let (mut state, now) = (filtered_state(), Instant::now());
        let mut io = Filtering::new(Verdict::Reject);
        let message = gossip(b"bad", 1);
        let id = message.id;
        state.handle(InEvent::RecvMessage(4, ihave(id, 1)), now, &mut io);
        state.handle(
            InEvent::RecvMessage(2, Message::Gossip(message)),
            now,
            &mut io,
        );
        assert!(state.missing_messages.is_empty());
        io.out.clear();
        state.handle(InEvent::TimerExpired(Timer::SendGraft(id)), now, &mut io);
        assert!(
            io.out.is_empty(),
            "graft for a rejected message: {:?}",
            io.out
        );
    }

    /// A message to the neighbors passes the filter once, however often it arrives.
    #[test]
    fn filter_runs_once_for_neighbors_messages() {
        for verdict in [Verdict::Accept, Verdict::Reject] {
            let (mut state, now) = (filtered_state(), Instant::now());
            let mut io = Filtering::new(verdict);
            let content = Bytes::from_static(b"hello");
            let neighbors = Gossip::new(content.clone(), DeliveryScope::Neighbors, T);
            for from in [2, 3, 2] {
                let message = Message::Gossip(neighbors.clone());
                state.handle(InEvent::RecvMessage(from, message), now, &mut io);
            }
            assert_eq!(io.asked.len(), 1, "{verdict:?}");
            let delivered = if verdict == Verdict::Accept { 3 } else { 0 };
            assert_eq!(received(&io), delivered, "{verdict:?}");

            // The same content to the swarm takes the verdict too.
            let swarm = Gossip::new(content, DeliveryScope::Swarm(Round(1)), T);
            let id = swarm.id;
            state.handle(
                InEvent::RecvMessage(3, Message::Gossip(swarm)),
                now,
                &mut io,
            );
            assert_eq!(io.asked.len(), 1, "{verdict:?}");
            assert_eq!(state.cache.contains_key(&id), verdict == Verdict::Accept);
        }
    }

    /// Without a filter, the state keeps nothing about messages to the neighbors.
    #[test]
    fn no_filter_keeps_no_verdicts() {
        let (mut state, now) = (filtered_state(), Instant::now());
        let mut io = VecDeque::new();
        let neighbors = Gossip::new(Bytes::from_static(b"x"), DeliveryScope::Neighbors, T);
        state.handle(
            InEvent::RecvMessage(2, Message::Gossip(neighbors)),
            now,
            &mut io,
        );
        assert!(state.neighbor_verdicts.is_empty());
    }

    /// Returns a payload on `tree`, as received after one hop.
    fn on_tree(tree: u8, content: &'static [u8]) -> Gossip {
        let mut message = gossip(content, 1);
        message.tree = TreeId([tree; 8]);
        message
    }

    fn recv(
        state: &mut State<u32>,
        from: u32,
        message: Message,
    ) -> VecDeque<topic::tests::OutEvent<u32>> {
        let mut io = VecDeque::new();
        state.handle(InEvent::RecvMessage(from, message), Instant::now(), &mut io);
        io
    }

    fn eager_in(state: &State<u32>, tree: u8) -> BTreeSet<u32> {
        state.trees[&TreeId([tree; 8])].eager.clone()
    }

    fn recv_at(
        state: &mut State<u32>,
        from: u32,
        message: Message,
        now: Instant,
    ) -> VecDeque<topic::tests::OutEvent<u32>> {
        let mut io = VecDeque::new();
        state.handle(InEvent::RecvMessage(from, message), now, &mut io);
        io
    }

    /// A tree prunes on its own, and at the first duplicate.
    #[test]
    fn pruning_acts_on_one_tree() {
        let mut state = State::new(1u32, Config::default(), 1024);
        for peer in [2, 3] {
            state.on_neighbor_up(peer);
        }
        recv(&mut state, 2, Message::Gossip(on_tree(1, b"a")));
        recv(&mut state, 2, Message::Gossip(on_tree(2, b"b")));
        let io = recv(&mut state, 3, Message::Gossip(on_tree(1, b"a")));
        assert_eq!(sent_to(&io, 3), vec![Message::Prune(TreeId([1; 8]))]);
        assert_eq!(eager_in(&state, 1), BTreeSet::from([2]));
        assert_eq!(
            eager_in(&state, 2),
            BTreeSet::from([2, 3]),
            "tree 2 changed"
        );
        let io = recv(&mut state, 3, Message::Gossip(on_tree(2, b"b")));
        assert_eq!(sent_to(&io, 3), vec![Message::Prune(TreeId([2; 8]))]);
        assert_eq!(eager_in(&state, 2), BTreeSet::from([2]));
    }

    /// A link that alternates is pruned at its first duplicate after the hold.
    ///
    /// The link alternates between first copy and duplicate. A duplicate of a message first
    /// seen within the hold of a prune does not count.
    #[test]
    fn alternating_link_prunes_and_hold_spares_in_flight_messages() {
        let hold = Config::default().graft_timeout_1;
        let mut state = State::new(1u32, Config::default(), 1024);
        for peer in [2, 3, 4] {
            state.on_neighbor_up(peer);
        }
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let tree = TreeId([2; 8]);
        // The first message floods. Its duplicates prune 3 and 4.
        recv_at(&mut state, 2, Message::Gossip(on_tree(2, b"a")), at(0));
        recv_at(&mut state, 3, Message::Gossip(on_tree(2, b"a")), at(10));
        assert_eq!(eager_in(&state, 2), BTreeSet::from([2, 4]));
        // `b` is first seen within the hold of that prune: its duplicate from 4 stays.
        recv_at(&mut state, 2, Message::Gossip(on_tree(2, b"b")), at(20));
        let io = recv_at(&mut state, 4, Message::Gossip(on_tree(2, b"b")), at(30));
        assert!(
            sent_to(&io, 4).is_empty(),
            "pruned for an in-flight message"
        );
        // Now 4 and 2 alternate. `c` and `d` come after the hold, and the first duplicate of
        // `c` from 2 prunes it: no streak of two is needed.
        let later = 10 + hold.as_millis() as u64 + 1;
        recv_at(&mut state, 4, Message::Gossip(on_tree(2, b"c")), at(later));
        let io = recv_at(
            &mut state,
            2,
            Message::Gossip(on_tree(2, b"c")),
            at(later + 5),
        );
        assert_eq!(sent_to(&io, 2), vec![Message::Prune(tree)]);
        assert_eq!(eager_in(&state, 2), BTreeSet::from([4]));
    }

    /// Our first broadcasts go on the shared tree, and the next ones on our own.
    ///
    /// The count starts again after the tree has been idle.
    #[test]
    fn broadcast_switches_to_our_own_tree_after_n() {
        let config = Config::default();
        let n = config.source_tree_after;
        let mut state = State::new(1u32, config, 1024);
        state.on_neighbor_up(2);
        let now = Instant::now();
        let broadcast = |state: &mut State<u32>, i: u32, now: Instant| {
            let mut io = VecDeque::new();
            let content: Bytes = format!("m{i}").into_bytes().into();
            state.handle(InEvent::Broadcast(content, Scope::Swarm), now, &mut io);
            io.iter()
                .find_map(|event| match event {
                    topic::tests::OutEvent::SendMessage(
                        _,
                        topic::Message::Gossip(Message::Gossip(gossip)),
                    ) => Some(gossip.tree),
                    _ => None,
                })
                .expect("a payload went out")
        };
        for i in 0..n {
            assert_eq!(broadcast(&mut state, i, now), TreeId::default());
        }
        assert_eq!(broadcast(&mut state, n, now), TreeId::of(&1u32));
        assert_eq!(broadcast(&mut state, n + 1, now), TreeId::of(&1u32));
        // After the idle time the count restarts.
        let later = now + TREE_RETENTION;
        assert_eq!(broadcast(&mut state, n + 2, later), TreeId::default());
    }

    /// The shared tree is never evicted for a new tree.
    #[test]
    fn shared_tree_is_not_evicted() {
        let mut state = State::new(1u32, Config::default(), 1024);
        state.on_neighbor_up(2);
        let now = Instant::now();
        state.tree(TreeId::default(), now, None);
        for i in 0..(MAX_TREES as u64 + 5) {
            let id = TreeId(i.to_be_bytes());
            state.tree(id, now + Duration::from_millis(i + 1), None);
        }
        assert!(state.trees.contains_key(&TreeId::default()));
        assert_eq!(state.trees.len(), MAX_TREES);
    }

    /// An announcement while a graft waits for its reply starts no second chain of grafts.
    #[test]
    fn announcement_during_a_graft_starts_no_second_chain() {
        let now = Instant::now();
        let mut state = State::new(1u32, Config::default(), 1024);
        let id = MessageId::from_content(b"x");
        let mut io = VecDeque::new();
        for peer in [2, 3] {
            state.on_neighbor_up(peer);
            state.handle(InEvent::RecvMessage(peer, Message::Prune(T)), now, &mut io);
        }
        state.handle(InEvent::RecvMessage(2, ihave(id, 1)), now, &mut io);
        let mut io = VecDeque::new();
        state.handle(InEvent::TimerExpired(Timer::SendGraft(id)), now, &mut io);
        let graft = Message::Graft(Graft {
            id: Some(id),
            tree: T,
        });
        assert_eq!(sent_to(&io, 2), vec![graft]);
        let later = now + Duration::from_millis(10);
        let timers = |io: &VecDeque<topic::tests::OutEvent<u32>>| {
            io.iter()
                .filter(|event| {
                    matches!(
                        event,
                        topic::tests::OutEvent::ScheduleTimer(_, topic::Timer::Gossip(Timer::SendGraft(i)))
                            if *i == id
                    )
                })
                .count()
        };
        let mut io = VecDeque::new();
        state.handle(InEvent::RecvMessage(3, ihave(id, 1)), later, &mut io);
        assert_eq!(timers(&io), 0, "a second chain started");
        // The chain asks the next announcer after the second timeout.
        let mut io = VecDeque::new();
        state.handle(InEvent::TimerExpired(Timer::SendGraft(id)), later, &mut io);
        let graft = Message::Graft(Graft {
            id: Some(id),
            tree: T,
        });
        assert_eq!(sent_to(&io, 3), vec![graft]);
        assert_eq!(timers(&io), 1);
    }

    /// The shared tree does not expire, so a tree that starts later copies it.
    #[test]
    fn shared_tree_does_not_expire() {
        let mut state = State::new(1u32, Config::default(), 1024);
        for peer in [2, 3] {
            state.on_neighbor_up(peer);
        }
        let now = Instant::now();
        state.tree(TreeId::default(), now, None);
        state.add_lazy(TreeId::default(), 3, now);
        let later = now + TREE_RETENTION + Duration::from_secs(1);
        let mut io = VecDeque::new();
        state.handle(InEvent::TimerExpired(Timer::EvictCache), later, &mut io);
        assert!(state.trees.contains_key(&TreeId::default()), "it expired");
        let tree = state.tree(TreeId([7; 8]), later, None);
        assert_eq!(tree.eager, BTreeSet::from([2]), "a new tree floods");
    }

    /// A new tree copies the shared tree, or has every neighbor eager if there is none.
    #[test]
    fn new_tree_is_seeded_from_shared_tree() {
        let now = Instant::now();
        let mut state = State::new(1u32, Config::default(), 1024);
        for peer in [2, 3, 4] {
            state.on_neighbor_up(peer);
        }
        let other = TreeId([5; 8]);
        assert_eq!(
            state.tree(other, now, None).eager,
            BTreeSet::from([2, 3, 4])
        );

        state.add_lazy(TreeId::default(), 3, now);
        let own = TreeId([6; 8]);
        let tree = state.tree(own, now, None);
        assert_eq!(tree.eager, BTreeSet::from([2, 4]));
        assert_eq!(tree.lazy, BTreeSet::from([3]));
        // Trees that exist already do not follow the shared tree.
        assert_eq!(state.trees[&other].eager, BTreeSet::from([2, 3, 4]));
    }

    /// A prune and a graft change the tree they name, and a new neighbor joins every tree.
    #[test]
    fn prune_and_graft_name_their_tree() {
        let mut state = State::new(1u32, Config::default(), 1024);
        state.on_neighbor_up(2);
        recv(&mut state, 2, Message::Gossip(on_tree(1, b"a")));
        recv(&mut state, 2, Message::Gossip(on_tree(2, b"b")));
        state.on_neighbor_up(3);
        assert_eq!(eager_in(&state, 1), BTreeSet::from([2, 3]));
        assert_eq!(eager_in(&state, 2), BTreeSet::from([2, 3]));

        recv(&mut state, 3, Message::Prune(TreeId([1; 8])));
        assert_eq!(eager_in(&state, 1), BTreeSet::from([2]));
        assert_eq!(eager_in(&state, 2), BTreeSet::from([2, 3]));
        let graft = Graft {
            id: Some(MessageId::from_content(b"a")),
            tree: TreeId([2; 8]),
        };
        recv(&mut state, 3, Message::Prune(TreeId([2; 8])));
        let io = recv(&mut state, 3, Message::Graft(graft));
        assert_eq!(eager_in(&state, 2), BTreeSet::from([2, 3]));
        assert_eq!(
            eager_in(&state, 1),
            BTreeSet::from([2]),
            "grafted the wrong tree"
        );
        assert!(pushed_to(&io, 3).is_some(), "the graft got no payload");

        state.on_neighbor_down(2);
        assert!(state.trees.values().all(|tree| !tree.eager.contains(&2)));
        state.check_invariants();
    }

    /// An announcement names the tree of the graft it causes.
    #[test]
    fn graft_names_the_tree_of_the_announcement() {
        let mut state = State::new(1u32, Config::default(), 1024);
        state.on_neighbor_up(2);
        let id = MessageId::from_content(b"x");
        let tree = TreeId([9; 8]);
        let ihave = Message::IHave(vec![IHave {
            id,
            round: Round(1),
            tree,
        }]);
        recv(&mut state, 2, ihave);
        let mut io = VecDeque::new();
        state.handle(
            InEvent::TimerExpired(Timer::SendGraft(id)),
            Instant::now(),
            &mut io,
        );
        assert_eq!(
            sent_to(&io, 2),
            vec![Message::Graft(Graft { id: Some(id), tree })]
        );
    }

    /// A node keeps at most `MAX_TREES` trees, and drops the least recently used.
    #[test]
    fn trees_have_a_cap() {
        let mut state = State::new(1u32, Config::default(), 1024);
        let mut io = VecDeque::new();
        state.on_neighbor_up(2);
        let start = Instant::now();
        for i in 0..MAX_TREES + 10 {
            let now = start + Duration::from_millis(i as u64);
            let mut message = gossip(b"x", 1);
            message.content = Bytes::from(i.to_be_bytes().to_vec());
            message.id = MessageId::from_content(&message.content);
            message.tree = TreeId(((i + 1) as u64).to_be_bytes());
            state.handle(
                InEvent::RecvMessage(2, Message::Gossip(message)),
                now,
                &mut io,
            );
            io.clear();
        }
        assert_eq!(state.trees.len(), MAX_TREES);
        assert!(!state.trees.contains_key(&TreeId(1u64.to_be_bytes())));
        assert!(state
            .trees
            .contains_key(&TreeId(((MAX_TREES + 10) as u64).to_be_bytes())));

        // A tree that no message used for a while expires.
        let later = start + Duration::from_secs(2) + TREE_RETENTION;
        state.handle(InEvent::TimerExpired(Timer::EvictCache), later, &mut io);
        assert!(state.trees.is_empty());
    }

    /// Control messages and copies of a payload with tree ids we do not know create no tree.
    #[test]
    fn made_up_tree_ids_create_no_tree() {
        let mut state = State::new(1u32, Config::default(), 1024);
        state.on_neighbor_up(2);
        state.on_neighbor_up(3);
        recv(&mut state, 3, Message::Gossip(on_tree(1, b"a")));
        let now = Instant::now();
        let mut io = VecDeque::new();
        for i in 0..(MAX_TREES as u64 + 5) {
            let tree = TreeId((i + 100).to_be_bytes());
            let id = MessageId::from_content(&i.to_be_bytes());
            recv(&mut state, 2, Message::Prune(tree));
            recv(&mut state, 2, Message::Graft(Graft { id: None, tree }));
            let ihave = IHave {
                id,
                round: Round(1),
                tree,
            };
            recv(&mut state, 2, Message::IHave(vec![ihave]));
            state.handle(InEvent::TimerExpired(Timer::SendGraft(id)), now, &mut io);
            let mut copy = on_tree(1, b"a");
            copy.tree = tree;
            recv(&mut state, 2, Message::Gossip(copy));
        }
        assert_eq!(state.trees.len(), 1);
        assert_eq!(eager_in(&state, 1), BTreeSet::from([2, 3]));
    }

    /// The reply to a graft creates the tree, with the grafted peer eager.
    #[test]
    fn graft_reply_makes_the_peer_eager_in_a_new_tree() {
        let mut state = State::new(1u32, Config::default(), 1024);
        state.on_neighbor_up(2);
        state.on_neighbor_up(3);
        let now = Instant::now();
        state.add_lazy(TreeId::default(), 2, now);
        let message = on_tree(9, b"x");
        let ihave = IHave {
            id: message.id,
            round: Round(1),
            tree: message.tree,
        };
        recv(&mut state, 2, Message::IHave(vec![ihave]));
        let mut io = VecDeque::new();
        state.handle(
            InEvent::TimerExpired(Timer::SendGraft(message.id)),
            now,
            &mut io,
        );
        assert!(!state.trees.contains_key(&message.tree));
        recv(&mut state, 2, Message::Gossip(message));
        assert_eq!(eager_in(&state, 9), BTreeSet::from([2, 3]));
    }

    /// A graft makes its peer eager also if another payload created the tree before the reply.
    #[test]
    fn graft_reply_makes_the_peer_eager_in_a_tree_created_meanwhile() {
        let mut state = State::new(1u32, Config::default(), 1024);
        state.on_neighbor_up(2);
        state.on_neighbor_up(3);
        let now = Instant::now();
        state.add_lazy(TreeId::default(), 2, now);
        let message = on_tree(9, b"x");
        let ihave = IHave {
            id: message.id,
            round: Round(1),
            tree: message.tree,
        };
        recv(&mut state, 2, Message::IHave(vec![ihave]));
        let mut io = VecDeque::new();
        state.handle(
            InEvent::TimerExpired(Timer::SendGraft(message.id)),
            now,
            &mut io,
        );
        recv(&mut state, 3, Message::Gossip(on_tree(9, b"y")));
        assert_eq!(eager_in(&state, 9), BTreeSet::from([3]));
        recv(&mut state, 2, Message::Gossip(message));
        assert_eq!(eager_in(&state, 9), BTreeSet::from([2, 3]));
    }

    /// The tree of an origin is the first 8 bytes of the BLAKE3 hash of its encoding.
    #[test]
    fn tree_id_of_an_origin() {
        let origin = [5u8; 32];
        let hash = blake3::hash(&origin);
        assert_eq!(TreeId::of(&origin).0, hash.as_bytes()[..8]);
        assert_ne!(TreeId::of(&1u32), TreeId::of(&2u32));
        // Test vectors: the array, and an endpoint id, which encodes as its 32 bytes.
        assert_eq!(hex::encode(TreeId::of(&origin).0), "df6c9593e2faac45");
        let endpoint = iroh::SecretKey::from_bytes(&origin).public();
        assert_eq!(TreeId::of(&endpoint), TreeId::of(endpoint.as_bytes()));
        assert_eq!(hex::encode(TreeId::of(&endpoint).0), "90bdeef49d5d2664");
    }
}
