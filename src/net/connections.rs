use std::{
    collections::{hash_map, HashMap},
    ops::Deref,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
};

use anyhow::Result;
use iroh::{
    endpoint::{Connection, ConnectionError, RecvStream, SendStream},
    Endpoint, NodeId,
};
use n0_future::task::AbortOnDropHandle;
use postcard::experimental::max_size::MaxSize;
use quinn::ReadExactError;
use serde::{Deserialize, Serialize};
use tokio::{
    io::AsyncReadExt,
    sync::{mpsc, oneshot, Notify},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error_span, info, Instrument};

use crate::{proto::TopicId, ALPN};

#[derive(Debug)]
pub struct TopicRecvStream {
    pub topic_id: TopicId,
    pub node_id: NodeId,
    pub recv_stream: RecvStream,
    pub conn: StrongConnection,
}

#[derive(Debug)]
struct ConnectionsActor {
    endpoint: Endpoint,
    waiting: HashMap<NodeId, Vec<oneshot::Sender<Result<StrongConnection>>>>,
    active: HashMap<NodeId, WeakConnection>,
    dial_tasks: JoinSet<(NodeId, Result<Connection>)>,
    conn_tasks: JoinSet<(NodeId, Connection)>,
    stop_accepting: CancellationToken,
    topic_streams_tx: mpsc::Sender<TopicRecvStream>,
}

impl ConnectionsActor {
    pub fn new(
        endpoint: Endpoint,
        topic_streams_tx: mpsc::Sender<TopicRecvStream>,
        stop_accepting: CancellationToken,
    ) -> Self {
        Self {
            endpoint,
            waiting: Default::default(),
            active: Default::default(),
            dial_tasks: Default::default(),
            conn_tasks: Default::default(),
            stop_accepting,
            topic_streams_tx,
        }
    }
    pub async fn poll(&mut self) {
        tokio::select! {
            Some(res) = self.dial_tasks.join_next(), if !self.dial_tasks.is_empty() => {
                let (node_id, conn) = res.expect("dial task panicked");
                match conn {
                    Ok(conn) => {
                        self.handle_connection(conn);
                    },
                    Err(err) => {
                        let mut senders = self.waiting.remove(&node_id).unwrap_or_default();
                        senders.drain(..).for_each(|reply| {
                            // TODO: error propagation
                            let err = anyhow::anyhow!("{err}");
                            reply.send(Err(err)).ok();
                        });
                    }
                }
            },
            Some(res) = self.conn_tasks.join_next(), if !self.conn_tasks.is_empty() => {
                let (node_id, conn) = res.expect("dial task panicked");
                self.closed(node_id, conn);
            }
        }
    }

    pub fn get_or_connect(
        &mut self,
        node_id: NodeId,
        reply: oneshot::Sender<Result<StrongConnection>>,
    ) {
        if let Some(conn) = self.active.get(&node_id) {
            let tracked_conn = conn.clone_strong();
            reply.send(Ok(tracked_conn)).ok();
        } else {
            match self.waiting.entry(node_id) {
                hash_map::Entry::Occupied(mut entry) => {
                    entry.get_mut().push(reply);
                }
                hash_map::Entry::Vacant(entry) => {
                    let endpoint = self.endpoint.clone();
                    self.dial_tasks
                        .spawn(async move { (node_id, endpoint.connect(node_id, ALPN).await) });
                    entry.insert(vec![reply]);
                }
            }
        }
    }

    pub fn handle_connection(&mut self, conn: Connection) {
        let Ok(node_id) = conn.remote_node_id() else {
            return;
        };
        let mut senders = self.waiting.remove(&node_id).unwrap_or_default();
        let conn = WeakConnection::new(conn);
        let old = self.active.insert(node_id, conn.clone());
        if let Some(old) = old {
            old.notify_replace();
        }
        senders.drain(..).for_each(|reply| {
            reply.send(Ok(conn.clone_strong())).ok();
        });
        let fut = Self::connection_loop(
            // self.topics.clone(),
            node_id,
            conn,
            self.stop_accepting.child_token(),
            self.topic_streams_tx.clone(),
        );
        let fut = fut.instrument(error_span!("conn", me=%self.endpoint.node_id().fmt_short(), remote=%node_id.fmt_short()));
        self.conn_tasks.spawn(fut);
    }

    fn closed(&mut self, node_id: NodeId, conn: Connection) {
        if let hash_map::Entry::Occupied(entry) = self.active.entry(node_id) {
            if entry.get().stable_id() == conn.stable_id() {
                let _ = entry.remove();
            }
        }
    }

