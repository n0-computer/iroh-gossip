use super::*;

/// Pushes `count` payloads of `len` bytes for `topic` into a large queue.
fn queue_payloads(topic: TopicId, count: usize, len: usize) -> (SendQueue, QueueReader) {
    let (queue, reader) = SendQueue::new(64 << 20);
    for _ in 0..count {
        let message = ProtoMessage {
            topic,
            message: TopicMessage::test_gossip(len),
        };
        assert!(matches!(queue.send(message), Pushed::Queued));
    }
    (queue, reader)
}

/// A write that makes no progress for the write timeout closes the connection.
///
/// The peer never reads, so flow control blocks every write once the
/// receive window is full. The writer never ended.
#[tokio::test]
#[traced_test]
async fn stalled_write_closes_the_connection() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"stalled").into();
    // More than the windows of the peer, which reads nothing.
    let (_queue, reader) = queue_payloads(topic, 200, 60_000);
    let writer = start_writer(conn, &peer_conn, reader, TEST_FRAME_LIMIT, Vec::new()).await?;
    let end = ended(writer.test.task, wire::WRITE_TIMEOUT * 5).await?;
    assert_eq!(end, ConnEnd::Failed);
    closed_with(&peer_conn, CloseCode::Blocked).await
}

/// A peer that reads one frame slower than the write timeout keeps the connection.
///
/// The timeout counts the time without progress, not the time of a frame.
#[tokio::test]
#[traced_test]
async fn slow_reader_keeps_the_connection() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"slow").into();
    // At 64 KiB per 300 ms, the frame takes about 6.5 s, over the write timeout.
    let (count, len) = (1, 1_400_000);
    let (_queue, reader) = queue_payloads(topic, count, len);
    let writer = start_writer(conn, &peer_conn, reader, 4 << 20, Vec::new()).await?;

    let start = Instant::now();
    let mut stream = peer_conn.accept_uni().await.std_context("accept")?;
    let mut chunk = vec![0u8; 64 * 1024];
    let mut read = 0;
    while read < count * len {
        tokio::time::sleep(Duration::from_millis(300)).await;
        read += stream
            .read(&mut chunk)
            .await
            .std_context("read")?
            .std_context("the stream ended early")?;
    }
    assert!(
        start.elapsed() > wire::WRITE_TIMEOUT,
        "the reader was not slow"
    );
    assert!(!writer.test.task.is_finished(), "the writer failed");
    assert!(peer_conn.close_reason().is_none(), "the connection closed");
    Ok(())
}

/// A peer at its stream limit loses the connection after the open timeout.
///
/// It accepts no stream, so no stream of ours ends, and the peer never
/// raises its stream limit.
#[tokio::test]
#[traced_test]
async fn stream_limit_closes_the_connection() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let (queue, reader) = SendQueue::new(64 << 20);
    for i in 0..1000u32 {
        let topic: TopicId = blake3::hash(&i.to_be_bytes()).into();
        assert!(matches!(queue.send(join_message(topic, 0)), Pushed::Queued));
    }
    let writer = start_writer(conn, &peer_conn, reader, TEST_FRAME_LIMIT, Vec::new()).await?;
    let end = ended(writer.test.task, wire::OPEN_TIMEOUT * 5).await?;
    assert_eq!(end, ConnEnd::Failed);
    closed_with(&peer_conn, CloseCode::Blocked).await
}

