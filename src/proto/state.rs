//! The protocol state of the `iroh-gossip` protocol.

use std::collections::{hash_map, HashMap};

use n0_future::time::{Duration, Instant};
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use tracing::trace;

use crate::proto::{
    hyparview,
    topic::{self, Command},
    util::idbytes_impls,
    Config, PeerData, PeerIdentity, MIN_MAX_MESSAGE_SIZE,
};

/// The identifier for a topic
#[derive(Clone, Copy, Eq, PartialEq, Hash, Serialize, Ord, PartialOrd, Deserialize)]
pub struct TopicId([u8; 32]);
idbytes_impls!(TopicId, "TopicId");

impl TopicId {
    /// Convert to a hex string limited to the first 5 bytes for a friendly string
    /// representation of the key.
    pub fn fmt_short(&self) -> String {
        data_encoding::HEXLOWER.encode(&self.as_bytes()[..5])
    }
}

/// Protocol wire message
///
/// This is the wire frame of the `iroh-gossip` protocol.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message<PI> {
    pub(crate) topic: TopicId,
    pub(crate) message: topic::Message<PI>,
}

impl<PI> Message<PI> {
    /// Get the kind of this message
    pub fn kind(&self) -> MessageKind {
        self.message.kind()
    }

    /// Returns whether a runtime may drop this message to a slow peer, see [`IO::send`].
    ///
    /// This holds for a payload and for an announcement of payloads. The state announces
    /// a dropped payload again, and the peer can request it.
    pub fn is_droppable(&self) -> bool {
        matches!(
            self.message,
            topic::Message::Gossip(
                super::plumtree::Message::Gossip(_) | super::plumtree::Message::IHave(_)
            )
        )
    }
}

/// Whether this is a control or data message
#[derive(Debug)]
#[non_exhaustive]
pub enum MessageKind {
    /// A data message.
    Data,
    /// A control message.
    Control,
}

impl<PI: Serialize> Message<PI> {
    /// Get the encoded size of this message
    pub fn size(&self) -> postcard::Result<usize> {
        postcard::experimental::serialized_size(&self)
    }
}

/// A timer that the runtime arms for the protocol state.
///
/// The state is IO-less, so it arms timers through [`IO::schedule`] with a
/// delay. When the delay ends, the runtime hands the timer back with
/// [`InEvent::TimerExpired`].
#[derive(Clone, Debug)]
pub struct Timer<PI> {
    topic: TopicId,
    /// The instance of the topic that armed the timer.
    instance: u64,
    timer: topic::Timer<PI>,
}

impl<PI> Timer<PI> {
    /// Returns whether this timer runs a periodic membership task.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn is_membership_cycle(&self) -> bool {
        matches!(
            self.timer,
            topic::Timer::Swarm(
                super::hyparview::Timer::DoShuffle | super::hyparview::Timer::DoMaintenance
            )
        )
    }
}

/// Input event to the protocol state.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum InEvent<PI> {
    /// Message received from the network.
    RecvMessage(PI, Message<PI>),
    /// Execute a command from the application.
    Command(TopicId, Command<PI>),
    /// Trigger a previously scheduled timer.
    TimerExpired(Timer<PI>),
    /// Peer disconnected on the network level.
    PeerDisconnected(PI),
    /// Update the opaque peer data about yourself.
    UpdatePeerData(PeerData),
}

impl<PI> InEvent<PI> {
    /// Returns whether handling this event can change what a topic wants, see [`State::wants`].
    ///
    /// This is true for all events except Plumtree messages, Plumtree timers, and peer data
    /// updates. These are most of the events, and they never change a view. A runtime asks
    /// [`State::wants`] only after an event for which this holds.
    pub fn may_change_wants(&self) -> bool {
        match self {
            InEvent::RecvMessage(_, message) => matches!(message.message, topic::Message::Swarm(_)),
            InEvent::TimerExpired(timer) => matches!(timer.timer, topic::Timer::Swarm(_)),
            InEvent::UpdatePeerData(_) => false,
            _ => true,
        }
    }
}

/// An action of the protocol state, as a value. A `Vec` of these implements [`IO`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum OutEvent<PI> {
    /// Send a message on the network
    SendMessage(PI, Message<PI>),
    /// Emit an event to the application.
    EmitEvent(TopicId, topic::Event<PI>),
    /// Schedule a timer. The runtime is responsible for sending an [InEvent::TimerExpired]
    /// after the duration.
    ScheduleTimer(Duration, Timer<PI>),
    /// Updated peer data
    PeerData(PI, PeerData),
}

