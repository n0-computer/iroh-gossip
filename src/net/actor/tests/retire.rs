use super::*;

/// Accepts the next stream of the peer, and reads its header.
async fn read_stream_header(conn: &Connection) -> Result<(TopicId, RecvStream)> {
    let mut stream = conn.accept_uni().await.std_context("accept stream")?;
    let header = wire::StreamHeader::read(&mut stream, &mut Vec::new())
        .await
        .std_context("read header")?;
    Ok((header.topic, stream))
}

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

    // A third node joins us, and then forwards the shuffle to us with its walk at an end.
    // We take a shuffle only from a neighbor.
    let ep3 = create_endpoint(rng, relay_map, None).await?;
    let conn = ep3
        .connect(ep1_addr, GOSSIP_ALPN)
        .await
        .std_context("connect")?;
    let _control = control_open(&conn).await?;
    let mut stream = conn.open_uni().await.std_context("open_uni")?;
    let mut buf = Vec::new();
    wire::StreamHeader::write(topic, None, &mut stream, &mut buf)
        .await
        .std_context("header")?;
    let join = proto::topic::Message::Swarm(proto::hyparview::Message::Join(None));
    let shuffle = proto::topic::Message::test_shuffle(origin_id, 0);
    for message in [join, shuffle] {
        wire::write_message(&mut stream, &message, &mut buf, 1024)
            .await
            .std_context("frame")?;
    }

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

/// Returns the names of the events of a connection task, in order.
fn event_names(events: &mut mpsc::Receiver<ConnEvent>) -> Vec<&'static str> {
    let mut seen = Vec::new();
    while let Ok(event) = events.try_recv() {
        seen.push(match event.kind {
            ConnEventKind::Header(..) => "header",
            ConnEventKind::Message(_) => "message",
            ConnEventKind::Refused(_) => "refused",
            ConnEventKind::Rejected(_) => "rejected",
            ConnEventKind::Retiring => "retiring",
        });
    }
    seen
}

/// The idle timer of a dialer runs while no stream is open in either direction.
///
/// A change of the streams restarts it.
#[tokio::test]
#[traced_test]
async fn idle_timer_follows_the_streams() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let test = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_millis(300));
    let mut control = RawControl::accept(&peer_conn).await?;
    let topic: TopicId = blake3::hash(b"idle_timer").into();
    let idle = Duration::from_millis(900);
    // A stream of the peer keeps the connection. The timer starts once the stream ends.
    let mut stream = open_stream(&peer_conn, topic, &[]).await?;
    assert!(timeout(idle, control.read()).await.is_err());
    stream.finish().std_context("finish")?;
    let start = Instant::now();
    assert_eq!(control.read().await?, ControlMessage::Goaway(0));
    assert!(start.elapsed() >= Duration::from_millis(250));
    drop(test);

    // A stream of ours, for a topic of the peer, keeps it too.
    let (conn, peer_conn) = pair.connect().await?;
    let test = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_millis(300));
    let mut control = RawControl::accept(&peer_conn).await?;
    let (_queue, reader) = SendQueue::new(queue::MIN_QUEUE_SIZE);
    test.take_queue(reader, vec![topic]);
    let (opened, _stream) = read_stream_header(&peer_conn).await?;
    assert_eq!(opened, topic);
    assert!(timeout(idle, control.read()).await.is_err());
    Ok(())
}

/// An idle node sends `GOAWAY(0)` and closes with `Retired` after the peer answers.
///
/// The close waits until the peer finished its half.
#[tokio::test]
#[traced_test]
async fn idle_node_sends_goaway_and_closes_after_the_answer() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let mut test = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_millis(200));
    let mut control = RawControl::accept(&peer_conn).await?;
    assert_eq!(control.read().await?, ControlMessage::Goaway(0));
    control.write(ControlMessage::Goaway(0)).await?;
    // The node waits until the peer read all of its streams too.
    assert!(
        timeout(Duration::from_millis(300), peer_conn.closed())
            .await
            .is_err(),
        "closed before the peer finished"
    );
    control.finish()?;
    assert_eq!(
        close_code(&peer_conn, SECS_10).await?,
        Some(CloseCode::Retired)
    );
    assert_eq!(ended(test.task, SECS_10).await?, ConnEnd::Retired);
    assert_eq!(event_names(&mut test.events), ["header", "retiring"]);
    Ok(())
}

