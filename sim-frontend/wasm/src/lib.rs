use std::{cell::RefCell, rc::Rc};

use iroh_gossip::proto::{
    Event,
    sim::{BootstrapMode, EventRecorder, NetworkConfig, Simulator, SimulatorConfig},
};
use serde::{Deserialize, Serialize};
use tracing::level_filters::LevelFilter;
use tracing_subscriber_wasm::MakeConsoleWriter;
use wasm_bindgen::{JsError, JsValue, prelude::wasm_bindgen};

#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();

    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::DEBUG)
        .with_writer(
            // To avoide trace events in the browser from showing their JS backtrace
            MakeConsoleWriter::default().map_trace_level_to(tracing::Level::DEBUG),
        )
        // If we don't do this in the browser, we get a runtime error.
        .without_time()
        .with_ansi(false)
        .init();

    tracing::info!("(testing logging) Logging setup");
}

#[derive(Default, Clone)]
struct Events(Rc<RefCell<Vec<Event2>>>);

impl EventRecorder<u64> for Events {
    fn record(
        &mut self,
        time: std::time::Duration,
        node: u64,
        _topic: iroh_gossip::proto::TopicId,
        event: Event<u64>,
    ) {
        let mut this = RefCell::borrow_mut(&self.0);
        let e = Event2 {
            i: this.len() as u64,
            time: time.as_millis() as u64,
            node,
            event: match &event {
                Event::NeighborUp(_) => EventType::Up,
                Event::NeighborDown(_) => EventType::Down,
                Event::Received(_) => EventType::Recv,
            },
            peer: match &event {
                Event::NeighborUp(n) => *n,
                Event::NeighborDown(n) => *n,
                Event::Received(e) => e.delivered_from,
            },
            msg: match &event {
                Event::Received(e) => Some(String::from_utf8_lossy(&e.content).to_string()),
                _ => None,
            },
        };
        this.push(e);
    }

    fn flush(&mut self) {}
}

#[wasm_bindgen]
pub fn default_config() -> JsValue {
    let config = SimConfig::default();
    serde_wasm_bindgen::to_value(&config).unwrap()
}

#[wasm_bindgen]
pub fn run_simulation(config: JsValue) -> Result<JsValue, JsValue> {
    let config: SimConfig = serde_wasm_bindgen::from_value(config)?;
    let sim_config = SimulatorConfig {
        rng_seed: config.seed,
        peers: config.peers,
        events_csv_path: None,
        gossip_round_timeout: n0_future::time::Duration::from_secs(1),
    };
    println!("config: {config:?}");
    tracing::info!("config: {config:?}");
    let mut simulator = Simulator::new(sim_config, config.config);
    let events = Events::default();
    simulator
        .network
        .set_event_recorder(Box::new(events.clone()));
    let _outcome = simulator.bootstrap(BootstrapMode::Single);
    simulator.gossip_round(vec![(1, "hi".as_bytes().to_vec().into())]);
    let out = Out {
        events: events.0.borrow().clone(),
    };
    let out = serde_wasm_bindgen::to_value(&out)?;
    Ok(out)
    // let events = simulator
    //     .network
    //     .events()
    //     .map(|(peer, _topic, event)| (peer, event));

    // let events2 = events.enumerate().map(|((node, event), i)| Event2 {
    //     i: i as u64,
    //     time: todo!(),
    //     node: todo!(),
    //     event: todo!(),
    //     peer: todo!(),
    //     msg: todo!(),
    // });
    // todo!()

    // let out = Out { events };
}

// pub trait EventRecorder<PI> {
//     ///
//     fn record(&mut self, time: Duration, node: PI, topic: TopicId, event: Event<PI>);
//     ///
//     fn flush(&mut self);
// }

#[derive(Debug, Serialize, Deserialize)]
struct Out {
    events: Vec<Event2>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Event2 {
    i: u64,
    time: u64,
    node: u64,
    event: EventType,
    peer: u64,
    msg: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
enum EventType {
    #[serde(rename = "up")]
    Up,
    #[serde(rename = "down")]
    Down,
    #[serde(rename = "recv")]
    Recv,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SimConfig {
    seed: u64,
    peers: usize,
    config: NetworkConfig,
}
