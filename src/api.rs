//! Public API for using iroh-gossip

use std::{
    collections::{BTreeSet, HashSet},
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use iroh_base::EndpointId;
use n0_error::e;
use n0_future::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{proto::DeliveryScope, Error};

/// Default channel capacity for topic subscription channels (one per topic)
const TOPIC_EVENTS_DEFAULT_CAP: usize = 2048;

/// Sender for a gossip topic.
///
/// Each broadcast is checked before the protocol processes it. When the data
/// sections of all eager-peer send queues are half full, it waits for pressure
/// to ease, up to 100 ms. Commands behind it in the same subscription keep their
/// order and wait with it, including joins and the leave when its last handle drops.
/// Other subscriptions continue independently.
///
/// Sending a command waits for space in the subscription's command channel.
/// The 100 ms limit bounds one broadcast's admission wait, not this API call,
/// commands queued ahead of it, or delivery to a peer.
///
/// See [`QueueOptions`] for what this means for your app.
///
/// [`QueueOptions`]: crate::net::QueueOptions
#[derive(Debug, Clone)]
pub struct GossipSender {
    sender: mpsc::Sender<Command>,
    max_content: usize,
}

impl GossipSender {
    pub(crate) fn new(sender: mpsc::Sender<Command>, max_content: usize) -> Self {
        Self {
            sender,
            max_content,
        }
    }

    /// Returns the largest message content that [`Self::broadcast`] takes.
    ///
    /// It is [`Builder::max_message_size`] less the encoding, about 40 bytes,
    /// and holds for [`Self::broadcast_neighbors`] too.
    ///
    /// [`Builder::max_message_size`]: crate::net::Builder::max_message_size
    pub fn max_content_size(&self) -> usize {
        self.max_content
    }

    /// Returns an error if the message content is over the limit.
    fn check_size(&self, message: &Bytes) -> Result<(), Error> {
        if message.len() > self.max_content {
            return Err(e!(Error::TooLarge {
                size: message.len(),
                max_content: self.max_content
            }));
        }
        Ok(())
    }

    /// Broadcasts a message to all endpoints.
    ///
    /// `Ok` means that the node accepted the message, not that a peer received it.
    /// Delivery is best effort, and the order of messages from different senders is not defined.
    /// Your own broadcast does not come back as [`Event::Received`].
    ///
    /// A peer ignores a message with the same content as one that it saw in the last 90 seconds
    /// (the default of [`PlumtreeConfig::message_id_retention`]). To send the same payload twice,
    /// add a nonce or a sequence number to the content.
    ///
    /// # Errors
    ///
    /// Returns [`Error::TooLarge`] if the content is over [`Self::max_content_size`],
    /// and [`Error::Closed`] if the gossip actor stopped.
    ///
    /// [`PlumtreeConfig::message_id_retention`]: crate::proto::PlumtreeConfig::message_id_retention
    pub async fn broadcast(&self, message: Bytes) -> Result<(), Error> {
        self.check_size(&message)?;
        self.send(Command::Broadcast(message)).await?;
        Ok(())
    }

    /// Broadcasts a message to our direct neighbors.
    ///
    /// The neighbors do not relay the message. A neighbor whose send queue is full does not get
    /// it, and there is no repair or retransmission for neighbor-only broadcasts.
    ///
    /// # Errors
    ///
    /// Returns [`Error::TooLarge`] and [`Error::Closed`] as [`Self::broadcast`] does.
    pub async fn broadcast_neighbors(&self, message: Bytes) -> Result<(), Error> {
        self.check_size(&message)?;
        self.send(Command::BroadcastNeighbors(message)).await?;
        Ok(())
    }

    /// Joins the topic through `peers`.
    ///
    /// A peer that is already a neighbor gets no join, as it would forward the
    /// join to all its neighbors. A join without an answer goes out once more
    /// after 10 seconds.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Closed`] if the gossip actor stopped.
    pub async fn join_peers(&self, peers: Vec<EndpointId>) -> Result<(), Error> {
        self.send(Command::JoinPeers(peers)).await?;
        Ok(())
    }

    async fn send(&self, command: Command) -> Result<(), Error> {
        self.sender
            .send(command)
            .await
            .map_err(|_| e!(Error::Closed))
    }
}

/// Subscribed gossip topic.
///
/// This handle is a [`Stream`] of [`Event`]s from the topic, and can be used to send messages.
///
/// Once the [`GossipTopic`] is dropped, the network actor will leave the gossip topic.
///
/// It may be split into sender and receiver parts with [`Self::split`]. In this case, the topic will
/// be left once both the [`GossipSender`] and [`GossipReceiver`] halves are dropped.
#[derive(Debug)]
pub struct GossipTopic {
    sender: GossipSender,
    receiver: GossipReceiver,
}

impl GossipTopic {
    pub(crate) fn new(
        sender: mpsc::Sender<Command>,
        receiver: mpsc::Receiver<Event>,
        max_content: usize,
    ) -> Self {
        let sender = GossipSender::new(sender, max_content);
        Self {
            sender,
            receiver: GossipReceiver::new(receiver),
        }
    }

    /// Returns the largest message content that [`Self::broadcast`] takes.
    ///
    /// See [`GossipSender::max_content_size`].
    pub fn max_content_size(&self) -> usize {
        self.sender.max_content_size()
    }

    /// Splits `self` into [`GossipSender`] and [`GossipReceiver`] parts.
    pub fn split(self) -> (GossipSender, GossipReceiver) {
        (self.sender, self.receiver)
    }

    /// Sends a message to all peers.
    ///
    /// # Errors
    ///
    /// Returns [`Error::TooLarge`] and [`Error::Closed`] as [`GossipSender::broadcast`] does.
    pub async fn broadcast(&mut self, message: Bytes) -> Result<(), Error> {
        self.sender.broadcast(message).await
    }

    /// Sends a message to our direct neighbors in the swarm.
    ///
    /// # Errors
    ///
    /// Returns [`Error::TooLarge`] and [`Error::Closed`] as [`GossipSender::broadcast`] does.
    pub async fn broadcast_neighbors(&mut self, message: Bytes) -> Result<(), Error> {
        self.sender.broadcast_neighbors(message).await
    }

    /// Lists our current direct neighbors.
    pub fn neighbors(&self) -> impl Iterator<Item = EndpointId> + '_ {
        self.receiver.neighbors()
    }

    /// Waits until we are connected to at least one endpoint.
    ///
    /// See [`GossipReceiver::joined`] for details.
    pub async fn joined(&mut self) -> Result<(), Error> {
        self.receiver.joined().await
    }

    /// Returns `true` if we are connected to at least one endpoint.
    pub fn is_joined(&self) -> bool {
        self.receiver.is_joined()
    }
}

