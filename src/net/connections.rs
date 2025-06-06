use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex, Weak,
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
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error_span, info, instrument, Instrument};

use crate::{proto::TopicId, ALPN};

#[derive(derive_more::Debug, derive_more::Deref)]
pub struct RemoteRecvStream {
    #[debug("{}", node_id.fmt_short())]
    pub node_id: NodeId,
    #[debug(skip)]
    #[deref]
    pub stream: RecvStream,
    #[debug(skip)]
    pub conn: StrongConnection,
}

#[derive(derive_more::Debug, derive_more::Deref)]
pub struct RemoteSendStream {
    #[debug(skip)]
    #[deref]
    pub stream: SendStream,
    #[debug(skip)]
    pub conn: StrongConnection,
}

#[derive(Debug, Copy, Clone)]
enum Side {
    Connect,
    Accept,
}

#[derive(Debug, Clone)]
pub struct ConnectionPool(Arc<Mutex<ConnectionsInner>>);

struct WeakConnections(Weak<Mutex<ConnectionsInner>>);

impl WeakConnections {
    fn upgrade(&self) -> Option<ConnectionPool> {
        self.0.upgrade().map(ConnectionPool)
    }
}

#[derive(Debug)]
struct ConnectionsInner {
    endpoint: Endpoint,
    active: HashMap<NodeId, WeakConnection>,
    conn_tasks: HashMap<usize, AbortOnDropHandle<()>>,
    dialing: HashMap<NodeId, Dialing>,
    accept: HashMap<TopicId, mpsc::Sender<RemoteRecvStream>>,
    cancel: CancellationToken,
}

#[derive(Debug)]
struct Dialing {
    _task: AbortOnDropHandle<()>,
    replies: Vec<oneshot::Sender<Result<StrongConnection>>>,
}

impl ConnectionPool {
    pub fn new(endpoint: Endpoint) -> Self {
        Self(Arc::new(Mutex::new(ConnectionsInner {
            endpoint,
            accept: Default::default(),
            active: Default::default(),
            conn_tasks: Default::default(),
            cancel: Default::default(),
            dialing: Default::default(),
        })))
    }

    pub fn accept_topic(
        &self,
        topic_id: TopicId,
        channel_cap: usize,
    ) -> mpsc::Receiver<RemoteRecvStream> {
        let (tx, rx) = mpsc::channel(channel_cap);
        self.0.lock().expect("poisoned").accept.insert(topic_id, tx);
        rx
    }

    #[instrument("open_topic", skip_all, fields(remote=%node_id.fmt_short(), topic=%topic_id.fmt_short()))]
    pub async fn open_topic(&self, node_id: NodeId, topic_id: TopicId) -> Result<RemoteSendStream> {
        debug!("topic stream requested");
        let conn = self.get_or_connect(node_id).await?;
        debug!("got connection");
        match open_stream(conn, topic_id).await {
            Ok(stream) => {
                debug!("stream open!");
                Ok(stream)
            }
            Err(err) => {
                debug!("opening stream failed: {err:#}");
                Err(err)
            }
        }
    }

    pub fn handle_connection(&self, conn: Connection) {
        let Ok(node_id) = conn.remote_node_id() else {
            return;
        };
        self.handle_connection_inner(node_id, Ok(WeakConnection::new(conn)), Side::Accept);
    }

    fn handle_connection_inner(&self, node_id: NodeId, conn: Result<WeakConnection>, side: Side) {
        let mut inner = self.0.lock().expect("poisoned");
        debug!(?side, err=?conn.as_ref().err(), "handle conn inner");

        if let Side::Connect = side {
            let Some(mut dialing) = inner.dialing.remove(&node_id) else {
                tracing::error!(remote=%node_id.fmt_short(), ?conn, "dial complete but dialing state not found");
                return;
            };
            for reply in dialing.replies.drain(..) {
                match &conn {
                    Ok(conn) => reply.send(Ok(conn.clone_strong())).ok(),
                    // TODO: Error cloning
                    Err(err) => reply.send(Err(anyhow::anyhow!("{err}"))).ok(),
                };
            }
        }

        if let Ok(conn) = conn {
            let cancel = inner.cancel.child_token();
            let conn_task = tokio::task::spawn(
                Self::connection_loop(node_id, conn.clone(), cancel, self.downgrade()).instrument(
                    error_span!("conn", remote=%node_id.fmt_short(), side=?Side::Accept),
                ),
            );
            inner
                .conn_tasks
                .insert(conn.stable_id(), AbortOnDropHandle::new(conn_task));
            let old = inner.active.insert(node_id, conn.clone());

            if let Some(old) = old {
                old.notify_replace();
            }
        }
    }

    async fn get_or_connect(&self, node_id: NodeId) -> Result<StrongConnection> {
        let reply_rx = {
            let mut inner = self.0.lock().expect("poisoned");

            if let Some(conn) = inner.active.get(&node_id) {
                debug!("active connection available!");
                let conn = conn.clone_strong();
                return Ok(conn);
            }

            let (reply, reply_rx) = oneshot::channel();
            if let Some(dialing) = inner.dialing.get_mut(&node_id) {
                debug!("no active connection available, dialing in process already");
                dialing.replies.push(reply);
            } else {
                debug!("no active connection available, init dial");
                let dial_task = tokio::task::spawn(
                    Self::connect(inner.endpoint.clone(), node_id, self.downgrade())
                        .instrument(error_span!("connect", remote=%node_id.fmt_short())),
                );
                let dialing = Dialing {
                    _task: AbortOnDropHandle::new(dial_task),
                    replies: vec![reply],
                };
                inner.dialing.insert(node_id, dialing);
            }
            reply_rx
        };
        reply_rx.await?
    }