/// The receiver of the actions of the protocol state.
///
/// The state calls this while it handles an event, and never waits on it. A runtime
/// may drop only some messages, see [`IO::send`].
pub trait IO<PI> {
    /// Sends `message` to `peer`. Returns the message if the queue dropped it.
    ///
    /// A queue may drop a message for which [`Message::is_droppable`] holds: a payload
    /// or an announcement, to a slow peer that stays connected. The state then
    /// announces the message to the peer, which can request it once it has room. It
    /// returns `None` if the message is queued, or if the peer is gone.
    ///
    /// The queue must not drop any other message, such as a membership message, a
    /// prune, or a graft. The two peers would then disagree on their links, and neither
    /// would notice. If the runtime cannot queue such a message, it closes every
    /// connection of the peer, drops what it queued for the peer, and hands
    /// [`InEvent::PeerDisconnected`] to the state. Both sides then start over.
    fn send(&mut self, peer: PI, message: Message<PI>) -> Option<Message<PI>>;

    /// Hands an event of `topic` to the application.
    fn emit(&mut self, topic: TopicId, event: topic::Event<PI>);

    /// Arms `timer`, which comes back as [`InEvent::TimerExpired`] after `delay`.
    fn schedule(&mut self, delay: Duration, timer: Timer<PI>);

    /// Reports new [`PeerData`] of `peer`.
    fn peer_data(&mut self, peer: PI, data: PeerData);
}

impl<PI> IO<PI> for Vec<OutEvent<PI>> {
    fn send(&mut self, peer: PI, message: Message<PI>) -> Option<Message<PI>> {
        self.push(OutEvent::SendMessage(peer, message));
        None
    }

    fn emit(&mut self, topic: TopicId, event: topic::Event<PI>) {
        self.push(OutEvent::EmitEvent(topic, event));
    }

    fn schedule(&mut self, delay: Duration, timer: Timer<PI>) {
        self.push(OutEvent::ScheduleTimer(delay, timer));
    }

    fn peer_data(&mut self, peer: PI, data: PeerData) {
        self.push(OutEvent::PeerData(peer, data));
    }
}

enum InEventMapped<PI> {
    All(topic::InEvent<PI>),
    TopicEvent(TopicId, topic::InEvent<PI>),
}

impl<PI> From<InEvent<PI>> for InEventMapped<PI> {
    fn from(event: InEvent<PI>) -> InEventMapped<PI> {
        match event {
            InEvent::RecvMessage(from, Message { topic, message }) => {
                Self::TopicEvent(topic, topic::InEvent::RecvMessage(from, message))
            }
            InEvent::Command(topic, command) => {
                Self::TopicEvent(topic, topic::InEvent::Command(command))
            }
            InEvent::TimerExpired(Timer { topic, timer, .. }) => {
                Self::TopicEvent(topic, topic::InEvent::TimerExpired(timer))
            }
            InEvent::PeerDisconnected(peer) => Self::All(topic::InEvent::PeerDisconnected(peer)),
            InEvent::UpdatePeerData(data) => Self::All(topic::InEvent::UpdatePeerData(data)),
        }
    }
}

/// The state of the `iroh-gossip` protocol.
///
/// The implementation works as an IO-less state machine. The implementer injects events through
/// [`Self::handle`], which writes the actions of the state into an [`IO`].
///
/// This struct contains a map of [`topic::State`] for each topic that was joined. It mostly acts as
/// a forwarder of [`InEvent`]s to matching topic state. Each topic's state is completely
/// independent; thus the actual protocol logic lives with [`topic::State`].
#[derive(Debug)]
pub struct State<PI, R> {
    me: PI,
    me_data: PeerData,
    config: Config,
    rng: R,
    states: HashMap<TopicId, topic::State<PI, R>>,
    /// The instance of each joined topic, so that the timers of a former one do nothing.
    instances: HashMap<TopicId, u64>,
    next_instance: u64,
}

