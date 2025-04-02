//! Simulation framework for testing the protocol implementation

#![allow(unused)]

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
    str::FromStr,
};

use bytes::Bytes;
use n0_future::time::{Duration, Instant};
use rand::Rng;
use rand_chacha::ChaCha12Rng;
use rand_core::SeedableRng;
use tracing::{debug, error_span, info, trace, warn};

use super::{
    util::TimerMap, Command, Config, Event, InEvent, OutEvent, PeerIdentity, State, Timer, TopicId,
};
use crate::proto::Scope;

const TICK_DURATION: Duration = Duration::from_millis(10);
const DEFAULT_LATENCY: Duration = TICK_DURATION.saturating_mul(1);

/// Test network implementation.
///
/// Stores events in VecDeques and processes on ticks.
/// Timers are checked after each tick. The local time is increased with TICK_DURATION before
/// each tick.
///
/// Note: Panics when sending to an unknown peer.
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
    if let Some(q) = inqueues.get_mut(&peer) {
        q.push_back(event);
    }
}

impl<PI: PeerIdentity + Ord + std::fmt::Display, R: Rng + SeedableRng + Clone> Network<PI, R> {
    pub fn set_default_latency_in_ticks(&mut self, latency_in_ticks: usize) {
        self.default_latency =
            Duration::from_millis(self.tick_duration.as_millis() as u64 * latency_in_ticks as u64)
    }

    fn push(&mut self, peer: State<PI, R>) {
        let id = *peer.me();
        self.inqueues.insert(id, VecDeque::new());
        self.peers.insert(id, peer);
    }

    pub fn insert(&mut self, peer_id: PI) {
        let rng = R::from_rng(&mut self.rng).unwrap();
        self.push(State::new(
            peer_id,
            Default::default(),
            self.config.clone(),
            rng,
        ));
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
            let latency = latency_between(self.default_latency, &mut self.latencies, &a, &b);
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
            .cloned()
            .filter(|c| c.peers().contains(peer))
            .collect();
        for conn in remove_conns.into_iter() {
            self.kill_connection(conn);
        }
        self.peers.remove(peer);
        self.inqueues.remove(peer);
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
                trace!(node=%id, active = ?state.swarm.active_view.iter().collect::<Vec<_>>(), passive=?state.swarm.passive_view.iter().collect::<Vec<_>>(), "EMPTY");
            }
            // trace!(active = ?state.swarm.active_view.iter().collect::<Vec<_>>(), passive=?state.swarm.passive_view.iter().collect::<Vec<_>>(), "view");
        }
        stats
    }
}

fn latency_between<PI: PeerIdentity>(
    default_latency: Duration,
    _latencies: &mut BTreeMap<ConnId<PI>, Duration>,
    _a: &PI,
    _b: &PI,
) -> Duration {
    default_latency
}

pub fn assert_synchronous_active<PI: PeerIdentity + Ord + Copy, R: Rng + Clone>(
    network: &Network<PI, R>,
) -> bool {
    for state in network.peers.values() {
        let peer = *state.me();
        for (topic, state) in state.states() {
            for other in state.swarm.active_view.iter() {
                let other_state = &network
                    .peers
                    .get(other)
                    .unwrap()
                    .state(topic)
                    .unwrap()
                    .swarm
                    .active_view;
                if !other_state.contains(&peer) {
                    warn!(peer = ?peer, other = ?other, "missing active_view peer in other");
                    return false;
                }
            }
            for other in state.gossip.eager_push_peers.iter() {
                let other_state = &network
                    .peers
                    .get(other)
                    .unwrap()
                    .state(topic)
                    .unwrap()
                    .gossip
                    .eager_push_peers;
                if !other_state.contains(&peer) {
                    warn!(peer = ?peer, other = ?other, "missing eager_push peer in other");
                    return false;
                }
            }
        }
    }
    true
}

pub type PeerId = u64;

pub struct SimulatorConfig {
    pub seed: u64,
    pub peers_count: usize,
    pub bootstrap_count: usize,
    pub warmup_ticks: usize,
    pub round_max_ticks: usize,
}

impl SimulatorConfig {
    pub fn from_env() -> Self {
        let peers_count = read_var("PEERS", 100);
        let bootstrap_count = read_var("BOOTSTRAP", peers_count / 10);
        Self {
            seed: read_var("SEED", 0),
            peers_count,
            bootstrap_count,
            warmup_ticks: read_var("WARMUP_TICKS", 100),
            round_max_ticks: read_var("ROUND_MAX_TICKS", 200),
        }
    }
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            seed: 0,
            peers_count: 100,
            bootstrap_count: 5,
            warmup_ticks: 100,
            round_max_ticks: 200,
        }
    }
}

