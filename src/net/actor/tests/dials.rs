use super::*;

/// A reopened queue goes to the writer that waits on its connection, with no dial.
#[tokio::test]
async fn reopened_queue_keeps_the_waiting_writer() {
    let topic = [42; 32].into();
    let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
    let lease = peer.handle_connection(Side::Server, 1).unwrap();
    peer.queue.close();
    let mut reader = lease.lock().await;
    assert!(*lease.closing().borrow(), "the queue is closed");

    let message = join_message(topic, 0);
    assert!(matches!(peer.send(message.clone()), Pushed::Queued));
    assert!(!lease.revoked().is_cancelled());
    assert!(!peer.needs_dial(), "the waiting writer takes the queue");
    assert!(reader.wait_next().await);
    assert_eq!(reader.current().unwrap().message.message, message.message);
}

/// A completed dial writes only a queue that has no writer.
#[test]
fn completed_dial_leaves_an_existing_writer() {
    let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
    let first = peer.handle_connection(Side::Server, 1).unwrap();
    let second = peer.handle_connection(Side::Server, 2).unwrap();
    let late = peer.handle_connection(Side::Client, 3);
    assert!(first.revoked().is_cancelled());
    assert!(!second.revoked().is_cancelled());
    assert!(late.is_none(), "the dial must not replace the writer");

    let now = Instant::now();
    assert_eq!(peer.connection_finished(1, now, false), Finished::Quiet);
    assert_eq!(peer.connection_finished(3, now, false), Finished::Quiet);
    assert_eq!(
        peer.connection_finished(2, now, false),
        Finished::Disconnected
    );
    peer.discard_queue(now); // The actor processes PeerDisconnected immediately.
    assert!(is_idle(&peer));

    let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
    peer.queue.close();
    let first = peer.handle_connection(Side::Server, 1).unwrap();
    let second = peer.handle_connection(Side::Server, 2).unwrap();
    let late = peer.handle_connection(Side::Client, 3);
    assert!(first.revoked().is_cancelled());
    assert!(!second.revoked().is_cancelled());
    assert!(
        late.is_none(),
        "the dial of a closed peer must only receive"
    );
    assert_eq!(peer.connection_finished(1, now, false), Finished::Quiet);
    assert_eq!(peer.connection_finished(3, now, false), Finished::Quiet);
    assert_eq!(
        peer.connection_finished(2, now, false),
        Finished::Disconnected
    );
    peer.discard_queue(now);
    assert!(is_idle(&peer));
    assert!(peer.can_forget(now + PEER_RETENTION));
}

/// A disconnect drops the queue, but its writer still sends what it holds.
#[tokio::test]
async fn discarded_queue_goes_out_on_its_writer() {
    let now = Instant::now();
    let topic = [7; 32].into();
    let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
    let lease = peer.handle_connection(Side::Server, 1).unwrap();
    let aborted = peer.aborted.clone();
    let message = join_message(topic, 0);
    assert!(matches!(peer.send(message.clone()), Pushed::Queued));
    peer.discard_queue(now);
    assert!(is_idle(&peer));
    assert!(!aborted.is_cancelled());

    let mut reader = lease.lock().await;
    assert!(reader.wait_next().await);
    assert_eq!(reader.current().unwrap().message.message, message.message);
    reader.advance();
    assert!(!reader.wait_next().await, "the old queue must end");
}

/// A failed dial does not disconnect a peer whose connection still writes a closed queue.
#[tokio::test]
async fn failed_dial_leaves_the_draining_writer() -> Result {
    let mut actor = t_actor().await?;
    let peer_id = SecretKey::generate().public();
    let mut state = open_peer(queue::MIN_QUEUE_SIZE);
    let _lease = state.handle_connection(Side::Server, 1).unwrap();
    let aborted = state.aborted.clone();
    state.send(join_message([8; 32].into(), 0));
    state.queue.close();
    actor.peers.insert(peer_id, state);
    actor.on_dial_failed(peer_id);
    assert_eq!(
        actor.peers[&peer_id].writer,
        Some(1),
        "the writer was dropped"
    );
    assert!(
        !actor.peers[&peer_id].queue.is_empty(),
        "the queue was dropped"
    );
    assert!(!aborted.is_cancelled(), "the writer was aborted");
    Ok(())
}