impl<PI: PeerIdentity, R: Rng + SeedableRng> State<PI, R> {
    /// Create a new protocol state instance.
    ///
    /// `me` is the [`PeerIdentity`] of the local node, `peer_data` is the initial [`PeerData`]
    /// (which can be updated over time).
    /// For the protocol to perform as recommended in the papers, the [`Config`] should be
    /// identical for all nodes in the network.
    ///
    /// ## Panics
    ///
    /// Panics if [`Config::max_message_size`] is below [`MIN_MAX_MESSAGE_SIZE`].
    pub fn new(me: PI, me_data: PeerData, config: Config, rng: R) -> Self {
        assert!(
            config.max_message_size >= MIN_MAX_MESSAGE_SIZE,
            "max_message_size must be at least {MIN_MAX_MESSAGE_SIZE}"
        );
        Self {
            me,
            me_data,
            config,
            rng,
            states: Default::default(),
            instances: Default::default(),
            next_instance: 0,
        }
    }

    /// Get a reference to the node's [`PeerIdentity`]
    pub fn me(&self) -> &PI {
        &self.me
    }

    /// Get a reference to the protocol state for a topic.
    pub fn state(&self, topic: &TopicId) -> Option<&topic::State<PI, R>> {
        self.states.get(topic)
    }

    /// Resets the tracked stats for a topic.
    pub fn reset_stats(&mut self, topic: &TopicId) {
        if let Some(state) = self.states.get_mut(topic) {
            state.reset_stats();
        }
    }

    /// Get an iterator of all joined topics.
    pub fn topics(&self) -> impl Iterator<Item = &TopicId> {
        self.states.keys()
    }

    /// Get an iterator for the states of all joined topics.
    pub fn states(&self) -> impl Iterator<Item = (&TopicId, &topic::State<PI, R>)> {
        self.states.iter()
    }

    /// Returns whether a topic wants a connection to `peer`.
    ///
    /// A topic wants a neighbor, and a peer it waits on for a reply. The state does not
    /// say when to close a connection. The runtime decides it with this method: after
    /// each event for which [`InEvent::may_change_wants`] holds, it asks this for each
    /// peer that it has a connection to. It closes the connection to a peer that no
    /// topic wants, once the messages queued for the peer went out. Other events do not
    /// change what a topic wants, so they need no check.
    ///
    /// # Examples
    ///
    /// ```
    /// use iroh_gossip::proto::{Command, Config, InEvent, PeerData, State, TopicId};
    /// use n0_future::time::Instant;
    /// use rand::{rngs::StdRng, SeedableRng};
    ///
    /// let rng = StdRng::seed_from_u64(0);
    /// let mut state = State::new(1u64, PeerData::default(), Config::default(), rng);
    /// let topic = TopicId::from_bytes([0; 32]);
    /// let event = InEvent::Command(topic, Command::Join(vec![2]));
    /// let check = event.may_change_wants();
    /// let mut out = Vec::new();
    /// state.handle(event, Instant::now(), &mut out);
    /// if check {
    ///     // The join waits for a reply of peer 2, so the runtime keeps a connection to it.
    ///     assert!(state.wants(&2));
    ///     // A runtime closes a connection to a peer that no topic wants.
    ///     assert!(!state.wants(&3));
    /// }
    /// ```
    pub fn wants(&self, peer: &PI) -> bool {
        self.states.values().any(|state| state.wants(peer))
    }

    /// Returns the topics that want a connection to `peer`.
    #[cfg(feature = "net")]
    pub(crate) fn topics_wanting<'a>(&'a self, peer: &'a PI) -> impl Iterator<Item = TopicId> + 'a {
        self.states
            .iter()
            .filter(move |(_, state)| state.wants(peer))
            .map(|(topic, _)| *topic)
    }

    /// Returns the peers to which `topic` pushes its payloads.
    #[cfg(feature = "net")]
    pub(crate) fn eager_peers(&self, topic: &TopicId) -> impl Iterator<Item = &PI> {
        self.state(topic)
            .into_iter()
            .flat_map(|state| state.gossip.eager_push_peers.iter())
    }

    /// Returns the peers to which `topic` pushes its payloads.
    #[cfg(feature = "net")]
    pub(crate) fn neighbors(&self, topic: &TopicId) -> impl Iterator<Item = &PI> {
        self.state(topic)
            .into_iter()
            .flat_map(|state| state.swarm.active_view.iter())
    }

    /// Check if a topic has any active (connected) peers.
    pub fn has_active_peers(&self, topic: &TopicId) -> bool {
        self.state(topic)
            .map(|s| s.has_active_peers())
            .unwrap_or(false)
    }

