//! The send and receive loops of a connection, around its control stream.
//!
//! The dialer opens the control stream and writes its header. The acceptor
//! answers with its own. After that the stream carries only `GOAWAY`, with the
//! number of topic streams its sender opened. Either writer sends it when the
//! connection is idle, or when it got the `GOAWAY` of the peer. A side finishes
//! its half of the stream once it read all streams that the peer counted, and
//! closes the connection once the peer finished its half too.

use std::{
    collections::{hash_map::Entry, HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};

use bytes::Bytes;
use iroh::{
    endpoint::{Connection, ConnectionError, RecvStream, SendStream, StreamId, VarInt},
    EndpointId,
};
use n0_error::StdResultExt as _;
use n0_future::{
    boxed::BoxFuture,
    time::{self, Instant},
    FuturesUnordered, StreamExt as _,
};
use tokio::sync::{mpsc, oneshot, watch, Notify};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error_span, trace, warn, Instrument};

use super::{
    auth::TopicAuth,
    peers::{ConnEnd, ConnId},
    queue::{set_if_changed, QueueReader, QueuedMessage},
    wire::{
        read_message, write_all, write_sized_message, CloseCode, ControlMessage, ReadError,
        Session, StreamHeader, WriteError, HEADER_TIMEOUT, OPEN_TIMEOUT,
    },
    ProtoMessage, Verdict,
};
use crate::proto::TopicId;

/// How long a retire may take, from the time the writer stops writing.
pub(super) const RETIRE_TIMEOUT: Duration = Duration::from_secs(if cfg!(test) { 3 } else { 120 });

/// The most rejected topics that a connection remembers.
///
/// The peer picks the topic ids, so the set needs a cap.
const MAX_REJECTED_TOPICS: usize = 1024;

/// The stop code of a topic stream whose token the verifier rejected.
pub(super) const STOP_REJECTED: VarInt = VarInt::from_u32(1);

/// An event from a connection task to the actor.
///
/// The actor drops the events of a connection that it does not know.
#[derive(Debug)]
pub(super) struct ConnEvent {
    pub(super) peer: EndpointId,
    pub(super) conn: ConnId,
    pub(super) kind: ConnEventKind,
}

#[derive(Debug)]
pub(super) enum ConnEventKind {
    /// A message of the peer, in the order that the peer wrote it.
    Message(ProtoMessage),
    /// The peer refused the topic on this connection.
    Refused(TopicId),
    /// We rejected a stream of the peer for the topic.
    Rejected(TopicId),
    /// The session in the control header of the peer.
    Header(Session),
    /// The writer sent `GOAWAY`, and writes nothing more.
    Retiring,
}

/// A topic and the auth token for the header of its streams.
type TopicToken = (TopicId, Option<Bytes>);

/// The command that gives the writer of a connection the queue of the peer.
///
/// It replaces the queue that the writer has, which the actor discarded.
#[derive(Debug)]
pub(super) struct TakeQueue {
    pub(super) reader: QueueReader,
    /// The topics that get a stream at once.
    pub(super) topics: Vec<TopicToken>,
}

/// A slot for one value, which a later value replaces.
///
/// No code keeps the lock across an await.
#[derive(Debug)]
pub(super) struct Mailbox<T> {
    value: Mutex<Option<T>>,
    notify: Notify,
}

impl<T> Default for Mailbox<T> {
    fn default() -> Self {
        Self {
            value: Mutex::new(None),
            notify: Notify::new(),
        }
    }
}

impl<T> Mailbox<T> {
    /// Puts `value` in the slot, and drops the value that nobody took.
    pub(super) fn put(&self, value: T) {
        let old = self.value.lock().expect("poisoned").replace(value);
        drop(old);
        self.notify.notify_one();
    }

    /// Takes the value, if the slot has one.
    pub(super) fn try_take(&self) -> Option<T> {
        self.value.lock().expect("poisoned").take()
    }