/// The end of the writer of an open queue disconnects the peer, also if a dial still receives.
#[test]
fn lost_writer_disconnects_while_a_dial_receives() {
    let now = Instant::now();
    let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
    let _writer = peer.handle_connection(Side::Server, 1).unwrap();
    assert!(peer.handle_connection(Side::Client, 2).is_none());
    assert_eq!(
        peer.connection_finished(1, now, false),
        Finished::Disconnected
    );
    peer.discard_queue(now);
    assert!(is_idle(&peer));
    assert!(!peer.needs_dial());
    assert_eq!(peer.connection_finished(2, now, false), Finished::Quiet);
    assert!(peer.can_forget(now + PEER_RETENTION));
}

/// The end of the writer of a closed queue drops the rest of it, and is no disconnect.
#[test]
fn lost_writer_of_a_closed_queue_drops_it() {
    let now = Instant::now();
    let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
    let _writer = peer.handle_connection(Side::Server, 1).unwrap();
    peer.send(join_message([9; 32].into(), 0));
    peer.queue.close();
    assert_eq!(peer.connection_finished(1, now, false), Finished::Quiet);
    assert!(is_idle(&peer));
    assert!(!peer.needs_dial(), "no dial for a closed peer");
}

/// A message that does not fit still reopens a closed queue, which then needs a dial.
#[test]
fn dropped_message_reopens_a_closed_queue() {
    let topic = [10; 32].into();
    let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
    let mut i = 0;
    while matches!(peer.send(numbered(topic, i)), Pushed::Queued) {
        i += 1;
    }
    peer.queue.close();
    assert!(!peer.needs_dial(), "a closed queue needs no dial");
    assert!(matches!(peer.send(numbered(topic, i)), Pushed::Dropped(_)));
    assert!(!peer.queue.is_closed());
    assert!(peer.needs_dial(), "the reopened queue has no writer");
}

/// Queue disposal cannot reset the signals still used by older connections.
#[test]
fn peer_signals_survive_queue_replacement() {
    let now = Instant::now();
    let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
    let old = peer.handle_connection(Side::Server, 1).unwrap();
    let old_aborted = peer.aborted.clone();
    let handles_close = peer.handles_close.subscribe();
    peer.handles_close.send_replace(true);

    // Ordinary replacement lets the old queue drain, but all existing
    // connections must still observe a later overflow of the new queue.
    peer.discard_queue(now);
    assert!(!old_aborted.is_cancelled());
    assert!(!old.revoked().is_cancelled());
    peer.open_queue();
    let _current = peer.handle_connection(Side::Client, 2).unwrap();
    assert!(peer.handle_connection(Side::Client, 3).is_none());
    let receiving_aborted = peer.aborted.clone();
    peer.abort(now);
    assert!(old_aborted.is_cancelled());
    assert!(receiving_aborted.is_cancelled());
    assert!(!peer.can_forget(now + PEER_RETENTION));

    peer.open_queue();
    let new = peer
        .handle_connection(Side::Client, 4)
        .expect("awaiting writer");
    let new_aborted = peer.aborted.clone();
    assert!(*handles_close.borrow());
    assert!(!new_aborted.is_cancelled());
    assert!(!new.revoked().is_cancelled());
    assert_eq!(
        peer.connection_finished(1, now, false),
        Finished::Quiet,
        "old completion must leave the new queue intact"
    );
    assert_eq!(peer.connection_finished(2, now, false), Finished::Quiet);
    assert_eq!(peer.connection_finished(3, now, false), Finished::Quiet);
    assert_eq!(peer.writer, Some(4));
    assert!(!new_aborted.is_cancelled());
    assert!(!peer.needs_dial());
}