    /// Returns the maximum message size configured in the gossip protocol.
    pub fn max_message_size(&self) -> usize {
        self.config.max_message_size
    }

    /// Handles an [`InEvent`], and puts what it does into `io`.
    pub fn handle(&mut self, event: InEvent<PI>, now: Instant, io: &mut impl IO<PI>) {
        trace!("in : {event:?}");

        // A timer of a topic we quit, or of its former instance, is stale.
        if let InEvent::TimerExpired(timer) = &event {
            if self.instances.get(&timer.topic) != Some(&timer.instance) {
                return;
            }
        }
        let event: InEventMapped<PI> = event.into();

        match event {
            InEventMapped::TopicEvent(topic, event) => {
                // when receiving a join command, initialize state if it doesn't exist
                if matches!(&event, topic::InEvent::Command(Command::Join(_peers))) {
                    if let hash_map::Entry::Vacant(e) = self.states.entry(topic) {
                        self.next_instance += 1;
                        self.instances.insert(topic, self.next_instance);
                        e.insert(topic::State::with_rng(
                            self.me,
                            Some(self.me_data.clone()),
                            self.config.clone(),
                            R::from_rng(&mut self.rng),
                        ));
                    }
                }

                // when receiving a quit command, note this and drop the topic state after
                // processing this last event
                let quit = matches!(event, topic::InEvent::Command(Command::Quit));

                // pass the event to the state handler
                if let Some(state) = self.states.get_mut(&topic) {
                    let mut topic_io = TopicIO {
                        topic,
                        instance: self.instances.get(&topic).copied().unwrap_or(0),
                        io,
                    };
                    state.handle(event, now, &mut topic_io);
                } else if let topic::InEvent::RecvMessage(from, message) = &event {
                    // The sender thinks that we are on the topic. Plumtree messages are
                    // mostly stragglers after we left, so they get no answer.
                    if matches!(message, topic::Message::Swarm(_)) && !message.is_disconnect() {
                        let message = topic::Message::Swarm(hyparview::Message::left());
                        io.send(*from, Message { topic, message });
                    }
                }

                if quit {
                    self.states.remove(&topic);
                    self.instances.remove(&topic);
                }
            }
            // when a peer disconnected on the network level, forward event to all states
            InEventMapped::All(event) => {
                if let topic::InEvent::UpdatePeerData(data) = &event {
                    self.me_data = data.clone();
                }
                for (topic, state) in self.states.iter_mut() {
                    let mut topic_io = TopicIO {
                        topic: *topic,
                        instance: self.instances.get(topic).copied().unwrap_or(0),
                        io,
                    };
                    state.handle(event.clone(), now, &mut topic_io);
                }
            }
        }
    }
}

/// The IO of one topic: it tags each action of the topic state with the topic.
struct TopicIO<'a, E> {
    topic: TopicId,
    instance: u64,
    io: &'a mut E,
}

impl<PI: PeerIdentity, E: IO<PI>> topic::IO<PI> for TopicIO<'_, E> {
    fn send(
        &mut self,
        peer: PI,
        message: impl Into<topic::Message<PI>>,
    ) -> Option<topic::Message<PI>> {
        let message = message.into();
        let topic = self.topic;
        self.io
            .send(peer, Message { topic, message })
            .map(|dropped| dropped.message)
    }

    fn emit(&mut self, event: topic::Event<PI>) {
        self.io.emit(self.topic, event);
    }

    fn schedule(&mut self, delay: Duration, timer: impl Into<topic::Timer<PI>>) {
        let timer = Timer {
            topic: self.topic,
            instance: self.instance,
            timer: timer.into(),
        };
        self.io.schedule(delay, timer);
    }

    fn peer_data(&mut self, peer: PI, data: PeerData) {
        self.io.peer_data(peer, data);
    }
}

#[cfg(test)]
mod tests {
    use rand::rngs::StdRng;

    use super::*;
    use crate::proto::{hyparview, plumtree};

    /// Handles `event` and returns what it produces.
    fn run(state: &mut State<u32, StdRng>, event: InEvent<u32>) -> Vec<OutEvent<u32>> {
        run_at(state, event, Instant::now())
    }

    /// Handles `event` at `now` and returns what it produces.
    fn run_at(
        state: &mut State<u32, StdRng>,
        event: InEvent<u32>,
        now: Instant,
    ) -> Vec<OutEvent<u32>> {
        let mut out = Vec::new();
        state.handle(event, now, &mut out);
        out
    }

