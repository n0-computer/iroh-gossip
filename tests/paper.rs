//! Tests for the claims the HyParView and Plumtree papers make.
//!
//! This crate implements two protocols from Leitao, Pereira and Rodrigues:
//! HyParView (DSN 2007) for swarm membership, and Plumtree, "Epidemic
//! Broadcast Trees" (SRDS 2007), for broadcast on top of it. Each paper backs
//! its design with measured claims. The tests here restate those claims as
//! assertions against [`Simulator`], and cite the section, figure, or table
//! they come from. Sections and figures refer to the technical reports of May
//! 2007: HyParView is TR-07-13
//! (<https://asc.di.fct.unl.pt/~jleitao/pdf/dsn07-leitao.pdf>), and Plumtree
//! is TR-07-14 (<https://asc.di.fct.unl.pt/~jleitao/pdf/srds07-leitao.pdf>),
//! with its 2019 correction of Algorithm 1.
//!
//! The papers report observations of their own simulations, not guarantees.
//! A test here checks that this implementation shows the same behavior in a
//! smaller swarm. Its bound sits between where this implementation measures
//! and where the defect it guards against starts, and says which. A bound
//! that the defect would also pass is no bound. Some tests check behavior
//! that this implementation adds to the papers, and say so.
//!
//! Peers join one at a time through a single contact, as in the HyParView
//! paper (5), and the overlay comes out close to the random graph that the
//! papers measure. Unlike there, shuffles and maintenance passes run during
//! the joins, and each swarm runs for 20s afterwards instead of 50 membership
//! cycles. The simulator's default of every peer joining at once yields an
//! overlay about 25 times more clustered, which is a fair model of a crowd
//! hitting one bootstrap peer but not of the papers' setup. The `big_*` tests
//! in `tests/sim.rs` cover it.
//!
//! A claim this implementation does not yet meet on every seed is marked
//! `#[ignore]` with the reason, rather than asserted at a weaker level, so the
//! commit that makes it hold shows up as the removal of that line.
//!
//! The papers simulate 10,000 nodes; these run at 200 to 400 so the file stays
//! fast in debug builds. Claims hold across five seeds by default. Set
//! `PAPER_SEEDS` to check more, for example `PAPER_SEEDS=50 cargo test
//! --release --test paper`, and `PAPER_PEERS` to run every claim at another
//! size, for example `PAPER_PEERS=10000` for the papers' own.

use std::{
    collections::{hash_map::Entry, BTreeMap, BTreeSet, HashMap, VecDeque},
    time::Duration,
};

use iroh_gossip::proto::{
    sim::{LatencyConfig, NetworkConfig, RoundStats, Simulator, SimulatorConfig},
    Config, TopicId,
};
use rand::seq::IteratorRandom;

/// The topic [`Simulator`] runs all its peers on.
const TOPIC: TopicId = TopicId::from_bytes([0u8; 32]);

/// Swarm size for tests that do not vary it.
///
/// Large enough that the overlay is far from fully connected (the active view
/// holds 5), small enough that a test stays well under a second in release.
const PEERS: usize = 200;

// ---------------------------------------------------------------------------
// HyParView: overlay shape
// ---------------------------------------------------------------------------

/// Claim: active views are bounded by their capacity and close to full.
///
/// HyParView 4.1 keeps a small active view of fixed size and fills it as far
/// as it can. Figure 5 of the paper goes further: after 50 membership cycles
/// "almost all nodes" hold a full view of 5.
///
/// That stronger claim does not quite hold here: over 50 seeds, 78 to 90
/// percent of the peers settle at 5 and the rest at 4. The bound on the share
/// of full views is therefore set below the level measured today, as a guard
/// against it getting worse, not at the paper's.
#[test]
fn hyparview_active_view_is_bounded_and_nearly_full() {
    let peers = swarm_size(PEERS);
    for seed in seeds() {
        let sim = swarm(peers, seed);
        let capacity = Config::default().membership.active_view_capacity;
        let sizes = active_view_sizes(&sim);

        let max = *sizes.iter().max().expect("swarm is not empty");
        let mean = sizes.iter().sum::<usize>() as f32 / sizes.len() as f32;
        let full =
            sizes.iter().filter(|size| **size == capacity).count() as f32 / sizes.len() as f32;
        report(
            "hyparview: active view",
            seed,
            format!(
                "max {max}, mean {mean:.2} of {capacity}, {:.0}% full",
                full * 100.0
            ),
        );

        assert!(
            max <= capacity,
            "seed {seed}: active view exceeded capacity"
        );
        assert!(
            mean >= capacity as f32 - 1.0,
            "seed {seed}: mean active view {mean:.2} is far below capacity {capacity}"
        );
        // Measured 78 to 90 percent over 50 seeds; the paper reports almost all.
        assert!(
            full >= 0.6,
            "seed {seed}: only {:.0}% of active views are full",
            full * 100.0
        );
    }
}

/// Claim: every peer reaches every other one over active links.
///
/// HyParView 4: connectivity is the property the protocol exists to preserve.
/// "No peer without neighbors" is strictly weaker: it passes for an overlay
/// split into two halves.
#[test]
fn hyparview_overlay_is_connected() {
    let peers = swarm_size(PEERS);
    for seed in seeds() {
        let sim = swarm(peers, seed);
        let components = components(&sim).len();
        report(
            "hyparview: connectivity",
            seed,
            format!("{components} component(s)"),
        );
        assert!(
            strongly_connected(&adjacency(&sim)),
            "seed {seed}: some peer cannot reach another over active links"
        );
    }
}

/// Claim: every active link is symmetric, and no node hangs on a single one.
///
/// HyParView 4.1: "if node q is in the active view of node p then node p is
/// also in the active view of node q", because the protocol relies on the link
/// to detect failure from both ends. An asymmetric view leaves one side
/// forwarding into a link the other side does not watch.
///
/// A link is briefly one-sided while the `Neighbor` or `Disconnect` that
/// changes it is in flight, and with shuffles running there is never an instant
/// with nothing in flight. So this checks that no link stays one-sided: a link
/// asymmetric in three snapshots two maximum link latencies apart is stuck.
///
/// The paper's degree argument (5.4) is that no node should be weakly attached.
/// With symmetric views, in-degree is the active view size, so the check that
/// adds something is the minimum: a node with a single link is isolated by the
/// next failure.
#[test]
fn hyparview_active_views_are_symmetric() {
    let peers = swarm_size(PEERS);
    for seed in seeds() {
        let mut sim = swarm(peers, seed);
        let min_degree = active_view_sizes(&sim)
            .into_iter()
            .min()
            .expect("swarm is not empty");

        let mut stuck = asymmetric_links(&sim);
        let transient = stuck.len();
        for _ in 0..2 {
            sim.network.run_trips(2);
            let now = asymmetric_links(&sim);
            stuck.retain(|link| now.contains(link));
        }
        report(
            "hyparview: symmetry",
            seed,
            format!(
                "{transient} link(s) one-sided in passing, {} stuck, minimum degree {min_degree}",
                stuck.len()
            ),
        );

        assert!(
            stuck.is_empty(),
            "seed {seed}: links stuck one-sided: {stuck:?}"
        );
        assert!(
            min_degree >= 2,
            "seed {seed}: a node is attached by {min_degree} link(s)"
        );
    }
}

