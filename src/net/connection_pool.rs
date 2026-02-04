//! A simple iroh connection pool
//!
//! Entry point is [`ConnectionPool`]. You create a connection pool for a specific
//! ALPN and [`Options`]. Then the pool will manage connections for you.
//!
//! Access to connections is via the [`ConnectionPool::get_or_connect`] method, which
//! gives you access to a connection via a [`ConnectionRef`] if possible.
//!
//! It is important that you keep the [`ConnectionRef`] alive while you are using
//! the connection.
use std::{
    collections::{HashMap, VecDeque},
    io,
    ops::Deref,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use iroh::{
    endpoint::{ConnectError, Connection},
    Endpoint, EndpointId,
};
use n0_error::{e, stack_error};
use n0_future::{
    future::{self},
    time::Duration,
    FuturesUnordered, MaybeFuture, Stream, StreamExt,
};
use tokio::sync::{
    mpsc::{self, error::SendError as TokioSendError},
    oneshot, Notify,
};
use tracing::{debug, error, error_span, info, trace, Instrument};

pub type OnConnected =
    Arc<dyn Fn(&Endpoint, ConnectionRef) -> n0_future::future::Boxed<io::Result<()>> + Send + Sync>;

/// Configuration options for the connection pool
#[derive(derive_more::Debug, Clone)]
pub struct Options {
    /// How long to keep idle connections around.
    pub idle_timeout: Duration,
    /// Timeout for connect. This includes the time spent in on_connect, if set.
    pub connect_timeout: Duration,
    /// Maximum number of connections to hand out.
    pub max_connections: usize,
    /// An optional callback that can be used to wait for the connection to enter some state.
    /// An example usage could be to wait for the connection to become direct before handing
    /// it out to the user.
    #[debug(skip)]
    pub on_connected: Option<OnConnected>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            idle_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(1),
            max_connections: 1024,
            on_connected: None,
        }
    }
}

impl Options {
    /// Set the on_connected callback
    pub fn with_on_connected<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(Endpoint, ConnectionRef) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = io::Result<()>> + Send + 'static,
    {
        self.on_connected = Some(Arc::new(move |ep, conn| {
            let ep = ep.clone();
            Box::pin(f(ep, conn))
        }));
        self
    }
}

/// A reference to a connection that is owned by a connection pool.
#[derive(Debug, Clone)]
pub struct ConnectionRef {
    connection: iroh::endpoint::Connection,
    _permit: OneConnection,
}

impl ConnectionRef {
    pub fn inner(&self) -> &Connection {
        &self.connection
    }
}

impl Deref for ConnectionRef {
    type Target = iroh::endpoint::Connection;
    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}

impl ConnectionRef {
    fn new(connection: iroh::endpoint::Connection, counter: OneConnection) -> Self {
        Self {
            connection,
            _permit: counter,
        }
    }
}

/// Error when a connection can not be acquired
///
/// This includes the normal iroh connection errors as well as pool specific
/// errors such as timeouts and connection limits.
#[stack_error(derive, add_meta)]
#[derive(Clone)]
pub enum PoolConnectError {
    /// Connection pool is shut down
    #[error("Connection pool is shut down")]
    Shutdown {},
    /// Timeout during connect
    #[error("Timeout during connect")]
    Timeout {},
    /// Too many connections
    #[error("Too many connections")]
    TooManyConnections {},
    /// Error during connect
    #[error(transparent)]
    ConnectError { source: Arc<ConnectError> },
    /// Error during on_connect callback
    #[error(transparent)]
    OnConnectError {
        #[error(std_err)]
        source: Arc<io::Error>,
    },
}

#[stack_error(derive, add_meta)]
#[derive(Clone)]
pub enum PoolHandleConnectionError {
    /// Connection pool is shut down
    #[error("Connection pool is shut down")]
    Shutdown {},
    /// Too many connections
    #[error("Too many connections")]
    TooManyConnections {},
}

impl From<ConnectError> for PoolConnectError {
    fn from(e: ConnectError) -> Self {
        e!(PoolConnectError::ConnectError, Arc::new(e))
    }
}

impl From<io::Error> for PoolConnectError {
    fn from(e: io::Error) -> Self {
        e!(PoolConnectError::OnConnectError, Arc::new(e))
    }
}

/// Error when calling a fn on the [`ConnectionPool`].
///
/// The only thing that can go wrong is that the connection pool is shut down.
#[stack_error(derive, add_meta)]
pub enum ConnectionPoolError {
    /// The connection pool has been shut down
    #[error("The connection pool has been shut down")]
    Shutdown {},
}