    /// Waits for a value and takes it.
    ///
    /// It is cancellation-safe: a value that arrives stays in the slot.
    async fn take(&self) -> T {
        loop {
            if let Some(value) = self.try_take() {
                return value;
            }
            // A put after the check above leaves a permit, so this does not miss it.
            self.notify.notified().await;
        }
    }
}

/// Everything a connection task needs.
pub(super) struct ConnArgs {
    pub(super) id: ConnId,
    pub(super) conn: Connection,
    /// A token that the actor cancels when it aborts the peer.
    ///
    /// The connection then closes with [`CloseCode::Blocked`].
    pub(super) aborted: CancellationToken,
    /// A token that the actor cancels when the connection lost a crossed dial.
    ///
    /// The connection then retires as if it were idle.
    pub(super) retire: CancellationToken,
    /// Gives the writer the queue of the peer.
    pub(super) take_queue: Arc<Mailbox<TakeQueue>>,
    pub(super) shared: ConnShared,
}

/// How a connection task ended.
#[derive(Debug)]
pub(super) struct ConnExit {
    pub(super) end: ConnEnd,
    /// The reader that the writer still had when the connection ended.
    pub(super) reader: Option<QueueReader>,
}

/// The inputs that every connection task shares.
#[derive(Clone)]
pub(super) struct ConnShared {
    pub(super) events: mpsc::Sender<ConnEvent>,
    pub(super) max_message_size: usize,
    /// Checks the token of each stream that the peer opens.
    pub(super) verifier: TopicAuth,
    /// The session that we send in the control header.
    pub(super) session: Session,
    /// How long an idle connection waits before it retires.
    pub(super) retire_after: Duration,
    /// A token that the actor cancels when the node shuts down.
    ///
    /// The writer then finishes its streams once the queue is empty.
    pub(super) shutdown: CancellationToken,
}

/// Returns a finished task's output, or `None` if the runtime cancelled the task.
///
/// A task cancelled at runtime shutdown is not a bug, so it must not panic the actor (#140).
/// A panic in the task panics the caller with the same payload.
pub(super) fn task_output<T>(res: Result<T, n0_future::task::JoinError>) -> Option<T> {
    match res {
        Ok(output) => Some(output),
        Err(err) if err.is_cancelled() => None,
        #[cfg(not(target_family = "wasm"))]
        Err(err) => std::panic::resume_unwind(err.into_panic()),
        #[cfg(target_family = "wasm")]
        Err(err) => panic!("task failed: {err}"),
    }
}

/// The reason why a connection fails, and the code that it closes with.
#[derive(Debug)]
struct Failure {
    code: CloseCode,
    reason: String,
}

impl Failure {
    fn new(code: CloseCode, reason: impl Into<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
        }
    }

    fn protocol(reason: impl Into<String>) -> Self {
        Self::new(CloseCode::Protocol, reason)
    }

    /// The actor stopped. The peer must not read a quiet close, which keeps us as a neighbor.
    fn actor_gone() -> Self {
        Self::new(CloseCode::Shutdown, "dropped by the actor")
    }
}

impl From<WriteError> for Failure {
    fn from(err: WriteError) -> Self {
        let code = match err {
            WriteError::Stalled { .. } | WriteError::OpenStalled { .. } => CloseCode::Blocked,
            _ => CloseCode::SendFailed,
        };
        Self::new(code, err.to_string())
    }
}

/// The signals between the send loop and the receive loop of a connection.
#[derive(Default)]
struct Signals {
    /// Whether the peer has a stream open. The writer does not retire while it has one.
    peer_streams_open: watch::Sender<bool>,
    /// The receive loop notifies it once the peer sent `GOAWAY` and the loop read all its streams.
    ///
    /// The writer then answers, and finishes the control stream.
    recv_drained: Notify,
}

/// Sends the events of a loop to the actor.
#[derive(Clone)]
struct EventSink {
    tx: mpsc::Sender<ConnEvent>,
    peer: EndpointId,
    id: ConnId,
}

impl EventSink {
    async fn send(&self, kind: ConnEventKind) -> Result<(), Failure> {
        let event = ConnEvent {
            peer: self.peer,
            conn: self.id,
            kind,
        };
        self.tx.send(event).await.map_err(|_| Failure::actor_gone())
    }
}