/// Returns the links held from one end only, as unordered pairs.
///
/// Unordered, so that a pair of peers adding and dropping each other in turn
/// shows up as one link stuck one-sided rather than as two that each resolve.
fn asymmetric_links(sim: &Simulator) -> BTreeSet<(u64, u64)> {
    let mut links = BTreeSet::new();
    for peer in sim.network.peer_ids() {
        for neighbor in neighbors(sim, peer) {
            if !neighbors(sim, neighbor).contains(&peer) {
                links.insert((peer.min(neighbor), peer.max(neighbor)));
            }
        }
    }
    links
}

/// Claim: the overlay has the shape of a random graph.
///
/// HyParView Table 1 reports, at 10,000 nodes, a clustering coefficient of
/// 0.00092 and an average shortest path of 6.39. For a random graph of degree
/// k those are about k/n = 0.0005 and ln(n)/ln(k-1) = 6.64, so the paper's
/// overlay sits close to random on both. Low clustering is what the paper
/// credits for surviving massive failure (5.2); short paths are what keep the
/// broadcast shallow.
///
/// This checks the same two ratios at 400 nodes, where random is 0.0125 and
/// 4.32.
#[test]
fn hyparview_overlay_is_a_random_graph() {
    let peers = swarm_size(400);
    let degree = Config::default().membership.active_view_capacity as f64;
    let random_clustering = degree / peers as f64;
    let random_path = (peers as f64).ln() / (degree - 1.0).ln();

    for seed in seeds() {
        let sim = swarm(peers, seed);
        assert!(
            strongly_connected(&adjacency(&sim)),
            "seed {seed}: graph is split"
        );
        let clustering = clustering_coefficient(&sim);
        let path = average_shortest_path(&sim);
        report(
            "hyparview: graph",
            seed,
            format!(
                "clustering {clustering:.4} (random {random_clustering:.4}), \
                 shortest path {path:.2} (random {random_path:.2})"
            ),
        );

        // Measured 0.0035 to 0.0136 over 50 seeds. An overlay built by every
        // peer joining at once measures 0.045 to 0.083.
        assert!(
            clustering < random_clustering * 2.0,
            "seed {seed}: clustering {clustering:.4} is far above a random graph's"
        );
        // Measured 4.12 to 4.18 over 50 seeds.
        assert!(
            path < random_path * 1.1,
            "seed {seed}: average shortest path {path:.2} is far above a random graph's"
        );
    }
}

// ---------------------------------------------------------------------------
// HyParView: failures
// ---------------------------------------------------------------------------

/// Claim: failed neighbors leave every active view at once.
///
/// HyParView uses its TCP links as a failure detector (4.1, 4.3), which is what
/// lets it recover "almost immediately" (5.2). The paper's accuracy metric (2.3)
/// is the share of a node's neighbors that are still alive; this asserts it is
/// back to 100 percent within five maximum link latencies of a failure, time
/// for a failed send to report back.
#[test]
fn hyparview_failed_neighbors_leave_active_views_at_once() {
    let peers = swarm_size(PEERS);
    for fraction in [50, 80] {
        for seed in seeds() {
            let mut sim = swarm(peers, seed);
            let mut rng = sim.rng();
            let failed: BTreeSet<u64> = sim
                .network
                .peer_ids()
                .sample(&mut rng, peers * fraction / 100)
                .into_iter()
                .collect();
            for peer in &failed {
                sim.network.remove(peer);
            }
            sim.network.run_trips(5);

            let stale: Vec<(u64, u64)> = sim
                .network
                .peer_ids()
                .flat_map(|peer| {
                    neighbors(&sim, peer)
                        .into_iter()
                        .filter(|neighbor| failed.contains(neighbor))
                        .map(move |neighbor| (peer, neighbor))
                })
                .collect();
            report(
                &format!("hyparview: accuracy after {fraction}%"),
                seed,
                format!("{} failed peer(s) still in active views", stale.len()),
            );
            assert!(
                stale.is_empty(),
                "seed {seed}, {fraction}% failure: failed peers still in active views: {stale:?}"
            );
        }
    }
}

/// Claim: delivery barely notices a large failure, even before the overlay heals.
///
/// HyParView 5.2 sends its messages right after the failure, "before the
/// execution of another cycle of the membership protocol", and reports that
/// failures below 90 percent have "almost no visible impact". Figure 3 shows
/// why: reliability returns to 100 percent within the first few messages.
///
/// The first message after a large failure can reach almost nobody, because its
/// sender may have lost every neighbor an instant ago. The Plumtree paper sees
/// the same (4.4, Figure 6a). So this asserts that from the fifth message on,
/// every message reaches every survivor.
///
/// No shuffle and no maintenance pass runs after the failure. Only the
/// reactive repair of 4.3 and the broadcast repair of Plumtree heal the swarm.
#[test]
fn hyparview_delivers_through_failure_without_waiting_to_heal() {
    let peers = swarm_size(PEERS);
    const MESSAGES: usize = 30;
    const SETTLED_FROM: usize = 4;

    for fraction in [10, 30, 50] {
        for seed in seeds() {
            let mut sim = swarm(peers, seed);
            sim.remove_peers(peers * fraction / 100);
            sim.network.stop_membership_cycles();

            let mut rates = Vec::with_capacity(MESSAGES);
            for idx in 0..MESSAGES {
                let stats = broadcast(&mut sim);
                if idx >= SETTLED_FROM {
                    assert_eq!(
                        stats.missed,
                        0.0,
                        "seed {seed}, {fraction}% failure: message {} missed peers",
                        idx + 1
                    );
                }
                rates.push(reliability(&sim, &stats));
            }
            report(
                &format!("hyparview: {fraction}% failure, no heal"),
                seed,
                format!(
                    "first message {:.1}%, mean {:.2}% over {MESSAGES}",
                    rates[0],
                    rates.iter().sum::<f32>() / MESSAGES as f32
                ),
            );
        }
    }
}

/// Claim: the swarm survives a large fraction of its nodes failing at once.
///
/// This is HyParView's headline result (5.2): failures below 90 percent have
/// "almost no visible impact" on delivery, where comparable protocols lose half
/// their reach at 40 percent, and the overlay recovers "in few rounds (only 1 or
/// 2)" (5.3). The paper measures a 10,000 node overlay; this runs 400 so that 80
/// percent failure still leaves a swarm worth testing, and gives it two
/// membership cycles to recover.
///
/// Up to half the swarm failing, nothing may be lost: a survivor would need all
/// 30 of its passive peers to have failed, which at 50 percent is a one in a
/// billion event. At 80 percent that chance is about one in a thousand per
/// survivor, and such a survivor has no address left to reach the swarm
/// through. These chances assume full passive views of independent peers,
/// which the test does not check. The paper reports the average, so above half
/// the swarm this asserts that a seed loses at most 1 percent of its survivors,
/// and at least 2 may go, and that delivery averages above 99.5 percent.
/// Measured over 50 seeds at 80 percent: one seed loses one survivor, and
/// delivery averages 99.97 percent.
#[test]
fn hyparview_survives_massive_simultaneous_failure() {
    let peers = swarm_size(400);

    for fraction in [10, 30, 50, 80] {
        let mut rates = Vec::new();
        for seed in seeds() {
            let mut sim = swarm(peers, seed);
            assert_eq!(broadcast(&mut sim).missed, 0.0, "baseline delivery failed");

            sim.remove_peers(peers * fraction / 100);
            run_membership_cycles(&mut sim, 2);

            let survivors = sim.peer_count();
            let parts = components(&sim);
            let components = parts.len();
            // A survivor cut off from the rest can be the sender, and then
            // nobody else gets its message. Send from the largest component, and
            // count the survivors outside it as lost too.
            let largest = parts
                .iter()
                .rev()
                .max_by_key(|part| part.len())
                .expect("a survivor is left");
            let sender = *largest.iter().next().expect("a survivor is left");
            let stats = broadcast_from(&mut sim, sender);
            let reliability = reliability(&sim, &stats);
            let cut_off = stats.missed;
            report(
                &format!("hyparview: {fraction}% failure"),
                seed,
                format!(
                    "{survivors} survivors, {components} component(s), \
                     reliability {reliability:.2}%"
                ),
            );

            if fraction <= 50 {
                assert!(
                    !sim.report().has_peers_with_no_neighbors(),
                    "seed {seed}, {fraction}% failure: a survivor was left isolated"
                );
                assert_eq!(
                    components, 1,
                    "seed {seed}, {fraction}% failure: overlay partitioned into {components} components"
                );
                assert_eq!(
                    stats.missed, 0.0,
                    "seed {seed}, {fraction}% failure: delivery was not total"
                );
            } else {
                let allowed = (survivors as f32 / 100.0).ceil().max(2.0);
                assert!(
                    cut_off <= allowed,
                    "seed {seed}, {fraction}% failure: {cut_off} of {survivors} survivors cut off"
                );
            }
            rates.push(reliability);
        }
        let mean = rates.iter().sum::<f32>() / rates.len() as f32;
        assert!(
            mean >= 99.5,
            "{fraction}% failure: delivery averaged {mean:.2}% across seeds"
        );
    }
}

