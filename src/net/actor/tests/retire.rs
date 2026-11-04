use super::*;

/// Returns whether the peer has no connection, no messages, and a closed queue.
fn is_idle(peer: &PeerState) -> bool {
    peer.is_idle()
}

/// The connection a shuffle reply opened retires once the origin reads the reply.
///
/// The replying node is usually not a neighbor of the shuffle's origin. The
/// origin drops it after it reads the reply. Our connection to the origin is
/// then idle, so we retire it.
#[tokio::test]
#[traced_test]
async fn shuffle_reply_connection_retires() -> Result {
    let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
    let ct = CancellationToken::new();
    let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
    let memory_lookup = MemoryLookup::new();
    let (_go1, actor, ep1_handle) =
        Gossip::t_new_with_actor(rng, Default::default(), relay_map.clone(), &ct).await?;
    let _ep1_handle = AbortOnDropHandle::new(ep1_handle);
    let mut actor = ManualActorLoop::new(actor).await;
    actor.conn_shared.retire_after = Duration::from_millis(300);
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

    // The origin of the shuffle reads the reply, then answers the retire.
    let origin = create_endpoint(rng, relay_map.clone(), None).await?;
    let origin_id = origin.id();
    memory_lookup.add_endpoint_info(EndpointAddr::new(origin_id).with_relay_url(relay_url));
    let mut origin_task = AbortOnDropHandle::new(spawn(async move {
        let incoming = origin.accept().await.expect("endpoint closed");
        let conn = incoming.await.expect("accept failed");
        let mut control = RawControl::accept(&conn).await.expect("control");
        let mut stream = conn.accept_uni().await.expect("accept_uni");
        let mut buf = Vec::new();
        wire::StreamHeader::read(&mut stream, &mut buf)
            .await
            .expect("header");
        let reply: Option<proto::topic::Message<PublicKey>> =
            wire::read_message(&mut stream, &mut buf, 1024)
                .await
                .expect("frame");
        let ControlMessage::Goaway(sent) = control.read().await.expect("goaway");
        control
            .write(ControlMessage::Goaway(0))
            .await
            .expect("goaway");
        control.finish().expect("finish");
        let reason = conn.closed().await;
        (reply.is_some(), sent, reason)
    }));

    // A third node forwards the shuffle to us with its walk at an end.
    let ep3 = create_endpoint(rng, relay_map, None).await?;
    let conn = ep3
        .connect(ep1_addr, GOSSIP_ALPN)
        .await
        .std_context("connect")?;
    let _control = control_open(&conn).await?;
    let mut stream = conn.open_uni().await.std_context("open_uni")?;
    let mut buf = Vec::new();
    wire::StreamHeader::write(topic, &mut stream, &mut buf)
        .await
        .std_context("header")?;
    let shuffle = proto::topic::Message::test_shuffle(origin_id, 0);
    wire::write_message(&mut stream, &shuffle, &mut buf, 1024)
        .await
        .std_context("frame")?;

    let (replied, sent, reason) = timeout(Duration::from_secs(10), async {
        loop {
            tokio::select! {
                _ = actor.step() => {}
                res = &mut origin_task => return res.expect("origin task"),
            }
        }
    })
    .await
    .std_context("the reply connection did not retire")?;
    assert!(replied, "the shuffle reply did not arrive");
    assert_eq!(sent, 1, "the reply opened one stream");
    assert!(
        matches!(&reason, iroh::endpoint::ConnectionError::ApplicationClosed(c) if c.error_code == CloseCode::Retired.code()),
        "{reason:?}"
    );
    pump_until(&mut actor, "the state of the origin goes", |actor| {
        actor.state.peers.get(&origin_id).is_none_or(is_idle)
    })
    .await
}

// The control stream and the `GOAWAY` retire.
//
// The node under test runs `connection_loop` on one side of a real connection.
// A raw peer plays the other side by hand: it writes the bytes of each step.

/// A `GOAWAY` whose count does not match the accepted streams fails the connection.
///
/// A count below the streams that the node accepted fails at once. So does a
/// stream after the `GOAWAY` that its count does not cover. A count over the
/// accepted streams is valid, as the streams can still arrive.
#[tokio::test]
#[traced_test]
async fn goaway_with_a_wrong_count_fails() -> Result {
    let pair = TestPair::new(1).await?;
    let topics: [TopicId; 2] = [
        blake3::hash(b"count a").into(),
        blake3::hash(b"count b").into(),
    ];
    let join = |topic| [join_message(topic, 0).message];
    for case in ["count too low", "stream after the count"] {
        let (conn, peer_conn) = pair.connect_back().await?;
        let mut test = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_secs(60));
        let mut control = RawControl::open(&peer_conn).await?;
        let _first = open_stream(&peer_conn, topics[0], &join(topics[0])).await?;
        test.next().await?;
        let _second = if case == "count too low" {
            let second = open_stream(&peer_conn, topics[1], &join(topics[1])).await?;
            test.next().await?;
            control.write(ControlMessage::Goaway(1)).await?;
            second
        } else {
            control.write(ControlMessage::Goaway(1)).await?;
            open_stream(&peer_conn, topics[1], &join(topics[1])).await?
        };
        let code = close_code(&peer_conn, SECS_10).await?;
        assert_eq!(code, Some(CloseCode::Protocol), "{case}");
        assert_eq!(ended(test.task, SECS_10).await?, ConnEnd::Failed, "{case}");
    }

    // A count over the accepted streams waits for the rest, and retires.
    let (conn, peer_conn) = pair.connect_back().await?;
    let mut test = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_secs(60));
    let mut control = RawControl::open(&peer_conn).await?;
    let mut first = open_stream(&peer_conn, topics[0], &join(topics[0])).await?;
    test.next().await?;
    control.write(ControlMessage::Goaway(2)).await?;
    let mut second = open_stream(&peer_conn, topics[1], &join(topics[1])).await?;
    test.next().await?;
    first.finish().std_context("finish")?;
    second.finish().std_context("finish")?;
    assert_eq!(control.read().await?, ControlMessage::Goaway(0));
    control.finish()?;
    assert_eq!(
        close_code(&peer_conn, SECS_10).await?,
        Some(CloseCode::Retired)
    );
    assert_eq!(ended(test.task, SECS_10).await?, ConnEnd::Retired);
    Ok(())
}
