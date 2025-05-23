//! Implementation of the iroh-gossip protocol, as an IO-less state machine
//!
//! This module implements the iroh-gossip protocol. The entry point is [`State`], which contains
//! the protocol state for a node.
//!
//! The iroh-gossip protocol is made up from two parts: A swarm membership protocol, based on
//! [HyParView][hyparview], and a gossip broadcasting protocol, based on [PlumTree][plumtree].
//!
//! For a full explanation it is recommended to read the two papers. What follows is a brief
//! outline of the protocols.
//!
//! All protocol messages are namespaced by a [`TopicId`], a 32 byte identifier. Topics are
//! separate swarms and broadcast scopes. The HyParView and PlumTree algorithms both work in the
//! scope of a single topic. Thus, joining multiple topics increases the number of open connections
//! to peers and the size of the local routing table.
//!
//! The **membership protocol** ([HyParView][hyparview]) is a cluster protocol where each peer
//! maintains a partial view of all nodes in the swarm.
//! A peer joins the swarm for a topic by connecting to any known peer that is a member of this
//! topic's swarm. Obtaining this initial contact info happens out of band. The peer then sends
//! a `Join` message to that initial peer. All peers maintain a list of
//! `active` and `passive` peers. Active peers are those that you maintain active connections to.
//! Passive peers is an addressbook of additional peers. If one of your active peers goes offline,
//! its slot is filled with a random peer from the passive set. In the default configuration, the
//! active view has a size of 5 and the passive view a size of 30.
//! The HyParView protocol ensures that active connections are always bidirectional, and regularly
//! exchanges nodes for the passive view in a `Shuffle` operation.
//! Thus, this protocol exposes a high degree of reliability and auto-recovery in the case of node
//! failures.
//!
//! The **gossip protocol** ([PlumTree][plumtree]) builds upon the membership protocol. It exposes
//! a method to broadcast messages to all peers in the swarm. On each node, it maintains two sets
//! of peers: An `eager` set and a `lazy` set. Both are subsets of the `active` view from the
//! membership protocol. When broadcasting a message from the local node, or upon receiving a
//! broadcast message, the message is pushed to all peers in the eager set. Additionally, the hash
//! of the message (which uniquely identifies it), but not the message content, is lazily pushed
//! to all peers  in the `lazy` set. When receiving such lazy pushes (called `Ihaves`), those peers
//! may request the message content after a timeout if they didn't receive the message by one of
//! their eager peers before. When requesting a message from a currently-lazy peer, this peer is
//! also upgraded to be an eager peer from that moment on. This strategy self-optimizes the
//! messaging graph by latency. Note however that this optimization will work best if the messaging
//! paths are stable, i.e. if it's always the same peer that broadcasts. If not, the relative
//! message redundancy will grow and the ideal messaging graph might change frequently.
//!
//! [hyparview]: https://asc.di.fct.unl.pt/~jleitao/pdf/dsn07-leitao.pdf
//! [plumtree]: https://asc.di.fct.unl.pt/~jleitao/pdf/srds07-leitao.pdf

use std::{fmt, hash::Hash};

use bytes::Bytes;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

mod hyparview;
mod plumtree;
pub mod state;
pub mod topic;
pub mod util;

#[cfg(any(test, feature = "test-utils"))]
pub mod sim;

pub use hyparview::Config as HyparviewConfig;
pub use plumtree::{Config as PlumtreeConfig, DeliveryScope, Scope};
pub use state::{InEvent, Message, OutEvent, State, Timer, TopicId};
pub use topic::{Command, Config, Event, IO};

/// The default maximum size in bytes for a gossip message.
/// This is a sane but arbitrary default and can be changed in the [`Config`].
pub const DEFAULT_MAX_MESSAGE_SIZE: usize = 4096;

/// The minimum allowed value for [`Config::max_message_size`].
pub const MIN_MAX_MESSAGE_SIZE: usize = 512;

/// The identifier for a peer.
///
/// The protocol implementation is generic over this trait. When implementing the protocol,
/// a concrete type must be chosen that will then be used throughout the implementation to identify
/// and index individual peers.
///
/// Note that the concrete type will be used in protocol messages. Therefore, implementations of
/// the protocol are only compatible if the same concrete type is supplied for this trait.
///
/// TODO: Rename to `PeerId`? It does not necessarily refer to a peer's address, as long as the
/// networking layer can translate the value of its concrete type into an address.
pub trait PeerIdentity: Hash + Eq + Ord + Copy + fmt::Debug + Serialize + DeserializeOwned {}
impl<T> PeerIdentity for T where
    T: Hash + Eq + Ord + Copy + fmt::Debug + Serialize + DeserializeOwned
{
}