/// Claim: the swarm stays whole while peers join and leave continuously.
///
/// HyParView 4.3: the overlay absorbs churn by itself, replacing a failed
/// neighbor from the passive view as soon as the failure is detected. Plumtree
/// 4.3 fails half a percent of the swarm every cycle and sees no drop in
/// delivery. This replaces five percent every membership cycle, joiners and
/// all, and asserts every round ends with one component and total delivery.
///
/// Newcomers join through three random peers, the way an application passes
/// several bootstrap addresses. With a single one, a newcomer whose contact is
/// among the next round's failures has no address left that reaches the swarm,
/// which no protocol can repair.
#[test]
fn hyparview_survives_continuous_churn() {
    let peers = swarm_size(300);
    // Peers replaced per round, five percent of the swarm.
    let churn = peers / 20;
    const ROUNDS: usize = 25;

    for seed in seeds() {
        let broken = churn_rounds_broken(swarm(peers, seed), churn, ROUNDS);
        report(
            "hyparview: churn",
            seed,
            format!("{ROUNDS} rounds at {churn}/{peers}: {broken} broken"),
        );
        assert_eq!(
            broken, 0,
            "seed {seed}: {broken} rounds had an isolated peer, a split, or a missed delivery"
        );
    }
}

/// Replaces `churn` peers per round for `rounds` rounds, one membership cycle
/// apart, and returns how many rounds ended with an isolated peer, a split
/// overlay, or a missed delivery.
fn churn_rounds_broken(mut sim: Simulator, churn: usize, rounds: usize) -> usize {
    let mut next_id = 100_000u64;
    let mut broken = 0;
    for _ in 0..rounds {
        sim.remove_peers(churn);
        for _ in 0..churn {
            let mut rng = sim.rng();
            let contacts = sim.network.peer_ids().sample(&mut rng, 3);
            sim.network.insert_and_join(next_id, TOPIC, contacts);
            next_id += 1;
        }
        run_membership_cycles(&mut sim, 1);
        let whole =
            !sim.report().has_peers_with_no_neighbors() && strongly_connected(&adjacency(&sim));
        let delivered = broadcast(&mut sim).missed == 0.0;
        if !whole || !delivered {
            broken += 1;
        }
    }
    broken
}

/// Extension: a peer cut off from everyone finds its way back.
///
/// The papers have no rejoin. This implementation adds it to the maintenance
/// pass. A peer whose only neighbor was its bootstrap peer, and whose passive view is
/// empty, has nobody left to dial when that neighbor restarts: HyParView's
/// repair works from the passive view (4.3), and the passive view refills
/// through shuffles with active peers (4.4), of which there are none. The
/// bootstrap peers the application joined with are the only addresses left.
/// Without the maintenance pass the peer is still alone a minute later.
#[test]
fn hyparview_isolated_peer_rejoins_through_its_bootstrap_peer() {
    // Peer 1 joins through peer 0, and they are each other's only peer.
    let mut sim = swarm(2, 0);
    assert_eq!(neighbors(&sim, 1), vec![0]);

    // Peer 0 restarts with no memory of peer 1.
    sim.network.remove(&0);
    sim.network.run_trips(5);
    assert!(neighbors(&sim, 1).is_empty(), "peer 1 should notice");
    sim.network.insert_with_config(0, Config::default());
    sim.network
        .command(0, TOPIC, iroh_gossip::proto::Command::Join(vec![]));

    sim.network.run_duration(Duration::from_secs(60));
    let reconnected = neighbors(&sim, 1) == vec![0] && neighbors(&sim, 0) == vec![1];
    report(
        "hyparview: isolated peer",
        0,
        if reconnected {
            "reconnected after a minute"
        } else {
            "still alone after a minute"
        },
    );
    assert!(reconnected, "the isolated peer did not rejoin");
}

