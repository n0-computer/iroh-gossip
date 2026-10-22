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
    io::AsyncReadExt,
    sync::{mpsc, watch},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, trace, warn, Instrument};

use super::{
    queue::{QueueLease, QueuedMessage},
    InEvent, ProtoMessage,
};
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
    /// A write made no progress for [`WRITE_TIMEOUT`]
    #[error("no write progress for {WRITE_TIMEOUT:?}")]
    Stalled {},
    /// Opening a stream waited for [`OPEN_TIMEOUT`] at the stream limit of the peer
    #[error("stream limit of the peer reached for {OPEN_TIMEOUT:?}")]
    OpenStalled {},
}

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
/// `max_concurrent_uni_streams` of the peer's endpoint, 100 by default.
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
    ///
    /// The actor shares it between all connections to the peer.
    handles_close: watch::Sender<bool>,
}

impl RecvLoop {
    pub(crate) fn new(
        remote_endpoint_id: EndpointId,
        conn: Connection,
        in_event_tx: mpsc::Sender<InEvent>,
        max_message_size: usize,
        handles_close: watch::Sender<bool>,
    ) -> Self {
        Self {
            remote_endpoint_id,
            conn,
            max_message_size,
            in_event_tx,
            handles_close,
        }
    }

    /// Reads the streams of the connection until it closes or the actor stops.
    ///
    /// A bad stream loses only itself, so this never fails.
    pub(crate) async fn run(&mut self) {
        // Each header is read in its own future, so a partial one blocks no other stream.
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
    queue_lease: QueueLease,
    /// Whether a message is queued or written, or a stream is not yet finished.
    sending: watch::Sender<bool>,
}

impl SendLoop {
    pub(crate) fn new(conn: Connection, queue_lease: QueueLease, max_message_size: usize) -> Self {
        Self {
            conn,
            max_message_size,
            buffer: Default::default(),
            streams: Default::default(),
            finishing: Default::default(),
            queue_lease,
            // The queue may hold messages before the loop takes it.
            sending: watch::channel(true).0,
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

    /// Writes the queue until it ends, the lease is revoked, or the connection closes.
    ///
    /// Takes `self`, so that the lease and the finishing streams drop once it returns.
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
            loop {
                self.set_sending(!queue.is_empty() || !self.finishing.is_empty());
                let more = tokio::select! {
                    biased;
                    _ = &mut closed => break,
                    // The next connection takes the rest of the queue.
                    _ = revoked.cancelled() => break,
                    // Before the queue: the queue of a busy peer is always ready, and
                    // would keep finished streams from being collected.
                    Some(_) = self.finishing.join_next(), if !self.finishing.is_empty() => continue,
                    more = queue.next() => more,
                };
                if !more {
                    break;
                }
                self.set_sending(true);
                let item = queue.current().expect("next returned a message");
                match self.open_stream(item.message.topic, &revoked).await {
                    Ok(true) => {}
                    // The lease ended before the stream opened. The message stays
                    // in the queue for the next connection.
                    Ok(false) => break,
                    Err(err) => {
                        self.handle_error(item.message.topic, err)?;
                        queue.written();
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
                queue.written();
            }
        }
        // Close remaining streams.
        self.set_sending(!self.streams.is_empty() || !self.finishing.is_empty());
        for (topic, stream) in std::mem::take(&mut self.streams) {
            self.finish_stream(topic, stream);
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
    /// if the connection is gone, or if the write or the open made no progress in
    /// time, which closes the connection.
    async fn send(&mut self, item: &QueuedMessage) -> Result<(), WriteError> {
        let message = &item.message;
        match self.write_message(message, item.encoded_len()).await {
            Ok(()) => Ok(()),
            Err(err) => self.handle_error(message.topic, err),
        }
    }

    /// Drops the stream of `topic` after a failed write, or returns the error if the connection fails.
    fn handle_error(&mut self, topic: TopicId, err: WriteError) -> Result<(), WriteError> {
        if matches!(
            err,
            WriteError::Stalled { .. } | WriteError::OpenStalled { .. }
        ) {
            // A peer that reads nothing blocks every stream, so a new stream would not help.
            warn!("{err}, close the connection");
            self.conn.close(CLOSE_STALLED.into(), b"write stalled");
            return Err(err);
        }
        if self.conn.close_reason().is_some() {
            return Err(err);
        }
        warn!(topic = %topic.fmt_short(), "write failed, drop the stream: {err:#}");
        if let Some(mut stream) = self.streams.remove(&topic) {
            stream.reset(0u32.into()).ok();
        }
        Ok(())
    }

    /// Opens the stream of `topic` and writes its header, unless it is open.
    ///
    /// Returns `false` if `revoked` ends the lease before the stream opened. A
    /// header that started is written in full, as a node up to 0.101 drops the
    /// whole connection on a partial one. A failure of the peer closes the
    /// connection anyway, and cuts the header.
    async fn open_stream(
        &mut self,
        topic: TopicId,
        revoked: &CancellationToken,
    ) -> Result<bool, WriteError> {
        let hash_map::Entry::Vacant(entry) = self.streams.entry(topic) else {
            return Ok(true);
        };
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
        entry.insert(stream);
        Ok(true)
    }

    /// Writes a [`ProtoMessage`] of `len` encoded bytes as a length-prefixed frame on its open stream.
    ///
    /// This function is not cancellation-safe.
    async fn write_message(
        &mut self,
        message: &ProtoMessage,
        len: usize,
    ) -> Result<(), WriteError> {
        let ProtoMessage { topic, message } = message;
        let topic_id = *topic;
        let is_last = message.is_disconnect();

        let hash_map::Entry::Occupied(mut entry) = self.streams.entry(topic_id) else {
            unreachable!("the stream was opened before");
        };
        let stream = entry.get_mut();

        match write_sized_frame(
            stream,
            message,
            len,
            &mut self.buffer,
            self.max_message_size,
        )
        .await
        {
            // Refused before any byte went out, so the stream is intact.
            Err(WriteError::TooLarge { .. }) => {
                warn!(max = self.max_message_size, "message too large, dropped");
                return Ok(());
            }
            res => res?,
        }

        if is_last {
            trace!(topic=%topic_id.fmt_short(), "stream closing");
            let stream = entry.remove();
            self.finish_stream(topic_id, stream);
        }

        Ok(())
    }

    /// Finishes `stream`, and waits in [`Self::finishing`] until the peer read all of it.
    fn finish_stream(&mut self, topic: TopicId, mut stream: SendStream) {
        if stream.finish().is_err() {
            return;
        }
        self.finishing.spawn(
            async move {
                stream.stopped().await.ok();
                debug!(topic = %topic.fmt_short(), "stream closed");
            }
            .instrument(tracing::Span::current()),
        );
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
