//! The queue of messages to one peer, bounded in bytes.
//!
//! All messages to the peer share one FIFO, so we write them in the order that
//! the protocol sent them. Data and announcements may each fill only a section of the queue, which
//! keeps room for the other control messages.
//!
//! The size bounds the bytes that wait to be sent, not how long they wait: a
//! peer that reads slowly can keep a message waiting for long. It is not an
//! exact bound on memory either: the queues to all peers and the cache of Plumtree
//! share the content of a broadcast, and QUIC buffers more.

use std::{
    mem,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use tokio::sync::{mpsc, watch, OwnedMutexGuard};
use tokio_util::sync::CancellationToken;

use super::ProtoMessage;
use crate::proto::state::MessageKind;

/// The bytes that a queued message takes on top of its encoded size.
///
/// It is the size of a [`QueuedMessage`] in memory, and the length prefix of
/// its frame. It also caps the number of messages in a queue.
const MESSAGE_OVERHEAD: usize = mem::size_of::<QueuedMessage>() + 4;

/// The least size of a queue, whatever its options.
pub(super) const MIN_QUEUE_SIZE: usize = 64 * 1024;

/// The size of a queue by default, see [`QueueOptions::size`](super::QueueOptions::size).
pub(super) const QUEUE_SIZE: usize = 1024 * 1024;

/// Returns the size of a queue for `size` of the options and `max_message_size`.
///
/// It fits at least eight messages of the largest size, so that the
/// announcements, an eighth of the queue, fit one.
pub(super) fn queue_size(size: usize, max_message_size: usize) -> usize {
    let least = max_message_size
        .saturating_add(MESSAGE_OVERHEAD)
        .saturating_mul(8);
    size.max(MIN_QUEUE_SIZE).max(least)
}

/// What [`SendQueue::push`] did with a message.
#[derive(Debug)]
pub(super) enum Pushed {
    /// The message was queued.
    Queued,
    /// The data or announcement message did not fit, so the peer loses it.
    Dropped(ProtoMessage),
    /// The queue had no room for a control message.
    Overflow,
}

/// A queued message, which keeps its reservation until it is dropped.
///
/// The writer drops it only after the write, so a message that waits for flow
/// control still counts.
#[derive(Debug)]
pub(super) struct QueuedMessage {
    pub(super) message: ProtoMessage,
    reservation: Reservation,
}

impl QueuedMessage {
    /// Returns the encoded size of the message, which the queue computed to reserve its room.
    pub(super) fn encoded_len(&self) -> usize {
        self.reservation.encoded_len as usize
    }
}

/// The section of a queue that a message counts against, besides the whole queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Control,
    Data,
    Announcements,
}

impl Section {
    fn of(message: &ProtoMessage) -> Self {
        match message.kind() {
            MessageKind::Data => Section::Data,
            MessageKind::Control if message.message.is_announcement() => Section::Announcements,
            MessageKind::Control => Section::Control,
        }
    }
}

/// The bytes in use of a queue and of its sections.
#[derive(Debug)]
struct Capacity {
    total: Limit,
    data: Limit,
    announcements: Limit,
}

impl Capacity {
    /// Returns the limit of `section`; control has none of its own.
    fn section(&self, section: Section) -> Option<&Limit> {
        match section {
            Section::Control => None,
            Section::Data => Some(&self.data),
            Section::Announcements => Some(&self.announcements),
        }
    }
}

/// A count of bytes in use, up to a maximum.
///
/// Nothing waits for room, so atomic counters do the work of a semaphore.
#[derive(Debug)]
struct Limit {
    used: AtomicUsize,
    max: usize,
}

impl Limit {
    fn new(max: usize) -> Self {
        Self {
            used: AtomicUsize::new(0),
            max,
        }
    }

    /// Takes `size` bytes, and returns `false` if they do not fit.
    fn reserve(&self, size: usize) -> bool {
        // The counter guards no other data, so relaxed ordering suffices. A loop of
        // its own, as `fetch_update` is deprecated on new and `try_update` missing on
        // old toolchains.
        let mut used = self.used.load(Ordering::Relaxed);
        loop {
            let Some(next) = used.checked_add(size).filter(|&next| next <= self.max) else {
                return false;
            };
            match self
                .used
                .compare_exchange_weak(used, next, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return true,
                Err(actual) => used = actual,
            }
        }
    }

