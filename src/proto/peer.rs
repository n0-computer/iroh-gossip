//! The identity and the data of a peer.

use std::{fmt, hash::Hash};

use bytes::Bytes;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// The identifier for a peer.
///
/// The protocol implementation is generic over this trait. When implementing the protocol,
/// a concrete type must be chosen that will then be used throughout the implementation to identify
/// and index individual peers.
///
/// Note that the concrete type will be used in protocol messages. Therefore, implementations of
/// the protocol are only compatible if the same concrete type is supplied for this trait.
///
/// It does not need to be a peer's address, as long as the networking layer can translate
/// the value of its concrete type into an address.
pub trait PeerIdentity: Hash + Eq + Ord + Copy + fmt::Debug + Serialize + DeserializeOwned {}
impl<T> PeerIdentity for T where
    T: Hash + Eq + Ord + Copy + fmt::Debug + Serialize + DeserializeOwned
{
}

/// Opaque binary data that is transmitted on messages that introduce new peers.
///
/// Implementations may use these bytes to supply addresses or other information needed to connect
/// to a peer that is not included in the peer's [`PeerIdentity`].
#[derive(derive_more::Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[debug("PeerData({}b)", self.0.len())]
pub struct PeerData(Bytes);

impl PeerData {
    /// Create a new [`PeerData`] from a byte buffer.
    pub fn new(data: impl Into<Bytes>) -> Self {
        Self(data.into())
    }

    /// Get a reference to the contained [`bytes::Bytes`].
    pub fn inner(&self) -> &bytes::Bytes {
        &self.0
    }

    /// Get the peer data as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}
