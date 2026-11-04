//! End-to-end tests of gossip between nodes in one process.
//!
//! The tests use only the public API, so they run against any version of the
//! crate. Nodes connect directly on this host, without a relay. Each node runs
//! its own accept loop. The loop keeps a weak handle to each connection, so that
//! a test can count the ones still open.

use std::{
    collections::BTreeSet,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
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
    api::{Event, GossipReceiver, GossipSender, JoinOptions},
    net::{Builder, Gossip, Verdict, GOSSIP_ALPN},
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

/// A wait longer than a node keeps a connection without streams.
///
/// That is the wait for our finished streams to be acknowledged, then the
/// default `retire_after` of 10 s, and a margin.
const SETTLE: Duration = Duration::from_secs(16);

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
        self.open_ids_from(peer).len()
    }

    /// Returns the positions in `accepted` of the open connections from `peer`.
    ///
    /// A position names one connection for the life of the node.
    fn open_ids_from(&self, peer: EndpointId) -> Vec<usize> {
        let accepted = self.accepted.lock().expect("poisoned");
        accepted
            .iter()
            .enumerate()
            .filter(|(_, (id, handle))| {
                *id == peer
                    && handle
                        .upgrade()
                        .is_some_and(|conn| conn.close_reason().is_none())
            })
            .map(|(pos, _)| pos)
            .collect()
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

/// A peer that restarted on other topics leaves the views of its former neighbor.
///
/// The close of the former instance's connection drops no neighbor, as the new
/// instance took over the writer. A forward join for a third node gets the
/// answer that the peer left.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn restart_on_other_topics_leaves_the_former_neighbor() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let (t, other) = (topic("restart_left"), topic("restart_other"));
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let _sa_other = Sub::new(&a, other, vec![]).await?;
    let _sb_former = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;

    let b_new = Node::spawn_with(&lookup, b.secret.clone()).await?;
    let _sb = Sub::join(&b_new, other, vec![a.id()]).await?;
    let b_id = b.id();
    b.crash().await;
    let c = Node::spawn(&lookup).await?;
    let _sc = Sub::join(&c, t, vec![a.id()]).await?;
    sa.neighbor_down(b_id).await
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

    // The connections of the vanished instances time out. That must not drop
    // the new instance.
    sb.stays_up(a.id(), SETTLE).await?;
    exchange(&mut sa, &mut sb, b"xx").await?;
    assert!(open_between(&a, &b) <= 2, "connections piled up");
    Ok(())
}

/// Shutting down twice, or twice at once, succeeds.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
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
    // `b` can get its `Disconnect` out before its endpoint stops. Then `a`
    // keeps its connection to `b` until it times out, and a dial to the
    // returned `b` before then fails with it.
    eventually(IDLE_TIMEOUT + PROMPT, "a kept the connection of b", || {
        a.open_from(secret.public()) == 0
    })
    .await?;

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
    eventually(PROMPT, "the dial did not fail", || {
        a.gossip.metrics().actor_tick_dialer_failure.get() > 0
    })
    .await?;

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
    let asked = Arc::new(AtomicUsize::new(0));
    let filter: AcceptFilter = {
        let (accept, asked) = (accept.clone(), asked.clone());
        Arc::new(move |_| {
            asked.fetch_add(1, Ordering::SeqCst);
            accept.load(Ordering::SeqCst)
        })
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
    eventually(PROMPT, "the connection did not come", || {
        asked.load(Ordering::SeqCst) > 0
    })
    .await?;

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
    // Both directions: a or b may have dialed a node that left.
    let all_closed = || {
        left.iter()
            .all(|node| open_between(&a, node) == 0 && open_between(&b, node) == 0)
    };
    // A neighbor request to a node that left times out after 3 s, and only
    // then does the requester drop the node.
    eventually(SETTLE * 2, "a connection was left open", all_closed).await?;
    exchange(&mut sa, &mut sb, b"01").await
}