/// Retention starts after the last processed completion, independently of the close flag.
#[test]
fn peer_retention_waits_for_all_connections_and_the_queue() {
    for handles_close in [false, true] {
        let now = Instant::now();
        let later = now + PEER_RETENTION;
        let mut peer = open_peer(queue::MIN_QUEUE_SIZE);
        let _first = peer.handle_connection(Side::Server, 1).unwrap();
        let _second = peer.handle_connection(Side::Server, 2).unwrap();
        peer.handles_close.send_replace(handles_close);
        peer.abort(now);
        assert_eq!(peer.connection_finished(1, now, false), Finished::Quiet);
        assert!(!peer.can_forget(later));
        assert_eq!(peer.connection_finished(2, later, false), Finished::Quiet);
        assert!(!peer.can_forget(later + PEER_RETENTION - Duration::from_nanos(1)));
        assert!(peer.can_forget(later + PEER_RETENTION));

        // A new queue cancels retention, even without a running connection.
        peer.open_queue();
        assert!(!peer.can_forget(later + PEER_RETENTION));
        peer.abort(later);
        peer.abort(later + Duration::from_secs(1));
        assert!(
            peer.can_forget(later + PEER_RETENTION),
            "repeated disposal must not prolong retention"
        );
    }
}

/// After an answer to a peer that no topic uses, the queue to it closes.
#[tokio::test]
async fn answer_to_an_unused_peer_closes_its_queue() -> Result {
    let mut actor = t_actor().await?;
    let peer = SecretKey::generate().public();
    actor.peers.insert(peer, open_peer(queue::MIN_QUEUE_SIZE));
    let topic: TopicId = [7; 32].into();
    actor.handle_in_event(
        InEvent::RecvMessage(peer, join_message(topic, 0)),
        Instant::now(),
    );
    let state = &actor.peers[&peer];
    assert!(!state.queue.is_empty(), "the disconnect was not queued");
    assert!(state.queue.is_closed(), "the queue stayed open");
    Ok(())
}

/// A failed dial keeps the state of a peer that reached us on its own connection.
///
/// Its connection still works, so dropping the state would lose the peer.
#[tokio::test]
#[traced_test]
async fn dial_failure_keeps_a_peer_with_a_writer() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect_back().await?;
    let peer_id = pair.ep2.id();
    let endpoint = Endpoint::builder(presets::Minimal)
        .relay_mode(RelayMode::Disabled)
        .bind()
        .await?;
    let metrics = Arc::new(Metrics::default());
    // The senders keep the event loop running.
    let (mut actor, _to_actor) = Actor::new(
        endpoint,
        ActorConfig {
            proto: Default::default(),
            ..Default::default()
        },
        metrics.clone(),
        Default::default(),
    );
    actor.add_connection(conn);
    // Our endpoint has no address for the peer, so the dial fails.
    actor.dialer.queue_dial(peer_id, actor.alpn.clone());

    let addr_updates = &mut n0_future::stream::pending();
    timeout(Duration::from_secs(5), async {
        while metrics.actor_tick_dialer_failure.get() == 0 {
            actor.event_loop(addr_updates).await;
        }
    })
    .await
    .std_context("the dial did not fail")?;
    assert!(matches!(
        send_state(&actor, peer_id),
        Some((Some(_), false))
    ));
    assert!(peer_conn.close_reason().is_none(), "the connection closed");
    Ok(())
}

