use super::*;

/// A flag change during a `retire_ends_after` run.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// Our send loop reports whether it has streams.
    Ours(bool),
    /// The receive loop reports whether the peer has streams.
    Theirs(bool),
}

/// Runs `retire` on paused time, applies `steps` at their offsets, and returns when it ended.
///
/// Each step comes at its offset in seconds. `None` means `retire` did not end within a minute.
async fn retire_ends_after(steps: &[(u64, Step)]) -> Option<Duration> {
    let ours = unset_flag();
    let theirs = unset_flag();
    let start = Instant::now();
    let mut retire = std::pin::pin!(retire(ours.subscribe(), theirs.subscribe()));
    for (at, step) in steps {
        let at = start + Duration::from_secs(*at);
        tokio::select! {
            _ = &mut retire => return Some(start.elapsed()),
            _ = n0_future::time::sleep_until(at) => {}
        }
        match step {
            Step::Ours(open) => ours.send_replace(*open),
            Step::Theirs(open) => theirs.send_replace(*open),
        };
    }
    timeout(Duration::from_secs(60), retire)
        .await
        .ok()
        .map(|_| start.elapsed())
}

/// A connection without streams retires after the grace.
#[tokio::test(start_paused = true)]
async fn retire_after_the_grace_without_streams() {
    assert_eq!(retire_ends_after(&[]).await, Some(RETIRE_GRACE));
}

/// A stream in either direction keeps the connection, and the grace starts once both ended.
#[tokio::test(start_paused = true)]
async fn retire_waits_for_the_last_stream() {
    assert_eq!(retire_ends_after(&[(1, Step::Ours(true))]).await, None);
    assert_eq!(retire_ends_after(&[(1, Step::Theirs(true))]).await, None);
    assert_eq!(
        retire_ends_after(&[
            (1, Step::Ours(true)),
            (2, Step::Theirs(true)),
            (3, Step::Ours(false)),
            (4, Step::Theirs(false))
        ])
        .await,
        Some(Duration::from_secs(4) + RETIRE_GRACE)
    );
}

/// A stream that opens and ends within the grace starts it over.
#[tokio::test(start_paused = true)]
async fn retire_starts_over_after_a_short_stream() {
    assert_eq!(
        retire_ends_after(&[(3, Step::Theirs(true)), (4, Step::Theirs(false))]).await,
        Some(Duration::from_secs(4) + RETIRE_GRACE)
    );
}

/// A retired writer does not disconnect the peer: an empty queue goes, a queued message dials.
#[test]
fn retired_writer_does_not_disconnect() {
    let now = Instant::now();
    let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
    let _lease = peer.handle_connection(Side::Server, 1).unwrap();
    assert_eq!(peer.connection_finished(1, now, true), Finished::Quiet);
    assert!(is_idle(&peer));
    assert!(
        peer.can_forget(now + PEER_RETENTION),
        "the peer state stays"
    );

    let _lease = peer.handle_connection(Side::Server, 2).unwrap();
    assert!(matches!(
        peer.send(join_message([3; 32].into(), 0)),
        Pushed::Queued
    ));
    assert_eq!(peer.connection_finished(2, now, true), Finished::Redial);
    assert!(peer.needs_dial(), "the queued message waits without a dial");
}

/// A failed dial after a disconnect drops the queue it kept.
#[tokio::test]
async fn failed_retired_dial_releases_its_queue() -> Result {
    let mut actor = t_actor().await?;
    let peer = SecretKey::generate().public();
    actor
        .state
        .peers
        .insert(peer, pending_with(vec![join_message([1; 32].into(), 0)]));
    actor
        .state
        .peers
        .get_mut(&peer)
        .expect("known peer")
        .queue
        .close();
    actor.on_dial_failed(peer);
    assert!(is_idle(&actor.state.peers[&peer]));
    Ok(())
}

