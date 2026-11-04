use super::*;

/// How long a slow peer test waits for something that should happen promptly.
const PROMPT: Duration = Duration::from_secs(10);

/// A gossip node that connects only directly, with its router.
struct DirectNode {
    router: Router,
    gossip: Gossip,
}

impl DirectNode {
    /// Spawns gossip from `builder` on an endpoint with `transport`, and adds its address to `lookup`.
    async fn spawn(
        lookup: &MemoryLookup,
        builder: Builder,
        transport: QuicTransportConfig,
    ) -> Result<Self> {
        let endpoint = direct_endpoint(lookup, transport).await?;
        let gossip = builder.spawn(endpoint.clone());
        let router = Router::builder(endpoint)
            .accept(GOSSIP_ALPN, gossip.clone())
            .spawn();
        Ok(Self { router, gossip })
    }

    fn id(&self) -> EndpointId {
        self.router.endpoint().id()
    }
}

/// Returns a transport with a receive window of `window` bytes per stream, and twice that per connection.
fn small_windows(window: u32) -> QuicTransportConfig {
    QuicTransportConfig::builder()
        .stream_receive_window(VarInt::from_u32(window))
        .receive_window(VarInt::from_u32(window * 2))
        .build()
}

/// Waits until `receiver` reports `peer` as a neighbor.
async fn neighbor_up(receiver: &mut GossipReceiver, peer: EndpointId) -> Result {
    timeout(PROMPT, async {
        while let Some(event) = receiver.try_next().await? {
            if matches!(event, api::Event::NeighborUp(up) if up == peer) {
                return Ok(());
            }
        }
        Err(n0_error::anyerr!("the subscription ended"))
    })
    .await
    .std_context("the peer did not become a neighbor")?
}

/// Waits until `receiver` got a message with `content`, and returns the contents it got.
async fn receive_until(receiver: &mut GossipReceiver, content: &[u8]) -> Result<Vec<Bytes>> {
    let mut received = Vec::new();
    while let Some(event) = receiver.try_next().await? {
        if let api::Event::Received(message) = event {
            let done = message.content == content;
            received.push(message.content);
            if done {
                return Ok(received);
            }
        }
    }
    Err(n0_error::anyerr!("the subscription ended"))
}

/// A healthy peer that shares more topics with us than its stream limit keeps its neighbors (a cost of the design).
///
/// v1 keeps a stream per topic open while the topic uses the peer. Over the
/// peer's limit of concurrent streams, the open of the next stream waits for
/// good, and the open timeout closes the connection. The topics rejoin, and
/// it closes again. The limit of iroh is 100 streams by default.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
#[ignore = "a cost of the design: one stream per topic, see SendQueue"]
async fn topics_over_the_stream_limit_keep_their_neighbors() -> Result {
    let lookup = MemoryLookup::new();
    let limited = || {
        QuicTransportConfig::builder()
            .max_concurrent_uni_streams(VarInt::from_u32(4))
            .build()
    };
    let a = DirectNode::spawn(&lookup, Gossip::builder(), limited()).await?;
    let b = DirectNode::spawn(&lookup, Gossip::builder(), limited()).await?;
    let mut topics = Vec::new();
    for i in 0..8u32 {
        let topic: TopicId = blake3::hash(&i.to_be_bytes()).into();
        topics.push(a.gossip.subscribe(topic, vec![]).await?);
        topics.push(b.gossip.subscribe(topic, vec![a.id()]).await?);
    }
    let joined = async {
        for topic in topics.iter_mut() {
            topic.joined().await?;
        }
        Ok::<_, AnyError>(())
    };
    timeout(PROMPT, joined)
        .await
        .std_context("the topics did not join")??;
    // Long enough for the open timeout of a stream over the limit.
    tokio::time::sleep(wire::OPEN_TIMEOUT + Duration::from_secs(2)).await;
    let down = a.gossip.metrics().neighbor_down.get() + b.gossip.metrics().neighbor_down.get();
    assert_eq!(down, 0, "neighbors went down");
    Ok(())
}

