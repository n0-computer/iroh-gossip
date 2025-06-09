//! Implementation of the HyParView membership protocol
//!
//! The implementation is based on [this paper][paper] by Joao Leitao, Jose Pereira, Luıs Rodrigues
//! and the [example implementation][impl] by Bartosz Sypytkowski
//!
//! [paper]: https://asc.di.fct.unl.pt/~jleitao/pdf/dsn07-leitao.pdf
//! [impl]: https://gist.github.com/Horusiath/84fac596101b197da0546d1697580d99

use std::collections::{HashMap, HashSet};

use derive_more::{From, Sub};
use n0_future::time::Duration;
use rand::{rngs::ThreadRng, Rng};
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::{util::IndexSet, PeerData, PeerIdentity, PeerInfo, IO};

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
    /// Joining a peer failed.
    JoinFailed(PI),
}

/// Kinds of timers HyParView needs to schedule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Timer<PI> {
    DoShuffle,
    PendingNeighborRequest(PI),
}

/// Messages that we can send and receive from peers within the topic.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Message<PI> {
    /// Request to add sender to an active view of recipient.
    NeighborRequest(NeighborRequest),
    /// Reply to a [`NeighborRequest`] if the neighbor request is accepted.
    /// If it's declined, a [`Message::Disconnect`] is sent instead.
    NeighborReply(NeighborReply),
    /// When receiving Join, ForwardJoin is forwarded to the peer's ActiveView to introduce the
    /// new member.
    ForwardJoin(ForwardJoin<PI>),
    /// A shuffle request is sent occasionally to re-shuffle the PassiveView with contacts from
    /// other peers.
    Shuffle(Shuffle<PI>),
    /// Peers reply to [`Message::Shuffle`] requests with a random peers from their active and
    /// passive views.
    ShuffleReply(ShuffleReply<PI>),
    /// Request to disconnect from a peer.
    /// If [`Disconnect::alive`] is true, the other peer is not shutting down, so it should be
    /// added to the passive set.
    Disconnect(Disconnect<PI>),
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
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
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
pub struct NeighborRequest {
    reason: NeighborReason,
    /// The user data of tyou want to join the swarm
    data: Option<PeerData>,
}

/// Reason why we want to become someone's neighbor.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum NeighborReason {
    /// We are joining the swarm.
    Join,
    /// We received a forward join from the peer and react on it.
    ReplyToForwardJoin { priority: Priority },
    /// We are activating a passive peer.
    Activate { priority: Priority },
}

impl NeighborReason {
    fn priority(&self) -> Priority {
        match self {
            Self::Join => Priority::High,
            Self::ReplyToForwardJoin { priority } => *priority,
            Self::Activate { priority } => *priority,
        }
    }
}

/// A neighbor message is sent after adding a peer to our active view to inform them that we are
/// now neighbors.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct NeighborReply {}