/// An unused connection to a peer whose header has no flags stays open.
///
/// Nodes up to 0.101 send the topic alone. Such a node would lose its later
/// messages to us if we closed a connection that is no neighbor link of it.
#[tokio::test]
#[traced_test]
async fn unused_connection_from_an_old_peer_stays() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect_back().await?;
    let peer_id = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"old_peer").into();
    let mut actor = t_actor().await?;
    actor.add_connection(conn);
    let mut stream = peer_conn.open_uni().await.std_context("open stream")?;
    let mut buffer = Vec::new();
    wire::write_message(&mut stream, &topic, &mut buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("write header")?;
    let join = join_message(topic, 0).message;
    wire::write_message(&mut stream, &join, &mut buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("write frame")?;
    let event = timeout(Duration::from_secs(5), actor.in_event_rx.recv())
        .await
        .std_context("the message did not arrive")?;
    assert!(
        matches!(event, Some(InEvent::RecvMessage(peer, _)) if peer == peer_id),
        "{event:?}"
    );

    tokio::time::sleep(RETIRE_GRACE + Duration::from_secs(1)).await;
    assert!(
        actor.connection_tasks.try_join_next().is_none(),
        "the connection closed"
    );
    assert!(peer_conn.close_reason().is_none(), "the connection closed");
    Ok(())
}

/// A control overflow during a handover closes the stuck old connection, not the new one (a cost of the design).
///
/// The old connection does not read, so its writer keeps the queue for up
/// to the handover limit, and control fills the queue meanwhile. Today the
/// overflow aborts the peer, and closes the new and healthy connection too.
#[tokio::test(flavor = "current_thread")]
#[traced_test]
#[ignore = "a cost of the design, see SendQueue"]
async fn overflow_during_a_handover_keeps_the_new_connection() -> Result {
    let pair = TestPair::new(1).await?;
    let (old, peer_old) = pair.connect_back().await?;
    let (new, peer_new) = pair.connect_back().await?;
    let peer = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"overflow during a handover").into();
    let mut actor = t_actor().await?;
    actor.state.send_queue_size = 8 << 20;
    actor.add_connection(old);
    for _ in 0..1000 {
        let message = ProtoMessage {
            topic,
            message: TopicMessage::test_gossip(3000),
        };
        push_active(&actor, peer, message);
    }
    // Wait until flow control blocks the old writer.
    tokio::time::sleep(Duration::from_secs(1)).await;
    actor.add_connection(new);

    // Large joins fill the queue before a writer runs again: nothing awaits.
    let data = PeerData::new(vec![0u8; 2000]);
    actor.handle_in_event(InEvent::UpdatePeerData(data), Instant::now());
    join_until_full(&mut actor, peer);
    assert_eq!(
        actor.state.metrics.peers_send_queue_full.get(),
        1,
        "no overflow"
    );
    timeout(Duration::from_secs(2), peer_old.closed())
        .await
        .std_context("the stuck old connection stayed open")?;
    assert!(
        peer_new.close_reason().is_none(),
        "the new connection closed"
    );
    Ok(())
}

/// A close by the peer with the retire code counts as a retirement, also over a stream of ours.
#[tokio::test]
#[traced_test]
async fn retirement_by_the_peer_counts() -> Result {
    let pair = TestPair::new(1).await?;
    let topic: TopicId = blake3::hash(b"peer_retirement").into();
    for with_stream in [false, true] {
        let (conn, peer_conn) = pair.connect().await?;
        let mut queue = SendQueue::new(queue::MIN_QUEUE_SIZE);
        let (in_event_tx, _in_event_rx) = mpsc::channel(16);
        let task = AbortOnDropHandle::new(spawn(connection_loop(
            pair.ep2.id(),
            conn,
            CancellationToken::new(),
            Some(queue.lease()),
            Vec::new(),
            in_event_tx,
            TEST_FRAME_LIMIT,
            unset_flag(),
        )));
        if with_stream {
            assert!(matches!(queue.send(join_message(topic, 0)), Pushed::Queued));
            timeout(Duration::from_secs(5), read_next_message(&peer_conn))
                .await
                .std_context("the message did not go out")??;
        }
        peer_conn.close(conn::CLOSE_RETIRED.into(), b"no stream");
        let retired = timeout(Duration::from_secs(5), task)
            .await
            .std_context("the connection loop did not end")?
            .anyerr()??;
        assert!(retired, "with a stream of ours: {with_stream}");
    }
    Ok(())
}

