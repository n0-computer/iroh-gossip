//! The gossip wire: frames, the stream header, and messages.
//!
//! All integers are LEB128 varints. A frame is a length followed by the
//! payload. The first frame on a stream is the header, a kind followed by a
//! list of parameters:
//!
//! ```text
//! frame     = length:varint  payload
//! header    = kind:varint  parameter*
//! parameter = id:varint  length:varint  value:[u8; length]
//! ```
//!
//! Every later frame is a message: a type code, then its fields in postcard.
//! A reader skips a message frame over its size limit, and ignores bytes after
//! the fields it knows. A reader skips unknown header parameters, as they tell
//! what a peer supports. A message of an unknown type fails the connection: a
//! peer may send a new type only after both peers declared support for it in a
//! header parameter.

use std::{io, time::Duration};

use bytes::Bytes;
use iroh::{
    endpoint::{SendStream, VarInt},
    EndpointId,
};
use n0_error::{e, stack_error};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt};
use tracing::debug;

use crate::proto::{hyparview, plumtree, topic, DeliveryScope, Round, TopicId, TreeId};

/// How long a write may make no progress before it fails the connection.
///
/// A peer extends its flow control window only after it read part of it, so
/// the writes to a slow reader block for a while now and then.
pub(super) const WRITE_TIMEOUT: Duration = Duration::from_secs(if cfg!(test) { 5 } else { 60 });

/// How long opening a stream may wait at the stream limit of the peer before it fails the connection.
///
/// The peer raises its limit only once one of our streams ends. With as many
/// topics as the limit, the wait does not end. The limit is the
/// `max_concurrent_uni_streams` of the peer's endpoint, 100 by default. A
/// stalled open of an initial topic only skips the other initial topics.
pub(super) const OPEN_TIMEOUT: Duration = Duration::from_secs(if cfg!(test) { 5 } else { 15 });

/// How long a peer may take to send a header, from the accept of its stream.
///
/// A header that is late fails the connection, as the streams after it wait for it.
pub(super) const HEADER_TIMEOUT: Duration = Duration::from_secs(if cfg!(test) { 3 } else { 10 });

/// The maximum length of a header frame payload, in bytes.
///
/// A longer header fails the connection.
///
/// Headers do not count against the message size limit, so a large auth token fits.
const MAX_HEADER_LEN: usize = 4096;

/// Header kind of the control stream.
const KIND_CONTROL: u64 = 0x00;

/// Header parameter with the 8 byte session id of the sender. Required on the control stream.
const PARAM_SESSION: u64 = 0x01;

/// The maximum length of a control message frame, in bytes.
///
/// A longer one fails the connection.
const MAX_CONTROL_FRAME: u64 = 32;

/// Header kind of a stream of one topic.
///
/// The stream carries both the control and the data messages of the topic.
const KIND_TOPIC: u64 = 0x01;

/// Header parameter with the 32 byte topic id. Required on topic streams.
const PARAM_TOPIC_ID: u64 = 0x00;

/// Header parameter with the auth token of the sender, opaque bytes. Optional.
const PARAM_TOKEN: u64 = 0x02;

/// The message type codes.
mod types {
    pub(super) const SWARM_JOIN: u64 = 0x01;
    pub(super) const SWARM_FORWARD_JOIN: u64 = 0x02;
    pub(super) const SWARM_SHUFFLE: u64 = 0x03;
    pub(super) const SWARM_SHUFFLE_REPLY: u64 = 0x04;
    pub(super) const SWARM_NEIGHBOR_REQUEST: u64 = 0x05;
    pub(super) const SWARM_NEIGHBOR_REPLY: u64 = 0x06;
    pub(super) const SWARM_CLOSE: u64 = 0x07;
    /// Content for the swarm: `content: bytes, round: u16, tree: [u8; 8]`.
    ///
    /// The tree is the first 8 bytes of the BLAKE3 hash of the origin's endpoint id.
    /// Receivers copy it and never derive it.
    pub(super) const GOSSIP_MESSAGE: u64 = 0x10;
    /// Moves the sender to the lazy set of a tree: `tree: [u8; 8]`.
    pub(super) const GOSSIP_PRUNE: u64 = 0x11;
    /// Moves the sender to the eager set of a tree: `id: Option<[u8; 32]>, tree: [u8; 8]`.
    pub(super) const GOSSIP_GRAFT: u64 = 0x12;
    /// Announces messages: `Vec<(id: [u8; 32], round: u16, tree: [u8; 8])>`.
    pub(super) const GOSSIP_IHAVE: u64 = 0x13;
    /// Content for the direct neighbors only: `content: bytes`. It has no round.
    ///
    /// Each scope is a type of its own. A scope that a later version adds is
    /// a new type.
    pub(super) const GOSSIP_MESSAGE_NEIGHBOR: u64 = 0x14;
    /// Control stream: `opened: varint`, the number of topic streams the sender opened.
    pub(super) const GOAWAY: u64 = 0x20;
}

/// The maximum length of a LEB128 varint of a `u64`.
const MAX_VARINT_LEN: usize = 10;

/// A topic message as it goes on the wire.
type WireMessage = topic::Message<EndpointId>;

