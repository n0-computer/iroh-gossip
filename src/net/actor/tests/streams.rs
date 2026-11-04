use super::*;

/// Returns a `Close` for `topic`.
fn disconnect_message(topic: TopicId) -> ProtoMessage {
    ProtoMessage {
        topic,
        message: TopicMessage::test_close(),
    }
}

/// A topic stream whose header stays partial fails the connection after the header timeout.
///
/// The streams after it wait for its header, so the node cannot go on without it.
#[tokio::test]
#[traced_test]
async fn partial_topic_header_fails_the_connection() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect_back().await?;
    let mut actor = ManualActorLoop::new(t_actor().await?).await;
    actor.add_connection(conn);
    let _control = RawControl::open(&peer_conn).await?;
    let mut stream = peer_conn.open_uni().await.std_context("open stream")?;
    stream
        .write_all(&[0x23, 0x01, 0x00])
        .await
        .std_context("partial header")?;
    pump_until(&mut actor, "the connection to end", |actor| {
        actor.connection_tasks.is_empty()
    })
    .await
    .ok();
    closed_with(&peer_conn, CloseCode::Protocol).await
}

/// A closed queue finishes the streams of the writer, so the connection can go idle.
#[tokio::test]
#[traced_test]
async fn closed_queue_finishes_the_stream() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"closed_queue_finishes_the_stream").into();
    let (queue, reader) = SendQueue::new(queue::MIN_QUEUE_SIZE);
    let _writer = start_writer(conn, &peer_conn, reader, TEST_FRAME_LIMIT, Vec::new()).await?;

    assert!(matches!(queue.send(join_message(topic, 0)), Pushed::Queued));
    let mut stream = peer_conn.accept_uni().await.std_context("accept")?;
    let mut buffer = Vec::new();
    wire::StreamHeader::read(&mut stream, &mut buffer).await?;
    let message: Option<TopicMessage> =
        wire::read_message(&mut stream, &mut buffer, TEST_FRAME_LIMIT).await?;
    assert!(message.is_some(), "the message did not go out");

    queue.close();
    let end: Option<TopicMessage> =
        wire::read_message(&mut stream, &mut buffer, TEST_FRAME_LIMIT).await?;
    assert!(end.is_none(), "the stream did not end");
    Ok(())
}

/// A queued message gets the stream of the peer before the initial topics of the writer.
///
/// The peer here allows one stream. A stream of an initial topic would take it,
/// and the queued message would wait until the open timeout.
#[tokio::test]
#[traced_test]
async fn queued_message_comes_before_the_initial_topics() -> Result {
    let lookup = MemoryLookup::new();
    let ours = direct_endpoint(&lookup, QuicTransportConfig::builder().build()).await?;
    let one_stream = QuicTransportConfig::builder()
        .max_concurrent_uni_streams(VarInt::from_u32(1))
        .build();
    let theirs = direct_endpoint(&lookup, one_stream).await?;
    let accept = async {
        let incoming = theirs.accept().await.std_context("endpoint closed")?;
        incoming.await.std_context("accept")
    };
    let dial = async {
        ours.connect(theirs.id(), GOSSIP_ALPN)
            .await
            .std_context("connect")
    };
    let (peer_conn, conn) = (accept, dial).try_join().await?;
    let (queued, initial): (TopicId, TopicId) = (
        blake3::hash(b"queued").into(),
        blake3::hash(b"initial").into(),
    );
    let (queue, reader) = SendQueue::new(queue::MIN_QUEUE_SIZE);
    assert!(matches!(
        queue.send(join_message(queued, 0)),
        Pushed::Queued
    ));
    let _writer = start_writer(conn, &peer_conn, reader, TEST_FRAME_LIMIT, vec![initial]).await?;

    let (topic, _) = timeout(Duration::from_secs(2), read_next_message(&peer_conn))
        .await
        .std_context("the queued message waited")??;
    assert_eq!(topic, queued);
    Ok(())
}

