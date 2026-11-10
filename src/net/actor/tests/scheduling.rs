use super::*;

/// A message that a connection delivered before its close is handled before the close.
///
/// The receive loop queues the message before its task ends, but the actor can
/// see the end first. A join from the peer then made it our neighbor after its
/// connection was gone.
#[tokio::test]
#[traced_test]
async fn message_before_a_close_is_handled_first() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, _peer_conn) = pair.connect_back().await?;
    let peer = conn.remote_id();
    let (mut actor, _to_actor) = Actor::new(
        pair.ep1.clone(),
        ActorConfig {
            proto: Default::default(),
            ..Default::default()
        },
        Default::default(),
        Default::default(),
    );
    let topic = [1; 32].into();
    let state = TopicHandles::default();
    let mut events = state.events.subscribe();
    actor.topics.insert(topic, state);
    actor.handle_in_event(
        InEvent::Command(topic, ProtoCommand::Join(vec![])),
        Instant::now(),
    );
    actor
        .in_event_tx
        .send(InEvent::RecvMessage(peer, join_message(topic, 0)))
        .await
        .std_context("queue join")?;
    conn.close(0u32.into(), b"done");
    actor.add_connection(conn);
    let (peer, conn, result) = actor
        .connection_tasks
        .join_next()
        .await
        .expect("one task")
        .expect("no panic");
    actor.on_conn_task_finished(peer, conn, result);

    assert!(
        !actor.state.has_active_peers(&topic),
        "the join made a neighbor after its close"
    );
    let up = events.try_recv().std_context("no event")?;
    let down = events.try_recv().std_context("no second event")?;
    assert!(
        matches!(up, ProtoEvent::NeighborUp(p) if p == peer),
        "{up:?}"
    );
    assert!(
        matches!(down, ProtoEvent::NeighborDown(p) if p == peer),
        "{down:?}"
    );
    Ok(())
}

/// Holds a broadcast without letting subsequent commands pass it.
///
/// Other commands and EOF do not consult pressure. Cancelling `next`
/// cannot lose the command.
#[tokio::test(start_paused = true)]
async fn admission_wait_preserves_command_order() -> Result {
    let topic = [1; 32].into();
    let mut commands = SubscriptionCommands::default();
    commands.insert(
        topic,
        commands_of(vec![
            Command::JoinPeers(vec![]),
            Command::Broadcast(Bytes::from_static(b"held")),
            Command::JoinPeers(vec![]),
            Command::BroadcastNeighbors(Bytes::from_static(b"bounded")),
        ]),
    );
    let next = commands
        .next(|_| panic!("a join must not check pressure"))
        .await;
    assert!(matches!(next, Some((_, Some(Command::JoinPeers(_))))));

    let soon = BROADCAST_WAIT_LIMIT / 2;
    assert!(timeout(soon, commands.next(|_| true)).await.is_err());
    let next = timeout(BROADCAST_RECHECK_INTERVAL * 2, commands.next(|_| false))
        .await
        .std_context("pressure eased but the broadcast did not resume")?;
    assert!(matches!(next, Some((_, Some(Command::Broadcast(_))))));
    let next = commands
        .next(|_| panic!("a join must not check pressure"))
        .await;
    assert!(matches!(next, Some((_, Some(Command::JoinPeers(_))))));

    let start = Instant::now();
    let next = timeout(BROADCAST_WAIT_LIMIT * 2, commands.next(|_| true))
        .await
        .std_context("the broadcast waited past its deadline")?;
    assert!(matches!(
        next,
        Some((_, Some(Command::BroadcastNeighbors(_))))
    ));
    assert_eq!(start.elapsed(), BROADCAST_WAIT_LIMIT);
    assert!(matches!(
        commands
            .next(|_| panic!("EOF must not check pressure"))
            .await,
        Some((_, None))
    ));
    assert!(commands.is_empty());
    Ok(())
}

