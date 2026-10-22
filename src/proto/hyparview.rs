//! Implementation of the HyParView membership protocol
//!
//! The implementation is based on [this paper][paper] by Joao Leitao, Jose Pereira, Luıs Rodrigues
//! and the [example implementation][impl] by Bartosz Sypytkowski
//!
//! [paper]: https://asc.di.fct.unl.pt/~jleitao/pdf/dsn07-leitao.pdf
//! [impl]: https://gist.github.com/Horusiath/84fac596101b197da0546d1697580d99

use std::collections::{hash_map, HashMap, HashSet, VecDeque};

use derive_more::{From, Sub};
use n0_future::time::Duration;
use rand::{rngs::ThreadRng, seq::IteratorRandom, Rng, RngExt};
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::{util::IndexSet, PeerData, PeerIdentity, PeerInfo, IO};

/// Delay before the first shuffle.
///
/// The interval doubles after each shuffle up to [`Config::shuffle_interval`].
/// A join leaves the passive view small and lopsided: it holds only the peers
/// that `ForwardJoin` walks passed by. The first shuffles fill it. The HyParView
/// paper shuffles at a fixed period, without jitter.
const INITIAL_SHUFFLE_INTERVAL: Duration = Duration::from_secs(1);

/// Interval of the check that refills a short active view.
///
/// The HyParView paper has no such check. Partisan runs the same check, its
/// "random promotion", every 5s.
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(5);

/// Maximum number of maintenance passes between two rejoins.
///
/// An isolated node rejoins in its 1st, 2nd, 4th, and 8th maintenance pass
/// alone, and then in every 12th: at first after 5s, and later once a minute.
const MAX_REJOIN_INTERVALS: u32 = 12;

/// Maximum number of contacts a topic remembers for a rejoin.
///
/// The topic keeps the contacts of the latest joins. An isolated node sends a
/// join to each of them.
const MAX_BOOTSTRAP_PEERS: usize = 8;

/// Maximum number of recent neighbors a topic remembers for a rejoin.
const MAX_RECENT_NEIGHBORS: usize = 8;

/// Input event for HyParView
#[derive(Debug)]
pub enum InEvent<PI> {
    /// A [`Message`] was received from a peer.
    RecvMessage(PI, Message<PI>),
    /// A timer has expired.
    TimerExpired(Timer<PI>),
    /// A peer was disconnected on the IO layer.
    PeerDisconnected(PI),
    /// Send a join request to a peer.
    RequestJoin(PI),
    /// Update the peer data that is transmitted on join requests.
    UpdatePeerData(PeerData),
    /// Quit the swarm, informing peers about us leaving.
    Quit,
}

/// Output event for HyParView
#[derive(Debug)]
pub enum OutEvent<PI> {
    /// Ask the IO layer to send a [`Message`] to peer `PI`.
    SendMessage(PI, Message<PI>),
    /// Schedule a [`Timer`].
    ScheduleTimer(Duration, Timer<PI>),
    /// Ask the IO layer to close the connection to peer `PI`.
    DisconnectPeer(PI),
    /// Emit an [`Event`] to the application.
    EmitEvent(Event<PI>),
    /// New [`PeerData`] was received for peer `PI`.
    PeerData(PI, PeerData),
}

/// Event emitted by the [`State`] to the application.
#[derive(Clone, Debug)]
pub enum Event<PI> {
    /// A peer was added to our set of active connections.
    NeighborUp(PI),
    /// A peer was removed from our set of active connections.
    NeighborDown(PI),
}

/// Kinds of timers HyParView needs to schedule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Timer<PI> {
    DoShuffle,
    DoMaintenance,
    /// The timeout of the `Neighbor` to the peer with the given id.
    ///
    /// The id tells it from the timer of an earlier `Neighbor` to the same peer.
    PendingNeighborRequest(PI, u64),
}

/// Messages that we can send and receive from peers within the topic.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Message<PI> {
    /// Sent to a peer if you want to join the swarm
    Join(Option<PeerData>),
    /// When receiving Join, ForwardJoin is forwarded to the peer's ActiveView to introduce the
    /// new member.
    ForwardJoin(ForwardJoin<PI>),
    /// A shuffle request is sent occasionally to re-shuffle the PassiveView with contacts from
    /// other peers.
    Shuffle(Shuffle<PI>),
    /// Peers reply to [`Message::Shuffle`] requests with a random peers from their active and
    /// passive views.
    ShuffleReply(ShuffleReply<PI>),
    /// Request to add sender to an active view of recipient. If [`Neighbor::priority`] is
    /// [`Priority::High`], the request cannot be denied.
    Neighbor(Neighbor),
    /// Request to disconnect from a peer.
    /// If [`Disconnect::alive`] is true, the other peer is not shutting down, so it should be
    /// added to the passive set.
    Disconnect(Disconnect),
}

/// The time-to-live for this message.
///
/// Each time a message is forwarded, the `Ttl` is decreased by 1. If the `Ttl` reaches 0, it
/// should not be forwarded further.
#[derive(From, Sub, Eq, PartialEq, Clone, Debug, Copy, Serialize, Deserialize)]
pub struct Ttl(pub u16);
impl Ttl {
    /// Returns `true` once the message may not be forwarded any further.
    pub fn expired(&self) -> bool {
        *self == Ttl(0)
    }
    /// Returns the `Ttl` to forward a message with, one hop lower.
    pub fn next(&self) -> Ttl {
        Ttl(self.0.saturating_sub(1))
    }
}

#[cfg(test)]
impl<PI> Message<PI> {
    /// Returns a shuffle from `origin` carrying no nodes, for tests outside this module.
    pub(crate) fn test_shuffle(origin: PI, ttl: u16) -> Self {
        Message::Shuffle(Shuffle {
            origin,
            nodes: Vec::new(),
            ttl: Ttl(ttl),
        })
    }

    /// Returns a `Disconnect` from a peer that stays alive, for tests outside this module.
    #[cfg(feature = "net")]
    pub(crate) fn test_disconnect() -> Self {
        Message::Disconnect(Disconnect {
            alive: true,
            _respond: false,
        })
    }

    /// Returns a shuffle reply carrying `peers` without data, for tests outside this module.
    pub(crate) fn test_shuffle_reply_with(peers: Vec<PI>) -> Self {
        let nodes = peers
            .into_iter()
            .map(|id| PeerInfo { id, data: None })
            .collect();
        Message::ShuffleReply(ShuffleReply { nodes })
    }
}

/// A message informing other peers that a new peer joined the swarm for this topic.
///
/// Will be forwarded in a random walk until `ttl` reaches 0.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ForwardJoin<PI> {
    /// The peer that newly joined the swarm
    peer: PeerInfo<PI>,
    /// The time-to-live for this message
    ttl: Ttl,
}

/// Shuffle messages are sent occasionally to shuffle our passive view with peers from other peer's
/// active and passive views.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Shuffle<PI> {
    /// The peer that initiated the shuffle request.
    origin: PI,
    /// A random subset of the active and passive peers of the `origin` peer.
    nodes: Vec<PeerInfo<PI>>,
    /// The time-to-live for this message.
    ttl: Ttl,
}

/// Once a shuffle messages reaches a [`Ttl`] of 0, a peer replies with a `ShuffleReply`.
///
/// The reply is sent to the peer that initiated the shuffle and contains a subset of the active
/// and passive views of the peer at the end of the random walk.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ShuffleReply<PI> {
    /// A random subset of the active and passive peers of the peer sending the `ShuffleReply`.
    nodes: Vec<PeerInfo<PI>>,
}

/// The priority of a `Join` message
///
/// This is `High` if the sender does not have any active peers, and `Low` otherwise.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Priority {
    /// High priority join that may not be denied.
    ///
    /// A peer may only send high priority joins if it doesn't have any active peers at the moment.
    High,
    /// Low priority join that can be denied.
    Low,
}

/// A neighbor message is sent after adding a peer to our active view to inform them that we are
/// now neighbors.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Neighbor {
    /// The priority of the `Join` or `ForwardJoin` message that triggered this neighbor request.
    priority: Priority,
    /// The user data of the peer sending this message.
    data: Option<PeerData>,
}

/// Message sent when leaving the swarm or closing down to inform peers about us being gone.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Disconnect {
    /// Whether we are actually shutting down or closing the connection only because our limits are
    /// reached.
    alive: bool,
    /// Obsolete field (kept in the struct to maintain wire compatibility).
    _respond: bool,
}