/// A connection that the peer closes while no topic uses it counts as retired.
#[tokio::test]
#[traced_test]
async fn close_of_a_closed_queue_counts_as_retired() -> Result {
    let pair = TestPair::new(1).await?;
    for closed in [false, true] {
        let (conn, peer_conn) = pair.connect().await?;
        let mut queue = SendQueue::new(queue::MIN_QUEUE_SIZE);
        if closed {
            queue.close();
        }
        let (in_event_tx, _in_event_rx) = mpsc::channel(16);
        let task = AbortOnDropHandle::new(spawn(connection_loop(
            pair.ep2.id(),
            conn,
            CancellationToken::new(),
            Some(queue.lease()),
            Vec::new(),
            in_event_tx,
            TEST_FRAME_LIMIT,
            unset_flag(),
        )));
        // As a node up to 0.101 closes a connection when it disconnects us.
        peer_conn.close(0u32.into(), b"close from disconnect");
        let retired = timeout(Duration::from_secs(5), task)
            .await
            .std_context("the connection loop did not end")?
            .anyerr()??;
        assert_eq!(retired, closed);
    }
    Ok(())
}

/// A replaced connection hands the rest of the queue to the new one, which keeps its order.
///
/// The old writer never took the queue, so all messages go out on the new
/// connection, in one stream and in the order of the queue.
///
/// The runtime has one thread, so the old writer cannot take the queue before the test does.
#[tokio::test(flavor = "current_thread")]
#[traced_test]
async fn replaced_connection_hands_over_its_queue() -> Result {
    let pair = TestPair::new(1).await?;
    let (old, peer_old) = pair.connect_back().await?;
    let (new, peer_new) = pair.connect_back().await?;
    let peer_id = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"handed over").into();
    let mut actor = t_actor().await?;
    actor.add_connection(old);
    // The old writer cannot take the queue before the new connection replaces it.
    let held = active_queue(&actor, peer_id)
        .try_reader()
        .std_context("the queue is locked")?;
    push_active(&actor, peer_id, join_message(topic, 1));
    actor.add_connection(new);
    push_active(&actor, peer_id, join_message(topic, 2));
    drop(held);

    let read = async {
        let mut stream = peer_new.accept_uni().await.std_context("accept")?;
        let mut buffer = BytesMut::new();
        wire::StreamHeader::read(&mut stream, &mut buffer, TEST_FRAME_LIMIT).await?;
        let mut sizes = Vec::new();
        for _ in 0..2 {
            let message: TopicMessage =
                wire::read_message(&mut stream, &mut buffer, TEST_FRAME_LIMIT)
                    .await?
                    .std_context("the stream ended")?;
            sizes.push(postcard::experimental::serialized_size(&message).anyerr()?);
        }
        Ok::<_, AnyError>(sizes)
    };
    let sizes = timeout(Duration::from_secs(5), read)
        .await
        .std_context("the queue did not go out on the new connection")??;
    assert!(sizes[0] < sizes[1], "the messages lost their order");
    assert!(
        timeout(Duration::from_millis(500), peer_old.accept_uni())
            .await
            .is_err(),
        "the old connection sent a message"
    );
    Ok(())
}

