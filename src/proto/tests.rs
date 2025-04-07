//! Simulation framework for testing the protocol implementation

#![allow(missing_docs)]

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
    str::FromStr,
};

use bytes::Bytes;
use n0_future::time::{Duration, Instant};
use rand::{seq::IteratorRandom, Rng};
use rand_chacha::ChaCha12Rng;
use rand_core::SeedableRng;
use serde::{Deserialize, Serialize};
use tracing::{debug, error_span, info, trace, warn};

use super::{
    util::TimerMap, Command, Config, Event, InEvent, OutEvent, PeerIdentity, State, Timer, TopicId,
};
use crate::proto::Scope;

const TICK_DURATION: Duration = Duration::from_millis(10);

/// Test network implementation.
///
/// Stores events in VecDeques and processes on ticks.
/// Timers are checked after each tick. The local time is increased with TICK_DURATION before
/// each tick.
///
/// Note: Panics when sending to an unknown peer.
#[derive(Debug)]
pub struct Network<PI, R> {
    start: Instant,
    time: Instant,
    tick_duration: Duration,
    default_latency: Duration,
    inqueues: BTreeMap<PI, VecDeque<InEvent<PI>>>,
    pub(crate) peers: BTreeMap<PI, State<PI, R>>,
    conns: BTreeSet<ConnId<PI>>,
    events: VecDeque<(PI, TopicId, Event<PI>)>,
    timers: TimerMap<(PI, Timer<PI>)>,
    transport: TimerMap<(PI, InEvent<PI>)>,
    latencies: BTreeMap<ConnId<PI>, Duration>,
    rng: R,
    config: Config,
}

impl<PI, R> Network<PI, R> {
    pub fn new(config: Config, rng: R) -> Self {
        Self::with_opts(config, rng, TICK_DURATION)
    }
    pub fn with_opts(config: Config, rng: R, tick_duration: Duration) -> Self {
        let time = Instant::now();
        Self {
            start: time,
            time,
            tick_duration,
            default_latency: tick_duration,
            inqueues: Default::default(),
            peers: Default::default(),
            conns: Default::default(),
            events: Default::default(),
            timers: TimerMap::new(),
            transport: TimerMap::new(),
            latencies: BTreeMap::new(),
            rng,
            config,
        }
    }
}

fn push_back<PI: Eq + std::hash::Hash + Ord>(
    inqueues: &mut BTreeMap<PI, VecDeque<InEvent<PI>>>,
    peer: &PI,
    event: InEvent<PI>,
) {
    if let Some(q) = inqueues.get_mut(peer) {
        q.push_back(event);
    }
}

impl<PI: PeerIdentity + Ord + std::fmt::Display, R: Rng + SeedableRng + Clone> Network<PI, R> {
    pub fn set_default_latency_in_ticks(&mut self, latency_in_ticks: usize) {
        self.default_latency =
            Duration::from_millis(self.tick_duration.as_millis() as u64 * latency_in_ticks as u64)
    }

    pub fn insert(&mut self, peer_id: PI) {
        let rng = R::from_rng(&mut self.rng).unwrap();
        let state = State::new(peer_id, Default::default(), self.config.clone(), rng);
        self.inqueues.insert(peer_id, VecDeque::new());
        self.peers.insert(peer_id, state);
    }

    pub fn insert_and_join(&mut self, peer_id: PI, topic: TopicId, bootstrap: Vec<PI>) {
        self.insert(peer_id);
        self.command(peer_id, topic, Command::Join(bootstrap));
    }