/// Errors related to message writing
#[allow(missing_docs)]
#[stack_error(derive, add_meta, from_sources)]
#[non_exhaustive]
pub(super) enum WriteError {
    /// Connection error
    #[error("Connection error")]
    Connection {
        #[error(std_err)]
        source: iroh::endpoint::ConnectionError,
    },
    /// Serialization failed
    #[error("Serialization failed")]
    Ser {
        #[error(std_err)]
        source: postcard::Error,
    },
    /// IO error
    #[error("IO error")]
    Io {
        #[error(std_err)]
        source: std::io::Error,
    },
    /// The peer stopped the stream with `code`.
    #[error("the peer stopped the stream with code {code}")]
    Stopped { code: u64 },
    /// A message or a header was over its size limit.
    #[error("message too large")]
    TooLarge {},
    /// A write made no progress for [`WRITE_TIMEOUT`].
    #[error("no write progress for {WRITE_TIMEOUT:?}")]
    Stalled {},
    /// Opening a stream waited for [`OPEN_TIMEOUT`] at the stream limit of the peer.
    #[error("stream limit of the peer reached for {OPEN_TIMEOUT:?}")]
    OpenStalled {},
}

/// Errors related to message reading
#[allow(missing_docs)]
#[stack_error(derive, add_meta, from_sources)]
#[non_exhaustive]
pub(super) enum ReadError {
    /// Deserialization failed
    #[error("Deserialization failed")]
    De {
        #[error(std_err)]
        source: postcard::Error,
    },
    /// IO error
    #[error("IO error")]
    Io {
        #[error(std_err)]
        source: std::io::Error,
    },
    /// A frame was over its size limit, or a length overflowed.
    #[error("frame over the size limit")]
    TooLarge {},
    /// A header of an unknown kind, or without a required parameter, or with an invalid one.
    #[error("malformed header")]
    Malformed {},
    /// A message of a type that the reader does not know.
    #[error("unknown message type {code}")]
    UnknownType { code: u64 },
}

/// The first frame of a stream: the topic of its messages, and the token of the sender.
#[derive(derive_more::Debug, Clone, PartialEq, Eq)]
pub(super) struct StreamHeader {
    pub(super) topic: TopicId,
    /// A credential, so it stays out of logs.
    #[debug("{}", if token.is_some() { "Some(..)" } else { "None" })]
    pub(super) token: Option<Bytes>,
}

/// Reads the payload of a header frame into `buffer`.
///
/// A header over [`MAX_HEADER_LEN`] fails before the reader reads its payload. The
/// whole header must arrive in [`HEADER_TIMEOUT`].
async fn read_header<R: AsyncRead + Unpin>(
    reader: &mut R,
    buffer: &mut Vec<u8>,
) -> Result<(), ReadError> {
    let read = async {
        let len = read_prefix(reader)
            .await?
            .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
        if len > MAX_HEADER_LEN as u64 {
            return Err(e!(ReadError::TooLarge));
        }
        read_payload(reader, buffer, len).await
    };
    n0_future::time::timeout(HEADER_TIMEOUT, read)
        .await
        .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?
}

/// Decodes the payload of a header frame of `kind` into its parameters, in order.
///
/// The decoder keeps a repeated parameter, and [`param`] finds the first.
fn decode_header(mut buf: &[u8], kind: u64) -> Result<Vec<(u64, Bytes)>, ReadError> {
    if take::<u64>(&mut buf)? != kind {
        return Err(e!(ReadError::Malformed));
    }
    let mut params = Vec::new();
    while !buf.is_empty() {
        let id: u64 = take(&mut buf)?;
        let value: &[u8] = take(&mut buf)?;
        params.push((id, Bytes::copy_from_slice(value)));
    }
    Ok(params)
}

/// Returns the first parameter `id` of a header that has exactly `N` bytes.
fn param<const N: usize>(params: &[(u64, Bytes)], id: u64) -> Result<[u8; N], ReadError> {
    params
        .iter()
        .find(|(i, _)| *i == id)
        .and_then(|(_, value)| value.as_ref().try_into().ok())
        .ok_or_else(|| e!(ReadError::Malformed))
}

impl StreamHeader {
    /// Reads the header frame.
    pub(super) async fn read<R: AsyncRead + Unpin>(
        reader: &mut R,
        buffer: &mut Vec<u8>,
    ) -> Result<Self, ReadError> {
        read_header(reader, buffer).await?;
        Self::decode(buffer)
    }

    /// Decodes the payload of a header frame.
    fn decode(buf: &[u8]) -> Result<Self, ReadError> {
        let params = decode_header(buf, KIND_TOPIC)?;
        Ok(Self {
            topic: TopicId::from_bytes(param(&params, PARAM_TOPIC_ID)?),
            token: params
                .into_iter()
                .find_map(|(id, value)| (id == PARAM_TOKEN).then_some(value)),
        })
    }

    /// Writes the header that we send on a stream for `topic`.
    ///
    /// Fails with [`WriteError::TooLarge`], before it writes, if the header is over
    /// [`MAX_HEADER_LEN`].
    pub(super) async fn write(
        topic: TopicId,
        token: Option<&[u8]>,
        stream: &mut SendStream,
        buffer: &mut Vec<u8>,
    ) -> Result<(), WriteError> {
        let mut payload = Vec::new();
        let mut params = vec![(PARAM_TOPIC_ID, topic.as_bytes().as_slice())];
        params.extend(token.map(|token| (PARAM_TOKEN, token)));
        put_header(&mut payload, KIND_TOPIC, &params);
        if payload.len() > MAX_HEADER_LEN {
            return Err(e!(WriteError::TooLarge));
        }
        buffer.clear();
        put_varint(buffer, payload.len() as u64);
        buffer.extend_from_slice(&payload);
        write_all(stream, buffer).await
    }
}