/// A replaced connection that cannot finish its message hands it to the new one, and loses nothing.
///
/// The old connection does not read, so flow control blocks its writer in
/// the middle of a message. The new connection writes that message first.
/// The old stream ends after the part of it: the peer gets each message
/// before it on the old connection, and that message once. The old
/// connection stays open, as the peer may still send on it.
#[tokio::test]
#[traced_test]
async fn stuck_replaced_connection_hands_over_its_queue() -> Result {
    let pair = TestPair::new(1).await?;
    let (old, peer_old) = pair.connect_back().await?;
    let (new, peer_new) = pair.connect_back().await?;
    let peer_id = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"stuck").into();
    let mut actor = t_actor().await?;
    actor.state.send_queue_size = 64 << 20;
    actor.add_connection(old);
    push_many(&actor, peer_id, topic, 3000);
    // Wait until flow control blocks the old writer.
    tokio::time::sleep(Duration::from_secs(1)).await;
    actor.add_connection(new);

    let mut new_stream = timeout(conn::HANDOVER_LIMIT * 3, peer_new.accept_uni())
        .await
        .std_context("the new connection sent nothing")?
        .std_context("accept")?;
    let mut new_buffer = BytesMut::new();
    let header =
        wire::StreamHeader::read(&mut new_stream, &mut new_buffer, TEST_FRAME_LIMIT).await?;
    assert_eq!(header.topic, topic);
    let last = numbered(topic, 2999).message;
    let read_new = async {
        let mut messages = Vec::new();
        while messages.last() != Some(&last) {
            match wire::read_message(&mut new_stream, &mut new_buffer, TEST_FRAME_LIMIT).await {
                Ok(Some(message)) => messages.push(message),
                _ => break,
            }
        }
        messages
    };
    let new_messages = timeout(Duration::from_secs(10), read_new)
        .await
        .std_context("the new connection did not send the rest")?;
    let handed_over = (0..3000)
        .find(|&i| Some(&numbered(topic, i).message) == new_messages.first())
        .std_context("an unknown message")?;
    let rest: Vec<_> = (handed_over..3000)
        .map(|i| numbered(topic, i).message)
        .collect();
    assert!(
        new_messages == rest,
        "the new connection sent {} messages, not those from {handed_over}",
        new_messages.len()
    );

    let mut stream = peer_old.accept_uni().await.std_context("accept")?;
    let mut buffer = BytesMut::new();
    wire::StreamHeader::read(&mut stream, &mut buffer, TEST_FRAME_LIMIT).await?;
    let read = async {
        let mut messages = Vec::new();
        // The part of the handed over message ends the stream with an error.
        while let Ok(Some(message)) =
            wire::read_message::<TopicMessage>(&mut stream, &mut buffer, TEST_FRAME_LIMIT).await
        {
            messages.push(message);
        }
        messages
    };
    let messages = timeout(Duration::from_secs(5), read)
        .await
        .std_context("the stuck stream did not end")?;
    let expected: Vec<_> = (0..handed_over)
        .map(|i| numbered(topic, i).message)
        .collect();
    assert!(
        messages == expected,
        "the old connection sent {} messages, not those before {handed_over}",
        messages.len()
    );
    assert!(
        peer_old.close_reason().is_none(),
        "the old connection closed"
    );
    Ok(())
}

/// A peer that fails during a handover loses both connections at once.
///
/// The old writer is stuck in a message and waits out the handover limit.
/// The abort must not wait for it, nor for its streams to finish.
#[tokio::test]
#[traced_test]
async fn aborted_peer_during_a_handover_loses_both_connections() -> Result {
    let pair = TestPair::new(1).await?;
    let (old, peer_old) = pair.connect_back().await?;
    let (new, peer_new) = pair.connect_back().await?;
    let peer_id = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"failed handover").into();
    let mut actor = t_actor().await?;
    actor.state.send_queue_size = 64 << 20;
    actor.add_connection(old);
    push_many(&actor, peer_id, topic, 3000);
    // Wait until flow control blocks the old writer.
    tokio::time::sleep(Duration::from_secs(1)).await;
    actor.add_connection(new);

    actor
        .state
        .peers
        .get_mut(&peer_id)
        .expect("known peer")
        .abort(Instant::now());
    closed_with(&peer_old, conn::CLOSE_QUEUE_FULL).await?;
    closed_with(&peer_new, conn::CLOSE_QUEUE_FULL).await
}

/// A replaced connection that waits to open a stream hands the queue over at once.
///
/// The peer allows one stream per connection, and the old connection used
/// it, so its writer waits to open the stream of a second topic. Nothing
/// of that stream went out, so the new connection takes the message at
/// once, not after the open timeout.
#[tokio::test]
#[traced_test]
async fn replaced_connection_waiting_to_open_hands_over_at_once() -> Result {
    let lookup = MemoryLookup::new();
    let ours = direct_endpoint(&lookup, QuicTransportConfig::builder().build()).await?;
    let one_stream = QuicTransportConfig::builder()
        .max_concurrent_uni_streams(VarInt::from_u32(1))
        .build();
    let theirs = direct_endpoint(&lookup, one_stream).await?;
    // The peer dials us, so the connection we keep is the server side and
    // takes the queue. The peer still limits the streams we open to it.
    let connect = || async {
        let dial = async {
            theirs
                .connect(ours.id(), GOSSIP_ALPN)
                .await
                .std_context("connect")
        };
        let accept = async {
            let incoming = ours.accept().await.std_context("endpoint closed")?;
            incoming.await.std_context("accept")
        };
        (accept, dial).try_join().await
    };
    let (old, _peer_old) = connect().await?;
    let (new, peer_new) = connect().await?;
    let peer_id = theirs.id();
    let (first, second): (TopicId, TopicId) = (
        blake3::hash(b"first").into(),
        blake3::hash(b"second").into(),
    );
    let mut actor = t_actor().await?;
    actor.add_connection(old);
    push_active(&actor, peer_id, join_message(first, 0));
    push_active(&actor, peer_id, join_message(second, 0));
    // Wait until the old writer opened the first stream and waits for the second.
    tokio::time::sleep(Duration::from_millis(500)).await;
    actor.add_connection(new);

    let (topic, _) = timeout(Duration::from_secs(2), read_next_message(&peer_new))
        .await
        .std_context("the new connection waited for the open of the old one")??;
    assert_eq!(topic, second);
    Ok(())
}

