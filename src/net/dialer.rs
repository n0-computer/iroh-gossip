use std::collections::HashSet;

use bytes::Bytes;
use iroh::{
    endpoint::{ConnectError, Connection},
    Endpoint, EndpointId,
};
use tokio::task::JoinSet;
use tracing::{debug, Instrument};

#[derive(Debug, Default)]
pub(crate) struct Dialer {
    tasks: JoinSet<(EndpointId, Result<Connection, ConnectError>)>,
    pending_endpoint_ids: HashSet<EndpointId>,
}

impl Dialer {
    /// Starts to dial a node by [`EndpointId`].
    pub(crate) fn queue_dial(&mut self, endpoint: &Endpoint, endpoint_id: EndpointId, alpn: Bytes) {
        if self.pending_endpoint_ids.insert(endpoint_id) {
            debug!(remote=%endpoint_id.fmt_short(), "start dial");
            let endpoint = endpoint.clone();
            let fut = async move { (endpoint_id, endpoint.connect(endpoint_id, &alpn).await) }
                .instrument(tracing::Span::current());
            self.tasks.spawn(fut);
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Waits for the next dial operation to complete.
    /// `None` means disconnected
    ///
    /// Will be pending forever if no connections are in progress.
    pub(crate) async fn next(&mut self) -> Option<(EndpointId, Result<Connection, ConnectError>)> {
        debug!("dialer next");
        match self.tasks.join_next().await {
            Some(res) => {
                debug!("dialer next some!");
                let (endpoint_id, res) = res.expect("connect task panicked");
                self.pending_endpoint_ids.remove(&endpoint_id);
                Some((endpoint_id, res))
            }
            None => None,
        }
    }
}