enum ActorMessage {
    RequestRef(RequestRef),
    ConnectionIdle { id: EndpointId },
    ConnectionShutdown { id: EndpointId },
}

#[derive(derive_more::Debug)]
struct RequestRef {
    mode: Mode,
    #[debug(skip)]
    tx: oneshot::Sender<Result<ConnectionRef, PoolConnectError>>,
}

#[derive(Clone, derive_more::Debug)]
enum Mode {
    #[debug("Connect({})", _0.fmt_short())]
    Connect(EndpointId),
    #[debug("Handle({} {})", _0.remote_id().fmt_short(), _0.stable_id())]
    Handle(Connection),
}

impl Mode {
    fn remote_id(&self) -> EndpointId {
        match self {
            Mode::Connect(id) => *id,
            Mode::Handle(connection) => connection.remote_id(),
        }
    }
}

struct Context {
    options: Options,
    endpoint: Endpoint,
    owner: ConnectionPool,
    alpn: Vec<u8>,
}

impl Context {
    async fn run_connection_actor(self: Arc<Self>, mode: Mode, mut rx: mpsc::Receiver<RequestRef>) {
        trace!("Connection actor starting");
        let context = self;

        let counter = ConnectionCounter::new();
        let node_id = mode.remote_id();

        let conn_fut = {
            let context = context.clone();
            let counter = counter.clone();
            async move {
                let conn = match mode {
                    Mode::Handle(conn) => {
                        info!(id=%node_id.fmt_short(), "starting new conn actor: handle connection");
                        conn
                    }
                    Mode::Connect(node_id) => {
                        info!(id=%node_id.fmt_short(), "starting new conn actor: connect");
                        let conn = context
                            .endpoint
                            .connect(node_id, &context.alpn)
                            .await
                            .map_err(PoolConnectError::from)?;
                        conn
                    }
                };
                if let Some(on_connect) = &context.options.on_connected {
                    let conn = ConnectionRef::new(conn.clone(), counter.get_one());
                    on_connect(&context.endpoint, conn)
                        .await
                        .map_err(PoolConnectError::from)?;
                }
                Result::<Connection, PoolConnectError>::Ok(conn)
            }
        };

        // Connect to the node
        let mut state = n0_future::time::timeout(context.options.connect_timeout, conn_fut)
            .await
            .map_err(|_| e!(PoolConnectError::Timeout))
            .and_then(|r| r);

        let conn_close = match &state {
            Ok(conn) => MaybeFuture::Some(closed(conn.clone())),
            Err(e) => {
                debug!(%node_id, "Failed to connect {e:?}, requesting shutdown");
                if context.owner.close(node_id).await.is_err() {
                    return;
                }
                MaybeFuture::None
            }
        };

        let idle_timer = MaybeFuture::default();
        let idle_stream = counter.clone().idle_stream();

        tokio::pin!(idle_timer, idle_stream, conn_close);

        loop {
            tokio::select! {
                biased;

                // Handle new work
                handler = rx.recv() => {
                    debug!(current=counter.current(), "msg {handler:?}");
                    match handler {
                        Some(RequestRef { mode, tx }) => {
                            assert!(mode.remote_id() == node_id, "Not for me!");
                            if let Mode::Handle(conn) = mode {
                                info!("handle new conn: replace old");
                                conn_close.as_mut().set_future({
                                    closed(conn.clone())
                                });
                                let old_conn = std::mem::replace(&mut state, Ok(conn));
                                // TODO: What do we do with the old conn here?
                                // We don't want to close it because it might be actively used.
                                // We just drop our reference for now.
                                drop(old_conn);
                            }
                            match &state {
                                Ok(state) => {
                                    let res = ConnectionRef::new(state.clone(), counter.get_one());
                                    info!(current_count=counter.current(), "Handing out ConnectionRef");

                                    // clear the idle timer
                                    idle_timer.as_mut().set_none();
                                    tx.send(Ok(res)).ok();
                                }
                                Err(cause) => {
                                    tx.send(Err(cause.clone())).ok();
                                }
                            }
                        }
                        None => {
                            // Channel closed - exit
                            break;
                        }
                    }
                }

                _ = &mut conn_close => {
                    // connection was closed by somebody, notify owner that we should be removed
                    context.owner.close(node_id).await.ok();
                }

                _ = idle_stream.next() => {
                    if !counter.is_idle() {
                        continue;
                    };
                    // notify the pool that we are idle.
                    trace!("Idle");
                    if context.owner.idle(node_id).await.is_err() {
                        // If we can't notify the pool, we are shutting down
                        break;
                    }
                    // set the idle timer
                    idle_timer.as_mut().set_future(n0_future::time::sleep(context.options.idle_timeout));
                }

                // Idle timeout - request shutdown
                _ = &mut idle_timer => {
                    trace!("Idle timer expired, requesting shutdown");
                    context.owner.close(node_id).await.ok();
                    // Don't break here - wait for main actor to close our channel
                }
            }
        }

        if let Ok(connection) = state {
            let reason = if counter.is_idle() { b"idle" } else { b"drop" };
            connection.close(0u32.into(), reason);
        }

        trace!("Connection actor shutting down");
    }
}

