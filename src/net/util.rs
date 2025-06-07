//! Utilities for iroh-gossip networking

use std::{
    collections::{hash_map, HashMap},
    io,
    pin::Pin,
};

use bytes::{Bytes, BytesMut};
use iroh::{
    endpoint::{Connection, RecvStream, SendStream},
    NodeId,
};
use n0_future::{
    time::{sleep_until, Instant, Sleep},
    FuturesUnordered, StreamExt,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
    task::JoinSet,
};
use tracing::debug;

use super::{InEvent, ProtoMessage};
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
    pub(crate) async fn read(
        stream: &mut RecvStream,
        buffer: &mut BytesMut,
        max_message_size: usize,
    ) -> Result<Self, ReadError> {
        let header: WireStreamHeader = read_message(stream, buffer, max_message_size)
            .await?
            .ok_or(ReadError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "stream ended before header",
            )))?;
        let WireStreamHeader::V0(header) = header;
        Ok(header)
    }

    pub(crate) async fn write(
        self,
        stream: &mut SendStream,
        buffer: &mut Vec<u8>,
        max_message_size: usize,
    ) -> Result<(), WriteError> {
        let frame = WireStreamHeader::V0(self);
        write_lp(stream, &frame, buffer, max_message_size).await?;
        Ok(())
    }
}

pub(crate) struct RecvLoop {
    remote_node_id: NodeId,
    conn: Connection,
    max_message_size: usize,
    in_event_tx: mpsc::Sender<InEvent>,
}

impl RecvLoop {
    pub(crate) fn new(
        remote_node_id: NodeId,
        conn: Connection,
        in_event_tx: mpsc::Sender<InEvent>,
        max_message_size: usize,
    ) -> Self {
        Self {
            remote_node_id,
            conn,
            max_message_size,
            in_event_tx,
        }
    }

    pub(crate) async fn run(&mut self) -> Result<(), ReadError> {
        let mut read_futures = FuturesUnordered::new();
        loop {
            tokio::select! {
                _ = self.conn.closed() => break,
                stream = self.conn.accept_uni() => {
                    let stream = stream.map_err(|err| io::Error::other(err))?;
                    let state = RecvStreamState::init(stream, self.max_message_size).await?;
                    debug!(topic=%state.header.topic_id.fmt_short(), "stream opened");
                    read_futures.push(state.read_next());
                }
                Some(res) = read_futures.next(), if !read_futures.is_empty() => {
                    let (state, msg) = res?;
                    match msg {
                        None => debug!(topic=%state.header.topic_id.fmt_short(), "stream closed"),
                        Some(msg) => {
                            if let Err(_) = self.in_event_tx.send(InEvent::RecvMessage(self.remote_node_id, msg)).await {
                                break;
                            }
                            read_futures.push(state.read_next());
                        }
                    }
                }
            }
        }
        debug!("done");
        Ok(())
    }
}

#[derive(Debug)]
struct RecvStreamState {
    stream: RecvStream,
    header: StreamHeader,
    buffer: BytesMut,
    max_message_size: usize,
}

impl RecvStreamState {
    pub async fn init(mut stream: RecvStream, max_message_size: usize) -> Result<Self, ReadError> {
        let mut buffer = BytesMut::new();
        let header = StreamHeader::read(&mut stream, &mut buffer, max_message_size).await?;
        Ok(Self {
            buffer: BytesMut::new(),
            max_message_size,
            stream,
            header,
        })
    }

    pub async fn read_next(mut self) -> Result<(Self, Option<ProtoMessage>), ReadError> {
        let msg = read_message(&mut self.stream, &mut self.buffer, self.max_message_size).await?;
        let msg = msg.map(|msg| ProtoMessage {
            topic: self.header.topic_id,
            message: msg,
        });
        Ok((self, msg))
    }
}

pub(crate) struct SendLoop {
    conn: Connection,
    streams: HashMap<TopicId, SendStream>,
    buffer: Vec<u8>,
    max_message_size: usize,
    finishing: JoinSet<()>,
    send_rx: mpsc::Receiver<ProtoMessage>,
}

