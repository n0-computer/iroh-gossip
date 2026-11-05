//! This module contains the implementation of the gossiping protocol for an individual topic

#[cfg(test)]
use std::collections::VecDeque;

use bytes::Bytes;
use derive_more::From;
use n0_future::time::{Duration, Instant};
use rand::Rng;
use serde::{Deserialize, Serialize};

use super::{
    hyparview::{self, InEvent as SwarmIn},
    plumtree::{self, GossipEvent, InEvent as GossipIn, Scope},
    state::MessageKind,
    PeerData, PeerIdentity, DEFAULT_MAX_MESSAGE_SIZE,
};
use crate::proto::MIN_MAX_MESSAGE_SIZE;

/// Input event to the topic state handler.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum InEvent<PI> {
    /// Message received from the network.
    RecvMessage(PI, Message<PI>),
    /// Execute a command from the application.
    Command(Command<PI>),
    /// Trigger a previously scheduled timer.
    TimerExpired(Timer<PI>),
    /// Peer disconnected on the network level.
    PeerDisconnected(PI),
    /// Update the opaque peer data about yourself.
    UpdatePeerData(PeerData),
}

/// An action of the topic state, as a value. A `VecDeque` of these implements [`IO`].
#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OutEvent<PI> {
    /// Send a message on the network
    SendMessage(PI, Message<PI>),
    /// Emit an event to the application.
    EmitEvent(Event<PI>),
    /// Schedule a timer. The runtime is responsible for sending an [InEvent::TimerExpired]
    /// after the duration.
    ScheduleTimer(Duration, Timer<PI>),
    /// Emitted when new [`PeerData`] was received for a peer.
    PeerData(PI, PeerData),
}

/// The receiver of the actions of the topic state.
///
/// The state calls this while it handles an event, and never waits on it.
pub trait IO<PI> {
    /// Sends `message` to `peer`. Returns the message if the queue dropped it.
    ///
    /// A queue may drop a broadcast message to a slow peer that stays
    /// connected. The state then announces the message to the peer, which
    /// can request it once it has room.
    fn send(&mut self, peer: PI, message: impl Into<Message<PI>>) -> Option<Message<PI>>;

    /// Hands `event` to the application.
    fn emit(&mut self, event: Event<PI>);

    /// Arms `timer`, which comes back as [`InEvent::TimerExpired`] after `delay`.
    fn schedule(&mut self, delay: Duration, timer: impl Into<Timer<PI>>);

    /// Reports new [`PeerData`] of `peer`.
    fn peer_data(&mut self, peer: PI, data: PeerData);
}

#[cfg(test)]
impl<PI> IO<PI> for VecDeque<OutEvent<PI>> {
    fn send(&mut self, peer: PI, message: impl Into<Message<PI>>) -> Option<Message<PI>> {
        self.push_back(OutEvent::SendMessage(peer, message.into()));
        None
    }

    fn emit(&mut self, event: Event<PI>) {
        self.push_back(OutEvent::EmitEvent(event));
    }

    fn schedule(&mut self, delay: Duration, timer: impl Into<Timer<PI>>) {
        self.push_back(OutEvent::ScheduleTimer(delay, timer.into()));
    }

    fn peer_data(&mut self, peer: PI, data: PeerData) {
        self.push_back(OutEvent::PeerData(peer, data));
    }
}

/// Returns the largest encoded Plumtree message that fits a frame.
///
/// A frame holds the enum tag of [`Message`] and the Plumtree message, as the
/// stream header carries the topic. It must be shorter than `max_message_size`,
/// as nodes up to 0.101 refuse to write a frame of that size.
pub(crate) fn max_plumtree_size(max_message_size: usize) -> usize {
    max_message_size - 2
}

/// Returns the largest content that a broadcast takes, so that it fits a frame at every hop.
///
/// A broadcast to the neighbors encodes no larger than one to the swarm, so the
/// limit holds for both.
#[cfg(feature = "net")]
pub(crate) fn max_content_size(max_message_size: usize) -> usize {
    (0..=max_plumtree_size(max_message_size))
        .rev()
        .find(|len| broadcast_fits(*len, Scope::Swarm, max_message_size))
        .unwrap_or(0)
}

