//! Shared helpers for the patchbay gossip tests.
//!
//! A test starts with [`lab_with_relay`], which builds a [`Lab`] holding a
//! relay server reachable at `https://relay.test`, then spawns [`Node`]s onto
//! devices in that lab.
//!
//! A [`Node`] owns an iroh endpoint and a gossip instance that live on the
//! device's namespace runtime, but the handles it hands back ([`Gossip`],
//! [`Endpoint`], [`GossipTopic`]) are channel clients that work from anywhere.
//! Tests therefore read as a linear script in the test function itself, with
//! no per-peer closures, and can interleave protocol calls with topology
//! changes such as [`Node::link_down`].

use std::{
    collections::HashSet,
    net::{IpAddr, Ipv6Addr},
    path::PathBuf,
    time::Duration,
};

use bytes::Bytes;
use iroh::{
    address_lookup::memory::MemoryLookup, endpoint::presets, protocol::Router, tls::CaTlsConfig,
    Endpoint, EndpointAddr, EndpointId, RelayMap, RelayMode,
};
use iroh_gossip::{
    api::{Event, GossipTopic},
    net::{Gossip, GOSSIP_ALPN},
    proto::TopicId,
};
use iroh_relay::{
    server::{
        CertConfig, QuicConfig, RelayConfig as RelayServerConfig, Server, ServerConfig, TlsConfig,
    },
    RelayConfig, RelayQuicConfig,
};
use n0_error::{Result, StdResultExt};
use n0_future::{task::AbortOnDropHandle, StreamExt};
use patchbay::{Device, Iface, IpSupport, Lab, LinkCondition, LinkDirection, TestGuard};
use tokio::sync::oneshot;
use tracing::info;

/// The hostname the in-lab relay server is reachable at.
const RELAY_HOST: &str = "relay.test";

// ---------------------------------------------------------------------------
// Lab setup
// ---------------------------------------------------------------------------

/// Builds a lab with a dual-stack relay server, ready for [`Node::spawn`].
///
/// The relay runs on its own device behind a public router and is reachable at
/// `https://relay.test` over both IPv4 and IPv6. Pass a per-test directory,
/// typically [`testdir!`](https://docs.rs/testdir), for the lab's event log.
///
/// Call [`TestGuard::ok`] at the end of a passing test so the lab records the
/// run as successful.
pub async fn lab_with_relay(dir: PathBuf) -> Result<(Lab, TestGuard, Relay)> {
    let (lab, guard) = Lab::for_test(dir).await?;

    // Start the DNS server before any endpoint binds. iroh's resolver reads
    // /etc/resolv.conf once, when it is constructed, and the lab only points
    // the overlay at its own server on this call.
    let dns = lab.dns_server()?;

    let dc = lab
        .add_router("dc")
        .ip_support(IpSupport::DualStack)
        .build()
        .await?;
    let device = lab.add_device("relay").uplink(dc.id()).build().await?;

    let ipv4 = device.ip().expect("relay device has an IPv4 address");
    let ipv6 = device.ip6().expect("relay device has an IPv6 address");
    dns.set_host(RELAY_HOST, IpAddr::V4(ipv4))?;
    dns.set_host(RELAY_HOST, IpAddr::V6(ipv6))?;

    let (map_tx, map_rx) = oneshot::channel();
    let task = device.spawn(move |_dev| async move {
        match spawn_relay_server().await {
            Ok((map, _server)) => {
                map_tx.send(Ok(map)).ok();
                // Hold the server alive for as long as the lab runs.
                std::future::pending::<()>().await;
            }
            Err(err) => {
                map_tx.send(Err(err)).ok();
            }
        }
    })?;

    let map = map_rx.await.anyerr()??;
    info!(?map, "relay server up");
    Ok((
        lab,
        guard,
        Relay {
            map,
            _task: AbortOnDropHandle::new(task),
        },
    ))
}

/// A relay server running on its own device inside the lab.
#[derive(Debug)]
pub struct Relay {
    map: RelayMap,
    _task: AbortOnDropHandle<()>,
}

