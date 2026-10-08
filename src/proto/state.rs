//! The protocol state of the `iroh-gossip` protocol.

use std::collections::{hash_map, HashMap, HashSet};

use n0_future::time::{Duration, Instant};
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use tracing::trace;

use crate::{
    metrics::Metrics,
    proto::{
        topic::{self, Command},
        util::idbytes_impls,
        Config, PeerData, PeerIdentity, MIN_MAX_MESSAGE_SIZE,
    },
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
}

impl<PI: Serialize> Message<PI> {
    pub(crate) fn postcard_header_size() -> usize {
        // We create a message that has no payload (gossip::Message::Prune), calculate the encoded size,
        // and subtract 1 for the discriminator of the inner gossip::Message enum.
        let m = Self {
            topic: TopicId(Default::default()),
            message: topic::Message::<PI>::Gossip(super::plumtree::Message::Prune),
        };
        postcard::experimental::serialized_size(&m).unwrap() - 1
    }
}

/// Whether this is a control or data message
#[derive(Debug)]
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

/// A timer to be registered into the runtime
///
/// As the implementation of the protocol is an IO-less state machine, registering timers does not
/// happen within the protocol implementation. Instead, these `Timer` structs are emitted as
/// [`OutEvent`]s. The implementer must register the timer in its runtime to be emitted on the specified [`Instant`],
/// and once triggered inject an [`InEvent::TimerExpired`] into the protocol state.
#[derive(Clone, Debug)]
pub struct Timer<PI> {
    topic: TopicId,
    timer: topic::Timer<PI>,
}

/// Input event to the protocol state.
#[derive(Clone, Debug)]
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

/// Output event from the protocol state.
#[derive(Debug, Clone)]
pub enum OutEvent<PI> {
    /// Send a message on the network
    SendMessage(PI, Message<PI>),
    /// Emit an event to the application.
    EmitEvent(TopicId, topic::Event<PI>),
    /// Schedule a timer. The runtime is responsible for sending an [InEvent::TimerExpired]
    /// after the duration.
    ScheduleTimer(Duration, Timer<PI>),
    /// Close the connection to a peer on the network level.
    DisconnectPeer(PI),
    /// Updated peer data
    PeerData(PI, PeerData),
}

