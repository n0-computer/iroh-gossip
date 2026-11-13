//! The error type of the crate.

/// An error of [`Gossip`] and of the topic handles, such as [`GossipTopic`].
///
/// [`Gossip`]: crate::Gossip
/// [`GossipTopic`]: crate::GossipTopic
#[n0_error::stack_error(derive, add_meta)]
#[non_exhaustive]
pub enum Error {
    /// The gossip actor stopped.
    ///
    /// The actor stops after [`Gossip::shutdown`], and when the last clone of
    /// [`Gossip`] drops.
    ///
    /// [`Gossip`]: crate::Gossip
    /// [`Gossip::shutdown`]: crate::Gossip::shutdown
    #[error("the gossip actor stopped")]
    Closed,
    /// The message content is over the limit, see [`GossipSender::max_content_size`].
    ///
    /// [`GossipSender::max_content_size`]: crate::GossipSender::max_content_size
    #[error("message of {size} bytes is over the limit of {max_content} bytes")]
    TooLarge {
        /// The size of the content.
        size: usize,
        /// The largest content that a broadcast takes.
        max_content: usize,
    },
}
