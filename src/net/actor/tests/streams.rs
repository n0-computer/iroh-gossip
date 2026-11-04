use super::*;

/// Accepts the peer's next stream and reads only its header.
async fn read_next_header(conn: &Connection) -> Result<TopicId> {
    let mut stream = conn.accept_uni().await.std_context("accept stream")?;
    let header = wire::StreamHeader::read(&mut stream, &mut BytesMut::new(), TEST_FRAME_LIMIT)
        .await
        .std_context("read header")?;
    Ok(header.topic)
}

/// The stream header that nodes from 0.90 to 0.101 write and read.
///
/// Their `StreamHeader` and `read_message` are the same in every release from
/// 0.90 to 0.101, and their `read_lp` and `write_message` are the same as ours
/// since 0.95. So `wire::read_message` and `wire::write_message` of this struct
/// read and write a header as those nodes do.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct OldStreamHeader {
    topic_id: TopicId,
}

/// Old and new nodes read the stream headers of each other on a stream, and the frames after them.
#[tokio::test]
#[traced_test]
async fn stream_headers_cross_versions_on_a_stream() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"versions").into();
    let join = join_message(topic, 0).message;
    let encoded_join = postcard::to_stdvec(&join).std_context("encode")?;
    let mut buffer = Vec::new();
    let mut read_buffer = BytesMut::new();

    // We write, an old node reads.
    let mut stream = conn.open_uni().await.std_context("open stream")?;
    wire::StreamHeader::write(topic, &mut stream, &mut buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("write header")?;
    wire::write_message(&mut stream, &join, &mut buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("write frame")?;
    stream.finish().std_context("finish")?;
    let mut recv = peer_conn.accept_uni().await.std_context("accept stream")?;
    let header: OldStreamHeader = wire::read_message(&mut recv, &mut read_buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("old node reads header")?
        .expect("a header");
    assert_eq!(header.topic_id, topic);
    let frame = wire::read_lp(&mut recv, &mut read_buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("old node reads frame")?
        .expect("a frame");
    assert_eq!(frame, encoded_join.as_slice(), "the frame after the header");

    // An old node writes, we read.
    let mut stream = peer_conn.open_uni().await.std_context("open stream")?;
    wire::write_message(
        &mut stream,
        &OldStreamHeader { topic_id: topic },
        &mut buffer,
        TEST_FRAME_LIMIT,
    )
    .await
    .std_context("write old header")?;
    wire::write_message(&mut stream, &join, &mut buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("write frame")?;
    stream.finish().std_context("finish")?;
    let mut recv = conn.accept_uni().await.std_context("accept stream")?;
    let header = wire::StreamHeader::read(&mut recv, &mut read_buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("read old header")?;
    assert_eq!(header.topic, topic);
    assert!(!header.handles_close);
    let frame = wire::read_lp(&mut recv, &mut read_buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("read frame")?
        .expect("a frame");
    assert_eq!(
        frame,
        encoded_join.as_slice(),
        "the frame after the old header"
    );
    Ok(())
}

/// A stream counts from its first byte, so a partial header keeps the connection.
///
/// Once the peer resets the stream, the connection closes after the grace.
#[tokio::test]
#[traced_test]
async fn partial_header_keeps_the_connection() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect_back().await?;
    let mut actor = t_actor().await?;
    actor.add_connection(conn);
    show_that_it_handles_a_close(&peer_conn, [0; 32].into()).await;
    let mut stream = peer_conn.open_uni().await.std_context("open stream")?;
    stream
        .write_all(&[0, 0, 0, 32, 0])
        .await
        .std_context("partial header")?;
    tokio::time::sleep(RETIRE_GRACE + Duration::from_secs(1)).await;
    assert!(peer_conn.close_reason().is_none(), "the connection closed");

    stream.reset(0u32.into()).anyerr()?;
    let result = timeout(
        RETIRE_GRACE + Duration::from_secs(3),
        actor.connection_tasks.join_next(),
    )
    .await
    .std_context("the connection stayed open")?;
    assert!(result.expect("connection task").is_ok());
    Ok(())
}

/// Waits until `flag` has `value`.
async fn flag_becomes(flag: &mut watch::Receiver<bool>, value: bool) -> Result {
    timeout(Duration::from_secs(5), flag.wait_for(|v| *v == value))
        .await
        .std_context("the flag did not change")?
        .anyerr()?;
    Ok(())
}

/// The send loop reports a stream from its open until the peer acknowledged its end.
#[tokio::test]
#[traced_test]
async fn send_loop_reports_its_streams() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let (next, next_peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"send_loop_reports_its_streams").into();
    let mut queue = SendQueue::new(queue::MIN_QUEUE_SIZE);
    // Set, as `connection_loop` creates it for a writer.
    let flag = watch::Sender::new(true);
    let mut has_streams = flag.subscribe();
    let _send = AbortOnDropHandle::new(spawn(
        SendLoop::new(conn, queue.lease(), TEST_FRAME_LIMIT, flag, Vec::new()).run(),
    ));
    flag_becomes(&mut has_streams, false).await?;

    assert!(matches!(queue.push(join_message(topic, 0)), Pushed::Queued));
    let mut stream = peer_conn.accept_uni().await.std_context("accept")?;
    let mut buffer = BytesMut::new();
    wire::StreamHeader::read(&mut stream, &mut buffer, TEST_FRAME_LIMIT).await?;
    let message: Option<TopicMessage> =
        wire::read_message(&mut stream, &mut buffer, TEST_FRAME_LIMIT).await?;
    assert!(message.is_some(), "the message did not go out");
    assert!(*has_streams.borrow(), "the open stream is not reported");

    // A new writer opens a stream for each topic of its lease, and the old one ends.
    let next_flag = unset_flag();
    let mut next_has_streams = next_flag.subscribe();
    let _next_send = AbortOnDropHandle::new(spawn(
        SendLoop::new(
            next,
            queue.lease(),
            TEST_FRAME_LIMIT,
            next_flag,
            vec![topic],
        )
        .run(),
    ));
    let mut next_stream = timeout(Duration::from_secs(5), next_peer_conn.accept_uni())
        .await
        .std_context("the stream did not open again")?
        .std_context("accept")?;
    let header = wire::StreamHeader::read(&mut next_stream, &mut buffer, TEST_FRAME_LIMIT).await?;
    assert_eq!(header.topic, topic);
    let end: Option<TopicMessage> =
        wire::read_message(&mut stream, &mut buffer, TEST_FRAME_LIMIT).await?;
    assert!(end.is_none(), "the old stream did not end");
    flag_becomes(&mut has_streams, false).await?;
    assert!(
        *next_has_streams.borrow(),
        "the stream that opened again is not reported"
    );

    // A closed queue finishes the stream. The flag clears once the peer acknowledged it.
    queue.close();
    let end: Option<TopicMessage> =
        wire::read_message(&mut next_stream, &mut buffer, TEST_FRAME_LIMIT).await?;
    assert!(end.is_none(), "the new stream did not end");
    flag_becomes(&mut next_has_streams, false).await?;
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
    let mut queue = SendQueue::new(queue::MIN_QUEUE_SIZE);
    assert!(matches!(
        queue.push(join_message(queued, 0)),
        Pushed::Queued
    ));
    let _send = AbortOnDropHandle::new(spawn(
        SendLoop::new(
            conn,
            queue.lease(),
            TEST_FRAME_LIMIT,
            unset_flag(),
            vec![initial],
        )
        .run(),
    ));

    let (topic, _) = timeout(Duration::from_secs(2), read_next_message(&peer_conn))
        .await
        .std_context("the queued message waited")??;
    assert_eq!(topic, queued);
    Ok(())
}

/// A topic that the writer left with a `Disconnect` gets no initial stream again.
#[tokio::test]
#[traced_test]
async fn left_topic_gets_no_initial_stream() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"left_topic_gets_no_initial_stream").into();
    let mut queue = SendQueue::new(queue::MIN_QUEUE_SIZE);
    assert!(matches!(
        queue.push(disconnect_message(topic)),
        Pushed::Queued
    ));
    let _send = AbortOnDropHandle::new(spawn(
        SendLoop::new(
            conn,
            queue.lease(),
            TEST_FRAME_LIMIT,
            unset_flag(),
            vec![topic],
        )
        .run(),
    ));

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

/// A newer stream of a topic waits until the older one ended, as after a `Disconnect`.
#[tokio::test]
#[traced_test]
async fn streams_of_a_topic_keep_their_order() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let (in_event_tx, mut in_event_rx) = mpsc::channel(4);
    let mut recv_loop = RecvLoop::new(
        peer_conn.remote_id(),
        conn,
        in_event_tx,
        TEST_FRAME_LIMIT,
        unset_flag(),
        unset_flag(),
    );
    let _recv = AbortOnDropHandle::new(spawn(async move { recv_loop.run().await }));

    let topic: TopicId = blake3::hash(b"streams_of_a_topic").into();
    let (first, second) = (join_message(topic, 1), join_message(topic, 2));
    let mut buffer = Vec::new();
    let mut older = peer_conn.open_uni().await.std_context("open stream")?;
    wire::StreamHeader::write(topic, &mut older, &mut buffer, TEST_FRAME_LIMIT).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    // The newer stream arrives before the last message of the older one.
    let mut newer = peer_conn.open_uni().await.std_context("open stream")?;
    wire::StreamHeader::write(topic, &mut newer, &mut buffer, TEST_FRAME_LIMIT).await?;
    wire::write_message(&mut newer, &second.message, &mut buffer, TEST_FRAME_LIMIT).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    wire::write_message(&mut older, &first.message, &mut buffer, TEST_FRAME_LIMIT).await?;
    older.finish().std_context("finish")?;

    for expected in [first, second] {
        let event = timeout(Duration::from_secs(5), in_event_rx.recv())
            .await
            .std_context("no message")?;
        assert!(
            matches!(&event, Some(InEvent::RecvMessage(_, m)) if m.message == expected.message),
            "{event:?}"
        );
    }
    Ok(())
}

/// An older stream whose header comes after a newer one is read at once, not after it.
#[tokio::test]
#[traced_test]
async fn late_header_of_an_older_stream_does_not_wait() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let (in_event_tx, mut in_event_rx) = mpsc::channel(4);
    let mut recv_loop = RecvLoop::new(
        peer_conn.remote_id(),
        conn,
        in_event_tx,
        TEST_FRAME_LIMIT,
        unset_flag(),
        unset_flag(),
    );
    let _recv = AbortOnDropHandle::new(spawn(async move { recv_loop.run().await }));

    let topic: TopicId = blake3::hash(b"late_header").into();
    let (first, second) = (join_message(topic, 1), join_message(topic, 2));
    let mut buffer = Vec::new();
    let mut older = peer_conn.open_uni().await.std_context("open stream")?;
    older.write_all(&[0]).await.std_context("write")?;
    let mut newer = peer_conn.open_uni().await.std_context("open stream")?;
    wire::StreamHeader::write(topic, &mut newer, &mut buffer, TEST_FRAME_LIMIT).await?;
    wire::write_message(&mut newer, &second.message, &mut buffer, TEST_FRAME_LIMIT).await?;
    let mut expect = async |expected: ProtoMessage| {
        let event = timeout(Duration::from_secs(5), in_event_rx.recv())
            .await
            .std_context("no message")?;
        assert!(
            matches!(&event, Some(InEvent::RecvMessage(_, m)) if m.message == expected.message),
            "{event:?}"
        );
        n0_error::Ok(())
    };
    expect(second).await?;
    // The rest of the older header, then a message. The newer stream stays open.
    let header = postcard::to_stdvec(&(topic, 1u8)).std_context("encode")?;
    let len = (header.len() as u32).to_be_bytes();
    older.write_all(&len[1..]).await.std_context("write")?;
    older.write_all(&header).await.std_context("write")?;
    wire::write_message(&mut older, &first.message, &mut buffer, TEST_FRAME_LIMIT).await?;
    older.finish().std_context("finish")?;
    expect(first).await?;
    Ok(())
}

/// A send loop whose connection closed lets go of the queue, for another connection of the peer.
#[tokio::test]
#[traced_test]
async fn closed_send_loop_lets_go_of_the_queue() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let mut queue = SendQueue::new(queue::MIN_QUEUE_SIZE);
    let send = SendLoop::new(
        conn,
        queue.lease(),
        TEST_FRAME_LIMIT,
        unset_flag(),
        Vec::new(),
    );
    peer_conn.close(0u32.into(), b"done");
    timeout(Duration::from_secs(3), send.run())
        .await
        .std_context("the send loop did not end")??;
    let message = join_message(TopicId::from_bytes([0; 32]), 0);
    assert!(matches!(queue.push(message), Pushed::Queued));
    let reader = queue
        .try_reader()
        .std_context("the queue is still locked")?;
    assert!(!reader.is_empty(), "the message did not stay queued");
    Ok(())
}

