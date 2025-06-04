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
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, oneshot, Notify},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error_span, info, Instrument};

use crate::{proto::TopicId, ALPN};

// #[derive(Debug, derive_more::Deref)]
// pub(crate) struct GuardedSendStream {
//     conn: StrongConnection,
//     #[deref]
//     stream: SendStream,
// }

// #[derive(Debug, derive_more::Deref)]
// pub(crate) struct GuardedRecvStream {
//     conn: StrongConnection,
//     #[deref]
//     stream: RecvStream,
// }

// impl std::ops::DerefMut for GuardedRecvStream {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         &mut self.stream
//     }
// }
// impl std::ops::DerefMut for GuardedSendStream {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         &mut self.stream
//     }
// }

// #[derive(Debug)]
// pub(crate) enum OpenError {
//     Dropped,
//     Connection(ConnectionError),
//     OpenStream,
// }

// #[derive(Debug)]
// pub(crate) enum AcceptError {
//     Dropped,
//     Connection(ConnectionError),
//     OpenStream,
// }

// #[derive(Debug)]
// pub(crate) struct Dropped;

// #[derive(Debug)]
// enum ToStreamPool {
//     Open {
//         node_id: NodeId,
//         topic_id: TopicId,
//         reply: oneshot::Sender<Result<GuardedSendStream, OpenError>>,
//     },
//     Accept {
//         topic_id: TopicId,
//         reply: mpsc::Sender<Result<GuardedRecvStream, AcceptError>>,
//     },
//     HandleConnection {
//         conn: Connection,
//     },
// }

// #[derive(Debug, Clone)]
// pub(crate) struct StreamPool {
//     tx: mpsc::Sender<ToStreamPool>,
//     _task_handle: Arc<AbortOnDropHandle<()>>,
// }

// impl StreamPool {
//     pub(crate) async fn open(
//         &self,
//         node_id: NodeId,
//         topic_id: TopicId,
//     ) -> Result<GuardedSendStream, OpenError> {
//         let (reply, reply_rx) = oneshot::channel();
//         self.tx
//             .send(ToStreamPool::Open {
//                 node_id,
//                 topic_id,
//                 reply,
//             })
//             .await
//             .map_err(|_| OpenError::Dropped)?;
//         reply_rx.await.map_err(|_| OpenError::Dropped)?
//     }

//     pub(crate) async fn accept(
//         &self,
//         topic_id: TopicId,
//     ) -> Result<mpsc::Receiver<Result<GuardedRecvStream, AcceptError>>, Dropped> {
//         let (reply, reply_rx) = mpsc::channel(16);
//         self.tx
//             .send(ToStreamPool::Accept { topic_id, reply })
//             .await
//             .map_err(|_| Dropped)?;
//         Ok(reply_rx)
//     }

//     pub(crate) async fn handle_connection(&self, conn: Connection) -> Result<(), Dropped> {
//         self.tx
//             .send(ToStreamPool::HandleConnection { conn })
//             .await
//             .map_err(|_| Dropped)
//     }

//     pub(crate) fn spawn(endpoint: Endpoint) -> Self {
//         let (tx, rx) = mpsc::channel(16);
//         let (topic_stream_tx, topic_stream_rx) = mpsc::channel(16);
//         let fut = async move {
//             let cancel = CancellationToken::new();
//             let mut actor = ConnectionsActor::new(endpoint, topic_streams_tx, cancel);
//             let waiting = HashMap::new();
//             loop {
//                 tokio::select! {
//                     _ = actor.poll() => {},
//                     msg = rx.recv() => {
//                         let Some(msg) = msg else {
//                             break;
//                         };
//                         match msg {
//                             ToStreamPool::Open { node_id, topic_id, reply } => {
//                                 let conn = actor.get_or_connect(node_id, reply);
//                                 // TODO: move into task
//                             }
//                             ToStreamPool::Accept { topic_id, reply } => todo!(),
//                             ToStreamPool::HandleConnection { conn } => todo!(),
//                         }
//                     }
//                 }
//             }
//         };
//         let task = tokio::task::spawn(fut);
//         let handle = AbortOnDropHandle::new(task);
//         Self {
//             _task_handle: handle,
//             tx,
//         }
//     }
// }