/// The largest auth token that fits in a stream header.
///
/// The header holds the kind, the topic parameter (id, length, 32 bytes), the
/// token id, a two byte token length, and the token.
pub(super) const MAX_TOKEN_LEN: usize = MAX_HEADER_LEN - (1 + (1 + 1 + 32) + 1 + 2);

/// The code of a connection close.
///
/// The orderly closes come first, then the failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CloseCode {
    /// A finished retire.
    ///
    /// This is not 0. noq sends code 0 when the last handle of a connection drops without a
    /// close, as in a crash or a dropped `Gossip`. Such a close must not read as a finished
    /// retire.
    Retired = 1,
    /// The connection belongs to a former instance of the peer, as the peer restarted.
    ///
    /// The loser of a crossed dial retires instead, and closes with `Retired`.
    Stale = 2,
    /// The node shuts down.
    Shutdown = 3,
    /// The peer broke the protocol.
    Protocol = 4,
    /// A write to the peer failed.
    SendFailed = 5,
    /// A queue, a write, a stream open, or a retire made no progress in time.
    Blocked = 6,
}

impl CloseCode {
    /// Returns the code of a close, or `None` for an unknown one.
    pub(super) fn from_code(code: VarInt) -> Option<Self> {
        Some(match code.into_inner() {
            1 => Self::Retired,
            2 => Self::Stale,
            3 => Self::Shutdown,
            4 => Self::Protocol,
            5 => Self::SendFailed,
            6 => Self::Blocked,
            _ => return None,
        })
    }

    pub(super) fn code(self) -> VarInt {
        VarInt::from_u32(self as u32)
    }
}

/// A random id that a node draws once at start, and sends in the header of each control stream.
///
/// The control stream is the first bidirectional stream of a connection. The
/// dialer opens it and writes its header. The acceptor answers with its own.
#[derive(Clone, Copy, PartialEq, Eq, Hash, derive_more::Debug)]
#[debug("{}", hex::encode(_0))]
pub(super) struct Session(pub(super) [u8; 8]);

impl Session {
    pub(super) fn random() -> Self {
        Self(rand::random())
    }

    /// Returns the frame of the control header.
    pub(super) fn to_frame(self) -> Vec<u8> {
        let mut payload = Vec::new();
        put_header(&mut payload, KIND_CONTROL, &[(PARAM_SESSION, &self.0)]);
        framed(&payload)
    }

    pub(super) async fn read<R: AsyncRead + Unpin>(
        reader: &mut R,
        buffer: &mut Vec<u8>,
    ) -> Result<Self, ReadError> {
        read_header(reader, buffer).await?;
        Self::decode(buffer)
    }

    fn decode(buf: &[u8]) -> Result<Self, ReadError> {
        Ok(Self(param(
            &decode_header(buf, KIND_CONTROL)?,
            PARAM_SESSION,
        )?))
    }
}

/// A message on the control stream after the headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ControlMessage {
    /// The writer of the sender stopped, after it opened this number of topic streams.
    Goaway(u64),
}

impl ControlMessage {
    /// Returns the frame of the message.
    pub(super) fn to_frame(self) -> Vec<u8> {
        let Self::Goaway(opened) = self;
        let mut payload = Vec::new();
        put_varint(&mut payload, types::GOAWAY);
        put_varint(&mut payload, opened);
        framed(&payload)
    }

    /// Reads the next message.
    ///
    /// The end of the stream is an error, as are a frame over [`MAX_CONTROL_FRAME`]
    /// and a message of an unknown type.
    pub(super) async fn read<R: AsyncRead + Unpin>(
        reader: &mut R,
        buffer: &mut Vec<u8>,
    ) -> Result<Self, ReadError> {
        let len = read_prefix(reader)
            .await?
            .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
        if len > MAX_CONTROL_FRAME {
            return Err(e!(ReadError::TooLarge));
        }
        read_payload(reader, buffer, len).await?;
        match Self::decode(buffer)? {
            Some(message) => Ok(message),
            None => Err(e!(ReadError::UnknownType {
                code: take(&mut &buffer[..])?
            })),
        }
    }

    fn decode(mut buf: &[u8]) -> Result<Option<Self>, ReadError> {
        let buf = &mut buf;
        Ok(Some(match take(buf)? {
            types::GOAWAY => Self::Goaway(take(buf)?),
            _ => return Ok(None),
        }))
    }
}

/// Returns `payload` as a frame.
fn framed(payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    put_varint(&mut buf, payload.len() as u64);
    buf.extend_from_slice(payload);
    buf
}

/// Appends the payload of a header frame.
fn put_header(buf: &mut Vec<u8>, kind: u64, params: &[(u64, &[u8])]) {
    put_varint(buf, kind);
    for (id, value) in params {
        put_varint(buf, *id);
        put_varint(buf, value.len() as u64);
        buf.extend_from_slice(value);
    }
}