/// A retire that does not finish closes the connection with `BLOCKED` at the deadline.
///
/// The peer gives no `GOAWAY`, or a stream that it counted never ends.
#[tokio::test]
#[traced_test]
async fn retire_deadline_closes() -> Result {
    let pair = TestPair::new(1).await?;
    let topic: TopicId = blake3::hash(b"deadline").into();
    for case in ["no answer", "stream of the peer"] {
        let (conn, peer_conn) = pair.connect().await?;
        let test = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_millis(100));
        let mut control = RawControl::accept(&peer_conn).await?;
        let _open = if case == "no answer" {
            None
        } else {
            assert_eq!(control.read().await?, ControlMessage::Goaway(0));
            let open = open_stream(&peer_conn, topic, &[join_message(topic, 0).message]).await?;
            control.write(ControlMessage::Goaway(1)).await?;
            Some(open)
        };
        let code = close_code(&peer_conn, conn::RETIRE_TIMEOUT * 3).await?;
        assert_eq!(code, Some(CloseCode::Blocked), "{case}");
        assert_eq!(ended(test.task, SECS_10).await?, ConnEnd::Failed, "{case}");
    }
    Ok(())
}

/// A node answers the `GOAWAY` of the peer once it read the counted streams.
///
/// It closes once the peer finished.
#[tokio::test]
#[traced_test]
async fn goaway_is_answered_after_the_counted_streams() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect_back().await?;
    let mut test = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_secs(60));
    let mut control = RawControl::open(&peer_conn).await?;
    let topic: TopicId = blake3::hash(b"goaway_answer").into();
    let mut stream = open_stream(&peer_conn, topic, &[join_message(topic, 0).message]).await?;
    control.write(ControlMessage::Goaway(1)).await?;
    // The node waits for the end of the counted stream.
    assert!(
        timeout(Duration::from_millis(300), control.read())
            .await
            .is_err(),
        "the answer came before the stream ended"
    );
    stream.finish().std_context("finish")?;
    assert_eq!(control.read().await?, ControlMessage::Goaway(0));
    // The node waits until we read all its streams: we finish our half.
    assert!(
        timeout(Duration::from_millis(300), peer_conn.closed())
            .await
            .is_err(),
        "closed before the peer finished"
    );
    control.finish()?;
    assert_eq!(
        close_code(&peer_conn, SECS_10).await?,
        Some(CloseCode::Retired)
    );
    assert_eq!(ended(test.task, SECS_10).await?, ConnEnd::Retired);
    // The message came before the end of the task.
    assert_eq!(
        event_names(&mut test.events),
        ["header", "message", "retiring"]
    );
    Ok(())
}

/// The end of a connection follows the close code of the peer.
#[tokio::test]
#[traced_test]
async fn close_code_decides_the_end() -> Result {
    let pair = TestPair::new(1).await?;
    for (code, end) in [
        (CloseCode::Retired, ConnEnd::Retired),
        (CloseCode::Stale, ConnEnd::Quiet),
        (CloseCode::Shutdown, ConnEnd::Failed),
        (CloseCode::SendFailed, ConnEnd::Failed),
        (CloseCode::Blocked, ConnEnd::Failed),
    ] {
        let (conn, peer_conn) = pair.connect().await?;
        let test = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_secs(60));
        let _control = RawControl::accept(&peer_conn).await?;
        peer_conn.close(code.code(), b"bye");
        assert_eq!(ended(test.task, SECS_10).await?, end, "{code:?}");
    }
    // An unknown code and code 0 are not a finished retire. noq sends code 0 when the
    // last handle of a connection drops without a close, as in a crash.
    for code in [0u32, 77] {
        let (conn, peer_conn) = pair.connect().await?;
        let test = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_secs(60));
        let _control = RawControl::accept(&peer_conn).await?;
        peer_conn.close(code.into(), b"bye");
        assert_eq!(ended(test.task, SECS_10).await?, ConnEnd::Failed, "{code}");
    }
    Ok(())
}