#[derive(derive_more::Debug)]
pub struct RemoteStream {
    #[debug("{}", topic_id.fmt_short())]
    pub topic_id: TopicId,
    #[debug("{}", node_id.fmt_short())]
    pub node_id: NodeId,
    #[debug(skip)]
    pub recv_stream: RecvStream,
    #[debug(skip)]
    pub conn: StrongConnection,
}

#[derive(Debug)]
pub struct ConnectionsActor {
    endpoint: Endpoint,
    waiting: HashMap<NodeId, Vec<oneshot::Sender<Result<StrongConnection>>>>,
    active: HashMap<NodeId, WeakConnection>,
    dial_tasks: JoinSet<(NodeId, Result<Connection>)>,
    conn_tasks: JoinSet<(NodeId, Connection)>,
    stop_accepting: CancellationToken,
    topic_streams_tx: mpsc::Sender<RemoteStream>,
}

impl ConnectionsActor {
    pub fn new(
        endpoint: Endpoint,
        topic_streams_tx: mpsc::Sender<RemoteStream>,
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
                        debug!(remote = %node_id.fmt_short(), "handle connection (connect)");
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
            _ = std::future::pending() => {}
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
        debug!(remote = %node_id.fmt_short(), "handle connection (accept)");
        let mut senders = self.waiting.remove(&node_id).unwrap_or_default();
        let conn = WeakConnection::new(conn);
        let old = self.active.insert(node_id, conn.clone());
        if let Some(old) = old {
            old.notify_replace();
        }
        senders.drain(..).for_each(|reply| {
            reply.send(Ok(conn.clone_strong())).ok();
        });
        let id = conn.stable_id();
        let fut = Self::connection_loop(
            // self.topics.clone(),
            node_id,
            conn,
            self.stop_accepting.child_token(),
            self.topic_streams_tx.clone(),
        );
        let fut = fut.instrument(error_span!("conn", remote=%node_id.fmt_short(), %id));
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
        topic_stream_tx: mpsc::Sender<RemoteStream>,
    ) -> (NodeId, Connection) {
        debug!("starting connection loop");
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    debug!("break: cancelled");
                    break;
                }
                _ = conn.should_close() => {
                    if let Some(reason) = conn.close_reason() {
                        info!("break: connection closed: {reason:?}");
                        break;
                    } else if !conn.is_active() {
                        info!("break: closing connection: unused");
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
                    debug!("stream incoming");
                    let Ok(header) = StreamHeader::read(&mut recv_stream).await else {
                        // TODO: Error?
                        debug!("failed to read stream header, abort");
                        continue;
                    };
                    let topic_id = header.topic_id;
                    let topic_stream = RemoteStream { topic_id, recv_stream, conn, node_id };
                    debug!(topic=%topic_id.fmt_short(), "stream ok, forward");
                    if let Err(_err) = topic_stream_tx.send(topic_stream).await {
                        break;
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
        stream.write_u32(buf.len() as u32).await?;
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
        if let Some(error) = self.conn.close_reason() {
            return Err(error);
        }
        loop {
            let notify = self.usage.notify_unused.notified();
            tokio::select! {
                _ = notify => {
                    if !self.is_active() {
                        break;
                    }
                },
                _ = self.conn.closed() => {}
            }
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
        // debug!(
        //     "drop guard - uses {} init {}",
        //     self.0.uses.load(Ordering::SeqCst),
        //     self.0.init.load(Ordering::SeqCst),
        // );
        if self.0.uses.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.0.notify_unused.notify_waiters();
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
    notify_unused: Notify,
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
            "conn.is_active - used {} init {}",
            self.is_used(),
            self.is_init()
        );
        self.is_used() || !self.is_init()
    }
}
