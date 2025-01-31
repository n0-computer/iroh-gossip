//! Utilities for iroh-gossip networking

use std::{io, pin::Pin, time::Instant};

use bytes::{Bytes, BytesMut};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    time::{sleep_until, Sleep},
};

use super::ProtoMessage;
use crate::proto::util::TimerMap;

/// Errors related to message writing
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
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

/// Write a `ProtoMessage` as a length-prefixed, postcard-encoded message.
pub async fn write_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    buffer: &mut BytesMut,
    frame: &ProtoMessage,
    max_message_size: usize,
) -> Result<(), WriteError> {
    let len = postcard::experimental::serialized_size(&frame)?;
    if len >= max_message_size {
        return Err(WriteError::TooLarge);
    }

    buffer.clear();
    buffer.resize(len, 0u8);
    let slice = postcard::to_slice(&frame, buffer)?;
    writer.write_u32(len as u32).await?;
    writer.write_all(slice).await?;
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
    reader: impl AsyncRead + Unpin,
    buffer: &mut BytesMut,
    max_message_size: usize,
) -> Result<Option<ProtoMessage>, ReadError> {
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
    mut reader: impl AsyncRead + Unpin,
    buffer: &mut BytesMut,
    max_message_size: usize,
) -> Result<Option<Bytes>, ReadError> {
    let size = match reader.read_u32().await {
        Ok(size) => size,
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    let mut reader = reader.take(size as u64);
    let size = usize::try_from(size).map_err(|_| ReadError::TooLarge)?;
    if size > max_message_size {
        return Err(ReadError::TooLarge);
    }
    buffer.reserve(size);
    loop {
        let r = reader.read_buf(buffer).await?;
        if r == 0 {
            break;
        }
    }
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
    /// Create a new timer map
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a new entry at the specified instant
    pub fn insert(&mut self, instant: Instant, item: T) {
        self.map.insert(instant, item);
    }

    fn reset(&mut self) {
        self.next = self
            .map
            .first()
            .map(|(instant, _)| (*instant, Box::pin(sleep_until((*instant).into()))))
    }

    /// Wait for the next timer to expire and return an iterator of all expired timers
    ///
    /// If the [TimerMap] is empty, this will return a future that is pending forever.
    /// After inserting a new entry, prior futures returned from this method will not become ready.
    /// They should be dropped after calling [Self::insert], and a new future as returned from
    /// this method should be awaited instead.
    pub async fn wait_and_drain(&mut self) -> impl Iterator<Item = (Instant, T)> {
        self.reset();
        match self.next.as_mut() {
            Some((instant, sleep)) => {
                sleep.await;
                self.map.drain_until(instant)
            }
            None => std::future::pending().await,
        }
    }
}