/// Runs the I/O of a connection until it ends, and returns how.
///
/// The connection ends when a loop fails, when the peer state aborts it, or
/// after a finished retire. The task closes it with the code of the failure.
/// It also returns the reader of the queue, if the writer still had it.
pub(super) async fn connection_loop(args: ConnArgs) -> ConnExit {
    let conn = args.conn.clone();
    debug!(side = ?conn.side(), "connection established");
    let aborted = args.aborted.clone();
    // Outside the loops, so that the reader stays when a loop fails.
    let mut reader = None;
    let result = tokio::select! {
        biased;
        _ = aborted.cancelled() => Err(Failure::new(CloseCode::Blocked, "send queue full")),
        res = run_loops(args, &mut reader) => res,
    };
    let end = match result {
        Ok(end) => end,
        Err(failure) => match conn.close_reason() {
            Some(reason) => end_of(Some(reason)),
            None => {
                debug!(code = ?failure.code, "close the connection: {}", failure.reason);
                conn.close(failure.code.code(), failure.reason.as_bytes());
                ConnEnd::Failed
            }
        },
    };
    ConnExit { end, reader }
}

/// Returns how a connection ended, from its close reason.
fn end_of(reason: Option<ConnectionError>) -> ConnEnd {
    match reason {
        Some(ConnectionError::ApplicationClosed(close)) => {
            match CloseCode::from_code(close.error_code) {
                Some(CloseCode::Retired) => ConnEnd::Retired,
                Some(CloseCode::Stale) => ConnEnd::Quiet,
                _ => ConnEnd::Failed,
            }
        }
        _ => ConnEnd::Failed,
    }
}

/// Runs the control handshake and both loops, until both end.
///
/// The send loop keeps the reader of the queue in `reader` while it writes.
async fn run_loops(args: ConnArgs, reader: &mut Option<QueueReader>) -> Result<ConnEnd, Failure> {
    let ConnArgs {
        id,
        conn,
        retire,
        take_queue,
        shared,
        ..
    } = args;
    let ConnShared {
        events,
        max_message_size,
        verifier,
        session,
        retire_after,
        shutdown,
    } = shared;
    let dialer = conn.side().is_client();
    let events = EventSink {
        tx: events,
        peer: conn.remote_id(),
        id,
    };
    let (mut ctl_tx, ctl_rx) = if dialer {
        // A peer that grants no bidirectional stream keeps the open pending.
        time::timeout(HEADER_TIMEOUT, conn.open_bi())
            .await
            .map_err(|_| Failure::protocol("cannot open the control stream in time"))?
            .map_err(|err| Failure::protocol(format!("cannot open the control stream: {err:#}")))?
    } else {
        time::timeout(HEADER_TIMEOUT, conn.accept_bi())
            .await
            .map_err(|_| Failure::protocol("no control stream"))?
            .map_err(|err| Failure::protocol(format!("no control stream: {err:#}")))?
    };
    write_all(&mut ctl_tx, &session.to_frame()).await?;
    let signals = Arc::new(Signals::default());
    let recv_loop = RecvLoop {
        conn: conn.clone(),
        events: events.clone(),
        max_message_size,
        verifier,
        rejected_topics: Default::default(),
        signals: signals.clone(),
    };
    let send_loop = SendLoop {
        conn: conn.clone(),
        events,
        signals,
        ctl: ctl_tx,
        retire,
        take_queue,
        shutdown,
        retire_after,
        max_message_size,
        streams: Default::default(),
        stream_ends: Default::default(),
        buffer: Default::default(),
        topics_to_open: Default::default(),
        opened_count: 0,
        refused_topics: Default::default(),
        retiring_until: None,
    };
    let send = async { send_loop.run(reader).await.map_err(Failure::from) }
        .instrument(error_span!("send"));
    let recv = async {
        let mut ctl_rx = ctl_rx;
        recv_loop.read_header(&mut ctl_rx).await?;
        recv_loop.run(ctl_rx).await
    }
    .instrument(error_span!("recv"));
    tokio::try_join!(send, recv).map(|(_, end)| end)
}

