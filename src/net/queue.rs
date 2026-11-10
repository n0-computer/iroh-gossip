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

use tokio::sync::{mpsc, watch};

use super::ProtoMessage;
use crate::{api::AuthToken, proto::state::MessageKind};

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
    /// The auth token of the topic, for a stream that this message opens.
    pub(super) token: Option<AuthToken>,
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
    /// Returns the limit of `section`, or `None` for control.
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
    ///
    /// Only the actor reserves, so a take that goes over the maximum and gives back
    /// cannot make another reservation fail that would fit.
    fn reserve(&self, size: usize) -> bool {
        // The counter guards no other data, so relaxed ordering suffices.
        let fits = self.used.fetch_add(size, Ordering::Relaxed) + size <= self.max;
        if !fits {
            self.release(size);
        }
        fits
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
    /// Reserves the room of a message of `encoded_len` bytes in the queue and in its section, if both have it.
    fn new(capacity: &Arc<Capacity>, encoded_len: Option<usize>, section: Section) -> Option<Self> {
        let encoded_len = u32::try_from(encoded_len?).ok()?;
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
/// still drain an older queue for a short time after a disconnect, so the bytes
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
///   earlier `Close`. The peer also limits its streams, to 100 by
///   default. Over that limit, an open waits
///   [`OPEN_TIMEOUT`](super::wire::OPEN_TIMEOUT) and closes the connection,
///   again after each rejoin.
/// - The queue bounds bytes, not time. It fills only once QUIC buffered all
///   it can for the peer, so a burst larger than both loses messages before
///   anyone waits. Under sustained overload, data at a peer on a 1 Mbit/s
///   link can arrive up to 30 to 40 s late, as it did in tests. Most of it
///   waits in the send buffer of QUIC, which the flow control window of the
///   peer sizes, not in our queue.
/// - Repair is capped, at 256 ids per peer, 4096 per topic, and 64 per peer
///   and second. It works only while the message is cached, and never for
///   neighbor scope.
/// - All topics share the queue to a peer. A busy topic crowds out another,
///   and control waits behind the data queued before it.
#[derive(Debug)]
pub(super) struct SendQueue {
    tx: mpsc::UnboundedSender<QueuedMessage>,
    capacity: Arc<Capacity>,
    closed: watch::Sender<bool>,
}

impl SendQueue {
    /// Creates an open queue of `size` bytes, of which data may fill 3/4 and announcements 1/8.
    ///
    /// Returns the queue and its only reader.
    pub(super) fn new(size: usize) -> (Self, QueueReader) {
        let (tx, rx) = mpsc::unbounded_channel();
        let capacity = Arc::new(Capacity {
            total: Limit::new(size),
            data: Limit::new(size / 4 * 3),
            announcements: Limit::new(size / 8),
        });
        let closed = watch::Sender::new(false);
        let reader = QueueReader {
            rx,
            unfinished: None,
            closed: closed.subscribe(),
            capacity: capacity.clone(),
        };
        let queue = Self {
            tx,
            capacity,
            closed,
        };
        (queue, reader)
    }

    /// Queues `message` of the test, with its encoded size.
    #[cfg(test)]
    pub(super) fn send(&self, message: ProtoMessage) -> Pushed {
        let len = super::wire::message_len(&message.message).ok();
        self.push(message, len, None)
    }

    /// Queues `message` with its encoded size `len`, if known, and never waits.
    ///
    /// The message keeps `token`, for the stream that it may open.
    pub(super) fn push(
        &self,
        message: ProtoMessage,
        len: Option<usize>,
        token: Option<AuthToken>,
    ) -> Pushed {
        let section = Section::of(&message);
        let Some(reservation) = Reservation::new(&self.capacity, len, section) else {
            return match section {
                Section::Control => Pushed::Overflow,
                Section::Data | Section::Announcements => Pushed::Dropped(message),
            };
        };
        let item = QueuedMessage {
            message,
            reservation,
            token,
        };
        let sent = self.tx.send(item);
        debug_assert!(sent.is_ok(), "the queue keeps its reader");
        Pushed::Queued
    }

    /// Closes the queue.
    ///
    /// We close the queue when no topic uses the peer anymore. Messages can
    /// still be pushed to a closed queue, and the writer still sends them. But
    /// a closed queue does not need a connection: we do not dial for it, and
    /// once it is empty, the writer finishes its streams so that the
    /// connection can retire.
    pub(super) fn close(&self) {
        set_if_changed(&self.closed, true);
    }

    /// Opens the queue when a topic sends to the peer again.
    pub(super) fn open(&self) {
        set_if_changed(&self.closed, false);
    }

    /// Returns whether the queue holds no messages.
    ///
    /// A message that a writer started but did not finish still counts.
    pub(super) fn is_empty(&self) -> bool {
        self.capacity.total.used.load(Ordering::Relaxed) == 0
    }

    /// Returns whether the queue is closed.
    pub(super) fn is_closed(&self) -> bool {
        *self.closed.borrow()
    }

    /// Returns the byte limit of the queue.
    pub(super) fn size(&self) -> usize {
        self.capacity.total.max
    }
}

/// Reads a queue, and keeps the message that a writer has not finished.
///
/// The connections of a peer take the reader in turn, and move it between
/// them. A message that one writer did not finish stays in the reader, so the
/// next writer writes it first.
#[derive(Debug)]
pub(super) struct QueueReader {
    rx: mpsc::UnboundedReceiver<QueuedMessage>,
    unfinished: Option<QueuedMessage>,
    /// Tells whether the queue is closed, see [`SendQueue::close`].
    pub(super) closed: watch::Receiver<bool>,
    /// The capacity of the queue. The reader uses it only to identify the queue.
    capacity: Arc<Capacity>,
}

impl QueueReader {
    /// Returns whether this is the reader of `queue`.
    ///
    /// A reader that comes back from a writer may belong to a discarded queue.
    pub(super) fn reads(&self, queue: &SendQueue) -> bool {
        Arc::ptr_eq(&self.capacity, &queue.capacity)
    }

    /// Waits for a message to write, and returns `false` once the queue was dropped.
    ///
    /// The actor drops the [`SendQueue`] when it replaces it with a new one,
    /// when it forgets the peer, and when it stops. Messages are still returned
    /// when the queue is closed. The message stays current until
    /// [`Self::advance`] is called. This is cancellation-safe.
    pub(super) async fn wait_next(&mut self) -> bool {
        if self.unfinished.is_none() {
            self.unfinished = self.rx.recv().await;
        }
        self.unfinished.is_some()
    }

    /// Returns the message that [`Self::wait_next`] waited for.
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

/// Sets `flag` to `value`, and wakes its receivers only if the value changed.
pub(super) fn set_if_changed(flag: &watch::Sender<bool>, value: bool) {
    flag.send_if_modified(|old| std::mem::replace(old, value) != value);
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
        proto(Message::test_close())
    }

    fn size(message: &ProtoMessage) -> usize {
        crate::net::wire::message_len(&message.message).unwrap() + MESSAGE_OVERHEAD
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
            match queue.send(message()) {
                Pushed::Queued => queued += 1,
                other => return (queued, other),
            }
        }
    }

    /// The reader gets data, control, and a `Close` in the order of the pushes.
    #[test]
    fn messages_keep_their_order() {
        let (queue, mut reader) = SendQueue::new(MIN_QUEUE_SIZE);
        let pushed = [data(10), shuffle(), ihave(), disconnect(), data(20)];
        for message in pushed.clone() {
            assert!(matches!(queue.send(message), Pushed::Queued));
        }
        for message in pushed {
            let item = reader.rx.try_recv().unwrap();
            assert_eq!(item.message.message, message.message);
        }
    }

    /// Data and announcements fill only their section, and leave room for control.
    #[test]
    fn sections_leave_room_for_control() {
        let queue_bytes = MIN_QUEUE_SIZE;
        let (queue, _reader) = SendQueue::new(queue_bytes);
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

    /// A `Close` takes room like any control message, so many of them overflow the queue.
    #[test]
    fn disconnects_overflow_the_queue() {
        let (queue, _reader) = SendQueue::new(MIN_QUEUE_SIZE);
        let (queued, last) = fill(&queue, disconnect);
        assert_eq!(queued, MIN_QUEUE_SIZE / size(&disconnect()));
        assert!(matches!(last, Pushed::Overflow));
    }

    /// A message that fits its section but not the whole queue leaves its section as it was.
    #[test]
    fn failed_reservation_releases_its_section() {
        let (queue, _reader) = SendQueue::new(MIN_QUEUE_SIZE);
        let (_, last) = fill(&queue, shuffle);
        assert!(matches!(last, Pushed::Overflow));
        assert!(matches!(queue.send(data(10)), Pushed::Dropped(_)));
        assert_eq!(used(&queue).1, 0);
    }

    /// A message keeps its reservation until the writer drops it, also once it left the channel.
    #[test]
    fn message_keeps_its_reservation_until_dropped() {
        let (queue, mut reader) = SendQueue::new(MIN_QUEUE_SIZE);
        let one = size(&data(1000));
        assert!(matches!(queue.send(data(1000)), Pushed::Queued));
        let in_writing = reader.rx.try_recv().unwrap();
        assert_eq!(used(&queue), (one, one));
        drop(in_writing);
        assert_eq!(used(&queue), (0, 0));
    }

    /// Messages are still returned when the queue is closed, and the reader sees it close and open.
    #[tokio::test]
    async fn closed_queue_gives_its_messages() {
        let (queue, mut reader) = SendQueue::new(MIN_QUEUE_SIZE);
        assert!(matches!(queue.send(disconnect()), Pushed::Queued));
        queue.close();
        let closed = reader.closed.clone();
        assert!(*closed.borrow(), "the reader did not see the close");
        assert!(reader.wait_next().await, "the queued message went");
        reader.advance();
        queue.open();
        assert!(!*closed.borrow(), "the reader did not see the open");
        assert!(matches!(queue.send(shuffle()), Pushed::Queued));
        assert!(reader.wait_next().await, "the opened queue did not go on");
    }

    /// A message that a writer did not finish stays first, for the next writer.
    #[tokio::test]
    async fn unfinished_message_stays_first() {
        let (queue, reader) = SendQueue::new(MIN_QUEUE_SIZE);
        assert!(matches!(queue.send(data(10)), Pushed::Queued));
        assert!(matches!(queue.send(data(20)), Pushed::Queued));
        let first = |mut reader: QueueReader, len| async move {
            assert!(reader.wait_next().await);
            let current = &reader.current().unwrap().message.message;
            assert_eq!(current, &data(len).message);
            reader
        };
        // The first writer stops without `advance`, and the reader moves on.
        let reader = first(reader, 10).await;
        let mut reader = first(reader, 10).await;
        reader.advance();
        first(reader, 20).await.advance();
        assert_eq!(used(&queue), (0, 0));
    }

    /// A reader knows its own queue, and not a queue that replaced it.
    #[test]
    fn reader_knows_its_queue() {
        let (queue, reader) = SendQueue::new(MIN_QUEUE_SIZE);
        let (next, _next_reader) = SendQueue::new(MIN_QUEUE_SIZE);
        assert!(reader.reads(&queue));
        assert!(!reader.reads(&next));
    }

    #[test]
    fn reader_sees_an_empty_queue() {
        let (queue, mut reader) = SendQueue::new(MIN_QUEUE_SIZE);
        assert!(reader.is_empty());
        assert!(matches!(queue.send(data(10)), Pushed::Queued));
        assert!(!reader.is_empty());
        reader.unfinished = reader.rx.try_recv().ok();
        assert!(!reader.is_empty());
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