impl Relay {
    /// Returns the [`RelayMap`] that points at this relay.
    pub fn map(&self) -> RelayMap {
        self.map.clone()
    }
}

/// Starts a relay server on the ports `relay.test` is expected to serve.
///
/// Binds `[::]` so the dual-stack device answers on both families. The
/// certificate is self-signed, which is why nodes are built with
/// [`CaTlsConfig::insecure_skip_verify`].
async fn spawn_relay_server() -> Result<(RelayMap, Server), iroh_relay::server::SpawnError> {
    let bind_ip: IpAddr = Ipv6Addr::UNSPECIFIED.into();
    let (_certs, server_config) = iroh_relay::server::testing::self_signed_tls_certs_and_config();

    let mut relay = RelayServerConfig::new((bind_ip, 80));
    relay.tls = Some(TlsConfig::new(
        (bind_ip, 443),
        CertConfig::Manual { server_config },
    ));
    relay.key_cache_capacity = Some(1024);

    let mut config = ServerConfig::default();
    config.relay = Some(relay);
    config.quic = Some(QuicConfig::new((bind_ip, 7842)));

    let server = Server::spawn(config).await?;

    let url = format!("https://{RELAY_HOST}")
        .parse()
        .expect("valid relay url");
    let quic = server
        .quic_addr()
        .map(|addr| RelayQuicConfig::new(addr.port()));
    let map: RelayMap = RelayConfig::new(url, quic).into();
    Ok((map, server))
}

// ---------------------------------------------------------------------------
// Nodes
// ---------------------------------------------------------------------------

/// A gossip node running inside a patchbay device namespace.
#[derive(Debug)]
pub struct Node {
    device: Device,
    gossip: Gossip,
    addr: EndpointAddr,
    lookup: MemoryLookup,
    shutdown: oneshot::Sender<()>,
    task: AbortOnDropHandle<()>,
}