/// A neighbor request whose dial is slow still makes the two neighbors.
///
/// A joins C. A forwards the join to B, and B asks C to be its neighbor. C
/// takes a second to accept the connection, as on a slow network. The
/// request timeout once counted the dial, so B gave up on C after 500 ms.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn neighbor_request_over_a_slow_dial_connects() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let slow: AcceptFilter = Arc::new(|_| {
        std::thread::sleep(Duration::from_secs(1));
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
    // No signal tells that the overlay is quiet, so wait for the idle connections to retire.
    sleep(SETTLE).await;

    // A connection we are done with stays open until one side retires it,
    // and the overlay keeps opening short-lived ones, for example for a
    // refused neighbor request. A leaked connection stays unlinked, so each
    // connection unlinked now must be linked or closed at some point within
    // `SETTLE`.
    for sub in subs.iter_mut() {
        sub.drain().await?;
    }
    let mut unlinked = unlinked_connections(&nodes, &subs);
    let wait = async {
        while !unlinked.is_empty() {
            sleep(Duration::from_millis(200)).await;
            for sub in subs.iter_mut() {
                sub.drain().await?;
            }
            let now = unlinked_connections(&nodes, &subs);
            unlinked.retain(|conn| now.contains(conn));
        }
        n0_error::Ok(())
    };
    match timeout(SETTLE, wait).await {
        Ok(drained) => drained,
        Err(_) => {
            let pairs: BTreeSet<_> = unlinked.iter().map(|c| (c.0, c.1)).collect();
            bail_any!("these nodes keep a connection without a neighbor: {pairs:?}")
        }
    }
}

/// Returns the open connections between nodes where neither holds the other as a neighbor.
///
/// It skips a node short of neighbors. Such a node asks its passive peers again
/// every few seconds, so its connections to them stay.
///
/// An entry holds the indices of the two nodes, the index of the node that
/// accepted the connection, and its position in `Node::accepted`.
fn unlinked_connections(nodes: &[Node], subs: &[Sub]) -> BTreeSet<(usize, usize, usize, usize)> {
    let mut conns = BTreeSet::new();
    for (i, a) in nodes.iter().enumerate() {
        for (j, b) in nodes.iter().enumerate().skip(i + 1) {
            let linked = subs[i].rx.neighbors().any(|p| p == b.id())
                || subs[j].rx.neighbors().any(|p| p == a.id());
            // Below half of the active view.
            let short = |sub: &Sub| sub.rx.neighbors().count() < 3;
            if linked || short(&subs[i]) || short(&subs[j]) {
                continue;
            }
            for pos in a.open_ids_from(b.id()) {
                conns.insert((i, j, i, pos));
            }
            for pos in b.open_ids_from(a.id()) {
                conns.insert((i, j, j, pos));
            }
        }
    }
    conns
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
    connected(&mut subs.iter_mut().collect::<Vec<_>>(), &ids).await?;

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

/// A former instance of a peer that leaves does not remove its replacement.
///
/// The session id in the control header tells the two instances apart: the
/// connection of the former instance closes as stale, and its `Disconnect`
/// never reaches the protocol.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn former_instance_leaving_preserves_replacement_neighbor() {
    former_instance_leaving_preserves_replacement()
        .await
        .unwrap();
}

/// Runs `former_instance_leaving_preserves_replacement_neighbor`.
async fn former_instance_leaving_preserves_replacement() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("review_former_shutdown");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let former = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;
    let replacement = Node::spawn_with(&lookup, b.secret.clone()).await?;
    let mut sb = Sub::join(&replacement, t, vec![a.id()]).await?;
    exchange(&mut sa, &mut sb, b"01").await?;
    drop(former);
    sa.stays_up(replacement.id(), SETTLE)
        .await
        .std_context("old instance leave removed the replacement neighbor")?;
    exchange(&mut sa, &mut sb, b"02").await
}