/// Returns whether a broadcast with `content_len` bytes of content fits a frame at every hop.
#[cfg(feature = "net")]
pub(crate) fn broadcast_fits(content_len: usize, scope: Scope, max_message_size: usize) -> bool {
    plumtree::max_gossip_size(content_len, scope) <= max_plumtree_size(max_message_size)
}

/// A protocol message for a particular topic
#[derive(From, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Message<PI> {
    /// A message of the swarm membership layer
    Swarm(hyparview::Message<PI>),
    /// A message of the gossip broadcast layer
    Gossip(plumtree::Message),
}

#[cfg(all(test, feature = "net"))]
impl<PI> Message<PI> {
    /// Returns a `Disconnect`, for the network tests.
    pub(crate) fn test_disconnect() -> Self {
        Message::Swarm(hyparview::Message::test_disconnect())
    }

    /// Returns a payload of `len` bytes, as an eager push sends it, for the network tests.
    pub(crate) fn test_gossip(len: usize) -> Self {
        Message::Gossip(plumtree::Message::test_gossip(len))
    }

    /// Returns a payload with `content`, as its publisher sends it, for the network tests.
    pub(crate) fn test_payload(content: Bytes) -> Self {
        Message::Gossip(plumtree::Message::test_payload(content))
    }

    /// Returns an `IHave` of one message, for the network tests.
    pub(crate) fn test_ihave() -> Self {
        Message::Gossip(plumtree::Message::test_ihave())
    }

    /// Returns a `Prune`, for the network tests.
    pub(crate) fn test_prune() -> Self {
        Message::Gossip(plumtree::Message::test_prune())
    }

    /// Returns a shuffle from `origin` carrying no nodes, for the network tests.
    pub(crate) fn test_shuffle(origin: PI, ttl: u16) -> Self {
        Message::Swarm(hyparview::Message::test_shuffle(origin, ttl))
    }
}

impl<PI> Message<PI> {
    /// Get the kind of this message
    pub fn kind(&self) -> MessageKind {
        match self {
            Message::Swarm(_) => MessageKind::Control,
            Message::Gossip(message) => match message {
                plumtree::Message::Gossip(_) => MessageKind::Data,
                _ => MessageKind::Control,
            },
        }
    }

    /// Returns `true` if this is a disconnect message (which is the last message sent to a peer per topic).
    pub fn is_disconnect(&self) -> bool {
        matches!(self, Message::Swarm(hyparview::Message::Disconnect(_)))
    }

    /// Returns `true` if this announces messages, which the peer can still request.
    #[cfg(feature = "net")]
    pub(crate) fn is_announcement(&self) -> bool {
        matches!(self, Message::Gossip(plumtree::Message::IHave(_)))
    }
}

/// An event to be emitted to the application for a particular topic.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Event<PI> {
    /// We have a new, direct neighbor in the swarm membership layer for this topic
    NeighborUp(PI),
    /// We dropped direct neighbor in the swarm membership layer for this topic
    NeighborDown(PI),
    /// A gossip message was received for this topic
    Received(GossipEvent<PI>),
}

/// A timer to be registered for a particular topic.
///
/// This should be treated as an opaque value by the implementer and, once emitted, simply returned
/// to the protocol through [`InEvent::TimerExpired`].
#[derive(Clone, From, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Timer<PI> {
    /// A timer for the swarm layer
    Swarm(hyparview::Timer<PI>),
    /// A timer for the gossip layer
    Gossip(plumtree::Timer),
}

