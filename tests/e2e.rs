//! End-to-end tests of gossip between nodes in one process.
//!
//! The tests use only the public API, so they run against any version of the
//! crate. Nodes connect directly on this host, without a relay. Each node runs
//! its own accept loop. The loop keeps a weak handle to each connection, so that
//! a test can count the ones still open.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use bytes::Bytes;
use futures_concurrency::future::TryJoin;
use iroh::{
    address_lookup::memory::MemoryLookup,
    endpoint::{presets, Connection, IdleTimeout, QuicTransportConfig, WeakConnectionHandle},
    Endpoint, EndpointId, RelayMode, SecretKey,
};
use iroh_gossip::{
    api::{Event, GossipReceiver, GossipSender},
    net::{Builder, Gossip, GOSSIP_ALPN},
    proto::TopicId,
};
use n0_error::{bail_any, Result, StdResultExt};
use n0_future::{task::AbortOnDropHandle, StreamExt};
use n0_tracing_test::traced_test;
use tokio::time::{sleep, timeout};

/// How long a test waits for something that should happen promptly.
const PROMPT: Duration = Duration::from_secs(10);

/// The idle timeout of the test transport: a silent peer is gone after this.
const IDLE_TIMEOUT: Duration = Duration::from_secs(3);

/// A wait longer than any version keeps a connection it no longer uses.
///
/// That is the idle grace plus the wait for our finished streams to be
/// acknowledged, five seconds each, and a margin.
const SETTLE: Duration = Duration::from_secs(12);
/// How long a join that its peer dropped takes to succeed.
///
/// The join is sent again after the neighbor request timeout of 10 s.
const JOIN_RETRY_WITHIN: Duration = Duration::from_secs(20);

/// Decides whether a node takes a connection, before gossip sees it.
type AcceptFilter = Arc<dyn Fn(&Connection) -> bool + Send + Sync>;

/// A gossip node with its own endpoint and accept loop.
struct Node {
    secret: SecretKey,
    endpoint: Endpoint,
    gossip: Gossip,
    /// Every connection the node accepted, by peer.
    ///
    /// The handles are weak, as a router keeps none: a connection closes
    /// once gossip drops it.
    accepted: Arc<Mutex<Vec<(EndpointId, WeakConnectionHandle)>>>,
    _accept: AbortOnDropHandle<()>,
}

impl Node {
    /// Starts a node with a new key.
    async fn spawn(lookup: &MemoryLookup) -> Result<Self> {
        Self::spawn_with(lookup, SecretKey::generate()).await
    }

    /// Starts a node with `secret`, as a restarted node does.
    async fn spawn_with(lookup: &MemoryLookup, secret: SecretKey) -> Result<Self> {
        Self::spawn_custom(lookup, secret, Gossip::builder(), None).await
    }

    /// Starts a node with `secret`, gossip from `builder`, and an accept `filter`.
    ///
    /// The node closes a connection the filter refuses, and does not count it as
    /// accepted.
    async fn spawn_custom(
        lookup: &MemoryLookup,
        secret: SecretKey,
        builder: Builder,
        filter: Option<AcceptFilter>,
    ) -> Result<Self> {
        let transport = QuicTransportConfig::builder()
            .keep_alive_interval(Duration::from_secs(1))
            .max_idle_timeout(Some(
                IdleTimeout::try_from(IDLE_TIMEOUT).std_context("idle timeout")?,
            ))
            .build();
        let endpoint = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .secret_key(secret.clone())
            .alpns(vec![GOSSIP_ALPN.to_vec()])
            .transport_config(transport)
            .bind()
            .await
            .std_context("bind")?;
        endpoint
            .address_lookup()
            .std_context("endpoint closed")?
            .add(lookup.clone());
        // A restarted node replaces the addresses of its former instance.
        lookup.set_endpoint_info(endpoint.addr());
        let gossip = builder.spawn(endpoint.clone());
        let accepted = Arc::new(Mutex::new(Vec::new()));
        let accept = {
            let endpoint = endpoint.clone();
            let gossip = gossip.clone();
            let accepted = accepted.clone();
            tokio::spawn(async move {
                while let Some(incoming) = endpoint.accept().await {
                    let Ok(conn) = incoming.await else {
                        continue;
                    };
                    if filter.as_ref().is_some_and(|accept| !accept(&conn)) {
                        conn.close(403u32.into(), b"refused");
                        continue;
                    }
                    accepted
                        .lock()
                        .expect("poisoned")
                        .push((conn.remote_id(), conn.weak_handle()));
                    if gossip.handle_connection(conn).await.is_err() {
                        break;
                    }
                }
            })
        };
        Ok(Self {
            secret,
            endpoint,
            gossip,
            accepted,
            _accept: AbortOnDropHandle::new(accept),
        })
    }

