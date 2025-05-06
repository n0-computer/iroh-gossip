//! Topic handles for sending and receiving on a gossip topic.
//!
//! These are returned from [`super::Gossip`].

use std::{
    collections::{BTreeSet, HashSet},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures_lite::{Stream, StreamExt};
use iroh::NodeId;
use irpc::channel::spsc;
use n0_future::TryStreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use super::Error;
use crate::{net::TOPIC_EVENTS_DEFAULT_CAP, proto::DeliveryScope};

/// Sender for a gossip topic.
#[derive(Debug, Clone)]
pub struct GossipSender(Arc<Mutex<spsc::Sender<Command>>>);

impl GossipSender {
    pub(crate) fn new(sender: spsc::Sender<Command>) -> Self {
        Self(Arc::new(Mutex::new(sender)))
    }

    /// Broadcast a message to all nodes.
    pub async fn broadcast(&self, message: Bytes) -> Result<(), Error> {
        self.send(Command::Broadcast(message)).await?;
        Ok(())
    }

    /// Broadcast a message to our direct neighbors.
    pub async fn broadcast_neighbors(&self, message: Bytes) -> Result<(), Error> {
        self.send(Command::BroadcastNeighbors(message)).await?;
        Ok(())
    }

    /// Join a set of peers.
    pub async fn join_peers(&self, peers: Vec<NodeId>) -> Result<(), Error> {
        self.send(Command::JoinPeers(peers)).await?;
        Ok(())
    }

    async fn send(&self, command: Command) -> Result<(), irpc::channel::SendError> {
        self.0.lock().await.send(command).await?;
        Ok(())
    }
}

/// Subscribed gossip topic.
///
/// This handle is a [`Stream`] of [`Event`]s from the topic, and can be used to send messages.
///
/// It may be split into sender and receiver parts with [`Self::split`].
#[derive(Debug)]
pub struct GossipTopic {
    sender: GossipSender,
    receiver: GossipReceiver,
}

impl GossipTopic {
    pub(crate) fn new(sender: spsc::Sender<Command>, receiver: spsc::Receiver<Event>) -> Self {
        let sender = GossipSender::new(sender);
        Self {
            sender: sender.clone(),
            receiver: GossipReceiver::new(receiver, sender),
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
    pub async fn joined(&mut self) -> Result<(), Error> {
        self.receiver.joined().await
    }

    /// Returns true if we are connected to at least one node.
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
    // stream: EventStream,
    #[debug("EventStream")]
    stream: n0_future::boxed::BoxStream<Result<Event, Error>>,
    _sender: GossipSender,
    neighbors: HashSet<NodeId>,
    joined: bool,
}

impl GossipReceiver {
    pub(crate) fn new(events_rx: spsc::Receiver<Event>, sender: GossipSender) -> Self {
        let stream = events_rx.into_stream().map_err(Error::from);
        let stream = Box::pin(stream);
        // let stream = EventStream {
        //     topic: topic_id,
        //     receiver_id,
        //     inner,
        //     to_actor_tx: api,
        // };
        Self {
            stream,
            neighbors: Default::default(),
            joined: false,
            _sender: sender,
        }
    }

    /// Lists our current direct neighbors.
    pub fn neighbors(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.neighbors.iter().copied()
    }

    /// Waits until we are connected to at least one node.
    ///
    /// This progresses the stream until we received [`GossipEvent::Joined`], which is the first
    /// item emitted on the stream.
    ///
    /// Note that this consumes the [`GossipEvent::Joined`] event. If you want to act on these
    /// initial neighbors, use [`Self::neighbors`] after awaiting [`Self::joined`].
    pub async fn joined(&mut self) -> Result<(), Error> {
        if !self.joined {
            match self.next().await.ok_or(Error::ReceiverClosed)?? {
                Event::Gossip(GossipEvent::Joined(_)) => {}
                _ => {
                    return Err(Error::UnexpectedEvent);
                }
            }
        }
        Ok(())
    }

    /// Returns true if we are connected to at least one node.
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
                Event::Gossip(GossipEvent::Joined(neighbors)) => {
                    self.joined = true;
                    self.neighbors.extend(neighbors.iter().copied());
                }
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
    /// We joined the topic with at least one peer.
    ///
    /// This is the first event on a [`GossipReceiver`] and will only be emitted once.
    Joined(Vec<NodeId>),
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
pub type CommandStream =
    Pin<Box<dyn Stream<Item = Result<Command, irpc::channel::RecvError>> + Send + 'static>>;

/// Send a gossip message
#[derive(Serialize, Deserialize, derive_more::Debug)]
pub enum Command {
    /// Broadcast a message to all nodes in the swarm
    Broadcast(#[debug("Bytes({})", _0.len())] Bytes),
    /// Broadcast a message to all direct neighbors
    BroadcastNeighbors(#[debug("Bytes({})", _0.len())] Bytes),
    /// Connect to a set of peers
    JoinPeers(Vec<NodeId>),
}

/// Options for joining a gossip topic.
#[derive(Serialize, Deserialize, Debug)]
pub struct JoinOptions {
    /// The initial bootstrap nodes
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

// /// Stream of events for a topic.
// #[derive(derive_more::Debug)]
// pub struct EventStream {
//     /// The actual stream polled to return [`Event`]s to the application.
//     #[debug("Stream")]
//     inner: Pin<Box<dyn Stream<Item = Result<Event, Error>> + Send + 'static>>,
//     // /// Channel to the actor task.
//     // ///
//     // /// This is used to handle the receiver being dropped. When all receiver and publishers are
//     // /// gone the topic will be unsubscribed.
//     // to_actor_tx: GossipApi,
//     // /// The topic for which this stream is reporting events.
//     // ///
//     // /// This is sent on drop to the actor to handle the receiver going away.
//     // topic: TopicId,
//     // /// An Id identifying this specific receiver.
//     // ///
//     // /// This is sent on drop to the actor to handle the receiver going away.
//     // receiver_id: ReceiverId,
// }

// impl Stream for EventStream {
//     type Item = Result<Event, Error>;

//     fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
//         self.inner.poll_next(cx)
//     }
// }

// impl Drop for EventStream {
//     fn drop(&mut self) {
//         // NOTE: unexpectedly, this works without a tokio runtime, so we leverage that to avoid yet
//         // another spawned task
//         if let Err(e) = self.to_actor_tx.try_send(ToActor::ReceiverGone {
//             topic: self.topic,
//             receiver_id: self.receiver_id,
//         }) {
//             match e {
//                 mpsc::error::TrySendError::Full(msg) => {
//                     // if we can't immediately inform then try to spawn a task that handles it
//                     if let Ok(handle) = tokio::runtime::Handle::try_current() {
//                         let to_actor_tx = self.to_actor_tx.clone();
//                         handle.spawn(async move {
//                             let _ = to_actor_tx.send(msg).await;
//                         });
//                     } else {
//                         // full but no runtime oh no
//                     }
//                 }
//                 mpsc::error::TrySendError::Closed(_) => {
//                     // we are probably shutting down so ignore the error
//                 }
//             }
//         }
//     }
// }
