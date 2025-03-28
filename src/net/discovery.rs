use anyhow::Result;
use iroh::{
    discovery::{static_provider::StaticProvider, Discovery, DiscoveryItem},
    watchable::Watchable,
    NodeAddr,
};
use n0_future::boxed::BoxStream;

use super::AddrInfo;

#[derive(Debug, Clone)]
pub struct GossipDiscovery {
    pub(super) inner: StaticProvider,
    pub(super) our_addr: Option<Watchable<Option<AddrInfo>>>,
}

impl Default for GossipDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

impl GossipDiscovery {
    /// Create a new [`GossipDiscovery`] that publishes our address with our join messages.
    ///
    /// By adding this to both an [`Endpoint`] and a [`Gossip`], this will both make address info
    /// retrieved from other nodes available to the endpoint, and make our address info available to
    /// other nodes.
    ///
    /// [`Endpoint`]: iroh::Endpoint
    /// [`Gossip`]: crate::net::Gossip
    pub fn new() -> Self {
        Self {
            inner: Default::default(),
            our_addr: Some(Default::default()),
        }
    }

    /// Create a new [`GossipDiscovery`] that does not publish our address with our join messages.
    ///
    /// By adding this to both an [`Endpoint`] and a [`Gossip`], this will make address info
    /// retrieved from other nodes available to the endpoint, but does not publish our address
    /// info to other nodes.
    ///
    /// [`Endpoint`]: iroh::Endpoint
    /// [`Gossip`]: crate::net::Gossip
    pub fn without_publish() -> Self {
        Self {
            inner: Default::default(),
            our_addr: None,
        }
    }

    pub(super) fn add_node_addr(&self, addr: NodeAddr) {
        self.inner.add_node_info(addr);
    }
}

impl Discovery for GossipDiscovery {
    fn publish(&self, data: &iroh::node_info::NodeData) {
        if let Some(watchable) = self.our_addr.as_ref() {
            tracing::info!("PUBLISH {data:?}");
            watchable.set(Some(data.into())).ok();
        }
    }

    fn resolve(
        &self,
        endpoint: iroh::Endpoint,
        node_id: iroh::NodeId,
    ) -> Option<BoxStream<Result<DiscoveryItem>>> {
        self.inner.resolve(endpoint, node_id)
    }
}
