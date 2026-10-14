//! Utilities for iroh-gossip networking

use std::{
    collections::{hash_map, HashMap},
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
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

use super::{
    queue::{QueueItem, QueueReceiver, QueueReceivers},
    InEvent, ProtoMessage,
};
use crate::proto::{state::MessageKind, util::TimerMap, TopicId};

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
    /// The peer took no byte for [`WRITE_TIMEOUT`]
    #[error("the peer took no byte for {WRITE_TIMEOUT:?}")]
    Stalled {},
}

/// How long a write may take no byte before it fails the connection.
///
/// A peer returns stream credit only after it read part of its window, so a
/// slow reader takes no byte for a while now and then. A shorter time would
/// fail such peers.
#[cfg(not(test))]
pub(crate) const WRITE_TIMEOUT: Duration = Duration::from_secs(60);
#[cfg(test)]
pub(crate) const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

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

/// The maximum number of stream headers that a [`RecvLoop`] reads at the same time.
///
/// At this limit, the loop accepts no new stream until a header is complete.
/// A peer that keeps many partial headers thus delays only its own streams.
pub(crate) const MAX_PENDING_HEADERS: usize = 16;

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
        // Headers are read apart, so that a partial one blocks no other stream.
        let mut header_futures = FuturesUnordered::new();
        let mut read_futures = FuturesUnordered::new();
        let mut conn_is_closed = false;
        let closed = self.conn.closed();
        tokio::pin!(closed);
        while !conn_is_closed || !header_futures.is_empty() || !read_futures.is_empty() {
            tokio::select! {
                _ = &mut closed, if !conn_is_closed => {
                    conn_is_closed = true;
                }
                stream = self.conn.accept_uni(), if !conn_is_closed && header_futures.len() < MAX_PENDING_HEADERS => {
                    let stream = match stream {
                        Ok(stream) => stream,
                        Err(_) => {
                            conn_is_closed = true;
                            continue;
                        }
                    };
                    header_futures.push(RecvStreamState::new(stream, self.max_message_size));
                }
                Some(state) = header_futures.next(), if !header_futures.is_empty() => {
                    // A bad header loses only its stream, as a bad frame does.
                    let state = match state {
                        Ok(state) => state,
                        Err(err) => {
                            debug!("stream header failed: {err:#}");
                            continue;
                        }
                    };
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

/// Writes the queued messages of a connection, with one writer per class.
///
/// Each writer has its own streams, one per topic. Control streams have the
/// higher priority, so control goes out first when data fills the connection.
pub(crate) struct SendLoop {
    conn: Connection,
    max_message_size: usize,
    queues: Option<QueueReceivers>,
    sending: Arc<Sending>,
}

/// Whether a writer has a message queued or in writing, or a stream not yet finished.
#[derive(Debug)]
struct Sending {
    busy: [AtomicBool; 2],
    tx: watch::Sender<bool>,
}

impl Sending {
    fn set(&self, class: usize, busy: bool) {
        self.busy[class].store(busy, Ordering::Relaxed);
        let any = self.busy.iter().any(|busy| busy.load(Ordering::Relaxed));
        self.tx
            .send_if_modified(|current| std::mem::replace(current, any) != any);
    }
}

impl SendLoop {
    pub(crate) fn new(conn: Connection, queues: QueueReceivers, max_message_size: usize) -> Self {
        Self {
            conn,
            max_message_size,
            queues: Some(queues),
            sending: Arc::new(Sending {
                busy: Default::default(),
                tx: watch::channel(false).0,
            }),
        }
    }

    /// Returns whether the loop still has data to send, as it changes.
    pub(crate) fn sending(&self) -> watch::Receiver<bool> {
        self.sending.tx.subscribe()
    }

    /// Sends `queue` first, then the queued messages, until the queue or the connection ends.
    pub(crate) async fn run(&mut self, queue: Vec<ProtoMessage>) -> Result<(), WriteError> {
        let QueueReceivers { control, data } = self.queues.take().expect("runs once");
        let (first_control, first_data): (Vec<_>, Vec<_>) = queue
            .into_iter()
            .partition(|message| matches!(message.kind(), MessageKind::Control));
        let writer = |class: usize, rx: QueueReceiver| Writer {
            conn: self.conn.clone(),
            class,
            priority: if class == CONTROL { 1 } else { 0 },
            rx,
            streams: Default::default(),
            buffer: Default::default(),
            max_message_size: self.max_message_size,
            finishing: Default::default(),
            sending: self.sending.clone(),
        };
        let mut control = writer(CONTROL, control);
        let mut data = writer(DATA, data);
        let result = n0_future::future::zip(control.run(first_control), data.run(first_data)).await;
        debug!("send loop closed");
        result.0.and(result.1)
    }
}

/// The index of the control writer in [`Sending`].
const CONTROL: usize = 0;
/// The index of the data writer in [`Sending`].
const DATA: usize = 1;

/// The code of a close for a peer that took no byte for [`WRITE_TIMEOUT`].
const CLOSE_STALLED: u32 = 3;

/// Writes the messages of one class.
struct Writer {
    conn: Connection,
    class: usize,
    priority: i32,
    rx: QueueReceiver,
    streams: HashMap<TopicId, SendStream>,
    buffer: Vec<u8>,
    max_message_size: usize,
    finishing: JoinSet<()>,
    sending: Arc<Sending>,
}

impl Writer {
    fn set_sending(&self, sending: bool) {
        self.sending.set(self.class, sending);
    }

    async fn run(&mut self, queue: Vec<ProtoMessage>) -> Result<(), WriteError> {
        self.set_sending(!queue.is_empty());
        for msg in queue {
            self.send(&msg).await?;
        }
        let conn_clone = self.conn.clone();
        let closed = conn_clone.closed();
        tokio::pin!(closed);
        loop {
            self.set_sending(!self.rx.is_empty() || !self.finishing.is_empty());
            tokio::select! {
                biased;
                _ = &mut closed => break,
                item = self.rx.recv() => match item {
                    Some(QueueItem::Message(msg, _size)) => {
                        self.set_sending(true);
                        self.send(&msg).await?
                    }
                    Some(QueueItem::End(topic)) => self.finish(topic),
                    // The actor dropped the sender.
                    None => break,
                },
                _ = self.finishing.join_next(), if !self.finishing.is_empty() => {}
            }
        }
        // Nothing reads the queue anymore, so the actor must not fill it.
        self.rx.close();

        // Close remaining streams.
        self.set_sending(!self.streams.is_empty() || !self.finishing.is_empty());
        let topics: Vec<_> = self.streams.keys().copied().collect();
        for topic_id in topics {
            self.finish(topic_id);
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
        Ok(())
    }

    /// Finishes the stream of `topic_id`, if one is open.
    fn finish(&mut self, topic_id: TopicId) {
        let Some(mut stream) = self.streams.remove(&topic_id) else {
            return;
        };
        trace!(topic=%topic_id.fmt_short(), "stream closing");
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
        if matches!(err, WriteError::Stalled { .. }) {
            // A peer that takes nothing holds every stream of the connection.
            warn!("the peer took no byte for {WRITE_TIMEOUT:?}, close the connection");
            self.conn.close(CLOSE_STALLED.into(), b"write stalled");
            return Err(err);
        }
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
    /// A `Disconnect` is the last message of its stream.
    ///
    /// This function is not cancellation-safe.
    async fn write_message(&mut self, message: &ProtoMessage) -> Result<(), WriteError> {
        let ProtoMessage { topic, message } = message;
        let topic_id = *topic;

        let stream = match self.streams.entry(topic_id) {
            hash_map::Entry::Occupied(entry) => entry.into_mut(),
            hash_map::Entry::Vacant(entry) => {
                // The peer grants streams as it reads them.
                let mut stream = n0_future::time::timeout(WRITE_TIMEOUT, self.conn.open_uni())
                    .await
                    .map_err(|_| e!(WriteError::Stalled))??;
                stream.set_priority(self.priority).ok();
                let header = StreamHeader::new(topic_id);
                header
                    .write(&mut stream, &mut self.buffer, self.max_message_size)
                    .await?;
                debug!(topic=%topic_id.fmt_short(), "stream opened");
                entry.insert(stream)
            }
        };
        let len = postcard::experimental::serialized_size(&message)?;
        if len >= self.max_message_size {
            return Err(e!(WriteError::TooLarge));
        }
        self.buffer.clear();
        self.buffer.extend_from_slice(&(len as u32).to_be_bytes());
        self.buffer.resize(4 + len, 0u8);
        postcard::to_slice(&message, &mut self.buffer[4..])?;
        // Each write takes some bytes, or fails once the peer takes none for long.
        let mut written = 0;
        while written < self.buffer.len() {
            written +=
                n0_future::time::timeout(WRITE_TIMEOUT, stream.write(&self.buffer[written..]))
                    .await
                    .map_err(|_| e!(WriteError::Stalled))?
                    .map_err(io::Error::other)?;
        }
        if message.is_disconnect() {
            self.finish(topic_id);
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
