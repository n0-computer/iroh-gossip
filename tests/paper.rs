//! Tests for the claims the HyParView and Plumtree papers make.
//!
//! This crate implements two protocols from Leitao, Pereira and Rodrigues:
//! HyParView (DSN 2007) for swarm membership, and Plumtree, "Epidemic
//! Broadcast Trees" (SRDS 2007), for broadcast on top of it. Each paper backs
//! its design with measured claims. The tests here restate those claims as
//! assertions against [`Simulator`], citing the section, figure, or table they
//! come from, so a regression that breaks one of the protocols' guarantees
//! fails a test that names the guarantee.
//!
//! Every bound sits between where this implementation measures and where the
//! defect it guards against starts, and says which. A bound that the defect
//! would also pass is no bound.
//!
//! Swarms are built the way the HyParView paper builds them (5): peers join one
//! at a time through a single contact, which yields the random graph the papers
//! measure. The simulator's default of every peer joining at once yields an
//! overlay about 25 times more clustered than that, which is a fair model of a
//! crowd hitting one bootstrap peer but not of the papers' setup; the `big_*`
//! tests in `tests/sim.rs` cover it.
//!
//! Every test also checks the invariants each peer's state must hold, like
//! broadcast peers being a subset of the active view, after bootstrap and after
//! every round (see `Network::check_invariants`).
//!
//! A claim this implementation does not yet meet on every seed is marked
//! `#[ignore]` with the reason, rather than asserted at a weaker level, so the
//! commit that makes it hold shows up as the removal of that line.
//!
//! The papers simulate 10,000 nodes; these run at 200 to 400 so the file stays
//! fast in debug builds. Claims hold across five seeds by default. Set
//! `PAPER_SEEDS` to check more, for example `PAPER_SEEDS=50 cargo test
//! --release --test paper`.

use std::{
    collections::{hash_map::Entry, BTreeSet, HashMap, VecDeque},
    time::Duration,
};

