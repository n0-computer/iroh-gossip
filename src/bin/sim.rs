use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::Parser;
use comfy_table::{
    modifiers::UTF8_ROUND_CORNERS,
    presets::{NOTHING, UTF8_FULL, UTF8_NO_BORDERS},
    Cell, CellAlignment, Table,
};
use iroh_gossip::proto::{
    tests::{BootstrapMode, RoundStats, RoundStatsAvg, Simulator, SimulatorConfig},
    Config,
};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use tracing::{error_span, warn};

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
    #[serde(default)]
    bootstrap: BootstrapMode,
    // bootstrap_nodes: Option<u32>,
    #[serde(default = "defaults::rounds")]
    rounds: u32,
    config: Option<Config>,
}

impl ScenarioDescription {
    pub fn label(&self) -> String {
        let &ScenarioDescription {
            sim,
            nodes,
            rounds,
            config: _,
            bootstrap: _,
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
    config: Option<Config>,
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
        #[clap(short, long)]
        single_threaded: bool,
        #[clap(short, long)]
        filter: Vec<String>,
    },
    /// Compare simulation runs
    Compare {
        baseline: PathBuf,
        current: PathBuf,
        #[clap(short, long)]
        filter: Vec<String>,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args: Cli = Cli::parse();
    match args.command {
        Command::Run {
            config_path,
            out_dir,
            baseline,
            single_threaded,
            filter,
        } => {
            let config_text = std::fs::read_to_string(&config_path)?;
            let config: SimConfig = toml::from_str(&config_text)?;

            let base_config = config.config.unwrap_or_default();
            let seeds = config.seeds;
            let mut scenarios = config.scenario;
            for scenario in scenarios.iter_mut() {
                scenario.config.get_or_insert_with(|| base_config.clone());
            }

            std::fs::create_dir_all(&out_dir)?;

            let filter_fn = |s: &ScenarioDescription| {
                let label = s.label();
                if filter.is_empty() {
                    true
                } else {
                    filter.iter().any(|x| x == &label)
                }
            };

            if !single_threaded {
                scenarios
                    .into_par_iter()
                    .filter(filter_fn)
                    .try_for_each(|scenario| run_and_save_simulation(scenario, &seeds, &out_dir))?;
            } else {
                scenarios
                    .into_iter()
                    .filter(filter_fn)
                    .try_for_each(|scenario| run_and_save_simulation(scenario, &seeds, &out_dir))?;
            }
            if let Some(baseline) = baseline {
                compare_dirs(baseline, out_dir, filter)?;
            }
        }
        Command::Compare {
            baseline,
            current,
            filter,
        } => {
            compare_dirs(baseline, current, filter)?;
        }
    }

    Ok(())
}

fn run_and_save_simulation(
    scenario: ScenarioDescription,
    seeds: &[u64],
    out_path: impl AsRef<Path>,
) -> Result<()> {
    let span = error_span!("sim", s=%scenario.label());
    let _guard = span.enter();

    let label = scenario.label();

    let path = out_path.as_ref().join(format!("{label}.config.toml"));
    let encoded = toml::to_string(&scenario)?;
    std::fs::write(path, encoded)?;

    let result = run_simulation(&seeds, scenario);

    let path = out_path.as_ref().join(format!("{label}.results.json"));
    let encoded = serde_json::to_string(&result)?;
    std::fs::write(path, encoded)?;

    anyhow::Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SimulationResults {
    /// Maps seeds to results
    results: HashMap<u64, Result<RoundStats, SimulationError>>,
    average: Option<RoundStatsAvg>,
}

impl SimulationResults {
    fn load_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let s = std::fs::read_to_string(path.as_ref())?;
        let out = serde_json::from_str(&s)?;
        Ok(out)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, thiserror::Error)]
enum SimulationError {
    #[error("failed to bootstrap")]
    FailedToBootstrap,
}

fn run_simulation(seeds: &[u64], scenario: ScenarioDescription) -> SimulationResults {
    let mut results = HashMap::new();
    let label = scenario.label();
    let proto_config = scenario.config.unwrap_or_default();
    for seed in seeds {
        let seed = *seed;
        let sim_config = SimulatorConfig {
            rng_seed: seed,
            peers: scenario.nodes as usize,
            ..Default::default()
        };
        let bootstrap = scenario.bootstrap.clone();
        let mut simulator = Simulator::new(sim_config, proto_config.clone());
        let result = if !simulator.bootstrap(bootstrap) {
            warn!("{label} failed to bootstrap with seed {seed}");
            Err(SimulationError::FailedToBootstrap)
        } else {
            Ok(match scenario.sim {
                Simulation::GossipSingle => BigSingle.run(simulator, scenario.rounds as usize),
                Simulation::GossipMulti => BigMulti.run(simulator, scenario.rounds as usize),
                Simulation::GossipAll => BigAll.run(simulator, scenario.rounds as usize),
            })
        };
        results.insert(seed, result);
    }

    let stats: Vec<_> = results
        .values()
        .filter_map(|x| x.as_ref().ok())
        .cloned()
        .collect();
    let average = if !stats.is_empty() {
        let avg = RoundStats::avg(&stats);
        println!("{label} with {} seeds", seeds.len());
        // println!("mean: {}", avg.mean);
        // println!("min:  {}", avg.min);
        // println!("max:  {}", avg.max);
        // println!("");

        let mut table = Table::new();
        let header = ["", "RMR", "LDH", "ticks", "missing"]
            .into_iter()
            .map(|s| Cell::new(s).set_alignment(CellAlignment::Right));
        table
            .load_preset(NOTHING)
            .set_header(header)
            .add_row(fmt_round("mean", &avg.mean))
            .add_row(fmt_round("max", &avg.max))
            .add_row(fmt_round("min", &avg.min));
        println!("{table}");
        println!("");
        Some(avg)
    } else {
        println!("all seeds failed");
        None
    };
    SimulationResults { average, results }
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
        simulator.report_round_average()
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

fn compare_dirs(baseline_dir: PathBuf, current_path: PathBuf, filter: Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(&current_path)?
        .filter_map(Result::ok)
        .filter(|x| x.path().is_file())
    {
        let current_file = entry.path();
        let Some(filename) = current_file.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(basename) = filename.strip_suffix(".results.json") else {
            continue;
        };
        if !filter.is_empty() && !filter.iter().any(|x| x == basename) {
            continue;
        }
        let baseline_file = baseline_dir.join(filename);
        if !baseline_file.exists() {
            println!("skip {} (not in baseline)", filename);
        }
        println!("");
        println!("comparing {}", basename);
        if let Err(err) = compare_files(&baseline_file, &current_file) {
            println!("  skip (reason: {err:#}");
        }
    }
    Ok(())
}

fn compare_files(baseline: impl AsRef<Path>, current: impl AsRef<Path>) -> Result<()> {
    let baseline =
        SimulationResults::load_from_file(baseline.as_ref()).context("failed to load baseline")?;
    let current =
        SimulationResults::load_from_file(current.as_ref()).context("failed to load current")?;
    compare_results(baseline, current);
    Ok(())
}

fn compare_results(baseline: SimulationResults, current: SimulationResults) {
    match (baseline.average, current.average) {
        (None, Some(_avg)) => {
            println!("baseline run did not finish");
        }
        (Some(_avg), None) => {
            println!("current run did not finish");
        }
        (None, None) => println!("both runs did not finish"),
        (Some(baseline), Some(current)) => {
            let diff = baseline.diff(&current);
            let mut table = Table::new();
            let header = ["", "RMR", "LDH", "ticks", "missing"]
                .into_iter()
                .map(|s| Cell::new(s).set_alignment(CellAlignment::Right));
            table
                .load_preset(NOTHING)
                .set_header(header)
                .add_row(fmt_diff_round("mean", &diff.mean))
                .add_row(fmt_diff_round("max", &diff.max))
                .add_row(fmt_diff_round("min", &diff.min));
            println!("{table}");
        }
    }
}

fn fmt_round(label: &str, round: &RoundStats) -> Vec<Cell> {
    [
        label.to_string(),
        format!("{:.2}", round.rmr),
        format!("{:.2}", round.ldh),
        format!("{:.2}", round.ticks),
        format!("{:.2}", round.missing_receives),
    ]
    .into_iter()
    .map(|s| Cell::new(s).set_alignment(CellAlignment::Right))
    .collect()
}
fn fmt_diff_round(label: &str, round: &RoundStats) -> Vec<String> {
    vec![
        label.to_string(),
        fmt_percent(round.rmr),
        fmt_percent(round.ldh),
        fmt_percent(round.ticks),
        fmt_percent(round.missing_receives),
    ]
}

fn fmt_percent(diff: f32) -> String {
    format!("{:>+10.2}%", diff * 100.)
}