/// Claim: the overlay heals quickly after a failure, without waiting for a
/// shuffle.
///
/// HyParView 5.3 recovers "in few rounds (only 1 or 2) for all percentages
/// below 80%", because a node that loses an active view member promotes a
/// passive one immediately (4.3). No shuffle and no maintenance pass runs after
/// the failure here, so only that promotion can heal the overlay.
///
/// Whole means that no active view holds a failed peer, that the overlay is
/// connected, and that every survivor has at least two neighbors, as in a
/// healthy swarm. A broadcast at that moment must reach every survivor.
#[test]
fn hyparview_heals_without_waiting_for_a_shuffle() {
    let peers = swarm_size(PEERS);
    /// One-way link latencies allowed until the overlay is whole again.
    ///
    /// Measured over 50 seeds: at most 5 after 10 percent failure, at most 10
    /// after 50 percent.
    const MAX_LATENCIES: usize = 20;

    for fraction in [10, 50] {
        for seed in seeds() {
            let mut sim = swarm(peers, seed);
            let before: BTreeMap<u64, Vec<u64>> = sim
                .network
                .peer_ids()
                .map(|peer| (peer, neighbors(&sim, peer)))
                .collect();
            sim.remove_peers(peers * fraction / 100);
            sim.network.stop_membership_cycles();
            let survivors: BTreeSet<u64> = sim.network.peer_ids().collect();
            let damaged = survivors
                .iter()
                .filter(|peer| before[*peer].iter().any(|n| !survivors.contains(n)))
                .count();
            assert!(
                damaged > 0,
                "seed {seed}, {fraction}% failure: no survivor lost a neighbor"
            );

            let latencies = latencies_until_whole(&mut sim, MAX_LATENCIES);
            report(
                &format!("hyparview: healing after {fraction}%"),
                seed,
                format!("{damaged} damaged peers whole again after {latencies} latencies"),
            );
            assert!(
                latencies < MAX_LATENCIES,
                "seed {seed}, {fraction}% failure: overlay still broken after {MAX_LATENCIES} latencies"
            );
            assert_eq!(
                broadcast(&mut sim).missed,
                0.0,
                "seed {seed}, {fraction}% failure: a broadcast at the healed moment missed peers"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Plumtree: stable swarm
// ---------------------------------------------------------------------------

/// Claim: redundancy drops to zero after two rounds, and those two cost no more
/// than flooding.
///
/// Plumtree 4.2: Plumtree "generate[s] a value of 0 (for most messages)"
/// (Figure 2a). During tree construction "for the 2 first cycles of simulation
/// the Plumtree protocols generate more redundant messages [but] always below
/// the number of redundant messages produced by the eager strategy" (Figure
/// 2b). The paper's eager strategy gossips to a random fanout. Here the
/// control floods the same overlay instead, which the first broadcast also
/// does. So the first round can only match it.
///
/// The paper's own count, 9999 payload messages for 10,000 nodes (Table 1), is
/// RMR 0: one payload per recipient, a spanning tree.
#[test]
fn plumtree_redundancy_drops_to_zero_after_two_rounds() {
    let peers = swarm_size(PEERS);
    const ROUNDS: usize = 20;

    for seed in seeds() {
        let mut sim = swarm(peers, seed);
        let sender = sim.random_peer();
        let flooding = flooding_rmr(&sim, sender);
        for _ in 0..ROUNDS {
            assert_eq!(broadcast_from(&mut sim, sender).missed, 0.0);
        }

        let rmr: Vec<f32> = sim.round_stats().iter().map(|round| round.rmr).collect();
        let mut settled = rmr[2..].to_vec();
        settled.sort_by(f32::total_cmp);
        let median = settled[settled.len() / 2];
        let mean = settled.iter().sum::<f32>() / settled.len() as f32;
        report(
            "plumtree: RMR",
            seed,
            format!(
                "flooding {flooding:.2}; rounds 1 and 2 {:.2}, {:.2}; \
                 from round 3 median {median:.3}, mean {mean:.3}",
                rmr[0], rmr[1]
            ),
        );

        // The first broadcast floods every link while it builds the tree, so it
        // costs what flooding costs. Measured equal on 49 of 50 seeds, and 0.01
        // higher on one.
        assert!(
            rmr[0] <= flooding + 0.02 && rmr[1] < flooding,
            "seed {seed}: building the tree cost more than flooding ({flooding:.2})"
        );
        // An RMR of exactly 0 means one payload per recipient: the eager links
        // form a spanning tree. Measured median 0 on all of 50 seeds, and mean
        // at most 0.002. A tree that keeps being grafted and pruned sits at 0.1
        // and up.
        assert_eq!(
            median, 0.0,
            "seed {seed}: median RMR {median:.3} after tree construction is not zero"
        );
        assert!(
            mean < 0.02,
            "seed {seed}: mean RMR {mean:.3} after tree construction is not near zero"
        );
    }
}

/// Claim: a single sender's tree is as fast as flooding.
///
/// Plumtree 4.2, Figure 2c: "The eager protocol and Plumtree with a single
/// sender present the best performance", because Plumtree keeps the links that
/// deliver first. Flooding delivers each message along the fastest path in the
/// overlay. The control computes the hops of those paths from the active views
/// and link latencies, apart from any broadcast. The settled tree must be at
/// most one hop deeper.
#[test]
fn plumtree_single_sender_tree_is_as_fast_as_flooding() {
    let peers = swarm_size(PEERS);
    for seed in seeds() {
        let mut sim = swarm(peers, seed);
        let sender = sim.random_peer();
        for _ in 0..15 {
            assert_eq!(broadcast_from(&mut sim, sender).missed, 0.0);
        }
        let rounds = sim.round_stats();
        let flooding = flooding_ldh(&sim, sender) as f32;
        let settled = rounds[5..]
            .iter()
            .map(|round| round.ldh)
            .fold(0.0, f32::max);
        report(
            "plumtree: tree depth",
            seed,
            format!("LDH {flooding} flooding, at most {settled} settled"),
        );
        // Measured equal on 49 of 50 seeds, and one hop deeper on one.
        assert!(
            settled <= flooding + 1.0,
            "seed {seed}: the settled tree delivers in {settled} hops, flooding in {flooding}"
        );
    }
}

/// Extension: the broadcast tree stays shallow as the swarm grows.
///
/// Plumtree 4.2 reports a last delivery hop close to flooding's at one swarm
/// size. Flooding over a random overlay takes a number of hops that grows with
/// the logarithm of the swarm, so the tree should too. The papers measure no
/// such scaling. This asserts a chosen bound of twice log2 of the swarm size,
/// at every size from 50 to 400 nodes.
#[test]
fn plumtree_last_delivery_hop_stays_logarithmic() {
    for seed in seeds() {
        let mut measured = Vec::new();
        let largest = swarm_size(400);
        for peers in [largest / 8, largest / 4, largest / 2, largest] {
            let mut sim = swarm(peers, seed);
            let sender = sim.random_peer();
            for _ in 0..15 {
                assert_eq!(broadcast_from(&mut sim, sender).missed, 0.0);
            }
            let ldh = sim.round_stats()[5..]
                .iter()
                .map(|round| round.ldh)
                .sum::<f32>()
                / (sim.round_stats().len() - 5) as f32;
            measured.push((peers, ldh));
        }

        report(
            "plumtree: LDH",
            seed,
            measured
                .iter()
                .map(|(peers, ldh)| {
                    format!(
                        "n={peers}: {ldh:.1} ({:.2}x log2 n)",
                        ldh / (*peers as f32).log2()
                    )
                })
                .collect::<Vec<_>>()
                .join("  "),
        );

        // Measured at most 1.42 times log2(n) over 50 seeds. Two times leaves
        // room while still failing if LDH starts tracking n instead.
        for (peers, ldh) in &measured {
            let bound = 2.0 * (*peers as f32).log2();
            assert!(
                *ldh < bound,
                "seed {seed}: LDH {ldh:.1} at n={peers} exceeds 2*log2(n) = {bound:.1}, \
                 which is no longer logarithmic growth"
            );
        }
    }
}

/// Claim: a tree shared by many senders carries no redundant payload, at about
/// twice the depth of a single sender's.
///
/// Plumtree Table 1 counts payload messages with a different random sender
/// every cycle and gets 9999 for 10,000 nodes, RMR 0, for both the original
/// protocol and the optimized one. 4.5 puts the latency cost of sharing one
/// tree at "twice that value".
///
/// The paper's simulator is cycle driven: a graft only fires once the eager
/// flood has finished, so it can never fire early. With a different source
/// every round, the tree path from the new source can be far longer than a lazy
/// shortcut to it, and a timeout that expires first grafts a peer that was
/// about to deliver anyway. This test therefore waits 3s, long enough to span
/// the whole tree, and gives each round up to 30s. That is close to the
/// paper's setting, but a graft can still fire while the eager copy travels.
/// The test does not cover the default timeout.
#[test]
fn plumtree_shared_tree_has_no_redundancy() {
    let peers = swarm_size(PEERS);
    const ROUNDS: usize = 40;

    for seed in seeds() {
        let mut shared = swarm_from(
            SimulatorConfig {
                rng_seed: seed,
                peers,
                gossip_round_timeout: Duration::from_secs(30),
            },
            fixed_graft_timeout(Duration::from_secs(3)),
        );
        for _ in 0..ROUNDS {
            assert_eq!(broadcast(&mut shared).missed, 0.0);
        }
        let settled = &shared.round_stats()[ROUNDS / 2..];
        let rmr = mean_rmr(settled);
        let shared_ldh = mean_ldh(settled);

        let mut single = swarm(peers, seed);
        let sender = single.random_peer();
        for _ in 0..15 {
            assert_eq!(broadcast_from(&mut single, sender).missed, 0.0);
        }
        let single_ldh = mean_ldh(&single.round_stats()[5..]);

        report(
            "plumtree: shared tree",
            seed,
            format!(
                "RMR {rmr:.3}, LDH {shared_ldh:.1} shared against {single_ldh:.1} single \
                 ({:.2}x)",
                shared_ldh / single_ldh
            ),
        );

        // Measured 0.001 median and 0.002 at most over 50 seeds. The defect
        // this guards against, a timeout that grafts before the tree delivers,
        // starts at 0.6.
        assert!(
            rmr < 0.1,
            "seed {seed}: a shared tree carries RMR {rmr:.3}, the paper's is 0"
        );
        // Measured at 1.2 to 2.0 times over 50 seeds.
        assert!(
            shared_ldh <= single_ldh * 2.5,
            "seed {seed}: a shared tree is {:.2} times as deep as a single sender's",
            shared_ldh / single_ldh
        );
    }
}

/// Claim: every broadcast reaches every member, with many sources at once.
///
/// Plumtree 3: lazy push plus IHave and Graft repair is what lets the protocol
/// prune the overlay down to a tree without giving up the reliability that
/// makes flooding attractive in the first place. The paper evaluates one sender
/// per cycle. This runs ten distinct senders at once, over the one tree that
/// this crate keeps per topic.
#[test]
fn plumtree_delivery_is_total_with_concurrent_senders() {
    let peers = swarm_size(PEERS);
    const SENDERS: usize = 10;
    const ROUNDS: usize = 10;

    for seed in seeds() {
        let mut sim = swarm(peers, seed);
        for round in 0..ROUNDS {
            let mut senders = BTreeSet::new();
            while senders.len() < SENDERS {
                senders.insert(sim.random_peer());
            }
            let messages = senders
                .into_iter()
                .enumerate()
                .map(|(idx, sender)| (sender, format!("{round}:{idx}").into_bytes().into()))
                .collect();
            let missed = sim.gossip_round(messages);
            sim.network.check_invariants().unwrap();
            assert_eq!(
                missed, 0,
                "seed {seed}, round {round}: {missed} deliveries missing"
            );
        }
        report(
            "plumtree: reliability",
            seed,
            format!("100.00% over {ROUNDS} rounds of {SENDERS} senders"),
        );
    }
}

// ---------------------------------------------------------------------------
// Plumtree: failures
// ---------------------------------------------------------------------------

/// Claim: delivery stays total under a constant rate of failures.
///
/// Plumtree 4.3 fails 50 of 10,000 nodes in each of 100 cycles, each failure
/// step followed directly by a broadcast, and reports "a constant reliability
/// of 100%" (Figure 3a). This fails the same half percent per round, with no
/// time to heal before the broadcast.
#[test]
fn plumtree_delivery_holds_under_a_constant_failure_rate() {
    let peers = swarm_size(400);
    const ROUNDS: usize = 100;
    let per_round = peers / 200;

    for seed in seeds() {
        let mut sim = swarm(peers, seed);
        let mut worst: f32 = 100.0;
        for round in 0..ROUNDS {
            sim.remove_peers(per_round);
            let stats = broadcast(&mut sim);
            worst = worst.min(reliability(&sim, &stats));
            assert_eq!(
                stats.missed, 0.0,
                "seed {seed}, round {round}: survivors missed the broadcast"
            );
        }
        report(
            "plumtree: constant failure",
            seed,
            format!(
                "{ROUNDS} rounds, {} survivors, worst reliability {worst:.2}%",
                sim.peer_count()
            ),
        );
    }
}

/// Claim: after a massive failure the tree repairs itself, and redundancy is
/// back to its old level within a couple of broadcasts.
///
/// Plumtree 4.4 fails 40, 60, and 80 percent of the nodes. "All protocols are
/// able to regain their RMR levels before failures in only a couple of cycles"
/// (Figure 5), and the last delivery hop stays "somewhat constant" (Figure 4).
///
/// The experiment keeps one source, and so does this test. This crate keeps
/// one tree per topic for all its senders. No shuffle and no maintenance pass
/// runs after the failure, and the first broadcast follows at once. The test
/// records the first broadcast that reaches every survivor, and the first
/// whose RMR is back near zero.
///
/// At the default graft timeout of 80ms, a repaired tree keeps some
/// redundancy, because a lazy path can beat the remaining eager path of
/// several links. The test uses 400ms, and
/// `plumtree_graft_timeout_below_the_eager_path_costs_redundancy` pins down why.
#[test]
fn plumtree_recovers_from_massive_failure() {
    let peers = swarm_size(400);
    const ROUNDS: usize = 15;
    /// Broadcasts until one reaches every survivor. Measured over 50 seeds: the
    /// first or the second. The first can reach nobody when its sender lost
    /// every neighbor.
    const MAX_FIRST_FULL: usize = 2;
    /// Broadcasts until the RMR is back near zero. Measured over 50 seeds: the
    /// third on most, the sixth at most. A tree that never settles stays above.
    const MAX_FIRST_SETTLED: usize = 8;

    for fraction in [40, 60, 80] {
        for seed in seeds() {
            let mut sim = swarm_with(peers, seed, fixed_graft_timeout(Duration::from_millis(400)));
            let sender = sim.random_peer();
            for _ in 0..15 {
                assert_eq!(broadcast_from(&mut sim, sender).missed, 0.0);
            }
            let rounds = sim.round_stats();
            let before = mean_rmr(&rounds[5..]);
            let ldh_before = rounds[rounds.len() - 1].ldh;

            remove_peers_except(&mut sim, peers * fraction / 100, sender);
            sim.network.stop_membership_cycles();

            let mut first_full = None;
            let mut first_settled = None;
            let mut first = None;
            for idx in 0..ROUNDS {
                let stats = broadcast_from(&mut sim, sender);
                first.get_or_insert(stats.clone());
                match first_full {
                    None if stats.missed == 0.0 => first_full = Some(idx),
                    None => {}
                    Some(_) => assert_eq!(
                        stats.missed, 0.0,
                        "seed {seed}, {fraction}% failure: broadcast {} missed peers after a full one",
                        idx + 1
                    ),
                }
                if first_full.is_some() && first_settled.is_none() && stats.rmr < 0.05 {
                    first_settled = Some(idx);
                }
            }
            let first = first.expect("at least one broadcast");
            let ldh_after = sim.round_stats().last().expect("a round").ldh;
            report(
                &format!("plumtree: {fraction}% failure"),
                seed,
                format!(
                    "RMR {before:.3} before, first broadcast reaches {:.1}% at RMR {:.3}; \
                     full from broadcast {:?}, RMR settled from {:?}; LDH {ldh_before} before, \
                     {ldh_after} after",
                    reliability(&sim, &first),
                    first.rmr,
                    first_full.map(|idx| idx + 1),
                    first_settled.map(|idx| idx + 1),
                ),
            );

            // The defect this guards against, a tree grafted and pruned every
            // round, starts at 0.1.
            assert!(
                before < 0.05,
                "seed {seed}: the tree was not settled before the failure, RMR {before:.3}"
            );
            let first_full = first_full.unwrap_or_else(|| {
                panic!("seed {seed}, {fraction}% failure: no broadcast reached every survivor")
            });
            let first_settled = first_settled.unwrap_or_else(|| {
                panic!("seed {seed}, {fraction}% failure: RMR never came back near zero")
            });
            assert!(
                first_full < MAX_FIRST_FULL && first_settled < MAX_FIRST_SETTLED,
                "seed {seed}, {fraction}% failure: full from broadcast {}, settled from {}",
                first_full + 1,
                first_settled + 1
            );
            // Fewer nodes make a shallower tree. A deeper one means the repair
            // left long detours behind. Measured over 50 seeds: at most 5 hops
            // deeper, without shuffles to find better links.
            assert!(
                ldh_after <= ldh_before + 6.0,
                "seed {seed}, {fraction}% failure: LDH went from {ldh_before} to {ldh_after}"
            );
        }
    }
}

/// Claim: the graft timeout has to account for the remaining eager path.
///
/// Plumtree 3.5 makes this a deployment parameter: "The timeout value is a
/// protocol parameter that should be configured considering the diameter of the
/// overlay and a target maximum recovery latency" (p. 8). An `IHave` can arrive
/// over a short lazy path while the eager copy still travels over several
/// links. A timeout below the rest of that eager path grafts a lazy peer the
/// node was about to hear from anyway, and the duplicate that follows prunes
/// the link straight back. The pair repeats every round, so redundancy never
/// returns to where a healthy tree would put it.
///
/// This pins the relationship rather than any particular default, on links of
/// 50ms each way. Both timeouts exceed one link. The depth gaps that the tree
/// optimizer leaves alone reach seven hops, 350ms, so 80ms falls short of them
/// and 400ms clears them. That figure holds for this implementation and this
/// overlay, not in general.
#[test]
fn plumtree_graft_timeout_below_the_eager_path_costs_redundancy() {
    let mut costs = Vec::new();
    for seed in seeds() {
        let impatient = repaired_tree_rmr(seed, Duration::from_millis(80));
        let patient = repaired_tree_rmr(seed, Duration::from_millis(400));
        report(
            "plumtree: graft timeout",
            seed,
            format!("RMR {impatient:.3} at 80ms, {patient:.3} at 400ms"),
        );

        // Measured at most 0.003 over 50 seeds, and 0 on 40.
        assert!(
            patient < 0.02,
            "seed {seed}: a graft timeout above the eager path should leave a clean \
             tree, got RMR {patient:.3}"
        );
        costs.push(impatient);
    }
    // How much depends on how deep the gaps in a particular tree run, from
    // little on a tree without deep gaps to much on one with many, so this can
    // only hold across seeds. Measured 0.05 to 0.64 over 50 seeds, 0.25 on
    // average.
    let mean = costs.iter().sum::<f32>() / costs.len() as f32;
    assert!(
        mean > 0.1,
        "a graft timeout below the eager path cost only RMR {mean:.3} on average"
    );
}

// ---------------------------------------------------------------------------
// Configuration reach
// ---------------------------------------------------------------------------

/// Every protocol parameter is settable from outside the crate.
///
/// `optimization_threshold` and the three random walk lengths have types that
/// lived in private modules, so code downstream could not name them.
#[test]
fn every_tuning_knob_is_reachable() {
    let peers = swarm_size(PEERS);
    use iroh_gossip::proto::{Round, Ttl};

    let mut config = Config::default();

    config.membership.active_view_capacity = 6;
    config.membership.passive_view_capacity = 40;
    config.membership.active_random_walk_length = Ttl(5);
    config.membership.passive_random_walk_length = Ttl(2);
    config.membership.shuffle_random_walk_length = Ttl(5);
    config.membership.shuffle_active_view_count = 2;
    config.membership.shuffle_passive_view_count = 3;
    config.membership.shuffle_interval = Duration::from_secs(30);
    config.membership.neighbor_request_timeout = Duration::from_secs(1);

    config.broadcast.graft_timeout_1 = Duration::from_millis(300);
    config.broadcast.graft_timeout_2 = Duration::from_millis(100);
    config.broadcast.dispatch_timeout = Duration::from_millis(10);
    config.broadcast.optimization_threshold = Round::from(3u16);
    config.broadcast.message_cache_retention = Duration::from_secs(60);
    config.broadcast.message_id_retention = Duration::from_secs(120);
    config.broadcast.cache_evict_interval = Duration::from_secs(2);

    config.max_message_size = 8192;

    // The swarm has to still work with all of it applied.
    let mut sim = swarm_with(peers, 0, config);
    assert_eq!(broadcast(&mut sim).missed, 0.0);
}

// ---------------------------------------------------------------------------
// Measurement
// ---------------------------------------------------------------------------

/// Builds a swarm of `peers` nodes whose links all take 50ms one way.
fn static_swarm(peers: usize) -> Simulator {
    let network = NetworkConfig {
        proto: Config::default(),
        latency: LatencyConfig::Static(Duration::from_millis(50)),
    };
    swarm_with(peers, 0, network)
}

/// Returns the payload messages that all peers received so far.
fn payloads(sim: &Simulator) -> u64 {
    sim.network
        .peer_states()
        .map(|peer| {
            peer.state(&TOPIC)
                .expect("peer joined the topic")
                .gossip_stats()
                .payload_messages_received
        })
        .sum()
}

/// A round counts every payload of its broadcast, also those after the last delivery.
///
/// In a triangle, the two recipients forward the message to each other after
/// both delivered it. Plumtree's evaluation counts a broadcast until no message
/// of it travels anymore (4).
#[test]
fn broadcast_statistics_include_trailing_payloads() {
    let mut sim = static_swarm(3);
    assert_eq!(sim.gossip_round(vec![(0, "triangle".into())]), 0);
    let at_delivery = payloads(&sim);
    sim.network.run_duration(Duration::from_millis(500));
    assert_eq!(
        at_delivery,
        payloads(&sim),
        "the round left out payloads that arrived after the last delivery"
    );
}

/// One payload to one recipient has no redundancy.
///
/// Plumtree 2.3 defines RMR as `m / (n - 1) - 1`, where `n` counts the
/// source among the nodes that have the message. The denominator is the
/// number of recipients.
#[test]
fn one_payload_to_one_recipient_has_zero_rmr() {
    let mut sim = static_swarm(2);
    assert_eq!(sim.gossip_round(vec![(0, "one".into())]), 0);
    assert_eq!(payloads(&sim), 1);
    assert_eq!(sim.round_stats()[0].rmr, 0.0);
}

/// A message that crossed one link has a last delivery hop of 1.
///
/// Plumtree 2.3 counts the hops a message takes to its last recipient.
#[test]
fn last_delivery_hop_counts_the_first_link() {
    let mut sim = static_swarm(2);
    assert_eq!(sim.gossip_round(vec![(0, "one-hop".into())]), 0);
    assert_eq!(sim.round_stats()[0].ldh, 1.0);
}

/// A round's RMR matches its payloads, counted apart from the simulator's report.
///
/// Every round of a single sender delivers to all 199 other peers, so the RMR
/// is the round's payloads divided by 199, minus 1. A settled tree carries
/// exactly one payload per recipient on most rounds.
#[test]
fn rmr_matches_the_payloads_of_the_round() {
    let mut sim = static_swarm(200);
    let sender = sim.random_peer();
    let mut exact = 0;
    for round in 0..20 {
        assert_eq!(
            sim.gossip_round(vec![(sender, format!("m{round}").into())]),
            0
        );
        let payloads = payloads(&sim);
        let rmr = payloads as f32 / 199.0 - 1.0;
        assert_eq!(sim.round_stats()[round].rmr, rmr, "round {round}");
        if payloads == 199 {
            exact += 1;
        }
    }
    assert!(
        exact >= 10,
        "a settled tree should carry one payload per recipient"
    );
}

/// The per-peer invariants hold while control messages and timers are in flight.
///
/// The other tests check them at settled moments only.
#[test]
fn local_invariants_hold_during_failure_and_repair() {
    let mut sim = static_swarm(100);
    sim.network.check_invariants().unwrap();
    assert_eq!(sim.gossip_round(vec![(0, "before".into())]), 0);
    sim.remove_peers(50);
    for _ in 0..200 {
        sim.network.run_duration(Duration::from_millis(50));
        sim.network.check_invariants().unwrap();
    }
    let sender = sim.random_peer();
    assert_eq!(sim.gossip_round(vec![(sender, "after".into())]), 0);
    sim.network.check_invariants().unwrap();
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns the swarm size a test runs at: `default`, unless `PAPER_PEERS` is set.
///
/// The papers simulate 10,000 nodes. `PAPER_PEERS=10000` runs every claim at
/// that size, and a test that varies the size scales its range to end there.
fn swarm_size(default: usize) -> usize {
    std::env::var("PAPER_PEERS")
        .map(|peers| peers.parse().expect("PAPER_PEERS must be a number"))
        .unwrap_or(default)
}

/// Returns the seeds to check every claim against.
///
/// A single seed proves a protocol worked once. These are randomised protocols,
/// so a claim that only holds for seed 0 is not a claim about the protocol.
/// `PAPER_SEEDS` sets how many to run; the default is five.
fn seeds() -> Vec<u64> {
    let count = std::env::var("PAPER_SEEDS")
        .map(|count| count.parse().expect("PAPER_SEEDS must be a number"))
        .unwrap_or(5);
    assert!(count > 0, "PAPER_SEEDS must be greater than zero");
    (0..count).collect()
}

/// Builds a swarm of `peers` nodes on the crate defaults, the way the paper does.
fn swarm(peers: usize, seed: u64) -> Simulator {
    swarm_with(peers, seed, Config::default())
}

/// Builds a swarm of `peers` nodes on `network`, the way the paper does.
fn swarm_with(peers: usize, seed: u64, network: impl Into<NetworkConfig>) -> Simulator {
    let config = SimulatorConfig {
        rng_seed: seed,
        peers,
        ..Default::default()
    };
    swarm_from(config, network)
}

/// How long a swarm runs after its last peer joins, before a test starts.
///
/// The papers let their overlays stabilize before they measure. The newest
/// peers need shuffles to fill their passive views. At 10,000 peers, with the
/// first shuffle 1s after a join, the smallest passive view holds 3 entries
/// right after the last join, 17 after 10s, and 28 after 20s. A peer whose
/// every known address fails is cut off for good, so without this a failure
/// of half the swarm now and then strands one of the newest peers at the
/// papers' size.
///
/// It is a fixed time rather than a number of shuffle intervals, so that a
/// swarm on a longer interval does not get more time to recover.
const WARMUP: Duration = Duration::from_secs(20);

/// Builds a swarm from a full simulator config, the way the paper does, and
/// lets it run for [`WARMUP`].
fn swarm_from(config: SimulatorConfig, network: impl Into<NetworkConfig>) -> Simulator {
    let mut sim = Simulator::new(config, network);
    sim.bootstrap_sequential();
    sim.network.run_duration(WARMUP);
    sim.network.check_invariants().unwrap();
    sim
}

/// Returns a config whose graft timeout is fixed at `timeout`.
fn fixed_graft_timeout(timeout: Duration) -> Config {
    let mut config = Config::default();
    config.broadcast.graft_timeout_1 = timeout;
    config.broadcast.graft_timeout_2 = timeout / 2;
    config
}

/// Builds a tree on 50ms links, fails a quarter of the swarm, and returns the
/// settled RMR.
fn repaired_tree_rmr(seed: u64, graft_timeout: Duration) -> f32 {
    let peers = swarm_size(PEERS);
    let network = NetworkConfig {
        proto: fixed_graft_timeout(graft_timeout),
        latency: LatencyConfig::Static(Duration::from_millis(50)),
    };
    let mut sim = swarm_with(peers, seed, network);
    let sender = sim.random_peer();
    for _ in 0..15 {
        assert_eq!(broadcast_from(&mut sim, sender).missed, 0.0);
    }
    remove_peers_except(&mut sim, peers / 4, sender);
    heal(&mut sim);
    for _ in 0..20 {
        assert_eq!(broadcast_from(&mut sim, sender).missed, 0.0);
    }
    let rounds = sim.round_stats();
    mean_rmr(&rounds[rounds.len() - 10..])
}

/// Removes `count` peers chosen at random, never `keep`.
fn remove_peers_except(sim: &mut Simulator, count: usize, keep: u64) {
    let mut rng = sim.rng();
    let victims: Vec<u64> = sim
        .network
        .peer_ids()
        .filter(|peer| *peer != keep)
        .sample(&mut rng, count);
    for peer in victims {
        sim.network.remove(&peer);
    }
}

/// Runs `cycles` membership cycles, the paper's unit of healing time.
///
/// A cycle is one shuffle interval, the period of the membership protocol's
/// cyclic behavior (HyParView 4.4).
fn run_membership_cycles(sim: &mut Simulator, cycles: u32) {
    let cycle = Config::default().membership.shuffle_interval;
    sim.network.run_duration(cycle * cycles);
    sim.network.check_invariants().unwrap();
}

/// Runs the simulation long enough for the membership layer to settle.
fn heal(sim: &mut Simulator) {
    sim.network.run_trips(50);
}

/// Runs one-way link latencies until the overlay is whole, and returns how many it took.
///
/// Whole means that no active view holds a peer that is gone, that the overlay
/// is connected, and that every peer has at least two neighbors. Stops at
/// `max` if it never gets there.
fn latencies_until_whole(sim: &mut Simulator, max: usize) -> usize {
    let whole = |sim: &Simulator| {
        let alive: BTreeSet<u64> = sim.network.peer_ids().collect();
        alive.iter().all(|peer| {
            let neighbors = neighbors(sim, *peer);
            neighbors.len() >= 2 && neighbors.iter().all(|n| alive.contains(n))
        }) && strongly_connected(&adjacency(sim))
    };
    let mut latencies = 0;
    while latencies < max && !whole(sim) {
        sim.network.run_trips(1);
        latencies += 1;
    }
    latencies
}

/// Broadcasts one message from a random peer and returns that round's stats.
fn broadcast(sim: &mut Simulator) -> RoundStats {
    let sender = sim.random_peer();
    broadcast_from(sim, sender)
}

/// Broadcasts one message from `sender` and returns that round's stats.
fn broadcast_from(sim: &mut Simulator, sender: u64) -> RoundStats {
    let round = sim.round_stats().len();
    let message = format!("m{round}").into_bytes().into();
    sim.gossip_round(vec![(sender, message)]);
    sim.network.check_invariants().unwrap();
    sim.round_stats()[round].clone()
}

/// Returns the percentage of intended recipients that got the message.
fn reliability(sim: &Simulator, stats: &RoundStats) -> f32 {
    let expected = sim.peer_count() as f32 - 1.0;
    (expected - stats.missed) / expected * 100.0
}

/// Returns the mean RMR across `rounds`.
fn mean_rmr(rounds: &[RoundStats]) -> f32 {
    rounds.iter().map(|round| round.rmr).sum::<f32>() / rounds.len() as f32
}

/// Returns the mean last delivery hop across `rounds`.
fn mean_ldh(rounds: &[RoundStats]) -> f32 {
    rounds.iter().map(|round| round.ldh).sum::<f32>() / rounds.len() as f32
}

/// Returns each peer's active view size.
fn active_view_sizes(sim: &Simulator) -> Vec<usize> {
    sim.network
        .peer_ids()
        .map(|peer| neighbors(sim, peer).len())
        .collect()
}

/// Returns the first copy each peer gets when `sender` floods a message.
///
/// Flooding sends over every active link, so each peer first receives a
/// message along its fastest path. This finds those paths by link latency, and
/// among equally fast ones takes the one with the fewest hops. Each peer maps to
/// its hops and the peer it got the first copy from.
fn flooding_paths(sim: &Simulator, sender: u64) -> HashMap<u64, (usize, Option<u64>)> {
    let adjacency = adjacency(sim);
    let mut best: HashMap<u64, (Duration, usize, Option<u64>)> =
        HashMap::from([(sender, (Duration::ZERO, 0, None))]);
    let mut queue = BTreeSet::from([(Duration::ZERO, 0usize, sender)]);
    while let Some((time, hops, peer)) = queue.pop_first() {
        if best.get(&peer).map(|(t, h, _)| (*t, *h)) != Some((time, hops)) {
            continue;
        }
        for next in &adjacency[&peer] {
            let latency = sim
                .network
                .link_latency(peer, *next)
                .expect("active links carried messages");
            let candidate = (time + latency, hops + 1);
            if best.get(next).is_none_or(|(t, h, _)| candidate < (*t, *h)) {
                best.insert(*next, (candidate.0, candidate.1, Some(peer)));
                queue.insert((candidate.0, candidate.1, *next));
            }
        }
    }
    assert_eq!(best.len(), adjacency.len(), "flooding reaches every peer");
    best.into_iter()
        .map(|(peer, (_, hops, parent))| (peer, (hops, parent)))
        .collect()
}

/// Returns the last delivery hop that flooding from `sender` has.
fn flooding_ldh(sim: &Simulator, sender: u64) -> usize {
    flooding_paths(sim, sender)
        .values()
        .map(|(hops, _)| *hops)
        .max()
        .unwrap_or(0)
}

/// Returns the RMR that flooding from `sender` has.
///
/// Each peer forwards its first copy to every neighbor except the one it got
/// it from.
fn flooding_rmr(sim: &Simulator, sender: u64) -> f32 {
    let payloads: usize = flooding_paths(sim, sender)
        .into_iter()
        .map(|(peer, (_, parent))| {
            neighbors(sim, peer)
                .into_iter()
                .filter(|neighbor| Some(*neighbor) != parent)
                .count()
        })
        .sum();
    payloads as f32 / (sim.peer_count() - 1) as f32 - 1.0
}

/// Returns the active view of every peer.
fn adjacency(sim: &Simulator) -> BTreeMap<u64, Vec<u64>> {
    sim.network
        .peer_ids()
        .map(|peer| (peer, neighbors(sim, peer)))
        .collect()
}

/// Returns the components of the overlay, with each link taken both ways.
fn components(sim: &Simulator) -> Vec<BTreeSet<u64>> {
    let mut undirected: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
    for (peer, neighbors) in adjacency(sim) {
        undirected.entry(peer).or_default();
        for neighbor in neighbors {
            undirected.entry(peer).or_default().insert(neighbor);
            undirected.entry(neighbor).or_default().insert(peer);
        }
    }
    let mut unvisited: BTreeSet<u64> = undirected.keys().copied().collect();
    let mut components = Vec::new();
    while let Some(start) = unvisited.pop_first() {
        let mut component = BTreeSet::from([start]);
        let mut queue = VecDeque::from([start]);
        while let Some(peer) = queue.pop_front() {
            for neighbor in &undirected[&peer] {
                if unvisited.remove(neighbor) {
                    component.insert(*neighbor);
                    queue.push_back(*neighbor);
                }
            }
        }
        components.push(component);
    }
    components
}

/// Returns whether every peer reaches every other one over active view links.
///
/// The links are directed: a peer that holds another as a neighbor sends to
/// it. A one-way link connects only one way.
fn strongly_connected(adjacency: &BTreeMap<u64, Vec<u64>>) -> bool {
    let Some(&start) = adjacency.keys().next() else {
        return true;
    };
    let reached = |edges: &BTreeMap<u64, Vec<u64>>| {
        let mut seen = BTreeSet::from([start]);
        let mut queue = VecDeque::from([start]);
        while let Some(peer) = queue.pop_front() {
            for next in edges.get(&peer).into_iter().flatten() {
                if seen.insert(*next) {
                    queue.push_back(*next);
                }
            }
        }
        adjacency.keys().all(|peer| seen.contains(peer))
    };
    let mut reverse: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for (peer, neighbors) in adjacency {
        for neighbor in neighbors {
            reverse.entry(*neighbor).or_default().push(*peer);
        }
    }
    reached(adjacency) && reached(&reverse)
}

/// A one-way link does not make a connected overlay.
#[test]
fn one_way_link_is_not_connected() {
    let one_way = BTreeMap::from([(0, vec![1]), (1, vec![])]);
    assert!(!strongly_connected(&one_way));
    let both_ways = BTreeMap::from([(0, vec![1]), (1, vec![0])]);
    assert!(strongly_connected(&both_ways));
}

/// Returns the mean clustering coefficient of the overlay.
///
/// A node's clustering coefficient is the share of its neighbor pairs that
/// are neighbors themselves (HyParView 2.3).
fn clustering_coefficient(sim: &Simulator) -> f64 {
    let peers: Vec<u64> = sim.network.peer_ids().collect();
    let total: f64 = peers
        .iter()
        .map(|peer| {
            let around = neighbors(sim, *peer);
            let pairs = around.len() * around.len().saturating_sub(1) / 2;
            if pairs == 0 {
                return 0.0;
            }
            let linked = around
                .iter()
                .enumerate()
                .flat_map(|(idx, a)| around[idx + 1..].iter().map(move |b| (*a, *b)))
                .filter(|(a, b)| neighbors(sim, *a).contains(b))
                .count();
            linked as f64 / pairs as f64
        })
        .sum();
    total / peers.len() as f64
}

/// Returns the mean shortest path between peers, over active view edges.
fn average_shortest_path(sim: &Simulator) -> f64 {
    // Read every active view once: at the papers' 10,000 nodes this runs
    // 10,000 searches over the whole overlay.
    let adjacency = adjacency(sim);
    let mut total = 0usize;
    let mut pairs = 0usize;
    for &start in adjacency.keys() {
        let mut distance = HashMap::from([(start, 0usize)]);
        let mut queue = VecDeque::from([start]);
        while let Some(peer) = queue.pop_front() {
            let hops = distance[&peer];
            for &neighbor in adjacency.get(&peer).into_iter().flatten() {
                if let Entry::Vacant(entry) = distance.entry(neighbor) {
                    entry.insert(hops + 1);
                    queue.push_back(neighbor);
                }
            }
        }
        assert_eq!(
            distance.len(),
            adjacency.len(),
            "peer {start} cannot reach every member; a partial average would hide a partition"
        );
        total += distance.values().sum::<usize>();
        pairs += distance.len() - 1;
    }
    total as f64 / pairs as f64
}

/// Returns a peer's active view, or an empty view if it has left the swarm.
fn neighbors(sim: &Simulator, peer: u64) -> Vec<u64> {
    sim.network.neighbors(&peer, &TOPIC).unwrap_or_default()
}

/// Prints a measurement so a run doubles as a report against the papers.
fn report(claim: &str, seed: u64, measured: impl AsRef<str>) {
    println!("{claim:<36} seed {seed}: {}", measured.as_ref());
}
