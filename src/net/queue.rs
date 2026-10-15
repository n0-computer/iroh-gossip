//! The queue of messages to one peer, bounded in bytes.
//!
//! All messages to the peer share one FIFO, so the peer gets them in the order
//! that the protocol sent them. Data and announcements may each fill only a
//! part of the queue, so other control messages always find some room.
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

use tokio::sync::mpsc;

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
    /// The data or announcement part was full, so the peer loses the message.
    Dropped(ProtoMessage),
    /// The queue had no room for a control message.
    Overflow,
    /// The writer is gone.
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

/// The part of a queue that a message counts against, besides the whole queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Control,
    Data,
    Announcements,
}

/// The bytes in use of a queue and of its parts.
#[derive(Debug)]
struct Capacity {
    total: Limit,
    data: Limit,
    announcements: Limit,
}

impl Capacity {
    fn part(&self, part: Part) -> Option<&Limit> {
        match part {
            Part::Control => None,
            Part::Data => Some(&self.data),
            Part::Announcements => Some(&self.announcements),
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
    part: Part,
}

/// Returns the room that a message of `encoded_len` bytes takes.
fn reserved_size(encoded_len: u32) -> usize {
    encoded_len as usize + MESSAGE_OVERHEAD
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let size = reserved_size(self.encoded_len);
        self.capacity.total.release(size);
        if let Some(limit) = self.capacity.part(self.part) {
            limit.release(size);
        }
    }
}

/// The receiving half of a queue.
pub(super) type QueueReceiver = mpsc::UnboundedReceiver<QueuedMessage>;

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
    pub(super) fn new(size: usize) -> (Self, QueueReceiver) {
        let (tx, rx) = mpsc::unbounded_channel();
        let capacity = Arc::new(Capacity {
            total: Limit::new(size),
            data: Limit::new(size / 4 * 3),
            announcements: Limit::new(size / 8),
        });
        (Self { tx, capacity }, rx)
    }

    /// Queues `message` if it fits, and never waits.
    pub(super) fn push(&self, message: ProtoMessage) -> Pushed {
        if self.tx.is_closed() {
            return Pushed::Closed;
        }
        let part = match message.kind() {
            MessageKind::Data => Part::Data,
            MessageKind::Control if message.message.is_announcement() => Part::Announcements,
            MessageKind::Control => Part::Control,
        };
        let Some(reservation) = self.reserve(&message, part) else {
            return match part {
                Part::Control => Pushed::Overflow,
                Part::Data | Part::Announcements => Pushed::Dropped(message),
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

    /// Reserves the room of `message` in the queue and in `part`, if both have it.
    fn reserve(&self, message: &ProtoMessage, part: Part) -> Option<Reservation> {
        let encoded_len = postcard::experimental::serialized_size(&message.message).ok()?;
        let encoded_len = u32::try_from(encoded_len).ok()?;
        let size = reserved_size(encoded_len);
        let limit = self.capacity.part(part);
        if !limit.is_none_or(|limit| limit.reserve(size)) {
            return None;
        }
        if !self.capacity.total.reserve(size) {
            if let Some(limit) = limit {
                limit.release(size);
            }
            return None;
        }
        Some(Reservation {
            capacity: self.capacity.clone(),
            encoded_len,
            part,
        })
    }
}
