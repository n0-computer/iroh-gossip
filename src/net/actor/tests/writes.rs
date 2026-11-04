use super::*;

/// Pushes `count` payloads of `len` bytes for `topic` into a large queue.
fn queue_payloads(topic: TopicId, count: usize, len: usize) -> QueueLease {
    let mut queue = SendQueue::new(64 << 20);
    for _ in 0..count {
        let message = ProtoMessage {
            topic,
            message: TopicMessage::test_gossip(len),
        };
        assert!(matches!(queue.push(message), Pushed::Queued));
    }
    queue.lease()
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
    let queue_lease = queue_payloads(topic, 200, 60_000);
    let send_loop = SendLoop::new(
        conn,
        queue_lease,
        TEST_FRAME_LIMIT,
        unset_flag(),
        Vec::new(),
    );
    let result = timeout(wire::WRITE_TIMEOUT * 5, send_loop.run())
        .await
        .std_context("the writer never ended")?;
    assert!(
        matches!(result, Err(wire::WriteError::Stalled { .. })),
        "{result:?}"
    );
    closed_with(&peer_conn, conn::CLOSE_STALLED).await
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
    let queue_lease = queue_payloads(topic, count, len);
    let send_loop = SendLoop::new(conn, queue_lease, 4 << 20, unset_flag(), Vec::new());
    let send = AbortOnDropHandle::new(spawn(async move { send_loop.run().await }));

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
    let result = timeout(Duration::from_secs(5), send)
        .await
        .std_context("the writer did not end")?
        .std_context("send task")?;
    assert!(result.is_ok(), "{result:?}");
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
    let mut queue = SendQueue::new(64 << 20);
    for i in 0..1000u32 {
        let topic: TopicId = blake3::hash(&i.to_be_bytes()).into();
        assert!(matches!(queue.push(join_message(topic, 0)), Pushed::Queued));
    }
    let send_loop = SendLoop::new(
        conn,
        queue.lease(),
        TEST_FRAME_LIMIT,
        unset_flag(),
        Vec::new(),
    );
    let result = timeout(wire::OPEN_TIMEOUT * 5, send_loop.run())
        .await
        .std_context("the writer never ended")?;
    assert!(
        matches!(result, Err(wire::WriteError::OpenStalled { .. })),
        "{result:?}"
    );
    closed_with(&peer_conn, conn::CLOSE_STALLED).await
}