impl Stream for GossipTopic {
    type Item = Result<Event, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.receiver).poll_next(cx)
    }
}

/// Receiver for gossip events on a topic.
///
/// This is a [`Stream`] of [`Event`]s emitted from the topic.
#[derive(Debug)]
pub struct GossipReceiver {
    events_rx: mpsc::Receiver<Event>,
    neighbors: HashSet<EndpointId>,
}

impl GossipReceiver {
    pub(crate) fn new(events_rx: mpsc::Receiver<Event>) -> Self {
        Self {
            events_rx,
            neighbors: Default::default(),
        }
    }

    /// Lists our current direct neighbors.
    ///
    /// This list follows the [`Event::NeighborUp`] and [`Event::NeighborDown`] events that this
    /// receiver read. It is out of date until you poll the stream, and after an
    /// [`Event::Lagged`].
    pub fn neighbors(&self) -> impl Iterator<Item = EndpointId> + '_ {
        self.neighbors.iter().copied()
    }

    /// Waits until we are connected to at least one endpoint.
    ///
    /// Progresses the event stream to the first [`Event::NeighborUp`] event.
    ///
    /// Note that this consumes this initial `NeighborUp` event. If you want to track
    /// neighbors, use [`Self::neighbors`] after awaiting [`Self::joined`], and then
    /// continue to track `NeighborUp` events on the event stream.
    pub async fn joined(&mut self) -> Result<(), Error> {
        while !self.is_joined() {
            let _event = self.next().await.ok_or(e!(Error::Closed))??;
        }
        Ok(())
    }

    /// Returns `true` if we are connected to at least one endpoint.
    pub fn is_joined(&self) -> bool {
        !self.neighbors.is_empty()
    }
}

impl Stream for GossipReceiver {
    type Item = Result<Event, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let item = std::task::ready!(self.events_rx.poll_recv(cx));
        if let Some(item) = &item {
            match item {
                Event::NeighborUp(endpoint_id) => {
                    self.neighbors.insert(*endpoint_id);
                }
                Event::NeighborDown(endpoint_id) => {
                    self.neighbors.remove(endpoint_id);
                }
                _ => {}
            }
        }
        Poll::Ready(item.map(Ok))
    }
}