/// A connection that takes over the writer opens a stream for each topic that uses the peer.
#[tokio::test]
#[traced_test]
async fn new_writer_opens_the_streams_of_the_topics() -> Result {
    let pair = TestPair::new(1).await?;
    let (old, peer_old) = pair.connect_back().await?;
    let (new, peer_new) = pair.connect_back().await?;
    let peer_id = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"new_writer_opens").into();
    let mut actor = t_actor().await?;
    actor.add_connection(old);
    let join = InEvent::Command(topic, ProtoCommand::Join(vec![peer_id]));
    actor.handle_in_event(join, Instant::now());
    let (sent, _) = timeout(Duration::from_secs(5), read_next_message(&peer_old))
        .await
        .std_context("the join did not go out")??;
    assert_eq!(sent, topic);

    actor.add_connection(new);
    let opened = timeout(Duration::from_secs(5), read_next_header(&peer_new))
        .await
        .std_context("the new writer opened no stream")??;
    assert_eq!(opened, topic);
    Ok(())
}

/// A `Disconnect` ends its stream, and a rejoin goes on a new stream.
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
    let mut queue = SendQueue::new(queue::MIN_QUEUE_SIZE);
    for message in messages {
        assert!(matches!(queue.push(message), Pushed::Queued));
    }
    let send_loop = SendLoop::new(conn, queue.lease(), 1024, unset_flag(), Vec::new());
    drop(queue);
    let _send = AbortOnDropHandle::new(spawn(async move { send_loop.run().await }));

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