/// A peer whose queue has no room for a control message loses its connection (#47).
///
/// The actor waited for room in the queue of a peer that did not read. While it
/// waited, no topic made progress, no timer fired, and the peer stayed.
#[tokio::test]
#[traced_test]
async fn full_control_queue_aborts_the_peer_without_waiting() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let peer = pair.ep2.id();
    let mut actor = t_actor().await?;
    let mut state = open_peer(queue::MIN_QUEUE_SIZE);
    let queue_lease = state
        .handle_connection(Side::Server, conn.stable_id())
        .expect("an accepted connection takes the lease");
    let aborted = state.aborted.clone();
    actor.peers.insert(peer, state);
    let (in_event_tx, _in_event_rx) = mpsc::channel(16);
    // The runtime has one thread, so the writer takes nothing before the queue is full.
    let _conn_loop = AbortOnDropHandle::new(spawn(connection_loop(
        peer,
        conn,
        aborted,
        Some(queue_lease),
        Vec::new(),
        in_event_tx,
        TEST_FRAME_LIMIT,
        unset_flag(),
    )));

    join_until_full(&mut actor, peer);
    assert_eq!(actor.metrics.peers_send_queue_full.get(), 1);
    // More control messages to the closing connection fail nothing new.
    let topic: TopicId = blake3::hash(b"one more").into();
    let join = InEvent::Command(topic, ProtoCommand::Join(vec![peer]));
    actor.handle_in_event(join, Instant::now());
    assert_eq!(actor.metrics.peers_send_queue_full.get(), 1);
    timeout(Duration::from_secs(2), peer_conn.closed())
        .await
        .std_context("the connection to the full peer stayed open")?;
    Ok(())
}

/// Crossed dials can leave the remote writer on our receive-only connection.
#[tokio::test(flavor = "current_thread")]
#[traced_test]
async fn control_overflow_closes_both_crossed_connections() -> Result {
    let pair = TestPair::new(1).await?;
    let (accepted, peer_accepted) = pair.connect_back().await?;
    let (dialed, peer_dialed) = pair.connect().await?;
    let peer = pair.ep2.id();
    let writer = accepted.stable_id();
    let mut actor = t_actor().await?;
    actor.send_queue_size = queue::MIN_QUEUE_SIZE;
    actor.add_connection(accepted);
    actor.add_connection(dialed);
    assert_eq!(actor.peers[&peer].writer, Some(writer));
    assert_eq!(actor.peers[&peer].connections, 2);

    // Neither task can drain the queue while this synchronous helper runs.
    join_until_full(&mut actor, peer);
    assert_eq!(actor.metrics.peers_send_queue_full.get(), 1);

    // An abort must close both, regardless of their leases or idle retirement.
    closed_with(&peer_accepted, conn::CLOSE_QUEUE_FULL).await?;
    closed_with(&peer_dialed, conn::CLOSE_QUEUE_FULL).await?;
    Ok(())
}

/// A peer whose queue fills during the dial goes down, and its queue with it.
#[tokio::test]
#[traced_test]
async fn full_queue_during_the_dial_drops_the_peer() -> Result {
    let mut actor = t_actor().await?;
    actor.send_queue_size = queue::MIN_QUEUE_SIZE;
    // The endpoint has no address for the peer, so the dial does not connect soon.
    let peer = SecretKey::generate().public();
    join_until_full(&mut actor, peer);
    assert_eq!(actor.metrics.peers_send_queue_full.get(), 1);
    assert!(
        !actor.state.uses_peer(&peer),
        "the protocol still uses the closed peer"
    );
    Ok(())
}

/// A message to a peer that no topic uses goes out on the waiting writer, with no dial.
#[tokio::test]
#[traced_test]
async fn answers_to_an_unused_peer_need_no_dial() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect_back().await?;
    let peer_id = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"answers_need_no_dial").into();
    let mut actor = t_actor().await?;
    actor.add_connection(conn);
    for _ in 0..2 {
        let join = InEvent::RecvMessage(peer_id, join_message(topic, 0));
        actor.handle_in_event(join, Instant::now());
        assert!(!actor.dialer.is_dialing(peer_id), "the answer dialed");
        let (sent, message) = timeout(Duration::from_secs(5), read_next_message(&peer_conn))
            .await
            .std_context("the answer did not go out")??;
        assert_eq!(sent, topic);
        assert!(message.is_disconnect());
    }
    Ok(())
}

