//! Gossip tests against real kernel networking, via patchbay.
//!
//! Each test builds a lab with an in-lab relay, spawns gossip nodes onto
//! devices behind routers with realistic NAT and link behaviour, and asserts
//! on what the gossip API reports. See [`util`] for the helpers.

// patchbay only runs on linux
#![cfg(target_os = "linux")]
// Allow to skip netsim tests (e.g. in cross-compiled CI)
#![cfg(not(iroh_skip_netsim))]

use std::time::Duration;

use iroh::{endpoint::presets, tls::CaTlsConfig, Endpoint, RelayMode};
use iroh_gossip::{api::GossipTopic, proto::TopicId};
use n0_error::{Result, StdResultExt};
use n0_tracing_test::traced_test;
use patchbay::{Lab, LinkCondition, RouterPreset};
use testdir::testdir;
use tokio::sync::oneshot;
use tracing::info;

use self::util::{
    collect_messages, lab_with_relay, payloads, spawn_nodes, text, topic, wait_message,
    wait_neighbor_up, wait_swarm_ready, Node, Relay,
};

#[path = "patchbay/util.rs"]
mod util;

/// Init the user namespace before any threads are spawned.
#[ctor::ctor(unsafe)]
fn userns_ctor() {
    unsafe {
        patchbay::init_userns_for_ctor();
    }
}

// ---------------------------------------------------------------------------
// Basic connectivity
// ---------------------------------------------------------------------------

/// Sanity check: a raw iroh endpoint can connect across the lab.
///
/// Nothing gossip-specific here. When this fails, the lab or the relay is
/// broken, not the gossip protocol, and the rest of the file is noise.
#[tokio::test]
#[traced_test]
async fn iroh_connect_smoke() -> Result {
    let (lab, guard, relay) = lab_with_relay(testdir!()).await?;
    let public = lab
        .add_router("public")
        .preset(RouterPreset::Public)
        .build()
        .await?;
    let dev1 = lab.add_device("dev1").uplink(public.id()).build().await?;
    let dev2 = lab.add_device("dev2").uplink(public.id()).build().await?;

    let alpn = b"test-alpn";
    let (addr_tx, addr_rx) = oneshot::channel();
    let relay_map = relay.map();
    let relay_map2 = relay.map();

    let accept = dev1.spawn(move |_dev| async move {
        let endpoint = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Custom(relay_map))
            .ca_tls_config(CaTlsConfig::insecure_skip_verify())
            .alpns(vec![alpn.to_vec()])
            .bind()
            .await?;
        endpoint.online().await;
        addr_tx.send(endpoint.addr()).ok();
        let incoming = endpoint.accept().await.expect("endpoint is open");
        let conn = incoming.accept().anyerr()?.await.anyerr()?;
        info!("dev1: accepted connection");
        conn.closed().await;
        endpoint.close().await;
        n0_error::Ok(())
    })?;

    let connect = dev2.spawn(move |_dev| async move {
        let endpoint = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Custom(relay_map2))
            .ca_tls_config(CaTlsConfig::insecure_skip_verify())
            .alpns(vec![alpn.to_vec()])
            .bind()
            .await?;
        let addr = addr_rx.await.anyerr()?;
        info!(?addr, "dev2: connecting to dev1");
        let conn = endpoint.connect(addr, alpn).await?;
        info!("dev2: connected");
        conn.close(0u32.into(), b"done");
        endpoint.close().await;
        n0_error::Ok(())
    })?;

    tokio::time::timeout(Duration::from_secs(15), async {
        connect.await.anyerr()??;
        accept.await.anyerr()??;
        n0_error::Ok(())
    })
    .await
    .anyerr()??;

    guard.ok();
    Ok(())
}

// ---------------------------------------------------------------------------
// Two peers
// ---------------------------------------------------------------------------

