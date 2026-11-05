use std::collections::HashSet;

use bytes::Bytes;
use iroh::{endpoint::Connection, Endpoint, EndpointId};
use n0_future::task::JoinSet;
use tracing::{error, Instrument};

#[derive(Debug)]
pub(super) struct Dialer {
    endpoint: Endpoint,
    pending: JoinSet<(EndpointId, Result<Connection, iroh::endpoint::ConnectError>)>,
    pending_dials: HashSet<EndpointId>,
}

impl Dialer {
    /// Create a new dialer for a [`Endpoint`]
    pub(super) fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            pending: Default::default(),
            pending_dials: Default::default(),
        }
    }

    /// Starts to dial a endpoint by [`EndpointId`].
    pub(super) fn queue_dial(&mut self, endpoint_id: EndpointId, alpn: Bytes) {
        if self.is_dialing(endpoint_id) {
            return;
        }
        self.pending_dials.insert(endpoint_id);
        let endpoint = self.endpoint.clone();
        self.pending.spawn(
            async move {
                let res = endpoint.connect(endpoint_id, &alpn).await;
                (endpoint_id, res)
            }
            .instrument(tracing::Span::current()),
        );
    }

    /// Checks if a endpoint is currently being dialed.
    pub(super) fn is_dialing(&self, endpoint: EndpointId) -> bool {
        self.pending_dials.contains(&endpoint)
    }

    /// Waits for the next dial operation to complete.
    pub(super) async fn next_conn(
        &mut self,
    ) -> (EndpointId, Result<Connection, iroh::endpoint::ConnectError>) {
        match self.pending_dials.is_empty() {
            false => {
                let (endpoint_id, res) = loop {
                    match self.pending.join_next().await {
                        Some(Ok((endpoint_id, res))) => {
                            self.pending_dials.remove(&endpoint_id);
                            break (endpoint_id, res);
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

                (endpoint_id, res)
            }
            true => std::future::pending().await,
        }
    }
}