    fn new_state(seed: u64) -> State<u32, StdRng> {
        State::new(
            0u32,
            PeerData::default(),
            Config::default(),
            StdRng::seed_from_u64(seed),
        )
    }

    /// Handles `event` and drops what it produces.
    fn handle(state: &mut State<u32, StdRng>, event: InEvent<u32>) {
        run(state, event);
    }

    /// A message for a topic that we are not on gets a `Close`, and makes no topic want the peer.
    ///
    /// A `Close` and a Plumtree message get no answer.
    #[test]
    fn message_for_an_unknown_topic_gets_a_disconnect() {
        let mut state = State::new(
            0u32,
            PeerData::default(),
            Config::default(),
            StdRng::seed_from_u64(1),
        );
        let topic: TopicId = [0u8; 32].into();
        let messages = [
            (topic::Message::Swarm(hyparview::Message::Join(None)), true),
            (
                topic::Message::Swarm(hyparview::Message::test_shuffle(2, 3)),
                true,
            ),
            (topic::Message::Gossip(plumtree::Message::Prune), false),
            (
                topic::Message::Swarm(hyparview::Message::test_neighbor()),
                true,
            ),
            (
                topic::Message::Swarm(hyparview::Message::test_close()),
                false,
            ),
            (topic::Message::Swarm(hyparview::Message::left()), false),
        ];
        for (peer, (message, answered)) in (1u32..).zip(messages) {
            let event = InEvent::RecvMessage(peer, Message { topic, message });
            let out = run(&mut state, event);
            let left = out
                .iter()
                .filter(|e| {
                    matches!(e, OutEvent::SendMessage(to, m)
                        if *to == peer && m.message == topic::Message::Swarm(hyparview::Message::left()))
                })
                .count();
            assert_eq!(left, usize::from(answered), "{out:?}");
        }
        assert!((1..=6).all(|peer| !state.wants(&peer)));
    }

    /// A repeated join for a topic that we are not on gets an answer each time.
    #[test]
    fn unknown_topic_answers_every_join() {
        let mut state = State::new(
            0u32,
            PeerData::default(),
            Config::default(),
            StdRng::seed_from_u64(1),
        );
        let topic: TopicId = [0u8; 32].into();
        let answers = |state: &mut State<u32, StdRng>| {
            let join = Message {
                topic,
                message: topic::Message::Swarm(hyparview::Message::Join(None)),
            };
            run(state, InEvent::RecvMessage(1, join))
                .iter()
                .filter(|e| matches!(e, OutEvent::SendMessage(1, m) if m.message.is_disconnect()))
                .count()
        };
        assert_eq!(answers(&mut state), 1);
        assert_eq!(answers(&mut state), 1, "the repeated join got no answer");
    }

    /// After a quit, no topic wants a peer that the topic only sent a join to.
    ///
    /// The peer is not on the topic and never answers, so nothing else ever
    /// dropped it.
    #[test]
    fn quit_ends_the_want_for_an_unanswered_join() {
        let mut state = new_state(0);
        let topic: TopicId = [0u8; 32].into();
        handle(&mut state, InEvent::Command(topic, Command::Join(vec![1])));
        assert!(state.wants(&1), "a pending join is not wanted");

        handle(&mut state, InEvent::Command(topic, Command::Quit));

        assert!(!state.wants(&1));
    }

    /// A Plumtree message from a non-neighbor makes no topic want the peer.
    ///
    /// It is a straggler from a peer that we dropped. We only ever see such a
    /// peer's messages for topics we joined, so the test joins the topic first.
    #[test]
    fn plumtree_message_from_a_non_neighbor_uses_no_peer() {
        let mut state = State::new(
            0u32,
            PeerData::default(),
            Config::default(),
            StdRng::seed_from_u64(1),
        );
        let topic: TopicId = [0u8; 32].into();
        let peer = 1u32;
        handle(&mut state, InEvent::Command(topic, Command::Join(vec![])));
        let ihave = plumtree::IHave {
            id: plumtree::MessageId::from_content(b"late"),
            round: 1u16.into(),
        };
        let message = Message {
            topic,
            message: topic::Message::Gossip(plumtree::Message::IHave(vec![ihave])),
        };
        handle(&mut state, InEvent::RecvMessage(peer, message));
        assert!(!state.wants(&peer));
    }