    fn release(&self, size: usize) {
        self.used.fetch_sub(size, Ordering::Relaxed);
    }
}

/// The room of a queued message, which the message releases when it is dropped.
#[derive(Debug)]
struct Reservation {
    capacity: Arc<Capacity>,
    encoded_len: u32,
    section: Section,
}

impl Reservation {
    /// Reserves the room of `message` in the queue and in its section, if both have it.
    fn new(capacity: &Arc<Capacity>, message: &ProtoMessage, section: Section) -> Option<Self> {
        let encoded_len = postcard::experimental::serialized_size(&message.message).ok()?;
        let encoded_len = u32::try_from(encoded_len).ok()?;
        let size = Self::size_of(encoded_len);
        let limit = capacity.section(section);
        if !limit.is_none_or(|limit| limit.reserve(size)) {
            return None;
        }
        if !capacity.total.reserve(size) {
            limit.inspect(|limit| limit.release(size));
            return None;
        }
        Some(Self {
            capacity: capacity.clone(),
            encoded_len,
            section,
        })
    }

    /// Returns the room that a message of `encoded_len` bytes takes.
    fn size_of(encoded_len: u32) -> usize {
        encoded_len as usize + MESSAGE_OVERHEAD
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let size = Self::size_of(self.encoded_len);
        self.capacity.total.release(size);
        if let Some(limit) = self.capacity.section(self.section) {
            limit.release(size);
        }
    }
}

/// The queue of messages to a peer, which its connections take in turn.
///
/// It lives as long as we know the peer, also while we close it. A writer may
/// still drain an older queue for a short time after an abort, so the bytes
/// to a peer can briefly reach two queues.
///
/// The [`Capacity`] bounds the queue, so its channel needs no bound of its own.
/// Each message reserves at least [`MESSAGE_OVERHEAD`] bytes until the writer drops it.
///
/// # Design and limitations
///
/// - We never wait on a peer. A full queue drops data, and a control message
///   that does not fit aborts the peer. What was queued for an aborted peer is
///   lost.
/// - Each topic has one stream to a peer, and we write it in order. The peer
///   reads each stream on its own, so it may handle a `Join` before an
///   earlier `Disconnect`. The peer also limits its streams, to 100 by
///   default. Over that limit, an open waits
///   [`OPEN_TIMEOUT`](super::util::OPEN_TIMEOUT) and closes the connection,
///   again after each rejoin.
/// - A handover keeps the order: the new connection waits for the old one to
///   finish its message. A control overflow meanwhile aborts the peer, and
///   closes the new connection too.
/// - The queue bounds bytes, not time. It fills only once QUIC buffered all
///   it can for the peer, so a burst larger than both loses messages before
///   anyone waits. Under sustained overload, data at a peer on a 1 Mbit/s
///   link can arrive up to 30 to 40 s late, as it did in tests. Most of it
///   waits in the send buffer of QUIC, which the flow control window of the
///   peer sizes, not in our queue.
/// - Busy queues to all eager peers may mean that our uplink is the limit.
///   They may also mean that all eager peers are slow,
///   or that relayed traffic and other topics fill the queues. A waiting
///   broadcast passes after [`GATE_LIMIT`](super::GATE_LIMIT) in any case.
/// - Repair is capped, at 256 ids per peer, 4096 per topic, and 64 per peer
///   and second. It works only while the message is cached, and never for
///   neighbor scope.
/// - All topics share the queue to a peer. A busy topic crowds out another,
///   and control waits behind the data queued before it.
#[derive(Debug)]
pub(super) struct SendQueue {
    tx: mpsc::UnboundedSender<QueuedMessage>,
    capacity: Arc<Capacity>,
    closing: watch::Sender<bool>,
    reader: SharedReader,
    /// Revokes the lease of the connection that writes the queue now.
    revoke: CancellationToken,
    /// Aborts the peer: every writer of the queue closes its connection.
    aborted: CancellationToken,
}