/// Reads the next message of a stream, or `None` at its end.
///
/// Skips frames over `max_message_size` without reading them into memory. A
/// message of an unknown type is an error.
pub(super) async fn read_message<R: AsyncRead + Unpin>(
    reader: &mut R,
    buffer: &mut Vec<u8>,
    max_message_size: usize,
) -> Result<Option<WireMessage>, ReadError> {
    while let Some(len) = read_prefix(reader).await? {
        if len > max_message_size as u64 {
            debug!(
                len,
                max = max_message_size,
                "skip a frame over the size limit"
            );
            let skipped =
                tokio::io::copy(&mut (&mut *reader).take(len), &mut tokio::io::sink()).await?;
            if skipped < len {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
            }
            continue;
        }
        read_payload(reader, buffer, len).await?;
        return match decode_message(buffer)? {
            Some(message) => Ok(Some(message)),
            None => Err(e!(ReadError::UnknownType {
                code: take(&mut &buffer[..])?
            })),
        };
    }
    Ok(None)
}

/// Reads the length prefix of a frame, or `None` at the end of the stream.
async fn read_prefix<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Option<u64>, ReadError> {
    let mut len = 0u64;
    for i in 0..MAX_VARINT_LEN {
        let byte = match reader.read_u8().await {
            Ok(byte) => byte,
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof && i == 0 => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        // The tenth byte holds the last bit of a `u64`.
        len |= u64::from(byte & 0x7f)
            .checked_mul(1 << (7 * i))
            .ok_or_else(|| e!(ReadError::TooLarge))?;
        if byte & 0x80 == 0 {
            return Ok(Some(len));
        }
    }
    Err(e!(ReadError::TooLarge))
}

/// Reads a payload of `len` bytes into the buffer.
///
/// The buffer reserves at most 64 KiB up front, and grows past that only as the bytes arrive.
async fn read_payload<R: AsyncRead + Unpin>(
    reader: &mut R,
    buffer: &mut Vec<u8>,
    len: u64,
) -> Result<(), ReadError> {
    buffer.clear();
    buffer.reserve(len.min(64 * 1024) as usize);
    let read = reader.take(len).read_to_end(buffer).await?;
    if (read as u64) < len {
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
    }
    Ok(())
}

/// A message as it goes on the wire: its type code, then its fields.
///
/// The fields are the postcard encoding of the proto type.
struct WireBody<'a>(&'a WireMessage);

impl Serialize for WireBody<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use hyparview::Message as Swarm;
        use plumtree::Message as Broadcast;
        match self.0 {
            topic::Message::Swarm(message) => match message {
                Swarm::Join(data) => (types::SWARM_JOIN, data).serialize(s),
                Swarm::ForwardJoin(join) => (types::SWARM_FORWARD_JOIN, join).serialize(s),
                Swarm::Shuffle(shuffle) => (types::SWARM_SHUFFLE, shuffle).serialize(s),
                Swarm::ShuffleReply(reply) => (types::SWARM_SHUFFLE_REPLY, reply).serialize(s),
                Swarm::NeighborRequest(request) => {
                    (types::SWARM_NEIGHBOR_REQUEST, request).serialize(s)
                }
                Swarm::NeighborReply(data) => (types::SWARM_NEIGHBOR_REPLY, data).serialize(s),
                Swarm::Close(close) => (types::SWARM_CLOSE, close).serialize(s),
            },
            topic::Message::Gossip(message) => match message {
                Broadcast::Gossip(gossip) => match gossip.scope {
                    DeliveryScope::Swarm(round) => {
                        (types::GOSSIP_MESSAGE, &gossip.content, round, gossip.tree).serialize(s)
                    }
                    DeliveryScope::Neighbors => {
                        (types::GOSSIP_MESSAGE_NEIGHBOR, &gossip.content).serialize(s)
                    }
                },
                Broadcast::Prune(tree) => (types::GOSSIP_PRUNE, tree).serialize(s),
                Broadcast::Graft(graft) => (types::GOSSIP_GRAFT, graft).serialize(s),
                Broadcast::IHave(ihave) => (types::GOSSIP_IHAVE, ihave).serialize(s),
            },
        }
    }
}

/// Returns how many bytes `message` takes encoded, without the frame prefix.
pub(super) fn message_len(message: &WireMessage) -> Result<usize, postcard::Error> {
    postcard::experimental::serialized_size(&WireBody(message))
}