    pub fn events(&mut self) -> impl Iterator<Item = (PI, TopicId, Event<PI>)> + '_ {
        self.events.drain(..)
    }

    pub fn events_sorted(&mut self) -> Vec<(PI, TopicId, Event<PI>)> {
        sort(self.events().collect())
    }

    pub fn conns(&self) -> Vec<(PI, PI)> {
        sort(self.conns.iter().cloned().map(Into::into).collect())
    }

    pub fn command(&mut self, peer: PI, topic: TopicId, command: Command<PI>) {
        debug!(?peer, "~~ COMMAND {command:?}");
        push_back(&mut self.inqueues, &peer, InEvent::Command(topic, command));
    }

    pub fn peer_states(&self) -> impl Iterator<Item = &State<PI, R>> {
        self.peers.values()
    }

    pub fn peer_ids(&self) -> impl Iterator<Item = PI> + '_ {
        self.peers.keys().cloned()
    }

    pub fn ticks(&mut self, n: usize) {
        (0..n).for_each(|_| self.tick())
    }

    pub fn current_tick(&self) -> u32 {
        ((self.time - self.start) / self.tick_duration.as_millis() as u32).as_millis() as u32
    }

    pub fn tick(&mut self) {
        self.time += self.tick_duration;
        let tick =
            self.time.duration_since(self.start).as_millis() / self.tick_duration.as_millis();

        // process timers
        for (_time, (peer, timer)) in self.timers.drain_until(&self.time) {
            push_back(&mut self.inqueues, &peer, InEvent::TimerExpired(timer));
        }

        // move messages
        for (_time, (peer, event)) in self.transport.drain_until(&self.time) {
            push_back(&mut self.inqueues, &peer, event);
        }

        // process inqueues: let peer handle all incoming events
        let mut messages_sent = 0;
        let mut kill: Vec<ConnId<PI>> = Vec::new();
        for (peer, queue) in self.inqueues.iter_mut() {
            let state = self.peers.get_mut(peer).unwrap();
            let peer = *state.me();
            let span = error_span!("tick", node = %peer, %tick);
            let _guard = span.enter();
            while let Some(event) = queue.pop_front() {
                if let InEvent::RecvMessage(from, _message) = &event {
                    self.conns.insert((*from, peer).into());
                }
                let out = state.handle(event, self.time);
                for event in out {
                    match event {
                        OutEvent::SendMessage(to, message) => {
                            let latency = latency_between(
                                self.default_latency,
                                &mut self.latencies,
                                &peer,
                                &to,
                                &mut self.rng,
                            );
                            self.transport.insert(
                                self.time + latency,
                                (to, InEvent::RecvMessage(peer, message)),
                            );
                            messages_sent += 1;
                        }
                        OutEvent::ScheduleTimer(latency, timer) => {
                            self.timers.insert(self.time + latency, (peer, timer));
                        }
                        OutEvent::DisconnectPeer(to) => {
                            debug!(peer = ?peer, other = ?to, "disconnect");
                            kill.push((peer, to).into());
                        }
                        OutEvent::EmitEvent(topic, event) => {
                            debug!(peer = ?peer, "emit   {event:?}");
                            self.events.push_back((peer, topic, event));
                        }
                        OutEvent::PeerData(_peer, _data) => {}
                    }
                }
            }
        }
        for conn in kill {
            self.kill_connection(conn);
        }
        debug!(
            tick = self.current_tick(),
            "~~ TICK (messages sent: {messages_sent})"
        );
    }

    pub fn kill_connection(&mut self, conn: ConnId<PI>) {
        if self.conns.remove(&conn) {
            let [a, b] = conn.peers();
            let latency = latency_between(
                self.default_latency,
                &mut self.latencies,
                &a,
                &b,
                &mut self.rng,
            );
            let msg = (a, InEvent::PeerDisconnected(b));
            self.transport.insert(self.time + latency, msg);
            let msg = (b, InEvent::PeerDisconnected(a));
            self.transport.insert(self.time + latency, msg);
        }
    }

    pub fn peer(&self, peer: &PI) -> Option<&State<PI, R>> {
        self.peers.get(peer)
    }

    pub fn get_active(&self, peer: &PI, topic: &TopicId) -> Option<Option<Vec<PI>>> {
        let peer = self.peer(peer)?;
        match peer.state(topic) {
            Some(state) => Some(Some(
                state.swarm.active_view.iter().cloned().collect::<Vec<_>>(),
            )),
            None => Some(None),
        }
    }

    pub fn remove(&mut self, peer: &PI) {
        let remove_conns: Vec<_> = self
            .conns
            .iter()
            .filter(|&c| c.peers().contains(peer))
            .cloned()
            .collect();
        for conn in remove_conns.into_iter() {
            self.kill_connection(conn);
        }
        self.peers.remove(peer);
        self.inqueues.remove(peer);
    }

    pub fn check_synchronicity(&self) -> bool {
        let mut ok = true;
        for state in self.peers.values() {
            let peer = *state.me();
            for (topic, state) in state.states() {
                for other in state.swarm.active_view.iter() {
                    let other_state = &self
                        .peers
                        .get(other)
                        .unwrap()
                        .state(topic)
                        .unwrap()
                        .swarm
                        .active_view;
                    if !other_state.contains(&peer) {
                        debug!(node = %peer, other = ?other, "missing active_view peer in other");
                        ok = false;
                    }
                }
                for other in state.gossip.eager_push_peers.iter() {
                    let other_state = &self
                        .peers
                        .get(other)
                        .unwrap()
                        .state(topic)
                        .unwrap()
                        .gossip
                        .eager_push_peers;
                    if !other_state.contains(&peer) {
                        debug!(node = %peer, other = ?other, "missing eager_push peer in other");
                        ok = false;
                    }
                }
            }
        }
        ok
    }
}

