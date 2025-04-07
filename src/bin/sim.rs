use std::{ffi::OsStr, path::PathBuf};

use clap::Parser;
use iroh_gossip::proto::{
    tests::{RoundStats, RoundStatsAvg, Simulator, SimulatorConfig},
    Config,
};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
enum Simulation {
    GossipSingle,
    GossipMulti,
    GossipAll,
}

#[derive(Debug, Serialize, Deserialize)]
struct ScenarioDescription {
    sim: Simulation,
    nodes: u32,
    bootstrap_nodes: Option<u32>,
    #[serde(default = "defaults::rounds")]
    rounds: u32,
    config: Option<Config>,
}

impl ScenarioDescription {
    pub fn label(&self) -> String {
        let &ScenarioDescription {
            sim,
            nodes,
            bootstrap_nodes: _,
            rounds,
            config: _,
        } = &self;
        format!("{sim:?}-n{nodes}-r{rounds}")
    }
}

mod defaults {
    pub fn rounds() -> u32 {
        30
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SimConfig {
    seeds: Vec<u64>,
    scenario: Vec<ScenarioDescription>,
}

#[derive(Debug, Parser)]
struct Cli {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Parser)]
enum Command {
    /// Run simulations
    Run {
        #[clap(short, long)]
        config_path: PathBuf,
        #[clap(short, long)]
        out_dir: PathBuf,
        #[clap(short, long)]
        baseline: Option<PathBuf>,
    },
    /// Compare simulation runs
    Compare { baseline: PathBuf, current: PathBuf },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args: Cli = Cli::parse();
    match args.command {
        Command::Run {
            config_path,
            out_dir,
            baseline,
        } => {
            let config_text = std::fs::read_to_string(&config_path)?;
            let config: SimConfig = toml::from_str(&config_text)?;

            let seeds = config.seeds;
            let scenarios = config.scenario;

            std::fs::create_dir_all(&out_dir)?;
            scenarios.into_par_iter().try_for_each(|scenario| {
                let path = out_dir.join(format!("{}.json", scenario.label()));
                let avg = run_simulation(&seeds, scenario);
                let json = serde_json::to_string(&avg)?;
                std::fs::write(path, json)?;
                anyhow::Ok(())
            })?;
            if let Some(baseline) = baseline {
                compare(baseline, out_dir)?;
            }
        }
        Command::Compare { baseline, current } => {
            compare(baseline, current)?;
        }
    }

    Ok(())
}

fn run_simulation(seeds: &[u64], scenario: ScenarioDescription) -> RoundStatsAvg {
    let label = scenario.label();
    let proto_config = scenario.config.unwrap_or_default();
    let mut stats = vec![];
    for seed in seeds {
        let seed = *seed;
        let bootstrap_nodes = scenario
            .bootstrap_nodes
            .unwrap_or_else(|| (scenario.nodes / 10).max(3));
        let sim_config = SimulatorConfig {
            seed,
            peers_count: scenario.nodes as usize,
            bootstrap_count: bootstrap_nodes as usize,
            ..Default::default()
        };
        let mut simulator = Simulator::new(sim_config, proto_config.clone());
        simulator.bootstrap();
        let avg = match scenario.sim {
            Simulation::GossipSingle => BigSingle.run(simulator, scenario.rounds as usize),
            Simulation::GossipMulti => BigMulti.run(simulator, scenario.rounds as usize),
            Simulation::GossipAll => BigAll.run(simulator, scenario.rounds as usize),
        };
        stats.push(avg);
    }
    let avg = RoundStats::avg(&stats);
    println!("{label} with {} seeds", scenario.rounds);
    println!("mean: {}", avg.mean);
    println!("min:  {}", avg.min);
    println!("max:  {}", avg.max);
    println!("");
    avg
}

trait Scenario {
    fn run(self, sim: Simulator, rounds: usize) -> RoundStats;
}

struct BigSingle;
impl Scenario for BigSingle {
    fn run(self, mut simulator: Simulator, rounds: usize) -> RoundStats {
        let from = simulator.random_peer();
        for i in 0..rounds {
            let message = format!("m{i}").into_bytes().into();
            let messages = vec![(from, message)];
            simulator.gossip_round(messages);
        }
        let avg = simulator.report_round_average();
        avg
    }
}

struct BigMulti;
impl Scenario for BigMulti {
    fn run(self, mut simulator: Simulator, rounds: usize) -> RoundStats {
        for i in 0..rounds {
            let from = simulator.random_peer();
            let message = format!("m{i}").into_bytes().into();
            let messages = vec![(from, message)];
            simulator.gossip_round(messages);
        }
        simulator.report_round_average()
    }
}

struct BigAll;
impl Scenario for BigAll {
    fn run(self, mut simulator: Simulator, rounds: usize) -> RoundStats {
        let messages_per_peer = 1;
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
        simulator.report_round_average()
    }
}

fn compare(baseline: PathBuf, current: PathBuf) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(&baseline)? {
        let entry = entry?;
        let path = entry.path();
        let Some(filename) = path.file_name() else {
            continue;
        };
        if !path.is_file() || path.extension() != Some(OsStr::new("json")) {
            continue;
        }
        let current_path = current.join(filename);
        let Ok(current) = std::fs::read_to_string(current_path) else {
            continue;
        };
        let baseline = std::fs::read_to_string(&path)?;
        let baseline: RoundStatsAvg = serde_json::from_str(&baseline)?;
        let current: RoundStatsAvg = serde_json::from_str(&current)?;
        let diff = baseline.diff(&current);
        println!("{}", filename.to_string_lossy());
        println!("mean {}", fmt_diff_round(&diff.mean));
        println!("min  {}", fmt_diff_round(&diff.min));
        println!("max  {}", fmt_diff_round(&diff.max));
    }
    Ok(())
}

fn fmt_diff_round(round: &RoundStats) -> String {
    format!(
        "RMR {} LDH {} ticks {}",
        fmt_percent(round.rmr),
        fmt_percent(round.ldh),
        fmt_percent(round.ticks),
    )
}
fn fmt_percent(diff: f32) -> String {
    if diff == 0.0 {
        "   0.00%".to_string()
    } else {
        format!("{:>+7.2}%", diff * 100.)
    }
}
