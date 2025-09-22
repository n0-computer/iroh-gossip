//! Utilities for iroh-gossip networking

use std::{collections::BTreeSet, net::SocketAddr, pin::Pin};

use iroh::{endpoint::Connection, NodeAddr, NodeId, RelayUrl};
use irpc::{rpc::RemoteService, RpcMessage};
use n0_future::{
    boxed::BoxFuture,
    time::{sleep_until, Instant, Sleep},
    Stream, StreamExt,
};
use serde::{Deserialize, Serialize};

use crate::proto::{util::TimerMap, PeerData};

/// A connection to a remote service.
#[derive(Debug, Clone)]
pub struct IrohRemoteConnection(Connection);

impl IrohRemoteConnection {
    pub fn new(connection: Connection) -> Self {
        Self(connection)
    }
}

impl irpc::rpc::RemoteConnection for IrohRemoteConnection {
    fn clone_boxed(&self) -> Box<dyn irpc::rpc::RemoteConnection> {
        Box::new(self.clone())
    }

    fn open_bi(
        &self,
    ) -> BoxFuture<
        Result<(iroh::endpoint::SendStream, iroh::endpoint::RecvStream), irpc::RequestError>,
    > {
        let this = self.0.clone();
        Box::pin(async move {
            let pair = this.open_bi().await?;
            Ok(pair)
        })
    }
}

impl IrohRemoteConnection {
    pub(crate) fn into_request_stream<T: RemoteService>(
        self,
    ) -> impl Stream<Item = std::io::Result<T::Message>> {
        n0_future::stream::unfold(Some(self.0), async |conn| {
            let conn = conn?;
            match irpc_iroh::read_request::<T>(&conn).await {
                Err(err) => Some((Err(err), None)),
                Ok(None) => None,
                Ok(Some(request)) => Some((Ok(request), Some(conn))),
            }
        })
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AddrInfo {
    pub(crate) relay_url: Option<RelayUrl>,
    pub(crate) direct_addresses: BTreeSet<SocketAddr>,
}

impl From<NodeAddr> for AddrInfo {
    fn from(
        NodeAddr {
            relay_url,
            direct_addresses,
            ..
        }: NodeAddr,
    ) -> Self {
        Self {
            relay_url,
            direct_addresses,
        }
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

    pub(crate) fn to_node_addr(self, node_id: NodeId) -> NodeAddr {
        NodeAddr {
            node_id,
            relay_url: self.relay_url,
            direct_addresses: self.direct_addresses,
        }
    }
}

/// A [`TimerMap`] with an async method to wait for the next timer expiration.
#[derive(Debug)]
pub struct Timers<T> {
    next: Option<(Instant, Pin<Box<Sleep>>)>,
    map: TimerMap<T>,
}

impl<T> Default for Timers<T> {
    fn default() -> Self {
        Self {
            next: None,
            map: TimerMap::default(),
        }
    }
}

impl<T> Timers<T> {
    /// Creates a new timer map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a new entry at the specified instant
    pub fn insert(&mut self, instant: Instant, item: T) {
        self.map.insert(instant, item);
    }

    fn reset(&mut self) {
        self.next = self
            .map
            .first()
            .map(|instant| (*instant, Box::pin(sleep_until(*instant))))
    }

    /// Waits for the next timer to elapse.
    pub async fn wait_next(&mut self) -> Instant {
        self.reset();
        match self.next.as_mut() {
            Some((instant, sleep)) => {
                sleep.await;
                *instant
            }
            None => std::future::pending().await,
        }
    }

    /// Pops the earliest timer that expires at or before `now`.
    pub fn pop_before(&mut self, now: Instant) -> Option<(Instant, T)> {
        self.map.pop_before(now)
    }
}