impl<R: Rng + Clone> Network<PeerId, R> {
    fn state_stats(&self) -> NetworkStats {
        let mut stats = NetworkStats::default();
        for (id, peer) in self.peers.iter() {
            let state = peer.state(&TOPIC).unwrap();
            add_one(&mut stats.active, state.swarm.active_view.len());
            add_one(&mut stats.passive, state.swarm.passive_view.len());
            add_one(&mut stats.eager, state.gossip.eager_push_peers.len());
            add_one(&mut stats.lazy, state.gossip.lazy_push_peers.len());
            if state.swarm.active_view.is_empty() {
                stats.empty.insert(*id);
                trace!(node=%id, active = ?state.swarm.active_view.iter().collect::<Vec<_>>(), passive=?state.swarm.passive_view.iter().collect::<Vec<_>>(), "active view empty^");
            }
        }
        stats
    }
}

// fn latency_between<PI: PeerIdentity + Ord + PartialOrd, R: Rng>(
//     _default_latency: Duration,
//     latencies: &mut BTreeMap<ConnId<PI>, Duration>,
//     a: &PI,
//     b: &PI,
//     rng: &mut R,
// ) -> Duration {
//     let id: ConnId<PI> = (*a, *b).into();
//     *latencies.entry(id).or_insert_with(|| {
//         let t: u64 = rng.gen_range(5..25);
//         Duration::from_millis(t)
//     })
// }

fn latency_between<PI: PeerIdentity + Ord + PartialOrd, R: Rng>(
    default_latency: Duration,
    _latencies: &mut BTreeMap<ConnId<PI>, Duration>,
    _a: &PI,
    _b: &PI,
    _rng: &mut R,
) -> Duration {
    default_latency
}

pub type PeerId = u64;