/// A peer that stops reading does not block our shutdown (#47).
///
/// The actor waited when a peer's send queue was full, and then handled
/// nothing else.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn slow_peer_does_not_block_shutdown() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("review_backpressure");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let conn = b
        .endpoint
        .connect(a.endpoint.addr(), GOSSIP_ALPN)
        .await
        .std_context("connect")?;
    // The control stream of the dialer: its header with a session id.
    let (mut control, _control_recv) = conn.open_bi().await.std_context("open control")?;
    let control_header = [0x0b, 0x00, 0x01, 0x08, 1, 2, 3, 4, 5, 6, 7, 8];
    control
        .write_all(&control_header)
        .await
        .std_context("send control header")?;
    let mut stream = conn.open_uni().await.std_context("open raw stream")?;
    // The wire format: the topic header, then `SWARM_JOIN(None)`.
    let mut join = vec![35, 0x01, 0x00, 32];
    join.extend_from_slice(t.as_bytes());
    join.extend_from_slice(&[2, 0x01, 0x00]);
    stream.write_all(&join).await.std_context("send join")?;
    sa.neighbor_up(b.id()).await?;
    // The peer keeps QUIC alive but does not read the gossip receive stream.
    let sender = tokio::spawn(async move {
        let bytes = Bytes::from(vec![8u8; 3900]);
        for _ in 0..10000 {
            sa.tx.broadcast_neighbors(bytes.clone()).await?;
        }
        Ok::<_, iroh_gossip::Error>(())
    });
    // Once the send queue to the peer is full, it drops messages.
    eventually(PROMPT, "the send queue did not fill", || {
        a.gossip.metrics().msgs_dropped.get() > 0
    })
    .await?;
    let result = timeout(Duration::from_secs(2), a.gossip.shutdown()).await;
    conn.close(0u32.into(), b"test finished");
    sender.abort();
    result.std_context("a flow-controlled peer blocked gossip shutdown")??;
    Ok(())
}

/// A verifier that accepts the token `secret:<topic>` and nothing else, and counts its calls.
fn shared_secret_builder(calls: Arc<AtomicUsize>) -> Builder {
    Gossip::builder().topic_auth(move |auth| {
        calls.fetch_add(1, Ordering::SeqCst);
        match auth.token() == Some(secret_for(auth.topic()).as_slice()) {
            true => Verdict::Accept,
            false => Verdict::Reject,
        }
    })
}

fn secret_for(topic: TopicId) -> Vec<u8> {
    [b"secret:".as_slice(), topic.as_bytes()].concat()
}

/// Subscribes `node` to `topic` with `token`, without waiting for a neighbor.
async fn subscribe_with_token(
    node: &Node,
    topic: TopicId,
    bootstrap: Vec<EndpointId>,
    token: Vec<u8>,
) -> Result<Sub> {
    let opts = JoinOptions::with_bootstrap(bootstrap).auth_token(token);
    let (tx, rx) = node.gossip.subscribe_with_opts(topic, opts).await?.split();
    Ok(Sub { tx, rx })
}

/// A peer with a wrong token is refused on that topic, and the connection stays usable.
///
/// The nodes meet on an open topic first. The joiner then joins a guarded topic with
/// a wrong token. It gets `Refused`, and the verifier runs once. The open topic
/// keeps its neighbor and still carries messages.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn wrong_token_is_refused_and_the_connection_stays_usable() -> Result {
    let lookup = MemoryLookup::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let guard = {
        let calls = calls.clone();
        let guarded = topic("guarded");
        Gossip::builder().topic_auth(move |auth| {
            if auth.topic() != guarded {
                return Verdict::Accept;
            }
            calls.fetch_add(1, Ordering::SeqCst);
            match auth.token() == Some(b"good".as_slice()) {
                true => Verdict::Accept,
                false => Verdict::Reject,
            }
        })
    };
    let a = Node::spawn_custom(&lookup, SecretKey::generate(), guard, None).await?;
    let b = Node::spawn(&lookup).await?;
    let (open, guarded) = (topic("open"), topic("guarded"));
    let mut a_open = Sub::new(&a, open, vec![]).await?;
    let mut b_open = Sub::join(&b, open, vec![a.id()]).await?;
    a_open.neighbor_up(b.id()).await?;

    let mut a_guarded = Sub::new(&a, guarded, vec![]).await?;
    let mut b_guarded = subscribe_with_token(&b, guarded, vec![a.id()], b"bad".to_vec()).await?;
    b_guarded
        .wait(PROMPT, |e| matches!(e, Event::Refused(p) if *p == a.id()))
        .await?;
    // No message of the refused peer reaches the guarded topic of a.
    b_guarded.broadcast(b"intruder").await?;
    let seen = a_guarded
        .wait(Duration::from_secs(1), |e| {
            matches!(e, Event::Received(_) | Event::NeighborUp(_))
        })
        .await;
    assert!(seen.is_err(), "the refused peer got through");
    assert_eq!(b_guarded.rx.neighbors().count(), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "one stream, one call");

    // The connection carries the open topic as before.
    b_open.stays_up(a.id(), Duration::from_secs(1)).await?;
    exchange(&mut a_open, &mut b_open, b"01").await?;
    Ok(())
}

