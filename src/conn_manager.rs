//! A connection manager to ensure roughly one open connection between peers,
//! if possible.
use std::{
    collections::{btree_map::Entry, BTreeMap},
    future::Future,
    sync::Arc,
};

use anyhow::Result;
use iroh::{
    endpoint::{ConnectOptions, Connection},
    Endpoint, NodeAddr, NodeId,
};
use n0_future::task;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{info_span, Instrument};

/// A connection manager.
///
/// Tries to de-duplicate connections between its endpoint and other nodes.
///
/// Instead of opening a new connection to another node it will return you
/// existing connections.
///
/// Unfortunately, it can't deduplicate connections to only a single one,
/// as we need to rely on the remote end to close redundant connections.
#[derive(Debug)]
pub struct ConnectionManager {
    endpoint: Endpoint,
    connections: Arc<Mutex<Connections>>,
    connect_opts: ConnectOptions,

    // Handling subtask cancellation, aborted on drop
    cancel: CancellationToken,
}

impl ConnectionManager {
    const CLOSE_CONNECTION_LIMIT_EXCEEDED_CODE: u32 = 10;
    const CLOSE_CONNECTION_LIMIT_EXCEEDED_MSG: &[u8] =
        b"ConnectionManager: Connection limit exceeded";

    const CLOSE_CONNECTION_SUPERSEDED_CODE: u32 = 11;
    const CLOSE_CONNECTION_SUPERSEDED_MSG: &[u8] = b"ConnectionManager: Connection superseded";

    /// TODO docs
    pub fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            connections: Default::default(),
            connect_opts: ConnectOptions::new(),
            cancel: CancellationToken::new(),
        }
    }

    /// TODO docs
    pub fn with_connect_options(mut self, opts: ConnectOptions) -> Self {
        self.connect_opts = opts;
        self
    }

    /// TODO docs
    #[tracing::instrument(skip_all)]
    pub async fn handle_connection(&self, conn: Connection) -> Result<()> {
        let remote_node_id = conn.remote_node_id()?;

        let alpn = conn
            .alpn()
            .ok_or_else(|| anyhow::anyhow!("Not tracking connections without ALPNs"))?;

        let mut conns = self.connections.lock().await;
        if let Err(_) = conns
            .accepted
            .insert(remote_node_id, alpn.clone(), conn.clone())
        {
            // Reject any incoming connection attempts over the connection limit
            conn.close(
                Self::CLOSE_CONNECTION_LIMIT_EXCEEDED_CODE.into(),
                &Self::CLOSE_CONNECTION_LIMIT_EXCEEDED_MSG,
            );
            return Ok(());
        }

        // If we had an open connection like this already, close it.
        if let Entry::Occupied(initiated_conn) =
            conns.initiated.entry((remote_node_id, alpn.clone()))
        {
            if !self.prefer_initiated(remote_node_id) {
                initiated_conn.remove().close(
                    Self::CLOSE_CONNECTION_SUPERSEDED_CODE.into(),
                    &Self::CLOSE_CONNECTION_SUPERSEDED_MSG,
                );
            }
        }

        // Listen to the remote end closing the connection:
        self.spawn(info_span!("observe_closed"), {
            let conns = self.connections.clone();
            let alpn = alpn.clone();
            async move {
                let close_err = conn.closed().await;
                tracing::debug!(
                    ?close_err,
                    "accepted conn closed, removing from connection set"
                );
                conns
                    .lock()
                    .await
                    .accepted
                    .remove(remote_node_id, alpn, &conn);
            }
        });

        Ok(())
    }

    /// TODO docs
    pub async fn get_or_open_connection(
        &self,
        remote_addr: impl Into<NodeAddr>,
        alpn: &[u8],
    ) -> Result<Connection> {
        let remote_addr = remote_addr.into();
        let remote_node_id = remote_addr.node_id;

        let mut conns = self.connections.lock().await;
        let Connections {
            ref mut initiated,
            ref mut accepted,
        } = &mut *conns;
        let initiated_conn = initiated.entry((remote_node_id, alpn.to_vec()));
        // If we already have an accepted connection & prefer that, reuse that one
        let accepted_conns = accepted.get_conns(remote_node_id, alpn.to_vec());
        let conn = match (initiated_conn, accepted_conns) {
            // No connection open for this - need to open a new connection
            (Entry::Vacant(spot), Entry::Vacant(_)) => {
                let connecting = self
                    .endpoint
                    .connect_with_opts(remote_addr, alpn, self.connect_opts.clone())
                    .await?;
                let conn = connecting.await?;
                spot.insert(conn.clone());
                conn
            }

            // We have accepted connections for this - re-use them.
            (Entry::Vacant(_), Entry::Occupied(accepted_conns)) => accepted_conns
                .get()
                .values()
                // Filter out closed connections as a best-effort in case they were closed while we were holding the lock
                .filter(|conn| conn.close_reason().is_none())
                // Hmm. Using "lowest RTT" as an arbitrary measure now.
                .min_by_key(|conn| conn.rtt())
                .expect("always one conn in ConnectionSet entry")
                .clone(),

            // We have already initiated a connection for this - reuse it.
            (Entry::Occupied(initiated_conn), Entry::Vacant(_)) => initiated_conn.get().clone(),

            // We have both initiated a connection for this, but also accepted some - potentially close ours.
            (Entry::Occupied(initiated_conn), Entry::Occupied(accepted_conns)) => {
                if !self.prefer_initiated(remote_node_id) {
                    initiated_conn.remove().close(
                        Self::CLOSE_CONNECTION_SUPERSEDED_CODE.into(),
                        &Self::CLOSE_CONNECTION_SUPERSEDED_MSG,
                    );

                    let best_conn = accepted_conns
                        .get()
                        .values()
                        // Filter out closed connections as a best-effort in case they were closed while we were holding the lock
                        .filter(|conn| conn.close_reason().is_none())
                        // Hmm. Using "lowest RTT" as an arbitrary measure now.
                        .min_by_key(|conn| conn.rtt())
                        .expect("always one conn in ConnectionSet entry")
                        .clone();
                    best_conn
                } else {
                    initiated_conn.get().clone()
                }
            }
        };

        Ok(conn)
    }

    fn spawn<F>(&self, span: tracing::Span, task: F)
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let token = self.cancel.clone();
        task::spawn(async move { token.run_until_cancelled(task).await }.instrument(span));
    }

    fn prefer_initiated(&self, remote_node_id: NodeId) -> bool {
        let node_id = self.endpoint.node_id();
        let our_way = blake3::hash(&[*node_id.as_bytes(), *remote_node_id.as_bytes()].concat());
        let remote_way = blake3::hash(&[*remote_node_id.as_bytes(), *node_id.as_bytes()].concat());
        our_way.as_bytes() < remote_way.as_bytes()
    }
}

