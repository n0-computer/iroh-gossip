//! The queue of messages to a peer, with a byte limit per class.
//!
//! Control and data have a writer and a limit each, so a data stream that flow
//! control holds up does not delay a control message.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use tokio::sync::mpsc;

use super::ProtoMessage;
use crate::proto::{state::MessageKind, TopicId};

/// What the queue charges for an item on top of its encoded size.
pub(super) const ITEM_COST: usize = 64;

/// The least size of a queue, whatever its options.
pub(super) const MIN_QUEUE_SIZE: usize = 64 * 1024;

/// The size of a queue by default, see [`QueueOptions::size`](super::QueueOptions::size).
pub(super) const QUEUE_SIZE: usize = 1024 * 1024;

/// An item of a class.
#[derive(Debug)]
pub(super) enum QueueItem {
    /// A message, with its encoded size.
    Message(ProtoMessage, usize),
    /// The topic ended with the peer, so the stream of the class ends after the items before.
    End(TopicId),
}

impl QueueItem {
    fn charge(&self) -> usize {
        match self {
            QueueItem::Message(_, size) => size + ITEM_COST,
            QueueItem::End(_) => ITEM_COST,
        }
    }
}

/// What [`QueueSender::push`] did with a message.
#[derive(Debug)]
pub(super) enum Pushed {
    Queued,
    /// The class was full, and the message is not worth the peer.
    Dropped(ProtoMessage),
    /// The control class was full. The topics cannot work with the peer then.
    Overflow,
    /// The writers are gone.
    Closed,
}

/// The sending half of the queue to one connection.
#[derive(Debug)]
pub(super) struct QueueSender {
    control: ClassSender,
    data: ClassSender,
}

/// The receiving halves of the queue to one connection, one per class.
#[derive(Debug)]
pub(super) struct QueueReceivers {
    pub(super) control: QueueReceiver,
    pub(super) data: QueueReceiver,
}

#[derive(Debug)]
struct ClassSender {
    tx: mpsc::UnboundedSender<QueueItem>,
    bytes: Arc<AtomicUsize>,
    limit: usize,
}

#[derive(Debug)]
pub(super) struct QueueReceiver {
    rx: mpsc::UnboundedReceiver<QueueItem>,
    bytes: Arc<AtomicUsize>,
}

fn class(limit: usize) -> (ClassSender, QueueReceiver) {
    let (tx, rx) = mpsc::unbounded_channel();
    let bytes = Arc::<AtomicUsize>::default();
    let receiver = QueueReceiver {
        rx,
        bytes: bytes.clone(),
    };
    (ClassSender { tx, bytes, limit }, receiver)
}

/// Returns the size of a queue for `size` of the options and `max_message_size`.
///
/// It holds at least four messages of the largest size.
pub(super) fn queue_size(size: usize, max_message_size: usize) -> usize {
    size.max(MIN_QUEUE_SIZE)
        .max(4 * (max_message_size + ITEM_COST))
}

impl QueueSender {
    /// Creates a queue of `limit` bytes: a quarter for control, the rest for data.
    pub(super) fn new(limit: usize) -> (Self, QueueReceivers) {
        let (control, control_rx) = class(limit / 4);
        let (data, data_rx) = class(limit - limit / 4);
        let queues = QueueReceivers {
            control: control_rx,
            data: data_rx,
        };
        (Self { control, data }, queues)
    }

