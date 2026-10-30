//! Utilities for iroh-gossip networking

use std::{
    collections::{hash_map, HashMap, VecDeque},
    io,
    time::Duration,
};

use bytes::{Bytes, BytesMut};
use iroh::{
    endpoint::{Connection, RecvStream, SendStream, StreamId},
    EndpointId,
};
use n0_error::{e, stack_error};
use n0_future::{
    boxed::BoxFuture,
    time::{sleep_until, Instant},
    FuturesUnordered, StreamExt,
};
use serde::{de::DeserializeOwned, Serialize};
use tokio::{
    io::AsyncReadExt,
    sync::{mpsc, watch},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use super::{
    queue::{QueueLease, QueuedMessage},
    InEvent, ProtoMessage,
};
use crate::proto::{util::TimerMap, TopicId};

/// How long a write may make no progress before it fails the connection.
///
/// A peer extends its flow control window only after it read part of it, so
/// the writes to a slow reader block for a while now and then.
#[cfg(not(test))]
pub(super) const WRITE_TIMEOUT: Duration = Duration::from_secs(60);
#[cfg(test)]
pub(super) const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// How long opening a stream may wait at the stream limit of the peer before it fails the connection.
///
/// The peer raises its limit only once one of our streams ends. With as many
/// topics as the limit, the wait does not end. The limit is the
/// `max_concurrent_uni_streams` of the peer's endpoint, 100 by default. A
/// stalled open of an initial topic only skips the other initial topics.
#[cfg(not(test))]
pub(super) const OPEN_TIMEOUT: Duration = Duration::from_secs(15);
#[cfg(test)]
pub(super) const OPEN_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a replaced connection may write its current message before it hands it over.
///
/// The new connection of the peer waits for it, so that we write the messages
/// in order. A stream header that started is written in full first. Only
/// the write timeout bounds it, which counts the time without progress.
#[cfg(not(test))]
pub(super) const HANDOVER_LIMIT: Duration = Duration::from_secs(5);
#[cfg(test)]
pub(super) const HANDOVER_LIMIT: Duration = Duration::from_secs(1);

/// The code of a close for a peer whose send queue had no room for a control message.
pub(super) const CLOSE_QUEUE_FULL: u32 = 2;

/// The code of a close for a write or a stream open that made no progress in time.
pub(super) const CLOSE_STALLED: u32 = 3;

/// The code of a close for a connection without streams, see [`RETIRE_GRACE`](super::RETIRE_GRACE).
pub(super) const CLOSE_RETIRED: u32 = 4;

/// The flag of [`StreamHeader::handles_close`].
const HANDLES_CLOSE: u8 = 1;

/// The maximum number of stream headers that a [`RecvLoop`] reads at the same time.
///
/// At this limit, the loop accepts no new stream until a header is complete.
/// A peer that keeps many partial headers thus delays only its own streams.
pub(crate) const MAX_PENDING_HEADERS: usize = 16;

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
    /// A write made no progress for [`WRITE_TIMEOUT`]
    #[error("no write progress for {WRITE_TIMEOUT:?}")]
    Stalled {},
    /// Opening a stream waited for [`OPEN_TIMEOUT`] at the stream limit of the peer
    #[error("stream limit of the peer reached for {OPEN_TIMEOUT:?}")]
    OpenStalled {},
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
    /// connection to a peer that did not send it.
    pub(crate) handles_close: bool,
}

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
    ///
    /// The actor shares it between all connections to the peer.
    handles_close: watch::Sender<bool>,
    /// Whether the peer has a stream open to us, from its first byte on.
    has_streams: watch::Sender<bool>,
}

impl RecvLoop {
    pub(crate) fn new(
        remote_endpoint_id: EndpointId,
        conn: Connection,
        in_event_tx: mpsc::Sender<InEvent>,
        max_message_size: usize,
        handles_close: watch::Sender<bool>,
        has_streams: watch::Sender<bool>,
    ) -> Self {
        Self {
            remote_endpoint_id,
            conn,
            max_message_size,
            in_event_tx,
            handles_close,
            has_streams,
        }
    }