/// Reads the streams that the peer opens on a connection.
struct RecvLoop {
    conn: Connection,
    events: EventSink,
    max_message_size: usize,
    verifier: TopicAuth,
    /// The topics whose token the verifier rejected on this connection, at most [`MAX_REJECTED_TOPICS`].
    ///
    /// The loop stops a later stream of such a topic, and does not call the verifier.
    rejected_topics: HashSet<TopicId>,
    signals: Arc<Signals>,
}

/// The result of a stream read: the stream, and its next message or its end.
type ReadResult = (TopicReader, Result<Option<ProtoMessage>, ReadError>);

impl RecvLoop {
    /// Reads the control header of the peer, and tells the actor its session.
    async fn read_header(&self, ctl: &mut RecvStream) -> Result<(), Failure> {
        let session = Session::read(ctl, &mut Vec::new())
            .await
            .map_err(|err| Failure::protocol(format!("control header: {err:#}")))?;
        self.events.send(ConnEventKind::Header(session)).await?;
        Ok(())
    }

    /// Reads the streams of the connection until it closes, or until a retire finished.
    ///
    /// The loop reads one header at a time, so streams decode in the order of
    /// their ids. A newer stream of a topic waits until the older one ended. So
    /// the actor sees the messages of a topic in the order the peer wrote them.
    /// A late header, a bad header, a bad frame, or a reset stream fails the
    /// connection.
    async fn run(mut self, mut ctl: RecvStream) -> Result<ConnEnd, Failure> {
        // The header of the stream that we accepted last, while we read it.
        let mut header: Option<BoxFuture<Result<TopicReader, ReadError>>> = None;
        // The next message of each stream that we read.
        let mut reads: FuturesUnordered<BoxFuture<ReadResult>> = FuturesUnordered::new();
        // The topics with a stream that the loop reads, and the streams that wait for them.
        let mut waiting: HashMap<TopicId, VecDeque<TopicReader>> = HashMap::new();
        // The streams of the peer that ended, and how many it opened, once it said so.
        let mut ended = 0u64;
        let mut peer_opened = None::<u64>;

        // Reads the `GOAWAY`, then ends when the peer finished the stream: it read all of ours.
        // This future owns `ctl` while the loop uses `self`, so `ctl` is not a field of `self`.
        let (goaway_tx, mut goaway_rx) = oneshot::channel();
        let mut control = Box::pin(async move {
            let ControlMessage::Goaway(n) = ControlMessage::read(&mut ctl, &mut Vec::new())
                .await
                .anyerr()?;
            debug!(n, "GOAWAY received");
            goaway_tx.send(n).ok();
            ctl.read_to_end(0).await.anyerr().map(drop)
        });
        let mut control_finished = false;

        let conn = self.conn.clone();
        let closed = conn.closed();
        tokio::pin!(closed);
        let mut conn_closed = false;

        // After a close, the loop waits for the header and the reads that are under way.
        while !conn_closed || header.is_some() || !reads.is_empty() {
            // A header that the loop still reads counts as an open stream. It times out
            // after `HEADER_TIMEOUT`.
            let streams_open = header.is_some() || !waiting.is_empty();
            set_if_changed(&self.signals.peer_streams_open, streams_open);

            tokio::select! {
                _ = &mut closed, if !conn_closed => conn_closed = true,

                // A new stream of the peer. We accept the next one after its header.
                stream = self.conn.accept_uni(), if !conn_closed && header.is_none() => match stream {
                    Ok(stream) => header = Some(Box::pin(TopicReader::accept(stream, self.max_message_size))),
                    Err(_) => conn_closed = true,
                },

                // The header of a new stream: verify it, then read the stream, or let it wait.
                reader = async { header.as_mut().expect("guarded").await }, if header.is_some() => {
                    header = None;
                    let mut reader = reader.map_err(|err| Failure::protocol(format!("stream header: {err:#}")))?;
                    let topic = reader.header.topic;
                    debug!(topic=%topic.fmt_short(), "stream opened");
                    if self.verify(&mut reader) == Verdict::Reject {
                        // A stopped stream ends here.
                        ended += 1;
                        self.events.send(ConnEventKind::Rejected(topic)).await?;
                    } else if let Some(queue) = waiting.get_mut(&topic) {
                        queue.push_back(reader);
                    } else {
                        waiting.insert(topic, VecDeque::new());
                        reads.push(Box::pin(reader.read_next()));
                    }
                }

                // A message of a stream, or its end.
                Some((reader, res)) = reads.next(), if !reads.is_empty() => {
                    let topic = reader.header.topic;
                    match res {
                        Ok(Some(msg)) => {
                            self.events.send(ConnEventKind::Message(msg)).await?;
                            reads.push(Box::pin(reader.read_next()));
                            continue;
                        }
                        Ok(None) => debug!(topic=%topic.fmt_short(), "stream closed"),
                        Err(err) => {
                            // A close fails each read. Its code decides how the connection ended.
                            if self.conn.close_reason().is_some() {
                                conn_closed = true;
                                continue;
                            }
                            return Err(Failure::protocol(format!("topic stream: {err:#}")));
                        }
                    }
                    // The stream ended. The loop now reads the next stream of its topic, if any.
                    ended += 1;
                    match waiting.get_mut(&topic).and_then(VecDeque::pop_front) {
                        Some(next) => reads.push(Box::pin(next.read_next())),
                        None => {
                            waiting.remove(&topic);
                        }
                    }
                }

                Ok(n) = &mut goaway_rx, if !goaway_rx.is_terminated() => peer_opened = Some(n),

                res = &mut control, if !control_finished => match res {
                    Ok(()) => control_finished = true,
                    // A close ends the stream, and its close code decides how the connection ended.
                    Err(_) if self.conn.close_reason().is_some() => control_finished = true,
                    Err(err) => return Err(Failure::protocol(format!("control stream: {err:#}"))),
                },
            }

            // The peer retires, and we read all the streams that it counted: the writer answers.
            let drained = peer_opened.is_some_and(|opened| ended >= opened);
            if drained {
                self.signals.recv_drained.notify_one();
            }
            // We read all the peer wrote, and the peer read all we wrote: close.
            if drained && control_finished {
                return Ok(match self.conn.close_reason() {
                    None => {
                        self.conn.close(CloseCode::Retired.code(), b"retired");
                        ConnEnd::Retired
                    }
                    reason => end_of(reason),
                });
            }
        }

        debug!("recv loop closed");
        Ok(end_of(self.conn.close_reason()))
    }