    /// Queues `message`, if its class has room for it.
    ///
    /// Announcements get half the control class, so they cannot crowd out the rest.
    /// A `Disconnect` also ends the data stream of its topic.
    pub(super) fn push(&self, message: ProtoMessage) -> Pushed {
        let announcement = message.message.is_announcement();
        let (class, dropped) = match message.kind() {
            MessageKind::Data => (&self.data, true),
            MessageKind::Control => (&self.control, announcement),
        };
        let limit = class.limit / if announcement { 2 } else { 1 };
        let size = postcard::experimental::serialized_size(&message.message).unwrap_or(0);
        let end = message.message.is_disconnect().then_some(message.topic);
        let item = QueueItem::Message(message, size);
        let charge = item.charge();
        if class.bytes.fetch_add(charge, Ordering::Relaxed) + charge > limit {
            class.bytes.fetch_sub(charge, Ordering::Relaxed);
            let QueueItem::Message(message, _) = item else {
                unreachable!("built as a message above");
            };
            return match dropped {
                true => Pushed::Dropped(message),
                false => Pushed::Overflow,
            };
        }
        if class.tx.send(item).is_err() {
            return Pushed::Closed;
        }
        if let Some(topic) = end {
            // It always fits, but is charged.
            self.data.bytes.fetch_add(ITEM_COST, Ordering::Relaxed);
            self.data.tx.send(QueueItem::End(topic)).ok();
        }
        Pushed::Queued
    }
}

impl QueueReceiver {
    /// Takes the next item, or returns `None` once the sender is gone.
    pub(super) async fn recv(&mut self) -> Option<QueueItem> {
        let item = self.rx.recv().await?;
        self.bytes.fetch_sub(item.charge(), Ordering::Relaxed);
        Some(item)
    }

    /// Returns whether no item waits.
    pub(super) fn is_empty(&self) -> bool {
        self.rx.is_empty()
    }

    /// Stops the queue, so that pushes fail from now on.
    pub(super) fn close(&mut self) {
        self.rx.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::topic::Message;

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

    fn disconnect() -> ProtoMessage {
        proto(Message::test_disconnect())
    }

    fn size(message: &ProtoMessage) -> usize {
        postcard::experimental::serialized_size(&message.message).unwrap() + ITEM_COST
    }

    /// Each class fills only its own share, and only a full control class fails the peer.
    #[test]
    fn classes_fill_their_own_share() {
        let limit = 4 * 1000;
        let (queue, _queues) = QueueSender::new(limit);
        let data_share = limit - limit / 4;
        let one = size(&data(100));
        for _ in 0..data_share / one {
            assert!(matches!(queue.push(data(100)), Pushed::Queued));
        }
        assert!(matches!(queue.push(data(100)), Pushed::Dropped(_)));

        let control_share = limit / 4;
        let one = size(&ihave());
        for _ in 0..control_share / 2 / one {
            assert!(matches!(queue.push(ihave()), Pushed::Queued));
        }
        assert!(matches!(queue.push(ihave()), Pushed::Dropped(_)));

        let one = size(&disconnect());
        let mut pushed = 0;
        while matches!(queue.push(disconnect()), Pushed::Queued) {
            pushed += 1;
        }
        assert!(pushed > 0 && pushed <= control_share / one);
        assert!(matches!(queue.push(disconnect()), Pushed::Overflow));
    }

    /// A control message does not wait behind data, and a `Disconnect` ends the data stream too.
    #[tokio::test]
    async fn classes_are_separate() {
        let (queue, mut queues) = QueueSender::new(100_000);
        assert!(matches!(queue.push(data(10)), Pushed::Queued));
        assert!(matches!(queue.push(disconnect()), Pushed::Queued));
        assert!(matches!(
            queues.control.recv().await,
            Some(QueueItem::Message(..))
        ));
        assert!(queues.control.is_empty());
        assert!(matches!(
            queues.data.recv().await,
            Some(QueueItem::Message(..))
        ));
        assert!(matches!(queues.data.recv().await, Some(QueueItem::End(_))));
    }

    /// Taking an item frees its bytes.
    #[tokio::test]
    async fn taking_frees_room() {
        let limit = 4 * size(&data(1000));
        let (queue, mut queues) = QueueSender::new(limit);
        while matches!(queue.push(data(1000)), Pushed::Queued) {}
        queues.data.recv().await.expect("an item");
        assert!(matches!(queue.push(data(1000)), Pushed::Queued));
    }

    /// The least queue holds four messages of the largest size.
    #[test]
    fn least_size_holds_four_large_messages() {
        let max = 1 << 20;
        assert_eq!(queue_size(0, max), 4 * (max + ITEM_COST));
        assert_eq!(queue_size(0, 1024), MIN_QUEUE_SIZE);
    }
}