/// A fixed deadline survives cancelled actor selects and repeated rechecks.
#[tokio::test(start_paused = true)]
async fn admission_deadline_survives_cancellation() -> Result {
    let mut commands = SubscriptionCommands::default();
    commands.insert(
        [1; 32].into(),
        commands_of(vec![Command::Broadcast(Bytes::from_static(b"bounded"))]),
    );
    let start = Instant::now();
    for _ in 0..3 {
        assert!(timeout(BROADCAST_WAIT_LIMIT / 4, commands.next(|_| true))
            .await
            .is_err());
    }
    let next = timeout(BROADCAST_WAIT_LIMIT, commands.next(|_| true))
        .await
        .std_context("the deadline was extended")?;
    assert!(matches!(next, Some((_, Some(Command::Broadcast(_))))));
    assert_eq!(start.elapsed(), BROADCAST_WAIT_LIMIT);
    Ok(())
}

/// Every waiting receiver retries independently when pressure eases.
#[tokio::test(start_paused = true)]
async fn admission_resumes_every_publisher() -> Result {
    let topic = [1; 32].into();
    let mut commands = SubscriptionCommands::default();
    for name in [b"first".as_slice(), b"second".as_slice()] {
        commands.insert(
            topic,
            commands_of(vec![Command::Broadcast(Bytes::copy_from_slice(name))]),
        );
    }
    assert!(timeout(BROADCAST_WAIT_LIMIT / 2, commands.next(|_| true))
        .await
        .is_err());
    let start = Instant::now();
    let mut received = BTreeSet::new();
    while received.len() < 2 {
        let next = timeout(BROADCAST_RECHECK_INTERVAL * 2, commands.next(|_| false))
            .await
            .std_context("a publisher did not resume")?;
        if let Some((_, Some(Command::Broadcast(message)))) = next {
            received.insert(message);
        } else {
            assert!(matches!(next, Some((_, None))));
        }
    }
    assert_eq!(received.len(), 2);
    assert!(start.elapsed() <= BROADCAST_RECHECK_INTERVAL * 2);
    Ok(())
}

/// Admitting one publisher can consume the room another publisher needs.
#[tokio::test(start_paused = true)]
async fn admission_rechecks_between_publishers() -> Result {
    let topic = [1; 32].into();
    let mut commands = SubscriptionCommands::default();
    for _ in 0..2 {
        commands.insert(
            topic,
            commands_of(vec![Command::Broadcast(Bytes::from_static(b"waiting"))]),
        );
    }
    let start = Instant::now();
    assert!(timeout(BROADCAST_WAIT_LIMIT / 2, commands.next(|_| true))
        .await
        .is_err());
    let first = commands.next(|_| false).await;
    assert!(matches!(first, Some((_, Some(Command::Broadcast(_))))));

    // Model the first broadcast filling the queues again. EOF from its
    // receiver may arrive first, but the second broadcast must still wait.
    loop {
        let next = commands.next(|_| true).await;
        if matches!(next, Some((_, Some(Command::Broadcast(_))))) {
            break;
        }
        assert!(matches!(next, Some((_, None))));
    }
    assert_eq!(start.elapsed(), BROADCAST_WAIT_LIMIT);
    Ok(())
}

/// Waiting on one subscription never stalls another topic's commands.
#[tokio::test(start_paused = true)]
async fn admission_leaves_other_topics_ready() -> Result {
    let blocked = [1; 32].into();
    let healthy = [2; 32].into();
    let mut commands = SubscriptionCommands::default();
    commands.insert(
        blocked,
        commands_of(vec![Command::Broadcast(Bytes::from_static(b"waiting"))]),
    );
    commands.insert(
        healthy,
        commands_of(vec![
            Command::JoinPeers(vec![]),
            Command::Broadcast(Bytes::from_static(b"ready")),
        ]),
    );
    let start = Instant::now();
    let next = commands.next(|topic| topic == blocked).await;
    assert!(matches!(next, Some((topic, Some(Command::JoinPeers(_)))) if topic == healthy));
    let next = commands.next(|topic| topic == blocked).await;
    assert!(matches!(next, Some((topic, Some(Command::Broadcast(_)))) if topic == healthy));
    assert_eq!(start.elapsed(), Duration::ZERO);
    Ok(())
}