/// The node fails the connection if the peer does not finish its control stream after `GOAWAY`.
#[tokio::test]
#[traced_test]
async fn acceptor_without_close_times_out() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect_back().await?;
    let test = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_secs(60));
    let mut control = RawControl::open(&peer_conn).await?;
    control.write(ControlMessage::Goaway(0)).await?;
    assert_eq!(control.read().await?, ControlMessage::Goaway(0));
    let code = close_code(&peer_conn, conn::RETIRE_TIMEOUT * 2).await?;
    assert_eq!(code, Some(CloseCode::Blocked));
    assert_eq!(ended(test.task, SECS_10).await?, ConnEnd::Failed);
    Ok(())
}

/// A second `GOAWAY`, or a control stream that ends without one, closes the connection with `PROTOCOL`.
#[tokio::test]
#[traced_test]
async fn control_violations_close_with_protocol() -> Result {
    let pair = TestPair::new(1).await?;
    for twice in [true, false] {
        let (conn, peer_conn) = pair.connect_back().await?;
        let test = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_secs(60));
        let mut control = RawControl::open(&peer_conn).await?;
        if twice {
            control.write(ControlMessage::Goaway(0)).await?;
            control.write(ControlMessage::Goaway(0)).await?;
        } else {
            control.finish()?;
        }
        assert_eq!(
            close_code(&peer_conn, SECS_10).await?,
            Some(CloseCode::Protocol)
        );
        drop(test);
    }
    Ok(())
}

/// A missing control header closes with `PROTOCOL` after the header timeout.
#[tokio::test]
#[traced_test]
async fn header_timeout_closes() -> Result {
    let pair = TestPair::new(1).await?;
    // The acceptor gets no control stream at all.
    let (conn, peer_conn) = pair.connect_back().await?;
    let test = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_secs(60));
    let code = close_code(&peer_conn, wire::HEADER_TIMEOUT * 3).await?;
    assert_eq!(code, Some(CloseCode::Protocol));
    drop(test.task);
    // A dial whose acceptor never answers its header: the connection fails.
    let (conn, peer_conn) = pair.connect().await?;
    let test = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_secs(60));
    let _silent = peer_conn.accept_bi().await.std_context("accept control")?;
    let code = close_code(&peer_conn, wire::HEADER_TIMEOUT * 3).await?;
    assert_eq!(code, Some(CloseCode::Protocol));
    assert_eq!(ended(test.task, SECS_10).await?, ConnEnd::Failed);
    Ok(())
}

/// An early topic stream waits for the control header, and a late header is valid.
#[tokio::test]
#[traced_test]
async fn early_topic_stream_waits_for_control() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect_back().await?;
    let mut test = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_secs(60));
    let topic: TopicId = blake3::hash(b"early").into();
    let _stream = open_stream(&peer_conn, topic, &[join_message(topic, 0).message]).await?;
    // The node dispatches nothing before the header.
    assert!(
        timeout(Duration::from_millis(500), test.events.recv())
            .await
            .is_err(),
        "an event came before the control header"
    );
    let _control = RawControl::open(&peer_conn).await?;
    let mut kinds = Vec::new();
    while kinds.len() < 2 {
        let event = timeout(Duration::from_secs(5), test.events.recv())
            .await
            .std_context("no event")?
            .std_context("the task ended")?;
        kinds.push(matches!(event.kind, ConnEventKind::Header(..)));
    }
    assert_eq!(kinds, [true, false], "the header comes before the message");
    assert!(peer_conn.close_reason().is_none());
    Ok(())
}

