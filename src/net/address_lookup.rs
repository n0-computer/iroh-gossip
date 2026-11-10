//! An address lookup service to gather addressing info collected from gossip Join and ForwardJoin messages.

use std::{
    collections::{BTreeMap, BTreeSet},
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::Duration,
};

use iroh::{
    address_lookup::{self, AddressLookup, EndpointData, EndpointInfo},
    EndpointAddr, RelayUrl,
};
use iroh_base::EndpointId;
use n0_future::{
    boxed::BoxStream,
    stream::{self, StreamExt},
    task::AbortOnDropHandle,
    time::SystemTime,
};
use serde::{Deserialize, Serialize};

use crate::proto::PeerData;

/// The most addresses we keep for an endpoint.
///
/// Every update adds the addresses that it carries, so without a cap, a peer
/// whose addresses keep changing grows its entry without limit.
const MAX_ADDRS: usize = 32;

struct RetentionOpts {
    /// How long to keep received endpoint info records alive before pruning them
    retention: Duration,
    /// How often to check for expired entries
    evict_interval: Duration,
}

impl Default for RetentionOpts {
    fn default() -> Self {
        Self {
            retention: Duration::from_secs(60 * 5),
            evict_interval: Duration::from_secs(30),
        }
    }
}

/// An address lookup service that expires endpoints after some time.
///
/// It is added to the endpoint when constructing a gossip instance, and the gossip actor
/// then adds endpoint addresses as received with Join or ForwardJoin messages.
#[derive(Debug, Clone)]
pub(super) struct GossipAddressLookup {
    endpoints: NodeMap,
    _task_handle: Arc<AbortOnDropHandle<()>>,
}

type NodeMap = Arc<RwLock<BTreeMap<EndpointId, StoredEndpointInfo>>>;

#[derive(Debug)]
struct StoredEndpointInfo {
    data: EndpointData,
    last_updated: SystemTime,
}

impl Default for GossipAddressLookup {
    fn default() -> Self {
        Self::new()
    }
}

impl GossipAddressLookup {
    const PROVENANCE: &'static str = "gossip";

    /// Creates a new gossip address lookup instance.
    fn new() -> Self {
        Self::with_opts(Default::default())
    }

    fn with_opts(opts: RetentionOpts) -> Self {
        let endpoints: NodeMap = Default::default();
        let task = {
            let endpoints = Arc::downgrade(&endpoints);
            n0_future::task::spawn(async move {
                let mut interval = n0_future::time::interval(opts.evict_interval);
                loop {
                    interval.tick().await;
                    let Some(endpoints) = endpoints.upgrade() else {
                        break;
                    };
                    let now = SystemTime::now();
                    endpoints.write().expect("poisoned").retain(|_k, v| {
                        let age = now.duration_since(v.last_updated).unwrap_or(Duration::MAX);
                        age <= opts.retention
                    });
                }
            })
        };
        Self {
            endpoints,
            _task_handle: Arc::new(AbortOnDropHandle::new(task)),
        }
    }

    /// Augments endpoint addressing information for the given endpoint ID.
    ///
    /// The provided addresses come first, then the ones already stored, up to
    /// [`MAX_ADDRS`]. The user data is overwritten.
    pub(super) fn add(&self, endpoint_info: impl Into<EndpointInfo>) {
        let last_updated = SystemTime::now();
        let EndpointInfo { endpoint_id, data } = endpoint_info.into();
        let mut guard = self.endpoints.write().expect("poisoned");
        let stored = guard
            .entry(endpoint_id)
            .or_insert_with(|| StoredEndpointInfo {
                data: EndpointData::default(),
                last_updated,
            });
        // `EndpointData::new` drops the duplicates.
        let addrs = data.addrs().chain(stored.data.addrs());
        stored.data = EndpointData::new(addrs.take(MAX_ADDRS).cloned().collect());
        stored.data.set_user_data(data.user_data().cloned());
        stored.last_updated = last_updated;
    }
}

impl AddressLookup for GossipAddressLookup {
    fn resolve(
        &self,
        endpoint_id: EndpointId,
    ) -> Option<BoxStream<Result<address_lookup::Item, address_lookup::Error>>> {
        let guard = self.endpoints.read().expect("poisoned");
        let info = guard.get(&endpoint_id)?;
        let last_updated = info
            .last_updated
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("time drift")
            .as_micros() as u64;
        let item = address_lookup::Item::new(
            EndpointInfo::from_parts(endpoint_id, info.data.clone()),
            Self::PROVENANCE,
            Some(last_updated),
        );
        Some(stream::iter(Some(Ok(item))).boxed())
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub(super) struct AddrInfo {
    pub(super) relay_url: Option<RelayUrl>,
    pub(super) direct_addresses: BTreeSet<SocketAddr>,
}

impl From<EndpointAddr> for AddrInfo {
    fn from(endpoint_addr: EndpointAddr) -> Self {
        Self {
            relay_url: endpoint_addr.relay_urls().next().cloned(),
            direct_addresses: endpoint_addr.ip_addrs().cloned().collect(),
        }
    }
}

pub(super) fn encode_peer_data(info: &AddrInfo) -> PeerData {
    let bytes = postcard::to_stdvec(info).expect("serializing AddrInfo may not fail");
    PeerData::new(bytes)
}

pub(super) fn decode_peer_data(peer_data: &PeerData) -> Result<AddrInfo, postcard::Error> {
    let bytes = peer_data.as_bytes();
    if bytes.is_empty() {
        return Ok(AddrInfo::default());
    }
    let info = postcard::from_bytes(bytes)?;
    Ok(info)
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, time::Duration};

    use iroh::{address_lookup::AddressLookup, EndpointAddr, SecretKey};
    use n0_future::StreamExt;
    use rand::{RngExt, SeedableRng};

    use super::{GossipAddressLookup, RetentionOpts, MAX_ADDRS};

    #[tokio::test]
    async fn test_retention() {
        let opts = RetentionOpts {
            evict_interval: Duration::from_millis(100),
            retention: Duration::from_millis(500),
        };
        let disco = GossipAddressLookup::with_opts(opts);

        let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
        let k1 = SecretKey::from_bytes(&rng.random());
        let a1 = EndpointAddr::new(k1.public());

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

        assert!(disco.resolve(k1.public()).is_none());
    }

    /// A peer whose address keeps changing has its newest addresses kept, up to the cap.
    #[tokio::test]
    async fn changing_addresses_stay_capped() {
        let disco = GossipAddressLookup::new();
        let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
        let id = SecretKey::from_bytes(&rng.random()).public();
        let addr = |port| SocketAddr::from(([10, 0, 0, 1], port));
        for port in 0..1000 {
            disco.add(EndpointAddr::new(id).with_ip_addr(addr(port)));
        }

        let endpoints = disco.endpoints.read().expect("poisoned");
        let ip_addrs: Vec<_> = endpoints[&id].data.ip_addrs().copied().collect();
        assert_eq!(ip_addrs.len(), MAX_ADDRS);
        assert_eq!(ip_addrs[0], addr(999), "the newest address comes first");
    }
}