    /// Asks the verifier about a new stream, and stops the stream if it rejects.
    fn verify(&mut self, reader: &mut TopicReader) -> Verdict {
        let topic = reader.header.topic;
        let verdict = if self.rejected_topics.contains(&topic) {
            Verdict::Reject
        } else {
            // The loop needs the token only here.
            let token = reader.header.token.take();
            self.verifier
                .verify(topic, self.conn.remote_id(), token.as_deref())
        };
        if verdict == Verdict::Reject {
            debug!(topic=%topic.fmt_short(), "stream rejected, stop it");
            // Past the cap, the loop does not remember the topic, and the verifier sees its
            // streams again.
            if self.rejected_topics.len() < MAX_REJECTED_TOPICS {
                self.rejected_topics.insert(topic);
            }
            reader.stream.stop(STOP_REJECTED).ok();
        }
        verdict
    }
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
        let header = StreamHeader::read(&mut stream, &mut buffer).await?;
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

/// Writes the queue of the peer on a connection, and retires the connection when it is idle.
struct SendLoop {
    conn: Connection,
    events: EventSink,
    signals: Arc<Signals>,
    /// The send half of the control stream, for the `GOAWAY`.
    ctl: SendStream,
    retire: CancellationToken,
    take_queue: Arc<Mailbox<TakeQueue>>,
    shutdown: CancellationToken,
    retire_after: Duration,
    max_message_size: usize,
    streams: HashMap<TopicId, SendStream>,
    buffer: Vec<u8>,
    /// A future per stream, which resolves once the peer stopped it or acknowledged all of it.
    stream_ends: FuturesUnordered<BoxFuture<(TopicId, StreamId, Option<VarInt>)>>,
    /// The topics for which the writer opens a stream when it has the queue.
    topics_to_open: Vec<TopicToken>,
    /// How many topic streams the writer opened on this connection.
    opened_count: u64,
    /// The topics that the peer refused on this connection.
    ///
    /// We open no stream for such a topic, and drop its queued messages. A new
    /// connection starts without refusals, so the writer tries the topic again there.
    refused_topics: HashSet<TopicId>,
    /// The time at which a retire fails the connection.
    ///
    /// The writer sets it to [`RETIRE_TIMEOUT`] from now when it stops writing. It then
    /// writes nothing more.
    retiring_until: Option<Instant>,
}

impl SendLoop {
    /// Writes the queue that the actor gives it, and retires the connection when it is idle.
    ///
    /// Either writer retires when no stream was open in either direction and
    /// the queue was empty for `retire_after`. It finishes its streams, and
    /// sends `GOAWAY` with the number of streams it opened. It does the same
    /// when the receive loop read all streams that the peer counted in its
    /// `GOAWAY`. It then also finishes the control stream, and leaves its
    /// queue for the next connection.
    ///
    /// After the `GOAWAY` the writer writes nothing. It tells the actor, which
    /// dials for a later message.
    ///
    /// After the queue closed, the writer finishes its streams and waits for
    /// the queue to open again. It never leaves a message unfinished, except
    /// when it fails.
    ///
    /// The writer keeps the reader in `queue`, which outlives this future. The
    /// reader goes back with the end of the connection, also after a retire.
    /// So the next writer starts once the peer read all that this one wrote.
    async fn run(mut self, queue: &mut Option<QueueReader>) -> Result<(), WriteError> {
        let conn = self.conn.clone();
        let closed = conn.closed();
        tokio::pin!(closed);
        let mut peer_streams_open = self.signals.peer_streams_open.subscribe();
        let mut queue_closed: Option<watch::Receiver<bool>> = None;
        let mut quiet_since: Option<Instant> = None;

        loop {
            let retiring = self.retiring_until.is_some();
            let writing = queue.is_some() && !retiring;
            // A close of the queue finishes our streams, if we have any.
            let wants_close =
                writing && (!self.streams.is_empty() || !self.topics_to_open.is_empty());
            // Nothing is open or queued in either direction: the idle time runs.
            let quiet = !retiring
                && self.streams.is_empty()
                && self.stream_ends.is_empty()
                && self.topics_to_open.is_empty()
                && queue.as_ref().is_none_or(|queue| queue.is_empty())
                && !*peer_streams_open.borrow_and_update();
            let retire_at =
                quiet.then(|| *quiet_since.get_or_insert_with(Instant::now) + self.retire_after);
            if !quiet {
                quiet_since = None;
            }

            tokio::select! {
                biased;

                _ = &mut closed => break,

                // The connection lost a crossed dial: the same retire as for an idle one.
                _ = self.retire.cancelled(), if !retiring => self.retire().await?,

                TakeQueue { reader, topics } = self.take_queue.take() => {
                    // The new queue replaces the old one. The old streams end first.
                    self.streams.clear();
                    queue_closed = Some(reader.closed.clone());
                    *queue = Some(reader);
                    // A writer that retires keeps the reader, and writes nothing.
                    if !retiring {
                        self.topics_to_open = topics;
                    }
                }

                // The peer retires and we read all it sent: answer, and finish the stream, so
                // that the peer may close. This arm comes before the queue, so that a busy queue
                // does not delay a retire.
                _ = self.signals.recv_drained.notified() => {
                    self.retire().await?;
                    self.ctl.finish().ok();
                }

                // This arm comes before the queue. The queue of a busy peer is always ready,
                // and would keep the loop from collecting ended streams.
                Some((topic, id, stop)) = self.stream_ends.next(), if !self.stream_ends.is_empty() => {
                    self.on_stream_end(topic, id, stop).await?;
                }

                // The next message of the queue, or the end of the queue.
                more = async {
                    match (&mut *queue, writing) {
                        (Some(queue), true) => queue.wait_next().await,
                        _ => std::future::pending().await,
                    }
                } => if more {
                    let queue = queue.as_mut().expect("a message came from the queue");
                    self.write_current(queue).await?;
                } else {
                    // The actor dropped the queue.
                    self.streams.clear();
                    *queue = None;
                },

                // These come after the queue, so they only run when the queue is empty.
                _ = self.shutdown.cancelled() => {
                    self.stop_writing();
                    break;
                }

                // We finish our streams and keep waiting on this connection, so the next
                // message does not need a dial. Dropping a stream finishes it.
                _ = async {
                    match &mut queue_closed {
                        Some(queue_closed) if wants_close => {
                            queue_closed.wait_for(|closed| *closed).await.ok();
                        }
                        _ => std::future::pending().await,
                    }
                } => {
                    self.drop_streams();
                }

                // After the queue, so that queued messages get the streams of the peer
                // first. A quiet topic needs a stream to keep the connection.
                Some((topic, token)) = async {
                    if writing { self.topics_to_open.pop() } else { None }
                } => match self.open_stream(topic, token).await {
                    // At the stream limit of the peer, the quiet topics get no stream.
                    Err(WriteError::OpenStalled { .. }) => self.topics_to_open.clear(),
                    Err(err) => tolerate_refusal(topic, err)?,
                    Ok(()) => {}
                },

                // The connection was idle for `retire_after`.
                () = time::sleep_until(retire_at.unwrap_or_else(Instant::now)), if retire_at.is_some() => self.retire().await?,

                // The retire did not finish in time.
                () = time::sleep_until(self.retiring_until.unwrap_or_else(Instant::now)), if retiring => return Err(n0_error::e!(WriteError::Stalled)),

                // The peer opened or ended a stream, which restarts the idle time.
                _ = peer_streams_open.changed() => {}
            }
        }

        // Finish the streams, and wait until the peer acknowledged or stopped each one.
        self.streams.clear();
        while self.stream_ends.next().await.is_some() {}
        // The writer ended without a close: the node shuts down. If the peer closed the
        // connection, its close code stays.
        if conn.close_reason().is_none() {
            conn.close(CloseCode::Shutdown.code(), b"shutdown");
        }
        debug!("send loop closed");
        Ok(())
    }