/// A reset or a truncated frame on a counted stream fails the connection, also after our `GOAWAY`.
#[tokio::test]
#[traced_test]
async fn reset_and_truncation_after_retire_fail() -> Result {
    let pair = TestPair::new(1).await?;
    let topic: TopicId = blake3::hash(b"broken").into();
    let break_stream = |how: &'static str, mut stream: SendStream| async move {
        match how {
            "reset" => stream.reset(0u32.into()).anyerr(),
            _ => {
                // A frame that announces 50 bytes and has 3.
                stream
                    .write_all(&[50, 1, 2, 3])
                    .await
                    .std_context("write")?;
                stream.finish().std_context("finish")
            }
        }
    };
    for how in ["reset", "truncated"] {
        // Before our `GOAWAY`.
        let (conn, peer_conn) = pair.connect_back().await?;
        let test = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_secs(60));
        let mut control = RawControl::open(&peer_conn).await?;
        let stream = open_stream(&peer_conn, topic, &[join_message(topic, 0).message]).await?;
        control.write(ControlMessage::Goaway(1)).await?;
        break_stream(how, stream).await?;
        assert_eq!(
            close_code(&peer_conn, SECS_10).await?,
            Some(CloseCode::Protocol),
            "{how} before GOAWAY"
        );
        drop(test.task);

        // After our `GOAWAY`: the counted stream breaks while we drain.
        let (conn, peer_conn) = pair.connect().await?;
        let test = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_millis(100));
        let mut control = RawControl::accept(&peer_conn).await?;
        assert_eq!(control.read().await?, ControlMessage::Goaway(0));
        let stream = open_stream(&peer_conn, topic, &[join_message(topic, 0).message]).await?;
        control.write(ControlMessage::Goaway(1)).await?;
        break_stream(how, stream).await?;
        assert_eq!(
            close_code(&peer_conn, SECS_10).await?,
            Some(CloseCode::Protocol),
            "{how} after GOAWAY"
        );
        assert_eq!(ended(test.task, SECS_10).await?, ConnEnd::Failed);
    }
    Ok(())
}

/// A new session id replaces a connection that waits for the close after `GOAWAY`, or has a stream open.
///
/// The node closes it with `STALE` and ignores the events that it still has buffered.
#[tokio::test]
#[traced_test]
async fn new_session_replaces() -> Result {
    let pair = TestPair::new(1).await?;
    let (mut actor, _to_actor) = Actor::new(
        pair.ep1.clone(),
        ActorConfig {
            proto: Default::default(),
            ..Default::default()
        },
        Default::default(),
        Default::default(),
    );
    let topic: TopicId = [1; 32].into();
    let state = TopicHandles::default();
    let mut events = state.events.subscribe();
    actor.state.topics.insert(topic, state);
    actor.handle_in_event(
        InEvent::Command(topic, ProtoCommand::Join(vec![])),
        Instant::now(),
    );
    let mut actor = ManualActorLoop::new(actor).await;

    // The first connection: the peer joins, and keeps its stream open.
    let (conn, peer_conn) = pair.connect_back().await?;
    let peer = conn.remote_id();
    actor.add_connection(conn);
    let _control = RawControl::open(&peer_conn).await?;
    let message = join_message(topic, 0).message;
    let _stream = open_stream(&peer_conn, topic, &[message]).await?;
    pump_until(&mut actor, "the join", |actor| actor.joined(&topic)).await?;

    // A second connection with a new session replaces it.
    let (conn2, peer_conn2) = pair.connect_back().await?;
    actor.add_connection(conn2);
    let _control2 = control_open_with(&peer_conn2, Session([10; 8])).await?;
    pump_until(&mut actor, "the replacement", |actor| {
        !actor.state.peers[&peer].has_conn(0) && actor.state.peers[&peer].has_conn(1)
    })
    .await?;
    closed_with(&peer_conn, CloseCode::Stale).await?;
    assert!(peer_conn2.close_reason().is_none());

    // Events of the old connection that were still buffered change nothing.
    let stale = ConnEvent {
        peer,
        conn: 0,
        kind: ConnEventKind::Message(join_message(topic, 1)),
    };
    actor.on_conn_event(stale, Instant::now());
    assert_eq!(actor.state.peers[&peer].conn_count(), 1);
    let mut seen = Vec::new();
    while let Ok(event) = events.try_recv() {
        seen.push(event);
    }
    // The old session made the peer a neighbor, and the new one reset it.
    assert!(
        matches!(&seen[..], [ProtoEvent::NeighborUp(a), ProtoEvent::NeighborDown(b)] if *a == peer && *b == peer),
        "{seen:?}"
    );
    Ok(())
}