/// After the peer stopped a stream, the next message of the topic goes out on a new stream.
///
/// The peer stops a stream that it cannot read, for example a frame over its
/// size limit.
#[tokio::test]
#[traced_test]
async fn next_message_after_a_stop_opens_a_new_stream() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let peer_id = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"stopped_stream").into();

    // The peer stops the first stream we open, and reads the next one.
    let peer = AbortOnDropHandle::new(spawn(async move {
        let mut first = peer_conn.accept_uni().await.expect("accept_uni");
        first.stop(0u32.into()).expect("stop");
        let mut next = peer_conn.accept_uni().await.expect("accept_uni");
        let mut buffer = BytesMut::new();
        let header = wire::StreamHeader::read(&mut next, &mut buffer, TEST_FRAME_LIMIT)
            .await
            .expect("read header");
        let message: Option<TopicMessage> =
            wire::read_message(&mut next, &mut buffer, TEST_FRAME_LIMIT)
                .await
                .expect("read frame");
        (header.topic, message.is_some(), peer_conn)
    }));
    let mut queue = SendQueue::new(queue::MIN_QUEUE_SIZE);
    let queue_lease = queue.lease();
    let aborted = CancellationToken::new();
    let _feed = AbortOnDropHandle::new(spawn(async move {
        loop {
            queue.push(join_message(topic, 0));
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }));
    let (in_event_tx, _in_event_rx) = mpsc::channel(16);
    let _conn_loop = AbortOnDropHandle::new(spawn(connection_loop(
        peer_id,
        conn,
        aborted,
        Some(queue_lease),
        Vec::new(),
        in_event_tx,
        TEST_FRAME_LIMIT,
        unset_flag(),
    )));

    let (read_topic, read_message, peer_conn) = timeout(Duration::from_secs(5), peer)
        .await
        .std_context("no message on a new stream")?
        .std_context("peer task")?;
    assert_eq!(read_topic, topic);
    assert!(read_message, "the new stream had no message");
    assert!(peer_conn.close_reason().is_none(), "the connection closed");
    Ok(())
}

