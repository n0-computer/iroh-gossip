//! Utilities for iroh-gossip networking

use std::{io, pin::Pin};

use bytes::{BufMut, Bytes, BytesMut};
use n0_future::time::{sleep_until, Instant, Sleep};
use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::proto::util::TimerMap;

/// Errors related to message writing
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// Serialization failed
    #[error(transparent)]
    Ser(#[from] postcard::Error),
    /// IO error
    #[error(transparent)]
    Write(#[from] iroh::endpoint::WriteError),
    // /// Message was larger than the configured maximum message size
    // #[error("message too large")]
    // TooLarge,
}

/// Write a `ProtoMessage` as a length-prefixed, postcard-encoded message.
pub async fn write_message<T: Serialize>(
    writer: &mut iroh::endpoint::SendStream,
    buffer: &mut BytesMut,
    frame: &T, // max_message_size: usize,
) -> Result<(), WriteError> {
    let len = postcard::experimental::serialized_size(&frame)?;
    // if len >= max_message_size {
    //     return Err(WriteError::TooLarge);
    // }

    buffer.clear();
    buffer.put_u32(len as u32);
    buffer.resize(len + 4, 0u8);
    let _slice = postcard::to_slice(&frame, &mut buffer[4..])?;
    writer.write_all(&buffer).await?;
    // writer.write_u32(len as u32).await?;
    // writer.write_all(slice).await?;
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
pub async fn read_message<T: DeserializeOwned>(
    reader: impl AsyncRead + Unpin,
    buffer: &mut BytesMut,
    max_message_size: usize,
) -> Result<Option<T>, ReadError> {
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
pub(crate) struct Timers<T> {
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
    // /// Creates a new timer map.
    // pub fn new() -> Self {
    //     Self::default()
    // }

    /// Inserts a new entry at the specified instant
    pub(crate) fn insert(&mut self, instant: Instant, item: T) {
        self.map.insert(instant, item);
    }

    fn reset(&mut self) {
        self.next = self
            .map
            .first()
            .map(|instant| (*instant, Box::pin(sleep_until(*instant))))
    }

    /// Waits for the next timer to elapse.
    pub(crate) async fn wait_next(&mut self) -> Instant {
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
    pub(crate) fn pop_before(&mut self, now: Instant) -> Option<(Instant, T)> {
        self.map.pop_before(now)
    }
}