/// A replaced connection keeps the message that it wrote, and new messages go on the new connection.
#[tokio::test]
#[traced_test]
async fn replaced_connection_keeps_what_it_wrote() -> Result {
    let pair = TestPair::new(1).await?;
    let (old, peer_old) = pair.connect_back().await?;
    let (new, peer_new) = pair.connect_back().await?;
    let peer_id = pair.ep2.id();
    let (t_old, t_new): (TopicId, TopicId) =
        (blake3::hash(b"old").into(), blake3::hash(b"new").into());
    let mut actor = t_actor().await?;
    actor.add_connection(old);
    push_active(&actor, peer_id, join_message(t_old, 0));
    let (topic, _) = timeout(Duration::from_secs(5), read_next_message(&peer_old))
        .await
        .std_context("the old connection sent nothing")??;
    assert_eq!(topic, t_old);

    actor.add_connection(new);
    push_active(&actor, peer_id, join_message(t_new, 0));
    let (topic, _) = timeout(Duration::from_secs(5), read_next_message(&peer_new))
        .await
        .std_context("the new message did not go out")??;
    assert_eq!(topic, t_new);
    Ok(())
}

/// A replaced connection stays open and delivers while the peer has a stream on it.
///
/// Two peers that dial each other can each keep a different connection, so
/// the peer may send on the one we replaced. The other connection carries
/// nothing and may close. Once the peer ends its stream, the replaced one
/// closes after the grace too.
#[tokio::test]
#[traced_test]
async fn replaced_connection_stays_while_the_peer_sends_on_it() -> Result {
    let pair = TestPair::new(1).await?;
    let (first, peer_first) = pair.connect_back().await?;
    let (second, peer_second) = pair.connect_back().await?;
    let peer_id = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"replaced_connection_stays").into();
    let mut actor = t_actor().await?;
    actor.add_connection(first);
    actor.add_connection(second);

    show_that_it_handles_a_close(&peer_second, topic).await;
    let mut stream = peer_first.open_uni().await.std_context("open stream")?;
    let mut buffer = Vec::new();
    wire::StreamHeader::write(topic, &mut stream, &mut buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("write header")?;
    let join = join_message(topic, 0).message;
    wire::write_message(&mut stream, &join, &mut buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("write frame")?;
    tokio::time::sleep(RETIRE_GRACE + Duration::from_secs(1)).await;
    wire::write_message(&mut stream, &join, &mut buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("write after the grace")?;
    for _ in 0..2 {
        let event = timeout(Duration::from_secs(5), actor.in_event_rx.recv())
            .await
            .std_context("a message did not arrive")?;
        assert!(
            matches!(event, Some(InEvent::RecvMessage(peer, _)) if peer == peer_id),
            "{event:?}"
        );
    }
    assert!(
        peer_first.close_reason().is_none(),
        "the replaced connection closed"
    );

    actor
        .state
        .peers
        .get_mut(&peer_id)
        .expect("known peer")
        .queue
        .close();
    stream.finish().std_context("finish")?;
    timeout(RETIRE_GRACE + Duration::from_secs(3), peer_first.closed())
        .await
        .std_context("the replaced connection stayed open")?;
    Ok(())
}

/// A late dial on which the peer never sends closes.
///
/// Two peers that dial each other at once can each end up with a
/// connection on which neither side sends. The peer showed on its other
/// connection that it handles a close, and that connection ends before
/// the dial runs.
#[tokio::test]
#[traced_test]
async fn silent_late_dial_closes() -> Result {
    let pair = TestPair::new(1).await?;
    let (accepted, peer_accepted) = pair.connect_back().await?;
    let (dialed, peer_dialed) = pair.connect().await?;
    let peer_id = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"silent_late_dial").into();
    let mut actor = t_actor().await?;

    actor.add_connection(accepted);
    show_that_it_handles_a_close(&peer_accepted, topic).await;
    actor
        .state
        .peers
        .get_mut(&peer_id)
        .expect("known peer")
        .queue
        .close();
    let ended = timeout(
        RETIRE_GRACE + Duration::from_secs(3),
        actor.connection_tasks.join_next(),
    )
    .await
    .std_context("the accepted connection stayed open")?;
    let (peer, conn, res) = ended.expect("one task").expect("no panic");

    // The test runtime runs the dial task only at the next await.
    actor.add_connection(dialed);
    actor.on_conn_task_finished(peer, conn, res);
    assert!(
        actor.state.peers.contains_key(&peer_id),
        "the flag went while the dial holds it"
    );

    let ended = timeout(
        RETIRE_GRACE + Duration::from_secs(3),
        actor.connection_tasks.join_next(),
    )
    .await
    .std_context("the dial stayed open")?;
    let (peer, conn, res) = ended.expect("one task").expect("no panic");
    actor.on_conn_task_finished(peer, conn, res);
    timeout(Duration::from_secs(1), peer_dialed.closed())
        .await
        .std_context("the peer did not see the close")?;
    actor.forget_peers(Instant::now() + PEER_RETENTION);
    assert!(
        !actor.state.peers.contains_key(&peer_id),
        "the flag outlived its time"
    );
    Ok(())
}

/// A silent late dial closes, also when the other connection of the peer ended before it.
///
/// The flag of the peer outlives its last connection for a while.
#[tokio::test]
#[traced_test]
async fn silent_dial_after_the_last_connection_closes() -> Result {
    let pair = TestPair::new(1).await?;
    let (accepted, peer_accepted) = pair.connect_back().await?;
    let (dialed, peer_dialed) = pair.connect().await?;
    let peer_id = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"silent_dial_after_the_last").into();
    let mut actor = t_actor().await?;

    actor.add_connection(accepted);
    show_that_it_handles_a_close(&peer_accepted, topic).await;
    actor
        .state
        .peers
        .get_mut(&peer_id)
        .expect("known peer")
        .queue
        .close();
    let ended = timeout(
        RETIRE_GRACE + Duration::from_secs(3),
        actor.connection_tasks.join_next(),
    )
    .await
    .std_context("the accepted connection stayed open")?;
    let (peer, conn, res) = ended.expect("one task").expect("no panic");
    actor.on_conn_task_finished(peer, conn, res);
    assert!(
        actor.state.peers.contains_key(&peer_id),
        "the flag went with the last connection"
    );

    actor.add_connection(dialed);
    let ended = timeout(
        RETIRE_GRACE + Duration::from_secs(3),
        actor.connection_tasks.join_next(),
    )
    .await
    .std_context("the dial stayed open")?;
    let (peer, conn, res) = ended.expect("one task").expect("no panic");
    actor.on_conn_task_finished(peer, conn, res);
    timeout(Duration::from_secs(1), peer_dialed.closed())
        .await
        .std_context("the peer did not see the close")?;
    Ok(())
}

