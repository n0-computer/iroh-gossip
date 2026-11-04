#![cfg_attr(feature = "net", doc = include_str!("../README.md"))]
//! Broadcast messages to peers subscribed to a topic
//!
//! The crate runs on an [iroh] endpoint with the `net` feature, through [`Gossip`].
//! The [`proto`] module has the protocol as an IO-less state machine, which you can
//! use standalone.
//!
//! [iroh]: https://docs.rs/iroh
#![deny(missing_docs, rustdoc::broken_intra_doc_links)]
#![cfg_attr(iroh_docsrs, feature(doc_cfg))]

#[cfg(feature = "net")]
pub use net::Gossip;
#[cfg(feature = "net")]
#[doc(inline)]
pub use net::GOSSIP_ALPN as ALPN;

#[cfg(feature = "net")]
pub mod api;
#[cfg(feature = "net")]
mod error;
pub mod metrics;
#[cfg(feature = "net")]
pub mod net;
pub mod proto;

#[cfg(feature = "net")]
pub use error::Error;
pub use proto::TopicId;
