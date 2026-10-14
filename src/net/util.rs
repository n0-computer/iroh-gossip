//! Utilities for iroh-gossip networking

use std::{
    collections::{hash_map, HashMap},
    io,
    time::Duration,
};

use bytes::{Bytes, BytesMut};
use iroh::{
    endpoint::{Connection, RecvStream, SendStream},
    EndpointId,
};
use n0_error::{e, stack_error};
use n0_future::{
    task::JoinSet,
    time::{sleep_until, Instant},
    FuturesUnordered, StreamExt,
};
use serde::{de::DeserializeOwned, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, watch},
};
use tracing::{debug, trace, warn, Instrument};

use super::{InEvent, ProtoMessage};
use crate::proto::{util::TimerMap, TopicId};

/// Errors related to message writing
#[allow(missing_docs)]
#[stack_error(derive, add_meta, from_sources)]
#[non_exhaustive]
pub(crate) enum WriteError {
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
    /// Message was larger than the configured maximum message size
    #[error("message too large")]
    TooLarge {},
}

/// The first frame of a stream: the topic of its messages, and a byte of flags.
///
/// Nodes up to 0.101 send the topic alone, and ignore the flags when they read
/// a header, as they ignore the rest of a frame.
#[derive(Debug)]
pub(crate) struct StreamHeader {
    pub(crate) topic_id: TopicId,
    /// Whether the sender drops its state for a peer whose connection closes.
    ///
    /// A node without it loses every later message to a peer that closes a
    /// connection that is no neighbor link of the node. So we close no
    /// connection on which the peer did not send it.
    pub(crate) handles_close: bool,
}

/// The flag of [`StreamHeader::handles_close`].
const HANDLES_CLOSE: u8 = 1;

impl StreamHeader {
    /// Returns the header that we send on a stream for `topic_id`.
    pub(crate) fn new(topic_id: TopicId) -> Self {
        Self {
            topic_id,
            handles_close: true,
        }
    }

    pub(crate) async fn read(
        stream: &mut RecvStream,
        buffer: &mut BytesMut,
        max_message_size: usize,
    ) -> Result<Self, ReadError> {
        let frame = read_lp(stream, buffer, max_message_size)
            .await?
            .ok_or_else(|| {
                ReadError::from(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "stream ended before header",
                ))
            })?;
        Self::decode(&frame)
    }

    fn decode(frame: &[u8]) -> Result<Self, ReadError> {
        let (topic_id, rest) = postcard::take_from_bytes::<TopicId>(frame)?;
        Ok(Self {
            topic_id,
            handles_close: rest.first().is_some_and(|flags| flags & HANDLES_CLOSE != 0),
        })
    }

    pub(crate) async fn write(
        self,
        stream: &mut SendStream,
        buffer: &mut Vec<u8>,
        max_message_size: usize,
    ) -> Result<(), WriteError> {
        let flags = if self.handles_close { HANDLES_CLOSE } else { 0 };
        write_frame(stream, &(self.topic_id, flags), buffer, max_message_size).await?;
        Ok(())
    }
}

pub(crate) struct RecvLoop {
    remote_endpoint_id: EndpointId,
    conn: Connection,
    max_message_size: usize,
    in_event_tx: mpsc::Sender<InEvent>,
    /// Whether a stream header of the peer had [`StreamHeader::handles_close`].
    handles_close: watch::Sender<bool>,
}

impl RecvLoop {
    pub(crate) fn new(
        remote_endpoint_id: EndpointId,
        conn: Connection,
        in_event_tx: mpsc::Sender<InEvent>,
        max_message_size: usize,
    ) -> Self {
        Self {
            remote_endpoint_id,
            conn,
            max_message_size,
            in_event_tx,
            handles_close: watch::channel(false).0,
        }
    }

    /// Returns whether the peer handles a close of this connection, as it learns it.
    pub(crate) fn handles_close(&self) -> watch::Receiver<bool> {
        self.handles_close.subscribe()
    }

