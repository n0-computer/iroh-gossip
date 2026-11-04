//! The tasks of a connection: the send and receive loops, and the connection loop around them.

use std::{
    collections::{hash_map, BTreeMap, HashMap},
    time::Duration,
};

use iroh::{
    endpoint::{Connection, ConnectionError, RecvStream, SendStream, StreamId},
    EndpointId, PublicKey,
};
use n0_error::{e, stack_error};
use n0_future::{boxed::BoxFuture, task, time, FuturesUnordered, StreamExt as _};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error_span, warn, Instrument};

use super::{
    queue::{set_if_changed, QueueLease, QueuedMessage},
    wire::{read_message, write_sized_message, ReadError, StreamHeader, WriteError, OPEN_TIMEOUT},
    InEvent, ProtoMessage,
};
use crate::proto::TopicId;

/// How long a connection stays open once neither side has a stream on it.
///
/// A topic that uses the peer keeps a stream open. Each side times the grace on
/// its own, so a stream that we open as the peer closes loses its message.
#[cfg(not(test))]
const RETIRE_GRACE: Duration = Duration::from_secs(10);
/// The e2e tests build without `cfg(test)`, so they see 10 s.
#[cfg(test)]
pub(super) const RETIRE_GRACE: Duration = Duration::from_secs(5);

/// How long a replaced connection may write its current message before it hands it over.
///
/// The new connection of the peer waits for it, so that we write the messages
/// in order. A stream header that started is written in full first. Only
/// the write timeout bounds it, which counts the time without progress.
#[cfg(not(test))]
const HANDOVER_LIMIT: Duration = Duration::from_secs(5);
#[cfg(test)]
pub(super) const HANDOVER_LIMIT: Duration = Duration::from_secs(1);

/// The code of a close for a peer whose send queue had no room for a control message.
pub(super) const CLOSE_QUEUE_FULL: u32 = 2;

/// The code of a close for a write or a stream open that made no progress in time.
pub(super) const CLOSE_STALLED: u32 = 3;

/// The code of a close for a connection without streams, see [`RETIRE_GRACE`](super::RETIRE_GRACE).
pub(super) const CLOSE_RETIRED: u32 = 4;

/// The maximum number of stream headers that a [`RecvLoop`] reads at the same time.
///
/// At this limit, the loop accepts no new stream until a header is complete or
/// late. A peer that keeps many partial headers thus delays only its own streams.
pub(super) const MAX_PENDING_HEADERS: usize = 16;

#[allow(missing_docs)]
#[stack_error(derive, add_meta, from_sources, std_sources)]
#[non_exhaustive]
pub(super) enum ConnectionLoopError {
    #[error(transparent)]
    Write {
        source: WriteError,
    },
    #[error(transparent)]
    Connection {
        #[error(std_err)]
        source: iroh::endpoint::ConnectionError,
    },
    ActorDropped {},
}

impl<T> From<mpsc::error::SendError<T>> for ConnectionLoopError {
    fn from(_value: mpsc::error::SendError<T>) -> Self {
        e!(ConnectionLoopError::ActorDropped)
    }
}

/// Returns a finished task's output, or `None` if the runtime cancelled the task.
///
/// A task cancelled at runtime shutdown is not a bug, so it must not panic the actor (#140).
/// A panic in the task panics the caller with the same payload.
pub(super) fn task_output<T>(res: Result<T, task::JoinError>) -> Option<T> {
    match res {
        Ok(output) => Some(output),
        Err(err) if err.is_cancelled() => None,
        #[cfg(not(target_family = "wasm"))]
        Err(err) => std::panic::resume_unwind(err.into_panic()),
        #[cfg(target_family = "wasm")]
        Err(err) => panic!("task failed: {err}"),
    }
}

/// Returns once the connection had no stream in either direction for [`RETIRE_GRACE`].
pub(super) async fn retire(mut ours: watch::Receiver<bool>, mut theirs: watch::Receiver<bool>) {
    loop {
        let ours_open = *ours.borrow_and_update();
        let theirs_open = *theirs.borrow_and_update();
        tokio::select! {
            // A stream that opens at the deadline keeps the connection.
            biased;
            // A dropped sender would make `changed` return at once.
            res = ours.changed() => if res.is_err() { std::future::pending().await },
            res = theirs.changed() => if res.is_err() { std::future::pending().await },
            () = time::sleep(RETIRE_GRACE), if !ours_open && !theirs_open => return,
        }
    }
}