async fn closed(conn: Connection) -> iroh::endpoint::ConnectionError {
    conn.closed().await
}

struct Actor {
    rx: mpsc::Receiver<ActorMessage>,
    connections: HashMap<EndpointId, mpsc::Sender<RequestRef>>,
    context: Arc<Context>,
    // idle set (most recent last)
    // todo: use a better data structure if this becomes a performance issue
    idle: VecDeque<EndpointId>,
    // per connection tasks
    tasks: FuturesUnordered<future::Boxed<()>>,
}

impl Actor {
    pub fn new(
        endpoint: Endpoint,
        alpn: &[u8],
        options: Options,
    ) -> (Self, mpsc::Sender<ActorMessage>) {
        let (tx, rx) = mpsc::channel(100);
        (
            Self {
                rx,
                connections: HashMap::new(),
                idle: VecDeque::new(),
                context: Arc::new(Context {
                    options,
                    alpn: alpn.to_vec(),
                    endpoint,
                    owner: ConnectionPool { tx: tx.clone() },
                }),
                tasks: FuturesUnordered::new(),
            },
            tx,
        )
    }

    fn add_idle(&mut self, id: EndpointId) {
        self.remove_idle(id);
        self.idle.push_back(id);
    }

    fn remove_idle(&mut self, id: EndpointId) {
        self.idle.retain(|&x| x != id);
    }

    fn pop_oldest_idle(&mut self) -> Option<EndpointId> {
        self.idle.pop_front()
    }

    fn remove_connection(&mut self, id: EndpointId) {
        self.connections.remove(&id);
        self.remove_idle(id);
    }

    async fn handle_msg(&mut self, msg: ActorMessage) {
        match msg {
            ActorMessage::RequestRef(mut msg) => {
                let id = msg.mode.remote_id();
                self.remove_idle(id);
                // Try to send to existing connection actor
                if let Some(conn_tx) = self.connections.get(&id) {
                    if let Err(TokioSendError(e)) = conn_tx.send(msg).await {
                        msg = e;
                    } else {
                        return;
                    }
                    // Connection actor died, remove it
                    self.remove_connection(id);
                }

                // No connection actor or it died - check limits
                if self.connections.len() >= self.context.options.max_connections {
                    if let Some(idle) = self.pop_oldest_idle() {
                        // remove the oldest idle connection to make room for one more
                        trace!("removing oldest idle connection {}", idle);
                        self.connections.remove(&idle);
                    } else {
                        msg.tx
                            .send(Err(e!(PoolConnectError::TooManyConnections)))
                            .ok();
                        return;
                    }
                }
                let (conn_tx, conn_rx) = mpsc::channel(100);
                self.connections.insert(id, conn_tx.clone());

                let context = self.context.clone();

                let mut msg = msg;
                let mode = match &mut msg.mode {
                    Mode::Connect(id) => Mode::Connect(*id),
                    Mode::Handle(conn) => {
                        let id = conn.remote_id();
                        std::mem::replace(&mut msg.mode, Mode::Connect(id))
                    }
                };
                self.tasks.push(Box::pin(
                    context
                        .run_connection_actor(mode, conn_rx)
                        .instrument(error_span!("conn_actor", remote=%id.fmt_short())),
                ));

                // Send the handler to the new actor
                if conn_tx.send(msg).await.is_err() {
                    error!(%id, "Failed to send handler to new connection actor");
                    self.connections.remove(&id);
                }
            }
            ActorMessage::ConnectionIdle { id } => {
                self.add_idle(id);
                trace!(%id, "connection idle");
            }
            ActorMessage::ConnectionShutdown { id } => {
                // Remove the connection from our map - this closes the channel
                self.remove_connection(id);
                trace!(%id, "removed connection");
            }
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                biased;

                msg = self.rx.recv() => {
                    if let Some(msg) = msg {
                        self.handle_msg(msg).await;
                    } else {
                        break;
                    }
                }

                _ = self.tasks.next(), if !self.tasks.is_empty() => {}
            }
        }
    }
}