impl Node {
    /// Spawns an endpoint and a gossip instance on `device`.
    ///
    /// Returns once the endpoint is online, so [`Node::relay_addr`] is
    /// immediately usable as a bootstrap address for other nodes.
    pub async fn spawn(device: &Device, relay: &Relay) -> Result<Self> {
        let relay_map = relay.map();
        let lookup = MemoryLookup::new();
        let lookup2 = lookup.clone();
        let (ready_tx, ready_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let task = device.spawn(move |_dev| async move {
            let started = async {
                let endpoint = Endpoint::builder(presets::Minimal)
                    .relay_mode(RelayMode::Custom(relay_map))
                    .ca_tls_config(CaTlsConfig::insecure_skip_verify())
                    .alpns(vec![GOSSIP_ALPN.to_vec()])
                    .bind()
                    .await?;
                endpoint.online().await;
                endpoint.address_lookup()?.add(lookup2);
                let gossip = Gossip::builder().spawn(endpoint.clone());
                let router = Router::builder(endpoint.clone())
                    .accept(GOSSIP_ALPN, gossip.clone())
                    .spawn();
                n0_error::Ok((endpoint, gossip, router))
            }
            .await;

            let (endpoint, gossip, router) = match started {
                Ok(parts) => parts,
                Err(err) => {
                    ready_tx.send(Err(err)).ok();
                    return;
                }
            };
            let addr = endpoint.addr();
            if ready_tx.send(Ok((gossip, addr))).is_ok() {
                shutdown_rx.await.ok();
            }
            router.shutdown().await.ok();
            endpoint.close().await;
        })?;

        let (gossip, addr) = ready_rx.await.anyerr()??;
        info!(device = device.name(), id = %addr.id.fmt_short(), "node up");
        Ok(Self {
            device: device.clone(),
            gossip,
            addr,
            lookup,
            shutdown: shutdown_tx,
            task: AbortOnDropHandle::new(task),
        })
    }

    /// Returns this node's endpoint id.
    pub fn id(&self) -> EndpointId {
        self.addr.id
    }

    /// Returns this node's address with direct paths stripped.
    ///
    /// A device's direct addresses are namespace-local, so a peer that dials
    /// them only stalls on paths that can never connect. Relay paths are the
    /// only ones that work across devices in a lab.
    pub fn relay_addr(&self) -> EndpointAddr {
        EndpointAddr::from_parts(
            self.addr.id,
            self.addr
                .addrs
                .iter()
                .filter(|addr| addr.is_relay())
                .cloned(),
        )
    }

    /// Teaches this node how to reach `peer`.
    pub fn learn(&self, peer: &Node) {
        self.lookup.add_endpoint_info(peer.relay_addr());
    }

    /// Subscribes to `topic`, bootstrapping from `peers`.
    ///
    /// Each peer's address is added to this node's address book first, so the
    /// join has somewhere to dial. Returns without waiting for the join to
    /// complete; follow with [`GossipTopic::joined`] or [`wait_neighbor_up`].
    pub async fn subscribe(&self, topic: TopicId, peers: &[&Node]) -> Result<GossipTopic> {
        for peer in peers {
            self.learn(peer);
        }
        let bootstrap = peers.iter().map(|peer| peer.id()).collect();
        Ok(self.gossip.subscribe(topic, bootstrap).await?)
    }

    /// Returns the handle for the interface carrying this node's default route.
    pub fn iface(&self) -> Iface {
        self.device
            .default_iface()
            .expect("device has a default interface")
    }

    /// Takes this node's uplink down.
    ///
    /// Packets are dropped silently, which is what peers see when a device
    /// loses its network or crashes outright.
    pub async fn link_down(&self) -> Result {
        let iface = self.iface();
        // Linux flushes a link's IPv6 addresses when it goes down, and
        // patchbay 0.7 does not put them back on the way up: `link_up` then
        // fails with EHOSTUNREACH while installing the IPv6 default route.
        // Keeping the addresses across the transition sidesteps that.
        let path = format!("/proc/sys/net/ipv6/conf/{}/keep_addr_on_down", iface.name());
        self.device.run_sync(move || {
            std::fs::write(&path, "1")?;
            Ok(())
        })?;
        iface.link_down().await?;
        Ok(())
    }

    /// Brings this node's uplink back up.
    pub async fn link_up(&self) -> Result {
        self.iface().link_up().await?;
        Ok(())
    }

    /// Applies `condition` to this node's uplink in both directions.
    pub async fn impair(&self, condition: LinkCondition) -> Result {
        self.iface()
            .set_condition(condition, LinkDirection::Both)
            .await?;
        Ok(())
    }

    /// Removes any impairment from this node's uplink.
    pub async fn heal(&self) -> Result {
        self.iface().clear_condition(LinkDirection::Both).await?;
        Ok(())
    }

    /// Closes the endpoint and waits for the namespace task to finish.
    ///
    /// This is a clean departure: peers see the connection close rather than
    /// time out. For an unclean one, use [`Node::link_down`].
    pub async fn shutdown(self) -> Result {
        let Node { shutdown, task, .. } = self;
        shutdown.send(()).ok();
        task.await.anyerr()?;
        Ok(())
    }
}

/// Spawns a gossip node on each device, concurrently.
///
/// Each node waits for the relay before it reports ready, so spawning them in
/// sequence would add up to a noticeable share of a test's runtime.
pub async fn spawn_nodes(devices: &[Device], relay: &Relay) -> Result<Vec<Node>> {
    let nodes = devices.iter().map(|device| Node::spawn(device, relay));
    n0_future::try_join_all(nodes).await
}

// ---------------------------------------------------------------------------
// Event helpers
// ---------------------------------------------------------------------------

/// Waits for the next [`Event::NeighborUp`] and returns the neighbor's id.
pub async fn wait_neighbor_up(sub: &mut GossipTopic, timeout: Duration) -> Result<EndpointId> {
    next_matching(sub, timeout, "NeighborUp", |event| match event {
        Event::NeighborUp(id) => Some(id),
        _ => None,
    })
    .await
}

/// Waits for the next [`Event::Received`] and returns its payload.
pub async fn wait_message(sub: &mut GossipTopic, timeout: Duration) -> Result<Bytes> {
    next_matching(sub, timeout, "Received", |event| match event {
        Event::Received(message) => Some(message.content),
        _ => None,
    })
    .await
}

/// Waits until `count` distinct payloads have arrived and returns them.
///
/// Duplicates are folded away, so a test that asserts on the set does not have
/// to care whether the mesh delivered a message once or twice.
pub async fn collect_messages(
    sub: &mut GossipTopic,
    count: usize,
    timeout: Duration,
) -> Result<HashSet<Bytes>> {
    let mut received = HashSet::new();
    while received.len() < count {
        received.insert(wait_message(sub, timeout).await?);
    }
    Ok(received)
}

/// Drives `sub` until `f` matches an event, or `timeout` elapses.
///
/// `what` names the awaited event in the timeout error, which is otherwise the
/// least informative failure a gossip test can produce.
async fn next_matching<T>(
    sub: &mut GossipTopic,
    timeout: Duration,
    what: &str,
    f: impl Fn(Event) -> Option<T>,
) -> Result<T> {
    let found = tokio::time::timeout(timeout, async {
        loop {
            match sub.next().await {
                Some(Ok(event)) => {
                    if let Some(found) = f(event.clone()) {
                        return n0_error::Ok(found);
                    }
                    info!(?event, "skipping event while waiting for {what}");
                }
                Some(Err(err)) => return Err(err.into()),
                None => n0_error::bail_any!("subscription closed while waiting for {what}"),
            }
        }
    })
    .await
    .map_err(|_| n0_error::anyerr!("timed out after {timeout:?} waiting for {what}"))??;
    Ok(found)
}

/// Formats a message payload for assertions and log lines.
pub fn text(bytes: &Bytes) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Returns a stable topic id derived from `name`.
pub fn topic(name: &str) -> TopicId {
    blake3::hash(name.as_bytes()).into()
}

/// Returns the payloads a set of messages is expected to contain.
pub fn payloads<'a>(messages: impl IntoIterator<Item = &'a str>) -> HashSet<Bytes> {
    messages
        .into_iter()
        .map(|message| Bytes::from(message.as_bytes().to_vec()))
        .collect()
}

