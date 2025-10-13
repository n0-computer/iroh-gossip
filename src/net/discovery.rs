//! A discovery service to gather addressing info collected from gossip Join and ForwardJoin messages.

use std::{
    collections::{btree_map::Entry, BTreeMap},
    sync::{Arc, RwLock},
    time::Duration,
};

use iroh_base::NodeId;
use n0_future::{
    boxed::BoxStream,
    stream::{self, StreamExt},
    task::AbortOnDropHandle,
    time::SystemTime,
};

use iroh::discovery::{Discovery, DiscoveryError, DiscoveryItem, NodeData, NodeInfo};

pub(crate) struct RetentionOpts {
    retention: Duration,
    check_interval: Duration,
}

impl Default for RetentionOpts {
    fn default() -> Self {
        Self {
            retention: Duration::from_secs(60 * 5),
            check_interval: Duration::from_secs(30),
        }
    }
}

/// A static node discovery that expires nodes after some time.
///
/// It is added to the endpoint when constructing a gossip instance, and the gossip actor
/// then adds node addresses as received with Join or ForwardJoin messages.
#[derive(Debug, Clone)]
pub(crate) struct GossipDiscovery {
    nodes: NodeMap,
    _task_handle: Arc<AbortOnDropHandle<()>>,
}

type NodeMap = Arc<RwLock<BTreeMap<NodeId, StoredNodeInfo>>>;

#[derive(Debug)]
struct StoredNodeInfo {
    data: NodeData,
    last_updated: SystemTime,
}

impl Default for GossipDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

impl GossipDiscovery {
    const PROVENANCE: &'static str = "gossip";

    /// Creates a new gossip discovery instance.
    pub(crate) fn new() -> Self {
        Self::with_opts(Default::default())
    }

    pub(crate) fn with_opts(opts: RetentionOpts) -> Self {
        let nodes: NodeMap = Default::default();
        let task = {
            let nodes = Arc::downgrade(&nodes);
            tokio::task::spawn(async move {
                loop {
                    tokio::time::sleep(opts.check_interval).await;
                    let now = SystemTime::now();
                    if let Some(nodes) = nodes.upgrade() {
                        let mut inner = nodes.write().expect("poisoned");
                        inner.retain(|_k, v| {
                            let age = now.duration_since(v.last_updated).unwrap_or(Duration::MAX);
                            age < opts.retention
                        });
                    } else {
                        break;
                    }
                }
            })
        };
        Self {
            nodes,
            _task_handle: Arc::new(AbortOnDropHandle::new(task)),
        }
    }

    /// Augments node addressing information for the given node ID.
    ///
    /// The provided addressing information is combined with the existing info in the static
    /// provider.  Any new direct addresses are added to those already present while the
    /// relay URL is overwritten.
    pub(crate) fn add(&self, node_info: impl Into<NodeInfo>) {
        let last_updated = SystemTime::now();
        let NodeInfo { node_id, data } = node_info.into();
        let mut guard = self.nodes.write().expect("poisoned");
        match guard.entry(node_id) {
            Entry::Occupied(mut entry) => {
                let existing = entry.get_mut();
                existing
                    .data
                    .add_direct_addresses(data.direct_addresses().iter().copied());
                existing.data.set_relay_url(data.relay_url().cloned());
                existing.data.set_user_data(data.user_data().cloned());
                existing.last_updated = last_updated;
            }
            Entry::Vacant(entry) => {
                entry.insert(StoredNodeInfo { data, last_updated });
            }
        }
    }
}

impl Discovery for GossipDiscovery {
    fn publish(&self, _data: &NodeData) {}

    fn resolve(&self, node_id: NodeId) -> Option<BoxStream<Result<DiscoveryItem, DiscoveryError>>> {
        let guard = self.nodes.read().expect("poisoned");
        let info = guard.get(&node_id);
        match info {
            Some(node_info) => {
                let last_updated = node_info
                    .last_updated
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .expect("time drift")
                    .as_micros() as u64;
                let item = DiscoveryItem::new(
                    NodeInfo::from_parts(node_id, node_info.data.clone()),
                    Self::PROVENANCE,
                    Some(last_updated),
                );
                Some(stream::iter(Some(Ok(item))).boxed())
            }
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use iroh::{discovery::Discovery, NodeAddr, SecretKey};
    use n0_future::StreamExt;

    use super::{GossipDiscovery, RetentionOpts};

    #[tokio::test]
    async fn test_retention() {
        let opts = RetentionOpts {
            check_interval: Duration::from_millis(100),
            retention: Duration::from_millis(500),
        };
        let disco = GossipDiscovery::with_opts(opts);

        let k1 = SecretKey::generate(&mut rand::rng());
        let a1 = NodeAddr::new(k1.public());

        disco.add(a1);

        assert!(matches!(
            disco.resolve(k1.public()).unwrap().next().await,
            Some(Ok(_))
        ));

        tokio::time::sleep(Duration::from_millis(200)).await;

        assert!(matches!(
            disco.resolve(k1.public()).unwrap().next().await,
            Some(Ok(_))
        ));

        tokio::time::sleep(Duration::from_millis(700)).await;

        assert!(matches!(disco.resolve(k1.public()), None));
    }
}