/// A topic that the writer left with a `Close` gets no initial stream again.
#[tokio::test]
#[traced_test]
async fn left_topic_gets_no_initial_stream() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"left_topic_gets_no_initial_stream").into();
    let (queue, reader) = SendQueue::new(queue::MIN_QUEUE_SIZE);
    assert!(matches!(
        queue.send(disconnect_message(topic)),
        Pushed::Queued
    ));
    let _writer = start_writer(conn, &peer_conn, reader, TEST_FRAME_LIMIT, vec![topic]).await?;

    let (sent, message) = timeout(Duration::from_secs(5), read_next_message(&peer_conn))
        .await
        .std_context("the disconnect did not go out")??;
    assert_eq!(sent, topic);
    assert!(message.is_disconnect());
    assert!(
        timeout(Duration::from_millis(500), peer_conn.accept_uni())
            .await
            .is_err(),
        "the left topic got a stream again"
    );
    Ok(())
}

/// A newer stream of a topic waits until the older one ended, as after a `Close`.
#[tokio::test]
#[traced_test]
async fn streams_of_a_topic_keep_their_order() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let (mut reader, _control) = start_conn_peered(conn, &peer_conn, TEST_FRAME_LIMIT).await?;

    let topic: TopicId = blake3::hash(b"streams_of_a_topic").into();
    let (first, second) = (join_message(topic, 1), join_message(topic, 2));
    let mut buffer = Vec::new();
    let mut older = peer_conn.open_uni().await.std_context("open stream")?;
    wire::StreamHeader::write(topic, None, &mut older, &mut buffer).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    // The newer stream arrives before the last message of the older one.
    let mut newer = peer_conn.open_uni().await.std_context("open stream")?;
    wire::StreamHeader::write(topic, None, &mut newer, &mut buffer).await?;
    wire::write_message(&mut newer, &second.message, &mut buffer, TEST_FRAME_LIMIT).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    wire::write_message(&mut older, &first.message, &mut buffer, TEST_FRAME_LIMIT).await?;
    older.finish().std_context("finish")?;

    for expected in [first, second] {
        assert_eq!(reader.next().await?.message, expected.message);
    }
    Ok(())
}

/// A stream waits until every older stream decoded its header.
///
/// The old stream carries a `Close`, and its header is late. The new
/// stream has its `Join` and its header first. The node must dispatch the
/// `Close` before the `Join`, as the peer wrote them in that order.
#[tokio::test]
#[traced_test]
async fn delayed_header_keeps_close_before_join() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let (mut reader, _control) = start_conn_peered(conn, &peer_conn, TEST_FRAME_LIMIT).await?;

    let topic: TopicId = blake3::hash(b"delayed_header").into();
    let (close, join) = (disconnect_message(topic), join_message(topic, 2));
    let mut buffer = Vec::new();
    // The old stream sends one byte of its header, and the rest comes late.
    let mut older = peer_conn.open_uni().await.std_context("open stream")?;
    older.write_all(&[0x23]).await.std_context("write")?;
    let mut newer = peer_conn.open_uni().await.std_context("open stream")?;
    wire::StreamHeader::write(topic, None, &mut newer, &mut buffer).await?;
    wire::write_message(&mut newer, &join.message, &mut buffer, TEST_FRAME_LIMIT).await?;
    assert!(
        !reader.gets_message_within(Duration::from_millis(500)).await,
        "the newer stream passed the older one"
    );

    older
        .write_all(&[0x01, 0x00, 0x20])
        .await
        .std_context("write")?;
    older
        .write_all(topic.as_bytes())
        .await
        .std_context("write")?;
    wire::write_message(&mut older, &close.message, &mut buffer, TEST_FRAME_LIMIT).await?;
    older.finish().std_context("finish")?;
    assert_eq!(reader.next().await?.message, close.message);
    assert_eq!(reader.next().await?.message, join.message);
    Ok(())
}

/// A send loop whose connection closed returns the reader, for another connection of the peer.
#[tokio::test]
#[traced_test]
async fn closed_send_loop_returns_the_reader() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let (queue, reader) = SendQueue::new(queue::MIN_QUEUE_SIZE);
    let writer = start_writer(conn, &peer_conn, reader, TEST_FRAME_LIMIT, Vec::new()).await?;
    // Once the writer has the queue, the peer closes.
    tokio::time::sleep(Duration::from_millis(100)).await;
    peer_conn.close(0u32.into(), b"done");
    let exit = exited(writer.test.task, Duration::from_secs(3)).await?;
    let message = join_message(TopicId::from_bytes([0; 32]), 0);
    assert!(matches!(queue.send(message), Pushed::Queued));
    let reader = exit.reader.std_context("the reader did not come back")?;
    assert!(reader.reads(&queue));
    assert!(!reader.is_empty(), "the message did not stay queued");
    Ok(())
}

