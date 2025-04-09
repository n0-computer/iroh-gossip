//! The protocol state of the `iroh-gossip` protocol.

use std::collections::{hash_map, HashMap, HashSet};

use n0_future::time::{Duration, Instant};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tracing::trace;

use crate::{
    metrics::Metrics,
    proto::{
        topic::{self, Command},
        util::idbytes_impls,
        Config, PeerData, PeerIdentity,
    },
};

/// The identifier for a topic
#[derive(Clone, Copy, Eq, PartialEq, Hash, Serialize, Ord, PartialOrd, Deserialize)]
pub struct TopicId([u8; 32]);
idbytes_impls!(TopicId, "TopicId");

/// Protocol wire message
///
/// This is the wire frame of the `iroh-gossip` protocol.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message<PI> {
    topic: TopicId,
    message: topic::Message<PI>,
}

impl<PI> Message<PI> {
    /// Get the kind of this message
    pub fn kind(&self) -> MessageKind {
        self.message.kind()
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

impl<PI: PeerIdentity, R: Rng + Clone> State<PI, R> {
    /// Create a new protocol state instance.
    ///
    /// `me` is the [`PeerIdentity`] of the local node, `peer_data` is the initial [`PeerData`]
    /// (which can be updated over time).
    /// For the protocol to perform as recommended in the papers, the [`Config`] should be
    /// identical for all nodes in the network.
    pub fn new(me: PI, me_data: PeerData, config: Config, rng: R) -> Self {
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

    /// Get a reference to the protocol state for a topic.
    #[cfg(test)]
    pub fn state_mut(&mut self, topic: &TopicId) -> Option<&mut topic::State<PI, R>> {
        self.states.get_mut(topic)
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
    ) -> impl Iterator<Item = OutEvent<PI>> + '_ {
        trace!("in_event: {event:?}");
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
                            self.rng.clone(),
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
                }
            }
            // when a peer disconnected on the network level, forward event to all states
            InEventMapped::All(event) => {
                if let topic::InEvent::UpdatePeerData(data) = &event {
                    self.me_data = data.clone();
                }
                for (topic, state) in self.states.iter_mut() {
                    let out = state.handle(event.clone(), now);
                    for event in out {
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
    trace!("out_event: {event:?}");
    match event {
        topic::OutEvent::SendMessage(to, message) => {
            outbox.push(OutEvent::SendMessage(to, Message { topic, message }))
        }
        topic::OutEvent::EmitEvent(event) => outbox.push(OutEvent::EmitEvent(topic, event)),
        topic::OutEvent::ScheduleTimer(delay, timer) => {
            outbox.push(OutEvent::ScheduleTimer(delay, Timer { topic, timer }))
        }
        topic::OutEvent::DisconnectPeer(peer) => {
            let empty = conns
                .get_mut(&peer)
                .map(|list| list.remove(&topic) && list.is_empty())
                .unwrap_or(false);
            if empty {
                conns.remove(&peer);
                outbox.push(OutEvent::DisconnectPeer(peer));
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