/// The sent counters count only the messages that a send queue took.
///
/// Nobody reads the queue, so a burst fills its data section. The broadcast whose
/// payload the queue drops adds to `msgs_dropped`, and not to the sent counters.
#[tokio::test]
#[traced_test]
async fn dropped_payload_is_not_counted_as_sent() -> Result {
    let mut actor = t_actor().await?;
    let peer = SecretKey::generate().public();
    let mut state = PeerState::open(queue::MIN_QUEUE_SIZE);
    let _reader = state.take_reader().expect("no writer");
    actor.state.peers.insert(peer, state);
    let topic: TopicId = blake3::hash(b"dropped is not sent").into();
    let now = Instant::now();
    actor.handle_in_event(InEvent::Command(topic, ProtoCommand::Join(vec![])), now);
    let join = ProtoMessage {
        topic,
        message: TopicMessage::Swarm(hyparview::Message::Join(None)),
    };
    actor.handle_in_event(InEvent::RecvMessage(peer, join), now);

    let metrics = actor.state.metrics.clone();
    let sent = || {
        (
            metrics.msgs_data_sent.get(),
            metrics.msgs_data_sent_size.get(),
        )
    };
    for i in 0u32..1000 {
        let before = sent();
        let content = Bytes::from([vec![7u8; 1000], i.to_be_bytes().to_vec()].concat());
        let broadcast = ProtoCommand::Broadcast(content, Scope::Swarm);
        actor.handle_in_event(InEvent::Command(topic, broadcast), now);
        if metrics.msgs_dropped.get() > 0 {
            assert_eq!(metrics.msgs_dropped.get(), 1);
            assert_eq!(sent(), before, "the dropped payload counts as sent");
            return Ok(());
        }
        assert_eq!(sent().0, before.0 + 1, "a queued payload is not counted");
    }
    panic!("no payload dropped");
}

/// The node later announces a payload that a full queue dropped.
///
/// Nobody reads the queue to the neighbor, so a burst fills its data section and
/// drops the last payload. Plumtree notes the drop, and announces the payload
/// when its timer runs. The peer stays, and no announcement goes out at once.
#[tokio::test]
#[traced_test]
async fn dropped_payload_is_announced_later() -> Result {
    let mut actor = t_actor().await?;
    let peer = SecretKey::generate().public();
    let mut state = PeerState::open(queue::MIN_QUEUE_SIZE);
    let mut reader = state.take_reader().expect("no writer");
    actor.state.peers.insert(peer, state);
    let topic: TopicId = blake3::hash(b"dropped payload").into();
    let now = Instant::now();
    actor.handle_in_event(InEvent::Command(topic, ProtoCommand::Join(vec![])), now);
    let join = ProtoMessage {
        topic,
        message: TopicMessage::Swarm(hyparview::Message::Join(None)),
    };
    actor.handle_in_event(InEvent::RecvMessage(peer, join), now);
    assert!(actor
        .proto
        .neighbors(&topic)
        .any(|neighbor| *neighbor == peer));

    // Broadcast until the queue drops a payload. The last one is the dropped one.
    let mut content = Bytes::new();
    for i in 0u32..1000 {
        if actor.state.metrics.msgs_dropped.get() > 0 {
            break;
        }
        content = Bytes::from([vec![7u8; 1000], i.to_be_bytes().to_vec()].concat());
        let broadcast = ProtoCommand::Broadcast(content.clone(), Scope::Swarm);
        actor.handle_in_event(InEvent::Command(topic, broadcast), now);
    }
    assert_eq!(
        actor.state.metrics.msgs_dropped.get(),
        1,
        "no payload dropped"
    );
    assert_eq!(
        actor.state.metrics.peers_send_queue_full.get(),
        0,
        "peer aborted"
    );
    let id = plumtree::MessageId::from_content(&content);

    // The writer reads what the queue holds: payloads, and no announcement.
    assert!(announced(&mut reader).await.is_empty(), "announced at once");

    // The announcement comes with the timers.
    for tick in 1..=3u32 {
        let at = now + Duration::from_millis(1500) * tick;
        while let Some((_, timer)) = actor.state.timers.pop_before(at) {
            actor.handle_in_event(InEvent::TimerExpired(timer), at);
        }
    }
    assert_eq!(announced(&mut reader).await, vec![id]);
    assert!(actor.state.peers.contains_key(&peer));
    assert_eq!(
        actor.state.metrics.peers_send_queue_full.get(),
        0,
        "peer aborted"
    );
    Ok(())
}

/// Takes the queued messages of `reader`, and returns the ids that its announcements name.
async fn announced(reader: &mut queue::QueueReader) -> Vec<plumtree::MessageId> {
    let mut ids = Vec::new();
    while let Ok(true) = timeout(Duration::from_millis(20), reader.wait_next()).await {
        let message = &reader.current().expect("next found it").message.message;
        if let TopicMessage::Gossip(plumtree::Message::IHave(ihaves)) = message {
            ids.extend(ihaves.iter().map(|ihave| ihave.id));
        }
        reader.advance();
    }
    ids
}
