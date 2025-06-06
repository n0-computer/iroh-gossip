//! Utilities for iroh-gossip networking

use std::{
    collections::{hash_map, HashMap},
    io,
    pin::Pin,
};

use bytes::{Bytes, BytesMut};
use iroh::endpoint::{RecvStream, SendStream};
use iroh::{endpoint::Connection, NodeId};
use n0_future::time::{sleep_until, Instant, Sleep};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    task::JoinSet,
};

use super::ProtoMessage;
use crate::proto::{util::TimerMap, TopicId};

/// Errors related to message writing
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// Connection error
    #[error(transparent)]
    Connection(#[from] iroh::endpoint::ConnectionError),
    /// Serialization failed
    #[error(transparent)]
    Ser(#[from] postcard::Error),
    /// IO error
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Message was larger than the configured maximum message size
    #[error("message too large")]
    TooLarge,
}

#[derive(Debug, Serialize, Deserialize)]
enum WireStreamHeader {
    V0(StreamHeader),
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct StreamHeader {
    pub(crate) topic_id: TopicId,
}

impl StreamHeader {
    pub(crate) async fn read(stream: &mut RecvStream) -> Result<Self, ReadError> {
        let len = stream.read_u32().await?;
        let mut buf = vec![0u8; len as usize];
        stream
            .read_exact(&mut buf)
            .await
            .map_err(|err| io::Error::other(err))?;
        let header: WireStreamHeader = postcard::from_bytes(&buf)?;
        let WireStreamHeader::V0(header) = header;
        Ok(header)
    }

    pub(crate) async fn write(self, stream: &mut SendStream) -> Result<(), WriteError> {
        let frame = WireStreamHeader::V0(self);
        let buf = postcard::to_stdvec(&frame)?;
        stream.write_u32(buf.len() as u32).await?;
        stream
            .write_all(&buf)
            .await
            .map_err(|err| io::Error::other(err))?;
        Ok(())
    }
}

/// Write a `ProtoMessage` as a length-prefixed, postcard-encoded message.
pub async fn write_message(
    conn: &Connection,
    streams: &mut HashMap<TopicId, SendStream>,
    finishing: &mut JoinSet<()>,
    buffer: &mut BytesMut,
    frame: &ProtoMessage,
    max_message_size: usize,
) -> Result<(), WriteError> {
    tracing::debug!(topic=?frame.topic,"write message {frame:?}");
    let mut entry = streams.entry(frame.topic);
    let stream = match entry {
        hash_map::Entry::Occupied(ref mut stream) => stream.get_mut(),
        hash_map::Entry::Vacant(entry) => {
            let mut stream = conn.open_uni().await?;
            let header = StreamHeader {
                topic_id: frame.topic,
            };
            header.write(&mut stream).await?;
            tracing::debug!(topic=?frame.topic,"send stream opened");
            entry.insert(stream)
        }
    };
    let len = postcard::experimental::serialized_size(&frame.message)?;
    if len >= max_message_size {
        return Err(WriteError::TooLarge);
    }

    let is_last = frame.message.is_disconnect();
    buffer.clear();
    buffer.resize(len, 0u8);
    let slice = postcard::to_slice(&frame.message, buffer)?;
    stream.write_u32(len as u32).await?;
    tracing::trace!("wrote len {len}");
    stream
        .write_all(slice)
        .await
        .map_err(|err| io::Error::other(err))?;
    tracing::trace!("wrote message len {}", slice.len());

    if is_last {
        let topic = frame.topic;
        tracing::debug!(?topic, "send stream closing");
        let mut stream = streams.remove(&frame.topic).expect("just checked");
        stream.finish().ok();
        // TODO(Frando): Reconsider how we are doing this.
        finishing.spawn(async move {
            stream.stopped().await.ok();
            tracing::debug!(?topic, "send stream closed");
        });
    }

    Ok(())
}

/// Errors related to message reading
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    /// Deserialization failed
    #[error(transparent)]
    De(#[from] postcard::Error),
    /// IO error
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Message was larger than the configured maximum message size
    #[error("message too large")]
    TooLarge,
}

/// Read a length-prefixed message and decode as `ProtoMessage`;
pub async fn read_message(
    reader: &mut RecvStream,
    buffer: &mut BytesMut,
    max_message_size: usize,
) -> Result<Option<crate::proto::topic::Message<NodeId>>, ReadError> {
    match read_lp(reader, buffer, max_message_size).await? {
        None => Ok(None),
        Some(data) => {
            let message = postcard::from_bytes(&data)?;
            Ok(Some(message))
        }
    }
}

/// Reads a length prefixed message.
///
/// # Returns
///
/// The message as raw bytes.  If the end of the stream is reached and there is no partial
/// message, returns `None`.
pub async fn read_lp(
    reader: &mut RecvStream,
    buffer: &mut BytesMut,
    max_message_size: usize,
) -> Result<Option<Bytes>, ReadError> {
    tracing::debug!("read_lp in");
    let size = match reader.read_u32().await {
        Ok(size) => size,
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    tracing::debug!("read_lp read size {size}");
    // let mut reader = reader.take(size as u64);
    let size = usize::try_from(size).map_err(|_| ReadError::TooLarge)?;
    if size > max_message_size {
        return Err(ReadError::TooLarge);
    }
    buffer.resize(size, 0u8);
    tracing::debug!("now read message len {}", buffer.len());
    reader
        .read_exact(&mut buffer[..])
        .await
        .map_err(|err| io::Error::other(err))?;
    tracing::debug!("read_lp read message");
    // loop {
    //     let r = reader.read_buf(buffer).await?;
    //     if r == 0 {
    //         break;
    //     }
    // }
    Ok(Some(buffer.split_to(size).freeze()))
}

/// A [`TimerMap`] with an async method to wait for the next timer expiration.
#[derive(Debug)]
pub struct Timers<T> {
    next: Option<(Instant, Pin<Box<Sleep>>)>,
    map: TimerMap<T>,
}

impl<T> Default for Timers<T> {
    fn default() -> Self {
        Self {
            next: None,
            map: TimerMap::default(),
        }
    }
}

impl<T> Timers<T> {
    /// Creates a new timer map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a new entry at the specified instant
    pub fn insert(&mut self, instant: Instant, item: T) {
        self.map.insert(instant, item);
    }

    fn reset(&mut self) {
        self.next = self
            .map
            .first()
            .map(|instant| (*instant, Box::pin(sleep_until(*instant))))
    }

    /// Waits for the next timer to elapse.
    pub async fn wait_next(&mut self) -> Instant {
        self.reset();
        match self.next.as_mut() {
            Some((instant, sleep)) => {
                sleep.await;
                *instant
            }
            None => std::future::pending().await,
        }
    }

    /// Pops the earliest timer that expires at or before `now`.
    pub fn pop_before(&mut self, now: Instant) -> Option<(Instant, T)> {
        self.map.pop_before(now)
    }
}