/// Message sent when leaving the swarm or closing down to inform peers about us being gone.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Disconnect<PI> {
    /// Whether we are actually shutting down or closing the connection only because our limits are
    /// reached.
    alive: bool,
    /// A list of nodes from the sender's active and passive views
    nodes: Vec<PeerInfo<PI>>,
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
    /// Timeout after which a `Neighbor` request is considered failed
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
            // Wild guess
            neighbor_request_timeout: Duration::from_millis(500),
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
    /// The set of neighbor requests we sent out but did not yet receive a reply for
    pending_neighbor_requests: HashMap<PI, NeighborReason>,
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
            peer_data: Default::default(),
            alive_disconnect_peers: Default::default(),
        }
    }

    pub fn handle(&mut self, event: InEvent<PI>, io: &mut impl IO<PI>) {
        match event {
            InEvent::RecvMessage(from, message) => self.handle_message(from, message, io),
            InEvent::TimerExpired(timer) => match timer {
                Timer::DoShuffle => self.handle_shuffle_timer(io),
                Timer::PendingNeighborRequest(peer) => self.neighbor_request_failed(peer, io),
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
    }

    fn handle_message(&mut self, from: PI, message: Message<PI>, io: &mut impl IO<PI>) {
        let is_disconnect = matches!(message, Message::Disconnect(Disconnect { .. }));
        if !is_disconnect && !self.active_view.contains(&from) {
            self.stats.total_connections += 1;
        }
        match message {
            Message::ForwardJoin(details) => self.on_forward_join(from, details, io),
            Message::Shuffle(details) => self.on_shuffle(from, details, io),
            Message::ShuffleReply(details) => self.on_shuffle_reply(details, io),
            Message::NeighborRequest(details) => self.on_neighbor_request(from, details, io),
            Message::NeighborReply(_) => self.on_neighbor_reply(from, io),
            Message::Disconnect(details) => self.on_disconnect(from, details, io),
        }

        // Disconnect from passive nodes right after receiving a message.
        // TODO(frando): I'm not sure anymore that this is correct. Maybe remove?
        if !is_disconnect && !self.active_view.contains(&from) {
            io.push(OutEvent::DisconnectPeer(from));
        }
    }

    fn handle_join(&mut self, peer: PI, io: &mut impl IO<PI>) {
        self.send_neighbor_request(peer, NeighborReason::Join, io);
    }

    fn neighbor_request_failed(&mut self, peer: PI, io: &mut impl IO<PI>) {
        if let Some(reason) = self.pending_neighbor_requests.remove(&peer) {
            self.refill_active_from_passive(Some(&peer), io);
            if let NeighborReason::Join = reason {
                io.push(OutEvent::EmitEvent(Event::JoinFailed(peer)));
            }
        }
    }

    /// We received a disconnect message.
    fn on_disconnect(&mut self, peer: PI, details: Disconnect<PI>, io: &mut impl IO<PI>) {
        self.neighbor_request_failed(peer, io);

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
        self.on_shuffle_nodes(details.nodes, Some(&peer), io);
    }

    /// A connection was closed by the peer.
    fn handle_connection_closed(&mut self, peer: PI, io: &mut impl IO<PI>) {
        self.neighbor_request_failed(peer, io);
        if self.active_view.contains(&peer) {
            self.remove_active(&peer, RemovalReason::ConnectionClosed, io);
        } else if !self.alive_disconnect_peers.remove(&peer) {
            self.passive_view.remove(&peer);
            self.peer_data.remove(&peer);
        }
    }

    fn handle_quit(&mut self, io: &mut impl IO<PI>) {
        for peer in self.active_view.clone().into_iter() {
            self.active_view.remove(&peer);
            self.send_disconnect(peer, false, io);
        }
    }

    fn send_disconnect(&mut self, peer: PI, alive: bool, io: &mut impl IO<PI>) {
        // Before disconnecting, send a `ShuffleReply` with some of our nodes to
        // prevent the other node from running out of connections. This is especially
        // relevant if the other node just joined the swarm.
        let nodes = self.get_nodes_for_shuffle(None);
        let message = Message::Disconnect(Disconnect { alive, nodes });
        io.push(OutEvent::SendMessage(peer, message));
        io.push(OutEvent::DisconnectPeer(peer));
    }

    fn on_forward_join(&mut self, sender: PI, message: ForwardJoin<PI>, io: &mut impl IO<PI>) {
        let peer_id = message.peer.id;
        // If the peer is already in our active view, we renew our neighbor relationship.
        // "i) If the time to live is equal to zero or if the number of nodes in p’s active view is equal to one,
        // it will add the new node to its active view (7)"
        if self.active_view.contains(&peer_id)
            || message.ttl.expired()
            || self.active_view.len() <= 1
        {
            // Modification from paper: Instead of adding the peer directly to our active view,
            // we only send the Neighbor message. We will add the peer to our active view once we receive a
            // reply from our neighbor.
            // This prevents us adding unreachable peers to our active view.
            self.insert_peer_info(message.peer, io);
            self.send_neighbor_request(
                peer_id,
                NeighborReason::ReplyToForwardJoin {
                    priority: Priority::High,
                },
                io,
            );
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

    fn on_neighbor_request(&mut self, from: PI, details: NeighborRequest, io: &mut impl IO<PI>) {
        let NeighborRequest { reason, data } = details;
        // "A node q that receives a high priority neighbor request will always accept the request, even
        // if it has to drop a random member from its active view (again, the member that is dropped will
        // receive a Disconnect notification). If a node q receives a low priority Neighbor request, it will
        // only accept the request if it has a free slot in its active view, otherwise it will refuse the request."
        self.insert_peer_info((from, data.clone()).into(), io);
        if !self.add_active(from, reason.priority(), io) {
            self.send_disconnect(from, true, io);
        } else {
            self.send_neighbor_reply(from, io);
        }

        if let NeighborReason::Join = reason {
            // "The contact node c will then send to all other nodes in its active view a ForwardJoin
            // request containing the new node identifier. Associated to the join procedure,
            // there are two configuration parameters, named Active Random Walk Length (ARWL),
            // that specifies the maximum number of hops a ForwardJoin request is propagated,
            // and Passive Random Walk Length (PRWL), that specifies at which point in the walk the node
            // is inserted in a passive view. To use these parameters, the ForwardJoin request carries
            // a “time to live” field that is initially set to ARWL and decreased at every hop. (7)"
            let ttl = self.config.active_random_walk_length;
            let peer_info = PeerInfo { id: from, data };
            for node in self.active_view.iter_without(&from) {
                let message = Message::ForwardJoin(ForwardJoin {
                    peer: peer_info.clone(),
                    ttl,
                });
                io.push(OutEvent::SendMessage(*node, message));
            }
        }
    }

    fn on_neighbor_reply(&mut self, from: PI, io: &mut impl IO<PI>) {
        if self.pending_neighbor_requests.remove(&from).is_some() {
            if !self.add_active(from, Priority::High, io) {
                self.send_disconnect(from, true, io);
            }
        } else {
            debug!(?from, "Received unsolicited neighbor reply")
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
            self.on_shuffle_nodes(shuffle.nodes, None, io);
            self.send_shuffle_reply(shuffle.origin, len, io);
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

    fn on_shuffle_nodes(
        &mut self,
        nodes: Vec<PeerInfo<PI>>,
        skip_peer: Option<&PI>,
        io: &mut impl IO<PI>,
    ) {
        for node in nodes {
            self.add_passive(node.id, node.data, io);
        }
        self.refill_active_from_passive(skip_peer, io);
    }

    fn send_shuffle_reply(&mut self, to: PI, len: usize, io: &mut impl IO<PI>) {
        let nodes = self.get_nodes_for_shuffle(Some(len));
        let message = Message::ShuffleReply(ShuffleReply { nodes });
        io.push(OutEvent::SendMessage(to, message));
    }

    fn get_nodes_for_shuffle(&mut self, len: Option<usize>) -> Vec<PeerInfo<PI>> {
        let len = len.unwrap_or_else(|| {
            self.config.shuffle_active_view_count + self.config.shuffle_passive_view_count
        });
        let mut nodes = self.passive_view.shuffled_and_capped(len, &mut self.rng);
        // If we don't have enough passive nodes for the expected length, we fill with
        // active nodes.
        if nodes.len() < len {
            nodes.extend(
                self.active_view
                    .shuffled_and_capped(len - nodes.len(), &mut self.rng),
            );
        }
        nodes.into_iter().map(|id| self.peer_info(&id)).collect()
    }

    fn on_shuffle_reply(&mut self, message: ShuffleReply<PI>, io: &mut impl IO<PI>) {
        self.on_shuffle_nodes(message.nodes, None, io);
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
        self.insert_peer_info((peer, data).into(), io);
        if self.active_view.contains(&peer) || self.passive_view.contains(&peer) || peer == self.me
        {
            return;
        }
        if self.passive_is_full() {
            self.passive_view.remove_random(&mut self.rng);
        }
        self.passive_view.insert(peer);
    }

    /// Remove a peer from the active view.
    ///
    /// If `reason` is [`RemovalReason::EvictRandom`], a [`Disconnect`] message will be sent to the peer.
    fn remove_active(&mut self, peer: &PI, reason: RemovalReason, io: &mut impl IO<PI>) {
        if let Some(idx) = self.active_view.get_index_of(peer) {
            let removed_peer = self.remove_active_by_index(idx, reason, io).unwrap();
            self.refill_active_from_passive(Some(&removed_peer), io);
        }
    }

    fn refill_active_from_passive(&mut self, skip_peer: Option<&PI>, io: &mut impl IO<PI>) {
        if self.active_view.len() + self.pending_neighbor_requests.len()
            >= self.config.active_view_capacity
        {
            return;
        }
        // "When a node p suspects that one of the nodes present in its active view has failed
        // (by either disconnecting or blocking), it selects a random node q from its passive view and
        // attempts to establish a TCP connection with q. If the connection fails to establish,
        // node q is considered failed and removed from p’s passive view; another node q′ is selected
        // at random and a new attempt is made. The procedure is repeated until a connection is established
        // with success." (p7)
        let mut skip_peers: Vec<_> = skip_peer.into_iter().collect();
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
            self.send_neighbor_request(node, NeighborReason::Activate { priority }, io);
            // schedule a timer that checks if the node replied with a neighbor message,
            // otherwise try again with another passive node.
            io.push(OutEvent::ScheduleTimer(
                self.config.neighbor_request_timeout,
                Timer::PendingNeighborRequest(node),
            ));
        };
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
                RemovalReason::EvictRandom => self.send_disconnect(peer, true, io),
                // close connection without sending anything further.
                RemovalReason::DisconnectReceived { is_alive: _ } => {
                    io.push(OutEvent::DisconnectPeer(peer))
                }
                // do nothing, connection already closed.
                RemovalReason::ConnectionClosed => {}
            }

            let keep_as_passive = match reason {
                // keep alive if previously marked as alive.
                RemovalReason::ConnectionClosed => self.alive_disconnect_peers.remove(&peer),
                // keep alive if other peer said to be still alive.
                RemovalReason::DisconnectReceived { is_alive } => is_alive,
                // keep alive (only we are removing for now)
                RemovalReason::EvictRandom => true,
            };

            if keep_as_passive {
                let data = self.peer_data.remove(&peer);
                self.add_passive(peer, data, io);
                // mark peer as alive, so it doesn't get removed from the passive view if the conn closes.
                if !matches!(reason, RemovalReason::ConnectionClosed) {
                    self.alive_disconnect_peers.insert(peer);
                }
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
            self.remove_active_by_index(index, RemovalReason::EvictRandom, io);
        }
    }

    /// Add a peer to the active view.
    ///
    /// If the active view is currently full, a random peer will be removed first.
    /// Sends a Neighbor message to the peer. If high_priority is true, the peer
    /// may not deny the Neighbor request.
    ///
    /// Returns `true` if the peer is now in the active view.
    fn add_active(&mut self, peer: PI, priority: Priority, io: &mut impl IO<PI>) -> bool {
        if peer == self.me {
            return false;
        }
        if self.active_view.contains(&peer) {
            return true;
        }
        match (priority, self.active_is_full()) {
            (Priority::High, is_full) => {
                if is_full {
                    self.free_random_slot_in_active_view(io);
                }
                self.add_active_unchecked(peer, io);
                true
            }
            (Priority::Low, false) => {
                self.add_active_unchecked(peer, io);
                true
            }
            (Priority::Low, true) => false,
        }
    }

    fn add_active_unchecked(&mut self, peer: PI, io: &mut impl IO<PI>) {
        self.passive_view.remove(&peer);
        if self.active_view.insert(peer) {
            debug!(other = ?peer, "add to active view");
            io.push(OutEvent::EmitEvent(Event::NeighborUp(peer)));
        }
    }

    fn send_neighbor_request(&mut self, peer: PI, reason: NeighborReason, io: &mut impl IO<PI>) {
        if self
            .pending_neighbor_requests
            .insert(peer, reason)
            .is_none()
        {
            let message = Message::NeighborRequest(NeighborRequest {
                reason,
                data: self.me_data.clone(),
            });
            io.push(OutEvent::SendMessage(peer, message));
        }
    }

    fn send_neighbor_reply(&mut self, peer: PI, io: &mut impl IO<PI>) {
        let message = Message::NeighborReply(NeighborReply {});
        io.push(OutEvent::SendMessage(peer, message));
    }
}

#[derive(Debug)]
enum RemovalReason {
    /// A peer is removed because the connection was closed ungracefully.
    ConnectionClosed,
    /// A peer is removed because we received a disconnect message.
    DisconnectReceived { is_alive: bool },
    /// A peer is removed after random selection to make room for a newly joined peer.
    EvictRandom,
}
