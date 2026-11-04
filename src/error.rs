//! The error type of the crate.

/// An error of [`Gossip`] and of the topic handles in [`api`].
///
/// [`Gossip`]: crate::Gossip
/// [`api`]: crate::api
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
    /// The join got no neighbor from its bootstrap peers, see [`Event::JoinFailed`].
    ///
    /// [`Event::JoinFailed`]: crate::api::Event::JoinFailed
    #[error("join failed: no bootstrap peer could be joined")]
    JoinFailed,
    /// The message content is over the limit, see [`GossipSender::max_content_size`].
    ///
    /// [`GossipSender::max_content_size`]: crate::api::GossipSender::max_content_size
    #[error("message of {size} bytes is over the limit of {max_content} bytes")]
    TooLarge {
        /// The size of the content.
        size: usize,
        /// The largest content that a broadcast takes.
        max_content: usize,
    },
    /// The auth token does not fit in a stream header, see [`JoinOptions::auth_token`].
    ///
    /// [`JoinOptions::auth_token`]: crate::api::JoinOptions::auth_token
    #[error("auth token of {size} bytes is over the limit of {max_token} bytes")]
    TokenTooLarge {
        /// The size of the token.
        size: usize,
        /// The largest token that fits in the header of a stream.
        max_token: usize,
    },
}