/// Drives every subscription until each one has at least `min_neighbors`.
///
/// A node's neighbor set only advances while its stream is polled, so the
/// subscriptions have to be driven together rather than one after another.
///
/// Waiting for a single `NeighborUp` per node is not enough to call a swarm
/// formed. A broadcast reaches the overlay as it exists at send time, and
/// gossip has no catch-up for nodes that attach afterwards, so sending into a
/// half-built swarm silently loses messages.
///
/// Events seen while waiting are discarded, so call this before anyone
/// broadcasts.
pub async fn wait_swarm_ready(
    subs: &mut [GossipTopic],
    min_neighbors: usize,
    timeout: Duration,
) -> Result {
    /// How long to poll one subscription before moving on to the next.
    const POLL: Duration = Duration::from_millis(20);

    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let counts = || -> Vec<usize> { subs.iter().map(|sub| sub.neighbors().count()).collect() };
        if subs
            .iter()
            .all(|sub| sub.neighbors().count() >= min_neighbors)
        {
            info!(counts = ?counts(), "swarm ready");
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            n0_error::bail_any!(
                "timed out after {timeout:?} waiting for {min_neighbors} neighbors per node, have {:?}",
                counts()
            );
        }
        for sub in subs.iter_mut() {
            match tokio::time::timeout(POLL, sub.next()).await {
                Ok(Some(Ok(event))) => info!(?event, "swarm forming"),
                Ok(Some(Err(err))) => return Err(err.into()),
                Ok(None) => n0_error::bail_any!("subscription closed while forming the swarm"),
                Err(_) => {}
            }
        }
    }
}
