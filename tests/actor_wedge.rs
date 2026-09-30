//! Regression test for the actor wedge described in #47 / fixed by #143.
//!
//! One "zombie" neighbor, a peer that completes the gossip membership
//! handshake and is promoted into the active view, then stops reading
//! from its connection while keeping it alive at the QUIC level (think:
//! laptop lid closed, NAT-degraded path, half-open connection) must not
//! take down gossip functionality that has nothing to do with that peer.
//!
//! Mechanism: the zombie's per-connection `SendLoop` blocks in
//! `write_all().await` once the zombie's stream flow-control credit is
//! exhausted. The per-peer send queue (`SEND_QUEUE_CAP = 64`) then fills,
//! and the actor's `OutEvent::SendMessage` dispatch, `.send(message).await`
//! on that bounded channel, blocks the actor's single `tokio::select!`
//! loop forever (a bounded-mpsc `.send().await` never returns `Full`).
//! From then on every API call into the actor hangs, on all topics.
//!
//! The test makes no claim about the flooding topic itself; backpressure
//! there could be a defensible design choice. It asserts only *collateral*
//! liveness: while topic A drains into a zombie, a broadcast on an
//! unrelated topic B must still reach a healthy peer end-to-end.
//!
//! Everything runs with **default** transport and gossip configs; the
//! zombie speaks the real wire protocol via the public `proto::State`
//! machine, so the victim genuinely promotes it into the active view.
//! Nothing is mocked and no windows are shrunk; the flood just has to
//! exceed the default 1.25 MiB stream receive window plus the 64-message
//! send queue, which ~400 near-max-size (4 KiB) messages do.
//!
//! On an unpatched tree this test fails: the probe on topic B never
//! reaches the healthy peer because the actor is blocked on the zombie's
//! full send queue. With the #143 fix (`try_send` + drop the peer after K
//! consecutive `Full`s) it passes.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use iroh::address_lookup::memory::MemoryLookup;
use iroh::endpoint::{presets, Connection, RecvStream, SendStream};
use iroh::protocol::Router;
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode, SecretKey, TransportAddr};
use iroh_gossip::api::{Event as ApiEvent, GossipReceiver};
use iroh_gossip::net::Gossip;
use iroh_gossip::proto::{
    Command, Config as ProtoConfig, Event as ProtoEvent, InEvent, Message as WireMessage, OutEvent,
    PeerData, State as ProtoState, TopicId,
};
use n0_error::{Result, StdResultExt};
use n0_future::StreamExt;
use rand::rngs::StdRng;
use rand::SeedableRng;
use tokio::time::timeout;
use tracing::{debug, info};

/// Just under `DEFAULT_MAX_MESSAGE_SIZE` (4096), leaving header room.
const PAYLOAD_SIZE: usize = 3800;
/// Enough to exhaust the default 1.25 MiB stream receive window (~330
/// payloads) plus the per-peer send queue (64) several times over.
const FLOOD_MESSAGES: usize = 1500;
/// How long the collateral-liveness probe may take end-to-end. Generous:
/// on a live actor this is a couple of loopback round-trips.
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