/// A publisher whose every eager peer has a full queue waits, loses nothing, and goes on later.
///
/// The only neighbor, an eager peer, reads nothing, as when our own uplink
/// is the limit for all peers. Without the wait, the queue drops the
/// broadcasts. Once the peer reads again, the gate opens.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn publisher_waits_while_all_neighbors_are_full() -> Result {
    let lookup = MemoryLookup::new();
    let a = DirectNode::spawn(&lookup, Gossip::builder(), small_windows(16 * 1024)).await?;
    let topic: TopicId = blake3::hash(b"all full").into();
    let (a_tx, mut a_rx) = a.gossip.subscribe(topic, vec![]).await?.split();

    // B joins on its own connection, and then reads nothing.
    let b = direct_endpoint(&lookup, small_windows(16 * 1024)).await?;
    let conn = b.connect(a.id(), GOSSIP_ALPN).await?;
    let _join = open_stream(&conn, topic, &[join_message(topic, 0).message]).await?;
    neighbor_up(&mut a_rx, b.id()).await?;

    // The gate never opens while B reads nothing, so a waiting broadcast
    // takes the whole limit.
    let mut waited = false;
    for i in 0..2000u32 {
        let mut message = vec![7u8; 3000];
        message[..4].copy_from_slice(&i.to_be_bytes());
        let start = Instant::now();
        timeout(PROMPT, a_tx.broadcast(message.into()))
            .await
            .std_context("the publisher waited past the limit")??;
        if start.elapsed() >= BROADCAST_WAIT_LIMIT {
            waited = true;
            break;
        }
    }
    assert!(waited, "the publisher never waited");

    // B reads again, so the gate opens, and the publisher goes on.
    const LAST: &[u8] = b"after 99";
    let (seen_tx, seen_rx) = tokio::sync::oneshot::channel();
    let reader = conn.clone();
    let _read = AbortOnDropHandle::new(spawn(async move {
        let mut stream = reader.accept_uni().await.expect("the topic stream");
        // The end of the previous chunk, as the marker may span two.
        let mut seen = Vec::new();
        while let Ok(Some(chunk)) = stream.read_chunk(usize::MAX).await {
            seen.drain(..seen.len().saturating_sub(LAST.len()));
            seen.extend_from_slice(&chunk);
            if seen.windows(LAST.len()).any(|w| w == LAST) {
                seen_tx.send(()).ok();
                break;
            }
        }
    }));
    // Far less than a limit per broadcast, so the gate opened rather than timed out.
    timeout(BROADCAST_WAIT_LIMIT * 20, async {
        for i in 0..100u32 {
            a_tx.broadcast(Bytes::from(format!("after {i}"))).await?;
        }
        Ok::<_, AnyError>(())
    })
    .await
    .std_context("the publisher still waits after B read again")??;
    timeout(Duration::from_secs(10), seen_rx)
        .await
        .std_context("B did not get the last broadcast")?
        .std_context("B stopped reading")?;
    assert!(conn.close_reason().is_none(), "the connection closed");
    assert_eq!(a.gossip.metrics().msgs_dropped.get(), 0, "messages dropped");
    conn.close(0u32.into(), b"done");
    Ok(())
}

