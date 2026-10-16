//! The queue of messages to one peer, bounded in bytes.
//!
//! All messages to the peer share one FIFO, so the peer gets them in the order
//! that the protocol sent them. Data and announcements may each fill only a
//! section of the queue, so other control messages always find some room.
//!
//! The size bounds the bytes that wait to be sent, and so how long a message
//! waits behind them. It overstates memory: the queues to all peers and the
//! cache of Plumtree share the content of a broadcast.

use std::{
    mem,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use tokio::sync::{mpsc, OwnedMutexGuard};
use tokio_util::sync::CancellationToken;

use super::ProtoMessage;
use crate::proto::state::MessageKind;

/// The bytes that a queued message takes on top of its encoded size.
///
/// It is the size of a [`QueuedMessage`] in memory, and the length prefix of
/// its frame. It also caps the number of messages in a queue.
pub(super) const MESSAGE_OVERHEAD: usize = mem::size_of::<QueuedMessage>() + 4;

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

/// What [`QueueSender::push`] did with a message.
#[derive(Debug)]
pub(super) enum Pushed {
    /// The message was queued.
    Queued,
    /// The data or announcement section was full, so the peer loses the message.
    Dropped(ProtoMessage),
    /// The queue had no room for a control message.
    Overflow,
    /// No connection can take the queue anymore.
    Closed,
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
        // The counter guards no other data, so relaxed ordering suffices.
        self.used
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                used.checked_add(size).filter(|&used| used <= self.max)
            })
            .is_ok()
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

/// The receiving half of the queue to a peer, which its connections take in turn.
///
/// The writer of a connection locks it once, and keeps the lock for as long as
/// it writes: across all its messages, not per message. So the lock is free of
/// contention while a peer has one connection. Only the actor pushes, and it
/// never locks.
///
/// Two writers contend only at a handover, when a new connection replaces the
/// active one. The new writer waits in [`Self::lock`], and the old writer
/// unlocks after its current message. Waiters get the lock in order, and a
/// writer that stops while it waits leaves the line. The messages thus go out
/// in the order of the queue. The peer may still read the last messages on the
/// old connection after the first ones on the new connection.
///
/// A writer must not keep the lock without bound, or the next connection waits
/// for good. Every way out of the loop of a writer unlocks: the end of the
/// queue, a stop, a closed connection, an error, and a drop of its future. A
/// stopped writer gets [`HANDOVER_LIMIT`](super::util::HANDOVER_LIMIT) for its
/// current message, and then closes its connection, which ends the write.
#[derive(Debug, Clone)]
pub(super) struct SharedReceiver(Arc<tokio::sync::Mutex<QueueReader>>);

impl SharedReceiver {
    /// Waits until no other writer reads the queue, and returns its reader.
    ///
    /// Waits only at a handover, see [`SharedReceiver`].
    pub(super) async fn lock(&self) -> OwnedMutexGuard<QueueReader> {
        self.0.clone().lock_owned().await
    }

    /// Returns the reader of the queue, if no writer reads it.
    #[cfg(test)]
    pub(super) fn try_lock(&self) -> Option<OwnedMutexGuard<QueueReader>> {
        self.0.clone().try_lock_owned().ok()
    }
}

/// The turn of a connection at the queue of its peer.
#[derive(Debug)]
pub(super) struct QueueTurn {
    pub(super) queue: SharedReceiver,
    /// Ends the turn after the current message, once another connection takes the queue.
    pub(super) stop: CancellationToken,
}

/// Reads a queue, and keeps the message that a writer has not finished.
///
/// A writer that stops in the middle of a message leaves it here, so the next
/// connection writes it first.
#[derive(Debug)]
pub(super) struct QueueReader {
    rx: mpsc::UnboundedReceiver<QueuedMessage>,
    unfinished: Option<QueuedMessage>,
}

impl QueueReader {
    /// Waits for the next message to write, and returns `false` once the sender is gone.
    ///
    /// The message stays the current one until [`Self::written`]. This is
    /// cancellation-safe.
    pub(super) async fn next(&mut self) -> bool {
        if self.unfinished.is_none() {
            self.unfinished = self.rx.recv().await;
        }
        self.unfinished.is_some()
    }

    /// Returns the message that [`Self::next`] waited for.
    pub(super) fn current(&self) -> Option<&QueuedMessage> {
        self.unfinished.as_ref()
    }

    /// Ends the current message, which releases its reservation.
    pub(super) fn written(&mut self) {
        self.unfinished = None;
    }

    /// Returns whether no message waits to be written.
    pub(super) fn is_empty(&self) -> bool {
        self.unfinished.is_none() && self.rx.is_empty()
    }
}

/// The sending half of a queue.
///
/// The [`Capacity`] bounds the queue, so its channel needs no bound of its own.
/// Each message reserves at least [`MESSAGE_OVERHEAD`] bytes until the writer drops it.
#[derive(Debug)]
pub(super) struct QueueSender {
    tx: mpsc::UnboundedSender<QueuedMessage>,
    capacity: Arc<Capacity>,
}