/// A receive loop that ended ends the connection loop despite a live send loop.
///
/// The receive loop ends once the actor takes no more messages. A bad
/// stream header does not end it: it drops only its stream.
#[tokio::test]
#[traced_test]
async fn ended_receive_loop_ends_connection_loop() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"ended_receive_loop").into();
    let join = join_message(topic, 0).message;
    let _stream = open_stream(&peer_conn, topic, &[join]).await?;

    // The queue stays alive, so only the receive loop can end the connection.
    let mut queue = SendQueue::new(queue::MIN_QUEUE_SIZE);
    let (in_event_tx, in_event_rx) = mpsc::channel(16);
    drop(in_event_rx);
    timeout(
        Duration::from_secs(5),
        connection_loop(
            pair.ep2.id(),
            conn,
            CancellationToken::new(),
            Some(queue.lease()),
            Vec::new(),
            in_event_tx,
            TEST_FRAME_LIMIT,
            unset_flag(),
        ),
    )
    .await
    .std_context("connection loop still runs after the receive loop ended")??;
    Ok(())
}

/// A stream with a header that does not decode does not end the other streams.
#[tokio::test]
#[traced_test]
async fn bad_header_drops_only_its_stream() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let (in_event_tx, mut in_event_rx) = mpsc::channel(4);
    let mut recv_loop = RecvLoop::new(
        peer_conn.remote_id(),
        conn,
        in_event_tx,
        1024,
        unset_flag(),
        unset_flag(),
    );
    let _recv = AbortOnDropHandle::new(spawn(async move { recv_loop.run().await }));

    // A complete frame of three bytes, too short for a topic id.
    let mut bad = peer_conn.open_uni().await.std_context("open stream")?;
    bad.write_all(&[0, 0, 0, 3, 1, 2, 3])
        .await
        .std_context("write bad header")?;
    bad.finish().std_context("finish")?;
    let topic: TopicId = [1; 32].into();
    let _stream = open_stream(&peer_conn, topic, &[join_message(topic, 0).message]).await?;

    let event = timeout(Duration::from_secs(2), in_event_rx.recv())
        .await
        .std_context("the bad header ended the receive loop")?;
    assert!(matches!(event, Some(InEvent::RecvMessage(..))), "{event:?}");
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
        let (in_event_tx, mut in_event_rx) = mpsc::channel(2);
        let mut recv_loop = RecvLoop::new(
            peer_conn.remote_id(),
            conn.clone(),
            in_event_tx,
            1024,
            unset_flag(),
            unset_flag(),
        );
        // We poll it by hand, so that it sees the rest of the header only after the close.
        let run = recv_loop.run();
        tokio::pin!(run);

        let topic: TopicId = [1; 32].into();
        let mut partial = peer_conn.open_uni().await.std_context("open stream")?;
        partial
            .write_all(&[0, 0])
            .await
            .std_context("write partial header")?;
        // The loop accepts streams in order, so this message means it has the partial one.
        let mut stream = open_stream(&peer_conn, topic, &[join_message(topic, 0).message]).await?;
        stream.finish().std_context("finish")?;
        tokio::select! {
            res = &mut run => panic!("the recv loop ended: {res:?}"),
            event = in_event_rx.recv() => {
                assert!(matches!(event, Some(InEvent::RecvMessage(..))), "{event:?}");
            }
        }
        // Let the loop see the end of the full stream, so that only the header remains.
        stream.stopped().await.std_context("stopped")?;
        let res = n0_future::future::poll_once(&mut run).await;
        assert!(res.is_none(), "the recv loop ended: {res:?}");

        partial
            .write_all(&[0, 32])
            .await
            .std_context("write header length")?;
        partial
            .write_all(&[1; 32])
            .await
            .std_context("write topic")?;
        let mut buffer = Vec::new();
        wire::write_message(
            &mut partial,
            &join_message(topic, 0).message,
            &mut buffer,
            1024,
        )
        .await
        .std_context("write frame")?;
        partial.finish().std_context("finish")?;
        partial.stopped().await.std_context("stopped")?;
        peer_conn.close(0u32.into(), b"done");
        conn.closed().await;

        timeout(Duration::from_secs(2), &mut run)
            .await
            .std_context("the recv loop did not end")?;
        let event = in_event_rx.try_recv();
        assert!(
            matches!(event, Ok(InEvent::RecvMessage(..))),
            "round {round}: {event:?}"
        );
    }
    Ok(())
}