/// Peers with the right token take part, in both directions.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn right_token_is_accepted() -> Result {
    let lookup = MemoryLookup::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let a = Node::spawn_custom(
        &lookup,
        SecretKey::generate(),
        shared_secret_builder(calls.clone()),
        None,
    )
    .await?;
    let b = Node::spawn_custom(
        &lookup,
        SecretKey::generate(),
        shared_secret_builder(calls.clone()),
        None,
    )
    .await?;
    let t = topic("shared_secret");
    let mut sa = subscribe_with_token(&a, t, vec![], secret_for(t)).await?;
    let mut sb = subscribe_with_token(&b, t, vec![a.id()], secret_for(t)).await?;
    sa.neighbor_up(b.id()).await?;
    sb.neighbor_up(a.id()).await?;
    exchange(&mut sa, &mut sb, b"02").await?;
    assert!(calls.load(Ordering::SeqCst) >= 2, "both sides verified");
    Ok(())
}

/// Without a verifier, a token or its absence changes nothing.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn no_verifier_accepts_everything() -> Result {
    let lookup = MemoryLookup::new();
    let a = Node::spawn(&lookup).await?;
    let b = Node::spawn(&lookup).await?;
    let t = topic("no_verifier");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let mut sb = subscribe_with_token(&b, t, vec![a.id()], b"unchecked".to_vec()).await?;
    sa.neighbor_up(b.id()).await?;
    sb.neighbor_up(a.id()).await?;
    exchange(&mut sa, &mut sb, b"03").await?;
    Ok(())
}

/// A token never shows in the debug output of the types that carry it.
#[tokio::test(flavor = "multi_thread")]
async fn debug_output_has_no_token() -> Result {
    let token = b"hunter2-token".to_vec();
    let opts = JoinOptions::with_bootstrap([]).auth_token(token.clone());
    let builder = Gossip::builder().topic_auth(|_| Verdict::Accept);
    let lookup = MemoryLookup::new();
    let node = Node::spawn_custom(&lookup, SecretKey::generate(), builder.clone(), None).await?;
    let sub = node
        .gossip
        .subscribe_with_opts(
            topic("debug"),
            JoinOptions::with_bootstrap([]).auth_token(token),
        )
        .await?;
    for debug in [
        format!("{opts:?}"),
        format!("{builder:?}"),
        format!("{:?}", node.gossip),
        format!("{sub:?}"),
    ] {
        assert!(!debug.contains("hunter2"), "{debug}");
        assert!(!debug.contains("104, 117"), "{debug}");
    }
    Ok(())
}

/// Spawns a node whose gossip retires idle connections after `retire_after`.
async fn node_retiring_after(lookup: &MemoryLookup, retire_after: Duration) -> Result<Node> {
    let builder = Gossip::builder().retire_after(retire_after);
    Node::spawn_custom(lookup, SecretKey::generate(), builder, None).await
}

/// Either side retires an idle connection: the acceptor with a short timer closes it too.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn either_side_retires() -> Result {
    let lookup = MemoryLookup::new();
    // `a` dials, and never retires on its own within this test. `b` accepts, with a short timer.
    let a = node_retiring_after(&lookup, Duration::from_secs(600)).await?;
    let b = node_retiring_after(&lookup, Duration::from_millis(200)).await?;
    let t = topic("either_side_retires");
    let mut sb = Sub::new(&b, t, vec![]).await?;
    let sa = Sub::join(&a, t, vec![b.id()]).await?;
    sb.neighbor_up(a.id()).await?;
    drop(sa);
    sb.neighbor_down(a.id()).await?;
    eventually(PROMPT, "the acceptor did not retire", || {
        open_between(&a, &b) == 0
    })
    .await
}

