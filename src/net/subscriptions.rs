//! The handles of subscribed topics, and the loop that forwards events to a subscriber.

use iroh::EndpointId;
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
    /// Returns whether the topic has no publisher and no subscriber left.
    pub(super) fn is_empty(&self) -> bool {
        self.command_receivers == 0 && self.events.receiver_count() == 0
    }
}

/// Forwards the events of a topic to a subscriber.
///
/// First sends a [`Event::NeighborUp`] for each of the `neighbors`, then the live events.
/// The live events wait in `topic_events` meanwhile. If the subscriber falls behind there, it
/// gets [`Event::Lagged`].
pub(super) async fn topic_subscriber_loop(
    sender: mpsc::Sender<Event>,
    neighbors: Vec<EndpointId>,
    mut topic_events: broadcast::Receiver<ProtoEvent>,
) {
    for neighbor in neighbors {
        tokio::select! {
            sent = sender.send(Event::NeighborUp(neighbor)) => {
                if sent.is_err() {
                    return;
                }
            }
            _ = sender.closed() => return,
        }
    }
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