/// Events emitted from a gossip topic.
///
/// These are the events emitted from a [`GossipReceiver`].
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Event {
    /// We have a new, direct neighbor in the swarm membership layer for this topic.
    NeighborUp(EndpointId),
    /// We dropped direct neighbor in the swarm membership layer for this topic.
    NeighborDown(EndpointId),
    /// We received a gossip message for this topic.
    Received(Message),
    /// We missed some events because our [`GossipReceiver`] was not progressing fast enough.
    ///
    /// The missed events can include [`Event::Received`], [`Event::NeighborUp`], and
    /// [`Event::NeighborDown`]. The subscription stays open. See
    /// [`JoinOptions::subscription_capacity`] to set the capacity of the channel.
    Lagged,
}

impl From<crate::proto::Event<EndpointId>> for Event {
    fn from(event: crate::proto::Event<EndpointId>) -> Self {
        match event {
            crate::proto::Event::NeighborUp(endpoint_id) => Self::NeighborUp(endpoint_id),
            crate::proto::Event::NeighborDown(endpoint_id) => Self::NeighborDown(endpoint_id),
            crate::proto::Event::Received(message) => Self::Received(Message {
                content: message.content,
                scope: message.scope,
                delivered_from: message.delivered_from,
            }),
        }
    }
}

/// A gossip message
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, derive_more::Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Message {
    /// The content of the message
    #[debug("Bytes({})", self.content.len())]
    pub content: Bytes,
    /// The scope of the message.
    ///
    /// It tells whether a direct neighbor sent the message to its neighbors, or the swarm gossiped
    /// it.
    pub scope: DeliveryScope,
    /// The endpoint that delivered the message. This is not the same as the original author.
    ///
    /// A gossip message has no author field, and the content is not signed. If you need to know
    /// the author, or need to trust the content, sign it in your app.
    pub delivered_from: EndpointId,
}

/// Command for a gossip topic.
#[derive(derive_more::Debug, Clone)]
pub(crate) enum Command {
    /// Broadcasts a message to all endpoints in the swarm.
    Broadcast(#[debug("Bytes({})", _0.len())] Bytes),
    /// Broadcasts a message to all direct neighbors.
    BroadcastNeighbors(#[debug("Bytes({})", _0.len())] Bytes),
    /// Connects to a set of peers.
    JoinPeers(Vec<EndpointId>),
}

impl Command {
    /// Returns whether this command publishes a message.
    pub(crate) fn is_broadcast(&self) -> bool {
        matches!(self, Self::Broadcast(_) | Self::BroadcastNeighbors(_))
    }
}

/// Options for joining a gossip topic.
///
/// Build it with [`Self::with_bootstrap`] or [`Default`], and the methods that set an option.
/// New options can come in a minor version, so you cannot construct it with a struct literal.
#[derive(Debug)]
#[non_exhaustive]
pub struct JoinOptions {
    /// The initial bootstrap endpoints.
    pub bootstrap: BTreeSet<EndpointId>,
    /// The number of events that a subscription buffers for a reader that falls behind.
    ///
    /// A slow reader never blocks the gossip actor. When the buffer is full, the node drops
    /// the oldest events and the reader receives a [`Event::Lagged`]. The
    /// subscription stays open. 0 counts as 1.
    ///
    /// After a `Lagged`, the neighbors that [`GossipReceiver::neighbors`] reports can be wrong.
    /// If you need the exact state, subscribe again.
    pub subscription_capacity: usize,
}

impl Default for JoinOptions {
    /// Returns options with no bootstrap peers and the default subscription capacity.
    fn default() -> Self {
        Self::with_bootstrap([])
    }
}

impl JoinOptions {
    /// Creates [`JoinOptions`] with `endpoints` as bootstrap peers.
    pub fn with_bootstrap(endpoints: impl IntoIterator<Item = EndpointId>) -> Self {
        Self {
            bootstrap: endpoints.into_iter().collect(),
            subscription_capacity: TOPIC_EVENTS_DEFAULT_CAP,
        }
    }

    /// Sets the number of events that a subscription buffers, see [`Self::subscription_capacity`].
    pub fn with_subscription_capacity(mut self, capacity: usize) -> Self {
        self.subscription_capacity = capacity;
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::api::GossipTopic;

    #[test]
    fn ensure_gossip_topic_is_sync() {
        #[allow(unused)]
        fn get() -> GossipTopic {
            unimplemented!()
        }
        #[allow(unused)]
        fn check(_t: impl Sync) {}
        #[allow(unused)]
        fn foo() {
            check(get());
        }
    }
}
