//! The handles of subscribed topics, and the loop that forwards events to a subscriber.

use tokio::sync::{broadcast, mpsc};

use super::ProtoEvent;
use crate::api::Event;

/// Channel capacity for broadcast subscriber event queue (one per topic)
const TOPIC_EVENT_CAP: usize = 256;

/// The senders and subscribers of a topic.
#[derive(Debug)]
pub(super) struct TopicHandles {
    pub(super) events: broadcast::Sender<ProtoEvent>,
    /// Counts subscription command receivers, including those waiting to publish.
    ///
    /// A receiver remains counted until the actor processes its end.
    pub(super) command_receivers: usize,
}

impl Default for TopicHandles {
    fn default() -> Self {
        let (events, _) = broadcast::channel(TOPIC_EVENT_CAP);
        Self {
            command_receivers: 0,
            events,
        }
    }
}

impl TopicHandles {
    /// Returns whether the topic still has a publisher or a subscriber.
    pub(super) fn has_handles(&self) -> bool {
        self.command_receivers != 0 || self.events.receiver_count() > 0
    }
}

pub(super) async fn topic_subscriber_loop(
    sender: mpsc::Sender<Event>,
    mut topic_events: broadcast::Receiver<ProtoEvent>,
) {
    loop {
        tokio::select! {
           biased;
           msg = topic_events.recv() => {
               let event = match msg {
                   Err(broadcast::error::RecvError::Closed) => break,
                   Err(broadcast::error::RecvError::Lagged(_)) => Event::Lagged,
                   Ok(event) => event.into(),
               };
               if sender.send(event).await.is_err() {
                   break;
               }
           }
           _ = sender.closed() => break,
        }
    }
}