    /// Reads the streams of the connection until it closes or the actor stops.
    ///
    /// A bad stream loses only itself, so this never fails.
    pub(crate) async fn run(&mut self) {
        // Each header is read in its own future, so a partial one blocks no other stream.
        let mut header_futures = FuturesUnordered::new();
        let mut read_futures = FuturesUnordered::new();
        // Per topic, the id of the stream we read, and the newer streams that wait
        // until it ended, in id order. A sender opens a new stream of a topic only
        // after the old one ended, so a wait keeps the order and always ends.
        let mut topics: HashMap<TopicId, (StreamId, VecDeque<RecvStreamState>)> = HashMap::new();
        let mut conn_is_closed = false;
        let closed = self.conn.closed();
        tokio::pin!(closed);
        while !conn_is_closed || !header_futures.is_empty() || !read_futures.is_empty() {
            // A partial header counts. Only a broken peer keeps one partial.
            let open = !header_futures.is_empty() || !read_futures.is_empty();
            self.has_streams
                .send_if_modified(|has| std::mem::replace(has, open) != open);
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
                    let id = state.stream.id();
                    match topics.entry(state.header.topic_id) {
                        hash_map::Entry::Occupied(mut entry) if id > entry.get().0 => {
                            let waiting = &mut entry.get_mut().1;
                            let pos = waiting.partition_point(|s| s.stream.id() < id);
                            waiting.insert(pos, state);
                        }
                        // An older stream whose header came late. Its order is lost, so read it now.
                        hash_map::Entry::Occupied(_) => read_futures.push(state.next()),
                        hash_map::Entry::Vacant(entry) => {
                            entry.insert((id, VecDeque::new()));
                            read_futures.push(state.next());
                        }
                    }
                }
                Some(res) = read_futures.next(), if !read_futures.is_empty() => {
                    let (topic, id) = match res {
                        Ok((state, Some(msg))) => {
                            if self.in_event_tx.send(InEvent::RecvMessage(self.remote_endpoint_id, msg)).await.is_err() {
                                debug!("stop recv loop: actor closed");
                                break;
                            }
                            read_futures.push(state.next());
                            continue;
                        }
                        Ok((state, None)) => {
                            debug!(topic=%state.header.topic_id.fmt_short(), "stream closed");
                            (state.header.topic_id, state.stream.id())
                        }
                        Err((topic, id, err)) => {
                            debug!("recv stream closed with error: {err:#}");
                            (topic, id)
                        }
                    };
                    if let hash_map::Entry::Occupied(mut entry) = topics.entry(topic) {
                        if entry.get().0 == id {
                            match entry.get_mut().1.pop_front() {
                                Some(next) => {
                                    entry.get_mut().0 = next.stream.id();
                                    read_futures.push(next.next());
                                }
                                None => {
                                    entry.remove();
                                }
                            }
                        }
                    }
                }
            }
        }
        debug!("recv loop closed");
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
    async fn next(
        mut self,
    ) -> Result<(Self, Option<ProtoMessage>), (TopicId, StreamId, ReadError)> {
        let msg = read_frame(&mut self.stream, &mut self.buffer, self.max_message_size)
            .await
            .map_err(|err| (self.header.topic_id, self.stream.id(), err))?;
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
    /// A future per stream, which resolves once the peer stopped it or acknowledged all of it.
    live_streams: FuturesUnordered<BoxFuture<(TopicId, StreamId)>>,
    queue_lease: QueueLease,
    /// The topics that used the peer when the writer started, for which it opens streams.
    initial_topics: Vec<TopicId>,
    /// Whether we have a stream to the peer.
    ///
    /// It is set before an open, and clears once [`Self::live_streams`] is empty.
    has_streams: watch::Sender<bool>,
}

impl SendLoop {
    pub(crate) fn new(
        conn: Connection,
        queue_lease: QueueLease,
        max_message_size: usize,
        has_streams: watch::Sender<bool>,
        initial_topics: Vec<TopicId>,
    ) -> Self {
        Self {
            conn,
            max_message_size,
            buffer: Default::default(),
            streams: Default::default(),
            live_streams: Default::default(),
            queue_lease,
            initial_topics,
            has_streams,
        }
    }