#[tokio::test(flavor = "multi_thread")]
async fn zombie_neighbor_does_not_block_unrelated_topics() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,iroh=warn".into()),
        )
        .try_init()
        .ok();

    let flood_topic = TopicId::from_bytes([0xAB; 32]);
    let probe_topic = TopicId::from_bytes([0xCD; 32]);
    let lookup = MemoryLookup::new();

    // Zombie: default transport config, no shrunken windows, nothing
    // artificial. It simply stops reading after the handshake.
    let zombie_endpoint = Endpoint::builder(presets::Minimal)
        .secret_key(SecretKey::generate())
        .relay_mode(RelayMode::Disabled)
        .alpns(vec![iroh_gossip::ALPN.to_vec()])
        .bind()
        .await
        .std_context("bind zombie endpoint")?;
    let zombie_id = zombie_endpoint.id();
    lookup.add_endpoint_info(loopback_addr(&zombie_endpoint));
    tokio::spawn({
        let endpoint = zombie_endpoint.clone();
        async move {
            if let Err(err) = run_zombie(endpoint, flood_topic).await {
                debug!("zombie task ended: {err:#}");
            }
        }
    });

    // Victim: a real gossip node, member of both topics.
    let (victim_endpoint, victim_gossip, _victim_router) = gossip_node(&lookup).await?;
    // Healthy bystander: a real gossip node on the probe topic only.
    let (bystander_endpoint, bystander_gossip, _bystander_router) = gossip_node(&lookup).await?;

    // Bystander joins the probe topic with the victim as bootstrap.
    let victim_id = victim_endpoint.id();
    let bystander_join = tokio::spawn({
        let gossip = bystander_gossip.clone();
        async move {
            gossip
                .subscribe_and_join(probe_topic, vec![victim_id])
                .await
        }
    });
    let victim_probe_topic = timeout(
        Duration::from_secs(30),
        victim_gossip.subscribe_and_join(probe_topic, vec![]),
    )
    .await
    .std_context("timed out joining probe topic")??;
    let bystander_probe_topic = timeout(Duration::from_secs(30), bystander_join)
        .await
        .std_context("timed out waiting for bystander join")?
        .std_context("bystander join task panicked")?
        .std_context("bystander failed to join probe topic")?;
    let (probe_sender, _victim_probe_receiver) = victim_probe_topic.split();
    let (_bystander_sender, mut bystander_receiver) = bystander_probe_topic.split();
    info!("probe topic joined: victim <-> bystander");

    // Sanity: the probe path works before the zombie enters the picture.
    probe_sender
        .broadcast(Bytes::from_static(b"pre-flood probe"))
        .await
        .std_context("pre-flood probe broadcast")?;
    wait_for_message(&mut bystander_receiver, b"pre-flood probe", PROBE_TIMEOUT)
        .await
        .std_context("pre-flood probe was not received: harness problem, not the bug")?;
    info!("pre-flood probe delivered");

    // Victim joins the flood topic with the zombie as bootstrap and waits
    // until the zombie is an active-view neighbor.
    let flood_topic_handle = timeout(
        Duration::from_secs(30),
        victim_gossip.subscribe_and_join(flood_topic, vec![zombie_id]),
    )
    .await
    .std_context("timed out joining flood topic with the zombie as bootstrap")??;
    info!(zombie = %zombie_id.fmt_short(), "zombie joined as neighbor");
    let (flood_sender, mut flood_receiver) = flood_topic_handle.split();
    tokio::spawn(async move { while flood_receiver.next().await.is_some() {} });

    // Flood the zombie's topic, fire-and-forget. No assertion is made on
    // these sends: the test only requires that they cannot wedge the rest
    // of the node. The counter lets us observe stall vs. completion.
    let progress = Arc::new(AtomicUsize::new(0));
    let flood_task = tokio::spawn({
        let progress = progress.clone();
        async move {
            let payload = Bytes::from(vec![0x5A; PAYLOAD_SIZE]);
            for _ in 0..FLOOD_MESSAGES {
                if flood_sender.broadcast(payload.clone()).await.is_err() {
                    break;
                }
                progress.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    // Wait until the flood either finishes or stalls (no progress for 3 s;
    // on an unpatched tree it stalls once the zombie's stream credit and
    // the per-peer send queue are full).
    let mut last = 0usize;
    loop {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let now = progress.load(Ordering::Relaxed);
        if flood_task.is_finished() {
            info!(sent = now, "flood completed");
            break;
        }
        if now == last {
            info!(sent = now, "flood stalled (sender backpressured)");
            break;
        }
        last = now;
    }

    // Collateral liveness: a broadcast on the unrelated probe topic must
    // still reach the healthy bystander end-to-end.
    let probe_result: Result<()> = async {
        probe_sender
            .broadcast(Bytes::from_static(b"mid-flood probe"))
            .await
            .std_context("mid-flood probe broadcast did not complete")?;
        wait_for_message(&mut bystander_receiver, b"mid-flood probe", PROBE_TIMEOUT)
            .await
            .std_context("mid-flood probe was not received by the healthy peer")?;
        Ok(())
    }
    .await;

    // Tear the zombie down before asserting, so a failure doesn't leak the
    // wedged tasks past the test.
    zombie_endpoint.close().await;
    flood_task.abort();

    if let Err(err) = probe_result {
        panic!(
            "gossip actor wedged by one zombie neighbor: unrelated topic lost \
             end-to-end liveness while topic {} drained into a non-reading peer \
             (see #47 / #143): {err:#}",
            flood_topic.fmt_short()
        );
    }
    info!("collateral liveness held: unrelated topic delivered during flood");

    bystander_endpoint.close().await;
    victim_endpoint.close().await;
    Ok(())
}

async fn gossip_node(lookup: &MemoryLookup) -> Result<(Endpoint, Gossip, Router)> {
    let endpoint = Endpoint::builder(presets::Minimal)
        .secret_key(SecretKey::generate())
        .relay_mode(RelayMode::Disabled)
        .bind()
        .await
        .std_context("bind endpoint")?;
    endpoint
        .address_lookup()
        .std_context("endpoint closed")?
        .add(lookup.clone());
    lookup.add_endpoint_info(loopback_addr(&endpoint));
    let gossip = Gossip::builder().spawn(endpoint.clone());
    let router = Router::builder(endpoint.clone())
        .accept(iroh_gossip::ALPN, gossip.clone())
        .spawn();
    Ok((endpoint, gossip, router))
}

fn loopback_addr(endpoint: &Endpoint) -> EndpointAddr {
    let sockets: Vec<TransportAddr> = endpoint
        .bound_sockets()
        .into_iter()
        .map(|mut sa| {
            if sa.ip().is_unspecified() {
                let loopback = if sa.is_ipv4() {
                    std::net::IpAddr::from([127, 0, 0, 1])
                } else {
                    std::net::IpAddr::from(std::net::Ipv6Addr::LOCALHOST)
                };
                sa.set_ip(loopback);
            }
            TransportAddr::Ip(sa)
        })
        .collect();
    assert!(!sockets.is_empty(), "endpoint has no bound sockets");
    EndpointAddr::from_parts(endpoint.id(), sockets)
}

async fn wait_for_message(
    receiver: &mut GossipReceiver,
    expected: &[u8],
    budget: Duration,
) -> Result<()> {
    timeout(budget, async {
        while let Some(event) = receiver.next().await {
            if let Ok(ApiEvent::Received(msg)) = event {
                if msg.content.as_ref() == expected {
                    return Ok(());
                }
            }
        }
        Err(std::io::Error::other("receiver stream ended")).std_context("event stream")
    })
    .await
    .std_context("timed out waiting for message")?
}

/// Accepts gossip connections, completes the membership handshake via the
/// public `proto::State` machine so the victim promotes us into its active
/// view, then stops reading while keeping the connection open. The
/// endpoint driver keeps ACKing at the transport level, so the connection
/// stays alive while stream flow-control credit is never returned.
async fn run_zombie(endpoint: Endpoint, topic: TopicId) -> Result<()> {
    let me = endpoint.id();
    let mut state: ProtoState<EndpointId, StdRng> = ProtoState::new(
        me,
        PeerData::new(Vec::<u8>::new()),
        ProtoConfig::default(),
        StdRng::seed_from_u64(0xB0B),
    );
    // Prepare the topic state so incoming joins are answered.
    let _ = state
        .handle(
            InEvent::Command(topic, Command::Join(vec![])),
            tokio::time::Instant::now(),
            None,
        )
        .count();
    let state = Arc::new(tokio::sync::Mutex::new(state));

    loop {
        let Some(incoming) = endpoint.accept().await else {
            return Ok(());
        };
        let conn = match incoming.await {
            Ok(conn) => conn,
            Err(err) => {
                debug!("zombie: incoming connection failed: {err:#}");
                continue;
            }
        };
        let state = state.clone();
        tokio::spawn(async move {
            let remote = conn.remote_id();
            if let Err(err) = zombie_conn(conn, topic, remote, state).await {
                debug!("zombie conn ended: {err:#}");
            }
        });
    }
}

async fn zombie_conn(
    conn: Connection,
    topic: TopicId,
    remote: EndpointId,
    state: Arc<tokio::sync::Mutex<ProtoState<EndpointId, StdRng>>>,
) -> Result<()> {
    let mut recv = conn.accept_uni().await.std_context("accept_uni")?;

    // Per-topic stream header: length-prefixed postcard StreamHeader, whose
    // encoding is byte-identical to the bare 32-byte TopicId.
    let header = read_frame_bytes(&mut recv)
        .await
        .std_context("read header")?;
    assert_eq!(
        header.as_slice(),
        topic.as_bytes().as_slice(),
        "unexpected topic in stream header"
    );

    let mut send: Option<SendStream> = None;
    loop {
        let payload = read_frame_bytes(&mut recv)
            .await
            .std_context("read frame")?;
        // Reconstruct the state-level Message: postcard(Message{topic,inner})
        // == 32 topic bytes ++ postcard(inner) (postcard structs concatenate
        // fields with no framing).
        let mut buf = Vec::with_capacity(32 + payload.len());
        buf.extend_from_slice(topic.as_bytes());
        buf.extend_from_slice(&payload);
        let msg: WireMessage<EndpointId> =
            postcard::from_bytes(&buf).std_context("decode gossip message")?;

        let outs: Vec<OutEvent<EndpointId>> = state
            .lock()
            .await
            .handle(
                InEvent::RecvMessage(remote, msg),
                tokio::time::Instant::now(),
                None,
            )
            .collect();

        let mut neighbor_up = false;
        for out in outs {
            match out {
                OutEvent::SendMessage(_to, msg) => {
                    let bytes = postcard::to_stdvec(&msg).std_context("encode gossip message")?;
                    assert!(bytes.len() > 32 && &bytes[..32] == topic.as_bytes().as_slice());
                    let stream = match send.as_mut() {
                        Some(s) => s,
                        None => {
                            let mut s = conn.open_uni().await.std_context("open_uni")?;
                            write_frame_bytes(&mut s, topic.as_bytes()).await?;
                            send.insert(s)
                        }
                    };
                    write_frame_bytes(stream, &bytes[32..]).await?;
                }
                OutEvent::EmitEvent(_, ProtoEvent::NeighborUp(peer)) => {
                    info!(peer = %peer.fmt_short(), "zombie: neighbor up, going silent now");
                    neighbor_up = true;
                }
                // Timers, peer data updates, disconnects: irrelevant for the
                // short handshake; the zombie never services them.
                _ => {}
            }
        }
        if neighbor_up {
            break;
        }
    }

    // Zombie mode: stop reading, hold the connection and streams open
    // forever. The endpoint driver keeps ACKing at the transport level.
    let () = std::future::pending().await;
    drop(recv);
    drop(send);
    drop(conn);
    unreachable!()
}

async fn read_frame_bytes(recv: &mut RecvStream) -> Result<Vec<u8>> {
    let mut len4 = [0u8; 4];
    recv.read_exact(&mut len4)
        .await
        .std_context("read frame len")?;
    let len = u32::from_be_bytes(len4) as usize;
    assert!(len <= 1024 * 1024, "frame too large: {len}");
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf)
        .await
        .std_context("read frame body")?;
    Ok(buf)
}

async fn write_frame_bytes(send: &mut SendStream, payload: &[u8]) -> Result<()> {
    send.write_all(&(payload.len() as u32).to_be_bytes())
        .await
        .std_context("write frame len")?;
    send.write_all(payload)
        .await
        .std_context("write frame body")?;
    Ok(())
}