/// Opaque binary data that is transmitted on messages that introduce new peers.
///
/// Implementations may use these bytes to supply addresses or other information needed to connect
/// to a peer that is not included in the peer's [`PeerIdentity`].
#[derive(derive_more::Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[debug("PeerData({}b)", self.0.len())]
pub struct PeerData(Bytes);

impl PeerData {
    /// Create a new [`PeerData`] from a byte buffer.
    pub fn new(data: impl Into<Bytes>) -> Self {
        Self(data.into())
    }

    /// Get a reference to the contained [`bytes::Bytes`].
    pub fn inner(&self) -> &bytes::Bytes {
        &self.0
    }

    /// Get the peer data as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// PeerInfo contains a peer's identifier and the opaque peer data as provided by the implementer.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
struct PeerInfo<PI> {
    pub id: PI,
    pub data: Option<PeerData>,
}

impl<PI> From<(PI, Option<PeerData>)> for PeerInfo<PI> {
    fn from((id, data): (PI, Option<PeerData>)) -> Self {
        Self { id, data }
    }
}

#[cfg(test)]
mod test {

    use std::{collections::HashSet, env, fmt, str::FromStr};

    use rand::SeedableRng;
    use rand_chacha::ChaCha12Rng;
    use tracing_test::traced_test;

    use super::{Command, Config, Event};
    use crate::proto::{
        sim::{LatencyConfig, Network, NetworkConfig},
        Scope, TopicId,
    };

    #[test]
    #[traced_test]
    fn hyparview_smoke() {
        // Create a network with 4 nodes and active_view_capacity 2
        let rng = ChaCha12Rng::seed_from_u64(read_var("SEED", 0));
        let mut config = Config::default();
        config.membership.active_view_capacity = 2;
        let network_config = NetworkConfig {
            proto: config,
            latency: LatencyConfig::default_static(),
        };
        let mut network = Network::new(network_config, rng);
        for i in 0..4 {
            network.insert(i);
        }

        let t: TopicId = [0u8; 32].into();

        // Do some joins between nodes 0,1,2
        network.command(0, t, Command::Join(vec![1, 2]));
        network.command(1, t, Command::Join(vec![2]));
        network.command(2, t, Command::Join(vec![]));

        network.run_trips(3);

        // Confirm emitted events
        let actual = network.events_sorted();
        let expected = sort(vec![
            (0, t, Event::NeighborUp(1)),
            (0, t, Event::NeighborUp(2)),
            (1, t, Event::NeighborUp(2)),
            (1, t, Event::NeighborUp(0)),
            (2, t, Event::NeighborUp(0)),
            (2, t, Event::NeighborUp(1)),
        ]);
        assert_eq!(actual, expected);

        // Confirm active connections
        assert_eq!(network.conns(), vec![(0, 1), (0, 2), (1, 2)]);

        // Now let node 3 join node 0.
        // Node 0 is full, so it will disconnect from either node 1 or node 2.
        network.command(3, t, Command::Join(vec![0]));

        network.run_trips(2);

        // Confirm emitted events. There's two options because whether node 0 disconnects from
        // node 1 or node 2 is random.
        let actual = network.events_sorted();
        eprintln!("actual {actual:#?}");
        let expected1 = sort(vec![
            (3, t, Event::NeighborUp(0)),
            (0, t, Event::NeighborUp(3)),
            (0, t, Event::NeighborDown(1)),
            (1, t, Event::NeighborDown(0)),
        ]);
        let expected2 = sort(vec![
            (3, t, Event::NeighborUp(0)),
            (0, t, Event::NeighborUp(3)),
            (0, t, Event::NeighborDown(2)),
            (2, t, Event::NeighborDown(0)),
        ]);
        assert!((actual == expected1) || (actual == expected2));

        // Confirm active connections.
        if actual == expected1 {
            assert_eq!(network.conns(), vec![(0, 2), (0, 3), (1, 2)]);
        } else {
            assert_eq!(network.conns(), vec![(0, 1), (0, 3), (1, 2)]);
        }
        assert!(network.check_synchronicity());
    }