/// The node reads a burst of streams that the peer writes right after our `GOAWAY`, before the close.
#[tokio::test]
#[traced_test]
async fn retire_loses_no_unread_data() -> Result {
    const STREAMS: usize = 40;
    const PER_STREAM: usize = 10;
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let mut test = start_conn(conn, TEST_FRAME_LIMIT, Duration::from_millis(200));
    let mut events = std::mem::replace(&mut test.events, mpsc::channel(1).1);
    let counter = spawn(async move {
        let mut messages = 0;
        while let Some(event) = events.recv().await {
            messages += usize::from(matches!(event.kind, ConnEventKind::Message(_)));
        }
        messages
    });
    let mut control = RawControl::accept(&peer_conn).await?;
    assert_eq!(control.read().await?, ControlMessage::Goaway(0));
    let topic: TopicId = blake3::hash(b"unread").into();
    // The burst: every stream carries messages that are still unread when our `GOAWAY` arrives.
    let mut writers = Vec::new();
    for i in 0..STREAMS {
        let messages: Vec<_> = (0..PER_STREAM)
            .map(|j| join_message(topic, i * PER_STREAM + j).message)
            .collect();
        let mut stream = open_stream(&peer_conn, topic, &messages).await?;
        writers.push(spawn(async move { stream.finish() }));
    }
    control
        .write(ControlMessage::Goaway(STREAMS as u64))
        .await?;
    for writer in writers {
        writer.await.std_context("writer")?.std_context("finish")?;
    }
    control.finish()?;
    assert_eq!(
        close_code(&peer_conn, SECS_10).await?,
        Some(CloseCode::Retired)
    );
    assert_eq!(ended(test.task, SECS_10).await?, ConnEnd::Retired);
    // The actor got every message before the task ended.
    assert_eq!(counter.await.std_context("counter")?, STREAMS * PER_STREAM);
    Ok(())
}

/// A retiring writer keeps the queue until its connection closes.
///
/// Thus the peer reads the messages of a topic in order.
///
/// The close comes only once the peer read every stream that the writer counted.
#[tokio::test]
#[traced_test]
async fn retiring_writer_keeps_the_queue_until_it_closes() -> Result {
    let pair = TestPair::new(1).await?;
    let peer = pair.ep2.id();
    let topic: TopicId = blake3::hash(b"keeps_the_queue").into();
    let mut actor = ManualActorLoop::new(t_actor().await?).await;
    let (first, first_peer) = pair.connect_back().await?;
    actor.add_connection(first);
    let mut first_control = RawControl::open(&first_peer).await?;
    pump_until(&mut actor, "the first connection writes", |actor| {
        actor.state.peers[&peer].writer() == Some(0)
    })
    .await?;
    let (second, second_peer) = pair.connect_back().await?;
    actor.add_connection(second);
    let _second_control = RawControl::open(&second_peer).await?;

    // The peer retires the first connection, and the node answers.
    first_control.write(ControlMessage::Goaway(0)).await?;
    assert_eq!(first_control.read().await?, ControlMessage::Goaway(0));
    let join = InEvent::RecvMessage(peer, join_message(topic, 0));
    actor.handle_in_event(join, Instant::now());
    let moved = timeout(Duration::from_millis(500), async {
        while actor.state.peers[&peer].writer() == Some(0) {
            actor.step().await;
        }
    })
    .await;
    assert!(moved.is_err(), "the queue moved while the retire ran");
    let early = timeout(Duration::from_millis(100), second_peer.accept_uni()).await;
    assert!(early.is_err(), "the second connection wrote");

    // The peer read all of it: the first connection closes, and the queue moves.
    first_control.finish()?;
    pump_until(&mut actor, "the second connection writes", |actor| {
        actor.state.peers[&peer].writer() == Some(1)
    })
    .await?;
    assert_eq!(
        close_code(&first_peer, SECS_10).await?,
        Some(CloseCode::Retired)
    );
    let (sent, message) = timeout(Duration::from_secs(5), read_next_message(&second_peer))
        .await
        .std_context("the queue did not move")??;
    assert_eq!(sent, topic);
    assert!(message.is_disconnect());
    Ok(())
}