    fn id(&self) -> EndpointId {
        self.endpoint.id()
    }

    /// Counts the connections from `peer` this node accepted that are still open.
    fn open_from(&self, peer: EndpointId) -> usize {
        let accepted = self.accepted.lock().expect("poisoned");
        accepted
            .iter()
            .filter(|(id, handle)| {
                *id == peer
                    && handle
                        .upgrade()
                        .is_some_and(|conn| conn.close_reason().is_none())
            })
            .count()
    }

    /// Leaves all topics, telling the neighbors, then closes the endpoint.
    async fn shutdown(self) -> Result<SecretKey> {
        self.gossip.shutdown().await.std_context("shutdown")?;
        self.endpoint.close().await;
        Ok(self.secret)
    }

    /// Closes the endpoint without leaving any topic, as a crashing process does.
    ///
    /// The peers see the connections close, but get no `Disconnect`.
    async fn crash(self) -> SecretKey {
        self.endpoint.close().await;
        self.secret
    }
}

/// Counts the connections open between `a` and `b`, either way.
fn open_between(a: &Node, b: &Node) -> usize {
    a.open_from(b.id()) + b.open_from(a.id())
}

/// A topic with its sender and receiver.
struct Sub {
    tx: GossipSender,
    rx: GossipReceiver,
}

impl Sub {
    /// Subscribes `node` to `topic` with `bootstrap`, without waiting for a neighbor.
    async fn new(node: &Node, topic: TopicId, bootstrap: Vec<EndpointId>) -> Result<Self> {
        let (tx, rx) = node.gossip.subscribe(topic, bootstrap).await?.split();
        Ok(Self { tx, rx })
    }

    /// Subscribes `node` to `topic` and waits for the first neighbor.
    async fn join(node: &Node, topic: TopicId, bootstrap: Vec<EndpointId>) -> Result<Self> {
        let topic = timeout(PROMPT, node.gossip.subscribe_and_join(topic, bootstrap))
            .await
            .std_context("the join did not complete")??;
        let (tx, rx) = topic.split();
        Ok(Self { tx, rx })
    }

    /// Waits for an event that `accept` takes, skipping others.
    async fn wait(&mut self, within: Duration, mut accept: impl FnMut(&Event) -> bool) -> Result {
        timeout(within, async {
            loop {
                match self.rx.try_next().await.std_context("receive")? {
                    Some(event) if accept(&event) => return Ok(()),
                    Some(_) => {}
                    None => bail_any!("the topic closed"),
                }
            }
        })
        .await
        .std_context("the event did not come")?
    }

    async fn neighbor_up(&mut self, peer: EndpointId) -> Result {
        self.wait(PROMPT, |e| matches!(e, Event::NeighborUp(p) if *p == peer))
            .await
    }

    async fn neighbor_down(&mut self, peer: EndpointId) -> Result {
        self.wait(
            PROMPT,
            |e| matches!(e, Event::NeighborDown(p) if *p == peer),
        )
        .await
    }

    /// Waits for a message with `content`.
    async fn received(&mut self, content: &'static [u8]) -> Result {
        self.wait(
            PROMPT,
            |e| matches!(e, Event::Received(m) if m.content == content),
        )
        .await
    }

    /// Fails if `peer` goes down within `within`.
    async fn stays_up(&mut self, peer: EndpointId, within: Duration) -> Result {
        match timeout(within, self.fail_on_down(peer)).await {
            Ok(res) => res,
            Err(_elapsed) => Ok(()),
        }
    }

    /// Fails when `peer` goes down, or when the topic closes or fails.
    async fn fail_on_down(&mut self, peer: EndpointId) -> Result {
        loop {
            match self.rx.try_next().await.std_context("receive")? {
                Some(Event::NeighborDown(p)) if p == peer => bail_any!("the neighbor went down"),
                Some(_) => {}
                None => bail_any!("the topic closed"),
            }
        }
    }

    /// Takes the events that wait, so that `neighbors()` is current.
    async fn drain(&mut self) -> Result {
        while let Ok(event) = timeout(Duration::from_millis(50), self.rx.try_next()).await {
            if event.std_context("receive")?.is_none() {
                bail_any!("the topic closed");
            }
        }
        Ok(())
    }

    async fn broadcast(&self, content: &'static [u8]) -> Result {
        self.tx
            .broadcast(Bytes::from_static(content))
            .await
            .std_context("broadcast")
    }
}

fn topic(name: &str) -> TopicId {
    blake3::hash(name.as_bytes()).into()
}

/// Waits until `done` returns true, and fails with `what` after `within`.
async fn eventually(
    within: Duration,
    what: &'static str,
    mut done: impl FnMut() -> bool,
) -> Result {
    let wait = async {
        while !done() {
            sleep(Duration::from_millis(100)).await;
        }
    };
    timeout(within, wait).await.std_context(what)
}