impl SendQueue {
    /// Creates a queue of `size` bytes, of which data may fill 3/4 and announcements 1/8.
    pub(super) fn new(size: usize) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let capacity = Arc::new(Capacity {
            total: Limit::new(size),
            data: Limit::new(size / 4 * 3),
            announcements: Limit::new(size / 8),
        });
        let (closing, closing_rx) = watch::channel(false);
        let reader = QueueReader {
            rx,
            unfinished: None,
            closing: closing_rx,
        };
        let aborted = CancellationToken::new();
        Self {
            tx,
            capacity,
            closing,
            reader: Arc::new(tokio::sync::Mutex::new(reader)),
            revoke: aborted.child_token(),
            aborted,
        }
    }

    /// Queues `message` if it fits, and never waits.
    pub(super) fn push(&self, message: ProtoMessage) -> Pushed {
        let section = Section::of(&message);
        let Some(reservation) = Reservation::new(&self.capacity, &message, section) else {
            return match section {
                Section::Control => Pushed::Overflow,
                Section::Data | Section::Announcements => Pushed::Dropped(message),
            };
        };
        let item = QueuedMessage {
            message,
            reservation,
        };
        let sent = self.tx.send(item);
        debug_assert!(sent.is_ok(), "the queue keeps its reader");
        Pushed::Queued
    }

    /// Returns whether data fills at least half of its section.
    pub(super) fn is_busy(&self) -> bool {
        let data = &self.capacity.data;
        data.used.load(Ordering::Relaxed) >= data.max / 2
    }

    /// Sets whether the writer ends once the queue is empty, as it does for a closing peer.
    pub(super) fn set_closing(&self, closing: bool) {
        self.closing.send_replace(closing);
    }

    /// Revokes the lease of the connection that writes the queue, and returns one for the next.
    pub(super) fn lease(&mut self) -> QueueLease {
        self.revoke.cancel();
        self.revoke = self.aborted.child_token();
        QueueLease {
            reader: self.reader.clone(),
            revoked: self.revoke.clone(),
            aborted: self.aborted.clone(),
        }
    }

    /// Revokes every lease on the queue, and makes each writer close its connection.
    pub(super) fn abort(&self) {
        self.aborted.cancel();
    }

    /// Returns the reader of the queue, if no writer reads it.
    #[cfg(test)]
    pub(super) fn try_reader(&self) -> Option<OwnedMutexGuard<QueueReader>> {
        self.reader.clone().try_lock_owned().ok()
    }
}

/// The lease of a connection on the queue to its peer.
///
/// The actor revokes it when a new connection replaces this one. The writer
/// then lets go of the queue after its current message, and the writer of the
/// new connection takes it. When the actor aborts the peer, it cancels
/// `aborted`, which revokes every lease on the queue, and each writer closes
/// its connection.
#[derive(Debug)]
pub(super) struct QueueLease {
    reader: SharedReader,
    /// A child of `aborted`.
    revoked: CancellationToken,
    aborted: CancellationToken,
}

impl QueueLease {
    /// Waits until no other writer reads the queue, and returns its reader.
    ///
    /// Waits only at a handover, see [`SharedReader`].
    pub(super) async fn lock(&self) -> OwnedMutexGuard<QueueReader> {
        self.reader.clone().lock_owned().await
    }

    /// Returns the token that ends the lease, at a handover or at an abort of the peer.
    pub(super) fn revoked(&self) -> &CancellationToken {
        &self.revoked
    }

    /// Returns the token that the actor cancels when it aborts the peer.
    pub(super) fn aborted(&self) -> &CancellationToken {
        &self.aborted
    }
}

/// The reader of the queue to a peer, which its connections take in turn.
///
/// The writer of a connection locks it once, and keeps the lock for as long as
/// it writes: across all its messages, not per message. So the lock is free of
/// contention while a peer has one connection. Only the actor pushes, and it
/// never locks.
///
/// Two writers contend only at a handover, when a new connection replaces the
/// active one. The new writer waits in [`QueueLease::lock`], and the old writer
/// unlocks after its current message. Waiters get the lock in order, and a
/// writer that stops while it waits leaves the line. We thus write the
/// messages in the order of the queue. The peer may still handle them in
/// another order, see what the design gives up in [`SendQueue`].
///
/// A writer must not keep the lock without bound, or the next connection waits
/// forever. Every way out of the loop of a writer unlocks: the end of the
/// queue, a stop, a closed connection, an error, and a drop of its future. A
/// stopped writer gets [`HANDOVER_LIMIT`](super::util::HANDOVER_LIMIT) for its
/// current message. Then it stops the write, and the next connection writes
/// the message again.
type SharedReader = Arc<tokio::sync::Mutex<QueueReader>>;