    /// Handles a stream that the peer stopped, or acknowledged in full.
    async fn on_stream_end(
        &mut self,
        topic: TopicId,
        id: StreamId,
        stop: Option<VarInt>,
    ) -> Result<(), WriteError> {
        // Only the reject code is expected. Any other stop fails the connection.
        let rejected = match stop {
            Some(code) if code != STOP_REJECTED => {
                return Err(n0_error::e!(WriteError::Stopped {
                    code: code.into_inner()
                }));
            }
            stop => stop.is_some(),
        };
        // The next message of the topic opens a new stream.
        if self
            .streams
            .get(&topic)
            .is_some_and(|stream| stream.id() == id)
        {
            self.streams.remove(&topic);
        }
        // The peer refuses the topic, not the connection. Tell the
        // protocol once, and send nothing more for the topic.
        if rejected && self.refused_topics.insert(topic) {
            warn!(topic = %topic.fmt_short(), "the peer refused the topic");
            self.streams.remove(&topic);
            self.topics_to_open.retain(|(t, _)| *t != topic);
            self.events.send(ConnEventKind::Refused(topic)).await.ok();
        }
        Ok(())
    }

    /// Stops writing and sends `GOAWAY`, only on the first call.
    async fn retire(&mut self) -> Result<(), WriteError> {
        if self.retiring_until.is_some() {
            return Ok(());
        }
        self.stop_writing();
        debug!(opened = self.opened_count, "send GOAWAY");
        write_all(
            &mut self.ctl,
            &ControlMessage::Goaway(self.opened_count).to_frame(),
        )
        .await?;
        // The actor never waits for a send loop, so this does not deadlock.
        self.events.send(ConnEventKind::Retiring).await.ok();
        Ok(())
    }