/// A peer with too many partial headers waits until one of them is complete.
#[tokio::test]
#[traced_test]
async fn partial_headers_are_limited() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let (in_event_tx, mut in_event_rx) = mpsc::channel(4);
    let mut recv_loop = RecvLoop::new(
        peer_conn.remote_id(),
        conn,
        in_event_tx,
        1024,
        unset_flag(),
        unset_flag(),
    );
    let _recv = AbortOnDropHandle::new(spawn(async move { recv_loop.run().await }));

    let mut partials = Vec::new();
    for _ in 0..conn::MAX_PENDING_HEADERS {
        let mut stream = peer_conn.open_uni().await.std_context("open stream")?;
        stream
            .write_all(&[0, 0])
            .await
            .std_context("write partial header")?;
        partials.push(stream);
    }
    let topic: TopicId = [1; 32].into();
    let _stream = open_stream(&peer_conn, topic, &[join_message(topic, 0).message]).await?;
    let event = timeout(Duration::from_millis(500), in_event_rx.recv()).await;
    assert!(event.is_err(), "the loop read more headers than its limit");

    let partial = &mut partials[0];
    partial
        .write_all(&[0, 32])
        .await
        .std_context("write header length")?;
    // Another topic, so that its open stream does not hold back the one above.
    partial
        .write_all(&[2; 32])
        .await
        .std_context("write topic")?;
    let event = timeout(Duration::from_secs(2), in_event_rx.recv())
        .await
        .std_context("a complete header did not free its place")?;
    assert!(matches!(event, Some(InEvent::RecvMessage(..))), "{event:?}");
    Ok(())
}