/// Runs the I/O of a connection, and closes it on an abort or once it carries no stream.
///
/// Returns `true` if the end of the connection does not count as a disconnect.
/// That is the case when either side retired the connection, or when no topic
/// used the peer at the moment the connection ended. A connection to a node up
/// to 0.101 never retires. Without a lease, the connection only receives.
/// Every connection observes the peer abort signal, independently of whether
/// it writes.
// Each argument is state the loops share. A struct would only wrap them.
#[allow(clippy::too_many_arguments)]
pub(super) async fn connection_loop(
    from: PublicKey,
    conn: Connection,
    aborted: CancellationToken,
    queue_lease: Option<QueueLease>,
    initial_topics: Vec<TopicId>,
    in_event_tx: mpsc::Sender<InEvent>,
    max_message_size: usize,
    handles_close: watch::Sender<bool>,
) -> Result<bool, ConnectionLoopError> {
    debug!(side = ?conn.side(), "connection established");
    let mut handles_close_rx = handles_close.subscribe();
    // Here, so that the flags outlive the loops. The send flag starts set, as a
    // new writer can wait for the lock of the queue.
    let our_streams = watch::Sender::new(queue_lease.is_some());
    let their_streams = watch::Sender::new(false);

    let queue_closed = queue_lease.as_ref().map(QueueLease::closing);
    let send_loop = queue_lease.map(|lease| {
        let flag = our_streams.clone();
        SendLoop::new(conn.clone(), lease, max_message_size, flag, initial_topics)
    });
    let mut recv_loop = RecvLoop::new(
        from,
        conn.clone(),
        in_event_tx,
        max_message_size,
        handles_close,
        their_streams.clone(),
    );

    let io = async {
        let recv_fut = recv_loop.run().instrument(error_span!("recv"));
        tokio::pin!(recv_fut);
        if let Some(send_loop) = send_loop {
            tokio::select! {
                res = send_loop.run().instrument(error_span!("send")) => res?,
                _ = &mut recv_fut => return Ok(()),
            }
        }
        // We are done sending. The peer may still send on this connection, as two
        // peers that dialed each other can each use a different one.
        recv_fut.await;
        Ok(())
    };
    let retirement = async {
        // Waits until the peer opened a stream.
        if handles_close_rx.wait_for(|known| *known).await.is_err() {
            std::future::pending::<()>().await;
        }
        retire(our_streams.subscribe(), their_streams.subscribe()).await;
    };
    tokio::select! {
        biased;
        // All connections close, including receive-only ones, so the peer
        // learns it on the connection that it uses. A
        // node up to 0.101 may then lose its later messages to us (#155). It
        // missed a control message, so its views differ from ours anyway.
        _ = aborted.cancelled() => {
            warn!("the peer is aborted, close the connection");
            conn.close(CLOSE_QUEUE_FULL.into(), b"send queue full");
            Ok(false)
        }
        res = io => {
            // A retirement by the peer counts also when it cut one of our writes.
            let retired_by_peer = matches!(
                conn.close_reason(),
                Some(ConnectionError::ApplicationClosed(close))
                    if close.error_code == CLOSE_RETIRED.into()
            );
            // Read now, as a topic may open the queue before the actor sees the end.
            // A message from then on needs a new dial, not a disconnect.
            let queue_was_closed = queue_closed.is_some_and(|closed| *closed.borrow());
            if retired_by_peer || queue_was_closed {
                Ok(true)
            } else {
                res.map(|()| false)
            }
        }
        _ = retirement => {
            debug!("no stream for a while, close");
            conn.close(CLOSE_RETIRED.into(), b"no stream");
            Ok(true)
        }
    }
}

pub(super) struct RecvLoop {
    remote_endpoint_id: EndpointId,
    conn: Connection,
    max_message_size: usize,
    in_event_tx: mpsc::Sender<InEvent>,
    /// Whether the peer opened a stream, which shows that it handles a close.
    ///
    /// The actor shares it between all connections to the peer.
    handles_close: watch::Sender<bool>,
    /// Whether the peer has a stream open to us, from its first byte on.
    has_streams: watch::Sender<bool>,
}

/// The result of a stream read: the stream, and its next message or its end.
type ReadResult = (TopicReader, Result<Option<ProtoMessage>, ReadError>);

impl RecvLoop {
    pub(super) fn new(
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
    pub(super) async fn run(&mut self) {
        // Each header is read in its own future, so a partial one blocks no other stream.
        let mut header_futures = FuturesUnordered::new();
        let mut read_futures = FuturesUnordered::new();
        // For each topic, the streams in the order the peer opened them. The peer
        // only opens a new stream for a topic after the previous one ended. So if
        // a stream waits for the older ones, the order is kept and the wait ends.
        let mut streams: BTreeMap<(TopicId, StreamId), Order> = BTreeMap::new();
        let mut conn_is_closed = false;
        let closed = self.conn.closed();
        tokio::pin!(closed);
        while !conn_is_closed || !header_futures.is_empty() || !read_futures.is_empty() {
            // A header that the loop still reads counts as an open stream. It times out
            // after `HEADER_TIMEOUT`.
            let open = !header_futures.is_empty() || !read_futures.is_empty();
            set_if_changed(&self.has_streams, open);
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
                    header_futures.push(TopicReader::accept(stream, self.max_message_size));
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
                    debug!(topic=%state.header.topic.fmt_short(), "stream opened");
                    // Every peer on this ALPN handles a close.
                    self.handles_close.send_replace(true);
                    let (topic, id) = (state.header.topic, state.stream.id());
                    // A stream waits while there is an older stream for the same topic. If
                    // the header of an older stream arrives late, we read that stream right
                    // away, and it does not wait for the newer one.
                    let before = streams.range(..(topic, id)).next_back();
                    let is_first_for_topic = before.is_none_or(|((t, _), _)| *t != topic);
                    if is_first_for_topic {
                        streams.insert((topic, id), Order::Reading);
                        read_futures.push(state.read_next());
                    } else {
                        streams.insert((topic, id), Order::Waiting(state));
                    }
                }
                Some((state, res)) = read_futures.next(), if !read_futures.is_empty() => {
                    match res {
                        Ok(Some(msg)) => {
                            if self.in_event_tx.send(InEvent::RecvMessage(self.remote_endpoint_id, msg)).await.is_err() {
                                debug!("stop recv loop: actor closed");
                                break;
                            }
                            read_futures.push(state.read_next());
                            continue;
                        }
                        Ok(None) => {
                            debug!(topic=%state.header.topic.fmt_short(), "stream closed");
                        }
                        Err(err) => debug!("recv stream closed with error: {err:#}"),
                    }
                    let (topic, id) = (state.header.topic, state.stream.id());
                    streams.remove(&(topic, id));
                    // Start reading the next stream for this topic, unless we already do.
                    let after = streams.range_mut((topic, id)..).next();
                    let next_for_topic = after.filter(|((t, _), _)| *t == topic);
                    if let Some((_, order)) = next_for_topic {
                        if let Order::Waiting(next) = std::mem::replace(order, Order::Reading) {
                            read_futures.push(next.read_next());
                        }
                    }
                }
            }
        }
        debug!("recv loop closed");
    }
}

