//! Utilities for iroh-gossip networking

use std::{
    collections::BTreeSet,
    net::SocketAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use iroh::{endpoint::Connection, NodeAddr, NodeId, RelayUrl};
use irpc::rpc::RemoteService;
use n0_future::{
    future::Boxed as BoxFuture,
    time::{sleep_until, Instant},
    Stream,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

use crate::proto::{util::TimerMap, PeerData};

/// A connection to a remote service.
#[derive(Debug, Clone)]
pub struct IrohRemoteConnection(Connection);

impl IrohRemoteConnection {
    pub fn new(connection: Connection) -> Self {
        Self(connection)
    }
}

impl irpc::rpc::RemoteConnection for IrohRemoteConnection {
    fn clone_boxed(&self) -> Box<dyn irpc::rpc::RemoteConnection> {
        Box::new(self.clone())
    }

    fn open_bi(
        &self,
    ) -> BoxFuture<
        Result<(iroh::endpoint::SendStream, iroh::endpoint::RecvStream), irpc::RequestError>,
    > {
        let this = self.0.clone();
        Box::pin(async move {
            let pair = this.open_bi().await?;
            Ok(pair)
        })
    }
}

pub(crate) fn accept_stream<T: RemoteService>(
    connection: Connection,
) -> impl Stream<Item = std::io::Result<T::Message>> {
    n0_future::stream::unfold(Some(connection), async |conn| {
        let conn = conn?;
        match irpc_iroh::read_request::<T>(&conn).await {
            Err(err) => Some((Err(err), None)),
            Ok(None) => None,
            Ok(Some(request)) => Some((Ok(request), Some(conn))),
        }
    })
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AddrInfo {
    pub(crate) relay_url: Option<RelayUrl>,
    pub(crate) direct_addresses: BTreeSet<SocketAddr>,
}

impl From<NodeAddr> for AddrInfo {
    fn from(
        NodeAddr {
            relay_url,
            direct_addresses,
            ..
        }: NodeAddr,
    ) -> Self {
        Self {
            relay_url,
            direct_addresses,
        }
    }
}

impl AddrInfo {
    pub(crate) fn encode(&self) -> PeerData {
        let bytes = postcard::to_stdvec(self).expect("serializing AddrInfo may not fail");
        PeerData::new(bytes)
    }

    pub(crate) fn decode(peer_data: &PeerData) -> Result<AddrInfo, postcard::Error> {
        let bytes = peer_data.as_bytes();
        if bytes.is_empty() {
            return Ok(AddrInfo::default());
        }
        let info = postcard::from_bytes(bytes)?;
        Ok(info)
    }

    pub(crate) fn into_node_addr(self, node_id: NodeId) -> NodeAddr {
        NodeAddr {
            node_id,
            relay_url: self.relay_url,
            direct_addresses: self.direct_addresses,
        }
    }
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

#[derive(Debug)]
struct ConnectionCounterInner {
    count: AtomicUsize,
    notify: Notify,
}

#[derive(Debug, Clone)]
pub(crate) struct ConnectionCounter {
    inner: Arc<ConnectionCounterInner>,
}

impl ConnectionCounter {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(ConnectionCounterInner {
                count: Default::default(),
                notify: Notify::new(),
            }),
        }
    }

    /// Increase the connection count and return a guard for the new connection
    pub(crate) fn get_one(&self) -> OneConnection {
        self.inner.count.fetch_add(1, Ordering::SeqCst);
        OneConnection {
            inner: self.inner.clone(),
        }
    }

    pub(crate) fn guard<T>(&self, item: T) -> Guarded<T> {
        Guarded::new(item, self.get_one())
    }

    pub(crate) fn is_idle(&self) -> bool {
        self.inner.count.load(Ordering::SeqCst) == 0
    }

    pub(crate) async fn idle(&self) {
        self.inner.notify.notified().await
    }

    pub(crate) async fn idle_for(&self, duration: Duration) {
        let fut = self.idle();
        tokio::pin!(fut);
        loop {
            (&mut fut).await;
            fut.set(self.idle());
            tokio::time::sleep(duration).await;
            if self.is_idle() {
                break;
            }
        }
    }
}

/// Guard for one connection
#[derive(Debug)]
pub(crate) struct OneConnection {
    inner: Arc<ConnectionCounterInner>,
}

impl Clone for OneConnection {
    fn clone(&self) -> Self {
        self.inner.count.fetch_add(1, Ordering::SeqCst);
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl Drop for OneConnection {
    fn drop(&mut self) {
        let prev = self.inner.count.fetch_sub(1, Ordering::SeqCst);
        if prev == 1 {
            self.inner.notify.notify_waiters();
        }
    }
}

#[derive(derive_more::Deref, derive_more::DerefMut, Debug)]
pub(crate) struct Guarded<T> {
    #[deref]
    #[deref_mut]
    inner: T,
    guard: OneConnection,
}

impl<T> Guarded<T> {
    pub(crate) fn new(inner: T, guard: OneConnection) -> Self {
        Self { inner, guard }
    }

    pub(crate) fn split(self) -> (T, OneConnection) {
        (self.inner, self.guard)
    }
}