/// Waits until the neighbors of `subs` connect all of them.
///
/// Plumtree does not send a message again to a node that becomes a neighbor
/// after the broadcast, so a broadcast before this can miss a node.
async fn connected(subs: &mut [&mut Sub], ids: &[EndpointId]) -> Result {
    let wait = async {
        loop {
            for sub in subs.iter_mut() {
                sub.drain().await?;
            }
            let mut reached = vec![false; ids.len()];
            let mut next = vec![0];
            reached[0] = true;
            while let Some(i) = next.pop() {
                for peer in subs[i].rx.neighbors() {
                    if let Some(j) = ids.iter().position(|id| *id == peer) {
                        if !reached[j] {
                            reached[j] = true;
                            next.push(j);
                        }
                    }
                }
            }
            if reached.iter().all(|r| *r) {
                return n0_error::Ok(());
            }
            sleep(Duration::from_millis(100)).await;
        }
    };
    timeout(PROMPT, wait)
        .await
        .std_context("the neighbors did not connect")?
}

/// Checks that messages flow both ways between two subscriptions.
async fn exchange(a: &mut Sub, b: &mut Sub, tag: &'static [u8; 2]) -> Result {
    let (to_b, to_a): (&'static [u8], &'static [u8]) = match tag {
        b"01" => (b"01 a to b", b"01 b to a"),
        b"02" => (b"02 a to b", b"02 b to a"),
        b"03" => (b"03 a to b", b"03 b to a"),
        _ => (b"xx a to b", b"xx b to a"),
    };
    a.broadcast(to_b).await?;
    b.received(to_b).await?;
    b.broadcast(to_a).await?;
    a.received(to_a).await?;
    Ok(())
}

/// Two nodes become neighbors and exchange messages.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn two_nodes_exchange_messages() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("two_nodes");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let mut sb = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;

    exchange(&mut sa, &mut sb, b"01").await
}

/// Two nodes that join each other at once stay neighbors after the connections settle.
///
/// Each dials the other, and the two may keep different connections. A side
/// that closed the connection the other still used lost the neighbor.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn concurrent_joins_keep_the_neighbors() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("concurrent_joins");
    let (sa, sb) = tokio::join!(
        Sub::join(&a, t, vec![b.id()]),
        Sub::join(&b, t, vec![a.id()])
    );
    let (mut sa, mut sb) = (sa?, sb?);

    let (up_a, up_b) = tokio::join!(sa.stays_up(b.id(), SETTLE), sb.stays_up(a.id(), SETTLE));
    up_a?;
    up_b?;
    exchange(&mut sa, &mut sb, b"01").await?;
    // Each side may keep the connection the other dialed, but no more.
    assert!(open_between(&a, &b) <= 2, "connections piled up");
    Ok(())
}

/// Joining and leaving again and again leaves no connection open, and a rejoin then dials anew.
///
/// Nothing used to close a connection gossip stopped using, so each round left
/// one open for as long as both nodes ran.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn repeated_joins_leave_no_connection_open() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("repeated_joins");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    for _ in 0..5 {
        let sb = Sub::join(&b, t, vec![a.id()]).await?;
        drop(sb);
        sa.neighbor_down(b.id()).await?;
    }

    eventually(SETTLE, "connections were left open", || {
        open_between(&a, &b) == 0
    })
    .await?;

    let mut sb = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;
    exchange(&mut sa, &mut sb, b"01").await
}

/// Leaving a topic and joining it again at once works while another topic keeps the peer.
///
/// The connection stays up for the other topic throughout. A `Join` sent on a
/// new stream could overtake the `Disconnect` before it, so the peer took the
/// join and then dropped us. Its broadcasts then no longer reached us.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn quick_rejoin_while_another_topic_keeps_the_peer() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let (keep, quick) = (topic("keep"), topic("quick"));
    let _sa_keep = Sub::new(&a, keep, vec![]).await?;
    let mut sa = Sub::new(&a, quick, vec![]).await?;
    let _sb_keep = Sub::join(&b, keep, vec![a.id()]).await?;
    let sb = Sub::join(&b, quick, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;

    let mut sb = sb;
    for _ in 0..10 {
        drop(sb);
        sb = Sub::join(&b, quick, vec![a.id()]).await?;
    }
    // B can take a stale `Neighbor` from A for an answer, while A takes B in
    // only when the last join arrives.
    let taken_in = async {
        loop {
            sa.drain().await?;
            if sa.rx.neighbors().any(|peer| peer == b.id()) {
                return n0_error::Ok(());
            }
        }
    };
    timeout(PROMPT, taken_in)
        .await
        .std_context("A did not take B in again")??;
    exchange(&mut sa, &mut sb, b"01").await
}