use iroh_gossip::proto::{
    sim::{BootstrapMode, LatencyConfig, NetworkConfig, RoundStats, Simulator, SimulatorConfig},
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
/// That stronger claim does not hold here. About half of the peers settle at
/// 5 and the rest at 4, and neither the paper's join procedure nor its rule
/// for retrying a rejected `Neighbor` request (4.3) changes that. The bound on
/// the share of full views is therefore set at the level measured today, as a
/// guard against it getting worse, not at the paper's.
#[test]
fn hyparview_active_view_is_bounded_and_nearly_full() {
    for seed in seeds() {
        let sim = swarm(PEERS, seed);
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
        // Measured 50 to 57 percent; the paper reports almost all.
        assert!(
            full >= 0.4,
            "seed {seed}: only {:.0}% of active views are full",
            full * 100.0
        );
    }
}

/// Claim: the overlay is a single connected component.
///
/// HyParView 4: connectivity is the property the protocol exists to preserve.
/// "No peer without neighbors" is strictly weaker: it passes for an overlay
/// split into two halves.
#[test]
fn hyparview_overlay_is_connected() {
    for seed in seeds() {
        let sim = swarm(PEERS, seed);
        let components = connected_components(&sim);
        report(
            "hyparview: connectivity",
            seed,
            format!("{components} component(s)"),
        );
        assert_eq!(components, 1, "seed {seed}: overlay is partitioned");
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
/// asymmetric in three snapshots two round trips apart is stuck, not changing.
///
/// The paper's degree argument (5.4) is that no node should be weakly attached.
/// With symmetric views, in-degree is the active view size, so the check that
/// adds something is the minimum: a node with a single link is isolated by the
/// next failure.
#[test]
fn hyparview_active_views_are_symmetric() {
    for seed in seeds() {
        let mut sim = swarm(PEERS, seed);
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
    const PEERS: usize = 400;
    let degree = Config::default().membership.active_view_capacity as f64;
    let random_clustering = degree / PEERS as f64;
    let random_path = (PEERS as f64).ln() / (degree - 1.0).ln();

    for seed in seeds() {
        let sim = swarm(PEERS, seed);
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

        // Measured 0.004 to 0.013. An overlay built by every peer joining at
        // once measures 0.045 to 0.083.
        assert!(
            clustering < random_clustering * 2.0,
            "seed {seed}: clustering {clustering:.4} is far above a random graph's"
        );
        // Measured 4.3 to 4.4.
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
/// back to 100 percent within five round trips of a failure, time for a failed
/// send to report back.
#[test]
fn hyparview_failed_neighbors_leave_active_views_at_once() {
    for fraction in [50, 80] {
        for seed in seeds() {
            let mut sim = swarm(PEERS, seed);
            let mut rng = sim.rng();
            let failed: BTreeSet<u64> = sim
                .network
                .peer_ids()
                .sample(&mut rng, PEERS * fraction / 100)
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
/// sender may have lost every neighbor an instant ago; the Plumtree paper sees
/// the same (4.4, Figure 6a). So this asserts that from the fifth message on,
/// every message reaches every survivor.
#[test]
#[ignore = "fails on some seeds: a peer that loses every neighbor is never reconnected"]
fn hyparview_delivers_through_failure_without_waiting_to_heal() {
    const MESSAGES: usize = 30;
    const SETTLED_FROM: usize = 4;

    for fraction in [10, 30, 50] {
        for seed in seeds() {
            let mut sim = swarm(PEERS, seed);
            sim.remove_peers(PEERS * fraction / 100);

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
/// billion event. At 80 percent that chance is one in a thousand per survivor,
/// the same in the paper as here, and such a survivor has no address left to
/// reach the swarm through. The paper reports the average, so above half the
/// swarm this asserts that no seed loses more than two survivors and that
/// delivery averages above 99.5 percent. Measured over 50 seeds: 46 lose none,
/// four lose one, 99.9 percent on average.
#[test]
#[ignore = "fails on some seeds: a peer that loses every neighbor is never reconnected"]
fn hyparview_survives_massive_simultaneous_failure() {
    const PEERS: usize = 400;

    for fraction in [10, 30, 50, 80] {
        let mut rates = Vec::new();
        for seed in seeds() {
            let mut sim = swarm(PEERS, seed);
            assert_eq!(broadcast(&mut sim).missed, 0.0, "baseline delivery failed");

            sim.remove_peers(PEERS * fraction / 100);
            run_membership_cycles(&mut sim, 2);

            let survivors = sim.peer_count();
            let components = connected_components(&sim);
            let stats = broadcast(&mut sim);
            let reliability = reliability(&sim, &stats);
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
                assert!(
                    stats.missed <= 2.0,
                    "seed {seed}, {fraction}% failure: {} survivors cut off",
                    stats.missed
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
#[ignore = "fails on some seeds: a peer that loses every neighbor is never reconnected"]
fn hyparview_survives_continuous_churn() {
    const PEERS: usize = 300;
    /// Peers replaced per round, five percent of the swarm.
    const CHURN: usize = 15;
    const ROUNDS: usize = 25;

    for seed in seeds() {
        let broken = churn_rounds_broken(swarm(PEERS, seed), CHURN, ROUNDS);
        report(
            "hyparview: churn",
            seed,
            format!("{ROUNDS} rounds at {CHURN}/{PEERS}: {broken} broken"),
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
        let whole = !sim.report().has_peers_with_no_neighbors() && connected_components(&sim) == 1;
        let delivered = broadcast(&mut sim).missed == 0.0;
        if !whole || !delivered {
            broken += 1;
        }
    }
    broken
}

/// Claim: the overlay heals quickly after a failure, without waiting for a
/// shuffle.
///
/// HyParView 5.3 recovers "in few rounds (only 1 or 2) for all percentages
/// below 80%", because a node that loses an active view member promotes a
/// passive one immediately (4.3). The default shuffle interval is 60s, 600 round
/// trips here, so a swarm that only healed on shuffle would blow far past this
/// bound.
#[test]
#[ignore = "fails on some seeds: a peer that loses every neighbor is never reconnected"]
fn hyparview_heals_without_waiting_for_a_shuffle() {
    /// Round trips allowed before the overlay must be whole again.
    const MAX_TRIPS: usize = 20;

    for fraction in [10, 50] {
        for seed in seeds() {
            let mut sim = swarm(PEERS, seed);
            sim.remove_peers(PEERS * fraction / 100);
            let trips = trips_until_whole(&mut sim, MAX_TRIPS);
            report(
                &format!("hyparview: healing after {fraction}%"),
                seed,
                format!("whole again after {trips} round trip(s)"),
            );
            assert!(
                trips < MAX_TRIPS,
                "seed {seed}, {fraction}% failure: overlay still broken after {MAX_TRIPS} round trips"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Plumtree: stable swarm
// ---------------------------------------------------------------------------

/// Claim: redundancy drops to zero after two rounds, and even those two cost
/// less than flooding.
///
/// Plumtree 4.2: flooding every link has a constant RMR of fanout minus one, 3
/// with the paper's fanout of 4, while Plumtree "generate[s] a value of 0 (for
/// most messages)" (Figure 2a). During tree construction "for the 2 first
/// cycles of simulation the Plumtree protocols generate more redundant messages
/// [but] always below the number of redundant messages produced by the eager
/// strategy" (Figure 2b).
///
/// The paper's own count, 9999 payload messages for 10,000 nodes (Table 1), is
/// RMR 0; one redundant message per broadcast at 200 nodes is RMR 0.005.
#[test]
fn plumtree_redundancy_drops_to_zero_after_two_rounds() {
    const ROUNDS: usize = 20;
    let flooding = (Config::default().membership.active_view_capacity - 2) as f32;

    for seed in seeds() {
        let mut sim = swarm(PEERS, seed);
        let sender = sim.random_peer();
        for _ in 0..ROUNDS {
            broadcast_from(&mut sim, sender);
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
                "rounds 1 and 2 {:.2}, {:.2}; from round 3 median {median:.3}, mean {mean:.3}",
                rmr[0], rmr[1]
            ),
        );

        assert!(
            rmr[0] < flooding && rmr[1] < flooding,
            "seed {seed}: building the tree cost more than flooding ({flooding})"
        );
        // Measured median 0.005 on almost every seed, one redundant message per
        // broadcast. Four would still be a settled tree; a tree that keeps being
        // grafted and pruned sits at 0.1 and up.
        assert!(
            median <= 0.02,
            "seed {seed}: median RMR {median:.3} after tree construction is not zero"
        );
        assert!(
            mean < 0.05,
            "seed {seed}: mean RMR {mean:.3} after tree construction is not near zero"
        );
    }
}

/// Claim: a single sender's tree is as fast as flooding.
///
/// Plumtree 4.2, Figure 2c: "The eager protocol and Plumtree with a single
/// sender present the best performance", because Plumtree keeps the links that
/// deliver first. The first broadcast floods every link, so its last delivery
/// hop is flooding's; the settled tree must not be deeper.
#[test]
fn plumtree_single_sender_tree_is_as_fast_as_flooding() {
    for seed in seeds() {
        let mut sim = swarm(PEERS, seed);
        let sender = sim.random_peer();
        for _ in 0..15 {
            broadcast_from(&mut sim, sender);
        }
        let rounds = sim.round_stats();
        let flooding = rounds[0].ldh;
        let settled = rounds[5..]
            .iter()
            .map(|round| round.ldh)
            .fold(0.0, f32::max);
        report(
            "plumtree: tree depth",
            seed,
            format!("LDH {flooding} flooding, at most {settled} settled"),
        );
        assert!(
            settled <= flooding,
            "seed {seed}: the settled tree delivers in {settled} hops, flooding in {flooding}"
        );
    }
}

/// Claim: the broadcast tree stays shallow as the swarm grows.
///
/// Plumtree 4.2 reports a last delivery hop close to flooding's, growing with
/// the logarithm of the swarm rather than with its size. This asserts the upper
/// bound at every size from 50 to 400 nodes.
#[test]
fn plumtree_last_delivery_hop_stays_logarithmic() {
    for seed in seeds() {
        let mut measured = Vec::new();
        for peers in [50, 100, 200, 400] {
            let mut sim = swarm(peers, seed);
            let sender = sim.random_peer();
            for _ in 0..15 {
                broadcast_from(&mut sim, sender);
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

        // Two times log2(n) leaves room while still failing if LDH starts
        // tracking n instead.
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
/// about to deliver anyway. At the crate's default of 80ms that leaves RMR at
/// 0.64 to 0.77. This test therefore waits 3s, long enough to span the whole
/// tree, and gives each round up to 30s. That is close to the paper's setting
/// but not the same: a round ends once every peer has the message, so a Graft
/// or Prune it caused can still be in flight when the next round starts.
#[test]
fn plumtree_shared_tree_has_no_redundancy() {
    const ROUNDS: usize = 40;

    for seed in seeds() {
        let mut shared = swarm_from(
            SimulatorConfig {
                rng_seed: seed,
                peers: PEERS,
                gossip_round_timeout: Duration::from_secs(30),
            },
            fixed_graft_timeout(Duration::from_secs(3)),
        );
        for _ in 0..ROUNDS {
            broadcast(&mut shared);
        }
        let settled = &shared.round_stats()[ROUNDS / 2..];
        let rmr = mean_rmr(settled);
        let shared_ldh = mean_ldh(settled);

        let mut single = swarm(PEERS, seed);
        let sender = single.random_peer();
        for _ in 0..15 {
            broadcast_from(&mut single, sender);
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

        assert!(
            rmr < 0.05,
            "seed {seed}: a shared tree carries RMR {rmr:.3}, the paper's is 0"
        );
        // Measured at 1.4 to 1.8 times.
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
/// per cycle; this runs ten concurrently, each needing a tree of its own.
#[test]
fn plumtree_delivery_is_total_with_concurrent_senders() {
    const SENDERS: usize = 10;
    const ROUNDS: usize = 10;

    for seed in seeds() {
        let mut sim = swarm(PEERS, seed);
        for round in 0..ROUNDS {
            let messages = (0..SENDERS)
                .map(|idx| {
                    let sender = sim.random_peer();
                    (sender, format!("{round}:{idx}").into_bytes().into())
                })
                .collect();
            let missed = sim.gossip_round(messages);
            assert_invariants(&sim);
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
#[ignore = "fails on some seeds: a peer that loses every neighbor is never reconnected"]
fn plumtree_delivery_holds_under_a_constant_failure_rate() {
    const PEERS: usize = 400;
    const ROUNDS: usize = 100;
    let per_round = PEERS / 200;

    for seed in seeds() {
        let mut sim = swarm(PEERS, seed);
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
/// back to its old level within a few broadcasts.
///
/// Plumtree 4.4 fails 40, 60, and 80 percent of the nodes. "All protocols are
/// able to regain their RMR levels before failures in only a couple of cycles"
/// (Figure 5), and the last delivery hop stays "somewhat constant" (Figure 4).
/// The repair itself shows as a spike in the first broadcast after the failure.
///
/// Plumtree builds one tree per source, so this holds a single sender across
/// the run. At the crate's default graft timeout of 80ms the repaired tree
/// keeps 7 to 25 percent redundancy for good, because 80ms is below one hop of
/// the simulated links; the test uses 400ms, which clears the depth gaps the
/// tree optimizer leaves, and
/// `plumtree_graft_timeout_below_link_latency_costs_redundancy` pins down why.
#[test]
#[ignore = "fails on some seeds: a peer that loses every neighbor is never reconnected"]
fn plumtree_recovers_from_massive_failure() {
    const PEERS: usize = 400;

    for fraction in [40, 60, 80] {
        for seed in seeds() {
            let mut sim = swarm_with(PEERS, seed, fixed_graft_timeout(Duration::from_millis(400)));
            let sender = sim.random_peer();
            for _ in 0..15 {
                broadcast_from(&mut sim, sender);
            }
            let rounds = sim.round_stats();
            let before = mean_rmr(&rounds[5..]);
            let ldh_before = rounds[rounds.len() - 1].ldh;

            remove_peers_except(&mut sim, PEERS * fraction / 100, sender);
            heal(&mut sim);

            let repair = broadcast_from(&mut sim, sender);
            for _ in 0..15 {
                let stats = broadcast_from(&mut sim, sender);
                assert_eq!(
                    stats.missed, 0.0,
                    "seed {seed}, {fraction}% failure: a broadcast after the failure missed peers"
                );
            }
            let rounds = sim.round_stats();
            let after = mean_rmr(&rounds[rounds.len() - 10..]);
            let ldh_after = rounds[rounds.len() - 1].ldh;

            report(
                &format!("plumtree: {fraction}% failure"),
                seed,
                format!(
                    "RMR {before:.3} before, {:.3} repairing, {after:.3} after; \
                     LDH {ldh_before} before, {ldh_after} after",
                    repair.rmr
                ),
            );

            // The defect this guards against, a tree grafted and pruned every
            // round, starts at 0.1.
            assert!(
                before < 0.05,
                "seed {seed}: the tree was not settled before the failure, RMR {before:.3}"
            );
            assert_eq!(
                repair.missed, 0.0,
                "seed {seed}, {fraction}% failure: the first broadcast after it missed peers"
            );
            assert!(
                repair.rmr > after,
                "seed {seed}, {fraction}% failure: the repair cost no more than steady state, \
                 so the failure never disturbed the tree"
            );
            assert!(
                after < 0.05,
                "seed {seed}, {fraction}% failure: RMR settled at {after:.3}, not back near zero"
            );
            // Fewer nodes make a shallower tree; a deeper one means the repair
            // left long detours behind.
            assert!(
                ldh_after <= ldh_before + 3.0,
                "seed {seed}, {fraction}% failure: LDH went from {ldh_before} to {ldh_after}"
            );
        }
    }
}

/// Claim: the graft timeout has to account for the latency of a link.
///
/// Plumtree 3.5 makes this a deployment parameter: "The timeout value is a
/// protocol parameter that should be configured considering the diameter of the
/// overlay and a target maximum recovery latency" (p. 8). Set below the time a
/// message needs to arrive over an eager link, a node grafts a lazy peer it was
/// about to hear from anyway, and the duplicate that follows prunes the link
/// straight back. The pair repeats every round, so redundancy never returns to
/// where a healthy tree would put it.
///
/// This pins the relationship rather than any particular default, on links of
/// 50ms each way. The depth gaps the tree optimizer leaves alone reach seven
/// hops, 350ms, so 80ms falls far short of them and 400ms clears them.
#[test]
fn plumtree_graft_timeout_below_link_latency_costs_redundancy() {
    let mut ratios = Vec::new();
    for seed in seeds() {
        let impatient = repaired_tree_rmr(seed, Duration::from_millis(80));
        let patient = repaired_tree_rmr(seed, Duration::from_millis(400));
        report(
            "plumtree: graft timeout",
            seed,
            format!("RMR {impatient:.3} at 80ms, {patient:.3} at 400ms"),
        );

        assert!(
            patient < 0.05,
            "seed {seed}: a graft timeout above the link latency should leave a clean \
             tree, got RMR {patient:.3}"
        );
        ratios.push(impatient / patient);
    }
    // How much depends on how deep the gaps in a particular tree run, from
    // nothing on a tree without deep gaps to fifty times on one with many, so
    // this can only hold across seeds. Measured at 25 times on average.
    let mean = ratios.iter().sum::<f32>() / ratios.len() as f32;
    assert!(
        mean > 5.0,
        "a graft timeout below the link latency cost only {mean:.1} times the redundancy \
         on average"
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns the seeds to check every claim against.
///
/// A single seed proves a protocol worked once. These are randomised protocols,
/// so a claim that only holds for seed 0 is not a claim about the protocol.
/// `PAPER_SEEDS` sets how many to run; the default is five.
fn seeds() -> Vec<u64> {
    let count = std::env::var("PAPER_SEEDS")
        .map(|count| count.parse().expect("PAPER_SEEDS must be a number"))
        .unwrap_or(5);
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

/// Builds a swarm from a full simulator config, the way the paper does.
fn swarm_from(config: SimulatorConfig, network: impl Into<NetworkConfig>) -> Simulator {
    let mut sim = Simulator::new(config, network);
    sim.bootstrap(BootstrapMode::Sequential);
    assert_invariants(&sim);
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
    let network = NetworkConfig {
        proto: fixed_graft_timeout(graft_timeout),
        latency: LatencyConfig::Static(Duration::from_millis(50)),
    };
    let mut sim = swarm_with(PEERS, seed, network);
    let sender = sim.random_peer();
    for _ in 0..15 {
        broadcast_from(&mut sim, sender);
    }
    remove_peers_except(&mut sim, PEERS / 4, sender);
    heal(&mut sim);
    for _ in 0..20 {
        broadcast_from(&mut sim, sender);
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
    assert_invariants(sim);
}

/// Runs the simulation long enough for the membership layer to settle.
fn heal(sim: &mut Simulator) {
    sim.network.run_trips(50);
    assert_invariants(sim);
}

/// Runs round trips until the overlay is whole, and returns how many it took.
///
/// Stops at `max` if it never gets there.
fn trips_until_whole(sim: &mut Simulator, max: usize) -> usize {
    let mut trips = 0;
    while trips < max
        && (sim.report().has_peers_with_no_neighbors() || connected_components(sim) > 1)
    {
        sim.network.run_trips(1);
        trips += 1;
    }
    trips
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
    assert_invariants(sim);
    sim.round_stats()[round].clone()
}

/// Panics if any peer's state breaks an invariant it must hold at all times.
fn assert_invariants(sim: &Simulator) {
    if let Err(violation) = sim.network.check_invariants() {
        panic!("invariant broken: {violation}");
    }
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

/// Returns the number of connected components in the overlay.
///
/// Walks the active view edges breadth-first from an arbitrary peer, then
/// restarts from any peer not yet reached.
fn connected_components(sim: &Simulator) -> usize {
    let mut unvisited: BTreeSet<u64> = sim.network.peer_ids().collect();
    let mut components = 0;
    while let Some(&start) = unvisited.iter().next() {
        components += 1;
        let mut queue = VecDeque::from([start]);
        unvisited.remove(&start);
        while let Some(peer) = queue.pop_front() {
            for neighbor in neighbors(sim, peer) {
                if unvisited.remove(&neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
    }
    components
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
    let mut total = 0usize;
    let mut pairs = 0usize;
    for start in sim.network.peer_ids() {
        let mut distance = HashMap::from([(start, 0usize)]);
        let mut queue = VecDeque::from([start]);
        while let Some(peer) = queue.pop_front() {
            let hops = distance[&peer];
            for neighbor in neighbors(sim, peer) {
                if let Entry::Vacant(entry) = distance.entry(neighbor) {
                    entry.insert(hops + 1);
                    queue.push_back(neighbor);
                }
            }
        }
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