type ConnsMap<PI> = HashMap<PI, HashSet<TopicId>>;
type Outbox<PI> = Vec<OutEvent<PI>>;

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
            InEvent::TimerExpired(Timer { topic, timer }) => {
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
/// [`Self::handle`], which returns an iterator of [`OutEvent`]s to be processed.
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
    outbox: Outbox<PI>,
    peer_topics: ConnsMap<PI>,
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
            outbox: Default::default(),
            peer_topics: Default::default(),
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

    /// Handle an [`InEvent`]
    ///
    /// This returns an iterator of [`OutEvent`]s that must be processed.
    pub fn handle(
        &mut self,
        event: InEvent<PI>,
        now: Instant,
        metrics: Option<&Metrics>,
    ) -> impl Iterator<Item = OutEvent<PI>> + '_ + use<'_, PI, R> {
        trace!("in : {event:?}");
        if let Some(metrics) = &metrics {
            track_in_event(&event, metrics);
        }

        let event: InEventMapped<PI> = event.into();

        match event {
            InEventMapped::TopicEvent(topic, event) => {
                // when receiving a join command, initialize state if it doesn't exist
                if matches!(&event, topic::InEvent::Command(Command::Join(_peers))) {
                    if let hash_map::Entry::Vacant(e) = self.states.entry(topic) {
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
                    // when receiving messages, update our conn map to take note that this topic state may want
                    // to keep this connection
                    if let topic::InEvent::RecvMessage(from, _message) = &event {
                        self.peer_topics.entry(*from).or_default().insert(topic);
                    }
                    let out = state.handle(event, now);
                    for event in out {
                        handle_out_event(topic, event, &mut self.peer_topics, &mut self.outbox);
                    }
                }

                if quit {
                    self.states.remove(&topic);
                    // The topic uses no peer anymore. A peer it only sent to, such
                    // as the contact of a join with no answer, has no other way out.
                    let mut unused = Vec::new();
                    for (peer, topics) in self.peer_topics.iter_mut() {
                        if topics.remove(&topic) && topics.is_empty() {
                            unused.push(*peer);
                        }
                    }
                    for peer in unused {
                        self.peer_topics.remove(&peer);
                        self.outbox.push(OutEvent::DisconnectPeer(peer));
                    }
                }
            }
            // when a peer disconnected on the network level, forward event to all states
            InEventMapped::All(event) => {
                if let topic::InEvent::UpdatePeerData(data) = &event {
                    self.me_data = data.clone();
                }
                let disconnected = match &event {
                    topic::InEvent::PeerDisconnected(peer) => Some(*peer),
                    _ => None,
                };
                // Clear the peer's `peer_topics` entry first, so that a retried join
                // below adds it again. A peer that only relayed a shuffle or forward
                // join to us has one too.
                if let Some(peer) = disconnected {
                    self.peer_topics.remove(&peer);
                }
                for (topic, state) in self.states.iter_mut() {
                    let out = state.handle(event.clone(), now);
                    for event in out {
                        // The network layer dropped the peer already. A disconnect
                        // would drop the join that another topic retries to it in
                        // this batch.
                        let gone = matches!(
                            event,
                            topic::OutEvent::DisconnectPeer(peer) if Some(peer) == disconnected
                        );
                        if gone {
                            continue;
                        }
                        handle_out_event(*topic, event, &mut self.peer_topics, &mut self.outbox);
                    }
                }
            }
        }

        // track metrics
        if let Some(metrics) = &metrics {
            track_out_events(&self.outbox, metrics);
        }

        self.outbox.drain(..)
    }
}

fn handle_out_event<PI: PeerIdentity>(
    topic: TopicId,
    event: topic::OutEvent<PI>,
    conns: &mut ConnsMap<PI>,
    outbox: &mut Outbox<PI>,
) {
    trace!("out: {event:?}");
    match event {
        topic::OutEvent::SendMessage(to, message) => {
            // A topic uses a peer from its first message to it. Else the
            // disconnect of a peer that a topic only sent to finds no entry. Only
            // HyParView sends to a peer that is not a neighbor yet.
            if matches!(message, topic::Message::Swarm(_)) {
                conns.entry(to).or_default().insert(topic);
            }
            outbox.push(OutEvent::SendMessage(to, Message { topic, message }))
        }
        topic::OutEvent::EmitEvent(event) => outbox.push(OutEvent::EmitEvent(topic, event)),
        topic::OutEvent::ScheduleTimer(delay, timer) => {
            outbox.push(OutEvent::ScheduleTimer(delay, Timer { topic, timer }))
        }
        topic::OutEvent::DisconnectPeer(peer) => {
            // The connection is shared by every topic that uses the peer.
            if let Some(topics) = conns.get_mut(&peer) {
                topics.remove(&topic);
                if topics.is_empty() {
                    // If the peer is no longer used by any topic, disconnect.
                    conns.remove(&peer);
                    outbox.push(OutEvent::DisconnectPeer(peer));
                }
            }
        }
        topic::OutEvent::PeerData(peer, data) => outbox.push(OutEvent::PeerData(peer, data)),
    }
}

fn track_out_events<PI: Serialize>(events: &[OutEvent<PI>], metrics: &Metrics) {
    for event in events {
        match event {
            OutEvent::SendMessage(_to, message) => match message.kind() {
                MessageKind::Data => {
                    metrics.msgs_data_sent.inc();
                    metrics
                        .msgs_data_sent_size
                        .inc_by(message.size().unwrap_or(0) as u64);
                }
                MessageKind::Control => {
                    metrics.msgs_ctrl_sent.inc();
                    metrics
                        .msgs_ctrl_sent_size
                        .inc_by(message.size().unwrap_or(0) as u64);
                }
            },
            OutEvent::EmitEvent(_topic, event) => match event {
                super::Event::NeighborUp(_peer) => {
                    metrics.neighbor_up.inc();
                }
                super::Event::NeighborDown(_peer) => {
                    metrics.neighbor_down.inc();
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn track_in_event<PI: Serialize>(event: &InEvent<PI>, metrics: &Metrics) {
    if let InEvent::RecvMessage(_from, message) = event {
        match message.kind() {
            MessageKind::Data => {
                metrics.msgs_data_recv.inc();
                metrics
                    .msgs_data_recv_size
                    .inc_by(message.size().unwrap_or(0) as u64);
            }
            MessageKind::Control => {
                metrics.msgs_ctrl_recv.inc();
                metrics
                    .msgs_ctrl_recv_size
                    .inc_by(message.size().unwrap_or(0) as u64);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::rngs::StdRng;

    use super::*;
    use crate::proto::{hyparview, plumtree};

    /// Handles `event` and drops what it produces.
    fn handle(state: &mut State<u32, StdRng>, event: InEvent<u32>) {
        state.handle(event, Instant::now(), None).for_each(drop);
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

        let out: Vec<_> = state
            .handle(InEvent::Command(topic, Command::Quit), Instant::now(), None)
            .collect();

        assert!(
            out.iter()
                .any(|event| matches!(event, OutEvent::DisconnectPeer(1))),
            "{out:?}"
        );
        assert!(state.peer_topics.is_empty());
    }

    /// A peer that never became a neighbor is removed from `peer_topics`.
    ///
    /// Such a peer only relayed a message to us. No topic disconnects it, so
    /// nothing else removes the entry. We only ever see such a peer's messages
    /// for topics we joined, so the test joins the topic first.
    #[test]
    fn peer_disconnected_prunes_peer_topics() {
        let mut state = State::new(
            0u32,
            PeerData::default(),
            Config::default(),
            StdRng::seed_from_u64(1),
        );
        let topic: TopicId = [0u8; 32].into();
        let peer = 1u32;
        handle(&mut state, InEvent::Command(topic, Command::Join(vec![])));
        let message = Message {
            topic,
            message: topic::Message::Gossip(plumtree::Message::Prune),
        };
        handle(&mut state, InEvent::RecvMessage(peer, message));
        assert!(state.peer_topics.contains_key(&peer));

        handle(&mut state, InEvent::PeerDisconnected(peer));

        assert!(!state.peer_topics.contains_key(&peer));
    }

    /// A closed connection must not make a topic disconnect the peer.
    ///
    /// The network layer dropped the peer before it raised the event. A topic
    /// that held the peer as neighbor still asks to disconnect it, while another
    /// topic may retry its join to the peer in the same batch. The disconnect
    /// would drop that retry. The state handles topics in a random order, so
    /// the test runs several times.
    #[test]
    fn peer_disconnected_sends_no_disconnect() {
        let topic_a: TopicId = [1u8; 32].into();
        let topic_b: TopicId = [2u8; 32].into();
        let peer = 1u32;
        for seed in 0..16 {
            let mut state = State::new(
                0u32,
                PeerData::default(),
                Config::default(),
                StdRng::seed_from_u64(seed),
            );
            // The peer is our neighbor in `topic_a`: it joined us there.
            handle(&mut state, InEvent::Command(topic_a, Command::Join(vec![])));
            let join = Message {
                topic: topic_a,
                message: topic::Message::Swarm(hyparview::Message::Join(None)),
            };
            handle(&mut state, InEvent::RecvMessage(peer, join));
            // In `topic_b`, our join to the peer is pending.
            handle(
                &mut state,
                InEvent::Command(topic_b, Command::Join(vec![peer])),
            );

            let out: Vec<_> = state
                .handle(InEvent::PeerDisconnected(peer), Instant::now(), None)
                .collect();

            assert!(
                !out.iter()
                    .any(|e| matches!(e, OutEvent::DisconnectPeer(p) if *p == peer)),
                "the gone peer was disconnected (seed {seed})"
            );
            assert!(
                out.iter()
                    .any(|e| matches!(e, OutEvent::SendMessage(to, _) if *to == peer)),
                "the join was not retried (seed {seed})"
            );
            // Only the retry in `topic_b` uses the peer now.
            assert_eq!(
                state.peer_topics.get(&peer),
                Some(&HashSet::from([topic_b]))
            );
        }
    }

    /// A join retried after a closed connection disconnects the peer when it gives up.
    ///
    /// The retry sends to the peer again, so the topic uses it again. If the
    /// close cleared the topic's entry after the retry, the disconnect at the
    /// end would find no entry and the connection would stay open.
    #[test]
    fn join_retried_after_a_close_disconnects_when_it_gives_up() {
        let mut state = State::new(
            0u32,
            PeerData::default(),
            Config::default(),
            StdRng::seed_from_u64(1),
        );
        let topic: TopicId = [0u8; 32].into();
        let peer = 1u32;
        handle(
            &mut state,
            InEvent::Command(topic, Command::Join(vec![peer])),
        );

        let mut out: Vec<_> = state
            .handle(InEvent::PeerDisconnected(peer), Instant::now(), None)
            .collect();
        assert!(
            state.peer_topics.contains_key(&peer),
            "the retry is not tracked"
        );
        for _ in 0..8 {
            if out
                .iter()
                .any(|e| matches!(e, OutEvent::DisconnectPeer(p) if *p == peer))
            {
                return;
            }
            let timer = out.iter().find_map(|e| match e {
                OutEvent::ScheduleTimer(_, timer) => Some(timer.clone()),
                _ => None,
            });
            let timer = timer.expect("no join timer left, and no disconnect");
            out = state
                .handle(InEvent::TimerExpired(timer), Instant::now(), None)
                .collect();
        }
        panic!("the join never gave up");
    }

    /// Leaving one topic must not disconnect a peer another topic still uses.
    #[test]
    fn disconnect_peer_waits_for_the_last_topic() {
        let topic_a: TopicId = [1u8; 32].into();
        let topic_b: TopicId = [2u8; 32].into();
        let peer = 1u32;
        let mut conns = ConnsMap::from([(peer, HashSet::from([topic_a, topic_b]))]);
        let mut outbox = Outbox::new();

        let event = topic::OutEvent::DisconnectPeer(peer);
        handle_out_event(topic_a, event, &mut conns, &mut outbox);
        assert!(
            outbox.is_empty(),
            "disconnected while topic_b still uses the peer"
        );

        let event = topic::OutEvent::DisconnectPeer(peer);
        handle_out_event(topic_b, event, &mut conns, &mut outbox);
        assert!(matches!(outbox[..], [OutEvent::DisconnectPeer(p)] if p == peer));
        assert!(!conns.contains_key(&peer));
    }

    /// A disconnect reaches the network layer for a peer we only sent to.
    ///
    /// A shuffle reply goes to a peer we never received from, and the topic
    /// disconnects it right after.
    #[test]
    fn disconnect_peer_after_sending_only() {
        let topic: TopicId = [1u8; 32].into();
        let peer = 1u32;
        let mut conns = ConnsMap::default();
        let mut outbox = Outbox::new();

        let reply = hyparview::Message::test_shuffle_reply_with(Vec::new());
        let message = topic::Message::Swarm(reply);
        handle_out_event(
            topic,
            topic::OutEvent::SendMessage(peer, message),
            &mut conns,
            &mut outbox,
        );
        handle_out_event(
            topic,
            topic::OutEvent::DisconnectPeer(peer),
            &mut conns,
            &mut outbox,
        );

        assert!(
            matches!(outbox[..], [OutEvent::SendMessage(..), OutEvent::DisconnectPeer(p)] if p == peer),
            "{outbox:?}"
        );
        assert!(!conns.contains_key(&peer));
    }
}
