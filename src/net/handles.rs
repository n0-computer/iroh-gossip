//! Topic handles for sending and receiving on a gossip topic.
//!
//! These are returned from [`super::Gossip`].

use std::{
    collections::{BTreeSet, HashSet},
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures_lite::{Stream, StreamExt};
use iroh::NodeId;
use serde::{Deserialize, Serialize};

use super::{Error, EventStream};
use crate::{net::TOPIC_EVENTS_DEFAULT_CAP, proto::DeliveryScope};

/// Sender for a gossip topic.
#[derive(Debug, Clone)]
pub struct GossipSender(async_channel::Sender<Command>);

impl GossipSender {
    pub(crate) fn new(sender: async_channel::Sender<Command>) -> Self {
        Self(sender)
    }

    /// Broadcasts a message to all nodes.
    pub async fn broadcast(&self, message: Bytes) -> Result<(), Error> {
        self.0.send(Command::Broadcast(message)).await?;
        Ok(())
    }

    /// Broadcasts a message to our direct neighbors.
    pub async fn broadcast_neighbors(&self, message: Bytes) -> Result<(), Error> {
        self.0.send(Command::BroadcastNeighbors(message)).await?;
        Ok(())
    }

    /// Joins a set of peers.
    pub async fn join_peers(&self, peers: Vec<NodeId>) -> Result<(), Error> {
        self.0.send(Command::JoinPeers(peers)).await?;
        Ok(())
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
    pub(crate) fn new(sender: async_channel::Sender<Command>, receiver: EventStream) -> Self {
        Self {
            sender: GossipSender::new(sender),
            receiver: GossipReceiver::new(receiver),
        }
    }

    /// Splits `self` into [`GossipSender`] and [`GossipReceiver`] parts.
    pub fn split(self) -> (GossipSender, GossipReceiver) {
        (self.sender, self.receiver)
    }

    /// Sends a message to all peers.
    pub async fn broadcast(&self, message: Bytes) -> Result<(), Error> {
        self.sender.broadcast(message).await
    }

    /// Sends a message to our direct neighbors in the swarm.
    pub async fn broadcast_neighbors(&self, message: Bytes) -> Result<(), Error> {
        self.sender.broadcast_neighbors(message).await
    }

    /// Waits until we are connected to at least one node.
    ///
    /// See [`GossipReceiver::joined`] for details.
    pub async fn joined(&mut self) -> Result<(), Error> {
        self.receiver.joined().await
    }

    /// Returns `true` if we are connected to at least one node.
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
#[derive(derive_more::Debug)]
pub struct GossipReceiver {
    #[debug("EventStream")]
    stream: EventStream,
    neighbors: HashSet<NodeId>,
}

impl GossipReceiver {
    pub(crate) fn new(events_rx: EventStream) -> Self {
        Self {
            stream: events_rx,
            neighbors: Default::default(),
        }
    }

    /// Lists our current direct neighbors.
    pub fn neighbors(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.neighbors.iter().copied()
    }

    /// Waits until we are connected to at least one node.
    ///
    /// Progresses the event stream to the first [`GossipEvent::NeighborUp`] event.
    ///
    /// Note that this consumes this initial `NeighborUp` event. If you want to track
    /// neighbors, use [`Self::neighbors`] after awaiting [`Self::joined`], and then
    /// continue to track `NeighborUp` events on the event stream.
    pub async fn joined(&mut self) -> Result<(), Error> {
        while !self.is_joined() {
            let _event = self.next().await.ok_or(Error::ReceiverClosed)??;
        }
        Ok(())
    }

    /// Returns `true` if we are connected to at least one node.
    pub fn is_joined(&self) -> bool {
        !self.neighbors.is_empty()
    }
}

impl Stream for GossipReceiver {
    type Item = Result<Event, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let item = std::task::ready!(Pin::new(&mut self.stream).poll_next(cx));
        if let Some(Ok(item)) = &item {
            match item {
                Event::Gossip(GossipEvent::NeighborUp(node_id)) => {
                    self.neighbors.insert(*node_id);
                }
                Event::Gossip(GossipEvent::NeighborDown(node_id)) => {
                    self.neighbors.remove(node_id);
                }
                _ => {}
            }
        }
        Poll::Ready(item)
    }
}

/// Events emitted from a gossip topic with a lagging notification.
///
/// This is the item of the [`GossipReceiver`] stream. It wraps the actual gossip events to also
/// provide a notification if we missed gossip events for the topic.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum Event {
    /// We received an event.
    Gossip(GossipEvent),
    /// We missed some messages because our [`GossipReceiver`] was not progressing fast enough.
    Lagged,
}

/// Events emitted from a gossip topic.
///
/// These are the events emitted from a [`GossipReceiver`], wrapped in [`Event::Gossip`].
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum GossipEvent {
    /// We have a new, direct neighbor in the swarm membership layer for this topic.
    NeighborUp(NodeId),
    /// We dropped direct neighbor in the swarm membership layer for this topic.
    NeighborDown(NodeId),
    /// We received a gossip message for this topic.
    Received(Message),
}

impl From<crate::proto::Event<NodeId>> for GossipEvent {
    fn from(event: crate::proto::Event<NodeId>) -> Self {
        match event {
            crate::proto::Event::NeighborUp(node_id) => Self::NeighborUp(node_id),
            crate::proto::Event::NeighborDown(node_id) => Self::NeighborDown(node_id),
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
pub struct Message {
    /// The content of the message
    #[debug("Bytes({})", self.content.len())]
    pub content: Bytes,
    /// The scope of the message.
    /// This tells us if the message is from a direct neighbor or actual gossip.
    pub scope: DeliveryScope,
    /// The node that delivered the message. This is not the same as the original author.
    pub delivered_from: NodeId,
}

/// A stream of commands for a gossip subscription.
pub type CommandStream = Pin<Box<dyn Stream<Item = Command> + Send + Sync + 'static>>;

/// Command for a gossip topic.
#[derive(Serialize, Deserialize, derive_more::Debug)]
pub enum Command {
    /// Broadcasts a message to all nodes in the swarm.
    Broadcast(#[debug("Bytes({})", _0.len())] Bytes),
    /// Broadcasts a message to all direct neighbors.
    BroadcastNeighbors(#[debug("Bytes({})", _0.len())] Bytes),
    /// Connects to a set of peers.
    JoinPeers(Vec<NodeId>),
}

/// Options for joining a gossip topic.
#[derive(Serialize, Deserialize, Debug)]
pub struct JoinOptions {
    /// The initial bootstrap nodes.
    pub bootstrap: BTreeSet<NodeId>,
    /// The maximum number of messages that can be buffered in a subscription.
    ///
    /// If this limit is reached, the subscriber will receive a `Lagged` response,
    /// the message will be dropped, and the subscriber will be closed.
    ///
    /// This is to prevent a single slow subscriber from blocking the dispatch loop.
    /// If a subscriber is lagging, it should be closed and re-opened.
    pub subscription_capacity: usize,
}

impl JoinOptions {
    /// Creates [`JoinOptions`] with the provided bootstrap nodes and the default subscription
    /// capacity.
    pub fn with_bootstrap(nodes: impl IntoIterator<Item = NodeId>) -> Self {
        Self {
            bootstrap: nodes.into_iter().collect(),
            subscription_capacity: TOPIC_EVENTS_DEFAULT_CAP,
        }
    }
}