/// Admission uses current eager targets, including before the first broadcast.
///
/// No network: the peers join by received messages, and have active states
/// that nothing writes.
#[tokio::test]
#[traced_test]
async fn admission_follows_current_eager_peers() -> Result {
    let topic: TopicId = blake3::hash(b"eager").into();
    let mut actor = t_actor().await?;
    actor.send_queue_size = queue::MIN_QUEUE_SIZE;
    actor.topics.insert(topic, TopicHandles::default());
    // A subscriber keeps the topic, which the actor quits otherwise.
    let _events = actor.topics[&topic].events.subscribe();
    actor.handle_in_event(
        InEvent::Command(topic, ProtoCommand::Join(vec![])),
        Instant::now(),
    );
    let mut peers = Vec::new();
    for id in 0..2usize {
        let peer = SecretKey::generate().public();
        let mut state = open_peer(queue::MIN_QUEUE_SIZE);
        let _lease = state
            .handle_connection(Side::Server, id)
            .expect("an accepted connection takes the lease");
        actor.peers.insert(peer, state);
        let join = join_message(topic, 0);
        actor.handle_in_event(InEvent::RecvMessage(peer, join), Instant::now());
        peers.push(peer);
    }
    let [full, other] = peers[..] else {
        unreachable!()
    };
    assert_eq!(
        actor.state.eager_peers(&topic).count(),
        2,
        "both joined eager"
    );
    let fill = |actor: &Actor, peer| {
        let queue = &actor.peers[&peer].queue;
        while !queue.is_busy() {
            assert!(matches!(queue.push(numbered(topic, 0)), Pushed::Queued));
        }
    };
    let backed_up = |actor: &Actor| all_eager_peers_busy(&actor.state, &actor.peers, topic);

    fill(&actor, full);
    assert!(
        !backed_up(&actor),
        "one eager peer with room must keep the gate open"
    );

    // A prune makes the other peer lazy, so only the full one counts.
    let prune = ProtoMessage {
        topic,
        message: TopicMessage::test_prune(),
    };
    actor.handle_in_event(InEvent::RecvMessage(other, prune), Instant::now());
    assert!(
        backed_up(&actor),
        "a lazy peer with room must not keep the gate open"
    );

    actor.command_rx.insert(
        topic,
        commands_of(vec![Command::Broadcast(Bytes::from_static(b"first"))]),
    );
    actor.topics.get_mut(&topic).unwrap().command_receivers += 1;
    assert!(
        timeout(
            BROADCAST_RECHECK_INTERVAL * 2,
            actor
                .command_rx
                .next(|topic| { all_eager_peers_busy(&actor.state, &actor.peers, topic) })
        )
        .await
        .is_err(),
        "the first broadcast must wait before protocol processing"
    );
    actor
        .peers
        .get_mut(&full)
        .unwrap()
        .discard_queue(Instant::now());
    assert!(!backed_up(&actor), "a replacement queue has room");
    let next = timeout(
        BROADCAST_RECHECK_INTERVAL * 2,
        actor
            .command_rx
            .next(|topic| all_eager_peers_busy(&actor.state, &actor.peers, topic)),
    )
    .await
    .std_context("queue replacement did not resume admission")?;
    assert!(matches!(next, Some((_, Some(Command::Broadcast(_))))));

    actor.handle_in_event(InEvent::PeerDisconnected(full), Instant::now());
    assert!(
        !backed_up(&actor),
        "the gate stayed closed without eager peers"
    );
    Ok(())
}

/// A publisher that always has a command ready does not starve a finished connection.
#[tokio::test]
#[traced_test]
async fn busy_publisher_does_not_starve_finished_connections() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, _peer_conn) = pair.connect_back().await?;
    let (mut actor, _to_actor) = Actor::new(
        pair.ep1.clone(),
        ActorConfig {
            proto: Default::default(),
            ..Default::default()
        },
        Default::default(),
        Default::default(),
    );
    publish_forever(&mut actor);
    conn.close(0u32.into(), b"done");
    actor.add_connection(conn);
    let mut addresses = n0_future::stream::pending();
    for _ in 0..256 {
        actor.event_loop(&mut addresses).await;
        tokio::task::yield_now().await;
        if actor.connection_tasks.is_empty() {
            return Ok(());
        }
    }
    panic!("publishing starved the finished connection");
}