/// Leaving one of two topics keeps the other working over the shared connection.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn leaving_one_topic_keeps_the_other() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let (gone, kept) = (topic("gone"), topic("kept"));
    let _sa_gone = Sub::new(&a, gone, vec![]).await?;
    let mut sa = Sub::new(&a, kept, vec![]).await?;
    let sb_gone = Sub::join(&b, gone, vec![a.id()]).await?;
    let mut sb = Sub::join(&b, kept, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;

    drop(sb_gone);
    sa.stays_up(b.id(), SETTLE).await?;
    exchange(&mut sa, &mut sb, b"01").await
}

/// A node that starts again before its former instance is gone rejoins.
///
/// The peer still holds the former instance as a neighbor when the new one
/// joins. The peer must answer the join, and the later close of the former
/// instance's connection must not drop the new one.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn restart_while_the_former_instance_runs() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("restart_overlap");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let _sb_former = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;

    let b_new = Node::spawn_with(&lookup, b.secret.clone()).await?;
    let mut sb = Sub::join(&b_new, t, vec![a.id()]).await?;
    exchange(&mut sa, &mut sb, b"01").await?;

    b.crash().await;
    sb.stays_up(a.id(), IDLE_TIMEOUT + Duration::from_secs(2))
        .await?;
    exchange(&mut sa, &mut sb, b"02").await
}

/// A node that vanishes and starts again at once rejoins, and stays.
///
/// The peer still holds the vanished instance as a neighbor, over a
/// connection that only times out later. The peer must answer the new
/// instance's join, and the timeout of the old connection must not drop the
/// new one.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn restart_right_after_vanishing() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("restart_after_vanishing");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let sb = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;

    let secret = b.secret.clone();
    drop((b, sb));
    let b = Node::spawn_with(&lookup, secret).await?;
    let mut sb = Sub::join(&b, t, vec![a.id()]).await?;
    exchange(&mut sa, &mut sb, b"01").await?;

    sb.stays_up(a.id(), IDLE_TIMEOUT + Duration::from_secs(2))
        .await?;
    exchange(&mut sa, &mut sb, b"02").await
}

/// A node that vanishes and restarts again and again keeps working, with no pile-up.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn repeated_vanish_and_restart() -> Result {
    let lookup = MemoryLookup::new();
    let (a, mut b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("repeated_vanish");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let mut sb = Sub::join(&b, t, vec![a.id()]).await?;
    for tag in [b"01", b"02", b"03"] {
        let secret = b.secret.clone();
        drop((b, sb));
        b = Node::spawn_with(&lookup, secret).await?;
        sb = Sub::join(&b, t, vec![a.id()]).await?;
        exchange(&mut sa, &mut sb, tag).await?;
    }

    // The vanished instances' connections time out, and the overlay settles.
    sleep(SETTLE).await;
    exchange(&mut sa, &mut sb, b"xx").await?;
    assert!(open_between(&a, &b) <= 2, "connections piled up");
    Ok(())
}

/// A broadcast over the size limit leaves the neighbors up.
///
/// The message cannot go out. It once failed the write, which closed the
/// connection, and both sides lost each other for good (#131).
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn oversized_broadcast_keeps_the_neighbors() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("oversized");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let mut sb = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;

    let oversized = Bytes::from(vec![7u8; a.gossip.max_message_size() * 2]);
    sa.tx.broadcast(oversized).await.std_context("broadcast")?;

    sa.stays_up(b.id(), Duration::from_secs(4)).await?;
    exchange(&mut sa, &mut sb, b"01").await
}

/// An oversized broadcast in a swarm leaves every neighbor up.
///
/// Lazy peers learn of the message and ask for it, which the sender cannot
/// send either.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn oversized_broadcast_in_a_swarm_keeps_the_neighbors() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b, c) = (
        Node::spawn(&lookup).await?,
        Node::spawn(&lookup).await?,
        Node::spawn(&lookup).await?,
    );
    let t = topic("oversized_swarm");
    let sa = Sub::new(&a, t, vec![]).await?;
    let mut sb = Sub::join(&b, t, vec![a.id()]).await?;
    let mut sc = Sub::join(&c, t, vec![a.id()]).await?;
    sleep(Duration::from_secs(2)).await;

    let oversized = Bytes::from(vec![7u8; a.gossip.max_message_size() * 2]);
    sa.tx.broadcast(oversized).await.std_context("broadcast")?;

    sb.stays_up(a.id(), Duration::from_secs(4)).await?;
    sc.stays_up(a.id(), Duration::from_secs(1)).await?;
    sa.broadcast(b"after the oversized one").await?;
    sb.received(b"after the oversized one").await?;
    sc.received(b"after the oversized one").await
}