/// A `Close` ends its stream, and a rejoin goes on a new stream.
///
/// The ended stream frees a stream for the peer's limit on open streams.
#[tokio::test]
#[traced_test]
async fn disconnect_ends_its_stream() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"disconnect_ends_its_stream").into();
    let messages = vec![
        join_message(topic, 0),
        disconnect_message(topic),
        join_message(topic, 0),
    ];
    let (queue, reader) = SendQueue::new(queue::MIN_QUEUE_SIZE);
    for message in messages {
        assert!(matches!(queue.send(message), Pushed::Queued));
    }
    queue.close();
    let _writer = start_writer(conn, &peer_conn, reader, 1024, Vec::new()).await?;

    let (_, first) = timeout(Duration::from_secs(5), read_next_stream(&peer_conn))
        .await
        .std_context("the first stream did not end")??;
    let (_, second) = timeout(Duration::from_secs(5), read_next_stream(&peer_conn))
        .await
        .std_context("the second stream did not end")??;

    let first: Vec<bool> = first.iter().map(|m| m.is_disconnect()).collect();
    let second: Vec<bool> = second.iter().map(|m| m.is_disconnect()).collect();
    assert_eq!(first, vec![false, true], "first stream, by `is_disconnect`");
    assert_eq!(second, vec![false], "second stream, by `is_disconnect`");
    Ok(())
}

/// A stream that the peer stopped with the reject code loses its message, and the connection stays.
///
/// The writer tells the actor that the peer refused the topic.
#[tokio::test]
#[traced_test]
async fn reject_stop_drops_only_the_message() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"stopped_stream").into();
    let (queue, reader) = SendQueue::new(queue::MIN_QUEUE_SIZE);
    let mut writer = start_writer(conn, &peer_conn, reader, TEST_FRAME_LIMIT, Vec::new()).await?;
    let feed = async {
        loop {
            queue.send(join_message(topic, 0));
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    let peer = async {
        let mut first = peer_conn.accept_uni().await.std_context("accept_uni")?;
        first.stop(conn::STOP_REJECTED).anyerr()?;
        let event = loop {
            let event = timeout(Duration::from_secs(5), writer.test.events.recv())
                .await
                .std_context("the writer reported no refusal")?
                .std_context("the writer ended")?;
            if !matches!(event.kind, ConnEventKind::Header(..)) {
                break event;
            }
        };
        assert!(matches!(event.kind, ConnEventKind::Refused(t) if t == topic));
        n0_error::Ok(())
    };
    tokio::select! {
        _ = feed => unreachable!(),
        res = peer => res?,
    }
    assert!(peer_conn.close_reason().is_none(), "the connection closed");
    assert!(!writer.test.task.is_finished(), "the writer ended");
    Ok(())
}

/// A stream that the verifier rejects is reported, so that the topic drops the peer.
///
/// Else we would keep a peer as a neighbor that sends us nothing on the topic.
#[tokio::test]
#[traced_test]
async fn rejected_stream_is_reported() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, theirs) = pair.connect_back().await?;
    let reject = TopicAuth::new(|_| Verdict::Reject);
    let mut test = start_conn_with(conn, TEST_FRAME_LIMIT, Duration::from_secs(60), reject);
    let _control = RawControl::open(&theirs).await?;
    let topic: TopicId = blake3::hash(b"rejected_stream").into();
    let _stream = open_stream(&theirs, topic, &[join_message(topic, 0).message]).await?;
    loop {
        let event = timeout(Duration::from_secs(5), test.events.recv())
            .await
            .std_context("no rejection")?
            .std_context("the connection task ended")?;
        match event.kind {
            ConnEventKind::Rejected(t) if t == topic => break,
            ConnEventKind::Message(message) => panic!("a rejected stream got through: {message:?}"),
            _ => {}
        }
    }
    assert!(theirs.close_reason().is_none(), "the connection closed");
    Ok(())
}