/// An idle peer expires after retention even if it never set handles_close.
#[tokio::test]
#[traced_test]
async fn peer_with_unset_close_flag_expires() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect_back().await?;
    let peer_id = pair.ep2.id();
    let mut actor = t_actor().await?;

    actor.add_connection(conn);
    peer_conn.close(0u32.into(), b"done");
    let ended = timeout(Duration::from_secs(3), actor.connection_tasks.join_next())
        .await
        .std_context("the connection stayed open")?;
    let (peer, conn, res) = ended.expect("one task").expect("no panic");
    actor.on_conn_task_finished(peer, conn, res);
    assert!(
        actor.state.peers.contains_key(&peer_id),
        "retain the idle peer"
    );
    actor.forget_peers(Instant::now() + PEER_RETENTION);
    assert!(
        !actor.state.peers.contains_key(&peer_id),
        "the idle peer did not expire"
    );
    Ok(())
}

/// A dial on which neither side sends a header stays open.
///
/// Neither side learns that the other handles a close, see
/// [`wire::StreamHeader::handles_close`].
#[tokio::test]
#[traced_test]
async fn silent_dial_stays() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let peer_id = pair.ep2.id();
    let mut actor = t_actor().await?;

    actor.add_connection(conn);
    assert!(
        is_idle(&actor.state.peers[&peer_id]),
        "the dial got a queue"
    );

    tokio::time::sleep(RETIRE_GRACE + Duration::from_secs(1)).await;
    assert!(
        actor.connection_tasks.try_join_next().is_none(),
        "the connection closed"
    );
    assert!(peer_conn.close_reason().is_none(), "the connection closed");
    Ok(())
}

