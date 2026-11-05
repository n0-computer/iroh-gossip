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
    actor.state.topics.insert(topic, state);
    actor.handle_in_event(
        InEvent::Command(topic, ProtoCommand::Join(vec![])),
        Instant::now(),
    );
    // The actor gives its first connection the id 0.
    let message = ConnEvent {
        peer,
        conn: 0,
        kind: ConnEventKind::Message(join_message(topic, 0)),
    };
    actor
        .conn_shared
        .events
        .send(message)
        .await
        .std_context("queue join")?;
    conn.close(0u32.into(), b"done");
    actor.add_connection(conn);
    let (peer, id, end) = actor
        .connection_tasks
        .join_next()
        .await
        .expect("one task")
        .expect("no panic");
    actor.on_conn_task_finished(peer, id, end);

    assert!(
        !actor.proto.has_active_peers(&topic),
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

/// A reader that waits in the mailbox of an ended connection goes back to the state.
///
/// The header of an accepted connection makes it the writer only once the actor reads it.
/// When the task has ended by then, the reader is in the mailbox, not in the task result.
#[tokio::test]
#[traced_test]
async fn reader_in_the_mailbox_of_an_ended_task_comes_back() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect_back().await?;
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
    // The actor gives its first connection the id 0.
    let header = ConnEvent {
        peer,
        conn: 0,
        kind: ConnEventKind::Header(PEER_SESSION),
    };
    actor
        .conn_shared
        .events
        .send(header)
        .await
        .std_context("queue header")?;
    peer_conn.close(CloseCode::Retired.code(), b"retired");
    conn.closed().await;
    actor.add_connection(conn);
    let state = actor.state.peers.get_mut(&peer).expect("added");
    state.send(join_message([1; 32].into(), 0));
    let (peer, id, exit) = actor
        .connection_tasks
        .join_next()
        .await
        .expect("one task")
        .expect("no panic");
    assert!(exit.reader.is_none(), "the task never had the reader");
    actor.on_conn_task_finished(peer, id, exit);

    let state = actor.state.peers.get_mut(&peer).expect("a message waits");
    assert_eq!(state.writer(), None);
    assert!(!state.queue.is_empty(), "the queue was discarded");
    assert!(state.take_reader().is_some(), "the reader was lost");
    Ok(())
}
