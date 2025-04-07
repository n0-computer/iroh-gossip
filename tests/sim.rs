use std::{env, fmt, str::FromStr, time::Duration};

use rand::{seq::IteratorRandom, SeedableRng};

use iroh_gossip::proto::Config;

use iroh_gossip::proto::tests::{BootstrapMode, Simulator, SimulatorConfig};

#[test]
// #[traced_test]
fn big_hyparview() {
    let mut gossip_config = Config::default();
    gossip_config.membership.shuffle_interval = Duration::from_secs(5);
    let config = SimulatorConfig::from_env();
    let bootstrap = BootstrapMode::from_env(config.peers);
    let mut simulator = Simulator::new(config, gossip_config);
    simulator.bootstrap(bootstrap);
    let state = simulator.report_swarm();
    assert!(state.min_active_len > 0);
}

#[test]
// #[traced_test]
fn big_multiple_sender() {
    let mut gossip_config = Config::default();
    gossip_config.broadcast.optimization_threshold = (read_var("OPTIM", 7) as u16).into();
    gossip_config.membership.shuffle_interval = Duration::from_secs(5);
    let config = SimulatorConfig::from_env();
    let rounds = read_var("ROUNDS", 30);
    let bootstrap = BootstrapMode::from_env(config.peers);
    let mut simulator = Simulator::new(config, gossip_config);
    simulator.bootstrap(bootstrap);
    let mut rng = rand_chacha::ChaCha12Rng::seed_from_u64(0);
    for i in 0..rounds {
        let from = simulator.network.peer_ids().choose(&mut rng).unwrap();
        let message = format!("m{i}").into_bytes().into();
        let messages = vec![(from, message)];
        simulator.gossip_round(messages);
    }
    let avg = simulator.report_round_average();
    eprintln!(
        "average with {} peers after {} rounds:\n{}",
        simulator.peer_count(),
        rounds,
        avg
    );
    assert!(avg.ldh < 18.);
    assert!(avg.rmr < 0.2);
}

#[test]
// #[traced_test]
fn big_single_sender() {
    let mut gossip_config = Config::default();
    gossip_config.broadcast.optimization_threshold = (read_var("OPTIM", 7) as u16).into();
    gossip_config.membership.shuffle_interval = Duration::from_secs(5);
    let config = SimulatorConfig::from_env();
    let bootstrap = BootstrapMode::from_env(config.peers);
    let rounds = read_var("ROUNDS", 30);
    let mut simulator = Simulator::new(config, gossip_config);
    simulator.bootstrap(bootstrap);
    let from = 8;
    for i in 0..rounds {
        let message = format!("m{i}").into_bytes().into();
        let messages = vec![(from, message)];
        simulator.gossip_round(messages);
    }
    let avg = simulator.report_round_average();
    eprintln!(
        "average with {} peers after {} rounds:\n{}",
        simulator.peer_count(),
        rounds,
        avg
    );
    assert!(avg.ldh < 10.);
    assert!(avg.rmr < 0.1);
}

#[test]
// #[traced_test]
fn big_burst() {
    let mut gossip_config = Config::default();
    gossip_config.broadcast.optimization_threshold = (read_var("OPTIM", 7) as u16).into();
    gossip_config.membership.shuffle_interval = Duration::from_secs(5);

    let config = SimulatorConfig::from_env();
    let bootstrap = BootstrapMode::from_env(config.peers);
    let rounds = read_var("ROUNDS", 20);

    let mut simulator = Simulator::new(config, gossip_config);
    simulator.bootstrap(bootstrap);
    let messages_per_peer = read_var("MESSAGES_PER_PEER", 1);
    for i in 0..rounds {
        let mut messages = vec![];
        for id in simulator.network.peer_ids() {
            for j in 0..messages_per_peer {
                let message: bytes::Bytes = format!("{i}:{j}.{id}").into_bytes().into();
                messages.push((id, message));
            }
        }
        simulator.gossip_round(messages);
    }
    let avg = simulator.report_round_average();
    eprintln!(
        "average with {} peers after {} rounds:\n{}",
        simulator.peer_count(),
        rounds,
        avg
    );
    assert!(avg.ldh < 30.);
    assert!(avg.rmr < 2.);
}

fn read_var<T: FromStr<Err: fmt::Display + fmt::Debug>>(name: &str, default: T) -> T {
    env::var(name)
        .map(|x| {
            x.parse()
                .unwrap_or_else(|_| panic!("Failed to parse environment variable {name}"))
        })
        .unwrap_or(default)
}