    /// Writes the queue until it ends, the lease is revoked, or the connection closes.
    ///
    /// Once no queued message waits, it also opens a stream for each initial
    /// topic. After the queue closed, it waits for the queue to open again.
    /// Takes `self`, so that the lease drops once it returns.
    pub(crate) async fn run(mut self) -> Result<(), WriteError> {
        let conn = self.conn.clone();
        let closed = conn.closed();
        tokio::pin!(closed);
        let revoked = self.queue_lease.revoked().clone();
        // A replaced connection of the peer may still write its current message.
        let queue = tokio::select! {
            biased;
            _ = &mut closed => None,
            _ = revoked.cancelled() => None,
            queue = self.queue_lease.lock() => Some(queue),
        };
        if let Some(mut queue) = queue {
            let mut initial_topics = std::mem::take(&mut self.initial_topics);
            // Whether the queue closed, and the writer waits for it to open again.
            let mut parked = false;
            loop {
                // Every pass of the loop starts here, so the flag clears with the last stream.
                if self.live_streams.is_empty() {
                    self.has_streams
                        .send_if_modified(|has| std::mem::replace(has, false));
                }
                let more = tokio::select! {
                    biased;
                    _ = &mut closed => break,
                    // The next connection takes the rest of the queue.
                    _ = revoked.cancelled() => break,
                    // Before the queue: the queue of a busy peer is always ready, and
                    // would keep ended streams from being collected.
                    Some((topic, id)) = self.live_streams.next(), if !self.live_streams.is_empty() => {
                        // The peer stopped the stream, as on a frame over its size limit.
                        // The next message of the topic opens a new one.
                        if self.streams.get(&topic).is_some_and(|s| s.id() == id) {
                            self.streams.remove(&topic);
                        }
                        continue;
                    }
                    more = queue.next(!parked) => more,
                    // After the queue, so that queued messages get the streams of the peer
                    // first. A quiet topic needs a stream to keep the connection.
                    Some(topic) = async { initial_topics.pop() }, if !initial_topics.is_empty() => {
                        match self.open_stream(topic, &revoked).await {
                            // At the stream limit of the peer, the quiet topics go without.
                            Err(WriteError::OpenStalled { .. }) => initial_topics.clear(),
                            Err(err) => self.handle_error(topic, err)?,
                            Ok(_) => {}
                        }
                        continue;
                    }
                };
                if !more {
                    if parked {
                        // The queue is gone.
                        break;
                    }
                    // Wait on this connection, so that the next message needs no dial.
                    for mut stream in std::mem::take(&mut self.streams).into_values() {
                        stream.finish().ok();
                    }
                    initial_topics.clear();
                    parked = true;
                    continue;
                }
                parked = false;
                let item = queue.current().expect("next returned a message");
                match self.open_stream(item.message.topic, &revoked).await {
                    Ok(true) => {}
                    // The lease ended before the stream opened. The message stays
                    // in the queue for the next connection.
                    Ok(false) => break,
                    Err(err) => {
                        self.handle_error(item.message.topic, err)?;
                        queue.advance();
                        continue;
                    }
                }
                // A stuck write would keep the queue from the connection that replaced
                // this one. A message that we stop in the middle stays in the queue,
                // and the next connection writes it first. Its stream stays, and ends
                // with the others: the peer reads the messages before it, and drops
                // the part of this one.
                let handover = async {
                    revoked.cancelled().await;
                    n0_future::time::sleep(HANDOVER_LIMIT).await;
                };
                tokio::select! {
                    biased;
                    res = self.send(item) => res?,
                    _ = handover => {
                        warn!("the replaced connection did not finish its message in time, hand it over");
                        break;
                    }
                }
                // A topic that we just left needs no stream.
                if item.message.message.is_disconnect() {
                    initial_topics.retain(|topic| *topic != item.message.topic);
                }
                queue.advance();
            }
        }
        for mut stream in std::mem::take(&mut self.streams).into_values() {
            stream.finish().ok();
        }
        while self.live_streams.next().await.is_some() {}
        self.has_streams
            .send_if_modified(|has| std::mem::replace(has, false));
        debug!("send loop closed");
        Ok(())
    }