#[derive(Debug, Default)]
pub struct RoundStats {
    pub ticks: u32,
    pub rmr: f32,
    pub ldh: f32,
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

impl NetworkStats {
    fn min_active_len(&self) -> usize {
        self.active
            .first_key_value()
            .map(|(k, _v)| *k)
            .unwrap_or_default()
    }
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

pub struct TickReport {
    pub min_active_len: usize,
}

pub const TOPIC: TopicId = TopicId::from_bytes([0u8; 32]);

/// A simple simulator for the gossip protocol
pub struct Simulator {
    config: SimulatorConfig,
    pub network: Network<PeerId, rand_chacha::ChaCha12Rng>,
    round_stats: Vec<RoundStats>,
}

impl Simulator {
    pub fn new(simulator_config: SimulatorConfig, protocol_config: Config) -> Self {
        let rng = rand_chacha::ChaCha12Rng::seed_from_u64(simulator_config.seed);
        Self {
            network: Network::new(protocol_config, rng),
            config: simulator_config,
            round_stats: Default::default(),
        }
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

    fn run_until_all_active(&mut self, limit: usize) -> bool {
        for i in 0..limit {
            self.run_ticks(1);
            let _ = self.network.events();
            let report = self.report_swarm();
            if report.min_active_len > 0 {
                info!("bootstrapped {}", self.network.peers.len());
                return true;
            }
        }
        false
    }

    pub fn bootstrap(&mut self) {
        let bootstrap_count = self.config.bootstrap_count;
        self.network.insert_and_join(0, TOPIC, vec![]);
        self.run_ticks(1);
        for i in 1..bootstrap_count {
            self.network.insert_and_join(i as u64, TOPIC, vec![0]);
            self.run_ticks(4);
        }
        info!(tick=%self.network.current_tick(), "created {bootstrap_count}");

        let limit = bootstrap_count * 4;
        if !self.run_until_all_active(limit) {
            panic!("failed to activate {bootstrap_count} bootstrap nodes within a limit of {limit} ticks");
        }

        info!(tick=%self.network.current_tick(), "all active");

        let peer_count = self.config.peers_count;
        for i in bootstrap_count..peer_count {
            let contact = (i % bootstrap_count) as u64;
            self.network.insert_and_join(i as u64, TOPIC, vec![contact]);
            self.run_ticks(2);
        }
        info!(tick=%self.network.current_tick(), "created {peer_count}");

        let limit = peer_count;
        if !self.run_until_all_active(limit) {
            panic!("failed to activate nodes within a limit of {limit} ticks");
        }
        info!(tick=%self.network.current_tick(), "all active");

        self.run_ticks(self.config.warmup_ticks);
        info!(tick=%self.network.current_tick(), "warmup complete");

        let limit = 100;
        if !self.run_until_all_active(limit) {
            panic!("failed to keep nodes active after warmup");
        }
        info!(tick=%self.network.current_tick(), "all active");
    }

    pub fn run_ticks(&mut self, ticks: usize) -> BTreeMap<PeerId, Vec<Event<PeerId>>> {
        let mut events: BTreeMap<PeerId, Vec<Event<PeerId>>> = BTreeMap::new();
        for _ in 0..ticks {
            self.network.tick();
            self.report_swarm();
            for (peer, _topic, event) in self.network.events() {
                events.entry(peer).or_default().push(event);
            }
        }
        events
    }

    pub fn gossip_tick(&mut self, missing: &mut BTreeMap<PeerId, BTreeSet<Bytes>>) {
        let events = self.run_ticks(1);
        for (peer, events) in events.into_iter() {
            let entry = missing.get_mut(&peer).unwrap();
            for event in events {
                match event {
                    Event::Received(message) => {
                        entry.remove(&message.content);
                    }
                    _ => {}
                }
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
            missing.iter().map(|(_p, s)| s.len()).sum::<usize>(),
            expected_recv_count
        );

        info!(
            tick=%self.network.current_tick(),
            "round {i}: send {len} messages / recv {expected_recv_count} total",
            len = messages.len(),
            i = self.round_stats.len()
        );

        // Send all messages at once
        let mut cnt = 0;
        for (from, message) in messages {
            cnt += 1;
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
            self.gossip_tick(&mut missing);
        }

        let mut ticks = 0;
        loop {
            ticks += 1;
            let missing_count: usize = missing.iter().map(|(_k, set)| set.len()).sum();

            if missing_count == 0 {
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
        self.report_gossip_round(expected_recv_count, ticks);
    }

    fn report_gossip_round(&mut self, expected_recv_count: usize, ticks: usize) {
        let ticks = ticks as u32;
        let payloud_msg_count = self.total_payload_messages();
        let ctrl_msg_count = self.total_control_messages();
        let rmr = (payloud_msg_count as f32 / (expected_recv_count as f32 - 1.)) - 1.;
        let ldh = self.max_ldh();

        let round_stats = RoundStats {
            ticks,
            rmr,
            ldh: ldh as f32,
        };
        let network_stats = self.network.state_stats();
        info!(
            "round {}: pay {} ctrl {} rmr {:.4} ldh {} ticks {}\n{network_stats}",
            self.round_stats.len(),
            payloud_msg_count,
            ctrl_msg_count,
            round_stats.rmr,
            round_stats.ldh,
            round_stats.ticks
        );
        self.round_stats.push(round_stats);
    }

    pub fn report_round_average(&self) -> RoundStats {
        let len = self.round_stats.len();
        let mut avg = self
            .round_stats
            .iter()
            .fold(RoundStats::default(), |mut agg, round| {
                agg.rmr += round.rmr;
                agg.ldh += round.ldh;
                agg.ticks += round.ticks;
                agg
            });
        avg.rmr /= len as f32;
        avg.ldh /= len as f32;
        avg.ticks /= len as u32;
        let RoundStats { ticks, rmr, ldh } = avg;
        eprintln!(
            "average over {} rounds with {} peers: RMR {rmr:.2} LDH {ldh:.2} ticks {ticks:.2}",
            self.round_stats.len(),
            self.network.peers.len(),
        );
        eprintln!("RMR = Relative Message Redundancy, LDH = Last Delivery Hop");
        avg
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
    *entry = *entry + 1;
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
                .expect(&format!("Failed to parse environment variable {name}"))
        })
        .unwrap_or(default)
}
