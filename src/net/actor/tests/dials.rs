use super::*;

/// Returns a peer state with `messages` in its queue.
fn pending_with(messages: Vec<ProtoMessage>) -> PeerState {
    let state = PeerState::open(queue::QUEUE_SIZE);
    for message in messages {
        assert!(matches!(state.queue.send(message), Pushed::Queued));
    }
    state
}

/// A message that does not fit still reopens a closed queue, which then needs a dial.
#[test]
fn dropped_message_reopens_a_closed_queue() {
    let topic = [10; 32].into();
    let mut peer = PeerState::open(queue::MIN_QUEUE_SIZE);
    let mut i = 0;
    while matches!(peer.send(numbered(topic, i)), Pushed::Queued) {
        i += 1;
    }
    peer.queue.close();
    assert!(!peer.needs_dial(), "a closed queue needs no dial");
    assert!(matches!(peer.send(numbered(topic, i)), Pushed::Dropped(_)));
    assert!(!peer.queue.is_closed());
    assert!(peer.needs_dial(), "the reopened queue has no connection");
}

/// After an answer to a peer that no topic uses, the queue to it closes.
#[tokio::test]
async fn answer_to_an_unused_peer_closes_its_queue() -> Result {
    let mut actor = t_actor().await?;
    let peer = SecretKey::generate().public();
    let state = PeerState::open(queue::MIN_QUEUE_SIZE);
    actor.state.peers.insert(peer, state);
    let topic: TopicId = [7; 32].into();
    actor.handle_in_event(
        InEvent::RecvMessage(peer, join_message(topic, 0)),
        Instant::now(),
    );
    let state = &actor.state.peers[&peer];
    assert!(!state.queue.is_empty(), "the disconnect was not queued");
    assert!(state.queue.is_closed(), "the queue stayed open");
    Ok(())
}

/// A failed dial keeps the state of a peer that reached us on its own connection.
///
/// Its connection still works, so dropping the state would lose the peer.
#[tokio::test]
#[traced_test]
async fn dial_failure_keeps_a_peer_with_a_connection() -> Result {
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
    actor.state.dialer.queue_dial(peer_id);

    let addr_updates = &mut n0_future::stream::pending();
    timeout(Duration::from_secs(5), async {
        while metrics.actor_tick_dialer_failure.get() == 0 {
            actor.event_loop(addr_updates).await;
        }
    })
    .await
    .std_context("the dial did not fail")?;
    assert_eq!(actor.state.peers[&peer_id].conn_count(), 1);
    assert!(peer_conn.close_reason().is_none(), "the connection closed");
    Ok(())
}

/// A new connection of a peer with the most connections to us closes its oldest one.
///
/// The peer opens one connection more than the cap. The oldest closes with `Blocked`,
/// and the new one gets in, as the connection of a restarted peer must. Our own dial
/// does not count, and stays.
#[tokio::test]
#[traced_test]
async fn connection_over_the_cap_closes_the_oldest() -> Result {
    let pair = TestPair::new(1).await?;
    let peer_id = pair.ep2.id();
    let endpoint = Endpoint::builder(presets::Minimal)
        .relay_mode(RelayMode::Disabled)
        .bind()
        .await?;
    let metrics = Arc::new(Metrics::default());
    let (mut actor, _to_actor) = Actor::new(
        endpoint,
        ActorConfig::default(),
        metrics.clone(),
        Default::default(),
    );
    let mut theirs = Vec::new();
    // Our own dial does not count, and is never closed.
    let (ours, ours_theirs) = pair.connect().await?;
    actor.add_connection(ours);
    for _ in 0..MAX_ACCEPTED_PER_PEER {
        let (conn, peer_conn) = pair.connect_back().await?;
        actor.add_connection(conn);
        theirs.push(peer_conn);
    }
    let (conn, extra) = pair.connect_back().await?;
    actor.add_connection(conn);

    let oldest = theirs.remove(0);
    assert_eq!(
        close_code(&oldest, SECS_10).await?,
        Some(CloseCode::Blocked)
    );
    assert_eq!(metrics.conns_evicted.get(), 1);
    let peer = &actor.state.peers[&peer_id];
    assert_eq!(peer.accepted_count(), MAX_ACCEPTED_PER_PEER);
    for peer_conn in theirs.iter().chain([&ours_theirs, &extra]) {
        assert!(peer_conn.close_reason().is_none(), "a connection closed");
    }
    Ok(())
}

/// A failed dial without a connection drops the queue, and reports the peer as disconnected.
#[tokio::test]
async fn failed_dial_without_a_connection_releases_the_queue() -> Result {
    let mut actor = t_actor().await?;
    let peer = SecretKey::generate().public();
    actor
        .state
        .peers
        .insert(peer, pending_with(vec![join_message([1; 32].into(), 0)]));
    actor.on_dial_failed(peer);
    assert!(
        !actor.state.peers.contains_key(&peer),
        "the idle state stayed"
    );
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
    let (conn, peer_conn) = pair.connect_back().await?;
    let peer = pair.ep2.id();
    let mut actor = t_actor().await?;
    actor.add_connection(conn);
    let _control = control_open(&peer_conn).await?;

    join_until_full(&mut actor, peer);
    assert_eq!(actor.state.metrics.peers_send_queue_full.get(), 1);
    // The next message goes into a new queue, and dials.
    let topic: TopicId = blake3::hash(b"one more").into();
    let join = InEvent::Command(topic, ProtoCommand::Join(vec![peer]));
    actor.handle_in_event(join, Instant::now());
    assert_eq!(actor.state.metrics.peers_send_queue_full.get(), 1);
    assert!(
        actor.state.dialer.is_dialing(peer),
        "the next message dials"
    );
    closed_with(&peer_conn, CloseCode::Blocked).await
}