/// A neighbor edge keeps a stream, so a quiet topic keeps its connection and its neighbor.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn neighbor_edge_always_has_stream() -> Result {
    let lookup = MemoryLookup::new();
    let a = node_retiring_after(&lookup, Duration::from_millis(300)).await?;
    let b = node_retiring_after(&lookup, Duration::from_millis(300)).await?;
    let t = topic("neighbor_edge");
    let mut sb = Sub::new(&b, t, vec![]).await?;
    let mut sa = Sub::join(&a, t, vec![b.id()]).await?;
    sb.neighbor_up(a.id()).await?;
    let (up_a, up_b) = tokio::join!(
        sa.stays_up(b.id(), Duration::from_secs(4)),
        sb.stays_up(a.id(), Duration::from_secs(4))
    );
    up_a?;
    up_b?;
    assert_eq!(
        open_between(&a, &b),
        1,
        "the quiet topic lost its connection"
    );
    exchange(&mut sa, &mut sb, b"01").await
}

/// A message that a join sends while the connection retires still arrives.
///
/// The offsets sweep the time around the retire. The message waits in the
/// queue, and goes out on a new dial after the retire.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn message_during_retire_arrives() -> Result {
    let lookup = MemoryLookup::new();
    let retire_after = Duration::from_millis(300);
    let a = node_retiring_after(&lookup, retire_after).await?;
    let b = node_retiring_after(&lookup, retire_after).await?;
    for round in 0..10u64 {
        // A new topic each round, as a node ignores a message it saw before.
        let t = topic(&format!("message_during_retire_{round}"));
        let mut sb = Sub::new(&b, t, vec![]).await?;
        let sa = Sub::join(&a, t, vec![b.id()]).await?;
        sb.neighbor_up(a.id()).await?;
        drop(sa);
        sb.neighbor_down(a.id()).await?;
        // Around the moment that the connection retires.
        sleep(retire_after - Duration::from_millis(60) + Duration::from_millis(15 * round)).await;
        let t2 = topic(&format!("message_during_retire_again_{round}"));
        let mut sb2 = Sub::new(&b, t2, vec![]).await?;
        let mut sa2 = Sub::join(&a, t2, vec![b.id()]).await?;
        sb2.neighbor_up(a.id()).await?;
        exchange(&mut sa2, &mut sb2, b"01").await?;
    }
    Ok(())
}