/// Whether a stream in a [`RecvLoop`] is read now, or waits for an older stream of the same topic.
enum Order {
    Reading,
    Waiting(TopicReader),
}

/// A topic stream that we read: its header, and the buffer for its frames.
#[derive(Debug)]
struct TopicReader {
    stream: RecvStream,
    header: StreamHeader,
    buffer: Vec<u8>,
    max_message_size: usize,
}

impl TopicReader {
    /// Reads the header of a new stream.
    async fn accept(mut stream: RecvStream, max_message_size: usize) -> Result<Self, ReadError> {
        let mut buffer = Vec::new();
        let header = StreamHeader::read(&mut stream, &mut buffer, max_message_size).await?;
        Ok(Self {
            buffer,
            max_message_size,
            stream,
            header,
        })
    }

    /// Reads the next message of the stream, or `None` if it ended gracefully.
    ///
    /// Returns `self` together with the result, also when the read failed.
    async fn read_next(mut self) -> ReadResult {
        let msg = read_message(&mut self.stream, &mut self.buffer, self.max_message_size).await;
        let topic = self.header.topic;
        let msg = msg.map(|msg| msg.map(|message| ProtoMessage { topic, message }));
        (self, msg)
    }
}

pub(super) struct SendLoop {
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
    pub(super) fn new(
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
    pub(super) async fn run(mut self) -> Result<(), WriteError> {
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
            let mut closing = self.queue_lease.closing();
            loop {
                // Every pass of the loop starts here, so the flag clears with the last stream.
                if self.live_streams.is_empty() {
                    set_if_changed(&self.has_streams, false);
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
                    more = queue.wait_next() => more,
                    // This comes after the queue, so it only runs when the queue is empty. We
                    // finish our streams and keep waiting on this connection, so the next
                    // message does not need a dial. Dropping a stream finishes it.
                    _ = async { closing.wait_for(|closing| *closing).await.ok(); },
                        if !self.streams.is_empty() || !self.initial_topics.is_empty() =>
                    {
                        self.streams.clear();
                        self.initial_topics.clear();
                        continue;
                    }
                    // After the queue, so that queued messages get the streams of the peer
                    // first. A quiet topic needs a stream to keep the connection.
                    Some(topic) = async { self.initial_topics.pop() } => {
                        match self.open_stream(topic, &revoked).await {
                            // At the stream limit of the peer, the quiet topics go without.
                            Err(WriteError::OpenStalled { .. }) => self.initial_topics.clear(),
                            Err(err) => self.handle_error(topic, err)?,
                            Ok(_) => {}
                        }
                        continue;
                    }
                };
                if !more {
                    // The actor dropped the queue.
                    break;
                }
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
                    self.initial_topics
                        .retain(|topic| *topic != item.message.topic);
                }
                queue.advance();
            }
        }
        self.streams.clear();
        while self.live_streams.next().await.is_some() {}
        set_if_changed(&self.has_streams, false);
        debug!("send loop closed");
        Ok(())
    }

    /// Writes a message on the stream of its topic, and finishes the stream after a `Close`.
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
        match write_sized_message(
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
                    // Dropping a stream finishes it.
                    self.streams.remove(topic);
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
        set_if_changed(&self.has_streams, true);
        let mut stream = tokio::select! {
            biased;
            // Before the open, so that a revoked lease starts no header.
            _ = revoked.cancelled() => return Ok(false),
            open = n0_future::time::timeout(OPEN_TIMEOUT, self.conn.open_uni()) => {
                open.map_err(|_| e!(WriteError::OpenStalled))??
            }
        };
        StreamHeader::write(topic, &mut stream, &mut self.buffer, self.max_message_size).await?;
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
