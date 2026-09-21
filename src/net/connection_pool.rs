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
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

use iroh::{
    endpoint::{ConnectError, Connection},
    Endpoint, EndpointId,
};
use n0_error::{e, stack_error};
use n0_future::{time::Duration, MaybeFuture, Stream, StreamExt};
use tokio::{
    sync::{
        mpsc::{self, error::SendError as TokioSendError},
        oneshot, Notify,
    },
    task::JoinSet,
};
use tracing::{debug, error, error_span, trace, Instrument};

/// Close reason for a superseded connection that nothing used for a while.
pub(crate) const CLOSE_SUPERSEDED: &[u8] = b"superseded";

pub type OnConnected = Arc<
    dyn Fn(&Endpoint, ConnectionHandle) -> n0_future::future::Boxed<io::Result<()>> + Send + Sync,
>;

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
        F: Fn(Endpoint, ConnectionHandle) -> Fut + Send + Sync + 'static,
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

    /// Whether a newer connection to the same peer has taken this one's place.
    ///
    /// A superseded connection stays open for as long as it is used, but new
    /// work should move to the current one: the old one may be a connection to a
    /// peer that has since restarted, dead without us having noticed yet.
    pub fn is_superseded(&self) -> bool {
        self._permit.inner.superseded.load(Ordering::SeqCst)
    }
}

/// A connection as handed to [`Options::on_connected`].
///
/// Unlike a [`ConnectionRef`], holding one does not keep the connection in use,
/// so a task that watches the connection for as long as it lives can hold it.
/// Work on the connection that should keep it open takes a [`ConnectionRef`]
/// from [`Self::get_ref`] instead -- in particular a stream the peer opened,
/// since the peer may keep using a connection we have superseded.
#[derive(Debug, Clone)]
pub struct ConnectionHandle {
    connection: Connection,
    counter: ConnectionCounter,
}

impl ConnectionHandle {
    fn new(connection: &Connection, counter: &ConnectionCounter) -> Self {
        Self {
            connection: connection.clone(),
            counter: counter.clone(),
        }
    }