/// Decodes a message, and ignores the bytes after its fields.
///
/// The body is the postcard encoding of the proto type. Fields that a later
/// version appends sit in the bytes after the known fields.
///
/// Returns `None` for a message of an unknown type.
fn decode_message(mut buf: &[u8]) -> Result<Option<WireMessage>, ReadError> {
    use hyparview::Message as Swarm;
    use plumtree::Message as Broadcast;
    let buf = &mut buf;
    // The wire layout of a gossip message is not the serde of `plumtree::Gossip`:
    // the type code holds the scope, and the receiver derives the id from the content.
    let gossip = |content, scope, tree| {
        WireMessage::Gossip(Broadcast::Gossip(plumtree::Gossip::new(
            content, scope, tree,
        )))
    };
    let message = match take(buf)? {
        types::SWARM_JOIN => WireMessage::Swarm(Swarm::Join(take(buf)?)),
        types::SWARM_FORWARD_JOIN => WireMessage::Swarm(Swarm::ForwardJoin(take(buf)?)),
        types::SWARM_SHUFFLE => WireMessage::Swarm(Swarm::Shuffle(take(buf)?)),
        types::SWARM_SHUFFLE_REPLY => WireMessage::Swarm(Swarm::ShuffleReply(take(buf)?)),
        types::SWARM_NEIGHBOR_REQUEST => WireMessage::Swarm(Swarm::NeighborRequest(take(buf)?)),
        types::SWARM_NEIGHBOR_REPLY => WireMessage::Swarm(Swarm::NeighborReply(take(buf)?)),
        types::SWARM_CLOSE => WireMessage::Swarm(Swarm::Close(take(buf)?)),
        types::GOSSIP_MESSAGE => {
            let (content, round, tree): (Bytes, Round, TreeId) = take(buf)?;
            gossip(content, DeliveryScope::Swarm(round), tree)
        }
        types::GOSSIP_MESSAGE_NEIGHBOR => {
            gossip(take(buf)?, DeliveryScope::Neighbors, TreeId::default())
        }
        types::GOSSIP_PRUNE => WireMessage::Gossip(Broadcast::Prune(take(buf)?)),
        types::GOSSIP_GRAFT => WireMessage::Gossip(Broadcast::Graft(take(buf)?)),
        types::GOSSIP_IHAVE => WireMessage::Gossip(Broadcast::IHave(take(buf)?)),
        _ => return Ok(None),
    };
    Ok(Some(message))
}

/// Reads a value from the front of `buf`, and moves `buf` past it.
///
/// Integers are LEB128 varints, as in postcard.
fn take<'a, T: Deserialize<'a>>(buf: &mut &'a [u8]) -> Result<T, postcard::Error> {
    let (value, rest) = postcard::take_from_bytes(buf)?;
    *buf = rest;
    Ok(value)
}

/// Appends `value` as a LEB128 varint.
fn put_varint(buf: &mut Vec<u8>, value: u64) {
    *buf = postcard::to_extend(&value, std::mem::take(buf)).expect("a varint encodes");
}

/// Writes a message as a frame, with `len` as its encoded size from [`message_len`].
pub(super) async fn write_sized_message(
    stream: &mut SendStream,
    message: &WireMessage,
    len: usize,
    buffer: &mut Vec<u8>,
    max_message_size: usize,
) -> Result<(), WriteError> {
    if len > max_message_size {
        return Err(e!(WriteError::TooLarge));
    }
    buffer.clear();
    put_varint(buffer, len as u64);
    let start = buffer.len();
    buffer.resize(start + len, 0u8);
    let encoded = postcard::to_slice(&WireBody(message), &mut buffer[start..])?.len();
    debug_assert_eq!(encoded, len, "the length prefix does not match");
    write_all(stream, buffer).await
}

/// Writes a message as a frame.
#[cfg(test)]
pub(super) async fn write_message(
    stream: &mut SendStream,
    message: &WireMessage,
    buffer: &mut Vec<u8>,
    max_message_size: usize,
) -> Result<(), WriteError> {
    let len = message_len(message)?;
    write_sized_message(stream, message, len, buffer, max_message_size).await
}

