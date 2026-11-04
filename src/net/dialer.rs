use std::collections::HashSet;

use bytes::Bytes;
use iroh::{endpoint::Connection, Endpoint, EndpointId};
use n0_future::task::JoinSet;
use tracing::{error, Instrument};

#[derive(Debug)]
pub(super) struct Dialer {
    endpoint: Endpoint,
    /// The ALPN of every dial.
    alpn: Bytes,
    pending: JoinSet<(EndpointId, Result<Connection, iroh::endpoint::ConnectError>)>,
    pending_dials: HashSet<EndpointId>,
}

impl Dialer {
    /// Creates a dialer that connects with `alpn` through `endpoint`.
    pub(super) fn new(endpoint: Endpoint, alpn: Bytes) -> Self {
        Self {
            endpoint,
            alpn,
            pending: Default::default(),
            pending_dials: Default::default(),
        }
    }

    /// Starts to dial an endpoint by [`EndpointId`].
    pub(super) fn queue_dial(&mut self, endpoint_id: EndpointId) {
        if self.is_dialing(endpoint_id) {
            return;
        }
        self.pending_dials.insert(endpoint_id);
        let endpoint = self.endpoint.clone();
        let alpn = self.alpn.clone();
        self.pending.spawn(
            async move {
                let res = endpoint.connect(endpoint_id, &alpn).await;
                (endpoint_id, res)
            }
            .instrument(tracing::Span::current()),
        );
    }

    /// Checks if an endpoint is currently being dialed.
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
