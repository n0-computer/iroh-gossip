use std::{collections::HashMap, future::Future};

use bytes::Bytes;
use iroh::{endpoint::Connection, Endpoint, EndpointId};
use n0_error::{AnyError, StdResultExt as _};
use n0_future::task::{self, JoinSet};
use tracing::{error, Instrument};

/// The result of a dial.
type DialResult = Result<Connection, AnyError>;

#[derive(Debug)]
pub(super) struct Dialer {
    endpoint: Endpoint,
    /// The ALPN of every dial.
    alpn: Bytes,
    tasks: JoinSet<(EndpointId, DialResult)>,
    /// The id of the dial task of each peer that is being dialed.
    dialing: HashMap<EndpointId, task::Id>,
}

impl Dialer {
    /// Creates a dialer that connects with `alpn` through `endpoint`.
    pub(super) fn new(endpoint: Endpoint, alpn: Bytes) -> Self {
        Self {
            endpoint,
            alpn,
            tasks: Default::default(),
            dialing: Default::default(),
        }
    }

    /// Starts to dial an endpoint by [`EndpointId`].
    pub(super) fn queue_dial(&mut self, endpoint_id: EndpointId) {
        let endpoint = self.endpoint.clone();
        let alpn = self.alpn.clone();
        self.spawn(endpoint_id, async move {
            endpoint.connect(endpoint_id, &alpn).await.anyerr()
        });
    }

    /// Runs `dial` as the dial of a peer, unless the peer is being dialed.
    fn spawn(
        &mut self,
        endpoint_id: EndpointId,
        dial: impl Future<Output = DialResult> + Send + 'static,
    ) {
        if self.is_dialing(endpoint_id) {
            return;
        }
        let task = self
            .tasks
            .spawn(async move { (endpoint_id, dial.await) }.instrument(tracing::Span::current()));
        self.dialing.insert(endpoint_id, task.id());
    }

    /// Checks if an endpoint is currently being dialed.
    pub(super) fn is_dialing(&self, endpoint: EndpointId) -> bool {
        self.dialing.contains_key(&endpoint)
    }

    /// Waits for the next dial operation to complete.
    ///
    /// A dial task that panicked or was cancelled counts as a failed dial of its peer.
    pub(super) async fn next_conn(&mut self) -> (EndpointId, DialResult) {
        let Some(joined) = self.tasks.join_next_with_id().await else {
            return std::future::pending().await;
        };
        let (peer, res) = match joined {
            Ok((_, output)) => output,
            Err(err) => {
                error!("dial task failed: {err:?}");
                let peer = self.dialing.iter().find(|(_, id)| **id == err.id());
                let peer = *peer.expect("a dial task is tracked").0;
                (peer, Err(AnyError::from_std(err)))
            }
        };
        self.dialing.remove(&peer);
        (peer, res)
    }
}

#[cfg(test)]
mod tests {
    use iroh::{endpoint::presets, SecretKey};
    use n0_error::Result;
    use n0_tracing_test::traced_test;

    use super::*;

    /// A dial task that panics counts as a failed dial, so the peer can be dialed again.
    #[tokio::test]
    #[traced_test]
    async fn panicked_dial_task_clears_dialing() -> Result {
        let endpoint = Endpoint::bind(presets::Minimal).await?;
        let peer = SecretKey::generate().public();
        let mut dialer = Dialer::new(endpoint, Bytes::new());
        for _ in 0..2 {
            dialer.spawn(peer, async { panic!("the dial panics") });
            assert!(dialer.is_dialing(peer));
            let (id, res) = dialer.next_conn().await;
            assert_eq!(id, peer);
            assert!(res.is_err());
            assert!(!dialer.is_dialing(peer));
        }
        Ok(())
    }
}
