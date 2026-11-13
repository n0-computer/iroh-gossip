//! The state of each peer, and the dials to peers.

use std::time::Duration;

use iroh::endpoint::Side;
use n0_future::time::Instant;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::{
    queue::{Pushed, QueueLease, SendQueue},
    ProtoMessage,
};

/// How long we keep the state of an idle peer.
///
/// A dial that completes late still finds the `handles_close` flag of the peer.
// TODO(frando): Remove the retention once the wire format no longer needs the flag.
pub(super) const PEER_RETENTION: Duration = Duration::from_secs(20);

/// The [`Connection::stable_id`] of a connection.
///
/// [`Connection::stable_id`]: iroh::endpoint::Connection::stable_id
pub(super) type ConnId = usize;

/// What we know of a peer: its queue, and its connections.
///
/// The queue always exists. When it is closed, the writer sends what is left,
/// finishes its streams, and waits on its connection until the queue opens
/// again.
///
/// The state outlives the last connection by [`PEER_RETENTION`], so that a
/// peer we want again keeps its `handles_close` flag.
#[derive(Debug)]
pub(super) struct PeerState {
    pub(super) queue: SendQueue,
    /// Closes every connection of the peer when cancelled, also the receive-only ones.
    ///
    /// An abort cancels it, and puts a new token in its place for later connections.
    pub(super) aborted: CancellationToken,
    /// The connection that writes the queue, or `None` until a dial takes it.
    ///
    /// A replaced writer finishes its current message first. The queue lease
    /// keeps the two in order.
    pub(super) writer: Option<ConnId>,
    /// Connection tasks that the actor has not yet seen finish.
    pub(super) connections: usize,
    /// When [`Actor::forget_peers`] may remove the idle peer.
    ///
    /// [`Actor::forget_peers`]: super::actor::Actor::forget_peers
    forget_at: Option<Instant>,
    /// Whether the peer sent [`StreamHeader::handles_close`].
    ///
    /// It belongs to the peer, not to one connection. So a connection on which
    /// neither side sends closes too, as after two dials cross.
    ///
    /// [`StreamHeader::handles_close`]: super::wire::StreamHeader::handles_close
    pub(super) handles_close: watch::Sender<bool>,
}

impl PeerState {
    /// Creates the state of a peer with a closed, empty queue.
    ///
    /// A message that we send, or an accepted connection, opens the queue. A
    /// dial that completes after we forgot the peer thus has nothing to write.
    pub(super) fn new(queue_size: usize) -> Self {
        let aborted = CancellationToken::new();
        let queue = SendQueue::new(queue_size);
        queue.close();
        Self {
            queue,
            aborted,
            writer: None,
            connections: 0,
            forget_at: None,
            handles_close: watch::channel(false).0,
        }
    }

    /// Returns whether this peer needs a new dial because it has an open queue but no writer.
    pub(super) fn needs_dial(&self) -> bool {
        self.writer.is_none() && !self.queue.is_closed()
    }

    /// Opens the queue, which keeps its messages and its byte limit.
    pub(super) fn open_queue(&mut self) -> &SendQueue {
        self.forget_at = None;
        self.queue.open();
        &self.queue
    }

    /// Like [`Self::push`], and computes the encoded size of the message.
    #[cfg(test)]
    pub(super) fn send(&mut self, message: ProtoMessage) -> Pushed {
        let len = postcard::experimental::serialized_size(&message.message).ok();
        self.push(message, len)
    }

    /// Queues a message without waiting, and opens the queue.
    ///
    /// `len` is the encoded size of the message if it has one. Waiting for room
    /// would stop the actor, and with it every topic, for as long as this one
    /// peer does not read. On [`Pushed::Overflow`], the caller aborts the peer.
    pub(super) fn push(&mut self, message: ProtoMessage, len: Option<usize>) -> Pushed {
        self.open_queue().push(message, len)
    }

    /// Puts a closed, empty queue in place of the queue.
    ///
    /// A writer of the old queue still sends what it holds, and then ends. A
    /// later abort still closes its connection.
    pub(super) fn discard_queue(&mut self, now: Instant) {
        self.queue = SendQueue::new(self.queue.size());
        self.queue.close();
        self.writer = None;
        self.retain_if_idle(now);
    }

    /// Discards the queue, and closes every connection of the peer.
    ///
    /// The caller aborts a peer when a control message does not fit into its
    /// queue. If we dropped only that message and sent the ones after it, the
    /// peer would act on the later messages without the dropped one. Its view
    /// of us would then differ from our view of it, and neither side would
    /// notice. So no writer may send the rest of the queue, not even a writer
    /// that a handover replaced. Instead, both sides see a disconnect and start
    /// over. Connections that only receive close too, so that the peer learns
    /// of the abort on the connection that it writes on.
    ///
    /// A retirement can also lose a message, but it does not cause this problem.
    /// The lost message is the last one on its connection, so the peer does not
    /// act on later messages without it. It also starts a new exchange with the
    /// peer, and that exchange has its own timeout or retry.
    pub(super) fn abort(&mut self, now: Instant) {
        self.aborted.cancel();
        self.aborted = CancellationToken::new();
        self.discard_queue(now);
    }

    /// Counts a new connection, and returns its lease if it writes the queue.
    pub(super) fn handle_connection(&mut self, side: Side, id: ConnId) -> Option<QueueLease> {
        self.connections += 1;
        self.forget_at = None;
        // A dial writes only if no connection does. An accepted one always takes over.
        if side == Side::Client && self.writer.is_some() {
            None
        } else {
            self.writer = Some(id);
            // An accepted connection reopens a closed queue. A dial sends what is
            // left of it, and then waits.
            if side == Side::Server {
                self.queue.open();
            }
            Some(self.queue.lease())
        }
    }

    /// Counts a finished connection, and returns what the caller does next.
    ///
    /// When the writer of an open queue ended, the caller reports a disconnect,
    /// even if other connections remain. It must process `PeerDisconnected` at
    /// once, which discards the queue before invoking the protocol. A closed
    /// queue is discarded here instead, as no topic uses the peer. A `retired`
    /// writer reports no disconnect, because the link still works: an empty
    /// queue goes, and the caller dials for the rest.
    pub(super) fn connection_finished(
        &mut self,
        id: ConnId,
        now: Instant,
        retired: bool,
    ) -> Finished {
        self.connections = self
            .connections
            .checked_sub(1)
            .expect("finished connection must have been registered");
        if self.writer != Some(id) {
            self.retain_if_idle(now);
            Finished::Quiet
        } else if self.queue.is_closed() || (retired && self.queue.is_empty()) {
            self.discard_queue(now);
            Finished::Quiet
        } else if retired {
            self.writer = None;
            Finished::Redial
        } else {
            Finished::Disconnected
        }
    }

    /// Starts retention once the queue is closed and empty, and all connections finished.
    ///
    /// An open queue still needs a writer, even when empty. A later call does not extend retention.
    fn retain_if_idle(&mut self, now: Instant) {
        if self.queue.is_closed() && self.queue.is_empty() && self.connections == 0 {
            self.forget_at.get_or_insert(now + PEER_RETENTION);
        }
    }

    /// Whether the retention of the peer ended at `now`.
    pub(super) fn can_forget(&self, now: Instant) -> bool {
        self.forget_at.is_some_and(|deadline| deadline <= now)
    }
}

/// What the actor does when a connection of a peer finished.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Finished {
    /// Nothing.
    Quiet,
    /// Report that the peer disconnected.
    Disconnected,
    /// Dial the peer again, for the messages that a retired writer left.
    Redial,
}
