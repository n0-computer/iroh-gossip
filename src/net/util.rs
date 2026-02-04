//! Utilities for iroh-gossip networking

use std::collections::BTreeSet;

use iroh::{EndpointAddr, EndpointId, TransportAddr};
use n0_future::time::{sleep_until, Instant};
use serde::{Deserialize, Serialize};

use crate::proto::{util::TimerMap, PeerData};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AddrInfo {
    pub(crate) addrs: BTreeSet<TransportAddr>,
}

impl From<EndpointAddr> for AddrInfo {
    fn from(EndpointAddr { addrs, .. }: EndpointAddr) -> Self {
        Self { addrs }
    }
}

impl AddrInfo {
    pub(crate) fn encode(&self) -> PeerData {
        let bytes = postcard::to_stdvec(self).expect("serializing AddrInfo may not fail");
        PeerData::new(bytes)
    }

    pub(crate) fn decode(peer_data: &PeerData) -> Result<AddrInfo, postcard::Error> {
        let bytes = peer_data.as_bytes();
        if bytes.is_empty() {
            return Ok(AddrInfo::default());
        }
        let info = postcard::from_bytes(bytes)?;
        Ok(info)
    }

    pub(crate) fn into_endpoint_addr(self, endpoint_id: EndpointId) -> EndpointAddr {
        EndpointAddr {
            id: endpoint_id,
            addrs: self.addrs,
        }
    }
}

/// A [`TimerMap`] with an async method to wait for the next timer expiration.
#[derive(Debug)]
pub struct Timers<T> {
    map: TimerMap<T>,
}

impl<T> Default for Timers<T> {
    fn default() -> Self {
        Self {
            map: TimerMap::default(),
        }
    }
}

impl<T> Timers<T> {
    /// Inserts a new entry at the specified instant
    pub fn insert(&mut self, instant: Instant, item: T) {
        self.map.insert(instant, item);
    }

    /// Waits for the next timer to elapse.
    pub async fn wait_next(&mut self) -> Instant {
        match self.map.first() {
            None => std::future::pending::<Instant>().await,
            Some(instant) => {
                sleep_until(*instant).await;
                *instant
            }
        }
    }

    /// Pops the earliest timer that expires at or before `now`.
    pub fn pop_before(&mut self, now: Instant) -> Option<(Instant, T)> {
        self.map.pop_before(now)
    }
}