/// Reads a queue, and keeps the message that a writer has not finished.
///
/// A writer that stops in the middle of a message leaves it here, so the next
/// connection writes it first.
#[derive(Debug)]
pub(super) struct QueueReader {
    rx: mpsc::UnboundedReceiver<QueuedMessage>,
    unfinished: Option<QueuedMessage>,
    closing: watch::Receiver<bool>,
}

impl QueueReader {
    /// Waits for the next message to write, and returns `false` once no more will come.
    ///
    /// No more come once the queue is gone, or the queue is empty while it
    /// closes, see [`SendQueue::set_closing`]. The message stays the current
    /// one until [`Self::advance`]. This is cancellation-safe.
    pub(super) async fn next(&mut self) -> bool {
        if self.unfinished.is_none() {
            self.unfinished = tokio::select! {
                biased;
                item = self.rx.recv() => item,
                _ = self.closing.wait_for(|closing| *closing) => None,
            };
        }
        self.unfinished.is_some()
    }

    /// Returns the message that [`Self::next`] waited for.
    pub(super) fn current(&self) -> Option<&QueuedMessage> {
        self.unfinished.as_ref()
    }

    /// Ends the current message, written or dropped, which releases its reservation.
    pub(super) fn advance(&mut self) {
        self.unfinished = None;
    }