/// A command to the protocol state for a particular topic.
#[derive(Clone, derive_more::Debug)]
#[non_exhaustive]
pub enum Command<PI> {
    /// Join this topic and connect to peers.
    ///
    /// If the list of peers is empty, will prepare the state and accept incoming join requests,
    /// but only become operational after the first join request by another peer.
    Join(Vec<PI>),
    /// Broadcast a message for this topic.
    Broadcast(#[debug("<{}b>", _0.len())] Bytes, Scope),
    /// Leave this topic and drop all state.
    Quit,
}

/// Protocol configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct Config {
    /// Configuration for the swarm membership layer
    pub membership: hyparview::Config,
    /// Configuration for the gossip broadcast layer
    pub broadcast: plumtree::Config,
    /// Max message size in bytes.
    ///
    /// This size should be the same across a network to ensure all nodes can transmit and read large messages.
    ///
    /// At minimum, this size should be large enough to send gossip control messages. This can vary, depending on the size of the [`PeerIdentity`] you use and the size of the [`PeerData`] you transmit in your messages.
    ///
    /// The default is [`DEFAULT_MAX_MESSAGE_SIZE`].
    pub max_message_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            membership: Default::default(),
            broadcast: Default::default(),
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
        }
    }
}

/// The topic state maintains the swarm membership and broadcast tree for a particular topic.
#[derive(Debug)]
pub struct State<PI, R> {
    me: PI,
    pub(crate) swarm: hyparview::State<PI, R>,
    pub(crate) gossip: plumtree::State<PI>,
}

impl<PI: PeerIdentity> State<PI, rand::rngs::ThreadRng> {
    /// Initialize the local state with the default random number generator.
    ///
    /// ## Panics
    ///
    /// Panics if [`Config::max_message_size`] is below [`MIN_MAX_MESSAGE_SIZE`].
    pub fn new(me: PI, me_data: Option<PeerData>, config: Config) -> Self {
        Self::with_rng(me, me_data, config, rand::rng())
    }
}

impl<PI, R> State<PI, R> {
    /// The address of your local endpoint.
    pub fn endpoint(&self) -> &PI {
        &self.me
    }
}

impl<PI: PeerIdentity, R: Rng> State<PI, R> {
    /// Initialize the local state with a custom random number generator.
    ///
    /// ## Panics
    ///
    /// Panics if [`Config::max_message_size`] is below [`MIN_MAX_MESSAGE_SIZE`].
    pub fn with_rng(me: PI, me_data: Option<PeerData>, config: Config, rng: R) -> Self {
        assert!(
            config.max_message_size >= MIN_MAX_MESSAGE_SIZE,
            "max_message_size must be at least {MIN_MAX_MESSAGE_SIZE}"
        );
        Self {
            swarm: hyparview::State::new(me, me_data, config.membership, rng),
            gossip: plumtree::State::new(
                me,
                config.broadcast,
                max_plumtree_size(config.max_message_size),
            ),
            me,
        }
    }

    /// Handles an incoming event, and puts what it does into `io`.
    pub fn handle(&mut self, event: InEvent<PI>, now: Instant, io: &mut impl IO<PI>) {
        let swarm = &mut self.swarm;
        // The membership layer writes through the wrapper. The broadcast layer writes to `io`
        // directly.
        let mut io = SwarmIO {
            io,
            gossip: &mut self.gossip,
        };
        match event {
            InEvent::Command(command) => match command {
                Command::Join(peers) => {
                    for peer in peers {
                        swarm.handle(SwarmIn::RequestJoin(peer), &mut io);
                    }
                }
                Command::Broadcast(data, scope) => {
                    io.gossip
                        .handle(GossipIn::Broadcast(data, scope), now, io.io)
                }
                Command::Quit => {
                    // The quit empties the active view without a `NeighborDown`.
                    for peer in swarm.active_view.iter() {
                        io.gossip.on_neighbor_down(*peer);
                    }
                    swarm.handle(SwarmIn::Quit, &mut io);
                }
            },
            InEvent::RecvMessage(from, message) => match message {
                Message::Swarm(message) => {
                    swarm.handle(SwarmIn::RecvMessage(from, message), &mut io)
                }
                Message::Gossip(message) => {
                    io.gossip
                        .handle(GossipIn::RecvMessage(from, message), now, io.io)
                }
            },
            InEvent::TimerExpired(timer) => match timer {
                Timer::Swarm(timer) => swarm.handle(SwarmIn::TimerExpired(timer), &mut io),
                Timer::Gossip(timer) => io.gossip.handle(GossipIn::TimerExpired(timer), now, io.io),
            },
            InEvent::PeerDisconnected(peer) => {
                swarm.handle(SwarmIn::PeerDisconnected(peer), &mut io);
                io.gossip.on_neighbor_down(peer);
            }
            InEvent::UpdatePeerData(data) => swarm.handle(SwarmIn::UpdatePeerData(data), &mut io),
        }

        #[cfg(test)]
        self.check_invariants();
    }