/// An aborted peer loses the connection that took its queue, also once its writer ended.
///
/// The peer opened another connection, which took over the queue. The
/// writer of the old connection sent the queue and let go of it, but the
/// connection still reads from the peer, which must learn of the drop.
#[tokio::test]
#[traced_test]
async fn aborted_peer_loses_the_connection_that_took_its_queue() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let peer_id = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"reopened").into();
    let mut peers = HashMap::new();
    let mut state = open_peer(queue::MIN_QUEUE_SIZE);
    let queue_lease = state
        .handle_connection(Side::Server, conn.stable_id())
        .expect("an accepted connection takes the lease");
    let (in_event_tx, _in_event_rx) = mpsc::channel(16);
    let _conn_loop = AbortOnDropHandle::new(spawn(connection_loop(
        peer_id,
        conn,
        state.aborted.clone(),
        Some(queue_lease),
        Vec::new(),
        in_event_tx,
        TEST_FRAME_LIMIT,
        unset_flag(),
    )));
    assert!(matches!(
        state.queue.push(join_message(topic, 0)),
        Pushed::Queued
    ));
    timeout(Duration::from_secs(5), read_next_message(&peer_conn))
        .await
        .std_context("the queue did not go out")??;
    peers.insert(peer_id, state);
    let state = peers.get_mut(&peer_id).expect("known peer");
    state.queue.close();
    let _next = state.handle_connection(Side::Server, usize::MAX);
    // The writer lets go of the queue once the next connection revoked it.
    timeout(Duration::from_secs(5), async {
        while peers[&peer_id].queue.try_reader().is_none() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .std_context("the writer did not end")?;

    peers
        .get_mut(&peer_id)
        .expect("known peer")
        .abort(Instant::now());
    assert!(is_idle(&peers[&peer_id]), "the queue stayed");
    closed_with(&peer_conn, conn::CLOSE_QUEUE_FULL).await
}

/// A closed peer that the protocol reopens keeps its queue, and the dial delivers all of it.
#[tokio::test]
#[traced_test]
async fn peer_wanted_again_during_the_dial_keeps_its_queue() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let peer_id = pair.ep2.id();
    let (before, after): (TopicId, TopicId) = (
        blake3::hash(b"before").into(),
        blake3::hash(b"after").into(),
    );
    let mut actor = t_actor().await?;
    actor
        .peers
        .insert(peer_id, pending_with(vec![join_message(before, 0)]));
    actor
        .peers
        .get_mut(&peer_id)
        .expect("known peer")
        .queue
        .close();
    let join = InEvent::Command(after, ProtoCommand::Join(vec![peer_id]));
    actor.handle_in_event(join, Instant::now());
    assert!(
        matches!(send_state(&actor, peer_id), Some((None, false))),
        "the peer is not pending again"
    );

    actor.add_connection(conn);
    assert!(
        matches!(send_state(&actor, peer_id), Some((Some(_), false))),
        "the dial was not kept"
    );
    let mut topics = BTreeSet::new();
    for _ in 0..2 {
        let (topic, _) = timeout(Duration::from_secs(5), read_next_message(&peer_conn))
            .await
            .std_context("a queued message did not go out")??;
        topics.insert(topic);
    }
    assert_eq!(topics, BTreeSet::from([before, after]));
    Ok(())
}

/// A peer that we close and reopen keeps its queue, and the next connection writes the old messages first.
///
/// The old connection does not read, so its writer still has the queue,
/// which ends with a `Disconnect`. This checks the order of our writes
/// only: the peer reads each stream on its own.
#[tokio::test]
#[traced_test]
async fn dropped_peer_wanted_again_gets_the_old_queue_first() -> Result {
    let pair = TestPair::new(1).await?;
    let (old, _peer_old) = pair.connect_back().await?;
    let (new, peer_new) = pair.connect_back().await?;
    let peer_id = pair.ep2.id();
    let (left, joined): (TopicId, TopicId) =
        (blake3::hash(b"left").into(), blake3::hash(b"joined").into());
    let mut actor = t_actor().await?;
    actor.send_queue_size = 64 << 20;
    actor.add_connection(old);
    push_many(&actor, peer_id, left, 3000);
    push_active(&actor, peer_id, disconnect_message(left));
    // Wait until flow control blocks the old writer.
    tokio::time::sleep(Duration::from_secs(1)).await;
    actor
        .peers
        .get_mut(&peer_id)
        .expect("known peer")
        .queue
        .close();
    let state = actor.peers.get_mut(&peer_id).expect("dropped");
    state.open_queue();
    let pushed = state.queue.push(join_message(joined, 0));
    assert!(matches!(pushed, Pushed::Queued));
    actor.add_connection(new);

    let (topic, messages) = timeout(Duration::from_secs(10), read_next_stream(&peer_new))
        .await
        .std_context("the old queue did not go out")??;
    assert_eq!(topic, left, "the new queue went first");
    assert_eq!(messages.last(), Some(&TopicMessage::test_disconnect()));
    let (topic, _) = timeout(Duration::from_secs(5), read_next_message(&peer_new))
        .await
        .std_context("the new queue did not go out")??;
    assert_eq!(topic, joined);
    Ok(())
}