    /// Only payloads and announcements may be dropped, see [`IO::send`].
    #[cfg(feature = "net")]
    #[test]
    fn only_payloads_and_announcements_are_droppable() {
        let message = |message| Message {
            topic: TopicId::from_bytes([0; 32]),
            message,
        };
        assert!(message(topic::Message::<u32>::test_gossip(10)).is_droppable());
        assert!(message(topic::Message::<u32>::test_ihave()).is_droppable());
        assert!(!message(topic::Message::<u32>::test_prune()).is_droppable());
        assert!(!message(topic::Message::test_shuffle(1u32, 0)).is_droppable());
    }

    /// A timer of a topic that we quit does nothing to the topic we joined again.
    ///
    /// The new topic state starts its request ids at zero again, so the old
    /// timer has the id of the new request.
    #[test]
    fn timer_from_a_quit_topic_does_nothing_to_its_replacement() {
        let mut state = State::new(
            0u32,
            PeerData::default(),
            Config::default(),
            StdRng::seed_from_u64(0),
        );
        let topic: TopicId = [1; 32].into();
        let join = || InEvent::Command(topic, Command::Join(vec![]));
        // A shuffle reply names peer 2, and the refill sends it a `Neighbor`.
        let reply = || {
            let reply = hyparview::Message::test_shuffle_reply_with(vec![2]);
            InEvent::RecvMessage(
                1,
                Message {
                    topic,
                    message: topic::Message::Swarm(reply),
                },
            )
        };
        let now = Instant::now();
        let request_timer = |out: Vec<OutEvent<u32>>| {
            out.into_iter()
                .find_map(|event| match event {
                    OutEvent::ScheduleTimer(_, timer)
                        if matches!(
                            &timer.timer,
                            topic::Timer::Swarm(hyparview::Timer::PendingNeighborRequest(2, _))
                        ) =>
                    {
                        Some(timer)
                    }
                    _ => None,
                })
                .expect("a neighbor request to peer 2")
        };
        handle(&mut state, join());
        let old = request_timer(run_at(&mut state, reply(), now));
        handle(&mut state, InEvent::Command(topic, Command::Quit));
        handle(&mut state, join());
        let current = request_timer(run_at(&mut state, reply(), now));

        handle(&mut state, InEvent::TimerExpired(old));
        assert!(state.wants(&2), "the old timer ended the new request");
        handle(&mut state, InEvent::TimerExpired(current));
        assert!(!state.wants(&2), "the request did not time out");
    }

    /// A peer stays wanted until the last topic that wants it drops it.
    #[test]
    fn peer_is_wanted_until_the_last_topic_drops_it() {
        let mut state = new_state(0);
        let (a, b): (TopicId, TopicId) = ([1u8; 32].into(), [2u8; 32].into());
        for topic in [a, b] {
            handle(&mut state, InEvent::Command(topic, Command::Join(vec![1])));
        }
        handle(&mut state, InEvent::Command(a, Command::Quit));
        assert!(state.wants(&1), "dropped while topic b still waits for it");
        handle(&mut state, InEvent::Command(b, Command::Quit));
        assert!(!state.wants(&1));
    }

    /// A pending join stays wanted after the connection closed.
    ///
    /// Its retry still matches the entry. The runtime does not reopen the queue
    /// for a want, only a send does, so a wanted peer keeps no connection open.
    #[test]
    fn pending_join_is_wanted_after_a_close() {
        let mut state = new_state(0);
        let topic: TopicId = [0u8; 32].into();
        handle(&mut state, InEvent::Command(topic, Command::Join(vec![1])));
        handle(&mut state, InEvent::PeerDisconnected(1));
        assert!(state.wants(&1));
    }

    /// Only events for the membership layer can change what a topic wants.
    #[test]
    fn plumtree_events_do_not_change_wants() {
        let topic: TopicId = [0u8; 32].into();
        let gossip = topic::Message::Gossip(plumtree::Message::Prune);
        let swarm = topic::Message::Swarm(hyparview::Message::Join(None));
        let recv = |message| InEvent::RecvMessage(1, Message { topic, message });
        assert!(!recv(gossip).may_change_wants());
        assert!(recv(swarm).may_change_wants());
        assert!(InEvent::PeerDisconnected(1).may_change_wants());
        assert!(InEvent::<u32>::Command(topic, Command::Quit).may_change_wants());
    }
}