    /// Writes a message on the stream of its topic, and finishes the stream after a `Disconnect`.
    ///
    /// If the write fails because the peer stopped the stream, the message is lost.
    /// Returns the error only if the connection is gone, or if the write made no
    /// progress in time, which closes the connection.
    ///
    /// ## Cancellation safety
    ///
    /// This function is not cancellation-safe.
    async fn send(&mut self, item: &QueuedMessage) -> Result<(), WriteError> {
        let ProtoMessage { topic, message } = &item.message;
        let stream = self
            .streams
            .get_mut(topic)
            .expect("the stream was opened before");
        let len = item.encoded_len();
        match write_sized_frame(
            stream,
            message,
            len,
            &mut self.buffer,
            self.max_message_size,
        )
        .await
        {
            Ok(()) => {
                if message.is_disconnect() {
                    let mut stream = self.streams.remove(topic).expect("the stream is open");
                    stream.finish().ok();
                }
                Ok(())
            }
            // Refused before any byte went out, so the stream is intact.
            Err(WriteError::TooLarge { .. }) => {
                warn!(max = self.max_message_size, "message too large, dropped");
                Ok(())
            }
            Err(err) => {
                self.handle_error(*topic, err)?;
                self.streams.remove(topic);
                Ok(())
            }
        }
    }

    /// Returns the error if the connection is gone or stalled.
    ///
    /// Otherwise only one stream failed, and the loop goes on.
    fn handle_error(&self, topic: TopicId, err: WriteError) -> Result<(), WriteError> {
        if matches!(
            err,
            WriteError::Stalled { .. } | WriteError::OpenStalled { .. }
        ) {
            // A peer that reads nothing blocks every stream, so a new stream would not help.
            warn!("{err}, close the connection");
            self.conn.close(CLOSE_STALLED.into(), b"write stalled");
            Err(err)
        } else if self.conn.close_reason().is_some() {
            Err(err)
        } else {
            warn!(topic = %topic.fmt_short(), "write failed: {err:#}");
            Ok(())
        }
    }

    /// Opens the stream of `topic` and writes its header, unless it is open.
    ///
    /// Returns `false` if `revoked` ends the lease before the stream opened. A
    /// header that started is written in full, as a node up to 0.101 drops the
    /// whole connection on a partial one. An abort of the peer closes the
    /// connection anyway, and cuts the header.
    async fn open_stream(
        &mut self,
        topic: TopicId,
        revoked: &CancellationToken,
    ) -> Result<bool, WriteError> {
        let hash_map::Entry::Vacant(entry) = self.streams.entry(topic) else {
            return Ok(true);
        };
        // Before the open, which can wait at the stream limit of the peer.
        self.has_streams
            .send_if_modified(|has| !std::mem::replace(has, true));
        let mut stream = tokio::select! {
            biased;
            // Before the open, so that a revoked lease starts no header.
            _ = revoked.cancelled() => return Ok(false),
            open = n0_future::time::timeout(OPEN_TIMEOUT, self.conn.open_uni()) => {
                open.map_err(|_| e!(WriteError::OpenStalled))??
            }
        };
        StreamHeader::new(topic)
            .write(&mut stream, &mut self.buffer, self.max_message_size)
            .await?;
        debug!(topic=%topic.fmt_short(), "stream opened");
        // The QUIC stack of the peer acknowledges data that its app did not read
        // yet, so this needs no timeout.
        let (id, stopped) = (stream.id(), stream.stopped());
        self.live_streams.push(Box::pin(async move {
            stopped.await.ok();
            (topic, id)
        }));
        entry.insert(stream);
        Ok(true)
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
    write_sized_frame(stream, message, len, buffer, max_message_size).await
}

/// Writes a length-prefixed frame of `message`, whose encoded size is `len`.
async fn write_sized_frame<T: Serialize>(
    stream: &mut SendStream,
    message: &T,
    len: usize,
    buffer: &mut Vec<u8>,
    max_message_size: usize,
) -> Result<(), WriteError> {
    // A frame must be shorter than the limit. Nodes up to 0.101 read a frame of
    // the limit, but refuse to write one, so they could not forward it.
    if len >= max_message_size {
        return Err(e!(WriteError::TooLarge));
    }
    buffer.clear();
    buffer.extend_from_slice(&(len as u32).to_be_bytes());
    buffer.resize(4 + len, 0u8);
    postcard::to_slice(&message, &mut buffer[4..])?;
    // Each write takes some bytes, or fails once the peer takes none for long.
    // A slow peer thus keeps the connection, however long the frame takes.
    let mut written = 0;
    while written < buffer.len() {
        written += n0_future::time::timeout(WRITE_TIMEOUT, stream.write(&buffer[written..]))
            .await
            .map_err(|_| e!(WriteError::Stalled))?
            .map_err(io::Error::other)?;
    }
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