    /// Finishes the streams, and opens none of the pending ones.
    fn drop_streams(&mut self) {
        self.streams.clear();
        self.topics_to_open.clear();
    }

    /// Finishes the streams, and writes nothing more.
    fn stop_writing(&mut self) {
        self.drop_streams();
        self.retiring_until = Some(Instant::now() + RETIRE_TIMEOUT);
    }

    /// Writes the message that the queue has ready.
    async fn write_current(&mut self, queue: &mut QueueReader) -> Result<(), WriteError> {
        let item = queue.current().expect("wait_next returned a message");
        let topic = item.message.topic;
        if self.refused_topics.contains(&topic) {
            trace!(topic = %topic.fmt_short(), "the peer refused the topic, drop the message");
            // The protocol took the peer back, for example after a second `Join`.
            let message = &item.message.message;
            let readded = message.is_membership() && !message.is_disconnect();
            queue.advance();
            if readded {
                self.events.send(ConnEventKind::Refused(topic)).await.ok();
            }
            return Ok(());
        }
        if let Err(err) = self.open_stream(topic, item.token.clone()).await {
            tolerate_refusal(topic, err)?;
            queue.advance();
            return Ok(());
        }
        self.send(item).await?;
        // A topic that we just left needs no stream.
        if item.message.message.is_disconnect() {
            self.topics_to_open.retain(|(t, _)| *t != topic);
        }
        queue.advance();
        Ok(())
    }

