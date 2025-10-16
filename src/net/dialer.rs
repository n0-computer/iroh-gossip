use std::collections::HashSet;

use bytes::Bytes;
use iroh::{
    endpoint::{ConnectError, Connection},
    Endpoint, NodeId,
};
use tokio::task::JoinSet;
use tracing::Instrument;

#[derive(Debug, Default)]
pub(crate) struct Dialer {
    pending_tasks: JoinSet<(NodeId, Result<Connection, ConnectError>)>,
    pending_nodes: HashSet<NodeId>,
}

impl Dialer {
    /// Starts to dial a node by [`NodeId`].
    pub(crate) fn queue_dial(&mut self, endpoint: &Endpoint, node_id: NodeId, alpn: Bytes) {
        if self.pending_nodes.insert(node_id) {
            let endpoint = endpoint.clone();
            let fut = async move { (node_id, endpoint.connect(node_id, &alpn).await) }
                .instrument(tracing::Span::current());
            self.pending_tasks.spawn(fut);
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pending_tasks.is_empty()
    }

    /// Waits for the next dial operation to complete.
    /// `None` means disconnected
    ///
    /// Will be pending forever if no connections are in progress.
    pub(crate) async fn next(&mut self) -> Option<(NodeId, Result<Connection, ConnectError>)> {
        match self.pending_tasks.join_next().await {
            Some(res) => {
                let (node_id, res) = res.expect("connect task panicked");
                self.pending_nodes.remove(&node_id);
                Some((node_id, res))
            }
            None => None,
        }
    }
}