/// Nodes that dial each other at once end up with one connection, and lose no neighbor and no message.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn crossed_dials_lose_no_neighbor() -> Result {
    let lookup = MemoryLookup::new();
    for round in 0..4 {
        let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
        let t = topic(&format!("crossed_dials_{round}"));
        let (sa, sb) = tokio::join!(
            Sub::join(&a, t, vec![b.id()]),
            Sub::join(&b, t, vec![a.id()])
        );
        let (mut sa, mut sb) = (sa?, sb?);
        let (up_a, up_b) = tokio::join!(
            sa.stays_up(b.id(), Duration::from_secs(1)),
            sb.stays_up(a.id(), Duration::from_secs(1))
        );
        up_a?;
        up_b?;
        exchange(&mut sa, &mut sb, b"01").await?;
        exchange(&mut sb, &mut sa, b"02").await?;
        // The connection dialed by the higher id retires, the other one stays.
        timeout(Duration::from_secs(2), async {
            while open_between(&a, &b) != 1 {
                sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .std_context("crossed dials stayed two connections")?;
        exchange(&mut sa, &mut sb, b"03").await?;
        exchange(&mut sb, &mut sa, b"04").await?;
        sa.stays_up(b.id(), Duration::from_millis(500)).await?;
    }
    Ok(())
}

/// Crossed dials keep the order of the messages on each topic, with two topics on the pair.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn crossed_dials_keep_order_two_topics() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let topics = [topic("order_one"), topic("order_two")];
    let mut subs = Vec::new();
    for t in topics {
        let (sa, sb) = tokio::join!(
            Sub::join(&a, t, vec![b.id()]),
            Sub::join(&b, t, vec![a.id()])
        );
        subs.push((sa?, sb?));
    }
    // Messages while the crossed connections are up, and while the loser retires.
    const COUNT: usize = 30;
    for (sa, _) in subs.iter() {
        for i in 0..COUNT {
            let content = Bytes::from(format!("{i:03}"));
            sa.tx.broadcast(content).await?;
        }
    }
    for (_, sb) in subs.iter_mut() {
        let mut seen = Vec::new();
        timeout(PROMPT, async {
            while seen.len() < COUNT {
                match sb.rx.next().await {
                    Some(Ok(Event::Received(message))) => seen.push(message.content),
                    Some(Ok(_)) => {}
                    other => bail_any!("the subscription ended: {other:?}"),
                }
            }
            Ok::<_, n0_error::AnyError>(())
        })
        .await
        .std_context("messages are missing")??;
        let expected: Vec<Bytes> = (0..COUNT).map(|i| Bytes::from(format!("{i:03}"))).collect();
        assert_eq!(seen, expected, "the messages lost their order");
    }
    Ok(())
}

/// A clean restart: the peer sees the shutdown, resets the membership, and messages flow again.
///
/// The old instance leaves and closes with `SHUTDOWN`. The peer reports the neighbor as down, then up
/// once the new instance with the same key joins.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn clean_restart_cleans_membership() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("clean_restart");
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

/// An unclean restart: the former instance is silent but keeps its connection.
///
/// The new instance dials with a new session id. The peer replaces the old connection, resets the
/// membership (down, then up), and messages flow. This needs no idle timeout. The later end of
/// the former instance must not hurt the new one.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn unclean_restart_replaces_on_dial() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("unclean_restart");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let _sb_former = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;

    let b_new = Node::spawn_with(&lookup, b.secret.clone()).await?;
    let mut sb = Sub::join(&b_new, t, vec![a.id()]).await?;
    sa.neighbor_down(b.id()).await?;
    sa.neighbor_up(b.id()).await?;
    exchange(&mut sa, &mut sb, b"01").await?;

    // The former instance dies: the idle timeout of its connection, or the close, ends it.
    b.crash().await;
    sb.stays_up(a.id(), IDLE_TIMEOUT + Duration::from_secs(2))
        .await?;
    exchange(&mut sa, &mut sb, b"02").await
}

/// An unclean restart of a node that accepts: the peer learns of it only from the idle timeout.
///
/// The old instance vanishes without a close. The peer keeps a dead connection until the QUIC idle
/// timeout (`IDLE_TIMEOUT`) ends it, then reports the neighbor down. The new instance then joins
/// and is a neighbor again.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn unclean_restart_of_the_acceptor_waits_for_idle() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("unclean_acceptor");
    let sa = Sub::new(&a, t, vec![]).await?;
    let mut sb = Sub::join(&b, t, vec![a.id()]).await?;
    drop(sa);
    let a_id = a.id();
    // Dropped without leaving or closing: the node goes silent.
    let secret = {
        let secret = a.secret.clone();
        drop(a);
        secret
    };
    // Depends on the QUIC idle timeout of the transport (3 s in these tests).
    sb.wait(
        IDLE_TIMEOUT + PROMPT,
        |e| matches!(e, Event::NeighborDown(p) if *p == a_id),
    )
    .await?;
    let a = Node::spawn_with(&lookup, secret).await?;
    let mut sa = Sub::join(&a, t, vec![b.id()]).await?;
    sb.neighbor_up(a.id()).await?;
    exchange(&mut sa, &mut sb, b"01").await
}

