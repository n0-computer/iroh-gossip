//! The protocol state of the `iroh-gossip` protocol.

use std::collections::{hash_map, HashMap, HashSet};

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
    /// Returns the first 5 bytes as hex, a short form of the key for logs.
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

    /// Hands the runtime new [`PeerData`] of `peer`, for example its addresses.
    fn peer_data(&mut self, peer: PI, data: PeerData);

    /// Asks to close the connection to `peer`, as no topic uses it anymore.
    fn disconnect(&mut self, peer: PI);
}

type ConnsMap<PI> = HashMap<PI, HashSet<TopicId>>;

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
    peer_topics: ConnsMap<PI>,
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
            peer_topics: Default::default(),
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

    /// Returns whether a topic uses `peer`, so that our queue to it stays open.
    #[cfg(feature = "net")]
    pub(crate) fn uses_peer(&self, peer: &PI) -> bool {
        self.peer_topics.contains_key(peer)
    }

    /// Returns the topics that use `peer`.
    #[cfg(feature = "net")]
    pub(crate) fn topics_of(&self, peer: &PI) -> impl Iterator<Item = TopicId> + '_ {
        self.peer_topics.get(peer).into_iter().flatten().copied()
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
                    // A HyParView message makes the topic use the peer. A Plumtree message
                    // does not: it can be a straggler from a peer that we dropped.
                    if let topic::InEvent::RecvMessage(from, topic::Message::Swarm(_)) = &event {
                        self.peer_topics.entry(*from).or_default().insert(topic);
                    }
                    let mut topic_io = TopicIO {
                        topic,
                        instance: self.instances.get(&topic).copied().unwrap_or(0),
                        peer_topics: &mut self.peer_topics,
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
                    // A peer the topic only sent to, such as an unanswered join, has no
                    // other way out.
                    let mut unused = Vec::new();
                    for (peer, topics) in self.peer_topics.iter_mut() {
                        if topics.remove(&topic) && topics.is_empty() {
                            unused.push(*peer);
                        }
                    }
                    for peer in unused {
                        self.peer_topics.remove(&peer);
                        io.disconnect(peer);
                    }
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
                        peer_topics: &mut self.peer_topics,
                        io,
                    };
                    state.handle(event.clone(), now, &mut topic_io);
                }
                // If the peer disconnected, make sure to clear its `peer_topics` entry here.
                // `TopicIO::disconnect` does the same, but only for peers that we were neighbors
                // with. Peers that only relayed a shuffle or forward join to us also have
                // entries in `peer_topics`, so we clear them here explicitly. This has to
                // stay after the loop: `TopicIO::disconnect` needs the entry to tell whether a
                // topic's disconnect was the peer's last.
                if let topic::InEvent::PeerDisconnected(peer) = &event {
                    self.peer_topics.remove(peer);
                }
            }
        }
    }
}

/// The IO of one topic: it tags each action of the topic state with the topic.
///
/// It also keeps track of which topics use which peer.
struct TopicIO<'a, PI, E> {
    topic: TopicId,
    instance: u64,
    peer_topics: &'a mut ConnsMap<PI>,
    io: &'a mut E,
}

