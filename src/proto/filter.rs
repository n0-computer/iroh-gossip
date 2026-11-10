//! The message filter, which lets the application accept or reject a received message.

use bytes::Bytes;

use super::{plumtree::MessageId, DeliveryScope, TopicId};

/// The decision of a filter or a token verifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Verdict {
    /// Accepts the message or the stream.
    ///
    /// A filter delivers the message to the local subscribers, and forwards it to peers. A
    /// verifier reads the stream.
    Accept,
    /// Rejects the message or the stream.
    ///
    /// A filter neither delivers nor forwards the message. The message counts as seen, so
    /// the filter does not run again for it. A verifier stops the stream.
    Reject,
}

/// A received message, as the filter sees it.
#[derive(Debug)]
pub struct FilterContext<'a, PI> {
    pub(crate) topic: TopicId,
    pub(crate) id: MessageId,
    pub(crate) content: &'a Bytes,
    pub(crate) scope: DeliveryScope,
    pub(crate) delivered_from: PI,
}

impl<'a, PI: Copy> FilterContext<'a, PI> {
    /// Returns the topic of the message.
    pub fn topic(&self) -> TopicId {
        self.topic
    }

    /// Returns the id of the message, the blake3 hash of its content.
    pub fn id(&self) -> MessageId {
        self.id
    }

    /// Returns the content of the message.
    pub fn content(&self) -> &'a Bytes {
        self.content
    }

    /// Returns whether the message is for the swarm or for the neighbors only.
    pub fn scope(&self) -> DeliveryScope {
        self.scope
    }

    /// Returns the peer that sent the message to us. This is not the author.
    pub fn delivered_from(&self) -> PI {
        self.delivered_from
    }
}