    /// Returns the underlying connection.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Returns a reference that keeps the connection in use while it is alive.
    pub fn get_ref(&self) -> ConnectionRef {
        ConnectionRef::new(self.connection.clone(), self.counter.get_one())
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
        trace!(?mode, "Connection actor starting");
        let context = self;

        // One counter per connection, not per peer: a superseded connection has
        // to be able to go idle on its own while the current one is in use.
        let mut counter = ConnectionCounter::new();
        let node_id = mode.remote_id();

        let conn_fut = async {
            let conn = match mode {
                Mode::Handle(conn) => conn,
                Mode::Connect(node_id) => {
                    let conn = context
                        .endpoint
                        .connect(node_id, &context.alpn)
                        .instrument(tracing::info_span!("connect"))
                        .await
                        .map_err(PoolConnectError::from)?;
                    conn
                }
            };
            if let Some(on_connect) = &context.options.on_connected {
                on_connect(&context.endpoint, ConnectionHandle::new(&conn, &counter))
                    .await
                    .map_err(PoolConnectError::from)?;
            }
            Result::<Connection, PoolConnectError>::Ok(conn)
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
        // Boxed rather than pinned in place, so it can follow `counter` when a new
        // connection supersedes the current one.
        let mut idle_stream = Box::pin(counter.clone().idle_stream());

        tokio::pin!(idle_timer, conn_close);

        loop {
            tokio::select! {
                biased;

                // Handle new work
                handler = rx.recv() => {
                    match handler {
                        Some(RequestRef { mode, tx }) => {
                            assert!(mode.remote_id() == node_id, "Not for me!");
                            let supersedes = match (&mode, &state) {
                                (Mode::Handle(conn), Ok(current)) => {
                                    conn.stable_id() != current.stable_id()
                                }
                                (Mode::Handle(_), Err(_)) => true,
                                (Mode::Connect(_), _) => false,
                            };
                            if let (Mode::Handle(conn), true) = (mode, supersedes) {
                                debug!("handle new conn: supersede old");
                                let new_counter = ConnectionCounter::new();
                                if let Some(on_connect) = &context.options.on_connected {
                                    let handle = ConnectionHandle::new(&conn, &new_counter);
                                    if let Err(err) = on_connect(&context.endpoint, handle)
                                        .await
                                        .map_err(PoolConnectError::from) {
                                            tx.send(Err(err)).ok();
                                            continue;
                                        }
                                }
                                conn_close.as_mut().set_future(closed(conn.clone()));
                                let old_counter = std::mem::replace(&mut counter, new_counter);
                                old_counter.inner.superseded.store(true, Ordering::SeqCst);
                                idle_stream = Box::pin(counter.clone().idle_stream());
                                // Not closed here: the peer may still be using it. See
                                // `close_when_unused`.
                                if let Ok(old_conn) = std::mem::replace(&mut state, Ok(conn)) {
                                    let grace = context.options.idle_timeout;
                                    n0_future::task::spawn(
                                        close_when_unused(old_conn, old_counter, grace)
                                            .instrument(tracing::Span::current()),
                                    );
                                }
                            }
                            match &state {
                                Ok(state) => {
                                    let res = ConnectionRef::new(state.clone(), counter.get_one());
                                    debug!(current_count=counter.current(), "Handing out ConnectionRef");

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

/// Closes a superseded connection once nothing has used it for `grace`.
///
/// Closing it as soon as it is superseded would be wrong. Two peers that dial
/// each other at once each keep the connection they saw last, and the two may
/// disagree -- so each side can be receiving on the connection the other side
/// superseded. "Used" therefore covers both directions: our senders hold a
/// [`ConnectionRef`], and so does every stream the peer has open to us.
///
/// Detached rather than owned by the connection actor, because the connection
/// can outlive the actor that superseded it. It ends when the connection closes,
/// whoever closes it.
async fn close_when_unused(conn: Connection, counter: ConnectionCounter, grace: Duration) {
    loop {
        tokio::select! {
            _ = conn.closed() => return,
            _ = counter.idle() => {}
        }
        tokio::select! {
            _ = conn.closed() => return,
            _ = n0_future::time::sleep(grace) => {}
        }
        if counter.is_idle() {
            debug!(
                conn_id = conn.stable_id(),
                "closing superseded connection: unused"
            );
            conn.close(0u32.into(), CLOSE_SUPERSEDED);
            return;
        }
    }
}

struct Actor {
    rx: mpsc::Receiver<ActorMessage>,
    connections: HashMap<EndpointId, mpsc::Sender<RequestRef>>,
    context: Arc<Context>,
    // idle set (most recent last)
    // todo: use a better data structure if this becomes a performance issue
    idle: VecDeque<EndpointId>,
    // per connection tasks
    tasks: JoinSet<()>,
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
                tasks: JoinSet::new(),
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
                self.tasks.spawn(
                    context
                        .run_connection_actor(mode, conn_rx)
                        .instrument(error_span!("conn_actor", remote=%id.fmt_short())),
                );

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

                Some(res) = self.tasks.join_next(), if !self.tasks.is_empty() => {
                    res.expect("conn actor task panicked");
                }
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
    /// Set once a newer connection to the same peer took this one's place.
    superseded: AtomicBool,
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
                superseded: AtomicBool::new(false),
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

    /// Resolves once the count is zero.
    async fn idle(&self) {
        loop {
            // Registered before the check, so a drop to zero in between still
            // wakes us.
            let notified = self.inner.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.is_idle() {
                return;
            }
            notified.await;
        }
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

#[cfg(test)]
mod tests {
    use iroh::{endpoint::presets, Endpoint};
    use n0_error::{Result, StdResultExt};
    use n0_future::{task::AbortOnDropHandle, time::timeout};
    use n0_tracing_test::traced_test;
    use tokio::sync::mpsc;

    use super::*;

    const TEST_ALPN: &[u8] = b"iroh-gossip/pool-test/0";
    const IDLE_TIMEOUT: Duration = Duration::from_millis(200);

    /// Two connections from one client, both handed to a server-side pool.
    struct Superseded {
        /// Client end of the connection that was superseded.
        first: Connection,
        /// Client end of the connection that superseded it.
        second: Connection,
        /// The server's refs to `first` and `second`.
        first_ref: ConnectionRef,
        second_ref: ConnectionRef,
        _accept: AbortOnDropHandle<()>,
        _client: Endpoint,
    }

    async fn superseded() -> Result<Superseded> {
        let server = Endpoint::builder(presets::Minimal)
            .alpns(vec![TEST_ALPN.to_vec()])
            .bind()
            .await?;
        let server_addr = server.addr();
        let client = Endpoint::bind(presets::Minimal).await?;
        let options = Options {
            idle_timeout: IDLE_TIMEOUT,
            ..Default::default()
        };
        let pool = ConnectionPool::new(server.clone(), TEST_ALPN, options);

        let (refs_tx, mut refs) = mpsc::channel(4);
        let accept = AbortOnDropHandle::new(n0_future::task::spawn(async move {
            while let Some(incoming) = server.accept().await {
                let Ok(conn) = incoming.await else { continue };
                let conn_ref = pool.handle_connection(conn).await.expect("pool shut down");
                refs_tx.send(conn_ref).await.ok();
            }
        }));

        let first = client
            .connect(server_addr.clone(), TEST_ALPN)
            .await
            .std_context("connect first")?;
        // The pool must have taken the first connection before the second arrives,
        // otherwise there is nothing to supersede.
        let first_ref = refs.recv().await.expect("accept loop stopped");
        let second = client
            .connect(server_addr, TEST_ALPN)
            .await
            .std_context("connect second")?;
        assert_ne!(first.stable_id(), second.stable_id(), "connection reused");
        let second_ref = refs.recv().await.expect("accept loop stopped");
        Ok(Superseded {
            first,
            second,
            first_ref,
            second_ref,
            _accept: accept,
            _client: client,
        })
    }

    /// A superseded connection is closed once nothing uses it.
    ///
    /// Nothing else would close it: the pool stops watching it, and keep-alives
    /// stop the peer from closing it for us.
    #[tokio::test]
    #[traced_test]
    async fn superseded_connection_is_closed_once_unused() -> Result {
        let s = superseded().await?;
        assert!(s.first_ref.is_superseded());
        assert!(!s.second_ref.is_superseded());
        drop(s.first_ref);

        let err = timeout(IDLE_TIMEOUT * 10, s.first.closed())
            .await
            .std_context("superseded connection was not closed")?;
        assert!(
            matches!(
                &err,
                iroh::endpoint::ConnectionError::ApplicationClosed(frame)
                    if frame.reason == CLOSE_SUPERSEDED
            ),
            "closed for the wrong reason: {err:?}"
        );
        assert!(
            s.second.close_reason().is_none(),
            "the new connection was closed"
        );
        Ok(())
    }

    /// A superseded connection stays open for as long as something uses it.
    ///
    /// Two peers that dial each other at once each keep the connection they saw
    /// last, and may disagree, so the peer can still be using the one we
    /// superseded.
    #[tokio::test]
    #[traced_test]
    async fn superseded_connection_stays_open_while_used() -> Result {
        let s = superseded().await?;

        n0_future::time::sleep(IDLE_TIMEOUT * 5).await;
        assert!(
            s.first.close_reason().is_none(),
            "a superseded connection was closed while in use"
        );
        Ok(())
    }
}
