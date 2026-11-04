use super::*;

/// A close handles the messages queued before it, not those that arrive meanwhile.
///
/// Other connections can refill the queue as fast as the actor handles it.
#[tokio::test(flavor = "multi_thread")]
#[traced_test]
async fn close_does_not_wait_for_an_empty_queue() -> Result {
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
    conn.close(0u32.into(), b"done");
    actor.add_connection(conn);
    let (peer, id, end) = actor
        .connection_tasks
        .join_next()
        .await
        .expect("one task")
        .expect("no panic");

    // Another connection: it sends ten full queues, as fast as the actor takes them.
    let conn_events_tx = actor.conn_shared.events.clone();
    let other = SecretKey::generate().public();
    let producer = AbortOnDropHandle::new(spawn(async move {
        for _ in 0..10 * CONN_EVENT_CAP {
            let event = ConnEvent {
                peer: other,
                conn: 99,
                kind: ConnEventKind::Header(Session([0; 8])),
            };
            conn_events_tx.send(event).await.expect("actor alive");
        }
    }));
    while actor.conn_events_rx.len() < CONN_EVENT_CAP {
        tokio::task::yield_now().await;
    }
    actor.on_conn_task_finished(peer, id, end);
    assert!(
        !producer.is_finished(),
        "the close waited for an empty queue"
    );
    Ok(())
}

/// A shutdown comes before the commands of a busy publisher.
///
/// Without a priority, the actor picks its next branch at random, so a single
/// round can miss the bug.
#[tokio::test]
#[traced_test]
async fn shutdown_comes_before_a_busy_publisher() -> Result {
    let pair = TestPair::new(1).await?;
    for round in 0..8 {
        let (mut actor, to_actor) = Actor::new(
            pair.ep1.clone(),
            ActorConfig {
                proto: Default::default(),
                ..Default::default()
            },
            Default::default(),
            Default::default(),
        );
        publish_forever(&mut actor);
        let (reply, mut reply_rx) = oneshot::channel();
        to_actor
            .send(ToActor::Shutdown { reply })
            .await
            .std_context("send shutdown")?;
        let mut addresses = n0_future::stream::pending();
        assert!(
            !actor.event_loop(&mut addresses).await,
            "round {round}: the actor did not stop"
        );
        reply_rx.try_recv().std_context("no shutdown reply")?;
    }
    Ok(())
}

/// A shutdown lets the writer send its queue and finish its streams.
///
/// The connection then closes with `SHUTDOWN`.
#[tokio::test]
#[traced_test]
async fn shutdown_sends_the_queue_then_closes() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let topic: TopicId = blake3::hash(b"shutdown").into();
    let (queue, reader) = SendQueue::new(queue::MIN_QUEUE_SIZE);
    let (test, _control) = start_conn_peered(conn, &peer_conn, TEST_FRAME_LIMIT).await?;
    test.take_queue(reader, vec![]);
    for _ in 0..3 {
        assert!(matches!(queue.send(join_message(topic, 0)), Pushed::Queued));
    }
    test.shutdown.cancel();
    let (sent, messages) = read_next_stream(&peer_conn).await?;
    assert_eq!(
        (sent, messages.len()),
        (topic, 3),
        "the queue did not go out"
    );
    assert_eq!(
        close_code(&peer_conn, SECS_10).await?,
        Some(CloseCode::Shutdown)
    );
    assert_eq!(ended(test.task, SECS_10).await?, ConnEnd::Failed);
    Ok(())
}

/// An aborted peer closes the connection with `BLOCKED`.
#[tokio::test]
#[traced_test]
async fn aborted_peer_closes_the_connection() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let (test, _control) = start_conn_peered(conn, &peer_conn, TEST_FRAME_LIMIT).await?;
    test.aborted.cancel();
    assert_eq!(
        close_code(&peer_conn, SECS_10).await?,
        Some(CloseCode::Blocked)
    );
    assert_eq!(ended(test.task, SECS_10).await?, ConnEnd::Failed);
    Ok(())
}