/// A stream with an incomplete header does not block the other streams.
#[tokio::test]
#[traced_test]
async fn partial_header_does_not_block_other_streams() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let (in_event_tx, mut in_event_rx) = mpsc::channel(4);
    let mut recv_loop = RecvLoop::new(
        peer_conn.remote_id(),
        conn,
        in_event_tx,
        1024,
        unset_flag(),
        unset_flag(),
    );
    let _recv = AbortOnDropHandle::new(spawn(async move { recv_loop.run().await }));

    let mut partial = peer_conn.open_uni().await.std_context("open stream")?;
    partial
        .write_all(&[0, 0, 0, 32, 0])
        .await
        .std_context("write partial header")?;
    let topic: TopicId = [1; 32].into();
    let _stream = open_stream(&peer_conn, topic, &[join_message(topic, 0).message]).await?;

    let event = timeout(Duration::from_secs(2), in_event_rx.recv())
        .await
        .std_context("the partial header blocked the other stream")?;
    assert!(matches!(event, Some(InEvent::RecvMessage(..))), "{event:?}");
    Ok(())
}

/// A frame one byte over the size limit is refused on read.
#[tokio::test]
#[traced_test]
async fn frame_over_the_size_limit_is_refused() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"frame_over_the_size_limit").into();
    let message = join_message(topic, 64).message;
    let size = postcard::experimental::serialized_size(&message).std_context("size")?;

    let mut stream = conn.open_uni().await.std_context("open stream")?;
    wire::write_message(&mut stream, &message, &mut Vec::new(), size + 1)
        .await
        .std_context("write")?;
    stream.finish().std_context("finish")?;

    let mut stream = peer_conn.accept_uni().await.std_context("accept")?;
    let read: Result<Option<TopicMessage>, _> =
        wire::read_message(&mut stream, &mut BytesMut::new(), size - 1).await;
    assert!(
        matches!(read, Err(wire::ReadError::TooLarge { .. })),
        "{read:?}"
    );
    Ok(())
}