/// A peer that we close and reopen keeps one byte limit for its old and new messages.
#[tokio::test]
#[traced_test]
async fn closed_peer_keeps_its_byte_limit() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, _peer_conn) = pair.connect().await?;
    let peer_id = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"one limit").into();
    let mut peers = HashMap::new();
    let mut state = open_peer(queue::MIN_QUEUE_SIZE);
    // Nothing takes from the queue, as from that of a peer that does not read.
    let _queue_lease = state
        .handle_connection(Side::Server, conn.stable_id())
        .expect("an accepted connection takes the lease");
    peers.insert(peer_id, state);
    let push = |peers: &mut HashMap<EndpointId, PeerState>| {
        let state = peers.get_mut(&peer_id).expect("known");
        state.queue.push(numbered(topic, 0))
    };
    while matches!(push(&mut peers), Pushed::Queued) {}

    peers.get_mut(&peer_id).expect("known peer").queue.close();
    peers.get_mut(&peer_id).expect("dropped").open_queue();
    assert!(
        matches!(push(&mut peers), Pushed::Dropped(_)),
        "the peer got a second limit"
    );
    Ok(())
}

/// A connection from a peer we disconnected during a dial becomes its active one.
///
/// The dial's queue goes out on it. After the queue, we drop only a connection
/// we dialed, as one the peer opened is one it is about to use.
#[tokio::test]
#[traced_test]
async fn inbound_during_disconnected_dial_is_kept() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect_back().await?;
    let peer_id = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"inbound_during_disconnected_dial").into();
    let mut actor = t_actor().await?;
    actor
        .peers
        .insert(peer_id, pending_with(vec![join_message(topic, 0)]));
    actor
        .peers
        .get_mut(&peer_id)
        .expect("known peer")
        .queue
        .close();

    actor.add_connection(conn);

    assert!(
        matches!(send_state(&actor, peer_id), Some((Some(_), false))),
        "the peer's connection was not kept"
    );
    let (_, message) = timeout(Duration::from_secs(5), read_next_message(&peer_conn))
        .await
        .std_context("the queue did not go out")??;
    assert!(!message.is_disconnect(), "the queued join is missing");
    Ok(())
}

/// A dial that completes after the peer's state is gone closes.
///
/// The peer's state goes when its other connection closes or the protocol
/// drops it. Nothing uses the dialed connection, so it closes after the
/// grace once the peer ended the stream it opened.
#[tokio::test]
#[traced_test]
async fn dial_for_a_peer_without_state_closes() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let peer_id = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"dial_without_state").into();
    let mut actor = t_actor().await?;

    actor.add_connection(conn);
    let mut stream = open_stream(&peer_conn, topic, &[join_message(topic, 0).message]).await?;
    stream.finish().std_context("finish")?;

    assert!(is_idle(&actor.peers[&peer_id]), "the dial got a queue");
    let ended = timeout(
        RETIRE_GRACE + Duration::from_secs(3),
        actor.connection_tasks.join_next(),
    )
    .await
    .std_context("the connection stayed open")?;
    let (peer, conn, res) = ended.expect("one task").expect("no panic");
    actor.on_conn_task_finished(peer, conn, res);
    timeout(Duration::from_secs(1), peer_conn.closed())
        .await
        .std_context("the peer did not see the close")?;
    Ok(())
}