#[derive(Debug, Serialize, Deserialize)]
pub struct SimulatorConfig {
    /// Seed for the random number generator used in the nodes
    pub rng_seed: u64,
    /// Number of nodes to create
    pub peers: usize,
    /// Max number of ticks for a gossip round before the round is aborted
    pub round_max_ticks: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum BootstrapMode {
    Static(StaticBootstrap),
    Dynamic(DynamicBootstrap),
}

impl Default for BootstrapMode {
    fn default() -> Self {
        Self::Static(StaticBootstrap::default())
    }
}

impl BootstrapMode {
    pub fn from_env(nodes: usize) -> Self {
        let is_dynamic = read_var("BOOTSTRAP_MODE", "static".to_string());
        match is_dynamic.as_str() {
            "static" => BootstrapMode::default(),
            "dynamic" => BootstrapMode::Dynamic(DynamicBootstrap {
                bootstrap_nodes: read_var("BOOTSTRAP_COUNT", (nodes / 10).max(3)),
                ticks_per_join: 2,
                warmup_ticks: read_var("WARMUP_TICKS", 100),
            }),
            _ => panic!("BOOTSTRAP_MODE must be static or dynamic"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StaticBootstrap {
    random_contacts: usize,
    warmup_ticks: usize,
}

impl Default for StaticBootstrap {
    fn default() -> Self {
        Self {
            random_contacts: 3,
            warmup_ticks: 100,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DynamicBootstrap {
    bootstrap_nodes: usize,
    ticks_per_join: usize,
    warmup_ticks: usize,
}

impl Default for DynamicBootstrap {
    fn default() -> Self {
        Self {
            bootstrap_nodes: 12,
            ticks_per_join: 3,
            warmup_ticks: 100,
        }
    }
}

impl SimulatorConfig {
    pub fn from_env() -> Self {
        let nodes = read_var("NODES", 100);
        Self {
            rng_seed: read_var("SEED", 0),
            peers: nodes,
            round_max_ticks: read_var("ROUND_MAX_TICKS", 200),
        }
    }
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            rng_seed: 0,
            peers: 100,
            round_max_ticks: 200,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RoundStats {
    pub ticks: f32,
    pub rmr: f32,
    pub ldh: f32,
    pub missing_receives: f32,
}

impl fmt::Display for RoundStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RMR {:>6.2} LDH {:>6.2} ticks {:>6.2} missed {:>10.2}",
            self.rmr, self.ldh, self.ticks, self.missing_receives
        )
    }
}

impl RoundStats {
    fn new_max() -> Self {
        Self {
            ticks: f32::MAX,
            rmr: f32::MAX,
            ldh: f32::MAX,
            missing_receives: f32::MAX,
        }
    }

    pub fn merge<'a>(rounds: impl IntoIterator<Item = &'a RoundStats>) -> RoundStats {
        let (len, mut avg) =
            rounds
                .into_iter()
                .fold((0., RoundStats::default()), |(len, mut agg), round| {
                    agg.rmr += round.rmr;
                    agg.ldh += round.ldh;
                    agg.ticks += round.ticks as f32;
                    agg.missing_receives += round.missing_receives as f32;
                    (len + 1., agg)
                });
        avg.rmr /= len;
        avg.ldh /= len;
        avg.ticks /= len;
        avg
    }

    pub fn min<'a>(rounds: impl IntoIterator<Item = &'a RoundStats>) -> RoundStats {
        rounds
            .into_iter()
            .fold(RoundStats::new_max(), |mut agg, round| {
                agg.rmr = agg.rmr.min(round.rmr);
                agg.ldh = agg.ldh.min(round.ldh);
                agg.ticks = agg.ticks.min(round.ticks);
                agg.missing_receives = agg.missing_receives.min(round.missing_receives);
                agg
            })
    }

    pub fn max<'a>(rounds: impl IntoIterator<Item = &'a RoundStats>) -> RoundStats {
        rounds
            .into_iter()
            .fold(RoundStats::default(), |mut agg, round| {
                agg.rmr = agg.rmr.max(round.rmr);
                agg.ldh = agg.ldh.max(round.ldh);
                agg.ticks = agg.ticks.max(round.ticks);
                agg.missing_receives = agg.missing_receives.max(round.missing_receives);
                agg
            })
    }

    pub fn avg<'a>(rounds: &[RoundStats]) -> RoundStatsAvg {
        let min = Self::min(rounds);
        let max = Self::max(rounds);
        let mean = Self::merge(rounds);
        RoundStatsAvg { min, max, mean }
    }

    pub fn diff(&self, other: &Self) -> Self {
        RoundStats {
            ticks: diff_percent(self.ticks, other.ticks),
            rmr: diff_percent(self.rmr, other.rmr),
            ldh: diff_percent(self.ldh, other.ldh),
            missing_receives: diff_percent(self.missing_receives, other.missing_receives),
        }
    }
}

fn diff_percent(a: f32, b: f32) -> f32 {
    if a == 0.0 && b == 0.0 {
        0.0
    } else if b == 0.0 {
        -1.0
    } else if a == 0.0 {
        1.0
    } else {
        (b - a) / a
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RoundStatsAvg {
    pub min: RoundStats,
    pub max: RoundStats,
    pub mean: RoundStats,
}

impl RoundStatsAvg {
    pub fn diff(&self, other: &Self) -> Self {
        Self {
            min: self.min.diff(&other.min),
            max: self.max.diff(&other.max),
            mean: self.mean.diff(&other.mean),
        }
    }
}

#[derive(Debug, Default)]
struct NetworkStats {
    active: BTreeMap<usize, usize>,
    passive: BTreeMap<usize, usize>,
    eager: BTreeMap<usize, usize>,
    lazy: BTreeMap<usize, usize>,
    empty: BTreeSet<PeerId>,
}

fn avg(map: &BTreeMap<usize, usize>) -> f32 {
    let (sum, count) = map
        .iter()
        .fold((0, 0), |(sum, count), (k, v)| (sum + k * v, count + v));
    if count != 0 {
        sum as f32 / count as f32
    } else {
        0.
    }
}
fn min(map: &BTreeMap<usize, usize>) -> usize {
    map.first_key_value().map(|(k, _v)| *k).unwrap_or_default()
}
fn max(map: &BTreeMap<usize, usize>) -> usize {
    map.last_key_value().map(|(k, _v)| *k).unwrap_or_default()
}

impl fmt::Display for NetworkStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "    eager {:?}\n    lazy {:?}\n    active {:?}\n    passive {:?}",
            self.eager, self.lazy, self.active, self.passive
        )
    }
}

#[derive(Debug)]
pub struct TickReport {
    pub min_active_len: usize,
}

pub const TOPIC: TopicId = TopicId::from_bytes([0u8; 32]);

/// A simple simulator for the gossip protocol
#[derive(Debug)]
pub struct Simulator {
    pub config: SimulatorConfig,
    pub network: Network<PeerId, rand_chacha::ChaCha12Rng>,
    pub round_stats: Vec<RoundStats>,
}

impl Simulator {
    pub fn new(simulator_config: SimulatorConfig, protocol_config: Config) -> Self {
        let rng = rand_chacha::ChaCha12Rng::seed_from_u64(simulator_config.rng_seed);
        Self {
            network: Network::new(protocol_config, rng),
            config: simulator_config,
            round_stats: Default::default(),
        }
    }