impl<PI: PeerIdentity, E: IO<PI>> topic::IO<PI> for TopicIO<'_, PI, E> {
    fn send(
        &mut self,
        peer: PI,
        message: impl Into<topic::Message<PI>>,
    ) -> Option<topic::Message<PI>> {
        let message = message.into();
        // HyParView may send to a peer that is no neighbor yet. The topic uses it from then on.
        if matches!(message, topic::Message::Swarm(_)) {
            self.peer_topics.entry(peer).or_default().insert(self.topic);
        }
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

    fn disconnect(&mut self, peer: PI) {
        // The connection is shared by every topic that uses the peer.
        if let Some(topics) = self.peer_topics.get_mut(&peer) {
            topics.remove(&self.topic);
            if topics.is_empty() {
                // If the peer is no longer used by any topic, disconnect.
                self.peer_topics.remove(&peer);
                self.io.disconnect(peer);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use rand::rngs::StdRng;

    use super::*;
    use crate::proto::{hyparview, plumtree};

    /// An output of the protocol state, as a value, for the tests.
    ///
    /// A `Vec` of these implements [`IO`].
    // The tests read only some of the fields.
    #[allow(dead_code)]
    #[derive(Debug, Clone)]
    pub(crate) enum OutEvent<PI> {
        /// Send a message on the network
        SendMessage(PI, Message<PI>),
        /// Emit an event to the application.
        EmitEvent(TopicId, topic::Event<PI>),
        /// Schedules a timer.
        ///
        /// The runtime sends an [InEvent::TimerExpired] after the duration.
        ScheduleTimer(Duration, Timer<PI>),
        /// Close the connection to a peer on the network level.
        DisconnectPeer(PI),
        /// Updated peer data
        PeerData(PI, PeerData),
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

        fn disconnect(&mut self, peer: PI) {
            self.push(OutEvent::DisconnectPeer(peer));
        }
    }

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

    /// Handles `event` and drops what it produces.
    fn handle(state: &mut State<u32, StdRng>, event: InEvent<u32>) {
        run(state, event);
    }

    /// A message for a topic that we are not on gets a `Disconnect`, and uses no peer.
    ///
    /// A `Disconnect` and a Plumtree message get no answer.
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
                topic::Message::Swarm(hyparview::Message::test_disconnect()),
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
        assert!(state.peer_topics.is_empty());
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

    /// Quitting a topic disconnects a peer the topic only sent a join to.
    ///
    /// The peer is not on the topic and never answers, so nothing else ever
    /// dropped it.
    #[test]
    fn quit_disconnects_a_peer_it_only_sent_to() {
        let mut state = State::new(
            0u32,
            PeerData::default(),
            Config::default(),
            StdRng::seed_from_u64(1),
        );
        let topic: TopicId = [0u8; 32].into();
        handle(&mut state, InEvent::Command(topic, Command::Join(vec![1])));

        let out = run(&mut state, InEvent::Command(topic, Command::Quit));

        assert!(
            out.iter()
                .any(|event| matches!(event, OutEvent::DisconnectPeer(1))),
            "{out:?}"
        );
        assert!(state.peer_topics.is_empty());
    }

    /// A Plumtree message from a non-neighbor makes no topic use the peer.
    ///
    /// It is a straggler from a peer that we dropped. If it made an entry, the
    /// entry would keep the peer's connection open. We only ever see such a
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
        assert!(!state.peer_topics.contains_key(&peer));
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
        let disconnects = |out: &[OutEvent<u32>]| {
            out.iter()
                .any(|event| matches!(event, OutEvent::DisconnectPeer(2)))
        };

        let out = run_at(&mut state, InEvent::TimerExpired(old), now);
        assert!(
            !disconnects(&out),
            "the old timer ended the new request: {out:?}"
        );
        let out = run_at(&mut state, InEvent::TimerExpired(current), now);
        assert!(disconnects(&out), "the request did not time out: {out:?}");
    }

    /// Returns the IO of `topic` over `conns` and `outbox`.
    fn topic_io<'a>(
        topic: TopicId,
        conns: &'a mut ConnsMap<u32>,
        outbox: &'a mut Vec<OutEvent<u32>>,
    ) -> TopicIO<'a, u32, Vec<OutEvent<u32>>> {
        TopicIO {
            topic,
            instance: 1,
            peer_topics: conns,
            io: outbox,
        }
    }

    /// Leaving one topic must not disconnect a peer another topic still uses.
    #[test]
    fn disconnect_peer_waits_for_the_last_topic() {
        use topic::IO as _;
        let topic_a: TopicId = [1u8; 32].into();
        let topic_b: TopicId = [2u8; 32].into();
        let peer = 1u32;
        let mut conns = ConnsMap::from([(peer, HashSet::from([topic_a, topic_b]))]);
        let mut outbox = Vec::new();

        topic_io(topic_a, &mut conns, &mut outbox).disconnect(peer);
        assert!(
            outbox.is_empty(),
            "disconnected while topic_b still uses the peer"
        );

        topic_io(topic_b, &mut conns, &mut outbox).disconnect(peer);
        assert!(matches!(outbox[..], [OutEvent::DisconnectPeer(p)] if p == peer));
        assert!(!conns.contains_key(&peer));
    }

    /// A disconnect reaches the network layer for a peer we only sent to.
    ///
    /// A shuffle reply goes to a peer we never received from, and the topic
    /// disconnects it right after.
    #[test]
    fn disconnect_peer_after_sending_only() {
        use topic::IO as _;
        let topic: TopicId = [1u8; 32].into();
        let peer = 1u32;
        let mut conns = ConnsMap::default();
        let mut outbox = Vec::new();

        let reply = hyparview::Message::test_shuffle_reply_with(Vec::new());
        let mut io = topic_io(topic, &mut conns, &mut outbox);
        io.send(peer, topic::Message::Swarm(reply));
        io.disconnect(peer);

        assert!(
            matches!(outbox[..], [OutEvent::SendMessage(..), OutEvent::DisconnectPeer(p)] if p == peer),
            "{outbox:?}"
        );
        assert!(!conns.contains_key(&peer));
    }

    /// A closed connection ends the use of the peer by every topic.
    ///
    /// A join whose dial fails gets no `DisconnectPeer` from the topic, as the
    /// peer never was a neighbor. Only the close clears its entry.
    #[test]
    fn peer_disconnected_prunes_a_peer_we_only_sent_to() {
        let mut state = State::new(
            0u32,
            PeerData::default(),
            Config::default(),
            StdRng::seed_from_u64(1),
        );
        let topic: TopicId = [0u8; 32].into();
        handle(&mut state, InEvent::Command(topic, Command::Join(vec![1])));
        assert!(state.peer_topics.contains_key(&1));

        handle(&mut state, InEvent::PeerDisconnected(1));

        assert!(state.peer_topics.is_empty());
    }
}
