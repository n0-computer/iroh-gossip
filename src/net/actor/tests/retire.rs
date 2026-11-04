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