    /// Writes a message on the stream of its topic, and finishes the stream after a `Close`.
    ///
    /// If the write fails because the peer refused the stream, the message is lost.
    /// Returns the error if the write failed otherwise, which fails the connection.
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
                tolerate_refusal(*topic, err)?;
                self.streams.remove(topic);
                Ok(())
            }
        }
    }

    /// Opens the stream of `topic` and writes its header, unless it is open.
    ///
    /// The header goes out in full once the stream opened. If the actor aborts the
    /// peer, the connection closes anyway and cuts the header.
    async fn open_stream(
        &mut self,
        topic: TopicId,
        token: Option<Bytes>,
    ) -> Result<(), WriteError> {
        let Entry::Vacant(entry) = self.streams.entry(topic) else {
            return Ok(());
        };
        let mut stream = time::timeout(OPEN_TIMEOUT, self.conn.open_uni())
            .await
            .map_err(|_| n0_error::e!(WriteError::OpenStalled))??;
        StreamHeader::write(topic, token.as_deref(), &mut stream, &mut self.buffer).await?;
        self.opened_count += 1;
        debug!(topic=%topic.fmt_short(), "stream opened");
        // The QUIC stack of the peer acknowledges data that its app did not read
        // yet, so this needs no timeout.
        let (id, stopped) = (stream.id(), stream.stopped());
        self.stream_ends.push(Box::pin(async move {
            let stop = stopped.await.ok().flatten();
            (topic, id, stop)
        }));
        entry.insert(stream);
        Ok(())
    }
}

/// Returns the error if the write failed in a way that fails the connection.
///
/// A stream that the peer stopped with the reject code only loses its message.
fn tolerate_refusal(topic: TopicId, err: WriteError) -> Result<(), WriteError> {
    match &err {
        WriteError::Stopped { code, .. } if *code == STOP_REJECTED.into_inner() => {
            debug!(topic = %topic.fmt_short(), "the peer refused the stream, drop the message");
            Ok(())
        }
        _ => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A put replaces the value that nobody took, and a take that waits gets the next put.
    #[tokio::test]
    async fn mailbox_keeps_the_latest_value() {
        let mailbox = Mailbox::default();
        mailbox.put(1);
        mailbox.put(2);
        assert_eq!(mailbox.take().await, 2);
        assert_eq!(mailbox.try_take(), None);
        let mut take = std::pin::pin!(mailbox.take());
        assert!(n0_future::future::poll_once(&mut take).await.is_none());
        mailbox.put(3);
        assert_eq!(take.await, 3);
    }
}
