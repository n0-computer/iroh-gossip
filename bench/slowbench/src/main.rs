//! Measures gossip with slow peers on a patchbay lab.
//!
//! One process runs every node, each in its own network namespace with its
//! own single-threaded runtime. Slow nodes get a rate-limited link. The
//! harness uses only the public API, so it builds against any revision.

use std::{
    collections::{BTreeMap, HashSet},
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use bytes::Bytes;
use clap::Parser;
use iroh::{
    address_lookup::memory::MemoryLookup,
    endpoint::{presets, QuicTransportConfig, VarInt},
    protocol::Router,
    Endpoint,
    EndpointAddr, EndpointId, RelayMode,
};
use iroh_gossip::{
    api::{Event, GossipReceiver, GossipSender},
    net::{Gossip, GOSSIP_ALPN},
    proto::TopicId,
};
use iroh_metrics::{MetricValue, MetricsGroup};
use n0_future::StreamExt;
use patchbay::{Lab, LinkCondition, LinkDirection};

#[derive(Parser, Debug, Clone)]
struct Args {
    /// Label of the revision under test.
    #[arg(long, default_value = "dev")]
    label: String,
    #[arg(long, default_value_t = 10)]
    nodes: usize,
    /// Number of slow nodes. Node 0, the first publisher, is never slow.
    #[arg(long, default_value_t = 1)]
    slow: usize,
    #[arg(long, default_value_t = 1000)]
    slow_kbit: u32,
    #[arg(long, default_value_t = 20)]
    rtt_ms: u32,
    /// Messages per second, over all publishers and topics.
    #[arg(long, default_value_t = 50)]
    rate: u32,
    #[arg(long, default_value_t = 1000)]
    size: usize,
    #[arg(long, default_value_t = 1)]
    publishers: usize,
    #[arg(long, default_value_t = 1)]
    topics: usize,
    #[arg(long, default_value_t = 60)]
    publish_secs: u64,
    #[arg(long, default_value_t = 30)]
    drain_secs: u64,
    /// QUIC send window of every node, in bytes. The default of noq is 10 MB.
    #[arg(long)]
    send_window: Option<u64>,
    /// QUIC receive window per stream of every node, in bytes. The default of noq is 1.25 MB.
    #[arg(long)]
    stream_window: Option<u32>,
    /// The maximum message size of every node, in bytes. The default of iroh-gossip is 4096.
    #[arg(long)]
    max_message_size: Option<usize>,
    /// QUIC receive window per connection of every node, in bytes. Unlimited by default.
    #[arg(long)]
    receive_window: Option<u32>,
}

/// Returns the nanoseconds since the process started, on a clock that does not jump.
///
/// All nodes run in this process, so they share the clock.
fn now_nanos() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_nanos() as u64
}

/// What one node received.
#[derive(Default)]
struct NodeStats {
    seen: HashSet<(u16, u16, u64)>,
    /// Age of each first delivery, in ms, with whether it arrived after publishing ended.
    ages: Vec<(f64, bool)>,
    neighbor_up: u64,
    neighbor_down: u64,
    lagged: u64,
    /// The current neighbors, over all topics.
    neighbors: HashSet<EndpointId>,
}

struct Node {
    gossip: Gossip,
    _router: Router,
    senders: Vec<GossipSender>,
}

fn topic(i: usize) -> TopicId {
    let mut bytes = [0u8; 32];
    bytes[0] = i as u8;
    bytes[1] = 0xaa;
    TopicId::from_bytes(bytes)
}

fn main() -> Result<()> {
    patchbay::init_userns()?;
    now_nanos();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let args = Args::parse();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run(args))
}