    pub fn rng(&mut self) -> ChaCha12Rng {
        ChaCha12Rng::from_rng(&mut self.network.rng).unwrap()
    }

    pub fn random_peer(&mut self) -> PeerId {
        *self
            .network
            .peers
            .keys()
            .choose(&mut self.network.rng)
            .unwrap()
    }

    pub fn peer_count(&self) -> usize {
        self.network.peers.len()
    }

    pub fn kill(&mut self, n: usize) {
        for _i in 0..n {
            let key = *self.network.peers.keys().next().unwrap();
            println!("KILL {key}");
            self.network.remove(&key);
        }
    }

    pub fn report_swarm(&mut self) -> TickReport {
        let stats = self.network.state_stats();
        let tick = self.network.current_tick();
        let min_active_len = min(&stats.active);
        let max_active_len = max(&stats.active);
        let avg = avg(&stats.active);
        let len = self.network.peers.len();
        debug!(
            "{tick}: nodes {len} active: avg {avg:2.2} min {min_active_len} max {max_active_len} empty {}",
            stats.empty.len()
        );
        TickReport { min_active_len }
    }

    // fn run_until_all_active(&mut self, limit: usize) -> bool {
    //     info!("run max {limit} ticks or until all nodes active..");
    //     for _i in 0..limit {
    //         self.run_ticks(1);
    //         let report = self.report_swarm();
    //         if report.min_active_len > 0 {
    //             info!("bootstrapped {}", self.network.peers.len());
    //             return true;
    //         }
    //     }
    //     false
    // }

    pub fn bootstrap(&mut self, bootstrap_mode: BootstrapMode) -> bool {
        match bootstrap_mode {
            BootstrapMode::Static(opts) => self.bootstrap_static(opts),
            BootstrapMode::Dynamic(opts) => self.bootstrap_dynamic(opts),
        }
    }

    pub fn bootstrap_static(&mut self, opts: StaticBootstrap) -> bool {
        let node_count = self.config.peers;
        for i in (0..node_count).into_iter() {
            let chunk_size = opts.random_contacts;
            let contacts = self
                .network
                .peers
                .keys()
                .cloned()
                .choose_multiple(&mut self.network.rng, chunk_size);
            self.network.insert_and_join(i as u64, TOPIC, contacts);
        }
        self.warmup(opts.warmup_ticks)
    }

    pub fn bootstrap_static2(&mut self, opts: StaticBootstrap) -> bool {
        self.network.insert_and_join(0, TOPIC, vec![]);
        let node_count = self.config.peers;
        let mut chunk = 0;
        let chunk_size = opts.random_contacts;
        for i in (1..node_count).into_iter() {
            let contact = chunk * chunk_size as u64;
            if i % chunk_size == 0 {
                chunk += 1;
            }
            self.network.insert_and_join(i as u64, TOPIC, vec![contact]);
            // if i % 3 == 0 {
            //     self.run_ticks(4);
            // }
        }

        self.warmup(opts.warmup_ticks)
    }

    fn warmup(&mut self, ticks: usize) -> bool {
        self.drop_events();
        self.run_ticks(ticks);
        self.drop_events();
        info!(tick=%self.network.current_tick(), "warmup complete");
        if !self.network.check_synchronicity() {
            warn!("not all peers have synchronous relations");
        }

        let report = self.report_swarm();
        if report.min_active_len == 0 {
            warn!("failed to keep all nodes active after warmup");
            false
        } else {
            info!("bootstrap complete, all nodes active");
            true
        }
    }