/// The actor passes a reject to the protocol, which drops the neighbor and emits no `Refused`.
///
/// The peer did not refuse us, so the application must not see `Refused`.
#[tokio::test]
#[traced_test]
async fn reject_drops_the_neighbor_without_refused() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, _theirs) = pair.connect_back().await?;
    let peer = conn.remote_id();
    let mut actor = ManualActorLoop::new(t_actor().await?).await;
    let topic: TopicId = blake3::hash(b"reject_drops_the_neighbor").into();
    let mut events = actor
        .state
        .topics
        .entry(topic)
        .or_default()
        .events
        .subscribe();
    let now = Instant::now();
    actor.handle_in_event(InEvent::Command(topic, proto::Command::Join(vec![])), now);
    let id = actor.next_conn;
    actor.add_connection(conn);
    let join = ProtoMessage {
        topic,
        message: TopicMessage::Swarm(hyparview::Message::Join(None)),
    };
    actor.handle_in_event(InEvent::RecvMessage(peer, join), now);
    assert!(actor.joined(&topic), "the peer is no neighbor");

    let kind = ConnEventKind::Rejected(topic);
    actor.on_conn_event(
        ConnEvent {
            peer,
            conn: id,
            kind,
        },
        now,
    );
    assert!(!actor.joined(&topic), "the rejected peer stayed a neighbor");
    let mut seen = Vec::new();
    while let Ok(event) = events.try_recv() {
        seen.push(event);
    }
    assert!(
        seen.iter()
            .any(|event| matches!(event, ProtoEvent::NeighborDown(p) if *p == peer)),
        "{seen:?}"
    );
    assert!(
        !seen
            .iter()
            .any(|event| matches!(event, ProtoEvent::Refused(_))),
        "{seen:?}"
    );
    Ok(())
}

/// A membership message of a refused topic reports the refusal again.
///
/// The protocol of the node takes the peer back when the peer sends a second `Join`.
/// The writer drops what the node then queues, so the node must drop the peer again.
#[tokio::test]
#[traced_test]
async fn refused_topic_reports_each_membership_message() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"refused_again").into();
    let (queue, reader) = SendQueue::new(queue::MIN_QUEUE_SIZE);
    let mut writer = start_writer(conn, &peer_conn, reader, TEST_FRAME_LIMIT, Vec::new()).await?;
    let feed = async {
        loop {
            queue.send(join_message(topic, 0));
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    let peer = async {
        let mut first = peer_conn.accept_uni().await.std_context("accept_uni")?;
        first.stop(conn::STOP_REJECTED).anyerr()?;
        let mut refusals = 0;
        while refusals < 2 {
            let event = timeout(Duration::from_secs(5), writer.test.events.recv())
                .await
                .std_context("the writer reported no second refusal")?
                .std_context("the writer ended")?;
            if matches!(event.kind, ConnEventKind::Refused(t) if t == topic) {
                refusals += 1;
            }
        }
        n0_error::Ok(())
    };
    tokio::select! {
        _ = feed => unreachable!(),
        res = peer => res,
    }
}

/// A stream that the peer stopped with another code fails the connection.
#[tokio::test]
#[traced_test]
async fn other_stop_code_fails_the_connection() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"stopped_stream").into();
    let (queue, reader) = SendQueue::new(queue::MIN_QUEUE_SIZE);
    let _writer = start_writer(conn, &peer_conn, reader, TEST_FRAME_LIMIT, Vec::new()).await?;
    let feed = async {
        loop {
            queue.send(join_message(topic, 0));
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    let peer = async {
        let mut first = peer_conn.accept_uni().await.std_context("accept_uni")?;
        first.stop(0u32.into()).anyerr()?;
        closed_with(&peer_conn, CloseCode::SendFailed).await
    };
    tokio::select! {
        _ = feed => unreachable!(),
        res = peer => res,
    }
}

/// A receive loop that ended ends the connection loop despite a live send loop.
///
/// The receive loop ends once the actor takes no more messages. The peer reads a
/// shutdown, so that it drops us as a neighbor.
#[tokio::test]
#[traced_test]
async fn ended_receive_loop_ends_connection_loop() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"ended_receive_loop").into();
    let join = join_message(topic, 0).message;
    let test_conn = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_secs(60));
    let _control = RawControl::accept(&peer_conn).await?;
    // Nobody takes the events: the connection must end by itself.
    drop(test_conn.events);
    let _stream = open_stream(&peer_conn, topic, &[join]).await?;
    timeout(Duration::from_secs(5), test_conn.task)
        .await
        .std_context("connection loop still runs after the actor ended")?
        .std_context("connection task")?;
    closed_with(&peer_conn, CloseCode::Shutdown).await
}

