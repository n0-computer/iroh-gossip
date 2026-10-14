//! Hooks to remember the good peers of a topic, and to discover peers for it.

use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
};

use iroh::EndpointId;
use n0_future::boxed::BoxStream;

use crate::proto::TopicId;

/// Remembers the good peers of a topic, and discovers peers for it.
///
/// Gossip calls [`Self::discover`] for a topic that has no neighbors, and
/// joins the topic through each peer the stream yields. The hooks tell the
/// implementation which peers proved good: a peer that became our neighbor,
/// and a peer we received a message from. They are synchronous and called on
/// the gossip actor, so they must be cheap. An implementation that needs to do
/// IO in them should spawn it.
///
/// A peer we lose because we ourselves went offline looks failed to us, so an
/// implementation that remembers peers should not forget them on failure.
pub trait TopicDiscovery: std::fmt::Debug + Send + Sync + 'static {
    /// We joined `topic`. For example, start announcing it.
    fn topic_joined(&self, _topic: TopicId) {}

    /// We left `topic`. Stop announcing it, and drop what was kept for it.
    fn topic_left(&self, _topic: TopicId) {}

    /// `peer` became our neighbor in `topic`.
    fn neighbor_up(&self, _topic: TopicId, _peer: EndpointId) {}

    /// We received a message in `topic` from `peer`, so it is reachable and in the topic.
    ///
    /// Called for every message, from neighbors and other peers alike, so it
    /// must be very cheap. Use it to refresh a peer that stays a neighbor for
    /// a long time.
    fn peer_seen(&self, _topic: TopicId, _peer: EndpointId) {}

    /// Returns a stream of peers to join `topic` through, for example the
    /// results of a DHT lookup as they arrive.
    ///
    /// Called for a topic without neighbors, at most every 10 seconds, and
    /// never while an earlier stream for the topic still runs. Gossip joins through each peer the stream yields, and drops
    /// the stream once the topic has a neighbor. The stream is polled on the
    /// gossip actor, so it must not block.
    fn discover(&self, topic: TopicId) -> BoxStream<EndpointId>;
}

/// How often gossip checks for topics without neighbors, see [`TopicDiscovery::discover`].
pub(crate) const DISCOVERY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

/// Remembers the most recently seen good peers of each topic, in memory and up to a fixed number.
///
/// A peer counts as seen when it becomes our neighbor or sends us a message in
/// the topic. Peers only leave by age, never because they failed, so a node
/// that loses its own network can rejoin through the peers it had. All is
/// forgotten when we leave the topic.
#[derive(Debug)]
pub struct RecentPeers {
    capacity: usize,
    /// Per topic, least recently seen first.
    peers: Mutex<HashMap<TopicId, VecDeque<EndpointId>>>,
}

impl RecentPeers {
    /// Returns a memory of up to `capacity` peers per topic.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            peers: Default::default(),
        }
    }

    fn touch(&self, topic: TopicId, peer: EndpointId) {
        let mut peers = self.peers.lock().expect("poisoned");
        let recent = peers.entry(topic).or_default();
        recent.retain(|known| *known != peer);
        recent.push_back(peer);
        if recent.len() > self.capacity {
            recent.pop_front();
        }
    }
}

impl Default for RecentPeers {
    /// Remembers up to 8 peers per topic.
    fn default() -> Self {
        Self::new(8)
    }
}

impl TopicDiscovery for RecentPeers {
    fn topic_left(&self, topic: TopicId) {
        self.peers.lock().expect("poisoned").remove(&topic);
    }

    fn neighbor_up(&self, topic: TopicId, peer: EndpointId) {
        self.touch(topic, peer);
    }

    fn peer_seen(&self, topic: TopicId, peer: EndpointId) {
        self.touch(topic, peer);
    }

    /// Yields the remembered peers, the most recently seen first.
    fn discover(&self, topic: TopicId) -> BoxStream<EndpointId> {
        let peers = self.peers.lock().expect("poisoned");
        let recent: Vec<_> = peers
            .get(&topic)
            .into_iter()
            .flatten()
            .rev()
            .copied()
            .collect();
        Box::pin(n0_future::stream::iter(recent))
    }
}

#[cfg(test)]
mod tests {
    use iroh::SecretKey;
    use n0_future::StreamExt;

    use super::*;

    fn peer(n: u8) -> EndpointId {
        SecretKey::from_bytes(&[n; 32]).public()
    }

    fn discovered(recent: &RecentPeers, topic: TopicId) -> Vec<EndpointId> {
        n0_future::future::block_on(recent.discover(topic).collect())
    }

    /// The most recently seen peers come first, and the oldest beyond the
    /// capacity are dropped.
    #[test]
    fn recent_peers_keep_the_latest() {
        let topic: TopicId = [1; 32].into();
        let recent = RecentPeers::new(3);
        recent.neighbor_up(topic, peer(1));
        recent.neighbor_up(topic, peer(2));
        recent.peer_seen(topic, peer(3));
        recent.peer_seen(topic, peer(1));
        recent.neighbor_up(topic, peer(4));
        assert_eq!(discovered(&recent, topic), vec![peer(4), peer(1), peer(3)]);
    }

    /// Topics are remembered apart, and leaving one forgets it.
    #[test]
    fn recent_peers_are_per_topic() {
        let a: TopicId = [1; 32].into();
        let b: TopicId = [2; 32].into();
        let recent = RecentPeers::default();
        recent.neighbor_up(a, peer(1));
        recent.neighbor_up(b, peer(2));
        assert_eq!(discovered(&recent, a), vec![peer(1)]);
        recent.topic_left(a);
        assert!(discovered(&recent, a).is_empty());
        assert_eq!(discovered(&recent, b), vec![peer(2)]);
    }
}