    /// Returns whether no message waits to be written.
    pub(super) fn is_empty(&self) -> bool {
        self.unfinished.is_none() && self.rx.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use iroh::SecretKey;

    use super::*;
    use crate::proto::{topic::Message, TopicId};

    fn proto(message: Message<iroh::PublicKey>) -> ProtoMessage {
        let topic = TopicId::from_bytes([1; 32]);
        ProtoMessage { topic, message }
    }

    fn data(len: usize) -> ProtoMessage {
        proto(Message::test_gossip(len))
    }

    fn ihave() -> ProtoMessage {
        proto(Message::test_ihave())
    }

    fn shuffle() -> ProtoMessage {
        proto(Message::test_shuffle(SecretKey::generate().public(), 0))
    }

    fn disconnect() -> ProtoMessage {
        proto(Message::test_disconnect())
    }

    fn size(message: &ProtoMessage) -> usize {
        postcard::experimental::serialized_size(&message.message).unwrap() + MESSAGE_OVERHEAD
    }

    /// Returns the bytes in use of the whole queue and of its data section.
    fn used(queue: &SendQueue) -> (usize, usize) {
        let capacity = &queue.capacity;
        let used = |limit: &Limit| limit.used.load(Ordering::Relaxed);
        (used(&capacity.total), used(&capacity.data))
    }

    /// Pushes `message` until the queue no longer queues it, and returns how many it queued.
    fn fill(queue: &SendQueue, message: impl Fn() -> ProtoMessage) -> (usize, Pushed) {
        let mut queued = 0;
        loop {
            match queue.push(message()) {
                Pushed::Queued => queued += 1,
                other => return (queued, other),
            }
        }
    }

    /// The reader gets data, control and a `Disconnect` in the order of the pushes.
    #[test]
    fn messages_keep_their_order() {
        let queue = SendQueue::new(MIN_QUEUE_SIZE);
        let pushed = [data(10), shuffle(), ihave(), disconnect(), data(20)];
        for message in pushed.clone() {
            assert!(matches!(queue.push(message), Pushed::Queued));
        }
        let mut reader = queue.try_reader().unwrap();
        for message in pushed {
            let item = reader.rx.try_recv().unwrap();
            assert_eq!(item.message.message, message.message);
        }
    }

    /// Data and announcements fill only their section, and leave room for control.
    #[test]
    fn sections_leave_room_for_control() {
        let queue_bytes = MIN_QUEUE_SIZE;
        let queue = SendQueue::new(queue_bytes);
        let (queued, last) = fill(&queue, || data(1000));
        assert_eq!(queued, queue_bytes / 4 * 3 / size(&data(1000)));
        assert!(matches!(last, Pushed::Dropped(_)));

        let (queued, last) = fill(&queue, ihave);
        assert_eq!(queued, queue_bytes / 8 / size(&ihave()));
        assert!(matches!(last, Pushed::Dropped(_)));

        let (queued, last) = fill(&queue, shuffle);
        assert!(queued >= queue_bytes / 8 / size(&shuffle()));
        assert!(matches!(last, Pushed::Overflow));
        assert!(queue_bytes - used(&queue).0 < size(&shuffle()));
    }

    /// A `Disconnect` takes room as any control message, so many of them overflow the queue.
    #[test]
    fn disconnects_overflow_the_queue() {
        let queue = SendQueue::new(MIN_QUEUE_SIZE);
        let (queued, last) = fill(&queue, disconnect);
        assert_eq!(queued, MIN_QUEUE_SIZE / size(&disconnect()));
        assert!(matches!(last, Pushed::Overflow));
    }

    /// A message that fits its section but not the whole queue leaves its section as it was.
    #[test]
    fn failed_reservation_releases_its_section() {
        let queue = SendQueue::new(MIN_QUEUE_SIZE);
        let (_, last) = fill(&queue, shuffle);
        assert!(matches!(last, Pushed::Overflow));
        assert!(matches!(queue.push(data(10)), Pushed::Dropped(_)));
        assert_eq!(used(&queue).1, 0);
    }

    /// A message keeps its reservation until the writer drops it, also once it left the channel.
    #[test]
    fn message_keeps_its_reservation_until_dropped() {
        let queue = SendQueue::new(MIN_QUEUE_SIZE);
        let one = size(&data(1000));
        assert!(matches!(queue.push(data(1000)), Pushed::Queued));
        let in_writing = queue.try_reader().unwrap().rx.try_recv().unwrap();
        assert_eq!(used(&queue), (one, one));
        drop(in_writing);
        assert_eq!(used(&queue), (0, 0));
    }

    /// A closing queue ends once it is empty, and goes on if it opens again.
    #[tokio::test]
    async fn closing_queue_ends_when_empty() {
        let queue = SendQueue::new(MIN_QUEUE_SIZE);
        assert!(matches!(queue.push(disconnect()), Pushed::Queued));
        queue.set_closing(true);
        let mut reader = queue.try_reader().unwrap();
        assert!(reader.next().await, "the queued message went");
        reader.advance();
        assert!(!reader.next().await, "the empty closing queue did not end");
        queue.set_closing(false);
        assert!(matches!(queue.push(shuffle()), Pushed::Queued));
        assert!(reader.next().await, "the opened queue did not go on");
    }

    /// A message that a writer did not finish stays first, for the next writer.
    #[tokio::test]
    async fn unfinished_message_stays_first() {
        let mut queue = SendQueue::new(MIN_QUEUE_SIZE);
        assert!(matches!(queue.push(data(10)), Pushed::Queued));
        assert!(matches!(queue.push(data(20)), Pushed::Queued));
        let first = |queue_lease: QueueLease, len| async move {
            let mut reader = queue_lease.lock().await;
            assert!(reader.next().await);
            let current = &reader.current().unwrap().message.message;
            assert_eq!(current, &data(len).message);
            reader
        };
        // The first writer stops without `advance`.
        drop(first(queue.lease(), 10).await);
        let mut reader = first(queue.lease(), 10).await;
        reader.advance();
        drop(reader);
        first(queue.lease(), 20).await.advance();
        assert_eq!(used(&queue), (0, 0));
    }

    /// The least queue fits eight messages of the largest size, so its announcements fit one.
    #[test]
    fn least_size_fits_eight_large_messages() {
        let max = 1 << 20;
        assert_eq!(queue_size(0, max), 8 * (max + MESSAGE_OVERHEAD));
        assert_eq!(queue_size(0, 1024), MIN_QUEUE_SIZE);
        assert_eq!(queue_size(0, usize::MAX), usize::MAX);
    }
}