/// A stream with a header that does not decode fails the connection.
#[tokio::test]
#[traced_test]
async fn bad_stream_header_fails_the_connection() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let (reader, _control) = start_conn_peered(conn, &peer_conn, 1024).await?;

    // A complete frame of three bytes, too short for a topic id.
    let mut bad = peer_conn.open_uni().await.std_context("open stream")?;
    bad.write_all(&[3, 1, 2, 3])
        .await
        .std_context("write bad header")?;
    bad.finish().std_context("finish")?;
    closed_with(&peer_conn, CloseCode::Protocol).await?;
    assert_eq!(ended(reader.task, SECS_10).await?, ConnEnd::Failed);
    Ok(())
}

/// A stream whose header was incomplete at the close still delivers its message.
///
/// The loop picks its next branch at random, so a single round can miss the bug.
#[tokio::test]
#[traced_test]
async fn pending_header_is_read_after_a_close() -> Result {
    for round in 0..4 {
        let pair = TestPair::new(1).await?;
        let (conn, peer_conn) = pair.connect().await?;
        let (mut reader, _control) = start_conn_peered(conn.clone(), &peer_conn, 1024).await?;

        let topic: TopicId = [1; 32].into();
        let mut partial = peer_conn.open_uni().await.std_context("open stream")?;
        partial
            .write_all(&[0x23, 0x01])
            .await
            .std_context("write partial header")?;
        let mut stream = open_stream(&peer_conn, topic, &[join_message(topic, 0).message]).await?;
        stream.finish().std_context("finish")?;
        // The loop holds the full stream until the partial header is complete.
        assert!(!reader.gets_message_within(Duration::from_millis(200)).await);

        partial
            .write_all(&[0x00, 0x20])
            .await
            .std_context("write header length")?;
        partial
            .write_all(&[2; 32])
            .await
            .std_context("write topic")?;
        let mut buffer = Vec::new();
        wire::write_message(
            &mut partial,
            &join_message([2; 32].into(), 0).message,
            &mut buffer,
            1024,
        )
        .await
        .std_context("write frame")?;
        partial.finish().std_context("finish")?;
        partial.stopped().await.std_context("stopped")?;
        peer_conn.close(0u32.into(), b"done");
        conn.closed().await;

        let mut got = 0;
        while got < 2 {
            reader
                .next()
                .await
                .with_std_context(|_| format!("round {round}: message {got} is missing"))?;
            got += 1;
        }
        ended(reader.task, Duration::from_secs(2)).await?;
    }
    Ok(())
}

/// A peer that opens streams and sends no header fails the connection after the header deadline.
///
/// The streams after a silent one wait for its header. Letting them pass
/// would break the order of the messages, so the connection ends.
#[tokio::test]
#[traced_test]
async fn silent_header_fails_the_connection() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let (mut reader, _control) = start_conn_peered(conn, &peer_conn, 1024).await?;

    let mut silent = peer_conn.open_uni().await.std_context("open stream")?;
    // The node accepts a stream of the peer only after the peer sends a byte.
    silent.write_all(&[0x23]).await.std_context("write")?;
    let topic: TopicId = [1; 32].into();
    let _stream = open_stream(&peer_conn, topic, &[join_message(topic, 0).message]).await?;
    assert!(!reader.gets_message_within(Duration::from_millis(500)).await);
    let code = close_code(&peer_conn, wire::HEADER_TIMEOUT + Duration::from_secs(3)).await?;
    assert_eq!(code, Some(CloseCode::Protocol));
    assert_eq!(ended(reader.task, SECS_10).await?, ConnEnd::Failed);
    Ok(())
}