/// Crossed dials can leave the remote writer on our receive-only connection.
#[tokio::test(flavor = "current_thread")]
#[traced_test]
async fn control_overflow_closes_both_crossed_connections() -> Result {
    let pair = TestPair::new(1).await?;
    let (accepted, peer_accepted) = pair.connect_back().await?;
    let (dialed, peer_dialed) = pair.connect().await?;
    let peer = pair.ep2.id();
    let mut actor = t_actor().await?;
    actor.state.peers.queue_size = queue::MIN_QUEUE_SIZE;
    actor.add_connection(accepted);
    actor.add_connection(dialed);
    assert_eq!(actor.state.peers[&peer].conn_count(), 2);

    // Neither task can drain the queue while this synchronous helper runs.
    join_until_full(&mut actor, peer);
    assert_eq!(actor.state.metrics.peers_send_queue_full.get(), 1);

    // An abort must close both, regardless of their state.
    closed_with(&peer_accepted, CloseCode::Blocked).await?;
    closed_with(&peer_dialed, CloseCode::Blocked).await?;
    Ok(())
}

/// A peer whose queue fills during the dial goes down, and its queue with it.
#[tokio::test]
#[traced_test]
async fn full_queue_during_the_dial_drops_the_peer() -> Result {
    let mut actor = t_actor().await?;
    actor.state.peers.queue_size = queue::MIN_QUEUE_SIZE;
    // The endpoint has no address for the peer, so the dial does not connect soon.
    let peer = SecretKey::generate().public();
    join_until_full(&mut actor, peer);
    assert_eq!(actor.state.metrics.peers_send_queue_full.get(), 1);
    // The pending join is still wanted, but the abort closed the queue and nothing reopens it.
    assert!(actor.state.peers[&peer].queue.is_closed());
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
    let mut actor = ManualActorLoop::new(t_actor().await?).await;
    actor.add_connection(conn);
    let _control = RawControl::open(&peer_conn).await?;
    pump_until(&mut actor, "the writer takes the queue", |actor| {
        actor.state.peers[&peer_id].writer().is_some()
    })
    .await?;
    for _ in 0..2 {
        let join = InEvent::RecvMessage(peer_id, join_message(topic, 0));
        actor.handle_in_event(join, Instant::now());
        assert!(!actor.state.dialer.is_dialing(peer_id), "the answer dialed");
        let (sent, message) = timeout(Duration::from_secs(5), read_next_message(&peer_conn))
            .await
            .std_context("the answer did not go out")??;
        assert_eq!(sent, topic);
        assert!(message.is_disconnect());
    }
    Ok(())
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
        .state
        .peers
        .insert(peer_id, pending_with(vec![join_message(before, 0)]));
    actor
        .state
        .peers
        .get_mut(&peer_id)
        .expect("known peer")
        .queue
        .close();
    let join = InEvent::Command(after, ProtoCommand::Join(vec![peer_id]));
    actor.handle_in_event(join, Instant::now());
    assert_eq!(actor.state.peers[&peer_id].conn_count(), 0);
    let mut actor = ManualActorLoop::new(actor).await;

    actor.add_connection(conn);
    assert_eq!(actor.state.peers[&peer_id].conn_count(), 1);
    let _control = RawControl::accept(&peer_conn).await?;
    let mut topics = BTreeSet::new();
    let read = async {
        for _ in 0..2 {
            let (topic, _) = timeout(Duration::from_secs(5), read_next_message(&peer_conn))
                .await
                .std_context("a queued message did not go out")??;
            topics.insert(topic);
        }
        n0_error::Ok(())
    };
    // The actor gives the queue to the connection once it reads the header.
    tokio::select! {
        res = read => res?,
        _ = async { loop { actor.step().await; } } => unreachable!(),
    }
    assert_eq!(topics, BTreeSet::from([before, after]));
    Ok(())
}

/// Joins new topics with `peer` until its queue had no room for a control message.
fn join_until_full(actor: &mut Actor, peer: EndpointId) {
    for i in 0..10_000u32 {
        let topic: TopicId = blake3::hash(&i.to_be_bytes()).into();
        let join = InEvent::Command(topic, ProtoCommand::Join(vec![peer]));
        actor.handle_in_event(join, Instant::now());
        if actor.state.metrics.peers_send_queue_full.get() > 0 {
            return;
        }
    }
}

/// Returns the payload number `i`, which differs from the others in its size.
///
/// Up to 3000 of them fit the frame limit of the actor.
fn numbered(topic: TopicId, i: usize) -> ProtoMessage {
    ProtoMessage {
        topic,
        message: TopicMessage::test_gossip(1000 + i),
    }
}

/// A peer that we close and reopen keeps one byte limit for its old and new messages.
#[test]
fn closed_peer_keeps_its_byte_limit() {
    let topic: TopicId = blake3::hash(b"one limit").into();
    // Nothing takes from the queue, as from that of a peer that does not read.
    let state = PeerState::open(queue::MIN_QUEUE_SIZE);
    while matches!(state.queue.send(numbered(topic, 0)), Pushed::Queued) {}

    state.queue.close();
    state.queue.open();
    assert!(
        matches!(state.queue.send(numbered(topic, 0)), Pushed::Dropped(_)),
        "the peer got a second limit"
    );
}