    async fn forward_stream(&self, topic_id: TopicId, stream: RemoteRecvStream) {
        let sender = self
            .0
            .lock()
            .expect("poisoned")
            .accept
            .get(&topic_id)
            .cloned();
        if let Some(sender) = sender {
            debug!(topic=%topic_id.fmt_short(), "stream ok, topic registered, forward");
            if let Err(_err) = sender.send(stream).await {
                debug!(topic=%topic_id.fmt_short(), "topic dead, drop sender");
                let _ = self.0.lock().expect("poisoned").accept.remove(&topic_id);
            }
        } else {
            debug!(topic=%topic_id.fmt_short(), "no listener for topic, drop stream");
        }
    }

    async fn connection_loop(
        node_id: NodeId,
        conn: WeakConnection,
        cancel: CancellationToken,
        state: WeakConnections,
    ) {
        debug!("starting connection loop");
        loop {
            let mut stream = tokio::select! {
                _ = cancel.cancelled() => {
                    debug!("break: cancelled");
                    break;
                }
                _close_reason = conn.should_close() => break,
                res = conn.accept_uni() => {
                    match res {
                        Ok(stream) => stream,
                        Err(_err) => break,
                    }
                }
            };

            debug!("stream incoming");
            let Ok(header) = StreamHeader::read(&mut stream).await else {
                // TODO: Error?
                debug!("failed to read stream header, abort");
                continue;
            };
            let topic_id = header.topic_id;
            let topic_stream = RemoteRecvStream {
                stream,
                conn: conn.clone_strong(),
                node_id,
            };

            if let Some(state) = state.upgrade() {
                state.forward_stream(topic_id, topic_stream).await;
            } else {
                debug!("break: connection_pool dropped");
                break;
            }
        }

        // cleanup
        if let Some(state) = state.upgrade() {
            let mut inner = state.0.lock().expect("poisoned");
            let _ = inner.conn_tasks.remove(&conn.stable_id());
            if inner.active.get(&node_id).map(|conn| conn.stable_id()) == Some(conn.stable_id()) {
                inner.active.remove(&node_id);
            }
        }

        if let Some(reason) = conn.close_reason() {
            info!("break: connection closed: {reason:?}");
        } else {
            info!("break: closing connection: unused");
            // TODO(Frando): What to put here
            conn.close(42u32.into(), b"close unused");
        }
    }

    async fn connect(endpoint: Endpoint, node_id: NodeId, state: WeakConnections) {
        let res = endpoint
            .connect(node_id, ALPN)
            .await
            .map(WeakConnection::new);
        debug!(
            "dialed, sucess {},  error {:?}",
            res.is_ok(),
            res.as_ref().err()
        );

        if let Some(state) = state.upgrade() {
            state.handle_connection_inner(node_id, res, Side::Connect);
        }
    }

    fn downgrade(&self) -> WeakConnections {
        WeakConnections(Arc::downgrade(&self.0))
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

#[derive(Debug)]
struct TrackedConnection {
    conn: Connection,
    replaced: AtomicBool,
    notify_replace: Notify,
    notify_unused: Notify,
    strong_count: AtomicUsize,
}

#[derive(Debug)]
pub(crate) struct StrongConnection(Arc<TrackedConnection>);

impl StrongConnection {
    pub(crate) async fn should_replace(&self) {
        self.0.notify_replace.notified().await
    }
    pub(crate) fn has_replacement(&self) -> bool {
        self.0.replaced.load(Ordering::SeqCst)
    }
}

impl std::ops::Deref for StrongConnection {
    type Target = Connection;
    fn deref(&self) -> &Self::Target {
        &self.0.conn
    }
}

impl Clone for StrongConnection {
    fn clone(&self) -> Self {
        self.0.strong_count.fetch_add(1, Ordering::SeqCst);
        Self(Arc::clone(&self.0))
    }
}

impl Drop for StrongConnection {
    fn drop(&mut self) {
        if self.0.strong_count.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.0.notify_unused.notify_waiters();
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WeakConnection(Arc<TrackedConnection>);

impl WeakConnection {
    fn new(conn: Connection) -> Self {
        Self(Arc::new(TrackedConnection {
            conn,
            replaced: Default::default(),
            notify_unused: Default::default(),
            notify_replace: Default::default(),
            strong_count: Default::default(),
        }))
    }
    fn clone_strong(&self) -> StrongConnection {
        self.0.strong_count.fetch_add(1, Ordering::SeqCst);
        StrongConnection(Arc::clone(&self.0))
    }

    fn notify_replace(&self) {
        self.0.replaced.store(true, Ordering::SeqCst);
        self.0.notify_replace.notify_waiters();
    }

    async fn should_close(&self) -> Option<ConnectionError> {
        tokio::select! {
            _ = self.0.notify_unused.notified() => None,
            reason = self.0.conn.closed() => Some(reason)
        }
    }
}

impl std::ops::Deref for WeakConnection {
    type Target = Connection;
    fn deref(&self) -> &Self::Target {
        &self.0.conn
    }
}

async fn open_stream(conn: StrongConnection, topic_id: TopicId) -> Result<RemoteSendStream> {
    let mut stream = conn.open_uni().await?;
    debug!("stream opened");
    let header = StreamHeader { topic_id };
    header.write(&mut stream).await?;
    Ok(RemoteSendStream { stream, conn })
}