    pub(crate) async fn run(&mut self) -> Result<(), ReadError> {
        let mut read_futures = FuturesUnordered::new();
        let mut conn_is_closed = false;
        let closed = self.conn.closed();
        tokio::pin!(closed);
        while !conn_is_closed || !read_futures.is_empty() {
            tokio::select! {
                _ = &mut closed, if !conn_is_closed => {
                    conn_is_closed = true;
                }
                stream = self.conn.accept_uni(), if !conn_is_closed => {
                    let stream = match stream {
                        Ok(stream) => stream,
                        Err(_) => {
                            conn_is_closed = true;
                            continue;
                        }
                    };
                    let state = RecvStreamState::new(stream, self.max_message_size).await?;
                    debug!(topic=%state.header.topic_id.fmt_short(), "stream opened");
                    if state.header.handles_close {
                        self.handles_close.send_replace(true);
                    }
                    read_futures.push(state.next());
                }
                Some(res) = read_futures.next(), if !read_futures.is_empty() => {
                    let (state, msg) = match res {
                        Ok((state, msg)) => (state, msg),
                        Err(err) => {
                            debug!("recv stream closed with error: {err:#}");
                            continue;
                        }
                    };
                    match msg {
                        None => debug!(topic=%state.header.topic_id.fmt_short(), "stream closed"),
                        Some(msg) => {
                            if self.in_event_tx.send(InEvent::RecvMessage(self.remote_endpoint_id, msg)).await.is_err() {
                                debug!("stop recv loop: actor closed");
                                break;
                            }
                            read_futures.push(state.next());
                        }
                    }
                }
            }
        }
        debug!("recv loop closed");
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
    async fn new(mut stream: RecvStream, max_message_size: usize) -> Result<Self, ReadError> {
        let mut buffer = BytesMut::new();
        let header = StreamHeader::read(&mut stream, &mut buffer, max_message_size).await?;
        Ok(Self {
            buffer: BytesMut::new(),
            max_message_size,
            stream,
            header,
        })
    }

    /// Reads the next message from the stream.
    ///
    /// Returns `self` and the next message, or `None` if the stream ended gracefully.
    ///
    /// ## Cancellation safety
    ///
    /// This function is not cancellation-safe.
    async fn next(mut self) -> Result<(Self, Option<ProtoMessage>), ReadError> {
        let msg = read_frame(&mut self.stream, &mut self.buffer, self.max_message_size).await?;
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
    /// Whether a message is queued or written, or a stream is not yet finished.
    sending: watch::Sender<bool>,
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
            sending: watch::channel(false).0,
        }
    }

    /// Returns whether the loop still has data to send, as it changes.
    pub(crate) fn sending(&self) -> watch::Receiver<bool> {
        self.sending.subscribe()
    }

    fn set_sending(&self, sending: bool) {
        self.sending
            .send_if_modified(|current| std::mem::replace(current, sending) != sending);
    }

    pub(crate) async fn run(&mut self, queue: Vec<ProtoMessage>) -> Result<(), WriteError> {
        self.set_sending(!queue.is_empty());
        for msg in queue {
            self.send(&msg).await?;
        }
        let conn_clone = self.conn.clone();
        let closed = conn_clone.closed();
        tokio::pin!(closed);
        loop {
            self.set_sending(!self.send_rx.is_empty() || !self.finishing.is_empty());
            tokio::select! {
                biased;
                _ = &mut closed => break,
                msg = self.send_rx.recv() => match msg {
                    Some(msg) => {
                        self.set_sending(true);
                        self.send(&msg).await?
                    }
                    // The actor dropped the sender.
                    None => break,
                },
                _ = self.finishing.join_next(), if !self.finishing.is_empty() => {}
            }
        }
        // Nothing reads the queue anymore, so the actor must not fill it.
        self.send_rx.close();

        // Close remaining streams.
        self.set_sending(!self.streams.is_empty() || !self.finishing.is_empty());
        for (topic_id, mut stream) in self.streams.drain() {
            stream.finish().ok();
            self.finishing.spawn(
                async move {
                    stream.stopped().await.ok();
                    debug!(topic=%topic_id.fmt_short(), "stream closed");
                }
                .instrument(tracing::Span::current()),
            );
        }
        if !self.finishing.is_empty() {
            trace!(
                "send loop closing, waiting for {} send streams to finish",
                self.finishing.len()
            );
            // Wait for the remote to acknowledge all streams are finished.
            if let Err(_elapsed) = n0_future::time::timeout(Duration::from_secs(5), async {
                while self.finishing.join_next().await.is_some() {}
            })
            .await
            {
                debug!("not all send streams finished within timeout, abort")
            }
        }
        self.set_sending(false);
        debug!("send loop closed");
        Ok(())
    }

    /// Writes a message, and drops only its stream if the write fails while the connection runs.
    ///
    /// The peer stops a stream that it cannot read, for example on a frame over its
    /// size limit or with a message type that it does not know. The message is lost,
    /// and the next message of the topic opens a new stream. Returns the error only
    /// if the connection is gone.
    async fn send(&mut self, message: &ProtoMessage) -> Result<(), WriteError> {
        let Err(err) = self.write_message(message).await else {
            return Ok(());
        };
        if self.conn.close_reason().is_some() {
            return Err(err);
        }
        warn!(topic = %message.topic.fmt_short(), "write failed, drop the stream: {err:#}");
        if let Some(mut stream) = self.streams.remove(&message.topic) {
            stream.reset(0u32.into()).ok();
        }
        Ok(())
    }