impl Drop for ConnectionManager {
    fn drop(&mut self) {
        self.cancel.cancel();
        // quinn Connections will close automatically when dropped.
    }
}

// Private

#[derive(Debug, Default)]
struct Connections {
    initiated: BTreeMap<(NodeId, Alpn), Connection>,
    accepted: ConnectionSet,
}

#[derive(Debug, Default)]
struct ConnectionSet {
    inner: BTreeMap<(NodeId, Alpn), BTreeMap<usize, Connection>>,
}

type Alpn = Vec<u8>;

impl ConnectionSet {
    pub fn insert(&mut self, node_id: NodeId, alpn: Alpn, conn: Connection) -> Result<()> {
        let conns = self.inner.entry((node_id, alpn)).or_default();
        const CONN_LIMIT: usize = 5;
        anyhow::ensure!(conns.len() <= CONN_LIMIT, "Connection limit exceeded");
        conns.insert(conn.stable_id(), conn);
        Ok(())
    }

    pub fn remove(&mut self, node_id: NodeId, alpn: Alpn, conn: &Connection) {
        if let Entry::Occupied(mut entry) = self.inner.entry((node_id, alpn)) {
            entry.get_mut().remove(&conn.stable_id());
            if entry.get().is_empty() {
                entry.remove();
            }
        }
    }

    pub fn get_conns(
        &mut self,
        node_id: NodeId,
        alpn: Alpn,
    ) -> Entry<'_, (NodeId, Alpn), BTreeMap<usize, Connection>> {
        self.inner.entry((node_id, alpn))
    }
}