/// Shutting down twice, or twice at once, succeeds.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
#[ignore = "not yet passing"]
async fn repeated_shutdown_succeeds() -> Result {
    let lookup = MemoryLookup::new();
    let a = Node::spawn(&lookup).await?;
    let _sa = Sub::new(&a, topic("shutdown_twice"), vec![]).await?;

    let (first, second) = tokio::join!(a.gossip.shutdown(), a.gossip.shutdown());
    let third = a.gossip.shutdown().await;

    first.std_context("first shutdown")?;
    second.std_context("concurrent shutdown")?;
    third.std_context("shutdown after the actor stopped")?;
    Ok(())
}

/// A peer that left one of two topics and restarted is a neighbor on both again.
///
/// Leaving one topic once closed the connection the other still used, which
/// left stale state that ignored the restarted peer (#172).
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn restart_after_leaving_one_of_two_topics_rejoins_both() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let (t1, t2) = (topic("left_one"), topic("kept_one"));
    let mut sa1 = Sub::new(&a, t1, vec![]).await?;
    let mut sa2 = Sub::new(&a, t2, vec![]).await?;
    let sb1 = Sub::join(&b, t1, vec![a.id()]).await?;
    let sb2 = Sub::join(&b, t2, vec![a.id()]).await?;
    sa1.neighbor_up(b.id()).await?;
    sa2.neighbor_up(b.id()).await?;

    drop(sb1);
    sa1.neighbor_down(b.id()).await?;
    drop(sb2);
    let secret = b.shutdown().await?;
    sa2.neighbor_down(secret.public()).await?;
    let b = Node::spawn_with(&lookup, secret).await?;
    let mut sb1 = Sub::join(&b, t1, vec![a.id()]).await?;
    let mut sb2 = Sub::join(&b, t2, vec![a.id()]).await?;

    sa1.neighbor_up(b.id()).await?;
    sa2.neighbor_up(b.id()).await?;
    exchange(&mut sa1, &mut sb1, b"01").await?;
    exchange(&mut sa2, &mut sb2, b"02").await
}

/// A node redials a peer that timed out and then returned.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn survivor_redials_a_peer_that_timed_out() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("timed_out");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let sb = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;
    let secret = b.secret.clone();
    drop((b, sb));
    sa.neighbor_down(secret.public()).await?;

    let b = Node::spawn_with(&lookup, secret).await?;
    let mut sb = Sub::new(&b, t, vec![]).await?;
    sa.tx.join_peers(vec![b.id()]).await.std_context("join")?;

    sa.neighbor_up(b.id()).await?;
    exchange(&mut sa, &mut sb, b"01").await
}

/// A join whose dial failed works once the peer's address is known.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn join_after_a_failed_dial_connects() -> Result {
    let (lookup_a, lookup_b) = (MemoryLookup::new(), MemoryLookup::new());
    let a = Node::spawn(&lookup_a).await?;
    let b = Node::spawn(&lookup_b).await?;
    let t = topic("failed_dial");
    let mut sb = Sub::new(&b, t, vec![]).await?;
    // `a` has no address for `b`, so the dial fails.
    let mut sa = Sub::new(&a, t, vec![b.id()]).await?;
    sleep(Duration::from_secs(2)).await;

    lookup_a.set_endpoint_info(b.endpoint.addr());
    sa.tx.join_peers(vec![b.id()]).await.std_context("join")?;

    sa.neighbor_up(b.id()).await?;
    exchange(&mut sa, &mut sb, b"01").await
}

/// A peer whose connections were refused joins once they are accepted.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn refused_peer_joins_once_accepted() -> Result {
    let lookup = MemoryLookup::new();
    let accept = Arc::new(AtomicBool::new(false));
    let filter: AcceptFilter = {
        let accept = accept.clone();
        Arc::new(move |_| accept.load(Ordering::SeqCst))
    };
    let a = Node::spawn_custom(
        &lookup,
        SecretKey::generate(),
        Gossip::builder(),
        Some(filter),
    )
    .await?;
    let b = Node::spawn(&lookup).await?;
    let t = topic("refused");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let mut sb = Sub::new(&b, t, vec![a.id()]).await?;
    sleep(Duration::from_secs(2)).await;

    accept.store(true, Ordering::SeqCst);
    sb.tx.join_peers(vec![a.id()]).await.std_context("join")?;

    sb.neighbor_up(a.id()).await?;
    sa.neighbor_up(b.id()).await?;
    exchange(&mut sa, &mut sb, b"01").await
}