    #[test]
    #[traced_test]
    fn plumtree_smoke() {
        let rng = ChaCha12Rng::seed_from_u64(read_var("SEED", 0));
        let network_config = NetworkConfig {
            proto: Config::default(),
            latency: LatencyConfig::default_static(),
        };
        let mut network = Network::new(network_config, rng);
        // build a network with 6 nodes
        for i in 0..6 {
            network.insert(i);
        }

        let t = [0u8; 32].into();

        // let node 0 join the topic but do not connect to any peers
        network.command(0, t, Command::Join(vec![]));
        // connect nodes 1 and 2 to node 0
        (1..3).for_each(|i| network.command(i, t, Command::Join(vec![0])));
        // connect nodes 4 and 5 to node 3
        network.command(3, t, Command::Join(vec![]));
        (4..6).for_each(|i| network.command(i, t, Command::Join(vec![3])));
        // run ticks and drain events

        network.run_trips(4);

        let _ = network.events();
        assert!(network.check_synchronicity());

        // now broadcast a first message
        network.command(
            1,
            t,
            Command::Broadcast(b"hi1".to_vec().into(), Scope::Swarm),
        );

        network.run_trips(4);

        let events = network.events();
        let received = events.filter(|x| matches!(x, (_, _, Event::Received(_))));
        // message should be received by two other nodes
        assert_eq!(received.count(), 2);
        assert!(network.check_synchronicity());

        // now connect the two sections of the swarm
        network.command(2, t, Command::Join(vec![5]));
        network.run_trips(3);
        let _ = network.events();
        println!("{}", network.report());

        // now broadcast again
        network.command(
            1,
            t,
            Command::Broadcast(b"hi2".to_vec().into(), Scope::Swarm),
        );
        network.run_trips(5);
        let events = network.events();
        let received = events.filter(|x| matches!(x, (_, _, Event::Received(_))));
        // message should be received by all 5 other nodes
        assert_eq!(received.count(), 5);
        assert!(network.check_synchronicity());
        println!("{}", network.report());
    }

    #[test]
    #[traced_test]
    fn quit() {
        // Create a network with 4 nodes and active_view_capacity 2
        let rng = ChaCha12Rng::seed_from_u64(read_var("SEED", 0));
        let mut config = Config::default();
        config.membership.active_view_capacity = 2;
        let mut network = Network::new(config.into(), rng);
        let num = 4;
        for i in 0..num {
            network.insert(i);
        }

        let t: TopicId = [0u8; 32].into();

        // join all nodes
        network.command(0, t, Command::Join(vec![]));
        network.command(1, t, Command::Join(vec![0]));
        network.command(2, t, Command::Join(vec![1]));
        network.command(3, t, Command::Join(vec![2]));
        network.run_trips(2);

        // assert all peers appear in the connections
        let all_conns: HashSet<u64> = HashSet::from_iter((0u64..4).flat_map(|p| {
            network
                .neighbors(&p, &t)
                .into_iter()
                .flat_map(|x| x.into_iter())
        }));
        assert_eq!(all_conns, HashSet::from_iter([0, 1, 2, 3]));
        assert!(network.check_synchronicity());

        //  let node 3 leave the swarm
        network.command(3, t, Command::Quit);
        network.run_trips(4);
        assert!(network.peer(&3).unwrap().state(&t).is_none());

        // assert all peers without peer 3 appear in the connections
        let all_conns: HashSet<u64> = HashSet::from_iter((0..num).flat_map(|p| {
            network
                .neighbors(&p, &t)
                .into_iter()
                .flat_map(|x| x.into_iter())
        }));
        assert_eq!(all_conns, HashSet::from_iter([0, 1, 2]));
        assert!(network.check_synchronicity());
    }

    fn read_var<T: FromStr<Err: fmt::Display + fmt::Debug>>(name: &str, default: T) -> T {
        env::var(name)
            .map(|x| {
                x.parse()
                    .unwrap_or_else(|_| panic!("Failed to parse environment variable {name}"))
            })
            .unwrap_or(default)
    }

    fn sort<T: Ord + Clone>(items: Vec<T>) -> Vec<T> {
        let mut sorted = items;
        sorted.sort();
        sorted
    }
}