/// A publisher gets its broadcasts out while relayed traffic keeps its neighbors full.
///
/// B reads slowly, and is the only eager peer of A on `t1`. C floods `t2`,
/// and A relays it to B, so the queue to B stays over half full. Relays
/// never wait, so the gate alone stays closed for as long as C sends.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn relayed_traffic_does_not_starve_the_publisher() -> Result {
    let lookup = MemoryLookup::new();
    let default = || QuicTransportConfig::builder().build();
    let a = DirectNode::spawn(&lookup, Gossip::builder(), default()).await?;
    let c = DirectNode::spawn(&lookup, Gossip::builder(), default()).await?;
    let (t1, t2): (TopicId, TopicId) = (
        blake3::hash(b"starved").into(),
        blake3::hash(b"flood").into(),
    );
    let (a1, mut a1_rx) = a.gossip.subscribe(t1, vec![]).await?.split();
    let (_a2, _a2_rx) = a.gossip.subscribe(t2, vec![]).await?.split();

    // B joins both topics on its own connection, and reads slowly.
    let b = direct_endpoint(&lookup, small_windows(16 * 1024)).await?;
    let conn = b.connect(a.id(), GOSSIP_ALPN).await?;
    let _join1 = open_stream(&conn, t1, &[join_message(t1, 0).message]).await?;
    let _join2 = open_stream(&conn, t2, &[join_message(t2, 0).message]).await?;
    neighbor_up(&mut a1_rx, b.id()).await?;
    let reader = conn.clone();
    let _read = AbortOnDropHandle::new(spawn(async move {
        let mut tasks = JoinSet::new();
        while let Ok(mut stream) = reader.accept_uni().await {
            tasks.spawn(async move {
                while let Ok(Some(_)) = stream.read_chunk(4096).await {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            });
        }
    }));

    let (c2, _c2_rx) = c.gossip.subscribe_and_join(t2, vec![a.id()]).await?.split();
    let _flood = AbortOnDropHandle::new(spawn(async move {
        for i in 0u64.. {
            let mut message = vec![1u8; 1000];
            message[..8].copy_from_slice(&i.to_be_bytes());
            if c2.broadcast(message.into()).await.is_err() {
                break;
            }
        }
    }));
    // The relayed data fills the queue to B up to its limit.
    let metrics = a.gossip.metrics().clone();
    timeout(PROMPT, async {
        while metrics.msgs_dropped.get() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .std_context("the queue to B never filled")?;

    // More than the command channel holds, so the publisher waits for the actor.
    for i in 0..100u32 {
        timeout(PROMPT, a1.broadcast(Bytes::from(format!("own {i}"))))
            .await
            .std_context("the publisher starved")??;
    }
    assert!(conn.close_reason().is_none(), "the connection closed");
    conn.close(0u32.into(), b"done");
    Ok(())
}

/// A peer that reads nothing fills its queue, and the other peers still get each message (#47).
///
/// A neighbor of the publisher keeps its connection open but never reads,
/// so the queue to it fills and drops payloads. A healthy neighbor gets the
/// later messages on the same topic and on a second one, and the publisher
/// shuts down in time.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn blocked_peer_does_not_stall_the_others() -> Result {
    let lookup = MemoryLookup::new();
    let default = || QuicTransportConfig::builder().build();
    let a = DirectNode::spawn(&lookup, Gossip::builder(), default()).await?;
    let c = DirectNode::spawn(&lookup, Gossip::builder(), default()).await?;
    let (t1, t2): (TopicId, TopicId) = (
        blake3::hash(b"blocked 1").into(),
        blake3::hash(b"blocked 2").into(),
    );
    let (a1, mut a1_rx) = a.gossip.subscribe(t1, vec![]).await?.split();
    let (a2, _a2_rx) = a.gossip.subscribe(t2, vec![]).await?.split();
    let (_c1_tx, mut c1) = c.gossip.subscribe_and_join(t1, vec![a.id()]).await?.split();
    let (_c2_tx, mut c2) = c.gossip.subscribe_and_join(t2, vec![a.id()]).await?.split();

    // B joins on its own connection, and then reads nothing.
    let b = direct_endpoint(&lookup, small_windows(16 * 1024)).await?;
    let conn = b.connect(a.id(), GOSSIP_ALPN).await?;
    let _join = open_stream(&conn, t1, &[join_message(t1, 0).message]).await?;
    neighbor_up(&mut a1_rx, b.id()).await?;

    let metrics = a.gossip.metrics().clone();
    let payload = Bytes::from(vec![7u8; 3000]);
    timeout(PROMPT, async {
        let mut i = 0u32;
        while metrics.msgs_dropped.get() == 0 {
            let mut message = payload.to_vec();
            message.extend_from_slice(&i.to_be_bytes());
            a1.broadcast(message.into()).await?;
            i += 1;
        }
        Ok::<_, AnyError>(())
    })
    .await
    .std_context("the queue to B did not fill")??;
    // Checked now: B reads nothing, so the write timeout closes it later.
    assert!(conn.close_reason().is_none(), "B lost its connection");

    a1.broadcast(Bytes::from_static(b"after")).await?;
    a2.broadcast(Bytes::from_static(b"other topic")).await?;
    timeout(PROMPT, receive_until(&mut c1, b"after"))
        .await
        .std_context("C missed a message on the topic of B")??;
    timeout(PROMPT, receive_until(&mut c2, b"other topic"))
        .await
        .std_context("C missed a message on the other topic")??;
    assert_eq!(
        metrics.peers_send_queue_full.get(),
        0,
        "B lost its connection"
    );

    timeout(Duration::from_secs(5), a.gossip.shutdown())
        .await
        .std_context("B blocked the shutdown")??;
    conn.close(0u32.into(), b"done");
    Ok(())
}

/// A slow peer gets the payloads that its full queue dropped, once it catches up.
///
/// A relays the broadcasts of C to B. The queue to B is small and B's
/// windows are small, so a burst drops payloads. A relays and publishes
/// nothing, so it never waits. A announces the dropped payloads to B again,
/// and B requests them.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn slow_peer_gets_the_dropped_messages() -> Result {
    let lookup = MemoryLookup::new();
    let small_queue = Gossip::builder().send_queue(QueueOptions::default().size(0));
    let default = || QuicTransportConfig::builder().build();
    let a = DirectNode::spawn(&lookup, small_queue, default()).await?;
    let b = DirectNode::spawn(&lookup, Gossip::builder(), small_windows(8 * 1024)).await?;
    let topic: TopicId = blake3::hash(b"slow peer").into();
    let (_a_tx, mut a_rx) = a.gossip.subscribe(topic, vec![]).await?.split();
    let (_b_tx, mut b_rx) = b
        .gossip
        .subscribe_and_join(topic, vec![a.id()])
        .await?
        .split();

    // C publishes on its own connection, and reads every stream of A at once.
    let c = direct_endpoint(&lookup, default()).await?;
    let c_conn = c.connect(a.id(), GOSSIP_ALPN).await?;
    let mut publish = open_stream(&c_conn, topic, &[join_message(topic, 0).message]).await?;
    let reader = c_conn.clone();
    let _read = AbortOnDropHandle::new(spawn(async move {
        while let Ok(mut stream) = reader.accept_uni().await {
            spawn(async move { stream.read_to_end(usize::MAX).await.ok() });
        }
    }));
    timeout(PROMPT, async {
        while let Some(event) = a_rx.try_next().await? {
            if matches!(event, api::Event::NeighborUp(peer) if peer == c.id()) {
                return Ok(());
            }
        }
        Err(n0_error::anyerr!("the subscription ended"))
    })
    .await
    .std_context("C did not become a neighbor")??;

    // B reads all along, so that its subscription does not lag. The channel
    // holds more than all bursts.
    let (got_tx, mut got_rx) = mpsc::channel(8192);
    let _receive = AbortOnDropHandle::new(spawn(async move {
        while let Ok(Some(event)) = b_rx.try_next().await {
            if let api::Event::Received(message) = event {
                got_tx.send(message.content).await.ok();
            }
        }
    }));

    // A burst of 60 overflows the queue to B, which holds about 16 payloads.
    let metrics = a.gossip.metrics().clone();
    let mut sent = BTreeSet::new();
    let mut buffer = Vec::new();
    for burst in 0..40u32 {
        for i in 0..60u32 {
            let mut content = vec![7u8; 3000];
            content.extend_from_slice(&burst.to_be_bytes());
            content.extend_from_slice(&i.to_be_bytes());
            let content = Bytes::from(content);
            sent.insert(content.clone());
            let message = TopicMessage::test_payload(content);
            wire::write_message(&mut publish, &message, &mut buffer, TEST_FRAME_LIMIT)
                .await
                .std_context("publish")?;
        }
        if metrics.msgs_dropped.get() > 0 {
            break;
        }
        // Let the queue drain, so that each burst starts empty.
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(metrics.msgs_dropped.get() > 0, "no burst filled the queue");

    let mut received = BTreeSet::new();
    timeout(Duration::from_secs(25), async {
        while received.len() < sent.len() {
            match got_rx.recv().await {
                Some(content) => {
                    received.insert(content);
                }
                None => break,
            }
        }
    })
    .await
    .with_std_context(|_| {
        format!(
            "B got {} of {} payloads, {} dropped",
            received.len(),
            sent.len(),
            metrics.msgs_dropped.get()
        )
    })?;
    assert_eq!(received, sent);
    Ok(())
}