/// Peers that join and leave one after another leave no connection open (#145).
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn churn_leaves_no_connection_open() -> Result {
    const ROUNDS: usize = 8;
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("churn");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let mut sb = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;

    // The nodes stay up after they left, so only gossip can close the connections.
    let mut left = Vec::new();
    for _ in 0..ROUNDS {
        let node = Node::spawn(&lookup).await?;
        let sub = Sub::join(&node, t, vec![a.id()]).await?;
        drop(sub);
        left.push(node);
    }
    let all_closed = || {
        left.iter()
            .all(|node| a.open_from(node.id()) == 0 && b.open_from(node.id()) == 0)
    };
    eventually(SETTLE, "a connection was left open", all_closed).await?;
    exchange(&mut sa, &mut sb, b"01").await
}

/// A neighbor request whose dial is slow still makes the two neighbors.
///
/// A joins C. A forwards the join to B, and B asks C to be its neighbor. C
/// takes two seconds to accept the connection, as on a slow network. The
/// request timeout once counted the dial, so B gave up on C after 500 ms.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn neighbor_request_over_a_slow_dial_connects() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let slow: AcceptFilter = Arc::new(|_| {
        std::thread::sleep(Duration::from_secs(2));
        true
    });
    let c = Node::spawn_custom(
        &lookup,
        SecretKey::generate(),
        Gossip::builder(),
        Some(slow),
    )
    .await?;
    let t = topic("slow_dial");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let mut sb = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;

    let mut sc = Sub::join(&c, t, vec![a.id()]).await?;
    sb.neighbor_up(c.id()).await?;
    sc.neighbor_up(b.id()).await?;
    sb.stays_up(c.id(), Duration::from_secs(3)).await?;
    exchange(&mut sb, &mut sc, b"01").await
}

/// A join to a node that is not on the topic leaves no connection once we leave.
///
/// The node never answers, so only our side can drop the other.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn join_to_a_node_off_the_topic_leaves_no_connection() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("off_the_topic");
    let sb = Sub::new(&b, t, vec![a.id()]).await?;
    let connected = || open_between(&a, &b) > 0;
    eventually(PROMPT, "the join did not connect", connected).await?;

    drop(sb);
    let closed = || open_between(&a, &b) == 0;
    eventually(SETTLE, "a connection was left open", closed).await
}

/// A join that gets no answer while its connection stays open is sent again.
///
/// B joins A before A subscribes to the topic, so A drops the join (#175). The
/// connection stays open, so no close makes B retry, and B waited forever. A
/// join on a connection that died after a newer one replaced it is lost the
/// same way.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn unanswered_join_is_sent_again() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("unanswered_join");
    let mut sb = Sub::new(&b, t, vec![a.id()]).await?;
    let connected = || open_between(&a, &b) > 0;
    eventually(PROMPT, "the join did not connect", connected).await?;

    let mut sa = Sub::new(&a, t, vec![]).await?;
    sb.wait(
        JOIN_RETRY_WITHIN,
        |e| matches!(e, Event::NeighborUp(p) if *p == a.id()),
    )
    .await?;
    sa.neighbor_up(b.id()).await?;
    exchange(&mut sa, &mut sb, b"01").await
}

/// Repeated joins to one peer before it answers make one neighbor.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn repeated_joins_to_one_peer_make_one_neighbor() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("repeated_join_peers");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let mut sb = Sub::new(&b, t, vec![a.id()]).await?;
    for _ in 0..5 {
        sb.tx.join_peers(vec![a.id()]).await.std_context("join")?;
    }

    sb.neighbor_up(a.id()).await?;
    sa.neighbor_up(b.id()).await?;
    sb.stays_up(a.id(), Duration::from_secs(2)).await?;
    exchange(&mut sa, &mut sb, b"01").await?;
    assert!(
        open_between(&a, &b) <= 2,
        "the joins left extra connections"
    );
    Ok(())
}

/// Each open connection links two nodes, and at least one holds the other as a neighbor.
///
/// A connection that neither node uses for a topic is a leak (#145, #101).
/// More nodes than one active view holds join through one node, so that it
/// evicts neighbors.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn open_connections_link_neighbors() -> Result {
    const JOINERS: usize = 8;
    let lookup = MemoryLookup::new();
    let bootstrap = Node::spawn(&lookup).await?;
    let t = topic("connections_link_neighbors");
    let mut nodes = Vec::new();
    let mut subs = vec![Sub::new(&bootstrap, t, vec![]).await?];
    for _ in 0..JOINERS {
        let node = Node::spawn(&lookup).await?;
        subs.push(Sub::join(&node, t, vec![bootstrap.id()]).await?);
        nodes.push(node);
    }
    nodes.insert(0, bootstrap);
    sleep(SETTLE).await;

    // A connection we are done with stays open for a grace of a few seconds,
    // and the overlay can still open one, so we wait for a moment without an
    // unlinked connection. A leaked connection stays unlinked.
    let mut unlinked = None;
    let wait = async {
        loop {
            for sub in subs.iter_mut() {
                sub.drain().await?;
            }
            unlinked = unlinked_pair(&nodes, &subs);
            if unlinked.is_none() {
                return n0_error::Ok(());
            }
            sleep(Duration::from_millis(200)).await;
        }
    };
    match timeout(SETTLE, wait).await {
        Ok(drained) => drained,
        Err(_) => {
            let (i, j) = unlinked.expect("set before each sleep");
            bail_any!("nodes {i} and {j} keep a connection without a neighbor")
        }
    }
}