/// Writes all of `buffer`.
pub(super) async fn write_all(stream: &mut SendStream, buffer: &[u8]) -> Result<(), WriteError> {
    // Each write takes some bytes, or fails once the peer takes none for long.
    // A slow peer thus keeps the connection, however long the frame takes.
    let mut sent = 0;
    while sent < buffer.len() {
        sent += n0_future::time::timeout(WRITE_TIMEOUT, stream.write(&buffer[sent..]))
            .await
            .map_err(|_| e!(WriteError::Stalled))?
            .map_err(|err| match err {
                iroh::endpoint::WriteError::Stopped(code) => e!(WriteError::Stopped {
                    code: code.into_inner()
                }),
                other => WriteError::from(io::Error::other(other)),
            })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOPIC: [u8; 32] = [0x11; 32];
    /// The id of `hi`, the BLAKE3 hash of its content.
    const ID: &str = "85052e9aab1b67b6622d94a08441b09fd5b7aca61ee360416d70de5da67d86ca";
    /// A tree id.
    const T: &str = "3333333333333333";

    /// Returns a header payload of the topic kind with `params`.
    fn header(params: &[(u64, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        put_header(&mut buf, KIND_TOPIC, params);
        buf
    }

    fn encode(message: &WireMessage) -> Vec<u8> {
        let mut buf = vec![0; message_len(message).unwrap()];
        let len = postcard::to_slice(&WireBody(message), &mut buf)
            .unwrap()
            .len();
        assert_eq!(len, buf.len());
        buf
    }

    /// Decodes hex of a message that must not be skipped.
    fn decode_hex(hex: &str) -> WireMessage {
        decode_message(&hex::decode(hex).unwrap()).unwrap().unwrap()
    }

    fn prune() -> WireMessage {
        decode_hex(&format!("11{T}"))
    }

    /// The frame payload of [`prune`].
    fn prune_bytes() -> Vec<u8> {
        hex::decode(format!("11{T}")).unwrap()
    }

    /// The encodings of every message type, as hex.
    fn vectors() -> Vec<String> {
        let p = hex::encode(iroh::SecretKey::from_bytes(&[5u8; 32]).public().as_bytes());
        vec![
            "0100".to_string(),
            "01010003aabbcc".to_string(),
            // Data of an unknown kind stays as it is.
            "01010702beef".to_string(),
            format!("02{p}0006"),
            format!("03{p}01{p}0006"),
            format!("0401{p}00"),
            format!("0401{p}01070101"),
            "050100".to_string(),
            "0500010003010203".to_string(),
            "0600".to_string(),
            "06010003aabbcc".to_string(),
            "0700".to_string(),
            "0701".to_string(),
            format!("10026869ac02{T}"),
            "14026869".to_string(),
            format!("11{T}"),
            format!("1201{ID}{T}"),
            format!("1200{T}"),
            format!("1301{ID}02{T}"),
        ]
    }

    #[test]
    fn max_token_fills_the_header() {
        let token = vec![0u8; MAX_TOKEN_LEN];
        let payload = header(&[(PARAM_TOPIC_ID, &TOPIC[..]), (PARAM_TOKEN, &token[..])]);
        assert_eq!(payload.len(), MAX_HEADER_LEN);
    }

    #[test]
    fn varint_lengths() {
        for (value, len) in [
            (0, 1),
            (127, 1),
            (128, 2),
            (16383, 2),
            (16384, 3),
            (u64::MAX, 10),
        ] {
            let mut buf = Vec::new();
            put_varint(&mut buf, value);
            assert_eq!(buf.len(), len, "length of {value}");
            let mut slice = &buf[..];
            assert_eq!(take::<u64>(&mut slice).unwrap(), value);
            assert!(slice.is_empty());
        }
    }

    #[test]
    fn every_message_type_roundtrips() {
        let vectors = vectors();
        let kinds: std::collections::BTreeSet<_> = vectors.iter().map(|v| &v[..2]).collect();
        assert_eq!(kinds.len(), 12, "a type has no vector");
        for vector in vectors {
            let message = decode_hex(&vector);
            let bytes = encode(&message);
            assert_eq!(hex::encode(&bytes), vector);
            assert_eq!(message_len(&message).unwrap(), bytes.len());
            assert_eq!(decode_message(&bytes).unwrap(), Some(message));
        }
    }

    /// The receiver takes the id of a message as the hash of its content.
    #[test]
    fn gossip_id_is_the_content_hash() {
        let WireMessage::Gossip(plumtree::Message::Gossip(gossip)) = decode_hex("14026869") else {
            panic!("not a gossip message");
        };
        assert_eq!(gossip.id.to_string(), ID);
    }

    #[test]
    fn header_roundtrips() {
        let payload = header(&[(PARAM_TOPIC_ID, &TOPIC)]);
        assert_eq!(payload.len(), 35);
        let topic = TopicId::from_bytes(TOPIC);
        assert_eq!(StreamHeader::decode(&payload).unwrap().topic, topic);
        // The frame of a header with one topic parameter.
        assert_eq!(
            hex::encode(framed(&payload)),
            format!("230100{}{}", "20", "11".repeat(32))
        );
    }

    /// The token is parameter 2. Our writer sends it after the topic.
    #[test]
    fn header_with_a_token() {
        let payload = header(&[(PARAM_TOPIC_ID, &TOPIC), (PARAM_TOKEN, b"secret")]);
        assert_eq!(
            hex::encode(&payload),
            format!("0100{}{}{}", "20", "11".repeat(32), "0206736563726574")
        );
        let decoded = StreamHeader::decode(&payload).unwrap();
        assert_eq!(decoded.token.as_deref(), Some(&b"secret"[..]));
        // The parameters can come in any order, and an empty token still counts as a token.
        let payload = header(&[(PARAM_TOKEN, b""), (PARAM_TOPIC_ID, &TOPIC)]);
        assert_eq!(
            StreamHeader::decode(&payload).unwrap().token.as_deref(),
            Some(&b""[..])
        );
        // No token parameter, no token.
        let payload = header(&[(PARAM_TOPIC_ID, &TOPIC)]);
        assert_eq!(StreamHeader::decode(&payload).unwrap().token, None);
        // A long token has a two byte length.
        let long = vec![7u8; 300];
        let payload = header(&[(PARAM_TOPIC_ID, &TOPIC), (PARAM_TOKEN, &long)]);
        assert_eq!(payload.len(), 35 + 1 + 2 + 300);
        assert_eq!(
            StreamHeader::decode(&payload).unwrap().token.as_deref(),
            Some(&long[..])
        );
    }

    #[test]
    fn header_debug_hides_the_token() {
        let header = StreamHeader {
            topic: TopicId::from_bytes(TOPIC),
            token: Some(Bytes::from_static(b"hunter2")),
        };
        let debug = format!("{header:?}");
        assert!(!debug.contains("hunter2"), "{debug}");
    }

    #[test]
    fn unknown_parameters_are_skipped() {
        let buf = header(&[
            (27, b"grease"),
            (PARAM_TOPIC_ID, &TOPIC),
            (58, b""),
            (58, b""),
        ]);
        let header = StreamHeader::decode(&buf).unwrap();
        assert_eq!(header.topic, TopicId::from_bytes(TOPIC));
    }

    #[test]
    fn malformed_headers_fail() {
        let mut truncated = header(&[(PARAM_TOPIC_ID, &TOPIC)]);
        truncated.pop();
        let mut unknown_kind = Vec::new();
        put_header(&mut unknown_kind, 0x3f, &[(PARAM_TOPIC_ID, &TOPIC)]);
        let cases = [
            ("empty", vec![], "Deserialization failed"),
            ("unknown kind", unknown_kind, "malformed header"),
            ("truncated", truncated, "Deserialization failed"),
            (
                "short topic",
                header(&[(PARAM_TOPIC_ID, &TOPIC[..31])]),
                "malformed header",
            ),
            ("missing topic", header(&[]), "malformed header"),
        ];
        for (case, buf, expected) in cases {
            let err = StreamHeader::decode(&buf).expect_err(case);
            assert_eq!(err.to_string(), expected, "{case}");
        }
    }

    /// A header with many parameters decodes in one pass.
    #[test]
    fn header_with_many_parameters() {
        let mut params: Vec<(u64, &[u8])> = vec![(PARAM_TOPIC_ID, &TOPIC)];
        params.extend((1..2000).map(|id| (id, &b""[..])));
        assert!(StreamHeader::decode(&header(&params)).is_ok());
    }

    /// A header over the limit fails on its length, before its payload.
    #[tokio::test]
    async fn oversized_header_is_rejected() {
        // The kind and the topic parameter take 35 bytes. The id and length of the filler
        // take 3 bytes.
        let frame = |filler: usize| {
            let filler = vec![0; filler];
            framed(&header(&[(PARAM_TOPIC_ID, &TOPIC[..]), (58, &filler[..])]))
        };
        let fits = frame(MAX_HEADER_LEN - 38);
        assert_eq!(fits.len(), 2 + MAX_HEADER_LEN);
        let ok = StreamHeader::read(&mut &fits[..], &mut Vec::new()).await;
        assert!(ok.is_ok());
        // Only the prefix is there, so the reader did not wait for the payload.
        let mut over = frame(MAX_HEADER_LEN - 37);
        over.truncate(2);
        let mut buffer = Vec::new();
        let err = StreamHeader::read(&mut &over[..], &mut buffer)
            .await
            .unwrap_err();
        assert!(matches!(err, ReadError::TooLarge { .. }), "{err}");
        assert_eq!(buffer.capacity(), 0, "the reader allocated");
    }

    /// A header that does not arrive in time fails.
    #[tokio::test(start_paused = true)]
    async fn silent_header_times_out() {
        let (mut reader, _writer) = tokio::io::duplex(64);
        let err = StreamHeader::read(&mut reader, &mut Vec::new())
            .await
            .unwrap_err();
        assert!(
            matches!(&err, ReadError::Io { source, .. } if source.kind() == io::ErrorKind::TimedOut),
            "{err}"
        );
    }

    fn session() -> Session {
        Session([1, 2, 3, 4, 5, 6, 7, 8])
    }

    #[test]
    fn control_header_vectors() {
        let frame = session().to_frame();
        assert_eq!(hex::encode(&frame), "0b0001080102030405060708");
        assert_eq!(frame.len(), 12);
        assert_eq!(Session::decode(&frame[1..]).unwrap(), session());
        // The reader skips unknown parameters.
        let mut payload = Vec::new();
        put_header(
            &mut payload,
            KIND_CONTROL,
            &[(7, b"a"), (PARAM_SESSION, &session().0)],
        );
        assert_eq!(Session::decode(&payload).unwrap(), session());
    }

    #[test]
    fn malformed_control_headers_fail() {
        let mut payload = Vec::new();
        put_header(&mut payload, KIND_CONTROL, &[(PARAM_SESSION, &[1; 7])]);
        assert!(Session::decode(&payload).is_err(), "short session");
        let mut payload = Vec::new();
        put_header(&mut payload, KIND_CONTROL, &[]);
        assert!(Session::decode(&payload).is_err(), "no session");
        let mut payload = Vec::new();
        put_header(&mut payload, KIND_TOPIC, &[(PARAM_SESSION, &[1; 8])]);
        assert!(Session::decode(&payload).is_err(), "wrong kind");
    }

    #[test]
    fn control_message_vectors() {
        for (message, hex) in [
            (ControlMessage::Goaway(0), "022000"),
            (ControlMessage::Goaway(5), "022005"),
            (ControlMessage::Goaway(300), "0320ac02"),
        ] {
            let frame = message.to_frame();
            assert_eq!(hex::encode(&frame), hex);
            assert_eq!(ControlMessage::decode(&frame[1..]).unwrap(), Some(message));
        }
        assert_eq!(ControlMessage::decode(&[0x7f, 1]).unwrap(), None);
        assert!(ControlMessage::decode(&[0x20]).is_err());
    }

    #[tokio::test]
    async fn control_reader_fails_on_unknown_types_and_limits_frames() {
        let mut bytes = framed(&[0x2f, 1]);
        bytes.extend(ControlMessage::Goaway(1).to_frame());
        let err = ControlMessage::read(&mut &bytes[..], &mut Vec::new()).await;
        assert!(
            matches!(err, Err(ReadError::UnknownType { code: 0x2f, .. })),
            "{err:?}"
        );
        let big = framed(&[0x22; 33]);
        let err = ControlMessage::read(&mut &big[..], &mut Vec::new()).await;
        assert!(matches!(err, Err(ReadError::TooLarge { .. })));
        let end = ControlMessage::read(&mut &[][..], &mut Vec::new()).await;
        assert!(
            matches!(end, Err(ReadError::Io { .. })),
            "the end is an error"
        );
    }

    #[test]
    fn close_codes() {
        for code in [
            CloseCode::Retired,
            CloseCode::Stale,
            CloseCode::Shutdown,
            CloseCode::Protocol,
            CloseCode::SendFailed,
            CloseCode::Blocked,
        ] {
            assert_eq!(CloseCode::from_code(code.code()), Some(code));
        }
        assert_eq!(CloseCode::from_code(VarInt::from_u32(0)), None);
        assert_eq!(CloseCode::from_code(VarInt::from_u32(99)), None);
        assert_eq!(CloseCode::Retired.code(), VarInt::from_u32(1));
        assert_eq!(CloseCode::Blocked.code(), VarInt::from_u32(6));
    }

    #[tokio::test]
    async fn frames_over_the_limit_are_skipped_without_allocating() {
        let mut bytes = framed(&vec![types::GOSSIP_PRUNE as u8; 5000]);
        bytes.extend(framed(&prune_bytes()));
        let mut buffer = Vec::new();
        let reader = &mut &bytes[..];
        let message = read_message(reader, &mut buffer, 100).await.unwrap();
        assert_eq!(message, Some(prune()));
        assert!(
            buffer.capacity() <= 100,
            "the reader allocated the big frame"
        );
        assert_eq!(read_message(reader, &mut buffer, 100).await.unwrap(), None);
    }

    #[tokio::test]
    async fn truncated_frames_fail() {
        let bytes = framed(&[types::SWARM_JOIN as u8, 0]);
        let err = read_message(&mut &bytes[..2], &mut Vec::new(), 100).await;
        assert!(matches!(err, Err(ReadError::Io { .. })), "{err:?}");
        let skipped = framed(&[0; 200]);
        let err = read_message(&mut &skipped[..100], &mut Vec::new(), 10).await;
        assert!(matches!(err, Err(ReadError::Io { .. })), "{err:?}");
    }

    #[tokio::test]
    async fn unknown_types_fail() {
        let mut bytes = framed(&[0x7f, 1, 2, 3]);
        bytes.extend(framed(&[0x08]));
        bytes.extend(framed(&prune_bytes()));
        let message = read_message(&mut &bytes[..], &mut Vec::new(), 100).await;
        assert!(
            matches!(message, Err(ReadError::UnknownType { code: 0x7f, .. })),
            "{message:?}"
        );
    }

    #[test]
    fn trailing_bytes_are_ignored() {
        for (bytes, expected) in [
            ("0100dead", "0100"),
            (&format!("11{T}01"), &format!("11{T}")),
            ("070109", "0701"),
            (&format!("1201{ID}{T}00"), &format!("1201{ID}{T}")),
        ] {
            assert_eq!(hex::encode(encode(&decode_hex(bytes))), expected);
        }
    }

    /// Malformed fields give an error, not a panic.
    #[test]
    fn invalid_messages_fail() {
        let long_round = "1000808080808080808080";
        let bad_scope_round = "1000ffffff7f";
        for bytes in [
            "",
            "01",
            "05",
            "1001",
            "0305",
            "1305",
            "1201",
            // Without the tree.
            "11",
            "113333",
            "10026869ac02",
            "1200",
            "130100",
            long_round,
            bad_scope_round,
        ] {
            let bytes = hex::decode(bytes).unwrap();
            assert!(decode_message(&bytes).is_err(), "{bytes:?}");
        }
    }

    #[test]
    fn unknown_enum_values_read_as_defaults() {
        assert_eq!(
            hex::encode(encode(&decode_hex("050900"))),
            "050100",
            "low priority"
        );
        assert_eq!(hex::encode(encode(&decode_hex("0709"))), "0700", "shutdown");
    }

    /// `max_gossip_size` is the size at the largest round, and no less at any other.
    #[test]
    fn max_gossip_size_covers_every_round() {
        use crate::proto::Scope;
        let size = |content: &Bytes, scope| {
            let gossip = plumtree::Gossip::new(content.clone(), scope, TreeId::default());
            message_len(&WireMessage::Gossip(plumtree::Message::Gossip(gossip))).unwrap()
        };
        for len in [0, 127, 128, 4000, 20_000] {
            let content = Bytes::from(vec![0; len]);
            let max = plumtree::max_gossip_size(len, Scope::Swarm);
            for round in [0, 127, 128, 16_383, 16_384] {
                assert!(size(&content, DeliveryScope::Swarm(Round::from(round))) <= max);
            }
            assert_eq!(
                size(&content, DeliveryScope::Swarm(Round::from(u16::MAX))),
                max
            );
            assert_eq!(
                size(&content, DeliveryScope::Neighbors),
                plumtree::max_gossip_size(len, Scope::Neighbors)
            );
        }
    }

    #[test]
    fn other_decode_errors_are_not_skipped() {
        // Content cut short.
        assert!(decode_message(&hex::decode("10056869").unwrap()).is_err());
        assert!(decode_message(&hex::decode("14056869").unwrap()).is_err());
        // A swarm message without its round.
        assert!(decode_message(&hex::decode("10026869").unwrap()).is_err());
    }
}