    /// Panics if Plumtree's peers are not exactly HyParView's neighbors.
    #[cfg(test)]
    fn check_invariants(&self) {
        let gossip = &self.gossip;
        let peers: std::collections::BTreeSet<_> = gossip
            .eager_push_peers
            .union(&gossip.lazy_push_peers)
            .collect();
        let neighbors: std::collections::BTreeSet<_> = self.swarm.active_view.iter().collect();
        assert_eq!(peers, neighbors, "Plumtree's peers are not the neighbors");
    }

    /// Reset all statistics.
    pub fn reset_stats(&mut self) {
        self.gossip.stats = Default::default();
    }

    /// Get statistics for the gossip broadcast state
    ///
    /// TODO: Remove/replace with metrics?
    pub fn gossip_stats(&self) -> &plumtree::Stats {
        self.gossip.stats()
    }

    /// Returns whether the topic wants a connection to `peer`, see [`hyparview::State::wants`].
    pub(crate) fn wants(&self, peer: &PI) -> bool {
        self.swarm.wants(peer)
    }

    /// Check if this topic has any active (connected) peers.
    pub fn has_active_peers(&self) -> bool {
        !self.swarm.active_view.is_empty()
    }
}

/// The IO of the membership layer: its neighbor changes reach the broadcast layer first.
struct SwarmIO<'a, PI, E> {
    io: &'a mut E,
    gossip: &'a mut plumtree::State<PI>,
}

impl<PI: PeerIdentity, E: IO<PI>> IO<PI> for SwarmIO<'_, PI, E> {
    fn send(&mut self, peer: PI, message: impl Into<Message<PI>>) -> Option<Message<PI>> {
        self.io.send(peer, message)
    }

    fn emit(&mut self, event: Event<PI>) {
        match &event {
            Event::NeighborUp(peer) => self.gossip.on_neighbor_up(*peer),
            Event::NeighborDown(peer) => self.gossip.on_neighbor_down(*peer),
            _ => {}
        }
        self.io.emit(event);
    }

    fn schedule(&mut self, delay: Duration, timer: impl Into<Timer<PI>>) {
        self.io.schedule(delay, timer);
    }

    fn peer_data(&mut self, peer: PI, data: PeerData) {
        self.io.peer_data(peer, data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A broadcast goes out if its frame fits the limit at every round, and not otherwise.
    #[test]
    fn broadcast_fits_the_frame_limit_at_every_round() {
        let content = Bytes::from(vec![1; 1024]);
        let goes_out = |max_message_size: usize| {
            let now = Instant::now();
            let config = Config {
                max_message_size,
                ..Config::default()
            };
            let mut state = State::with_rng(0u32, None, config, rand::rng());
            let join = InEvent::RecvMessage(1, Message::Swarm(hyparview::Message::Join(None)));
            state.handle(join, now, &mut VecDeque::new());
            let broadcast = InEvent::Command(Command::Broadcast(content.clone(), Scope::Swarm));
            let mut out = VecDeque::new();
            state.handle(broadcast, now, &mut out);
            out.iter().any(|event| {
                matches!(
                    event,
                    OutEvent::SendMessage(1, Message::Gossip(plumtree::Message::Gossip(_)))
                )
            })
        };
        // The frame holds the enum tag of the topic message, and must be shorter than the limit.
        let limit = plumtree::max_gossip_size(content.len(), Scope::Swarm) + 2;
        assert!(goes_out(limit));
        assert!(!goes_out(limit - 1));
    }
}