/// Configuration for the swarm membership layer
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Number of peers to which active connections are maintained
    pub active_view_capacity: usize,
    /// Number of peers for which contact information is remembered,
    /// but to which we are not actively connected to.
    pub passive_view_capacity: usize,
    /// Number of hops a `ForwardJoin` message is propagated until the new peer's info
    /// is added to a peer's active view.
    pub active_random_walk_length: Ttl,
    /// Number of hops a `ForwardJoin` message is propagated until the new peer's info
    /// is added to a peer's passive view.
    pub passive_random_walk_length: Ttl,
    /// Number of hops a `Shuffle` message is propagated until a peer replies to it.
    pub shuffle_random_walk_length: Ttl,
    /// Number of active peers to be included in a `Shuffle` request.
    pub shuffle_active_view_count: usize,
    /// Number of passive peers to be included in a `Shuffle` request.
    pub shuffle_passive_view_count: usize,
    /// Interval between shuffle requests, once the first shuffles are done.
    ///
    /// Shuffles keep the passive view a live sample of the swarm. A node
    /// falls back on its passive view when its neighbors fail.
    pub shuffle_interval: Duration,
    /// Timeout after which a neighbor request is considered failed.
    ///
    /// It runs from when we send the request, so it covers a dial to the peer
    /// on a slow network. A dial that fails ends the request earlier.
    pub neighbor_request_timeout: Duration,
}
impl Default for Config {
    /// Default values for the HyParView layer
    fn default() -> Self {
        Self {
            // From the paper (p9)
            active_view_capacity: 5,
            // From the paper (p9)
            passive_view_capacity: 30,
            // From the paper (p9)
            active_random_walk_length: Ttl(6),
            // From the paper (p9)
            passive_random_walk_length: Ttl(3),
            // From the paper (p9)
            shuffle_random_walk_length: Ttl(6),
            // From the paper (p9)
            shuffle_active_view_count: 3,
            // From the paper (p9)
            shuffle_passive_view_count: 4,
            // Wild guess
            shuffle_interval: Duration::from_secs(60),
            // A dial and a round trip on a slow network. Each dead candidate
            // holds a refill slot this long.
            neighbor_request_timeout: Duration::from_secs(3),
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct Stats {
    total_connections: usize,
}

/// The state of the HyParView protocol
#[derive(Debug)]
pub struct State<PI, RG = ThreadRng> {
    /// Our peer identity
    me: PI,
    /// Our opaque user data to transmit to peers on join messages
    me_data: Option<PeerData>,
    /// The active view, i.e. peers we are connected to
    pub(crate) active_view: IndexSet<PI>,
    /// The passive view, i.e. peers we know about but are not connected to at the moment
    pub(crate) passive_view: IndexSet<PI>,
    /// Protocol configuration (cannot change at runtime)
    config: Config,
    /// Whether the shuffle and maintenance timers are scheduled
    timers_scheduled: bool,
    /// Delay before the next shuffle, see [`INITIAL_SHUFFLE_INTERVAL`].
    next_shuffle_interval: Duration,
    /// The latest peers the application asked us to join, oldest first.
    ///
    /// A node with empty views has only them and its recent neighbors left to
    /// dial. See [`MAX_BOOTSTRAP_PEERS`].
    bootstrap_peers: VecDeque<PI>,
    /// Maintenance passes in a row with both views empty, see [`MAX_REJOIN_INTERVALS`].
    passes_alone: u32,
    /// The latest peers that were our neighbors, least recently seen first.
    ///
    /// Peers leave when newer neighbors push them out, or when they leave the
    /// topic. They do not leave because they failed: a node that loses its own
    /// network takes every peer as failed, and empties its views of exactly the
    /// peers it needs once it is back. See [`MAX_RECENT_NEIGHBORS`].
    recent_neighbors: VecDeque<PI>,
    /// Random number generator
    rng: RG,
    /// Statistics
    pub(crate) stats: Stats,
    /// The neighbor requests we sent out but did not yet receive a reply for, with their ids
    pending_neighbor_requests: HashMap<PI, u64>,
    /// The id of the next neighbor request
    next_neighbor_request: u64,
    /// The opaque user peer data we received for other peers
    peer_data: HashMap<PI, PeerData>,
    /// List of peers that are disconnecting, but which we want to keep in the passive set once the connection closes
    alive_disconnect_peers: HashSet<PI>,
    /// Passive peers that refused our neighbor request since the last maintenance pass.
    ///
    /// A refill skips them. Else a node short of neighbors, whose passive peers
    /// are all full, asks them in turn without end.
    refused: HashSet<PI>,
}

impl<PI, RG> State<PI, RG>
where
    PI: PeerIdentity,
    RG: Rng,
{
    pub fn new(me: PI, me_data: Option<PeerData>, config: Config, rng: RG) -> Self {
        Self {
            me,
            me_data,
            active_view: IndexSet::new(),
            passive_view: IndexSet::new(),
            next_shuffle_interval: INITIAL_SHUFFLE_INTERVAL.min(config.shuffle_interval),
            config,
            timers_scheduled: false,
            bootstrap_peers: VecDeque::new(),
            passes_alone: 0,
            recent_neighbors: VecDeque::new(),
            rng,
            stats: Stats::default(),
            pending_neighbor_requests: Default::default(),
            next_neighbor_request: 0,
            peer_data: Default::default(),
            alive_disconnect_peers: Default::default(),
            refused: Default::default(),
        }
    }

    pub fn handle(&mut self, event: InEvent<PI>, io: &mut impl IO<PI>) {
        match event {
            InEvent::RecvMessage(from, message) => self.handle_message(from, message, io),
            InEvent::TimerExpired(timer) => match timer {
                Timer::DoShuffle => self.handle_shuffle_timer(io),
                Timer::DoMaintenance => self.handle_maintenance_timer(io),
                Timer::PendingNeighborRequest(peer, id) => {
                    self.handle_pending_neighbor_timer(peer, id, io)
                }
            },
            InEvent::PeerDisconnected(peer) => self.handle_connection_closed(peer, io),
            InEvent::RequestJoin(peer) => self.handle_join(peer, io),
            InEvent::UpdatePeerData(data) => {
                self.me_data = Some(data);
            }
            InEvent::Quit => self.handle_quit(io),
        }

        // this will only happen on the first call
        if !self.timers_scheduled {
            self.timers_scheduled = true;
            self.schedule_shuffle(io);
            io.push(OutEvent::ScheduleTimer(
                MAINTENANCE_INTERVAL,
                Timer::DoMaintenance,
            ));
        }

        #[cfg(test)]
        self.check_invariants();
    }

    /// Panics if the per-peer bookkeeping disagrees with the views.
    ///
    /// Runs after every event in the unit tests. Every leak fixed so far
    /// was an entry that outlived its peer's place in a view.
    #[cfg(test)]
    fn check_invariants(&self) {
        let in_a_view =
            |peer: &PI| self.active_view.contains(peer) || self.passive_view.contains(peer);
        assert!(
            self.active_view.len() <= self.config.active_view_capacity,
            "active view over capacity: {:?}",
            self.active_view
        );
        assert!(
            self.passive_view.len() <= self.config.passive_view_capacity,
            "passive view over capacity: {:?}",
            self.passive_view
        );
        assert!(!in_a_view(&self.me), "we are in our own view");
        for peer in self.active_view.iter() {
            assert!(
                !self.passive_view.contains(peer),
                "{peer:?} is in both views"
            );
        }
        for peer in self.peer_data.keys() {
            // A forwarded join stores the peer's data while our request to it is out.
            assert!(
                in_a_view(peer) || self.pending_neighbor_requests.contains_key(peer),
                "data kept for {peer:?}, which is in no view and not asked"
            );
        }
        for peer in self.alive_disconnect_peers.iter() {
            assert!(
                self.passive_view.contains(peer),
                "{peer:?} marked alive but not passive"
            );
        }
    }

    fn handle_message(&mut self, from: PI, message: Message<PI>, io: &mut impl IO<PI>) {
        let is_disconnect = matches!(message, Message::Disconnect(Disconnect { .. }));
        if !is_disconnect && !self.active_view.contains(&from) {
            self.stats.total_connections += 1;
        }
        match message {
            Message::Join(data) => self.on_join(from, data, io),
            Message::ForwardJoin(details) => self.on_forward_join(from, details, io),
            Message::Shuffle(details) => self.on_shuffle(from, details, io),
            Message::ShuffleReply(details) => self.on_shuffle_reply(from, details, io),
            Message::Neighbor(details) => self.on_neighbor(from, details, io),
            Message::Disconnect(details) => self.on_disconnect(from, details, io),
        }

        // Disconnect from passive nodes right after receiving a message, a
        // `Disconnect` included.
        self.release_if_unused(from, io);
    }

    /// Disconnects `peer` unless it is a neighbor or we wait for its `Neighbor` reply.
    ///
    /// The reply can come on the connection, and a closed connection would lose it.
    fn release_if_unused(&self, peer: PI, io: &mut impl IO<PI>) {
        if !self.active_view.contains(&peer) && !self.pending_neighbor_requests.contains_key(&peer)
        {
            io.push(OutEvent::DisconnectPeer(peer));
        }
    }

    fn handle_join(&mut self, peer: PI, io: &mut impl IO<PI>) {
        self.bootstrap_peers.retain(|known| *known != peer);
        self.bootstrap_peers.push_back(peer);
        if self.bootstrap_peers.len() > MAX_BOOTSTRAP_PEERS {
            self.bootstrap_peers.pop_front();
        }
        self.join(peer, io);
    }

    /// Sends a join to `peer`.
    fn join(&self, peer: PI, io: &mut impl IO<PI>) {
        io.push(OutEvent::SendMessage(
            peer,
            Message::Join(self.me_data.clone()),
        ));
    }

    /// We received a disconnect message.
    fn on_disconnect(&mut self, peer: PI, details: Disconnect, io: &mut impl IO<PI>) {
        if !details.alive {
            // The peer left the topic, a rejoin through it would fail.
            self.recent_neighbors.retain(|known| *known != peer);
        }
        let requested = self.pending_neighbor_requests.remove(&peer).is_some();
        if requested && !self.active_view.contains(&peer) {
            self.refused.insert(peer);
        }
        if self.active_view.contains(&peer) {
            self.remove_active(
                &peer,
                RemovalReason::DisconnectReceived {
                    is_alive: details.alive,
                },
                io,
            );
        } else if details.alive && self.passive_view.contains(&peer) {
            self.alive_disconnect_peers.insert(peer);
        }
    }

    /// A connection was closed by the peer.
    fn handle_connection_closed(&mut self, peer: PI, io: &mut impl IO<PI>) {
        let requested = self.pending_neighbor_requests.remove(&peer).is_some();
        if self.active_view.contains(&peer) {
            self.remove_active(&peer, RemovalReason::ConnectionClosed, io);
        } else {
            if !self.alive_disconnect_peers.remove(&peer) {
                self.passive_view.remove(&peer);
                self.peer_data.remove(&peer);
            }
            // The request failed with its dial, long before its timer fires.
            if requested {
                self.refill_active_from_passive(&[&peer], io);
            }
        }
    }

    fn handle_quit(&mut self, io: &mut impl IO<PI>) {
        for peer in self.active_view.clone().into_iter() {
            self.active_view.remove(&peer);
            self.send_disconnect(peer, false, io);
        }
        self.pending_neighbor_requests.clear();
        self.passive_view = IndexSet::new();
        self.peer_data.clear();
        self.alive_disconnect_peers.clear();
        self.refused.clear();
    }

    fn send_disconnect(&mut self, peer: PI, alive: bool, io: &mut impl IO<PI>) {
        // Before disconnecting, send a `ShuffleReply` with some of our nodes to
        // prevent the other node from running out of connections. This is especially
        // relevant if the other node just joined the swarm.
        self.send_shuffle_reply(
            peer,
            self.config.shuffle_active_view_count + self.config.shuffle_passive_view_count,
            io,
        );
        let message = Message::Disconnect(Disconnect {
            alive,
            _respond: false,
        });
        io.push(OutEvent::SendMessage(peer, message));
        io.push(OutEvent::DisconnectPeer(peer));
    }

    fn on_join(&mut self, peer: PI, data: Option<PeerData>, io: &mut impl IO<PI>) {
        // Drop a neighbor request we may still have pending to this peer. The peer
        // is joining, usually after a restart, so it has no memory of a request we
        // sent it before and will not answer it. While that request is pending,
        // `send_neighbor` would send nothing, and the Join would go unanswered.
        // The request and the Join can also cross in flight: another of the
        // peer's bootstrap peers forwarded its join to us, and we sent the peer a
        // request before its own Join reached us. Then the peer gets one Neighbor
        // more than needed and takes it as the reply to its own.
        self.pending_neighbor_requests.remove(&peer);
        // "A node that receives a join request will start by adding the new
        // node to its active view, even if it has to drop a random node from it. (6)"
        self.add_active(peer, data.clone(), Priority::High, true, io);

        // "The contact node c will then send to all other nodes in its active view a ForwardJoin
        // request containing the new node identifier. Associated to the join procedure,
        // there are two configuration parameters, named Active Random Walk Length (ARWL),
        // that specifies the maximum number of hops a ForwardJoin request is propagated,
        // and Passive Random Walk Length (PRWL), that specifies at which point in the walk the node
        // is inserted in a passive view. To use these parameters, the ForwardJoin request carries
        // a “time to live” field that is initially set to ARWL and decreased at every hop. (7)"
        let ttl = self.config.active_random_walk_length;
        let peer_info = PeerInfo { id: peer, data };
        for node in self.active_view.iter_without(&peer) {
            let message = Message::ForwardJoin(ForwardJoin {
                peer: peer_info.clone(),
                ttl,
            });
            io.push(OutEvent::SendMessage(*node, message));
        }
    }

    fn on_forward_join(&mut self, sender: PI, message: ForwardJoin<PI>, io: &mut impl IO<PI>) {
        let peer_id = message.peer.id;
        // If the peer is already in our active view, we renew our neighbor relationship.
        if self.active_view.contains(&peer_id) {
            self.insert_peer_info(message.peer, io);
            self.send_neighbor(peer_id, Priority::High, io);
        }
        // "i) If the time to live is equal to zero or if the number of nodes in p’s active view is equal to one,
        // it will add the new node to its active view (7)"
        else if message.ttl.expired() || self.active_view.len() <= 1 {
            self.insert_peer_info(message.peer, io);
            // Modification from paper: Instead of adding the peer directly to our active view,
            // we only send the Neighbor message. We will add the peer to our active view once we receive a
            // reply from our neighbor.
            // This prevents us adding unreachable peers to our active view.
            self.send_neighbor(peer_id, Priority::High, io);
        } else {
            // "ii) If the time to live is equal to PRWL, p will insert the new node into its passive view"
            if message.ttl == self.config.passive_random_walk_length {
                self.add_passive(peer_id, message.peer.data.clone(), io);
            }
            // "iii) The time to live field is decremented."
            // "iv) If, at this point, n has not been inserted
            // in p’s active view, p will forward the request to a random node in its active view
            // (different from the one from which the request was received)."
            if !self.active_view.contains(&peer_id)
                && !self.pending_neighbor_requests.contains_key(&peer_id)
            {
                match self
                    .active_view
                    .pick_random_without(&[&sender], &mut self.rng)
                {
                    None => {
                        unreachable!("if the peer was not added, there are at least two peers in our active view.");
                    }
                    Some(next) => {
                        let message = Message::ForwardJoin(ForwardJoin {
                            peer: message.peer,
                            ttl: message.ttl.next(),
                        });
                        io.push(OutEvent::SendMessage(*next, message));
                    }
                }
            }
        }
    }

    fn on_neighbor(&mut self, from: PI, details: Neighbor, io: &mut impl IO<PI>) {
        // A `Neighbor` is a reply if we have a request out to the peer, and a
        // request otherwise. See `send_neighbor` for the full picture.
        let is_reply = self.pending_neighbor_requests.remove(&from).is_some();
        let do_reply = !is_reply;
        // "A node q that receives a high priority neighbor request will always accept the request, even
        // if it has to drop a random member from its active view (again, the member that is dropped will
        // receive a Disconnect notification). If a node q receives a low priority Neighbor request, it will
        // only accept the request if it has a free slot in its active view, otherwise it will refuse the request."
        if !self.add_active(from, details.data, details.priority, do_reply, io) {
            self.send_disconnect(from, true, io);
            // `add_active` stored the peer's data before it refused the peer.
            self.forget_peer(&from);
        }
    }

    /// Get the peer [`PeerInfo`] for a peer.
    fn peer_info(&self, id: &PI) -> PeerInfo<PI> {
        let data = self.peer_data.get(id).cloned();
        PeerInfo { id: *id, data }
    }

    fn insert_peer_info(&mut self, peer_info: PeerInfo<PI>, io: &mut impl IO<PI>) {
        if peer_info.id == self.me {
            return;
        }
        if let Some(data) = peer_info.data {
            let old = self.peer_data.remove(&peer_info.id);
            let same = matches!(old, Some(old) if old == data);
            if !same && !data.0.is_empty() {
                io.push(OutEvent::PeerData(peer_info.id, data.clone()));
            }
            self.peer_data.insert(peer_info.id, data);
        }
    }

    /// Handle a [`Message::Shuffle`]
    ///
    /// > A node q that receives a Shuffle request will first decrease its time to live. If the time
    /// > to live of the message is greater than zero and the number of nodes in q’s active view is
    /// > greater than 1, the node will select a random node from its active view, different from the
    /// > one he received this shuffle message from, and simply forwards the Shuffle request.
    /// > Otherwise, node q accepts the Shuffle request and send back (p.8)
    fn on_shuffle(&mut self, from: PI, shuffle: Shuffle<PI>, io: &mut impl IO<PI>) {
        if shuffle.ttl.expired() || self.active_view.len() <= 1 {
            let len = shuffle.nodes.len();
            for node in shuffle.nodes {
                self.add_passive(node.id, node.data, io);
            }
            self.send_shuffle_reply(shuffle.origin, len, io);
            // The origin is usually no neighbor, and the reply its only use.
            self.release_if_unused(shuffle.origin, io);
        } else if let Some(node) = self
            .active_view
            .pick_random_without(&[&shuffle.origin, &from], &mut self.rng)
        {
            let message = Message::Shuffle(Shuffle {
                origin: shuffle.origin,
                nodes: shuffle.nodes,
                ttl: shuffle.ttl.next(),
            });
            io.push(OutEvent::SendMessage(*node, message));
        }
    }

    fn send_shuffle_reply(&mut self, to: PI, len: usize, io: &mut impl IO<PI>) {
        let mut nodes = self.passive_view.shuffled_and_capped(len, &mut self.rng);
        // If we don't have enough passive nodes for the expected length, we fill with
        // active nodes.
        if nodes.len() < len {
            nodes.extend(
                self.active_view
                    .shuffled_and_capped(len - nodes.len(), &mut self.rng),
            );
        }
        let nodes = nodes.into_iter().map(|id| self.peer_info(&id));
        let message = Message::ShuffleReply(ShuffleReply {
            nodes: nodes.collect(),
        });
        io.push(OutEvent::SendMessage(to, message));
    }

    fn on_shuffle_reply(&mut self, from: PI, message: ShuffleReply<PI>, io: &mut impl IO<PI>) {
        for node in message.nodes {
            self.add_passive(node.id, node.data, io);
        }
        // A peer that refuses our request sends this reply first. Only a short
        // node goes on to the next passive peer at once, and skips the peers in
        // `refused`.
        let refusal = self.pending_neighbor_requests.contains_key(&from);
        if !refusal || self.is_short() {
            self.refill_active_from_passive(&[], io);
        }
    }

    fn handle_shuffle_timer(&mut self, io: &mut impl IO<PI>) {
        if let Some(node) = self.active_view.pick_random(&mut self.rng) {
            let active = self.active_view.shuffled_without_and_capped(
                &[node],
                self.config.shuffle_active_view_count,
                &mut self.rng,
            );
            let passive = self.passive_view.shuffled_without_and_capped(
                &[node],
                self.config.shuffle_passive_view_count,
                &mut self.rng,
            );
            let nodes = active
                .iter()
                .chain(passive.iter())
                .map(|id| self.peer_info(id));
            let me = PeerInfo {
                id: self.me,
                data: self.me_data.clone(),
            };
            let nodes = nodes.chain([me]);
            let message = Shuffle {
                origin: self.me,
                nodes: nodes.collect(),
                ttl: self.config.shuffle_random_walk_length,
            };
            io.push(OutEvent::SendMessage(*node, Message::Shuffle(message)));
        }
        self.schedule_shuffle(io);
    }

    /// Refills the active view if it is short, and rejoins if both views are empty.
    ///
    /// A refill otherwise starts only when a neighbor leaves, and it stops once
    /// the passive view has no peer left to try. A node with no neighbors cannot
    /// shuffle either, so its passive view stays empty. Only the bootstrap
    /// peers and the recent neighbors can get it back into the swarm.
    fn handle_maintenance_timer(&mut self, io: &mut impl IO<PI>) {
        self.refused.clear();
        // Neighbors that stay count as seen again, so that churn does not push
        // them out.
        let active: Vec<_> = self.active_view.iter().copied().collect();
        for peer in active {
            self.remember_neighbor(peer);
        }
        if self.active_view.is_empty() && self.passive_view.is_empty() {
            self.passes_alone += 1;
            let n = self.passes_alone;
            let rejoin = if n < MAX_REJOIN_INTERVALS {
                n.is_power_of_two()
            } else {
                n.is_multiple_of(MAX_REJOIN_INTERVALS)
            };
            if rejoin {
                debug!("no neighbors and no passive peers, rejoin");
                for peer in self.bootstrap_peers.iter() {
                    self.join(*peer, io);
                }
                // The bootstrap peers may be gone for good. Also try a random
                // recent neighbor that is not one of them.
                let recent = self
                    .recent_neighbors
                    .iter()
                    .filter(|peer| !self.bootstrap_peers.contains(peer))
                    .choose(&mut self.rng);
                if let Some(peer) = recent {
                    self.join(*peer, io);
                }
            }
        } else {
            self.passes_alone = 0;
            if self.is_short() {
                self.refill_active_from_passive(&[], io);
            }
        }
        io.push(OutEvent::ScheduleTimer(
            MAINTENANCE_INTERVAL,
            Timer::DoMaintenance,
        ));
    }

    /// Schedules the next shuffle, with the interval doubled up to its maximum.
    ///
    /// The delay varies by 25 percent either way. Else the nodes of a swarm
    /// that started together shuffle at the same moments for good.
    fn schedule_shuffle(&mut self, io: &mut impl IO<PI>) {
        let interval = self.next_shuffle_interval;
        self.next_shuffle_interval = (interval * 2).min(self.config.shuffle_interval);
        let jitter = self.rng.random_range(0.75..1.25);
        io.push(OutEvent::ScheduleTimer(
            interval.mul_f64(jitter),
            Timer::DoShuffle,
        ));
    }

    /// Returns whether the active view is below half its capacity.
    ///
    /// One slot short is the normal state: the peer that would fill it is
    /// usually full itself. Partisan refills below half its capacity.
    fn is_short(&self) -> bool {
        self.active_view.len() < self.config.active_view_capacity.div_ceil(2)
    }

    fn passive_is_full(&self) -> bool {
        self.passive_view.len() >= self.config.passive_view_capacity
    }

    fn active_is_full(&self) -> bool {
        self.active_view.len() >= self.config.active_view_capacity
    }

    /// Add a peer to the passive view.
    ///
    /// If the passive view is full, it will first remove a random peer and then insert the new peer.
    /// If a peer is currently in the active view it will not be added.
    fn add_passive(&mut self, peer: PI, data: Option<PeerData>, io: &mut impl IO<PI>) {
        self.insert_peer_info((peer, data).into(), io);
        if self.active_view.contains(&peer) || self.passive_view.contains(&peer) || peer == self.me
        {
            return;
        }
        if self.passive_is_full() {
            if let Some(evicted) = self.passive_view.remove_random(&mut self.rng) {
                self.forget_peer(&evicted);
            }
        }
        self.passive_view.insert(peer);
    }

    /// Drops the metadata we hold for a peer, unless it is still in a view.
    ///
    /// `peer_data` and `alive_disconnect_peers` describe peers in a view, so
    /// every path out of a view has to clear them. The membership check is for
    /// callers that cannot tell: a neighbor request can time out after the peer
    /// joined the active view through a forwarded join.
    fn forget_peer(&mut self, peer: &PI) {
        if self.active_view.contains(peer) || self.passive_view.contains(peer) {
            return;
        }
        self.peer_data.remove(peer);
        self.alive_disconnect_peers.remove(peer);
    }

    /// Remove a peer from the active view.
    ///
    /// If `reason` is [`RemovalReason::Random`], a [`Disconnect`] message will be sent to the peer.
    fn remove_active(&mut self, peer: &PI, reason: RemovalReason, io: &mut impl IO<PI>) {
        if let Some(idx) = self.active_view.get_index_of(peer) {
            let removed_peer = self.remove_active_by_index(idx, reason, io).unwrap();
            self.refill_active_from_passive(&[&removed_peer], io);
        }
    }

    fn refill_active_from_passive(&mut self, skip_peers: &[&PI], io: &mut impl IO<PI>) {
        // A pending reply is to a peer already in the active view; count it once.
        let pending_outside = self
            .pending_neighbor_requests
            .keys()
            .filter(|peer| !self.active_view.contains(*peer))
            .count();
        if self.active_view.len() + pending_outside >= self.config.active_view_capacity {
            return;
        }
        // "When a node p suspects that one of the nodes present in its active view has failed
        // (by either disconnecting or blocking), it selects a random node q from its passive view and
        // attempts to establish a TCP connection with q. If the connection fails to establish,
        // node q is considered failed and removed from p’s passive view; another node q′ is selected
        // at random and a new attempt is made. The procedure is repeated until a connection is established
        // with success." (p7)
        let mut skip_peers = skip_peers.to_vec();
        skip_peers.extend(self.pending_neighbor_requests.keys());
        skip_peers.extend(self.refused.iter());

        if let Some(node) = self
            .passive_view
            .pick_random_without(&skip_peers, &mut self.rng)
            .copied()
        {
            let priority = match self.active_view.is_empty() {
                true => Priority::High,
                false => Priority::Low,
            };
            self.send_neighbor(node, priority, io);
        };
    }

    /// Handles a `Neighbor` to `peer` that was not answered in time.
    ///
    /// For a request, the peer is taken as failed and dropped from the passive
    /// view. For a reply, which is never answered, the peer is in the active
    /// view and keeps its data; the entry is cleared, and the refill below
    /// only acts if the active view has room. A timer whose `id` is not the
    /// pending one belongs to an earlier `Neighbor`, and does nothing.
    fn handle_pending_neighbor_timer(&mut self, peer: PI, id: u64, io: &mut impl IO<PI>) {
        if self.pending_neighbor_requests.get(&peer) == Some(&id) {
            self.pending_neighbor_requests.remove(&peer);
            self.passive_view.remove(&peer);
            self.forget_peer(&peer);
            // A passive message may have kept the connection for the reply.
            self.release_if_unused(peer, io);
            self.refill_active_from_passive(&[], io);
        }
    }

    fn remove_active_by_index(
        &mut self,
        peer_index: usize,
        reason: RemovalReason,
        io: &mut impl IO<PI>,
    ) -> Option<PI> {
        if let Some(peer) = self.active_view.remove_index(peer_index) {
            io.push(OutEvent::EmitEvent(Event::NeighborDown(peer)));

            match reason {
                // send a disconnect message, then close connection.
                RemovalReason::Random => self.send_disconnect(peer, true, io),
                // close connection without sending anything further.
                RemovalReason::DisconnectReceived { is_alive: _ } => {
                    io.push(OutEvent::DisconnectPeer(peer))
                }
                RemovalReason::ConnectionClosed => io.push(OutEvent::DisconnectPeer(peer)),
            }

            let keep_as_passive = match reason {
                // keep alive if previously marked as alive.
                RemovalReason::ConnectionClosed => self.alive_disconnect_peers.remove(&peer),
                // keep alive if other peer said to be still alive.
                RemovalReason::DisconnectReceived { is_alive } => is_alive,
                // keep alive (only we are removing for now)
                RemovalReason::Random => true,
            };

            if keep_as_passive {
                let data = self.peer_data.remove(&peer);
                self.add_passive(peer, data, io);
                // mark peer as alive, so it doesn't get removed from the passive view if the conn closes.
                if !matches!(reason, RemovalReason::ConnectionClosed) {
                    self.alive_disconnect_peers.insert(peer);
                }
            } else {
                self.forget_peer(&peer);
            }
            debug!(other = ?peer, "removed from active view, reason: {reason:?}");
            Some(peer)
        } else {
            None
        }
    }

    /// Remove a random peer from the active view.
    fn free_random_slot_in_active_view(&mut self, io: &mut impl IO<PI>) {
        if let Some(index) = self.active_view.pick_random_index(&mut self.rng) {
            self.remove_active_by_index(index, RemovalReason::Random, io);
        }
    }

    /// Add a peer to the active view.
    ///
    /// If the active view is currently full, a random peer will be removed first.
    /// Sends a Neighbor message to the peer. If high_priority is true, the peer
    /// may not deny the Neighbor request.
    fn add_active(
        &mut self,
        peer: PI,
        data: Option<PeerData>,
        priority: Priority,
        reply: bool,
        io: &mut impl IO<PI>,
    ) -> bool {
        if peer == self.me {
            return false;
        }
        self.insert_peer_info((peer, data).into(), io);
        if self.active_view.contains(&peer) {
            if reply {
                self.send_neighbor(peer, priority, io);
            }
            return true;
        }
        match (priority, self.active_is_full()) {
            (Priority::High, is_full) => {
                if is_full {
                    self.free_random_slot_in_active_view(io);
                }
                self.add_active_unchecked(peer, Priority::High, reply, io);
                true
            }
            (Priority::Low, false) => {
                self.add_active_unchecked(peer, Priority::Low, reply, io);
                true
            }
            (Priority::Low, true) => false,
        }
    }

    fn add_active_unchecked(
        &mut self,
        peer: PI,
        priority: Priority,
        reply: bool,
        io: &mut impl IO<PI>,
    ) {
        self.passive_view.remove(&peer);
        self.alive_disconnect_peers.remove(&peer);
        if self.active_view.insert(peer) {
            debug!(other = ?peer, "add to active view");
            self.remember_neighbor(peer);
            io.push(OutEvent::EmitEvent(Event::NeighborUp(peer)));
            if reply {
                self.send_neighbor(peer, priority, io);
            }
        }
    }

    /// Moves a peer to the end of the recent neighbors, see [`MAX_RECENT_NEIGHBORS`].
    fn remember_neighbor(&mut self, peer: PI) {
        self.recent_neighbors.retain(|known| *known != peer);
        self.recent_neighbors.push_back(peer);
        if self.recent_neighbors.len() > MAX_RECENT_NEIGHBORS {
            self.recent_neighbors.pop_front();
        }
    }

    /// Sends a `Neighbor` unless one is pending for the peer, and arms its timer.
    ///
    /// `Neighbor` is both the request to become neighbors and the reply that
    /// accepts it. `on_neighbor` tells them apart by `pending_neighbor_requests`:
    /// a `Neighbor` from a peer we have one pending for is the reply, any other
    /// is a request. Replies are recorded as pending too. That is what stops two
    /// peers from answering each other forever when a request is duplicated: the
    /// extra `Neighbor` is taken as the reply and not answered. The entry for a
    /// reply is cleared by the timer alone.
    fn send_neighbor(&mut self, peer: PI, priority: Priority, io: &mut impl IO<PI>) {
        if let hash_map::Entry::Vacant(entry) = self.pending_neighbor_requests.entry(peer) {
            let id = self.next_neighbor_request;
            self.next_neighbor_request += 1;
            entry.insert(id);
            let message = Message::Neighbor(Neighbor {
                priority,
                data: self.me_data.clone(),
            });
            io.push(OutEvent::SendMessage(peer, message));
            io.push(OutEvent::ScheduleTimer(
                self.config.neighbor_request_timeout,
                Timer::PendingNeighborRequest(peer, id),
            ));
        }
    }
}

#[derive(Debug)]
enum RemovalReason {
    /// A peer is removed because the connection was closed ungracefully.
    ConnectionClosed,
    /// A peer is removed because we received a disconnect message.
    DisconnectReceived { is_alive: bool },
    /// A peer is removed after random selection to make room for a newly joined peer.
    Random,
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use rand::{rngs::StdRng, SeedableRng};

    use super::*;
    use crate::proto::topic::{self, OutEvent as TopicOut};

    type Io = VecDeque<TopicOut<u32>>;

    /// A state for peer `0` with the default config.
    fn new_state() -> State<u32, StdRng> {
        State::new(0, None, Config::default(), StdRng::seed_from_u64(1))
    }

    /// Seeds the metadata a peer accumulates while it is known to us.
    fn seed_metadata(state: &mut State<u32, StdRng>, peer: u32) {
        state.peer_data.insert(peer, PeerData::new(vec![1]));
        state.alive_disconnect_peers.insert(peer);
    }

    /// Returns whether any metadata for `peer` is left.
    fn has_metadata(state: &State<u32, StdRng>, peer: u32) -> bool {
        state.peer_data.contains_key(&peer) || state.alive_disconnect_peers.contains(&peer)
    }

    /// Returns whether a `Neighbor` to `peer` was sent.
    fn sent_neighbor(io: &Io, peer: u32) -> bool {
        io.iter().any(|event| {
            matches!(
                event,
                TopicOut::SendMessage(to, topic::Message::Swarm(Message::Neighbor(_))) if *to == peer
            )
        })
    }

    /// A join from a peer we still hold as a neighbor is answered.
    ///
    /// The peer restarted while a neighbor request to it was pending. The
    /// request is void and must not silence our answer.
    #[test]
    fn join_from_a_held_neighbor_is_answered() {
        let mut state = new_state();
        let io = &mut Io::new();
        state.active_view.insert(1);
        state.pending_neighbor_requests.insert(1, 0);

        state.handle(InEvent::RecvMessage(1, Message::Join(None)), io);

        assert!(sent_neighbor(io, 1), "the join was not answered");
    }

    /// Every `Neighbor` sent arms its timer, not only those refilling the active view.
    ///
    /// Without the timer a request the peer never answers blocks every later
    /// request to that peer.
    #[test]
    fn neighbor_request_schedules_a_timeout() {
        let mut state = new_state();
        let io = &mut Io::new();

        state.handle(InEvent::RecvMessage(1, Message::Join(None)), io);

        assert!(sent_neighbor(io, 1));
        let timer = topic::Timer::Swarm(Timer::PendingNeighborRequest(1, 0));
        assert!(
            io.iter()
                .any(|event| matches!(event, TopicOut::ScheduleTimer(_, t) if *t == timer)),
            "no timeout scheduled for the neighbor request"
        );
    }

    /// Delivers the `Swarm` messages between `a` and `b` until neither has any left.
    ///
    /// Returns the number of messages ferried. Bounded, so two peers answering
    /// each other without end fail the test instead of hanging it.
    fn ferry(a: &mut State<u32, StdRng>, b: &mut State<u32, StdRng>, io: &mut Io) -> usize {
        let (a_id, b_id) = (a.me, b.me);
        let mut count = 0;
        for _ in 0..20 {
            let outgoing: Vec<_> = io
                .drain(..)
                .filter_map(|event| match event {
                    TopicOut::SendMessage(to, topic::Message::Swarm(message)) => {
                        Some((to, message))
                    }
                    _ => None,
                })
                .collect();
            if outgoing.is_empty() {
                return count;
            }
            for (to, message) in outgoing {
                count += 1;
                if to == b_id {
                    b.handle(InEvent::RecvMessage(a_id, message), io);
                } else if to == a_id {
                    a.handle(InEvent::RecvMessage(b_id, message), io);
                }
            }
        }
        panic!("the two sides kept answering each other");
    }

    /// A join handshake ends with both sides neighbors after three messages.
    ///
    /// The contact's answer to the join is a request the joiner answers, and
    /// the joiner's answer is a reply the contact does not answer. The joiner's
    /// reply stays recorded until its timer clears it.
    #[test]
    fn join_handshake_settles() {
        let mut contact = new_state();
        let mut joiner = State::new(1, None, Config::default(), StdRng::seed_from_u64(2));
        let io = &mut Io::new();

        joiner.handle(InEvent::RequestJoin(0), io);
        let messages = ferry(&mut joiner, &mut contact, io);

        assert_eq!(messages, 3, "join, neighbor request, neighbor reply");
        assert!(contact.active_view.contains(&1));
        assert!(joiner.active_view.contains(&0));
        assert!(contact.pending_neighbor_requests.is_empty());
        assert!(joiner.pending_neighbor_requests.contains_key(&0));
    }

    /// A request to a peer that crosses with the peer's join settles.
    ///
    /// We sent the peer a request, as after a forwarded join from another of its
    /// bootstrap peers, and its own `Join` arrives before the answer. Clearing
    /// the pending request on the `Join` makes us send a second request; the
    /// peer takes it as the reply to the one it sent, and nothing loops.
    #[test]
    fn crossed_request_and_join_settle() {
        let mut us = new_state();
        let mut peer = State::new(1, None, Config::default(), StdRng::seed_from_u64(2));
        let io = &mut Io::new();

        us.send_neighbor(1, Priority::High, io);
        peer.handle(InEvent::RequestJoin(0), io);
        let messages = ferry(&mut us, &mut peer, io);

        assert_eq!(messages, 4, "request, join, second request, reply");
        assert!(us.active_view.contains(&1));
        assert!(peer.active_view.contains(&0));
        assert!(us.pending_neighbor_requests.is_empty());
    }

    /// Counts the shuffle replies sent to `peer`.
    fn shuffle_replies_sent(io: &Io, peer: u32) -> usize {
        io.iter()
            .filter(|event| {
                matches!(
                    event,
                    TopicOut::SendMessage(to, topic::Message::Swarm(Message::ShuffleReply(_))) if *to == peer
                )
            })
            .count()
    }

    /// After a shuffle reply to a non-neighbor, we disconnect it.
    ///
    /// The reply goes over a connection opened for it. The origin drops us once
    /// it read the reply, but cannot close the connection while our stream is
    /// open.
    #[test]
    fn shuffle_reply_disconnects_a_non_neighbor() {
        let mut state = new_state();
        let io = &mut Io::new();
        state.handle(InEvent::RecvMessage(1, Message::Join(None)), io);
        assert!(state.active_view.contains(&1));
        io.clear();

        state.handle(InEvent::RecvMessage(1, Message::test_shuffle(2, 0)), io);

        assert_eq!(shuffle_replies_sent(io, 2), 1);
        let reply_at = io
            .iter()
            .position(|event| matches!(event, TopicOut::SendMessage(2, _)))
            .expect("checked");
        let disconnect_at = io
            .iter()
            .position(|event| matches!(event, TopicOut::DisconnectPeer(2)))
            .expect("the origin was not disconnected");
        assert!(reply_at < disconnect_at, "disconnected before the reply");
        assert!(
            !io.iter()
                .any(|event| matches!(event, TopicOut::DisconnectPeer(1))),
            "the forwarding neighbor was disconnected"
        );
    }

    /// A shuffle reply to a neighbor leaves the neighbor alone.
    #[test]
    fn shuffle_reply_keeps_a_neighbor() {
        let mut state = new_state();
        let io = &mut Io::new();
        state.handle(InEvent::RecvMessage(1, Message::Join(None)), io);
        io.clear();

        state.handle(InEvent::RecvMessage(1, Message::test_shuffle(1, 0)), io);

        assert_eq!(shuffle_replies_sent(io, 1), 1);
        assert!(
            !io.iter()
                .any(|event| matches!(event, TopicOut::DisconnectPeer(1))),
            "the neighbor was disconnected"
        );
    }

    #[test]
    fn refused_request_forgets_peer() {
        let mut state = new_state();
        state.config.active_view_capacity = 1;
        let io = &mut Io::new();
        state.active_view.insert(1);
        let request = Neighbor {
            priority: Priority::Low,
            data: Some(PeerData::new(vec![2])),
        };

        state.handle(InEvent::RecvMessage(2, Message::Neighbor(request)), io);

        assert!(
            !state.active_view.contains(&2),
            "a low request filled a full view"
        );
        assert!(!has_metadata(&state, 2));
    }

    /// A peer evicted from a full passive view loses its metadata.
    #[test]
    fn passive_eviction_forgets_peer() {
        let mut state = new_state();
        state.config.passive_view_capacity = 1;
        let io = &mut Io::new();
        state.add_passive(1, None, io);
        seed_metadata(&mut state, 1);

        // The passive view is full, so peer 1 is the one evicted to make room.
        state.add_passive(2, None, io);

        assert!(!has_metadata(&state, 1));
    }

    /// A peer removed from the active view and not kept as passive loses its metadata.
    #[test]
    fn active_discard_forgets_peer() {
        let mut state = new_state();
        let io = &mut Io::new();
        state.active_view.insert(1);
        seed_metadata(&mut state, 1);

        // A peer that is not alive is dropped rather than kept as passive.
        let reason = RemovalReason::DisconnectReceived { is_alive: false };
        state.remove_active(&1, reason, io);

        assert!(!has_metadata(&state, 1));
    }

    /// The timer of an earlier `Neighbor` leaves a later one to the same peer alone (#174).
    ///
    /// The timer was keyed by the peer only, so it ended the new request
    /// early and dropped the peer from the passive view.
    #[test]
    fn stale_neighbor_timer_keeps_a_new_request() {
        let mut state = new_state();
        let io = &mut Io::new();
        state.passive_view.insert(1);
        state.send_neighbor(1, Priority::Low, io);
        let first = io
            .iter()
            .find_map(|event| match event {
                TopicOut::ScheduleTimer(_, topic::Timer::Swarm(timer)) => Some(timer.clone()),
                _ => None,
            })
            .expect("the request armed a timer");
        let disconnect = Disconnect {
            alive: true,
            _respond: false,
        };
        state.handle(InEvent::RecvMessage(1, Message::Disconnect(disconnect)), io);
        state.send_neighbor(1, Priority::Low, io);

        state.handle(InEvent::TimerExpired(first), io);

        assert!(
            state.pending_neighbor_requests.contains_key(&1),
            "the new request ended"
        );
        assert!(state.passive_view.contains(&1), "the peer was dropped");
    }

    /// A request whose dial failed is replaced by one to another passive peer.
    ///
    /// The request timer is long enough for a slow dial, so it must not be
    /// what moves on from a peer we cannot reach.
    #[test]
    fn failed_dial_of_a_request_refills_at_once() {
        let mut state = new_state();
        let io = &mut Io::new();
        state.passive_view.insert(1);
        state.passive_view.insert(2);
        state.send_neighbor(1, Priority::High, io);
        io.clear();

        state.handle(InEvent::PeerDisconnected(1), io);

        assert!(sent_neighbor(io, 2), "no request to another passive peer");
        assert!(!state.passive_view.contains(&1));
        assert!(!state.pending_neighbor_requests.contains_key(&1));
        assert!(state.pending_neighbor_requests.contains_key(&2));
    }

    /// A passive peer whose neighbor request timed out loses its metadata.
    #[test]
    fn neighbor_request_timeout_forgets_peer() {
        let mut state = new_state();
        let io = &mut Io::new();
        state.pending_neighbor_requests.insert(1, 0);
        state.passive_view.insert(1);
        seed_metadata(&mut state, 1);

        state.handle(
            InEvent::TimerExpired(Timer::PendingNeighborRequest(1, 0)),
            io,
        );

        assert!(!has_metadata(&state, 1));
        assert!(
            io.iter()
                .any(|event| matches!(event, TopicOut::DisconnectPeer(1))),
            "the peer that did not answer was kept"
        );
    }

    /// A pending reply to an active peer does not count twice against the active view.
    ///
    /// The active view has room, so losing a peer must refill it at once
    /// rather than when the reply's timer fires.
    #[test]
    fn refill_counts_active_peer_once() {
        let mut state = new_state();
        state.config.active_view_capacity = 2;
        let io = &mut Io::new();
        state.active_view.insert(1);
        state.passive_view.insert(3);
        // Peer 2 joins: it becomes active, and our reply to it is pending.
        state.handle(InEvent::RecvMessage(2, Message::Join(None)), io);
        io.clear();

        let reason = RemovalReason::DisconnectReceived { is_alive: false };
        state.remove_active(&1, reason, io);

        assert!(sent_neighbor(io, 3), "the active view was not refilled");
    }

    /// A timed-out neighbor request does not drop the data of an active peer.
    ///
    /// The request can time out after the peer joined the active view through
    /// a forwarded join, where its data is still in use.
    #[test]
    fn neighbor_request_timeout_keeps_active_peer() {
        let mut state = new_state();
        let io = &mut Io::new();
        state.pending_neighbor_requests.insert(1, 0);
        state.active_view.insert(1);
        state.peer_data.insert(1, PeerData::new(vec![1]));

        state.handle(
            InEvent::TimerExpired(Timer::PendingNeighborRequest(1, 0)),
            io,
        );

        assert!(
            state.peer_data.contains_key(&1),
            "data of an active peer was dropped"
        );
    }

    /// A shuffle reply that names us stores no data about us.
    #[test]
    fn shuffle_reply_naming_us_stores_nothing_about_us() {
        let mut state = new_state();
        let io = &mut Io::new();
        state.handle(InEvent::RecvMessage(1, Message::Join(None)), io);
        let us = PeerInfo {
            id: 0,
            data: Some(PeerData::new(vec![1])),
        };
        let reply = Message::ShuffleReply(ShuffleReply { nodes: vec![us] });

        state.handle(InEvent::RecvMessage(1, reply), io);

        assert!(!state.peer_data.contains_key(&0));
    }

    /// A neighbor that left alive and comes back loses its alive mark.
    ///
    /// The mark kept the peer in the passive view when its connection closed.
    /// Kept on a neighbor, it made a later crash look like a graceful leave.
    #[test]
    fn returning_neighbor_loses_the_alive_mark() {
        let mut state = new_state();
        let io = &mut Io::new();
        state.handle(InEvent::RecvMessage(1, Message::Join(None)), io);
        let leave = Message::Disconnect(Disconnect {
            alive: true,
            _respond: false,
        });
        state.handle(InEvent::RecvMessage(1, leave), io);
        assert!(state.alive_disconnect_peers.contains(&1));

        state.handle(InEvent::RecvMessage(1, Message::Join(None)), io);

        assert!(state.active_view.contains(&1));
        assert!(!state.alive_disconnect_peers.contains(&1));
    }

    /// A quit leaves nothing about any peer behind.
    #[test]
    fn quit_leaves_nothing_behind() {
        let mut state = new_state();
        let io = &mut Io::new();
        state.handle(InEvent::RecvMessage(1, Message::Join(None)), io);
        let nodes = Message::test_shuffle_reply_with(vec![2, 3]);
        state.handle(InEvent::RecvMessage(1, nodes), io);

        state.handle(InEvent::Quit, io);

        assert!(state.active_view.is_empty() && state.passive_view.is_empty());
        assert!(state.peer_data.is_empty() && state.alive_disconnect_peers.is_empty());
        assert!(state.pending_neighbor_requests.is_empty());
    }

    /// Returns whether the state asked to close the connection to `peer`.
    fn disconnected(io: &Io, peer: u32) -> bool {
        io.iter()
            .any(|event| matches!(event, TopicOut::DisconnectPeer(p) if *p == peer))
    }

    /// A `Disconnect` from a peer that is no neighbor closes its connection.
    ///
    /// Two peers that evict each other at once both send one. The second
    /// arrives from a peer that we dropped already. Without the disconnect,
    /// the message would leave the peer in use and its connection open.
    #[test]
    fn disconnect_from_a_non_neighbor_releases_it() {
        let mut state = new_state();
        let io = &mut Io::new();
        let disconnect = Message::Disconnect(Disconnect {
            alive: true,
            _respond: false,
        });
        state.handle(InEvent::RecvMessage(1, disconnect), io);
        assert!(disconnected(io, 1));
    }

    /// An unrelated passive reply must not close the connection on which a
    /// Neighbor acceptance is still due. This is the race discussed in #178.
    #[test]
    fn passive_reply_keeps_a_pending_neighbor_connection() {
        let mut state = new_state();
        state.passive_view.insert(1);
        state.send_neighbor(1, Priority::High, &mut Io::new());
        let io = &mut Io::new();
        state.handle(
            InEvent::RecvMessage(1, Message::ShuffleReply(ShuffleReply { nodes: vec![] })),
            io,
        );
        assert!(state.pending_neighbor_requests.contains_key(&1));
        assert!(
            !io.iter()
                .any(|event| matches!(event, TopicOut::DisconnectPeer(1))),
            "a passive reply closed the connection before the Neighbor reply"
        );
    }

    /// A request that ends without a neighbor releases the connection it kept.
    ///
    /// A passive message keeps the connection while the request is out. If the
    /// reply never comes, the timeout must close it, or it stays open for good.
    #[test]
    fn timed_out_request_releases_its_connection() {
        let mut state = new_state();
        state.passive_view.insert(1);
        state.send_neighbor(1, Priority::High, &mut Io::new());
        let reply = Message::ShuffleReply(ShuffleReply { nodes: vec![] });
        state.handle(InEvent::RecvMessage(1, reply), &mut Io::new());
        let io = &mut Io::new();
        state.handle(
            InEvent::TimerExpired(Timer::PendingNeighborRequest(1, 0)),
            io,
        );
        assert!(disconnected(io, 1), "the connection outlived its request");
    }

    /// An accepted request keeps its connection, also after a passive message came first.
    #[test]
    fn accepted_request_keeps_its_connection() {
        let mut state = new_state();
        state.passive_view.insert(1);
        state.send_neighbor(1, Priority::High, &mut Io::new());
        let io = &mut Io::new();
        let reply = Message::ShuffleReply(ShuffleReply { nodes: vec![] });
        state.handle(InEvent::RecvMessage(1, reply), io);
        let accept = Message::Neighbor(Neighbor {
            priority: Priority::High,
            data: None,
        });
        state.handle(InEvent::RecvMessage(1, accept), io);
        assert!(state.active_view.contains(&1));
        assert!(
            !disconnected(io, 1),
            "closed the connection of a new neighbor"
        );
    }

    /// Returns the peers that got a join.
    fn joined(io: &Io) -> Vec<u32> {
        io.iter()
            .filter_map(|event| match event {
                TopicOut::SendMessage(peer, topic::Message::Swarm(Message::Join(_))) => Some(*peer),
                _ => None,
            })
            .collect()
    }

    /// An isolated node without bootstrap peers rejoins through a recent neighbor.
    #[test]
    fn rejoin_tries_a_recent_neighbor() {
        let mut state = new_state();
        state.active_view.insert(1);
        state.handle(InEvent::TimerExpired(Timer::DoMaintenance), &mut Io::new());
        state.active_view.remove(&1);
        let io = &mut Io::new();
        state.handle(InEvent::TimerExpired(Timer::DoMaintenance), io);
        assert_eq!(joined(io), vec![1]);
    }

    /// The recent neighbor of a rejoin is never a bootstrap peer, which gets a join anyway.
    #[test]
    fn rejoin_picks_a_recent_neighbor_that_is_no_bootstrap_peer() {
        // A pick before the filter misses the other neighbor for about half the seeds.
        for seed in 0..16 {
            let mut state = State::new(0, None, Config::default(), StdRng::seed_from_u64(seed));
            state.bootstrap_peers.push_back(1);
            state.active_view.insert(1);
            state.active_view.insert(2);
            state.handle(InEvent::TimerExpired(Timer::DoMaintenance), &mut Io::new());
            state.active_view.remove(&1);
            state.active_view.remove(&2);
            let io = &mut Io::new();
            state.handle(InEvent::TimerExpired(Timer::DoMaintenance), io);
            assert_eq!(joined(io), vec![1, 2], "seed {seed}");
        }
    }

    /// A neighbor is remembered when it joins, without waiting for a maintenance pass.
    #[test]
    fn recent_neighbors_record_a_new_neighbor_at_once() {
        let mut state = new_state();
        state.handle(InEvent::RecvMessage(1, Message::Join(None)), &mut Io::new());
        assert_eq!(state.recent_neighbors, [1]);
    }

    /// A neighbor that failed stays a recent neighbor, and a rejoin tries it.
    #[test]
    fn recent_neighbors_keep_a_failed_neighbor() {
        let mut state = new_state();
        state.handle(InEvent::RecvMessage(1, Message::Join(None)), &mut Io::new());
        state.handle(InEvent::PeerDisconnected(1), &mut Io::new());
        assert!(state.active_view.is_empty() && state.passive_view.is_empty());
        assert_eq!(state.recent_neighbors, [1]);

        let io = &mut Io::new();
        state.handle(InEvent::TimerExpired(Timer::DoMaintenance), io);
        assert_eq!(joined(io), vec![1]);
    }

    /// A neighbor that left the topic is no recent neighbor anymore.
    #[test]
    fn recent_neighbors_forget_a_neighbor_that_left() {
        let mut state = new_state();
        state.handle(InEvent::RecvMessage(1, Message::Join(None)), &mut Io::new());
        let leave = Message::Disconnect(Disconnect {
            alive: false,
            _respond: false,
        });
        state.handle(InEvent::RecvMessage(1, leave), &mut Io::new());
        assert!(state.recent_neighbors.is_empty());
    }

    /// The recent neighbors keep the latest ones, and drop the oldest beyond the limit.
    #[test]
    fn recent_neighbors_drop_the_oldest() {
        let mut state = new_state();
        let last = MAX_RECENT_NEIGHBORS as u32 + 2;
        for peer in 1..=last {
            state.handle(
                InEvent::RecvMessage(peer, Message::Join(None)),
                &mut Io::new(),
            );
        }
        let expected: Vec<u32> = (3..=last).collect();
        assert_eq!(Vec::from(state.recent_neighbors.clone()), expected);
    }

    /// A node that loses its last neighbor, with an empty passive view,
    /// rejoins through the peers the application joined.
    #[test]
    fn maintenance_rejoins_when_isolated() {
        let mut state = new_state();
        state.handle(InEvent::RequestJoin(1), &mut Io::new());
        let accept = Message::Neighbor(Neighbor {
            priority: Priority::High,
            data: None,
        });
        state.handle(InEvent::RecvMessage(1, accept), &mut Io::new());
        assert!(state.active_view.contains(&1));

        state.handle(InEvent::PeerDisconnected(1), &mut Io::new());
        assert!(state.active_view.is_empty() && state.passive_view.is_empty());

        let io = &mut Io::new();
        state.handle(InEvent::TimerExpired(Timer::DoMaintenance), io);
        assert_eq!(joined(io), vec![1], "no rejoin through the bootstrap peer");
    }

    /// An isolated node rejoins less often the longer it stays alone.
    #[test]
    fn rejoins_back_off() {
        let mut state = new_state();
        state.handle(InEvent::RequestJoin(1), &mut Io::new());
        let mut rejoins = Vec::new();
        for tick in 0..60 {
            let io = &mut Io::new();
            state.handle(InEvent::TimerExpired(Timer::DoMaintenance), io);
            assert!(io.iter().any(|event| matches!(
                event,
                TopicOut::ScheduleTimer(delay, topic::Timer::Swarm(Timer::DoMaintenance))
                    if *delay == MAINTENANCE_INTERVAL
            )));
            if !joined(io).is_empty() {
                rejoins.push(tick);
            }
        }
        assert_eq!(rejoins, vec![0, 1, 3, 7, 11, 23, 35, 47, 59]);
    }

    /// Recovery must reset a long isolation backoff before a later failure.
    #[test]
    fn rejoin_backoff_resets_after_recovery() {
        let mut state = new_state();
        state.handle(InEvent::RequestJoin(1), &mut Io::new());
        for _ in 0..60 {
            state.handle(InEvent::TimerExpired(Timer::DoMaintenance), &mut Io::new());
        }
        assert_eq!(state.passes_alone, 60);
        state.handle(
            InEvent::RecvMessage(
                1,
                Message::Neighbor(Neighbor {
                    priority: Priority::High,
                    data: None,
                }),
            ),
            &mut Io::new(),
        );
        state.handle(InEvent::TimerExpired(Timer::DoMaintenance), &mut Io::new());
        assert_eq!(state.passes_alone, 0);
        state.handle(InEvent::PeerDisconnected(1), &mut Io::new());
        let io = &mut Io::new();
        state.handle(InEvent::TimerExpired(Timer::DoMaintenance), io);
        assert_eq!(joined(io), vec![1]);
    }

    /// A node below half its active view capacity refills from its passive view.
    #[test]
    fn maintenance_refills_a_short_active_view() {
        let mut state = new_state();
        state.active_view.insert(1);
        for peer in 10..20 {
            state.passive_view.insert(peer);
        }

        let io = &mut Io::new();
        state.handle(InEvent::TimerExpired(Timer::DoMaintenance), io);
        let requested: Vec<u32> = (10..20).filter(|peer| sent_neighbor(io, *peer)).collect();
        assert_eq!(requested.len(), 1, "expected one neighbor request: {io:?}");
    }

    /// A node with half its active view or more sends nothing.
    ///
    /// One slot short is the normal state. Requests there would only be refused.
    #[test]
    fn maintenance_is_quiet_on_a_healthy_view() {
        let mut state = new_state();
        for peer in 1..=3 {
            state.active_view.insert(peer);
        }
        for peer in 10..20 {
            state.passive_view.insert(peer);
        }

        let io = &mut Io::new();
        state.handle(InEvent::TimerExpired(Timer::DoMaintenance), io);
        assert!(
            !io.iter()
                .any(|event| matches!(event, TopicOut::SendMessage(..))),
            "a healthy node sent messages: {io:?}"
        );
    }

    /// The remembered bootstrap contacts stay within their bound, newest first.
    ///
    /// An application can pass every peer it discovers to a join. Before, the
    /// topic kept all of them for good, and an isolated node sent a join to each.
    #[test]
    fn bootstrap_contacts_stay_bounded() {
        let mut state = new_state();
        let max = MAX_BOOTSTRAP_PEERS as u32;
        for peer in 1..=max + 5 {
            state.handle(InEvent::RequestJoin(peer), &mut Io::new());
        }
        assert_eq!(state.bootstrap_peers.len(), MAX_BOOTSTRAP_PEERS);
        assert!(
            !state.bootstrap_peers.contains(&5),
            "the oldest contacts go first"
        );
        // A repeated contact moves to the newest place instead of growing the set.
        state.handle(InEvent::RequestJoin(7), &mut Io::new());
        assert_eq!(state.bootstrap_peers.len(), MAX_BOOTSTRAP_PEERS);
        state.handle(InEvent::RequestJoin(max + 6), &mut Io::new());
        assert!(
            !state.bootstrap_peers.contains(&6),
            "the oldest contact goes"
        );
        assert!(
            state.bootstrap_peers.contains(&7),
            "a repeated contact stays"
        );

        // An isolated node rejoins through every remembered contact.
        let io = &mut Io::new();
        state.handle(InEvent::TimerExpired(Timer::DoMaintenance), io);
        let mut rejoined = joined(io);
        rejoined.sort();
        let mut expected: Vec<u32> = state.bootstrap_peers.iter().copied().collect();
        expected.sort();
        assert_eq!(rejoined, expected);
    }

    /// A node asks each passive peer once between two maintenance passes.
    ///
    /// A peer with a full active view refuses with a `ShuffleReply` and a
    /// `Disconnect`. The `ShuffleReply` starts a refill. If the refill could
    /// pick peers that refused, a node short of neighbors whose passive peers
    /// are all full would ask them in turn without end.
    #[test]
    fn refused_peers_are_asked_again_only_after_maintenance() {
        let mut state = new_state();
        state.active_view.insert(1);
        for peer in 10..13 {
            state.passive_view.insert(peer);
        }
        let mut asked = Vec::new();
        state.handle(InEvent::TimerExpired(Timer::DoMaintenance), &mut Io::new());
        for _ in 0..10 {
            let Some(peer) = state.pending_neighbor_requests.keys().next().copied() else {
                break;
            };
            asked.push(peer);
            let reply = Message::ShuffleReply(ShuffleReply {
                nodes: (10..13).map(|id| (id, None).into()).collect(),
            });
            let refusal = Message::Disconnect(Disconnect {
                alive: true,
                _respond: false,
            });
            state.handle(InEvent::RecvMessage(peer, reply), &mut Io::new());
            state.handle(InEvent::RecvMessage(peer, refusal), &mut Io::new());
            state.handle(InEvent::PeerDisconnected(peer), &mut Io::new());
        }
        asked.sort();
        assert_eq!(asked, vec![10, 11, 12], "each passive peer once");
        assert!(state.pending_neighbor_requests.is_empty());
        assert!((10..13).all(|peer| state.passive_view.contains(&peer)));

        let io = &mut Io::new();
        state.handle(InEvent::TimerExpired(Timer::DoMaintenance), io);
        let requested: Vec<u32> = (10..13).filter(|peer| sent_neighbor(io, *peer)).collect();
        assert_eq!(requested.len(), 1, "the next pass asks again");
    }

    /// A node with half its active view or more stops at a refusal.
    ///
    /// One slot short is the normal state, and the next passive peer is
    /// usually full too.
    #[test]
    fn healthy_node_stops_at_a_refusal() {
        let mut state = new_state();
        for peer in 1..=3 {
            state.active_view.insert(peer);
        }
        for peer in 10..13 {
            state.passive_view.insert(peer);
        }
        state.pending_neighbor_requests.insert(10, 0);
        let io = &mut Io::new();
        let reply = Message::ShuffleReply(ShuffleReply {
            nodes: (10..13).map(|id| (id, None).into()).collect(),
        });
        let refusal = Message::Disconnect(Disconnect {
            alive: true,
            _respond: false,
        });
        state.handle(InEvent::RecvMessage(10, reply), io);
        state.handle(InEvent::RecvMessage(10, refusal), io);
        let requested: Vec<u32> = (11..13).filter(|peer| sent_neighbor(io, *peer)).collect();
        assert!(requested.is_empty(), "asked {requested:?} after a refusal");
    }

    /// The first shuffle comes soon after joining, and the interval backs
    /// off to the configured one.
    #[test]
    fn shuffle_interval_backs_off_to_the_configured_one() {
        let mut state = new_state();
        let mut delays = Vec::new();
        let mut handle = |event: InEvent<u32>| {
            let mut io = Io::new();
            state.handle(event, &mut io);
            delays.extend(io.into_iter().filter_map(|event| match event {
                TopicOut::ScheduleTimer(delay, topic::Timer::Swarm(Timer::DoShuffle)) => {
                    Some(delay)
                }
                _ => None,
            }));
        };
        handle(InEvent::RequestJoin(1));
        for _ in 0..7 {
            handle(InEvent::TimerExpired(Timer::DoShuffle));
        }

        let nominal = [1, 2, 4, 8, 16, 32, 60, 60].map(Duration::from_secs);
        assert_eq!(delays.len(), nominal.len());
        for (delay, nominal) in delays.iter().zip(nominal) {
            assert!(
                *delay >= nominal.mul_f64(0.75) && *delay <= nominal.mul_f64(1.25),
                "delay {delay:?} is not within jitter of {nominal:?}, all delays {delays:?}"
            );
        }
        assert!(
            delays[6] != delays[7],
            "the delays should vary, got {delays:?}"
        );
    }
}