/// The reader skips a payload over the size limit, and reads the next frame.
#[tokio::test]
#[traced_test]
async fn frame_over_the_size_limit_is_skipped() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let big = TopicMessage::test_gossip(64);
    let small = TopicMessage::test_prune();
    let size = wire::message_len(&big).std_context("size")?;

    let mut stream = conn.open_uni().await.std_context("open stream")?;
    let mut buffer = Vec::new();
    wire::write_message(&mut stream, &big, &mut buffer, size)
        .await
        .std_context("write")?;
    wire::write_message(&mut stream, &small, &mut buffer, size)
        .await
        .std_context("write")?;
    stream.finish().std_context("finish")?;

    let mut stream = peer_conn.accept_uni().await.std_context("accept")?;
    let mut buffer = Vec::new();
    let read = wire::read_message(&mut stream, &mut buffer, size - 1)
        .await
        .std_context("read")?;
    assert_eq!(read, Some(small));
    let read = wire::read_message(&mut stream, &mut buffer, size - 1)
        .await
        .std_context("read")?;
    assert_eq!(read, None);
    Ok(())
}

/// A frame of the size limit goes through, and one byte more does not.
#[tokio::test]
#[traced_test]
async fn frame_of_the_size_limit_goes_through() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"frame_of_the_size_limit").into();
    let message = join_message(topic, 64).message;
    let size = wire::message_len(&message).std_context("size")?;

    let mut stream = conn.open_uni().await.std_context("open stream")?;
    let mut buffer = Vec::new();
    let refused = wire::write_message(&mut stream, &message, &mut buffer, size - 1).await;
    assert!(
        matches!(refused, Err(wire::WriteError::TooLarge { .. })),
        "{refused:?}"
    );
    wire::write_message(&mut stream, &message, &mut buffer, size)
        .await
        .std_context("write")?;
    stream.finish().std_context("finish")?;

    let mut stream = peer_conn.accept_uni().await.std_context("accept")?;
    let read = wire::read_message(&mut stream, &mut Vec::new(), size)
        .await
        .std_context("read")?;
    assert!(read.is_some(), "the frame did not arrive");
    Ok(())
}

/// The send task drops a payload or a forward join over the size limit, and the
/// connection stays.
///
/// A forward join carries the peer data of the joiner, which our limit does not bound.
#[tokio::test]
#[traced_test]
async fn oversized_message_is_dropped() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"oversized_message").into();
    let (queue, reader) = SendQueue::new(queue::MIN_QUEUE_SIZE);
    let writer = start_writer(conn, &peer_conn, reader, 1024, Vec::new()).await?;

    let payload = ProtoMessage {
        topic,
        message: TopicMessage::test_gossip(4096),
    };
    assert!(matches!(queue.send(payload), Pushed::Queued));
    let joiner = SecretKey::generate().public();
    let forward_join = ProtoMessage {
        topic,
        message: TopicMessage::Swarm(hyparview::Message::test_forward_join(joiner, 4096)),
    };
    assert!(matches!(queue.send(forward_join), Pushed::Queued));
    assert!(matches!(
        queue.send(join_message(topic, 16)),
        Pushed::Queued
    ));

    let (_, message) = timeout(Duration::from_secs(5), read_next_message(&peer_conn))
        .await
        .std_context("nothing arrived after the oversized message")??;
    let size = wire::message_len(&message).std_context("size")?;
    assert!(size < 1024, "the oversized message went out");
    assert!(!writer.test.task.is_finished(), "the writer ended");
    Ok(())
}

/// Appends `value` as a LEB128 varint.
fn varint(buf: &mut Vec<u8>, value: u64) {
    *buf = postcard::to_extend(&value, std::mem::take(buf)).expect("a varint encodes");
}

/// Appends `payload` as a frame.
fn frame(buf: &mut Vec<u8>, payload: &[u8]) {
    varint(buf, payload.len() as u64);
    buf.extend_from_slice(payload);
}

