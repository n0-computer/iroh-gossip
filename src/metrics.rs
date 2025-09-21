//! Metrics for iroh-gossip

use iroh_metrics::{Counter, MetricsGroup};
use serde::Serialize;

use crate::proto::{
    self,
    state::MessageKind,
    topic::{InEvent, OutEvent},
};

/// Enum of metrics for the module
#[derive(Debug, Default, MetricsGroup)]
#[metrics(name = "gossip")]
pub struct Metrics {
    /// Number of control messages sent
    pub msgs_ctrl_sent: Counter,
    /// Number of control messages received
    pub msgs_ctrl_recv: Counter,
    /// Number of data messages sent
    pub msgs_data_sent: Counter,
    /// Number of data messages received
    pub msgs_data_recv: Counter,
    /// Total size of all data messages sent
    pub msgs_data_sent_size: Counter,
    /// Total size of all data messages received
    pub msgs_data_recv_size: Counter,
    /// Total size of all control messages sent
    pub msgs_ctrl_sent_size: Counter,
    /// Total size of all control messages received
    pub msgs_ctrl_recv_size: Counter,
    /// Number of times we connected to a peer
    pub neighbor_up: Counter,
    /// Number of times we disconnected from a peer
    pub neighbor_down: Counter,
    /// Number of messages we broadcasted to all nodes
    pub msgs_broadcast_swarm: Counter,
    /// Number of messages we broadcasted to direct neighbors
    pub msgs_broadcast_neighbors: Counter,
    /// Number of topcis we joined.
    pub topics_joined: Counter,
    /// Number of topcis we left.
    pub topics_quit: Counter,
    /// Number of times we successfully dialed a remote node.
    pub peers_dialed_success: Counter,
    /// Number of times we failed to dial a remote node.
    pub peers_dialed_failure: Counter,
    /// Number of times we accepted a connection from a remote node.
    pub peers_accepted: Counter,
    /// Number of times the main actor loop ticked
    pub actor_tick_main: Counter,
}

impl Metrics {
    /// Track an [`InEvent`].
    pub fn track_in_event<PI: Serialize>(&self, in_event: &InEvent<PI>) {
        match in_event {
            InEvent::RecvMessage(_, message) => match message.kind() {
                MessageKind::Data => {
                    self.msgs_data_recv.inc();
                    self.msgs_data_recv_size
                        .inc_by(message.size().unwrap_or(0) as u64);
                }
                MessageKind::Control => {
                    self.msgs_ctrl_recv.inc();
                    self.msgs_ctrl_recv_size
                        .inc_by(message.size().unwrap_or(0) as u64);
                }
            },
            InEvent::Command(cmd) => match cmd {
                proto::Command::Broadcast(_, scope) => match scope {
                    proto::Scope::Swarm => inc(&self.msgs_broadcast_swarm),
                    proto::Scope::Neighbors => inc(&self.msgs_broadcast_neighbors),
                },
                proto::Command::Join(_) => {}
                proto::Command::Quit => {}
            },
            InEvent::TimerExpired(_) => {}
            InEvent::PeerDisconnected(_) => {}
            InEvent::UpdatePeerData(_) => {}
        }
    }

    /// Track an [`OutEvent`].
    pub fn track_out_event<PI: Serialize>(&self, out_event: &OutEvent<PI>) {
        match out_event {
            OutEvent::SendMessage(_to, message) => match message.kind() {
                MessageKind::Data => {
                    self.msgs_data_sent.inc();
                    self.msgs_data_sent_size
                        .inc_by(message.size().unwrap_or(0) as u64);
                }
                MessageKind::Control => {
                    self.msgs_ctrl_sent.inc();
                    self.msgs_ctrl_sent_size
                        .inc_by(message.size().unwrap_or(0) as u64);
                }
            },
            OutEvent::EmitEvent(event) => match event {
                proto::Event::NeighborUp(_peer) => inc(&self.neighbor_up),
                proto::Event::NeighborDown(_peer) => inc(&self.neighbor_down),
                _ => {}
            },
            _ => {}
        }
    }
}

pub(crate) fn inc(counter: &Counter) {
    counter.inc();
}