    pub fn bootstrap_dynamic(&mut self, opts: DynamicBootstrap) -> bool {
        let bootstrap_count = opts.bootstrap_nodes;
        self.network.insert_and_join(0, TOPIC, vec![]);
        self.run_ticks(1);
        for i in 1..bootstrap_count {
            self.network.insert_and_join(i as u64, TOPIC, vec![0]);
            self.run_ticks(4);
        }
        info!(tick=%self.network.current_tick(), "created {bootstrap_count}");

        if !self.warmup(opts.warmup_ticks) {
            warn!(
                "failed to activate {bootstrap_count} bootstrap nodes within a limit of {} ticks",
                opts.warmup_ticks
            );
        } else {
            info!("bootstrap nodes actived");
        }

        let node_count = self.config.peers;
        for i in bootstrap_count..node_count {
            let contact = (i % bootstrap_count) as u64;
            self.network.insert_and_join(i as u64, TOPIC, vec![contact]);
            self.run_ticks(opts.ticks_per_join);
        }
        info!(tick=%self.network.current_tick(), "created {node_count}");

        self.warmup(opts.warmup_ticks)
    }

    pub fn run_ticks(&mut self, ticks: usize) {
        for _ in 0..ticks {
            self.network.tick();
        }
    }

    pub fn drop_events(&mut self) {
        let _ = self.network.events();
    }

    pub fn gossip_tick(&mut self, missing: &mut BTreeMap<PeerId, BTreeSet<Bytes>>) {
        self.run_ticks(1);

        for (peer, _topic, event) in self.network.events() {
            let entry = missing.get_mut(&peer).unwrap();
            if let Event::Received(message) = event {
                entry.remove(&message.content);
            }
        }
    }

    pub fn gossip_round(&mut self, messages: Vec<(PeerId, Bytes)>) {
        self.reset_stats();
        let expected_recv_count: usize = messages.len() * (self.network.peers.len() - 1);

        // assemble expected receives.
        let mut missing: BTreeMap<_, _> = self
            .network
            .peers
            .keys()
            .map(|peer| (*peer, BTreeSet::new()))
            .collect();
        for (from, message) in messages.iter().cloned() {
            for (peer, set) in missing.iter_mut() {
                if *peer != from {
                    set.insert(message.clone());
                }
            }
        }
        assert_eq!(
            missing.values().map(|s| s.len()).sum::<usize>(),
            expected_recv_count
        );

        info!(
            tick=%self.network.current_tick(),
            "round {i}: send {len} messages / recv {expected_recv_count} total",
            len = messages.len(),
            i = self.round_stats.len()
        );

        // Send all messages at once
        for (from, message) in messages {
            self.network.command(
                from,
                TOPIC,
                Command::Broadcast(message.clone(), Scope::Swarm),
            );
            for (peer, set) in missing.iter_mut() {
                if *peer != from {
                    set.insert(message.clone());
                }
            }
            // tick once after each sent message
            self.gossip_tick(&mut missing);
        }

        let mut ticks = 0;
        loop {
            ticks += 1;
            let missing_count: usize = missing.values().map(|set| set.len()).sum();

            if missing_count == 0 {
                info!(
                    tick=%self.network.current_tick(),
                    "break: all messages received by all peers"
                );
                break;
            } else if ticks > self.config.round_max_ticks {
                info!(
                    tick=%self.network.current_tick(),
                    "break: max ticks for round exceeded (still missing {missing_count})"
                );
                break;
            }

            self.gossip_tick(&mut missing);
        }
        let missing_count: usize = missing.values().map(|set| set.len()).sum();
        self.report_gossip_round(expected_recv_count, missing_count, ticks);
    }

    fn report_gossip_round(
        &mut self,
        expected_recv_count: usize,
        missing_receives_count: usize,
        ticks: usize,
    ) {
        let ticks = ticks as f32;
        let payloud_msg_count = self.total_payload_messages();
        let ctrl_msg_count = self.total_control_messages();
        let rmr_expected_count = expected_recv_count - missing_receives_count;
        let rmr = (payloud_msg_count as f32 / (rmr_expected_count as f32 - 1.)) - 1.;
        let ldh = self.max_ldh();

        let round_stats = RoundStats {
            ticks,
            rmr,
            ldh: ldh as f32,
            missing_receives: missing_receives_count as f32,
        };
        let network_stats = self.network.state_stats();
        info!(
            "round {}: pay {} ctrl {} {round_stats} \n{network_stats}",
            self.round_stats.len(),
            payloud_msg_count,
            ctrl_msg_count,
        );
        self.round_stats.push(round_stats);
    }