/// Returns a header payload of `kind` with `params`.
fn header(kind: u64, params: &[(u64, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    varint(&mut buf, kind);
    for (id, value) in params {
        varint(&mut buf, *id);
        varint(&mut buf, value.len() as u64);
        buf.extend_from_slice(value);
    }
    buf
}

/// A type code that no version uses yet.
const LATER: u64 = 0x3f;

/// A node skips what a later version may add without a negotiation, and keeps the connection.
///
/// The peer sends unknown parameters in both headers, a frame over the size
/// limit, and bytes after the fields of a known message. The test spells out
/// the wire codes, so a changed code fails it.
#[tokio::test]
#[traced_test]
async fn skips_what_a_later_version_adds() -> Result {
    let max_message_size = 1024;
    let pair = TestPair::new(1).await?;
    let (conn, theirs) = pair.connect_back().await?;
    let mut test = start_conn(conn, max_message_size, Duration::from_secs(60));

    let (mut control, mut control_recv) = theirs.open_bi().await.std_context("open control")?;
    let mut bytes = Vec::new();
    let control_header = header(0x00, &[(LATER, b"later"), (0x01, &PEER_SESSION.0)]);
    frame(&mut bytes, &control_header);
    control
        .write_all(&bytes)
        .await
        .std_context("write control")?;

    let topic = TopicId::from_bytes([7; 32]);
    let mut bytes = Vec::new();
    let topic_header = header(0x01, &[(0x00, topic.as_bytes()), (LATER, b"later")]);
    frame(&mut bytes, &topic_header);
    let mut oversized = vec![0x14];
    oversized.resize(max_message_size + 1, 0);
    frame(&mut bytes, &oversized);
    // A message to the neighbors: type, then the content as postcard bytes.
    let mut known = vec![0x14];
    varint(&mut known, 5);
    known.extend_from_slice(b"hello");
    known.extend_from_slice(b"later fields");
    frame(&mut bytes, &known);
    let mut stream = theirs.open_uni().await.std_context("open stream")?;
    stream.write_all(&bytes).await.std_context("write stream")?;
    stream.finish().std_context("finish stream")?;

    let message = test.next().await?;
    assert_eq!(message.topic, topic);
    let TopicMessage::Gossip(proto::plumtree::Message::Gossip(gossip)) = message.message else {
        panic!("not a payload: {:?}", message.message);
    };
    assert_eq!(gossip.content, &b"hello"[..]);
    assert_eq!(gossip.scope, proto::DeliveryScope::Neighbors);
    Session::read(&mut control_recv, &mut Vec::new())
        .await
        .std_context("read control header")?;
    // The node answers a GOAWAY once our stream ended.
    control
        .write_all(&ControlMessage::Goaway(1).to_frame())
        .await
        .std_context("write GOAWAY")?;
    assert!(matches!(
        read_control(&mut control_recv).await?,
        ControlMessage::Goaway(_)
    ));
    assert!(theirs.close_reason().is_none(), "the connection closed");
    Ok(())
}

/// A message of an unknown type fails the connection, on a topic stream and on the control stream.
///
/// A peer may send a new type only after both peers declared support for it in a header
/// parameter. So an unknown type is a protocol error, not a message to skip.
#[tokio::test]
#[traced_test]
async fn unknown_message_type_fails_the_connection() -> Result {
    for on_control in [false, true] {
        let pair = TestPair::new(1).await?;
        let (conn, theirs) = pair.connect_back().await?;
        let test = start_conn(conn, 1024, Duration::from_secs(60));
        let (mut control, _control_recv) = control_open(&theirs).await?;
        let mut unknown = Vec::new();
        frame(&mut unknown, &[LATER as u8, 1, 2, 3]);
        if on_control {
            control.write_all(&unknown).await.std_context("write")?;
        } else {
            let topic = TopicId::from_bytes([7; 32]);
            let mut bytes = Vec::new();
            frame(&mut bytes, &header(0x01, &[(0x00, topic.as_bytes())]));
            bytes.extend(unknown);
            let mut stream = theirs.open_uni().await.std_context("open stream")?;
            stream.write_all(&bytes).await.std_context("write stream")?;
        }
        closed_with(&theirs, CloseCode::Protocol).await?;
        assert_eq!(ended(test.task, SECS_10).await?, ConnEnd::Failed);
    }
    Ok(())
}

/// A control message over the size limit fails the connection.
///
/// Without it, the membership of the peers would get out of step.
#[tokio::test]
#[traced_test]
async fn oversized_control_message_fails_the_connection() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"oversized_control").into();
    let (queue, reader) = SendQueue::new(queue::MIN_QUEUE_SIZE);
    assert!(matches!(
        queue.send(join_message(topic, 4096)),
        Pushed::Queued
    ));
    let writer = start_writer(conn, &peer_conn, reader, 1024, Vec::new()).await?;
    let end = ended(writer.test.task, Duration::from_secs(5)).await?;
    assert_eq!(end, ConnEnd::Failed);
    closed_with(&peer_conn, CloseCode::SendFailed).await
}
