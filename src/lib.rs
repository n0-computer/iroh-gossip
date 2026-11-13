#![cfg_attr(feature = "net", doc = include_str!("../README.md"))]
//! Broadcast messages to peers subscribed to a topic
//!
//! The crate runs on an [iroh] endpoint with the `net` feature, through
#![cfg_attr(feature = "net", doc = "[`Gossip`].")]
#![cfg_attr(not(feature = "net"), doc = "`Gossip`.")]
//!
//! The protocol itself is an IO-less state machine. The `experimental-proto-internals`
//! feature makes it public in the `proto` module. Its API may change in any release.
//!
//! [iroh]: https://docs.rs/iroh
#![deny(missing_docs, rustdoc::broken_intra_doc_links)]
#![cfg_attr(iroh_docsrs, feature(doc_cfg))]

#[cfg(feature = "net")]
mod api;
#[cfg(feature = "net")]
mod error;
mod metrics;
#[cfg(feature = "net")]
mod net;
#[cfg(feature = "experimental-proto-internals")]
pub mod proto;
// Without the feature, only the networking code uses the protocol.
#[cfg(all(feature = "net", not(feature = "experimental-proto-internals")))]
mod proto;

pub use self::metrics::Metrics;
#[cfg(any(feature = "net", feature = "experimental-proto-internals"))]
#[doc(inline)]
pub use self::proto::{
    BroadcastConfig, MembershipConfig, ParseIdError, TopicId, DEFAULT_MAX_MESSAGE_SIZE,
    MIN_MAX_MESSAGE_SIZE,
};
#[cfg(feature = "net")]
pub use self::{
    api::{DeliveryScope, Event, GossipReceiver, GossipSender, GossipTopic, JoinOptions, Message},
    error::Error,
    net::{Builder, Gossip, QueueOptions, ALPN},
};
