use std::collections::HashMap;

use bytes::Bytes;
use iroh::{
    endpoint::{ConnectError, Connection},
    Endpoint, NodeId,
};
use tokio::task::JoinSet;
use tracing::{error, Instrument};

#[derive(Debug)]
pub(crate) struct Dialer {
    endpoint: Endpoint,
    pending: JoinSet<(NodeId, Result<Connection, ConnectError>)>,
    pending_dials: HashMap<NodeId, ()>,
}

impl Dialer {
    /// Create a new dialer for a [`Endpoint`]
    pub(crate) fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            pending: Default::default(),
            pending_dials: Default::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Starts to dial a node by [`NodeId`].
    pub(crate) fn queue_dial(&mut self, node_id: NodeId, alpn: Bytes) {
        if self.is_pending(node_id) {
            return;
        }
        self.pending_dials.insert(node_id, ());
        let endpoint = self.endpoint.clone();
        self.pending.spawn(
            async move {
                let res = endpoint.connect(node_id, &alpn).await;
                (node_id, res)
            }
            .instrument(tracing::Span::current()),
        );
    }

    /// Checks if a node is currently being dialed.
    pub(crate) fn is_pending(&self, node: NodeId) -> bool {
        self.pending_dials.contains_key(&node)
    }

    /// Waits for the next dial operation to complete.
    /// `None` means disconnected
    pub(crate) async fn next_conn(&mut self) -> (NodeId, Result<Connection, ConnectError>) {
        match self.pending_dials.is_empty() {
            false => {
                let (node_id, res) = loop {
                    match self.pending.join_next().await {
                        Some(Ok((node_id, res))) => {
                            self.pending_dials.remove(&node_id);
                            break (node_id, res);
                        }
                        Some(Err(e)) => {
                            error!("next conn error: {:?}", e);
                        }
                        None => {
                            error!("no more pending conns available");
                            std::future::pending().await
                        }
                    }
                };

                (node_id, res)
            }
            true => std::future::pending().await,
        }
    }
}
