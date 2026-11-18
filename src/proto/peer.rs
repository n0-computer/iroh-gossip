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
///
/// On the wire, the data is its kind as a varint, then the bytes with a length prefix.
/// A reader keeps data of an unknown kind unchanged.
#[derive(derive_more::Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[debug("PeerData({}, {}b)", self.kind, self.bytes.len())]
pub struct PeerData {
    kind: u64,
    bytes: Bytes,
}

impl PeerData {
    /// The kind of data that holds the addresses of a peer.
    pub const KIND_ADDRESSES: u64 = 0;

    /// Creates a new [`PeerData`] of the addresses kind from a byte buffer.
    pub fn new(data: impl Into<Bytes>) -> Self {
        Self::with_kind(Self::KIND_ADDRESSES, data)
    }

    /// Creates a new [`PeerData`] of `kind` from a byte buffer.
    pub fn with_kind(kind: u64, data: impl Into<Bytes>) -> Self {
        Self {
            kind,
            bytes: data.into(),
        }
    }

    /// Returns the kind of the data.
    pub fn kind(&self) -> u64 {
        self.kind
    }

    /// Get the peer data as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}