/// A dial that the peer's connection overtook after a disconnect does not replace it.
///
/// The peer's connection took the queue and is the active one. The dial
/// stays while the peer has a stream on it, and closes after the grace once
/// the peer ended it.
#[tokio::test]
#[traced_test]
async fn overtaken_dial_stays_while_the_peer_sends_on_it() -> Result {
    let pair = TestPair::new(1).await?;
    let (dialed, dialed_theirs) = pair.connect().await?;
    let (inbound, _inbound_theirs) = pair.connect_back().await?;
    let peer_id = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"overtaken_dial").into();
    let mut actor = t_actor().await?;
    actor.state.peers.insert(peer_id, pending_with(vec![]));
    actor
        .state
        .peers
        .get_mut(&peer_id)
        .expect("known peer")
        .queue
        .close();

    let inbound_id = inbound.stable_id();
    actor.add_connection(inbound);
    actor.add_connection(dialed);
    let mut stream = open_stream(&dialed_theirs, topic, &[join_message(topic, 0).message]).await?;

    assert!(
        matches!(
            send_state(&actor, peer_id),
            Some((Some(writer), false)) if writer == inbound_id
        ),
        "the dial replaced the peer's connection"
    );
    tokio::time::sleep(RETIRE_GRACE + Duration::from_secs(1)).await;
    assert!(
        dialed_theirs.close_reason().is_none(),
        "the dial closed while the peer has a stream on it"
    );

    actor
        .state
        .peers
        .get_mut(&peer_id)
        .expect("known peer")
        .queue
        .close();
    stream.finish().std_context("finish")?;
    timeout(
        RETIRE_GRACE + Duration::from_secs(3),
        dialed_theirs.closed(),
    )
    .await
    .std_context("the dial stayed open")?;
    Ok(())
}

/// A dial for a disconnected peer delivers the queue, then closes.
#[tokio::test]
#[traced_test]
async fn disconnected_pending_dial_flushes_then_closes() -> Result {
    let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
    let ct = CancellationToken::new();
    let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
    let (_go1, actor, ep1_handle) =
        Gossip::t_new_with_actor(rng, Default::default(), relay_map.clone(), &ct).await?;
    let _ep1_handle = AbortOnDropHandle::new(ep1_handle);
    let mut actor = ManualActorLoop::new(actor).await;
    let topic: TopicId = blake3::hash(b"pending_dial_flushes").into();

    // The peer reads one message, then waits for us to close.
    let ep2 = create_endpoint(rng, relay_map, None).await?;
    let ep2_id = ep2.id();
    let ep2_addr = EndpointAddr::new(ep2_id).with_relay_url(relay_url);
    let peer = spawn(async move {
        let incoming = ep2.accept().await.expect("endpoint closed");
        let conn = incoming.await.expect("accept failed");
        let mut stream = conn.accept_uni().await.expect("accept_uni");
        let mut buf = bytes::BytesMut::new();
        let header = wire::StreamHeader::read(&mut stream, &mut buf, 1024)
            .await
            .expect("header");
        let message: Option<proto::topic::Message<PublicKey>> =
            wire::read_message(&mut stream, &mut buf, 1024)
                .await
                .expect("frame");
        show_that_it_handles_a_close(&conn, header.topic).await;
        conn.closed().await;
        (header.topic, message.is_some())
    });
    let peer = AbortOnDropHandle::new(peer);

    let conn = actor
        .endpoint
        .connect(ep2_addr, GOSSIP_ALPN)
        .await
        .std_context("connect")?;
    actor
        .state
        .peers
        .insert(ep2_id, pending_with(vec![join_message(topic, 0)]));
    actor
        .state
        .peers
        .get_mut(&ep2_id)
        .expect("known peer")
        .queue
        .close();
    actor.add_connection(conn);
    assert!(
        matches!(send_state(&actor, ep2_id), Some((Some(_), true))),
        "the connection became the peer's active one"
    );

    let mut peer = peer;
    let delivered = timeout(RETIRE_GRACE * 2 + Duration::from_secs(3), async {
        loop {
            tokio::select! {
                _ = actor.step() => {}
                res = &mut peer => return res.expect("peer task"),
            }
        }
    })
    .await
    .std_context("the connection was not closed")?;
    assert_eq!(
        delivered,
        (topic, true),
        "the queued message did not arrive"
    );
    Ok(())
}