/// A frame shorter than the size limit goes through, and one of the limit does not.
///
/// Nodes up to 0.101 read a frame of the limit, but refuse to write one, so
/// they could not forward it.
#[tokio::test]
#[traced_test]
async fn frame_shorter_than_the_size_limit_goes_through() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"frame_shorter_than_the_size_limit").into();
    let message = join_message(topic, 64).message;
    let size = postcard::experimental::serialized_size(&message).std_context("size")?;

    let mut stream = conn.open_uni().await.std_context("open stream")?;
    let mut buffer = Vec::new();
    let refused = wire::write_message(&mut stream, &message, &mut buffer, size).await;
    assert!(
        matches!(refused, Err(wire::WriteError::TooLarge { .. })),
        "{refused:?}"
    );
    wire::write_message(&mut stream, &message, &mut buffer, size + 1)
        .await
        .std_context("write")?;
    stream.finish().std_context("finish")?;

    let mut stream = peer_conn.accept_uni().await.std_context("accept")?;
    let read: Option<TopicMessage> = wire::read_message(&mut stream, &mut BytesMut::new(), size)
        .await
        .std_context("read")?;
    assert!(read.is_some(), "the frame did not arrive");
    Ok(())
}

/// The send task drops a message over the size limit, and the connection stays.
#[tokio::test]
#[traced_test]
async fn oversized_message_is_dropped() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let peer_id = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"oversized_message").into();
    let mut queue = SendQueue::new(queue::MIN_QUEUE_SIZE);
    let (in_event_tx, _in_event_rx) = mpsc::channel(16);
    let task = AbortOnDropHandle::new(spawn(connection_loop(
        peer_id,
        conn,
        CancellationToken::new(),
        Some(queue.lease()),
        Vec::new(),
        in_event_tx,
        1024,
        unset_flag(),
    )));

    assert!(matches!(
        queue.push(join_message(topic, 4096)),
        Pushed::Queued
    ));
    assert!(matches!(
        queue.push(join_message(topic, 16)),
        Pushed::Queued
    ));

    let (_, message) = timeout(Duration::from_secs(5), read_next_message(&peer_conn))
        .await
        .std_context("nothing arrived after the oversized message")??;
    let size = postcard::experimental::serialized_size(&message).std_context("size")?;
    assert!(size < 1024, "the oversized message went out");
    assert!(!task.is_finished(), "the connection ended");
    Ok(())
}
