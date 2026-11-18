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
            .accept(ALPN, gossip.clone())
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

/// Peers that share more topics than their endpoints allow streams keep their neighbors with `max_peer_streams` (#223).
///
/// The endpoints allow 4 streams, gossip raises it to 16, and 8 topics send both ways.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn max_peer_streams_keeps_topics_over_the_endpoint_limit() -> Result {
    let lookup = MemoryLookup::new();
    let limited = || {
        QuicTransportConfig::builder()
            .max_concurrent_uni_streams(VarInt::from_u32(4))
            .build()
    };
    let builder = || Gossip::builder().max_peer_streams(16);
    let a = DirectNode::spawn(&lookup, builder(), limited()).await?;
    let b = DirectNode::spawn(&lookup, builder(), limited()).await?;
    let mut topics = Vec::new();
    for i in 0..8u32 {
        let topic: TopicId = blake3::hash(&i.to_be_bytes()).into();
        let (a_tx, a_rx) = a.gossip.subscribe(topic, vec![]).await?.split();
        let (b_tx, b_rx) = b.gossip.subscribe(topic, vec![a.id()]).await?.split();
        topics.push((a_tx, a_rx, b_tx, b_rx));
    }
    for (_, a_rx, _, b_rx) in topics.iter_mut() {
        neighbor_up(a_rx, b.id()).await?;
        neighbor_up(b_rx, a.id()).await?;
    }
    for (i, (a_tx, a_rx, b_tx, b_rx)) in topics.iter_mut().enumerate() {
        let content = format!("from a, topic {i}");
        a_tx.broadcast(content.clone().into()).await?;
        timeout(PROMPT, receive_until(b_rx, content.as_bytes()))
            .await
            .std_context("b did not get the message")??;
        let content = format!("from b, topic {i}");
        b_tx.broadcast(content.clone().into()).await?;
        timeout(PROMPT, receive_until(a_rx, content.as_bytes()))
            .await
            .std_context("a did not get the message")??;
    }
    // Long enough for the open timeout of a stream over the limit.
    tokio::time::sleep(wire::OPEN_TIMEOUT + Duration::from_secs(2)).await;
    let down = a.gossip.metrics().neighbor_down.get() + b.gossip.metrics().neighbor_down.get();
    assert_eq!(down, 0, "neighbors went down");
    Ok(())
}

/// A topic that the peer left finishes its stream, while another topic keeps the peer.
///
/// The endpoints allow 3 streams. A and B keep one topic, and join and leave four
/// more in turn. If A kept the streams of the left topics, the third would stall.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn left_topics_finish_their_streams() -> Result {
    let lookup = MemoryLookup::new();
    let limited = || {
        QuicTransportConfig::builder()
            .max_concurrent_uni_streams(VarInt::from_u32(3))
            .build()
    };
    let a = DirectNode::spawn(&lookup, Gossip::builder(), limited()).await?;
    let b = DirectNode::spawn(&lookup, Gossip::builder(), limited()).await?;
    let kept: TopicId = blake3::hash(b"kept").into();
    let (_a_kept_tx, mut a_kept) = a.gossip.subscribe(kept, vec![]).await?.split();
    let (b_kept_tx, mut b_kept) = b.gossip.subscribe(kept, vec![a.id()]).await?.split();
    neighbor_up(&mut a_kept, b.id()).await?;
    neighbor_up(&mut b_kept, a.id()).await?;

    for i in 0..4u32 {
        let topic: TopicId = blake3::hash(&i.to_be_bytes()).into();
        let (a_tx, mut a_rx) = a.gossip.subscribe(topic, vec![]).await?.split();
        let (b_tx, mut b_rx) = b.gossip.subscribe(topic, vec![a.id()]).await?.split();
        // Less than the open timeout, so a stalled open fails the test.
        let prompt = wire::OPEN_TIMEOUT / 2;
        timeout(prompt, neighbor_up(&mut a_rx, b.id()))
            .await
            .std_context("a did not get b as a neighbor in time")??;
        timeout(prompt, neighbor_up(&mut b_rx, a.id()))
            .await
            .std_context("b did not get a as a neighbor in time")??;
        // B leaves the topic. A gets its `Close`, and no longer wants B for it.
        drop((b_tx, b_rx));
        timeout(PROMPT, async {
            while let Some(event) = a_rx.try_next().await? {
                if matches!(event, api::Event::NeighborDown(down) if down == b.id()) {
                    return Ok(());
                }
            }
            Err(n0_error::anyerr!("the subscription ended"))
        })
        .await
        .std_context("a did not see b leave")??;
        drop((a_tx, a_rx));
    }

    // The kept topic still works, and never lost its neighbor.
    b_kept_tx.broadcast("still here".into()).await?;
    timeout(PROMPT, receive_until(&mut a_kept, b"still here"))
        .await
        .std_context("a did not get the message")??;
    let down = a.gossip.metrics().neighbor_down.get();
    assert_eq!(down, 4, "a lost a neighbor of the kept topic");
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
    let conn = b.connect(a.id(), ALPN).await?;
    let _control = control_open(&conn).await?;
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
    let c_conn = c.connect(a.id(), ALPN).await?;
    let _control = control_open(&c_conn).await?;
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

/// A dialed connection fails when the peer grants no bidirectional stream for the control stream.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn dialer_fails_without_a_control_stream() -> Result {
    let lookup = MemoryLookup::new();
    let no_bidi = QuicTransportConfig::builder()
        .max_concurrent_bidi_streams(VarInt::from_u32(0))
        .build();
    let a = direct_endpoint(&lookup, QuicTransportConfig::builder().build()).await?;
    let b = direct_endpoint(&lookup, no_bidi).await?;
    let accept = async {
        let incoming = b.accept().await.std_context("endpoint closed")?;
        incoming.await.std_context("accept")
    };
    let (conn, peer_conn) = tokio::join!(a.connect(b.id(), ALPN), accept);
    let (conn, _peer_conn) = (conn.std_context("connect")?, peer_conn?);
    let test_conn = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_secs(60));
    let end = ended(
        test_conn.task,
        wire::HEADER_TIMEOUT + Duration::from_secs(2),
    )
    .await?;
    assert_eq!(end, ConnEnd::Failed);
    Ok(())
}