async fn run(args: Args) -> Result<()> {
    let lab = Lab::new().await?;
    let router = lab.add_router("lan").build().await?;
    let lookup = MemoryLookup::new();
    let ended = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut nodes = Vec::new();
    let mut stats = Vec::new();
    for i in 0..args.nodes {
        let dev = lab
            .add_device(&format!("n{i}"))
            .uplink(router.id())
            .build()
            .await?;
        let slow = i >= args.nodes - args.slow && i != 0;
        let condition = match slow {
            true => LinkCondition::new()
                .rate_kbit(args.slow_kbit)
                .rtt_ms(args.rtt_ms),
            false => LinkCondition::new().latency_ms(args.rtt_ms / 4),
        };
        dev.default_iface()
            .context("iface")?
            .set_condition(condition, LinkDirection::Both)
            .await?;
        let ip = dev.ip().context("ip")?;
        let node_stats = Arc::new(Mutex::new(NodeStats::default()));
        stats.push((slow, node_stats.clone()));
        let lookup = lookup.clone();
        let args2 = args.clone();
        let ended = ended.clone();
        let bootstrap: Vec<EndpointId> = match nodes.first() {
            Some((id, _)) => vec![*id],
            None => vec![],
        };
        let (id, node) = dev
            .spawn(async move |_dev| -> Result<(EndpointId, Node)> {
                let mut transport = QuicTransportConfig::builder();
                if let Some(window) = args2.send_window {
                    transport = transport.send_window(window);
                }
                if let Some(window) = args2.receive_window {
                    transport = transport.receive_window(VarInt::from_u32(window));
                }
                if let Some(window) = args2.stream_window {
                    transport = transport.stream_receive_window(VarInt::from_u32(window));
                }
                let endpoint = Endpoint::builder(presets::Minimal)
                    .relay_mode(RelayMode::Disabled)
                    .alpns(vec![GOSSIP_ALPN.to_vec()])
                    .transport_config(transport.build())
                    .bind()
                    .await?;
                endpoint.address_lookup()?.add(lookup.clone());
                let port = endpoint.bound_sockets()[0].port();
                lookup.add_endpoint_info(
                    EndpointAddr::new(endpoint.id())
                        .with_ip_addr(SocketAddr::new(IpAddr::V4(ip), port)),
                );
                let mut builder = Gossip::builder();
                if let Some(size) = args2.max_message_size {
                    builder = builder.max_message_size(size);
                }
                if std::env::var("SLOWBENCH_NEWEST").is_ok_and(|v| v == "1") {
                    builder = newest(builder);
                }
                let gossip = builder.spawn(endpoint.clone());
                let router = Router::builder(endpoint.clone())
                    .accept(GOSSIP_ALPN, gossip.clone())
                    .spawn();
                let mut senders = Vec::new();
                for t in 0..args2.topics {
                    let (sender, receiver) =
                        gossip.subscribe(topic(t), bootstrap.clone()).await?.split();
                    senders.push(sender);
                    tokio::spawn(receive(receiver, node_stats.clone(), ended.clone()));
                }
                Ok((endpoint.id(), Node { gossip, _router: router, senders }))
            })?
            .await??;
        nodes.push((id, node));
    }
    eprintln!("[{}] {} nodes up, waiting for the swarm", args.label, args.nodes);
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Publish.
    let start = Instant::now();
    let interval = Duration::from_secs_f64(1.0 / args.rate as f64);
    let mut sent = vec![0u64; args.topics];
    let mut max_lateness = Duration::ZERO;
    let mut seq = 0u64;
    let mut next = start;
    let deadline = start + Duration::from_secs(args.publish_secs);
    while Instant::now() < deadline {
        let now = Instant::now();
        if now < next {
            tokio::time::sleep_until(next.into()).await;
        }
        max_lateness = max_lateness.max(Instant::now().saturating_duration_since(next));
        let publisher = (seq as usize) % args.publishers;
        let t = (seq as usize / args.publishers) % args.topics;
        let mut message = Vec::with_capacity(args.size);
        message.extend_from_slice(&(publisher as u16).to_be_bytes());
        message.extend_from_slice(&(t as u16).to_be_bytes());
        message.extend_from_slice(&seq.to_be_bytes());
        message.extend_from_slice(&now_nanos().to_be_bytes());
        message.resize(args.size.max(20), 0);
        let send = nodes[publisher].1.senders[t].broadcast(Bytes::from(message));
        // A stalled actor blocks the publisher. Give up on this one after a while.
        match tokio::time::timeout(Duration::from_secs(5), send).await {
            Ok(res) => res?,
            Err(_) => eprintln!("[{}] broadcast blocked for 5s", args.label),
        }
        sent[t] += 1;
        seq += 1;
        next += interval;
        // Do not burst to catch up after a stall.
        if Instant::now() > next + Duration::from_secs(1) {
            next = Instant::now();
        }
    }
    let published = start.elapsed();
    ended.store(true, std::sync::atomic::Ordering::Relaxed);
    tokio::time::sleep(Duration::from_secs(args.drain_secs)).await;

    // Collect.
    let total_sent: u64 = sent.iter().sum();
    let mut groups: BTreeMap<&str, (Vec<f64>, Vec<f64>, u64, u64, u64)> = BTreeMap::new();
    let mut churn = (0u64, 0u64, 0u64);
    let mut per_node = Vec::new();
    for (i, (slow, s)) in stats.iter().enumerate() {
        let s = s.lock().unwrap();
        let mut ages: Vec<f64> = s.ages.iter().map(|(a, _)| *a).collect();
        ages.sort_by(|a, b| a.partial_cmp(b).unwrap());
        per_node.push(serde_json::json!({
            "slow": slow,
            "delivered": s.seen.len(),
            "p99_ms": ages.get((ages.len() as f64 * 0.99) as usize).copied(),
            "neighbor_down": s.neighbor_down,
            "publisher_neighbor": s.neighbors.contains(&nodes[0].0),
            "dropped": metric(&nodes[i].1, "msgs_dropped"),
        }));
        churn.0 += s.neighbor_up;
        churn.1 += s.neighbor_down;
        churn.2 += s.lagged;
        if i < args.publishers {
            continue;
        }
        let name = if *slow { "slow" } else { "healthy" };
        let g = groups.entry(name).or_default();
        g.0.push(s.seen.len() as f64 / total_sent.max(1) as f64);
        g.1.extend(s.ages.iter().map(|(age, _)| *age));
        g.2 += s.ages.iter().filter(|(_, late)| *late).count() as u64;
        g.3 += 1;
        g.4 += s.seen.len() as u64;
    }
    let mut metrics: BTreeMap<String, i128> = BTreeMap::new();
    for (_, node) in &nodes {
        for item in node.gossip.metrics().iter() {
            let value = match item.value() {
                MetricValue::Counter(v) => v as i128,
                MetricValue::Gauge(v) => v as i128,
                _ => continue,
            };
            *metrics.entry(item.name().to_string()).or_default() += value;
        }
    }
    let mut out = serde_json::json!({
        "label": args.label,
        "nodes": args.nodes, "slow": args.slow, "slow_kbit": args.slow_kbit,
        "rate": args.rate, "size": args.size, "publishers": args.publishers,
        "topics": args.topics, "publish_secs": args.publish_secs, "drain_secs": args.drain_secs,
        "send_window": args.send_window, "stream_window": args.stream_window,
        "receive_window": args.receive_window, "rtt_ms": args.rtt_ms,
        "max_message_size": args.max_message_size,
        "env": std::env::vars().filter(|(k, _)| k.starts_with("GOSSIP_EXP")).collect::<BTreeMap<_, _>>(),
        "sent": total_sent,
        "achieved_rate": total_sent as f64 / published.as_secs_f64(),
        "max_publish_lateness_ms": max_lateness.as_secs_f64() * 1000.0,
        "neighbor_up": churn.0, "neighbor_down": churn.1, "lagged": churn.2,
        "max_rss_mb": max_rss_mb(),
    });
    for (name, (fractions, mut ages, late, count, delivered)) in groups {
        ages.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let pct = |p: f64| ages.get(((ages.len() as f64 * p) as usize).min(ages.len().saturating_sub(1))).copied().unwrap_or(f64::NAN);
        out[format!("{name}_nodes")] = serde_json::json!({
            "nodes": count,
            "delivered_mean": fractions.iter().sum::<f64>() / fractions.len() as f64,
            "delivered_min": fractions.iter().cloned().fold(f64::INFINITY, f64::min),
            "delivered_total": delivered,
            "after_publish": late,
            "age_p50_ms": pct(0.5), "age_p99_ms": pct(0.99), "age_max_ms": pct(1.0),
        });
    }
    out["metrics"] = serde_json::json!(metrics);
    out["per_node"] = serde_json::json!(per_node);
    println!("{out}");
    std::process::exit(0);
}