impl QueueSender {
    /// Creates a queue of `size` bytes, of which data may fill 3/4 and announcements 1/8.
    pub(super) fn new(size: usize) -> (Self, SharedReceiver) {
        let (tx, rx) = mpsc::unbounded_channel();
        let capacity = Arc::new(Capacity {
            total: Limit::new(size),
            data: Limit::new(size / 4 * 3),
            announcements: Limit::new(size / 8),
        });
        let reader = QueueReader {
            rx,
            unfinished: None,
        };
        let rx = SharedReceiver(Arc::new(tokio::sync::Mutex::new(reader)));
        (Self { tx, capacity }, rx)
    }

    /// Queues `message` if it fits, and never waits.
    pub(super) fn push(&self, message: ProtoMessage) -> Pushed {
        if self.tx.is_closed() {
            return Pushed::Closed;
        }
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
        match self.tx.send(item) {
            Ok(()) => Pushed::Queued,
            Err(_) => Pushed::Closed,
        }
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
    fn used(queue: &QueueSender) -> (usize, usize) {
        let capacity = &queue.capacity;
        let used = |limit: &Limit| limit.used.load(Ordering::Relaxed);
        (used(&capacity.total), used(&capacity.data))
    }

    /// Pushes `message` until the queue no longer queues it, and returns how many it queued.
    fn fill(queue: &QueueSender, message: impl Fn() -> ProtoMessage) -> (usize, Pushed) {
        let mut queued = 0;
        loop {
            match queue.push(message()) {
                Pushed::Queued => queued += 1,
                other => return (queued, other),
            }
        }
    }

    /// The peer gets data, control and a `Disconnect` in the order of the pushes.
    #[test]
    fn messages_keep_their_order() {
        let (queue, rx) = QueueSender::new(MIN_QUEUE_SIZE);
        let pushed = [data(10), shuffle(), ihave(), disconnect(), data(20)];
        for message in pushed.clone() {
            assert!(matches!(queue.push(message), Pushed::Queued));
        }
        let mut reader = rx.try_lock().unwrap();
        for message in pushed {
            let item = reader.rx.try_recv().unwrap();
            assert_eq!(item.message.message, message.message);
        }
    }

    /// Data and announcements fill only their section, and leave room for control.
    #[test]
    fn sections_leave_room_for_control() {
        let queue_bytes = MIN_QUEUE_SIZE;
        let (queue, _rx) = QueueSender::new(queue_bytes);
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
        let (queue, _rx) = QueueSender::new(MIN_QUEUE_SIZE);
        let (queued, last) = fill(&queue, disconnect);
        assert_eq!(queued, MIN_QUEUE_SIZE / size(&disconnect()));
        assert!(matches!(last, Pushed::Overflow));
    }

    /// A message that fits its section but not the whole queue leaves its section as it was.
    #[test]
    fn failed_reservation_releases_its_section() {
        let (queue, _rx) = QueueSender::new(MIN_QUEUE_SIZE);
        let (_, last) = fill(&queue, shuffle);
        assert!(matches!(last, Pushed::Overflow));
        assert!(matches!(queue.push(data(10)), Pushed::Dropped(_)));
        assert_eq!(used(&queue).1, 0);
    }

    /// A message keeps its reservation until the writer drops it, also once it left the channel.
    #[test]
    fn message_keeps_its_reservation_until_dropped() {
        let (queue, rx) = QueueSender::new(MIN_QUEUE_SIZE);
        let one = size(&data(1000));
        assert!(matches!(queue.push(data(1000)), Pushed::Queued));
        let in_writing = rx.try_lock().unwrap().rx.try_recv().unwrap();
        assert_eq!(used(&queue), (one, one));
        drop(in_writing);
        assert_eq!(used(&queue), (0, 0));
    }

    /// A message that a writer did not finish stays first, for the next writer.
    #[tokio::test]
    async fn unfinished_message_stays_first() {
        let (queue, reader) = QueueSender::new(MIN_QUEUE_SIZE);
        assert!(matches!(queue.push(data(10)), Pushed::Queued));
        assert!(matches!(queue.push(data(20)), Pushed::Queued));
        let first = |len| {
            let reader = reader.clone();
            async move {
                let mut reader = reader.lock().await;
                assert!(reader.next().await);
                let current = &reader.current().unwrap().message.message;
                assert_eq!(current, &data(len).message);
                reader
            }
        };
        // The first writer stops without `written`.
        drop(first(10).await);
        let mut reader = first(10).await;
        reader.written();
        drop(reader);
        first(20).await.written();
        assert_eq!(used(&queue), (0, 0));
    }

    /// A queue without reader takes nothing, and releases the reservations of its messages.
    #[test]
    fn closed_queue_frees_its_room() {
        let (queue, rx) = QueueSender::new(MIN_QUEUE_SIZE);
        assert!(matches!(queue.push(data(1000)), Pushed::Queued));
        drop(rx);
        assert!(matches!(queue.push(shuffle()), Pushed::Closed));
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