/// A connection pool
#[derive(Debug, Clone)]
pub struct ConnectionPool {
    tx: mpsc::Sender<ActorMessage>,
}

impl ConnectionPool {
    pub fn new(endpoint: Endpoint, alpn: &[u8], options: Options) -> Self {
        let (actor, tx) = Actor::new(endpoint, alpn, options);

        // Spawn the main actor
        n0_future::task::spawn(actor.run().instrument(error_span!("pool")));

        Self { tx }
    }

    /// Returns either a fresh connection or a reference to an existing one.
    ///
    /// This is guaranteed to return after approximately [Options::connect_timeout]
    /// with either an error or a connection.
    pub async fn get_or_connect(
        &self,
        id: EndpointId,
    ) -> std::result::Result<ConnectionRef, PoolConnectError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(ActorMessage::RequestRef(RequestRef {
                mode: Mode::Connect(id),
                tx,
            }))
            .await
            .map_err(|_| e!(PoolConnectError::Shutdown))?;
        rx.await.map_err(|_| e!(PoolConnectError::Shutdown))?
    }

    pub async fn handle_connection(
        &self,
        conn: Connection,
    ) -> std::result::Result<ConnectionRef, PoolConnectError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(ActorMessage::RequestRef(RequestRef {
                mode: Mode::Handle(conn),
                tx,
            }))
            .await
            .map_err(|_| e!(PoolConnectError::Shutdown))?;
        rx.await.map_err(|_| e!(PoolConnectError::Shutdown))?
    }

    /// Close an existing connection, if it exists
    ///
    /// This will finish pending tasks and close the connection. New tasks will
    /// get a new connection if they are submitted after this call
    pub async fn close(&self, id: EndpointId) -> std::result::Result<(), ConnectionPoolError> {
        self.tx
            .send(ActorMessage::ConnectionShutdown { id })
            .await
            .map_err(|_| e!(ConnectionPoolError::Shutdown))?;
        Ok(())
    }

    /// Notify the connection pool that a connection is idle.
    ///
    /// Should only be called from connection handlers.
    pub(crate) async fn idle(
        &self,
        id: EndpointId,
    ) -> std::result::Result<(), ConnectionPoolError> {
        self.tx
            .send(ActorMessage::ConnectionIdle { id })
            .await
            .map_err(|_| e!(ConnectionPoolError::Shutdown))?;
        Ok(())
    }
}

#[derive(Debug)]
struct ConnectionCounterInner {
    count: AtomicUsize,
    notify: Notify,
}

#[derive(Debug, Clone)]
struct ConnectionCounter {
    inner: Arc<ConnectionCounterInner>,
}

impl ConnectionCounter {
    fn new() -> Self {
        Self {
            inner: Arc::new(ConnectionCounterInner {
                count: Default::default(),
                notify: Notify::new(),
            }),
        }
    }

    fn current(&self) -> usize {
        self.inner.count.load(Ordering::SeqCst)
    }

    /// Increase the connection count and return a guard for the new connection
    fn get_one(&self) -> OneConnection {
        self.inner.count.fetch_add(1, Ordering::SeqCst);
        OneConnection {
            inner: self.inner.clone(),
        }
    }

    fn is_idle(&self) -> bool {
        self.inner.count.load(Ordering::SeqCst) == 0
    }

    /// Infinite stream that yields when the connection is briefly idle.
    ///
    /// Note that you still have to check if the connection is still idle when
    /// you get the notification.
    ///
    /// Also note that this stream is triggered on [OneConnection::drop], so it
    /// won't trigger initially even though a [ConnectionCounter] starts up as
    /// idle.
    fn idle_stream(self) -> impl Stream<Item = ()> {
        n0_future::stream::unfold(self, |c| async move {
            c.inner.notify.notified().await;
            Some(((), c))
        })
    }
}

/// Guard for one connection
#[derive(Debug)]
struct OneConnection {
    inner: Arc<ConnectionCounterInner>,
}

impl Clone for OneConnection {
    fn clone(&self) -> Self {
        self.inner.count.fetch_add(1, Ordering::SeqCst);
        OneConnection {
            inner: self.inner.clone(),
        }
    }
}

impl Drop for OneConnection {
    fn drop(&mut self) {
        if self.inner.count.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.inner.notify.notify_waiters();
        }
    }
}