    /// Write a [`ProtoMessage`] as a length-prefixed, postcard-encoded message on its stream.
    ///
    /// If no stream is opened yet, this opens a new stream for the topic and writes the topic header.
    ///
    /// This function is not cancellation-safe.
    pub async fn write_message(&mut self, message: &ProtoMessage) -> Result<(), WriteError> {
        let ProtoMessage { topic, message } = message;
        let topic_id = *topic;
        let is_last = message.is_disconnect();

        let mut entry = match self.streams.entry(topic_id) {
            hash_map::Entry::Occupied(entry) => entry,
            hash_map::Entry::Vacant(entry) => {
                let mut stream = self.conn.open_uni().await?;
                let header = StreamHeader::new(topic_id);
                header
                    .write(&mut stream, &mut self.buffer, self.max_message_size)
                    .await?;
                debug!(topic=%topic_id.fmt_short(), "stream opened");
                entry.insert_entry(stream)
            }
        };
        let stream = entry.get_mut();

        write_frame(stream, message, &mut self.buffer, self.max_message_size).await?;

        if is_last {
            trace!(topic=%topic_id.fmt_short(), "stream closing");
            let mut stream = entry.remove();
            if stream.finish().is_ok() {
                self.finishing.spawn(
                    async move {
                        stream.stopped().await.ok();
                        debug!(topic=%topic_id.fmt_short(), "stream closed");
                    }
                    .instrument(tracing::Span::current()),
                );
            }
        }

        Ok(())
    }
}

/// Errors related to message reading
#[allow(missing_docs)]
#[stack_error(derive, add_meta, from_sources)]
#[non_exhaustive]
pub(crate) enum ReadError {
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
    /// Message was larger than the configured maximum message size
    #[error("message too large")]
    TooLarge {},
}

/// Read a length-prefixed frame and decode with postcard.
pub async fn read_frame<T: DeserializeOwned>(
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

/// Reads a length prefixed buffer.
///
/// Returns the frame as raw bytes.  If the end of the stream is reached before
/// the frame length starts, `None` is returned.
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
    let size = usize::try_from(size).map_err(|_| e!(ReadError::TooLarge))?;
    if size > max_message_size {
        return Err(e!(ReadError::TooLarge));
    }
    buffer.resize(size, 0u8);
    reader
        .read_exact(&mut buffer[..])
        .await
        .map_err(io::Error::other)?;
    Ok(Some(buffer.split_to(size).freeze()))
}

/// Writes a length-prefixed frame.
pub async fn write_frame<T: Serialize>(
    stream: &mut SendStream,
    message: &T,
    buffer: &mut Vec<u8>,
    max_message_size: usize,
) -> Result<(), WriteError> {
    let len = postcard::experimental::serialized_size(&message)?;
    if len >= max_message_size {
        return Err(e!(WriteError::TooLarge));
    }
    buffer.clear();
    buffer.resize(len, 0u8);
    let slice = postcard::to_slice(&message, buffer)?;
    stream.write_u32(len as u32).await?;
    stream.write_all(slice).await.map_err(io::Error::other)?;
    Ok(())
}

/// A [`TimerMap`] with an async method to wait for the next timer expiration.
#[derive(Debug)]
pub struct Timers<T> {
    map: TimerMap<T>,
}

impl<T> Default for Timers<T> {
    fn default() -> Self {
        Self {
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

    /// Waits for the next timer to elapse.
    pub async fn wait_next(&mut self) -> Instant {
        match self.map.first() {
            None => std::future::pending::<Instant>().await,
            Some(instant) => {
                sleep_until(*instant).await;
                *instant
            }
        }
    }

    /// Pops the earliest timer that expires at or before `now`.
    pub fn pop_before(&mut self, now: Instant) -> Option<(Instant, T)> {
        self.map.pop_before(now)
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    /// The header as nodes up to 0.101 encode and decode it.
    #[derive(Debug, Serialize, Deserialize)]
    struct OldStreamHeader {
        topic_id: TopicId,
    }

    /// Old and new nodes read the stream headers of each other.
    #[test]
    fn stream_header_works_across_versions() {
        let topic_id: TopicId = [7; 32].into();
        let new = postcard::to_stdvec(&(topic_id, HANDLES_CLOSE)).expect("encode");
        let old = postcard::to_stdvec(&OldStreamHeader { topic_id }).expect("encode");

        let read_by_old: OldStreamHeader = postcard::from_bytes(&new).expect("old reads new");
        assert_eq!(read_by_old.topic_id, topic_id);

        let header = StreamHeader::decode(&old).expect("new reads old");
        assert_eq!(header.topic_id, topic_id);
        assert!(!header.handles_close);

        let header = StreamHeader::decode(&new).expect("new reads new");
        assert_eq!(header.topic_id, topic_id);
        assert!(header.handles_close);
    }
}