/// Returns two nodes that keep a connection, where neither holds the other as a neighbor.
fn unlinked_pair(nodes: &[Node], subs: &[Sub]) -> Option<(usize, usize)> {
    for (i, a) in nodes.iter().enumerate() {
        for (j, b) in nodes.iter().enumerate().skip(i + 1) {
            let linked = subs[i].rx.neighbors().any(|p| p == b.id())
                || subs[j].rx.neighbors().any(|p| p == a.id());
            if open_between(a, b) > 0 && !linked {
                return Some((i, j));
            }
        }
    }
    None
}

/// Many nodes that all join each other at once form an overlay that reaches all.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn many_nodes_joining_each_other_at_once() -> Result {
    const NODES: usize = 6;
    let lookup = MemoryLookup::new();
    let mut nodes = Vec::new();
    for _ in 0..NODES {
        nodes.push(Node::spawn(&lookup).await?);
    }
    let ids: Vec<EndpointId> = nodes.iter().map(Node::id).collect();
    let t = topic("all_at_once");
    let joins: Vec<_> = nodes
        .iter()
        .map(|node| {
            let others = ids.iter().copied().filter(|id| *id != node.id()).collect();
            Sub::join(node, t, others)
        })
        .collect();
    let mut subs = joins.try_join().await?;
    sleep(SETTLE).await;

    let contents: [&'static [u8]; NODES] = [b"n0", b"n1", b"n2", b"n3", b"n4", b"n5"];
    for (i, content) in contents.iter().enumerate() {
        subs[i].broadcast(content).await?;
        for (j, sub) in subs.iter_mut().enumerate() {
            if i != j {
                sub.received(content).await?;
            }
        }
    }
    Ok(())
}

/// Nodes that join through one bootstrap node form an overlay that reaches all.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn nodes_joining_through_one_bootstrap_reach_each_other() -> Result {
    const NODES: usize = 8;
    let lookup = MemoryLookup::new();
    let mut nodes = Vec::new();
    for _ in 0..NODES {
        nodes.push(Node::spawn(&lookup).await?);
    }
    let t = topic("one_bootstrap");
    let bootstrap = nodes[0].id();
    let mut subs = vec![Sub::new(&nodes[0], t, vec![]).await?];
    let joins: Vec<_> = nodes[1..]
        .iter()
        .map(|node| Sub::join(node, t, vec![bootstrap]))
        .collect();
    subs.extend(joins.try_join().await?);
    let ids: Vec<_> = nodes.iter().map(Node::id).collect();
    connected(&mut subs.iter_mut().collect::<Vec<_>>(), &ids).await?;

    subs[NODES - 1].broadcast(b"from the last").await?;
    for sub in subs[..NODES - 1].iter_mut() {
        sub.received(b"from the last").await?;
    }
    subs[0].broadcast(b"from the first").await?;
    for sub in subs[1..].iter_mut() {
        sub.received(b"from the first").await?;
    }
    Ok(())
}

/// A node that leaves a topic and joins it again becomes a neighbor again.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn leave_and_rejoin() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("leave_and_rejoin");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    for tag in [b"01", b"02", b"03"] {
        let mut sb = Sub::join(&b, t, vec![a.id()]).await?;
        sa.neighbor_up(b.id()).await?;
        exchange(&mut sa, &mut sb, tag).await?;
        drop(sb);
        sa.neighbor_down(b.id()).await?;
    }
    Ok(())
}

/// A node that shuts down and starts again with the same key rejoins.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn restart_after_shutdown_rejoins() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("restart_after_shutdown");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let sb = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;

    drop(sb);
    let secret = b.shutdown().await?;
    sa.neighbor_down(secret.public()).await?;
    let b = Node::spawn_with(&lookup, secret).await?;
    let mut sb = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;
    exchange(&mut sa, &mut sb, b"01").await
}

/// A node that crashes and starts again with the same key rejoins.
///
/// The peers see its connections close without a `Disconnect`.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn restart_after_crash_rejoins() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("restart_after_crash");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let sb = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;

    let secret = b.crash().await;
    drop(sb);
    sa.neighbor_down(secret.public()).await?;
    let b = Node::spawn_with(&lookup, secret).await?;
    let mut sb = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;
    exchange(&mut sa, &mut sb, b"01").await
}

