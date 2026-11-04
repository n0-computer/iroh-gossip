//! The state of each peer, and the dials to peers.

use std::{collections::HashSet, time::Duration};

use iroh::endpoint::Side;
use n0_future::time::Instant;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::{
    queue::{Pushed, QueueLease, SendQueue},
    ProtoMessage,
};
use crate::proto::TopicId;

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
    /// Whether the peer opened a stream, which shows that it handles a close.
    ///
    /// It belongs to the peer, not to one connection. So a connection on which
    /// neither side sends closes too, as after two dials cross.
    pub(super) handles_close: watch::Sender<bool>,
    /// The topics that may have an open stream to this peer.
    ///
    /// It holds only topics that still want the peer. When a topic stops
    /// wanting the peer, the actor removes it, and tells the writer to close
    /// its stream.
    open_topic_streams: HashSet<TopicId>,
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
            open_topic_streams: HashSet::new(),
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
        let len = super::wire::message_len(&message.message).ok();
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

    /// Records that `topic` may have an open stream to this peer, and returns whether it is new.
    pub(super) fn add_topic_stream(&mut self, topic: TopicId) -> bool {
        self.open_topic_streams.insert(topic)
    }

    /// Records the initial topics of a new writer, for which it opens streams.
    pub(super) fn add_topic_streams(&mut self, topics: &[TopicId]) {
        self.open_topic_streams.extend(topics.iter().copied());
    }

    /// Queues the end of the stream of each topic for which `wanted` is false.
    ///
    /// Otherwise the stream stays open while another topic uses the peer, and
    /// takes one of its stream slots (#223). This also runs on a closed queue,
    /// as a topic can open it again before the writer drained it.
    pub(super) fn finish_unwanted_streams(&mut self, wanted: impl Fn(&TopicId) -> bool) {
        let queue = &self.queue;
        self.open_topic_streams.retain(|topic| {
            let keep = wanted(topic);
            if !keep {
                queue.finish_stream(*topic);
            }
            keep
        });
    }

    /// Queues the end of the stream of `topic`, unless done since its last message.
    pub(super) fn finish_topic_stream(&mut self, topic: TopicId) {
        if self.open_topic_streams.remove(&topic) {
            self.queue.finish_stream(topic);
        }
    }

    /// Puts a closed, empty queue in place of the queue.
    ///
    /// A writer of the old queue still sends what it holds, and then ends. A
    /// later abort still closes its connection.
    pub(super) fn discard_queue(&mut self, now: Instant) {
        self.queue = SendQueue::new(self.queue.size());
        self.queue.close();
        self.open_topic_streams.clear();
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

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;
    use crate::{
        net::queue::{QueueItem, QueueReader, MIN_QUEUE_SIZE},
        proto::topic::Message,
    };

    fn topic(name: &str) -> TopicId {
        blake3::hash(name.as_bytes()).into()
    }

    fn data(topic: TopicId, len: usize) -> ProtoMessage {
        ProtoMessage {
            topic,
            message: Message::test_payload(Bytes::from(vec![7u8; len])),
        }
    }

    /// Takes the items that the queue holds, as the topic and whether it is a `Finish`.
    async fn drain(reader: &mut QueueReader, count: usize) -> Vec<(TopicId, bool)> {
        let mut items = Vec::new();
        for _ in 0..count {
            let next = tokio::time::timeout(Duration::from_secs(1), reader.wait_next());
            assert!(
                next.await.expect("the queue holds fewer items"),
                "the queue ended"
            );
            items.push(match reader.current().expect("an item") {
                QueueItem::Message(item) => (item.message.topic, false),
                QueueItem::Finish(topic) => (*topic, true),
            });
            reader.advance();
        }
        assert!(reader.is_empty(), "the queue holds more items");
        items
    }

    /// A closed queue still gets the `Finish` of an unwanted topic, as a topic can open it again before it drains.
    #[tokio::test]
    async fn closed_queue_still_finishes_unwanted_topics() {
        let (left, other) = (topic("left"), topic("other"));
        let mut peer = PeerState::new(MIN_QUEUE_SIZE);
        assert!(matches!(peer.send(data(left, 10)), Pushed::Queued));
        assert!(peer.add_topic_stream(left));
        // No topic wants the peer, so the actor closes the queue.
        peer.queue.close();
        peer.finish_unwanted_streams(|_| false);
        assert!(peer.queue.is_closed(), "a finish opened the queue");
        assert!(matches!(peer.send(data(other, 10)), Pushed::Queued));
        let mut reader = peer.queue.try_reader().expect("no writer reads the queue");
        let items = drain(&mut reader, 3).await;
        assert_eq!(items, vec![(left, false), (left, true), (other, false)]);
    }
}
