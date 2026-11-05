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
    let (peer, conn, result) = actor
        .connection_tasks
        .join_next()
        .await
        .expect("one task")
        .expect("no panic");

    // Another connection: it sends ten full queues, as fast as the actor takes them.
    let in_event_tx = actor.in_event_tx.clone();
    let other = SecretKey::generate().public();
    let producer = AbortOnDropHandle::new(spawn(async move {
        for _ in 0..10 * IN_EVENT_CAP {
            in_event_tx
                .send(InEvent::PeerDisconnected(other))
                .await
                .expect("actor alive");
        }
    }));
    while actor.in_event_rx.len() < IN_EVENT_CAP {
        tokio::task::yield_now().await;
    }
    actor.on_conn_task_finished(peer, conn, result);
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