/// A neighbor that vanishes without a `Disconnect` goes down once the transport times out.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn vanished_neighbor_is_dropped() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("vanished");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let sb = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;

    let b_id = b.id();
    drop((b, sb));
    sa.neighbor_down(b_id).await
}

/// A burst of broadcasts reaches every neighbor.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn burst_of_broadcasts_is_delivered() -> Result {
    const MESSAGES: usize = 200;
    let lookup = MemoryLookup::new();
    let (a, b, c) = (
        Node::spawn(&lookup).await?,
        Node::spawn(&lookup).await?,
        Node::spawn(&lookup).await?,
    );
    let t = topic("burst");
    let sa = Sub::new(&a, t, vec![]).await?;
    let mut sb = Sub::join(&b, t, vec![a.id()]).await?;
    let mut sc = Sub::join(&c, t, vec![a.id()]).await?;

    for i in 0..MESSAGES {
        let content = Bytes::from(format!("burst {i:04} {}", "x".repeat(100)));
        sa.tx.broadcast(content).await.std_context("broadcast")?;
    }
    for sub in [&mut sb, &mut sc] {
        let mut received = 0;
        sub.wait(PROMPT, |e| {
            if matches!(e, Event::Received(_)) {
                received += 1;
            }
            received == MESSAGES
        })
        .await?;
    }
    Ok(())
}

/// A shutdown tells the neighbors, even while the endpoint stays open.
///
/// They learn of it from the `Disconnect` or the closed connection, not from
/// a timeout.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn shutdown_tells_the_neighbors() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("shutdown_tells");
    let _sb = Sub::new(&b, t, vec![]).await?;
    let mut sa = Sub::join(&a, t, vec![b.id()]).await?;

    b.gossip.shutdown().await.std_context("shutdown")?;

    sa.wait(
        Duration::from_secs(2),
        |e| matches!(e, Event::NeighborDown(p) if *p == b.id()),
    )
    .await
}

/// The swarm outlives the node that its members joined through (#86).
///
/// The others stay connected, and the node rejoins through any of them. A
/// newcomer that joins through another member reaches everyone.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn swarm_outlives_its_bootstrap_node() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b, c) = (
        Node::spawn(&lookup).await?,
        Node::spawn(&lookup).await?,
        Node::spawn(&lookup).await?,
    );
    let t = topic("outlives");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let mut sb = Sub::join(&b, t, vec![a.id()]).await?;
    let mut sc = Sub::join(&c, t, vec![a.id()]).await?;
    connected(&mut [&mut sa, &mut sb, &mut sc], &[a.id(), b.id(), c.id()]).await?;

    drop(sa);
    let secret = a.shutdown().await?;
    // B and C may have reached each other only through A.
    connected(&mut [&mut sb, &mut sc], &[b.id(), c.id()]).await?;
    exchange(&mut sb, &mut sc, b"01").await?;

    let a = Node::spawn_with(&lookup, secret).await?;
    let mut sa = Sub::join(&a, t, vec![c.id()]).await?;
    connected(&mut [&mut sa, &mut sb, &mut sc], &[a.id(), b.id(), c.id()]).await?;
    sb.broadcast(b"from b").await?;
    sa.received(b"from b").await?;
    sc.received(b"from b").await?;

    let d = Node::spawn(&lookup).await?;
    let mut sd = Sub::join(&d, t, vec![b.id()]).await?;
    let ids = [a.id(), b.id(), c.id(), d.id()];
    connected(&mut [&mut sa, &mut sb, &mut sc, &mut sd], &ids).await?;
    sd.broadcast(b"from d").await?;
    sa.received(b"from d").await?;
    sc.received(b"from d").await
}

/// Dropping the receiver keeps the topic, and dropping the sender too leaves it.
///
/// A topic lives while either half does (#101, #119).
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn receiver_drop_keeps_the_topic_until_the_sender_goes() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("receiver_drop");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let Sub { tx, rx } = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;

    drop(rx);
    sa.stays_up(b.id(), Duration::from_secs(3)).await?;
    tx.broadcast(Bytes::from_static(b"from b"))
        .await
        .std_context("broadcast")?;
    sa.received(b"from b").await?;

    drop(tx);
    sa.neighbor_down(b.id()).await
}

/// A join through a dead and a live bootstrap peer reaches the live one.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn join_with_a_dead_bootstrap_peer_reaches_the_live_one() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let dead = Node::spawn(&lookup).await?.crash().await.public();
    let t = topic("dead_bootstrap");
    let mut sa = Sub::new(&a, t, vec![]).await?;

    let mut sb = Sub::join(&b, t, vec![dead, a.id()]).await?;

    sa.neighbor_up(b.id()).await?;
    exchange(&mut sa, &mut sb, b"01").await
}