/// Two peers behind separate home NATs exchange a message in both direction.
///
/// The smallest end-to-end gossip test: join, mesh, deliver. When the swarm
/// tests below fail and this one passes, the problem is in forwarding rather
/// than in the transport or the join handshake.
#[tokio::test]
#[traced_test]
async fn gossip_two_peers() -> Result {
    let (lab, guard, relay) = lab_with_relay(testdir!()).await?;
    let nat1 = lab
        .add_router("nat1")
        .preset(RouterPreset::Home)
        .build()
        .await?;
    let nat2 = lab
        .add_router("nat2")
        .preset(RouterPreset::Home)
        .build()
        .await?;
    let dev1 = lab.add_device("dev1").uplink(nat1.id()).build().await?;
    let dev2 = lab.add_device("dev2").uplink(nat2.id()).build().await?;

    let topic = topic("two_peers");
    let timeout = Duration::from_secs(15);
    let nodes = spawn_nodes(&[dev1, dev2], &relay).await?;
    let (peer1, peer2) = (&nodes[0], &nodes[1]);

    let mut sub1 = peer1.subscribe(topic, &[]).await?;
    let mut sub2 = peer2.subscribe(topic, &[peer1]).await?;

    assert_eq!(wait_neighbor_up(&mut sub1, timeout).await?, peer2.id());
    assert_eq!(wait_neighbor_up(&mut sub2, timeout).await?, peer1.id());
    info!("mesh formed");

    sub2.broadcast(b"hello from peer2".to_vec().into()).await?;
    let message = wait_message(&mut sub1, timeout).await?;
    assert_eq!(text(&message), "hello from peer2");

    sub1.broadcast(b"hello from peer1".to_vec().into()).await?;
    let message = wait_message(&mut sub2, timeout).await?;
    assert_eq!(text(&message), "hello from peer1");

    guard.ok();
    Ok(())
}

// ---------------------------------------------------------------------------
// Swarms
// ---------------------------------------------------------------------------

/// Every node in a mixed-NAT swarm receives every other node's broadcast.
///
/// The swarm is deliberately larger than `active_view_capacity` (5), so no node
/// is a neighbor of every other and messages have to be forwarded. A
/// three-node swarm would be fully connected and would never exercise
/// forwarding at all.
#[tokio::test]
#[traced_test]
async fn gossip_swarm_broadcast() -> Result {
    /// The NAT class each node sits behind, cycled over the swarm.
    const PRESETS: [RouterPreset; 3] = [
        RouterPreset::Home,
        RouterPreset::Public,
        RouterPreset::Corporate,
    ];
    const NODES: usize = 8;

    let (lab, guard, relay) = lab_with_relay(testdir!()).await?;
    let mut devices = Vec::with_capacity(NODES);
    for idx in 0..NODES {
        let router = lab
            .add_router(&format!("router{idx}"))
            .preset(PRESETS[idx % PRESETS.len()])
            .build()
            .await?;
        devices.push(
            lab.add_device(&format!("dev{idx}"))
                .uplink(router.id())
                .build()
                .await?,
        );
    }

    let topic = topic("swarm_broadcast");
    let timeout = Duration::from_secs(60);

    let nodes = spawn_nodes(&devices, &relay).await?;
    let bootstrap = &nodes[0];

    let mut subs = vec![bootstrap.subscribe(topic, &[]).await?];
    for node in &nodes[1..] {
        subs.push(node.subscribe(topic, &[bootstrap]).await?);
    }

    // Each node should end up with close to `active_view_capacity` neighbors
    // out of the 7 on offer. Three is a lenient bar that still means the
    // overlay has settled rather than just started forming.
    wait_swarm_ready(&mut subs, 3, timeout).await?;

    let messages: Vec<String> = (0..NODES).map(|idx| format!("from-peer-{idx}")).collect();
    for (sub, message) in subs.iter_mut().zip(&messages) {
        sub.broadcast(message.clone().into_bytes().into()).await?;
    }

    for (idx, sub) in subs.iter_mut().enumerate() {
        let received = collect_messages(sub, NODES - 1, timeout).await?;
        let expected = payloads(
            messages
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != idx)
                .map(|(_, message)| message.as_str()),
        );
        assert_eq!(received, expected, "peer {idx} missed a message");
    }

    guard.ok();
    Ok(())
}

// ---------------------------------------------------------------------------
// Impaired links
// ---------------------------------------------------------------------------

/// Link goes down and comes back; gossip reconnects and messages flow again.
#[tokio::test]
#[traced_test]
async fn gossip_link_down_and_recovery() -> Result {
    let (lab, guard, relay) = lab_with_relay(testdir!()).await?;
    let (peer1, peer2, mut sub1, mut sub2) =
        two_peers_joined(&lab, &relay, topic("link_down_recovery")).await?;
    let timeout = Duration::from_secs(60);

    sub2.broadcast(b"before-down".to_vec().into()).await?;
    let message = wait_message(&mut sub1, timeout).await?;
    assert_eq!(text(&message), "before-down");
    info!("delivery works before the link goes down");

    peer2.link_down().await?;
    info!("peer2 link is down");
    tokio::time::sleep(Duration::from_secs(1)).await;

    peer2.link_up().await?;
    info!("peer2 link is up again");

    // Gossip has to notice the neighbor is gone and dial it again through the
    // relay before a broadcast can land.
    tokio::time::sleep(Duration::from_secs(3)).await;
    sub2.broadcast(b"after-recovery".to_vec().into()).await?;
    let message = wait_message(&mut sub1, timeout).await?;
    assert_eq!(text(&message), "after-recovery");

    peer1.shutdown().await?;
    peer2.shutdown().await?;
    guard.ok();
    Ok(())
}