async fn receive(
    mut receiver: GossipReceiver,
    stats: Arc<Mutex<NodeStats>>,
    ended: Arc<std::sync::atomic::AtomicBool>,
) {
    while let Some(Ok(event)) = receiver.next().await {
        let mut s = stats.lock().unwrap();
        match event {
            Event::Received(message) => {
                let c = &message.content;
                if c.len() < 20 {
                    continue;
                }
                let publisher = u16::from_be_bytes([c[0], c[1]]);
                let t = u16::from_be_bytes([c[2], c[3]]);
                let seq = u64::from_be_bytes(c[4..12].try_into().unwrap());
                let ts = u64::from_be_bytes(c[12..20].try_into().unwrap());
                if s.seen.insert((publisher, t, seq)) {
                    let age = now_nanos().saturating_sub(ts) as f64 / 1e6;

                    let late = ended.load(std::sync::atomic::Ordering::Relaxed);
                    s.ages.push((age, late));
                }
            }
            Event::NeighborUp(peer) => {
                s.neighbor_up += 1;
                s.neighbors.insert(peer);
            }
            Event::NeighborDown(peer) => {
                s.neighbor_down += 1;
                s.neighbors.remove(&peer);
            }
            Event::Lagged => s.lagged += 1,
        }
    }
}

/// Sets `Delivery::Newest` on `builder`, on revisions that have it.
///
/// Build with `--features newest` for such a revision.
#[cfg(feature = "newest")]
fn newest(builder: iroh_gossip::net::Builder) -> iroh_gossip::net::Builder {
    builder.send_queue(
        iroh_gossip::net::QueueOptions::default().delivery(iroh_gossip::net::Delivery::Newest),
    )
}

#[cfg(not(feature = "newest"))]
fn newest(_builder: iroh_gossip::net::Builder) -> iroh_gossip::net::Builder {
    panic!("SLOWBENCH_NEWEST needs a build with the feature newest")
}

fn max_rss_mb() -> f64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find(|l| l.starts_with("VmHWM:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|kb| kb.parse::<f64>().ok())
        .map(|kb| kb / 1024.0)
        .unwrap_or(f64::NAN)
}

/// Returns the counter `name` of one node, or 0 if its version has none.
fn metric(node: &Node, name: &str) -> i128 {
    node.gossip
        .metrics()
        .iter()
        .find(|item| item.name() == name)
        .and_then(|item| match item.value() {
            MetricValue::Counter(v) => Some(v as i128),
            _ => None,
        })
        .unwrap_or(0)
}
