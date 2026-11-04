//! The state of subscribed topics, and the loops that feed commands into the protocol.

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    task::{ready, Context, Poll},
    time::Duration,
};

use iroh::{EndpointId, PublicKey};
use n0_future::{
    time::{self, Instant, Sleep},
    FuturesUnordered, StreamExt as _,
};
use rand::rngs::StdRng;
use tokio::sync::{broadcast, mpsc};

use super::{peers::PeerState, ProtoEvent};
use crate::{
    api::{Command, Event},
    proto::{self, TopicId},
};

/// Channel capacity for broadcast subscriber event queue (one per topic)
const TOPIC_EVENT_CAP: usize = 256;

/// How often a waiting broadcast checks the current queue pressure again.
pub(super) const BROADCAST_RECHECK_INTERVAL: Duration = Duration::from_millis(10);

/// How long one broadcast may wait for queue pressure to ease.
///
/// This bounds admission waiting, not API channel backlog or delivery time.
pub(super) const BROADCAST_WAIT_LIMIT: Duration = Duration::from_millis(100);

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

/// Returns whether all eager targets have busy data queues.
///
/// No targets, or one target with room, lets a local broadcast proceed. The
/// receiver calls this before admitting a broadcast and when retrying one.
/// Pressure can come from local publications, relayed traffic, or slow receivers.
pub(super) fn all_eager_peers_busy(
    state: &proto::State<PublicKey, StdRng>,
    peers: &HashMap<EndpointId, PeerState>,
    topic: TopicId,
) -> bool {
    let mut eager_peers = state.eager_peers(&topic).peekable();
    eager_peers.peek().is_some()
        && eager_peers.all(|peer| peers.get(peer).is_some_and(|peer| peer.queue.is_busy()))
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

/// Receives subscription commands, delaying broadcasts while their targets are busy.
///
/// A held broadcast keeps ownership of its receiver, so later commands on that
/// subscription cannot pass it. Other subscriptions continue independently.
/// Operations use pooled storage; only a held broadcast allocates a retry timer.
#[derive(Debug, Default)]
pub(super) struct SubscriptionCommands {
    pending: FuturesUnordered<RecvFut>,
}

impl SubscriptionCommands {
    /// Registers a subscription's command receiver.
    pub(super) fn insert(&mut self, topic: TopicId, commands: mpsc::Receiver<Command>) {
        let receiver = CommandReceiver {
            topic,
            commands,
            wait: None,
        };
        self.pending.push(receiver.recv_next());
    }

    /// Returns whether all subscription receivers have ended.
    pub(super) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Returns the next command or the end of one subscription receiver.
    ///
    /// `topic_is_blocked(topic)` checks the actor's current queues before
    /// broadcast admission. It is never called for other commands or after a
    /// held broadcast's deadline. Each admitted command returns to the actor
    /// before another is checked, so its queue changes affect the next check.
    ///
    /// `Some((topic, None))` reports one receiver ending; `None` means no
    /// receivers remain. Cancelling this future preserves receivers, commands,
    /// and deadlines: every unfinished receiver is put back before another await.
    pub(super) async fn next(
        &mut self,
        topic_is_blocked: impl Fn(TopicId) -> bool,
    ) -> Option<(TopicId, Option<Command>)> {
        while let Some((receiver, item)) = self.pending.next().await {
            let topic = receiver.topic;
            let Some(item) = item else {
                return Some((topic, None));
            };
            let now = Instant::now();
            if item.is_broadcast() && !receiver.wait_expired(now) && topic_is_blocked(topic) {
                self.pending.push(receiver.retry(item, now));
                continue;
            } else {
                self.pending.push(receiver.recv_next());
                return Some((topic, Some(item)));
            }
        }
        None
    }
}

/// Keeps a subscription's stream, current command, and admission timing together.
#[derive(derive_more::Debug)]
struct CommandReceiver {
    topic: TopicId,
    commands: mpsc::Receiver<Command>,
    wait: Option<(Instant, Pin<Box<Sleep>>, Option<Command>)>,
}

impl CommandReceiver {
    fn wait_expired(&self, now: Instant) -> bool {
        match self.wait.as_ref() {
            None => false,
            Some((deadline, _timer, _)) => now >= *deadline,
        }
    }

    fn recv_next(mut self) -> RecvFut {
        self.wait = None;
        RecvFut(Some(self))
    }

    fn retry(mut self, item: Command, now: Instant) -> RecvFut {
        let (deadline, timer) = if let Some((deadline, mut timer, _)) = self.wait.take() {
            let next_retry = (now + BROADCAST_RECHECK_INTERVAL).min(deadline);
            timer.as_mut().reset(next_retry);
            (deadline, timer)
        } else {
            let deadline = now + BROADCAST_WAIT_LIMIT;
            let next_retry = (now + BROADCAST_RECHECK_INTERVAL).min(deadline);
            let timer = Box::pin(time::sleep_until(next_retry));
            (deadline, timer)
        };
        self.wait = Some((deadline, timer, Some(item)));
        RecvFut(Some(self))
    }
}

#[derive(Debug, Default)]
struct RecvFut(Option<CommandReceiver>);

impl Future for RecvFut {
    type Output = (CommandReceiver, Option<Command>);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let receiver = this.0.as_mut().expect("RecvFut polled after completion");
        let item = if let Some((_deadline, timer, item)) = &mut receiver.wait {
            ready!(timer.as_mut().poll(cx));
            item.take()
        } else {
            ready!(receiver.commands.poll_recv(cx))
        };
        let receiver = this.0.take().expect("checked above");
        Poll::Ready((receiver, item))
    }
}
