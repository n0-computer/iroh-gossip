use std::collections::HashSet;

use bytes::Bytes;
use iroh::{
    endpoint::{ConnectError, Connection},
    Endpoint, EndpointId,
};
use tokio::task::JoinSet;
use tracing::Instrument;

#[derive(Debug, Default)]
pub(crate) struct Dialer {
    pending_tasks: JoinSet<(EndpointId, Result<Connection, ConnectError>)>,
    pending_nodes: HashSet<EndpointId>,
}

impl Dialer {
    /// Starts to dial a node by [`EndpointId`].
    pub(crate) fn queue_dial(&mut self, endpoint: &Endpoint, endpoint_id: EndpointId, alpn: Bytes) {
        if self.pending_nodes.insert(endpoint_id) {
            let endpoint = endpoint.clone();
            let fut = async move { (endpoint_id, endpoint.connect(endpoint_id, &alpn).await) }
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
    pub(crate) async fn next(&mut self) -> Option<(EndpointId, Result<Connection, ConnectError>)> {
        match self.pending_tasks.join_next().await {
            Some(res) => {
                let (endpoint_id, res) = res.expect("connect task panicked");
                self.pending_nodes.remove(&endpoint_id);
                Some((endpoint_id, res))
            }
            None => None,
        }
    }
}