    pub fn report_round_average(&self) -> RoundStats {
        RoundStats::merge(&self.round_stats)
    }

    fn reset_stats(&mut self) {
        for state in self.network.peers.values_mut() {
            let state = state.state_mut(&TOPIC).unwrap();
            state.gossip.stats = Default::default();
        }
    }

    fn max_ldh(&self) -> u16 {
        let mut max = 0;
        for state in self.network.peers.values() {
            let state = state.state(&TOPIC).unwrap();
            let stats = state.gossip.stats();
            max = max.max(stats.max_last_delivery_hop);
        }
        max
    }

    fn total_payload_messages(&self) -> u64 {
        let mut sum = 0;
        for state in self.network.peers.values() {
            let state = state.state(&TOPIC).unwrap();
            let stats = state.gossip.stats();
            sum += stats.payload_messages_received;
        }
        sum
    }

    fn total_control_messages(&self) -> u64 {
        let mut sum = 0;
        for state in self.network.peers.values() {
            let state = state.state(&TOPIC).unwrap();
            let stats = state.gossip.stats();
            sum += stats.control_messages_received;
        }
        sum
    }
}

fn add_one(map: &mut BTreeMap<usize, usize>, key: usize) {
    let entry = map.entry(key).or_default();
    *entry += 1;
}

/// Helper struct for active connections. A sorted tuple.
#[derive(Debug, Clone, PartialOrd, Ord, Eq, PartialEq, Hash)]
pub struct ConnId<PI>([PI; 2]);
impl<PI: Ord + Copy> ConnId<PI> {
    pub fn new(a: PI, b: PI) -> Self {
        let mut conn = [a, b];
        conn.sort();
        Self(conn)
    }
    pub fn peers(&self) -> [PI; 2] {
        self.0
    }
}
impl<PI: Ord + Copy> From<(PI, PI)> for ConnId<PI> {
    fn from((a, b): (PI, PI)) -> Self {
        Self::new(a, b)
    }
}
impl<PI: Copy> From<ConnId<PI>> for (PI, PI) {
    fn from(conn: ConnId<PI>) -> (PI, PI) {
        (conn.0[0], conn.0[1])
    }
}

pub fn sort<T: Ord + Clone>(items: Vec<T>) -> Vec<T> {
    let mut sorted = items;
    sorted.sort();
    sorted
}

pub fn report_round_distribution<PI: PeerIdentity, R: Rng + Clone>(network: &Network<PI, R>) {
    let mut eager_distrib: BTreeMap<usize, usize> = BTreeMap::new();
    let mut lazy_distrib: BTreeMap<usize, usize> = BTreeMap::new();
    let mut active_distrib: BTreeMap<usize, usize> = BTreeMap::new();
    let mut passive_distrib: BTreeMap<usize, usize> = BTreeMap::new();
    let mut payload_recv = 0;
    let mut control_recv = 0;
    for state in network.peers.values() {
        for (_topic, state) in state.states() {
            let stats = state.gossip.stats();
            *eager_distrib
                .entry(state.gossip.eager_push_peers.len())
                .or_default() += 1;
            *lazy_distrib
                .entry(state.gossip.lazy_push_peers.len())
                .or_default() += 1;
            *active_distrib
                .entry(state.swarm.active_view.len())
                .or_default() += 1;
            *passive_distrib
                .entry(state.swarm.passive_view.len())
                .or_default() += 1;
            payload_recv += stats.payload_messages_received;
            control_recv += stats.control_messages_received;
        }
    }
    // eprintln!("distributions {round_distrib:?}");
    eprintln!("payload_recv {payload_recv} control_recv {control_recv}");
    eprintln!("eager_distrib {eager_distrib:?}");
    eprintln!("lazy_distrib {lazy_distrib:?}");
    eprintln!("active_distrib {active_distrib:?}");
    eprintln!("passive_distrib {passive_distrib:?}");
}

fn read_var<T: FromStr<Err: fmt::Display + fmt::Debug>>(name: &str, default: T) -> T {
    std::env::var(name)
        .map(|x| {
            x.parse()
                .unwrap_or_else(|_| panic!("Failed to parse environment variable {name}"))
        })
        .unwrap_or(default)
}