/// Both nodes write a burst while their crossed dials settle: nothing is lost.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn retire_loses_no_unread_data() -> Result {
    const COUNT: usize = 50;
    let lookup = MemoryLookup::new();
    for round in 0..3 {
        let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
        let t = topic(&format!("burst_during_crossed_dial_{round}"));
        let (sa, sb) = tokio::join!(
            Sub::join(&a, t, vec![b.id()]),
            Sub::join(&b, t, vec![a.id()])
        );
        let (mut sa, mut sb) = (sa?, sb?);
        for i in 0..COUNT {
            sa.tx.broadcast(Bytes::from(format!("a {i:03}"))).await?;
            sb.tx.broadcast(Bytes::from(format!("b {i:03}"))).await?;
        }
        for (sub, from) in [(&mut sb, "a"), (&mut sa, "b")] {
            let mut seen = Vec::new();
            timeout(PROMPT, async {
                while seen.len() < COUNT {
                    match sub.rx.next().await {
                        Some(Ok(Event::Received(m))) => seen.push(m.content),
                        Some(Ok(_)) => {}
                        other => bail_any!("the subscription ended: {other:?}"),
                    }
                }
                Ok::<_, n0_error::AnyError>(())
            })
            .await
            .std_context("messages are missing")??;
            let expected: Vec<Bytes> = (0..COUNT)
                .map(|i| Bytes::from(format!("{from} {i:03}")))
                .collect();
            assert_eq!(seen, expected, "round {round}");
        }
    }
    Ok(())
}

/// A `subscription_capacity` of 0 counts as 1, and does not panic.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn zero_subscription_capacity_works() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("zero_capacity");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let opts = JoinOptions::with_bootstrap([a.id()]).with_subscription_capacity(0);
    let (tx, rx) = b.gossip.subscribe_with_opts(t, opts).await?.split();
    let mut sb = Sub { tx, rx };
    sa.neighbor_up(b.id()).await?;
    exchange(&mut sa, &mut sb, b"01").await
}

/// A token that does not fit in a stream header fails the join with an error.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn token_over_the_header_limit_is_rejected() -> Result {
    let lookup = MemoryLookup::new();
    let a = Node::spawn(&lookup).await?;
    let t = topic("big_token");
    // The header limit is 4 KiB. The kind, the topic and the token parameter leave 4058
    // bytes for the token.
    let big = vec![7u8; 4059];
    let res = a
        .gossip
        .subscribe_with_opts(t, JoinOptions::with_bootstrap([]).auth_token(big))
        .await;
    assert!(
        matches!(res, Err(iroh_gossip::Error::TokenTooLarge { .. })),
        "{res:?}"
    );
    let opts = JoinOptions::with_bootstrap([]).auth_token(vec![7u8; 4058]);
    a.gossip.subscribe_with_opts(t, opts).await?;
    Ok(())
}

/// A token over the message size limit fits in the stream header, and the peers connect.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn token_over_the_message_limit_fits_in_the_header() -> Result {
    let lookup = MemoryLookup::new();
    let small = || Gossip::builder().max_message_size(1024);
    let a = Node::spawn_custom(&lookup, SecretKey::generate(), small(), None).await?;
    let b = Node::spawn_custom(&lookup, SecretKey::generate(), small(), None).await?;
    let t = topic("big_token");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let mut sb = subscribe_with_token(&b, t, vec![a.id()], vec![7u8; 4000]).await?;
    sa.neighbor_up(b.id()).await?;
    exchange(&mut sa, &mut sb, b"01").await
}

/// A reader that does not keep up gets `Lagged`, and the subscription stays open.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn slow_reader_gets_lagged() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("lagged");
    let opts = JoinOptions::with_bootstrap([]).with_subscription_capacity(1);
    let (tx, rx) = a.gossip.subscribe_with_opts(t, opts).await?.split();
    let mut sa = Sub { tx, rx };
    let sb = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;
    // More events than the subscription and the topic channel hold, with no read in between.
    for _ in 0..1000 {
        sb.tx
            .broadcast_neighbors(Bytes::from_static(b"x"))
            .await
            .std_context("broadcast")?;
    }
    eventually(PROMPT, "the messages did not arrive", || {
        a.gossip.metrics().msgs_data_recv.get() >= 1000
    })
    .await?;
    sa.wait(PROMPT, |e| matches!(e, Event::Lagged)).await?;
    // The subscription stays open.
    sb.broadcast(b"after").await?;
    sa.received(b"after").await
}

