//! Implementation of the HyParView membership protocol
//!
//! The implementation is based on [this paper][paper] by Joao Leitao, Jose Pereira, Luıs Rodrigues
//! and the [example implementation][impl] by Bartosz Sypytkowski
//!
//! [paper]: https://asc.di.fct.unl.pt/~jleitao/pdf/dsn07-leitao.pdf
//! [impl]: https://gist.github.com/Horusiath/84fac596101b197da0546d1697580d99

use std::collections::{hash_map, HashMap, HashSet};

use derive_more::{From, Sub};
use n0_future::time::Duration;
use rand::{rngs::ThreadRng, Rng};
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::{util::IndexSet, PeerData, PeerIdentity, PeerInfo, IO};

/// How often we retry a join that got no reply.
///
/// We retry when the join's connection closes, and when no reply came within
/// [`Config::neighbor_request_timeout`].
const JOIN_RETRIES: u8 = 2;

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
    /// The timeout of the `Neighbor` to the peer with the given id.
    ///
    /// The id tells it from the timer of an earlier `Neighbor` to the same peer.
    PendingNeighborRequest(PI, u64),
    /// The timeout of a join to the peer, with the number of retries before it.
    ///
    /// The number tells it from the timer of an earlier attempt.
    PendingJoin(PI, u8),
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
    pub fn expired(&self) -> bool {
        *self == Ttl(0)
    }
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
    /// Interval duration for shuffle requests
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
            // A dial and a round trip on a slow network, with room to spare.
            neighbor_request_timeout: Duration::from_secs(10),
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
    /// Whether a shuffle timer is currently scheduled
    shuffle_scheduled: bool,
    /// Random number generator
    rng: RG,
    /// Statistics
    pub(crate) stats: Stats,
    /// The neighbor requests we sent out but did not yet receive a reply for, with their ids
    pending_neighbor_requests: HashMap<PI, u64>,
    /// The id of the next neighbor request
    next_neighbor_request: u64,
    /// Joins we sent and got no reply for yet, with how often we retried each.
    pending_joins: HashMap<PI, u8>,
    /// The opaque user peer data we received for other peers
    peer_data: HashMap<PI, PeerData>,
    /// List of peers that are disconnecting, but which we want to keep in the passive set once the connection closes
    alive_disconnect_peers: HashSet<PI>,
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
            config,
            shuffle_scheduled: false,
            rng,
            stats: Stats::default(),
            pending_neighbor_requests: Default::default(),
            next_neighbor_request: 0,
            pending_joins: Default::default(),
            peer_data: Default::default(),
            alive_disconnect_peers: Default::default(),
        }
    }

    pub fn handle(&mut self, event: InEvent<PI>, io: &mut impl IO<PI>) {
        match event {
            InEvent::RecvMessage(from, message) => self.handle_message(from, message, io),
            InEvent::TimerExpired(timer) => match timer {
                Timer::DoShuffle => self.handle_shuffle_timer(io),
                Timer::PendingNeighborRequest(peer, id) => {
                    self.handle_pending_neighbor_timer(peer, id, io)
                }
                Timer::PendingJoin(peer, retries) => {
                    // A join that got no reply in time may be lost: the peer
                    // was not in the topic yet, or the connection died.
                    if self.pending_joins.get(&peer) == Some(&retries) {
                        debug!(other = ?peer, "join not answered in time, retry");
                        self.retry_join(peer, io);
                    }
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
        if !self.shuffle_scheduled {
            io.push(OutEvent::ScheduleTimer(
                self.config.shuffle_interval,
                Timer::DoShuffle,
            ));
            self.shuffle_scheduled = true;
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
        for peer in self.pending_joins.keys() {
            assert!(
                !self.active_view.contains(peer),
                "join to neighbor {peer:?} is pending"
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
            Message::ShuffleReply(details) => self.on_shuffle_reply(details, io),
            Message::Neighbor(details) => self.on_neighbor(from, details, io),
            Message::Disconnect(details) => self.on_disconnect(from, details, io),
        }

        // Disconnect from passive nodes right after receiving a message.
        // TODO(frando): I'm not sure anymore that this is correct. Maybe remove?
        if !is_disconnect && !self.active_view.contains(&from) {
            io.push(OutEvent::DisconnectPeer(from));
        }
    }

    fn handle_join(&mut self, peer: PI, io: &mut impl IO<PI>) {
        // A join to a neighbor needs no answer we could miss. Tracking it would
        // make us join the peer again when it leaves.
        if !self.active_view.contains(&peer) {
            self.pending_joins.entry(peer).or_insert(0);
        }
        self.send_join(peer, io);
    }

    /// Sends a join to `peer`, and arms its timeout if we track it.
    fn send_join(&mut self, peer: PI, io: &mut impl IO<PI>) {
        io.push(OutEvent::SendMessage(
            peer,
            Message::Join(self.me_data.clone()),
        ));
        if let Some(retries) = self.pending_joins.get(&peer) {
            io.push(OutEvent::ScheduleTimer(
                self.config.neighbor_request_timeout,
                Timer::PendingJoin(peer, *retries),
            ));
        }
    }

    /// We received a disconnect message.
    fn on_disconnect(&mut self, peer: PI, details: Disconnect, io: &mut impl IO<PI>) {
        self.pending_neighbor_requests.remove(&peer);
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
        // A join sent on the closed connection may be lost, so we retry it.
        if self.pending_joins.contains_key(&peer) {
            debug!(other = ?peer, "connection closed with join pending, retry");
            self.retry_join(peer, io);
        }
    }

    /// Sends a pending join to `peer` again, unless it ran out of retries.
    fn retry_join(&mut self, peer: PI, io: &mut impl IO<PI>) {
        let Some(retries) = self.pending_joins.get_mut(&peer) else {
            return;
        };
        if *retries < JOIN_RETRIES {
            *retries += 1;
            self.send_join(peer, io);
        } else {
            debug!(other = ?peer, "join not answered after all retries, give up");
            self.pending_joins.remove(&peer);
            let wanted = self.active_view.contains(&peer)
                || self.pending_neighbor_requests.contains_key(&peer);
            if !wanted {
                // The join was the topic's only use of the peer.
                io.push(OutEvent::DisconnectPeer(peer));
            }
        }
    }

    fn handle_quit(&mut self, io: &mut impl IO<PI>) {
        self.pending_joins.clear();
        for peer in self.active_view.clone().into_iter() {
            self.active_view.remove(&peer);
            self.send_disconnect(peer, false, io);
        }
        // The state is dropped after the quit. Leave nothing behind.
        self.pending_neighbor_requests.clear();
        self.passive_view = IndexSet::new();
        self.peer_data.clear();
        self.alive_disconnect_peers.clear();
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
            // The reply usually goes to a peer that is not our neighbor, over a
            // connection opened for it. The origin drops us once it read the
            // reply. But it cannot close the connection while our stream is
            // open, so we drop the origin too, unless we want something from it.
            let origin = shuffle.origin;
            if !self.active_view.contains(&origin)
                && !self.pending_neighbor_requests.contains_key(&origin)
                && !self.pending_joins.contains_key(&origin)
            {
                io.push(OutEvent::DisconnectPeer(origin));
            }
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

    fn on_shuffle_reply(&mut self, message: ShuffleReply<PI>, io: &mut impl IO<PI>) {
        for node in message.nodes {
            self.add_passive(node.id, node.data, io);
        }
        self.refill_active_from_passive(&[], io);
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
        io.push(OutEvent::ScheduleTimer(
            self.config.shuffle_interval,
            Timer::DoShuffle,
        ));
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
        // A shuffle may carry our own id.
        if peer == self.me {
            return;
        }
        self.insert_peer_info((peer, data).into(), io);
        if self.active_view.contains(&peer) || self.passive_view.contains(&peer) {
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
            if !self.active_view.contains(&peer) {
                // The request was the topic's only use of the peer.
                io.push(OutEvent::DisconnectPeer(peer));
            }
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
        self.pending_joins.remove(&peer);
        // The mark is from an earlier leave.
        self.alive_disconnect_peers.remove(&peer);
        if self.active_view.insert(peer) {
            debug!(other = ?peer, "add to active view");
            io.push(OutEvent::EmitEvent(Event::NeighborUp(peer)));
            if reply {
                self.send_neighbor(peer, priority, io);
            }
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

    /// Counts the joins sent to `peer`.
    fn joins_sent(io: &Io, peer: u32) -> usize {
        io.iter()
            .filter(|event| {
                matches!(
                    event,
                    TopicOut::SendMessage(to, topic::Message::Swarm(Message::Join(_))) if *to == peer
                )
            })
            .count()
    }

    /// A join lost to a closed connection is retried a bounded number of times.
    #[test]
    fn join_is_retried_when_the_connection_closes() {
        let mut state = new_state();
        let io = &mut Io::new();
        state.handle(InEvent::RequestJoin(1), io);
        assert_eq!(joins_sent(io, 1), 1);

        for retry in 1..=JOIN_RETRIES as usize {
            state.handle(InEvent::PeerDisconnected(1), io);
            assert_eq!(joins_sent(io, 1), 1 + retry);
        }
        state.handle(InEvent::PeerDisconnected(1), io);
        assert_eq!(joins_sent(io, 1), 1 + JOIN_RETRIES as usize);
        assert!(state.pending_joins.is_empty());
    }

    /// A join that got no reply in time is retried, and a stale timer does nothing.
    ///
    /// The peer may not be in the topic yet (#175), or the join went on a
    /// connection that died after a newer one replaced it. The connection then
    /// stays open, so no close made us retry.
    #[test]
    fn unanswered_join_is_retried_after_its_timeout() {
        let mut state = new_state();
        let io = &mut Io::new();
        state.handle(InEvent::RequestJoin(1), io);
        let first = Timer::PendingJoin(1, 0);
        assert_eq!(joins_sent(io, 1), 1);

        state.handle(InEvent::TimerExpired(first.clone()), io);
        assert_eq!(joins_sent(io, 1), 2, "no retry after the timeout");

        // The retry armed a timer of its own, so the first one is stale.
        state.handle(InEvent::TimerExpired(first), io);
        assert_eq!(joins_sent(io, 1), 2, "a stale timer retried");
        state.handle(InEvent::TimerExpired(Timer::PendingJoin(1, 1)), io);
        assert_eq!(joins_sent(io, 1), 3);
        state.handle(InEvent::TimerExpired(Timer::PendingJoin(1, 2)), io);
        assert_eq!(joins_sent(io, 1), 1 + JOIN_RETRIES as usize);
        assert!(state.pending_joins.is_empty(), "retried past the limit");
        assert!(
            io.iter()
                .any(|event| matches!(event, TopicOut::DisconnectPeer(1))),
            "the peer that never answered was kept"
        );
    }

    /// A join is not retried once the peer is a neighbor.
    ///
    /// Losing the connection then is an ordinary disconnect, not a lost join.
    #[test]
    fn join_is_not_retried_once_the_peer_is_a_neighbor() {
        let mut state = new_state();
        let io = &mut Io::new();
        state.handle(InEvent::RequestJoin(1), io);
        let neighbor = Neighbor {
            priority: Priority::High,
            data: None,
        };
        state.handle(InEvent::RecvMessage(1, Message::Neighbor(neighbor)), io);
        assert!(state.active_view.contains(&1));
        io.clear();

        state.handle(InEvent::PeerDisconnected(1), io);

        assert_eq!(joins_sent(io, 1), 0);
    }

    /// A join to a peer that is already a neighbor is not tracked.
    ///
    /// Otherwise we would join the peer again when it leaves. A peer that left
    /// the topic would then hold the new connection open for nothing.
    #[test]
    fn join_to_a_neighbor_is_not_retried() {
        let mut state = new_state();
        let io = &mut Io::new();
        state.handle(InEvent::RecvMessage(1, Message::Join(None)), io);
        assert!(state.active_view.contains(&1));
        state.handle(InEvent::RequestJoin(1), io);
        io.clear();

        state.handle(InEvent::PeerDisconnected(1), io);

        assert_eq!(joins_sent(io, 1), 0);
        assert!(state.pending_joins.is_empty());
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
}