impl SendLoop {
    pub(crate) fn new(
        conn: Connection,
        send_rx: mpsc::Receiver<ProtoMessage>,
        max_message_size: usize,
    ) -> Self {
        Self {
            conn,
            max_message_size,
            buffer: Default::default(),
            streams: Default::default(),
            finishing: Default::default(),
            send_rx,
        }
    }

    pub(crate) async fn run(&mut self, queue: Vec<ProtoMessage>) -> Result<(), WriteError> {
        for msg in queue {
            self.write_message(&msg).await?;
        }
        loop {
            tokio::select! {
                biased;
                _ = self.conn.closed() => break,
                Some(msg) = self.send_rx.recv() => self.write_message(&msg).await?,
                _ = self.finishing.join_next(), if !self.finishing.is_empty() => {}
                else => break,
            }
        }

        // Close remaining streams.
        for (_, mut stream) in self.streams.drain() {
            stream.finish().ok();
            self.finishing.spawn(async move {
                stream.stopped().await.ok();
            });
        }
        // Wait for the remote to acknowledge all streams are stopped.
        // TODO(Frando): We likely want to apply a timeout here.
        while let Some(_) = self.finishing.join_next().await {}
        Ok(())
    }

    /// Write a `ProtoMessage` as a length-prefixed, postcard-encoded message on its stream.
    ///
    /// If no stream is opened yet, this opens a new stream for the topic and writes the topic header.
    ///
    /// This function is not cancellation-safe.
    pub async fn write_message(&mut self, frame: &ProtoMessage) -> Result<(), WriteError> {
        let ProtoMessage { topic, message } = frame;
        let topic_id = *topic;
        let is_last = message.is_disconnect();

        let mut entry = match self.streams.entry(topic_id) {
            hash_map::Entry::Occupied(entry) => entry,
            hash_map::Entry::Vacant(entry) => {
                let mut stream = self.conn.open_uni().await?;
                let header = StreamHeader { topic_id };
                header
                    .write(&mut stream, &mut self.buffer, self.max_message_size)
                    .await?;
                debug!(topic=%topic_id.fmt_short(), "stream accepted");
                entry.insert_entry(stream)
            }
        };
        let stream = entry.get_mut();

        write_lp(stream, message, &mut self.buffer, self.max_message_size).await?;

        if is_last {
            debug!(topic=%topic_id.fmt_short(), "stream closing");
            let mut stream = entry.remove();
            if stream.finish().is_ok() {
                self.finishing.spawn(async move {
                    stream.stopped().await.ok();
                    debug!(topic=%topic_id.fmt_short(), "stream closed");
                });
            }
        }

        Ok(())
    }
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

/// Read a length-prefixed message and decode with postcard.
pub async fn read_message<T: DeserializeOwned>(
    reader: &mut RecvStream,
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
    reader: &mut RecvStream,
    buffer: &mut BytesMut,
    max_message_size: usize,
) -> Result<Option<Bytes>, ReadError> {
    let size = match reader.read_u32().await {
        Ok(size) => size,
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    // let mut reader = reader.take(size as u64);
    let size = usize::try_from(size).map_err(|_| ReadError::TooLarge)?;
    if size > max_message_size {
        return Err(ReadError::TooLarge);
    }
    buffer.resize(size, 0u8);
    reader
        .read_exact(&mut buffer[..])
        .await
        .map_err(|err| io::Error::other(err))?;
    Ok(Some(buffer.split_to(size).freeze()))
}

/// Writes a length-prefixed message.
pub async fn write_lp<T: Serialize>(
    stream: &mut SendStream,
    message: &T,
    buffer: &mut Vec<u8>,
    max_message_size: usize,
) -> Result<(), WriteError> {
    let len = postcard::experimental::serialized_size(&message)?;
    if len >= max_message_size {
        return Err(WriteError::TooLarge);
    }
    buffer.clear();
    buffer.resize(len, 0u8);
    let slice = postcard::to_slice(&message, buffer)?;
    stream.write_u32(len as u32).await?;
    stream
        .write_all(slice)
        .await
        .map_err(|err| io::Error::other(err))?;
    Ok(())
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