/// A node whose `Gossip` drops without a `shutdown` goes down for its neighbors.
///
/// It sends no `Close`, so the neighbor sees the connection fail.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn dropped_gossip_is_seen_as_down() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("dropped_gossip");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let sb = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;
    let id = b.id();
    drop((b, sb));
    sa.wait(
        IDLE_TIMEOUT + PROMPT,
        |e| matches!(e, Event::NeighborDown(p) if *p == id),
    )
    .await
}

/// The counters count what the nodes sent and received.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn counters_count_traffic() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("counters");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let mut sb = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;
    sa.broadcast(b"01 a to b").await?;
    sb.received(b"01 a to b").await?;

    let (ma, mb) = (a.gossip.metrics(), b.gossip.metrics());
    assert_eq!(ma.neighbor_up.get(), 1);
    assert_eq!(mb.neighbor_up.get(), 1);
    assert_eq!(ma.neighbor_down.get(), 0);
    assert_eq!(ma.msgs_data_sent.get(), 1);
    assert_eq!(mb.msgs_data_recv.get(), 1);
    assert_eq!(ma.msgs_data_sent_size.get(), mb.msgs_data_recv_size.get());
    assert!(mb.msgs_data_recv_size.get() > b"01 a to b".len() as u64);
    assert!(ma.msgs_ctrl_recv.get() >= 1 && mb.msgs_ctrl_sent.get() >= 1);
    assert_eq!(ma.msgs_dropped.get() + mb.msgs_dropped.get(), 0);
    assert_eq!(
        ma.peers_send_queue_full.get() + mb.peers_send_queue_full.get(),
        0
    );
    assert!(ma.actor_tick_main.get() > 0);
    Ok(())
}

/// A new subscriber gets every current neighbor, even with a capacity of 1.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn new_subscriber_gets_all_neighbors() -> Result {
    let lookup = MemoryLookup::new();
    let a = Node::spawn(&lookup).await?;
    let t = topic("all_neighbors");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let mut others = Vec::new();
    for _ in 0..3 {
        let node = Node::spawn(&lookup).await?;
        let sub = Sub::new(&node, t, vec![a.id()]).await?;
        sa.neighbor_up(node.id()).await?;
        others.push((node, sub));
    }

    let opts = JoinOptions::with_bootstrap([]).with_subscription_capacity(1);
    let (tx, rx) = a.gossip.subscribe_with_opts(t, opts).await?.split();
    let mut late = Sub { tx, rx };
    for (node, _) in &others {
        late.neighbor_up(node.id()).await?;
    }
    assert_eq!(late.rx.neighbors().count(), 3);

    let (gone, _gone_sub) = others.remove(0);
    gone.gossip.shutdown().await.std_context("shutdown")?;
    late.neighbor_down(gone.id()).await?;
    assert!(late.rx.is_joined());
    assert_eq!(late.rx.neighbors().count(), 2);
    Ok(())
}

/// A broadcast of the largest content that the sender takes reaches the peer.
///
/// The send queue once sized a message with its topic, 32 bytes more than the frame.
/// A broadcast near the limit then went out as too large, and reached no peer.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn broadcast_of_the_content_limit_arrives() -> Result {
    let lookup = MemoryLookup::new();
    let (a, b) = (Node::spawn(&lookup).await?, Node::spawn(&lookup).await?);
    let t = topic("content_limit");
    let mut sa = Sub::new(&a, t, vec![]).await?;
    let sb = Sub::join(&b, t, vec![a.id()]).await?;
    sa.neighbor_up(b.id()).await?;
    let max = sb.tx.max_content_size();
    let content = Bytes::from(vec![7u8; max]);
    sb.tx
        .broadcast(content.clone())
        .await
        .std_context("broadcast")?;
    sa.wait(
        PROMPT,
        |e| matches!(e, Event::Received(m) if m.content == content),
    )
    .await
}