/// The connection a shuffle reply opened closes once the origin reads the reply.
///
/// The replying node is usually not a neighbor of the shuffle's origin. The
/// origin drops it after it reads the reply. That does not close the
/// connection while the reply stream is open, so the replying node has to
/// drop the origin as well.
#[tokio::test]
#[traced_test]
async fn shuffle_reply_connection_closes() -> Result {
    let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
    let ct = CancellationToken::new();
    let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
    let memory_lookup = MemoryLookup::new();
    let (_go1, actor, ep1_handle) =
        Gossip::t_new_with_actor(rng, Default::default(), relay_map.clone(), &ct).await?;
    let _ep1_handle = AbortOnDropHandle::new(ep1_handle);
    let mut actor = ManualActorLoop::new(actor).await;
    actor
        .endpoint
        .address_lookup()
        .expect("endpoint is not closed")
        .add(memory_lookup.clone());
    let ep1_addr = EndpointAddr::new(actor.endpoint.id()).with_relay_url(relay_url.clone());
    let topic: TopicId = blake3::hash(b"shuffle_reply_closes").into();
    actor.handle_in_event(
        InEvent::Command(topic, proto::Command::Join(vec![])),
        Instant::now(),
    );

    // The origin of the shuffle reads the reply, then waits for the close.
    let origin = create_endpoint(rng, relay_map.clone(), None).await?;
    let origin_id = origin.id();
    memory_lookup.add_endpoint_info(EndpointAddr::new(origin_id).with_relay_url(relay_url));
    let mut origin_task = AbortOnDropHandle::new(spawn(async move {
        let incoming = origin.accept().await.expect("endpoint closed");
        let conn = incoming.await.expect("accept failed");
        let mut stream = conn.accept_uni().await.expect("accept_uni");
        let mut buf = bytes::BytesMut::new();
        let header = wire::StreamHeader::read(&mut stream, &mut buf, 1024)
            .await
            .expect("header");
        let reply: Option<proto::topic::Message<PublicKey>> =
            wire::read_message(&mut stream, &mut buf, 1024)
                .await
                .expect("frame");
        show_that_it_handles_a_close(&conn, header.topic).await;
        conn.closed().await;
        reply.is_some()
    }));

    // A third node forwards the shuffle to us with its walk at an end.
    let ep3 = create_endpoint(rng, relay_map, None).await?;
    let conn = ep3
        .connect(ep1_addr, GOSSIP_ALPN)
        .await
        .std_context("connect")?;
    let mut stream = conn.open_uni().await.std_context("open_uni")?;
    let mut buf = Vec::new();
    wire::StreamHeader::write(topic, &mut stream, &mut buf, 1024)
        .await
        .std_context("header")?;
    let shuffle = proto::topic::Message::test_shuffle(origin_id, 0);
    wire::write_message(&mut stream, &shuffle, &mut buf, 1024)
        .await
        .std_context("frame")?;

    let replied = timeout(RETIRE_GRACE * 2 + Duration::from_secs(5), async {
        loop {
            tokio::select! {
                _ = actor.step() => {}
                res = &mut origin_task => return res.expect("origin task"),
            }
        }
    })
    .await
    .std_context("the reply connection was not closed")?;
    assert!(replied, "the shuffle reply did not arrive");
    assert!(
        actor.state.peers.get(&origin_id).is_none_or(is_idle),
        "the origin's queue was kept"
    );
    Ok(())
}