/// A retiring writer that gets a queue writes none of it, and returns it with all its messages.
#[tokio::test]
#[traced_test]
async fn retiring_writer_returns_the_queue_at_its_end() -> Result {
    let pair = TestPair::new(1).await?;
    let (conn, peer_conn) = pair.connect().await?;
    let (test, mut control) = start_conn_peered(conn, &peer_conn, TEST_FRAME_LIMIT).await?;
    test.retire.cancel();
    assert_eq!(control.read().await?, ControlMessage::Goaway(0));
    let (queue, reader) = SendQueue::new(queue::MIN_QUEUE_SIZE);
    let topic: TopicId = blake3::hash(b"returns_the_queue").into();
    assert!(matches!(queue.send(join_message(topic, 0)), Pushed::Queued));
    test.take_queue(reader, vec![topic]);
    let opened = timeout(Duration::from_millis(300), peer_conn.accept_uni()).await;
    assert!(opened.is_err(), "the retiring writer wrote");
    control.write(ControlMessage::Goaway(0)).await?;
    control.finish()?;
    let exit = exited(test.task, SECS_10).await?;
    assert_eq!(exit.end, ConnEnd::Retired);
    let reader = exit.reader.std_context("the reader did not come back")?;
    assert!(
        reader.reads(&queue) && !reader.is_empty(),
        "the queue changed"
    );
    Ok(())
}

/// Handles the events of the actor until `fut` completes, or fails after `wait`.
async fn step_until<T>(
    actor: &mut ManualActorLoop,
    wait: Duration,
    fut: impl std::future::Future<Output = T>,
) -> Result<T> {
    let mut fut = std::pin::pin!(fut);
    timeout(wait, async {
        loop {
            tokio::select! {
                res = &mut fut => return res,
                _ = actor.step() => {}
            }
        }
    })
    .await
    .std_context("timed out")
}

/// A message during an idle retire dials at once, and goes out on the new connection after the close.
#[tokio::test]
#[traced_test]
async fn message_during_an_idle_retire_dials_at_once() -> Result {
    let pair = TestPair::new(1).await?;
    let peer = pair.ep2.id();
    let lookup = MemoryLookup::new();
    pair.ep1
        .address_lookup()
        .expect("endpoint is not closed")
        .add(lookup.clone());
    lookup.add_endpoint_info(pair.ep2_addr.clone());
    let (actor, _to_actor) = Actor::new(
        pair.ep1.clone(),
        ActorConfig {
            proto: Default::default(),
            ..Default::default()
        },
        Default::default(),
        Default::default(),
    );
    let mut actor = ManualActorLoop::new(actor).await;
    actor.conn_shared.retire_after = Duration::from_millis(200);
    let (first, first_peer) = pair.connect().await?;
    actor.add_connection(first.clone());
    // Only the first connection retires when idle.
    actor.conn_shared.retire_after = Duration::from_secs(60);
    let mut first_control = RawControl::accept(&first_peer).await?;
    assert_eq!(first_control.read().await?, ControlMessage::Goaway(0));

    // The peer does not answer the retire yet. A message dials at once.
    let topic: TopicId = blake3::hash(b"idle_retire_dials").into();
    let join = InEvent::RecvMessage(peer, join_message(topic, 0));
    actor.handle_in_event(join, Instant::now());
    let accept = async {
        let incoming = pair.ep2.accept().await.std_context("endpoint closed")?;
        let conn = incoming.await.std_context("accept")?;
        let control = RawControl::accept(&conn).await?;
        n0_error::Ok((conn, control))
    };
    let (second_peer, _second_control) = step_until(&mut actor, Duration::from_secs(2), accept)
        .await
        .std_context("no dial during the retire")??;
    let early = step_until(
        &mut actor,
        Duration::from_millis(300),
        second_peer.accept_uni(),
    )
    .await;
    assert!(early.is_err(), "the new connection wrote during the retire");
    assert_eq!(actor.state.peers[&peer].writer(), Some(0));

    // The peer answers: the first connection closes, and the message goes out on the second.
    first_control.write(ControlMessage::Goaway(0)).await?;
    first_control.finish()?;
    let (sent, message) =
        step_until(&mut actor, SECS_10, read_next_message(&second_peer)).await??;
    assert_eq!(sent, topic);
    assert!(message.is_disconnect());
    assert!(
        first.close_reason().is_some(),
        "the first connection is open"
    );
    assert_eq!(
        close_code(&first_peer, SECS_10).await?,
        Some(CloseCode::Retired)
    );
    Ok(())
}