/// Repeated down/up cycles do not leave gossip stuck: once the link settles,
/// messages flow again.
#[tokio::test]
#[traced_test]
async fn gossip_link_flap_recovery() -> Result {
    let (lab, guard, relay) = lab_with_relay(testdir!()).await?;
    let (peer1, peer2, mut sub1, mut sub2) =
        two_peers_joined(&lab, &relay, topic("link_flap")).await?;
    let timeout = Duration::from_secs(60);

    sub2.broadcast(b"pre-flap".to_vec().into()).await?;
    let message = wait_message(&mut sub1, timeout).await?;
    assert_eq!(text(&message), "pre-flap");

    for round in 0..3 {
        peer2.link_down().await?;
        info!(round, "link down");
        tokio::time::sleep(Duration::from_millis(500)).await;
        peer2.link_up().await?;
        info!(round, "link up");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    tokio::time::sleep(Duration::from_secs(5)).await;
    sub2.broadcast(b"post-flap".to_vec().into()).await?;
    let message = wait_message(&mut sub1, timeout).await?;
    assert_eq!(text(&message), "post-flap");

    peer1.shutdown().await?;
    peer2.shutdown().await?;
    guard.ok();
    Ok(())
}

/// A link degrades to 4G mid-session and then heals; delivery survives both
/// transitions.
#[tokio::test]
#[traced_test]
async fn gossip_dynamic_link_degradation() -> Result {
    let (lab, guard, relay) = lab_with_relay(testdir!()).await?;
    let (peer1, peer2, mut sub1, mut sub2) =
        two_peers_joined(&lab, &relay, topic("dynamic_degradation")).await?;
    let timeout = Duration::from_secs(30);

    sub2.broadcast(b"clean-link".to_vec().into()).await?;
    let message = wait_message(&mut sub1, timeout).await?;
    assert_eq!(text(&message), "clean-link");

    peer1.impair(LinkCondition::mobile_4g()).await?;
    peer2.impair(LinkCondition::mobile_4g()).await?;
    info!("applied 4G link conditions");

    sub2.broadcast(b"degraded-link".to_vec().into()).await?;
    let message = wait_message(&mut sub1, timeout).await?;
    assert_eq!(text(&message), "degraded-link");

    peer1.heal().await?;
    peer2.heal().await?;
    info!("removed link impairment");

    sub2.broadcast(b"healed-link".to_vec().into()).await?;
    let message = wait_message(&mut sub1, timeout).await?;
    assert_eq!(text(&message), "healed-link");

    peer1.shutdown().await?;
    peer2.shutdown().await?;
    guard.ok();
    Ok(())
}

/// Builds two nodes behind separate home NATs, joins them to `topic`, and
/// returns once both report a neighbor.
async fn two_peers_joined(
    lab: &Lab,
    relay: &Relay,
    topic: TopicId,
) -> Result<(Node, Node, GossipTopic, GossipTopic)> {
    let timeout = Duration::from_secs(60);
    let nat1 = lab
        .add_router("nat1")
        .preset(RouterPreset::Home)
        .build()
        .await?;
    let nat2 = lab
        .add_router("nat2")
        .preset(RouterPreset::Home)
        .build()
        .await?;
    let dev1 = lab.add_device("dev1").uplink(nat1.id()).build().await?;
    let dev2 = lab.add_device("dev2").uplink(nat2.id()).build().await?;

    let mut nodes = spawn_nodes(&[dev1, dev2], relay).await?;
    let peer2 = nodes.pop().expect("two nodes");
    let peer1 = nodes.pop().expect("two nodes");

    let mut sub1 = peer1.subscribe(topic, &[]).await?;
    let mut sub2 = peer2.subscribe(topic, &[&peer1]).await?;
    wait_neighbor_up(&mut sub1, timeout).await?;
    wait_neighbor_up(&mut sub2, timeout).await?;

    Ok((peer1, peer2, sub1, sub2))
}