    async fn connection_loop(
        // topics: TopicsMap,
        node_id: NodeId,
        conn: WeakConnection,
        cancel: CancellationToken,
        topic_stream_tx: mpsc::Sender<TopicRecvStream>,
    ) -> (NodeId, Connection) {
        {
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        debug!("break: cancelled");
                        break;
                    }
                    _ = conn.should_close() => {
                        if let Some(reason) = conn.close_reason() {
                            debug!("break: connection closed: {reason:?}");
                            break;
                        } else if !conn.is_active() {
                            info!("Closing connection: unused!");
                            conn.close(42u32.into(), b"close");
                            break;
                        }
                    }
                    res = conn.accept_uni() => {
                        let mut recv_stream = match res {
                            Ok(stream) => stream,
                            Err(_err) => {
                                break;
                            }
                        };
                        let conn = conn.clone_strong();
                        let Ok(header) = StreamHeader::read(&mut recv_stream).await else {
                            // TODO: Error?
                            break;
                        };
                        let topic_id = header.topic_id;
                        let topic_stream = TopicRecvStream { topic_id, recv_stream, conn, node_id };
                        if let Err(_err) = topic_stream_tx.send(topic_stream).await {
                            break;
                        }
                    }
                }
            }
        }
        (node_id, conn.into_inner())
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum WireStreamHeader {
    V0(StreamHeader),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamHeader {
    pub topic_id: TopicId,
}

impl StreamHeader {
    pub async fn read(stream: &mut RecvStream) -> Result<Self> {
        let len = stream.read_u32().await?;
        let mut buf = vec![0u8; len as usize];
        stream.read_exact(&mut buf).await?;
        let header: WireStreamHeader = postcard::from_bytes(&buf)?;
        let WireStreamHeader::V0(header) = header;
        Ok(header)
    }

    pub async fn write(self, stream: &mut SendStream) -> Result<()> {
        let frame = WireStreamHeader::V0(self);
        let buf = postcard::to_stdvec(&frame)?;
        stream.write_all(&buf).await?;
        Ok(())
    }
}

#[derive(Debug, Clone, derive_more::Deref)]
struct WeakConnection {
    #[deref]
    conn: Connection,
    usage: Arc<ConnectionUsage>,
    notify_replace: Arc<Notify>,
}

impl WeakConnection {
    fn new(conn: Connection) -> Self {
        Self {
            conn,
            usage: Default::default(),
            notify_replace: Default::default(),
        }
    }

    async fn should_close(&self) -> Result<(), ConnectionError> {
        tokio::select! {
            _ = self.usage.notify.notified() => {},
            _ = self.conn.closed() => {}
        }
        if let Some(error) = self.conn.close_reason() {
            Err(error)
        } else {
            Ok(())
        }
    }

    fn is_active(&self) -> bool {
        self.usage.is_active()
    }

    fn clone_strong(&self) -> StrongConnection {
        StrongConnection {
            conn: self.conn.clone(),
            _guard: self.usage.new(),
            notify_replace: self.notify_replace.clone(),
        }
    }

    fn notify_replace(&self) {
        self.notify_replace.notify_waiters();
    }

    fn into_inner(self) -> Connection {
        self.conn
    }
}

#[derive(Debug, Clone)]
pub struct StrongConnection {
    conn: Connection,
    _guard: UsageGuard,
    notify_replace: Arc<Notify>,
}

impl Deref for StrongConnection {
    type Target = Connection;
    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}

impl StrongConnection {
    pub async fn should_replace(&self) {
        self.notify_replace.notified().await
    }
}

#[derive(Debug)]
struct UsageGuard(Arc<ConnectionUsage>);

impl Drop for UsageGuard {
    fn drop(&mut self) {
        debug!(
            "drop guard - uses {} init {} bt {}",
            self.0.uses.load(Ordering::SeqCst),
            self.0.init.load(Ordering::SeqCst),
            std::backtrace::Backtrace::capture()
        );
        if self.0.uses.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.0.notify.notify_waiters();
        }
    }
}

impl Clone for UsageGuard {
    fn clone(&self) -> Self {
        self.0.init.store(true, Ordering::SeqCst);
        self.0.uses.fetch_add(1, Ordering::SeqCst);
        Self(self.0.clone())
    }
}

#[derive(Debug, Default)]
struct ConnectionUsage {
    init: AtomicBool,
    uses: AtomicU32,
    notify: Notify,
}
impl ConnectionUsage {
    fn new(self: &Arc<Self>) -> UsageGuard {
        self.init.store(true, Ordering::SeqCst);
        self.uses.fetch_add(1, Ordering::SeqCst);
        debug!(
            "conn usage new - uses {} init {}",
            self.uses.load(Ordering::SeqCst),
            self.init.load(Ordering::SeqCst)
        );
        UsageGuard(self.clone())
    }

    fn is_init(&self) -> bool {
        self.init.load(Ordering::SeqCst)
    }

    fn is_used(&self) -> bool {
        self.uses.load(Ordering::SeqCst) > 0
    }

    fn is_active(&self) -> bool {
        debug!(
            "conn checck - uses {} init {}",
            self.uses.load(Ordering::SeqCst),
            self.init.load(Ordering::SeqCst)
        );
        self.is_used() || !self.is_init()
    }
}
