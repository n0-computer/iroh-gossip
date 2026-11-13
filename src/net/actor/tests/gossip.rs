use super::*;

/// `subscribe` returns only after the actor registered the topic.
///
/// Otherwise a Join from a peer could arrive at a node that does not know the
/// topic yet, and the node would refuse it.
#[tokio::test]
#[traced_test]
async fn subscribe_returns_after_the_topic_is_registered() {
    let mut rng = rand::rngs::ChaCha12Rng::seed_from_u64(1);
    let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
    let cancel = CancellationToken::new();
    let (gossip, actor, _ep_task) =
        Gossip::t_new_with_actor(&mut rng, Default::default(), relay_map, &cancel)
            .await
            .unwrap();
    let mut actor = ManualActorLoop::new(actor).await;
    let topic: TopicId = blake3::hash(b"registered").into();

    let subscribe = gossip.subscribe(topic, vec![]);
    tokio::pin!(subscribe);
    // The actor does not run, so the call must wait for it.
    assert!(timeout(Duration::from_millis(100), &mut subscribe)
        .await
        .is_err());
    let result = loop {
        tokio::select! {
            result = &mut subscribe => break result,
            _ = actor.step() => {}
        }
    };
    let _topic = result.unwrap();
    assert!(
        actor.proto.state(&topic).is_some(),
        "topic is not registered"
    );
    assert!(actor.state.topics.contains_key(&topic));
    cancel.cancel();
}

#[tokio::test]
#[traced_test]
async fn gossip_net_smoke() {
    let mut rng = rand::rngs::ChaCha12Rng::seed_from_u64(1);
    let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();

    let memory_lookup = MemoryLookup::new();

    let ep1 = create_endpoint(&mut rng, relay_map.clone(), Some(memory_lookup.clone()))
        .await
        .unwrap();
    let ep2 = create_endpoint(&mut rng, relay_map.clone(), Some(memory_lookup.clone()))
        .await
        .unwrap();
    let ep3 = create_endpoint(&mut rng, relay_map.clone(), Some(memory_lookup.clone()))
        .await
        .unwrap();

    let go1 = Gossip::builder().spawn(ep1.clone());
    let go2 = Gossip::builder().spawn(ep2.clone());
    let go3 = Gossip::builder().spawn(ep3.clone());
    debug!("peer1 {:?}", ep1.id());
    debug!("peer2 {:?}", ep2.id());
    debug!("peer3 {:?}", ep3.id());
    let pi1 = ep1.id();
    let pi2 = ep2.id();

    let cancel = CancellationToken::new();
    let tasks = [
        spawn(endpoint_loop(ep1.clone(), go1.clone(), cancel.clone())),
        spawn(endpoint_loop(ep2.clone(), go2.clone(), cancel.clone())),
        spawn(endpoint_loop(ep3.clone(), go3.clone(), cancel.clone())),
    ];

    debug!("----- adding peers  ----- ");
    let topic: TopicId = blake3::hash(b"foobar").into();

    let addr1 = EndpointAddr::new(pi1).with_relay_url(relay_url.clone());
    let addr2 = EndpointAddr::new(pi2).with_relay_url(relay_url);
    memory_lookup.add_endpoint_info(addr1.clone());
    memory_lookup.add_endpoint_info(addr2.clone());

    debug!("----- joining  ----- ");
    // join the topics and wait for the connection to succeed
    let [sub1, mut sub2, mut sub3] = [
        go1.subscribe_and_join(topic, vec![]),
        go2.subscribe_and_join(topic, vec![pi1]),
        go3.subscribe_and_join(topic, vec![pi2]),
    ]
    .try_join()
    .await
    .unwrap();

    let (sink1, _stream1) = sub1.split();

    let len = 2;

    // publish messages on endpoint1
    let pub1 = spawn(async move {
        for i in 0..len {
            let message = format!("hi{i}");
            info!("go1 broadcast: {message:?}");
            sink1.broadcast(message.into_bytes().into()).await.unwrap();
            tokio::time::sleep(Duration::from_micros(1)).await;
        }
    });

    // wait for messages on endpoint2
    let sub2 = spawn(async move {
        let mut recv = vec![];
        loop {
            let ev = sub2.next().await.unwrap().unwrap();
            info!("go2 event: {ev:?}");
            if let Event::Received(msg) = ev {
                recv.push(msg.content);
            }
            if recv.len() == len {
                return recv;
            }
        }
    });

    // wait for messages on endpoint3
    let sub3 = spawn(async move {
        let mut recv = vec![];
        loop {
            let ev = sub3.next().await.unwrap().unwrap();
            info!("go3 event: {ev:?}");
            if let Event::Received(msg) = ev {
                recv.push(msg.content);
            }
            if recv.len() == len {
                return recv;
            }
        }
    });

    timeout(Duration::from_secs(10), pub1)
        .await
        .unwrap()
        .unwrap();
    let recv2 = timeout(Duration::from_secs(10), sub2)
        .await
        .unwrap()
        .unwrap();
    let recv3 = timeout(Duration::from_secs(10), sub3)
        .await
        .unwrap()
        .unwrap();

    // We assert the received messages, but not their order.
    // While commonly they will be received in-order, for go3 it may happen
    // that the second message arrives before the first one, because it managed to
    // forward-join go1 before the second message is published.
    let expected: HashSet<Bytes> = (0..len)
        .map(|i| Bytes::from(format!("hi{i}").into_bytes()))
        .collect();
    assert_eq!(HashSet::from_iter(recv2), expected);
    assert_eq!(HashSet::from_iter(recv3), expected);

    cancel.cancel();
    for t in tasks {
        timeout(Duration::from_secs(10), t)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

/// Test that when a gossip topic is no longer needed it's actually unsubscribed.
///
/// This test will:
/// - Create two endpoints, the first using manual event loop.
/// - Subscribe both endpoints to the same topic. The first endpoint will subscribe twice and connect
///   to the second endpoint. The second endpoint will subscribe without bootstrap.
/// - Ensure that the first endpoint removes the subscription iff all topic handles have been
///   dropped
// NOTE: this is a regression test.
#[tokio::test]
#[traced_test]
async fn subscription_cleanup() -> Result {
    let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
    let ct = CancellationToken::new();
    let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();

    // create the first endpoint with a manual actor loop
    let (go1, actor, ep1_handle) =
        Gossip::t_new_with_actor(rng, Default::default(), relay_map.clone(), &ct).await?;
    let mut actor = ManualActorLoop::new(actor).await;

    // create the second endpoint with the usual actor loop
    let (go2, ep2, ep2_handle, _test_actor_handle) =
        Gossip::t_new(rng, Default::default(), relay_map, &ct).await?;

    let endpoint_id1 = actor.endpoint.id();
    let endpoint_id2 = ep2.id();
    tracing::info!(
        endpoint_1 = %endpoint_id1.fmt_short(),
        endpoint_2 = %endpoint_id2.fmt_short(),
        "endpoints ready"
    );

    let topic: TopicId = blake3::hash(b"subscription_cleanup").into();
    tracing::info!(%topic, "joining");

    // create the tasks for each gossip instance:
    // - second endpoint subscribes once without bootstrap and listens to events
    // - first endpoint subscribes twice with the second endpoint as bootstrap. This is done on command
    //   from the main task (this)

    // second endpoint
    let ct2 = ct.clone();
    let go2_task = async move {
        let (_pub_tx, mut sub_rx) = go2.subscribe_and_join(topic, vec![]).await?.split();

        let subscribe_fut = async {
            while let Some(ev) = sub_rx.try_next().await? {
                match ev {
                    Event::Lagged => tracing::debug!("missed some messages :("),
                    Event::Received(_) => unreachable!("test does not send messages"),
                    other => tracing::debug!(?other, "gs event"),
                }
            }

            tracing::debug!("subscribe stream ended");
            Ok::<_, AnyError>(())
        };

        tokio::select! {
            _ = ct2.cancelled() => Ok(()),
            res = subscribe_fut => res,
        }
    }
    .instrument(tracing::debug_span!("endpoint_2", %endpoint_id2));
    let go2_handle = task::spawn(go2_task);

    // first endpoint
    let addr2 = EndpointAddr::new(endpoint_id2).with_relay_url(relay_url);
    let memory_lookup = MemoryLookup::new();
    memory_lookup.add_endpoint_info(addr2);
    actor.endpoint.address_lookup()?.add(memory_lookup);
    // we use a channel to signal advancing steps to the task
    let (tx, mut rx) = mpsc::channel::<()>(1);
    let ct1 = ct.clone();
    let go1_task = async move {
        // first subscribe is done immediately
        tracing::info!("subscribing the first time");
        let sub_1a = go1.subscribe_and_join(topic, vec![endpoint_id2]).await?;

        // wait for signal to subscribe a second time
        rx.recv().await.expect("signal for second subscribe");
        tracing::info!("subscribing a second time");
        let sub_1b = go1.subscribe_and_join(topic, vec![endpoint_id2]).await?;
        drop(sub_1a);

        // wait for signal to drop the second handle as well
        rx.recv().await.expect("signal for second subscribe");
        tracing::info!("dropping all handles");
        drop(sub_1b);

        // wait for cancellation
        ct1.cancelled().await;
        drop(go1);

        Ok::<_, AnyError>(())
    }
    .instrument(tracing::debug_span!("endpoint_1", %endpoint_id1));
    let go1_handle = task::spawn(go1_task);

    // advance and check that the topic is now subscribed
    let commands = |actor: &Actor| {
        actor
            .state
            .topics
            .get(&topic)
            .map_or(0, |state| state.command_receivers)
    };
    // Our subscribe, the connection, and the NeighborUp from the peer's information.
    let joined = |actor: &Actor| actor.joined(&topic);
    assert!(actor.steps_until(20, joined).await, "not joined");

    // signal the second subscribe, we should remain subscribed
    tx.send(())
        .await
        .std_context("signal additional subscribe")?;
    assert!(actor.steps_until(20, |a| commands(a) == 2).await);
    // The first handle goes.
    assert!(actor.steps_until(20, |a| commands(a) == 1).await);
    assert!(actor.joined(&topic));

    // signal to drop the second handle, the topic should no longer be subscribed
    tx.send(()).await.std_context("signal drop handles")?;
    assert!(
        actor
            .steps_until(20, |a| !a.state.topics.contains_key(&topic))
            .await,
        "the topic is still subscribed"
    );

    // cleanup and ensure everything went as expected
    ct.cancel();
    let wait = Duration::from_secs(2);
    timeout(wait, ep1_handle)
        .await
        .std_context("wait endpoint1 task")?
        .std_context("join endpoint1 task")??;
    timeout(wait, ep2_handle)
        .await
        .std_context("wait endpoint2 task")?
        .std_context("join endpoint2 task")??;
    timeout(wait, go1_handle)
        .await
        .std_context("wait gossip1 task")?
        .std_context("join gossip1 task")??;
    timeout(wait, go2_handle)
        .await
        .std_context("wait gossip2 task")?
        .std_context("join gossip2 task")??;
    timeout(wait, actor.finish())
        .await
        .std_context("wait actor finish")?;

    Ok(())
}

/// A cancelled task gives `None`, and a finished one its output.
#[tokio::test]
async fn task_output_of_cancelled_and_finished_tasks() {
    let task = task::spawn(std::future::pending::<u32>());
    task.abort();
    assert!(task_output(task.await).is_none());
    assert_eq!(task_output(task::spawn(async { 42u32 }).await), Some(42));
}

/// A panic in a task panics the caller with the same payload.
#[cfg(not(target_family = "wasm"))]
#[tokio::test]
#[should_panic(expected = "original child panic")]
async fn task_output_passes_a_panic_on() {
    task_output(task::spawn(async { panic!("original child panic") }).await);
}

/// `broadcast` refuses a message that does not fit a frame at every hop.
#[tokio::test]
#[traced_test]
async fn broadcast_refuses_an_oversized_message() -> Result {
    const MAX: usize = 1024;
    let endpoint = Endpoint::builder(presets::Minimal)
        .relay_mode(RelayMode::Disabled)
        .bind()
        .await?;
    let gossip = Gossip::builder().max_message_size(MAX).spawn(endpoint);
    let topic = gossip.subscribe([1; 32].into(), vec![]).await?;
    let (sender, _receiver) = topic.split();
    // The limit is the largest content that fits a frame at every hop.
    let fits = (0..MAX)
        .rev()
        .find(|len| proto::topic::broadcast_fits(*len, Scope::Swarm, MAX))
        .expect("some content fits");
    assert_eq!(sender.max_content_size(), fits);

    sender.broadcast(vec![0; fits].into()).await?;
    let refused = sender.broadcast(vec![0; fits + 1].into()).await;
    assert!(
        matches!(refused, Err(Error::TooLarge { size, max_content, .. }) if size == fits + 1 && max_content == fits),
        "{refused:?}"
    );
    sender.broadcast_neighbors(vec![0; fits].into()).await?;
    let refused = sender.broadcast_neighbors(vec![0; fits + 1].into()).await;
    assert!(
        matches!(refused, Err(Error::TooLarge { .. })),
        "{refused:?}"
    );
    Ok(())
}

/// Test that endpoints can reconnect to each other.
///
/// This test will create two endpoints subscribed to the same topic. The second endpoint will
/// unsubscribe and then resubscribe and connection between the endpoints should succeed both
/// times.
// NOTE: This is a regression test
#[tokio::test]
#[traced_test]
async fn can_reconnect() -> Result {
    let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
    let ct = CancellationToken::new();
    let (relay_map, relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();

    let (go1, ep1, ep1_handle, _test_actor_handle1) =
        Gossip::t_new(rng, Default::default(), relay_map.clone(), &ct).await?;

    let (go2, ep2, ep2_handle, _test_actor_handle2) =
        Gossip::t_new(rng, Default::default(), relay_map, &ct).await?;

    let endpoint_id1 = ep1.id();
    let endpoint_id2 = ep2.id();
    tracing::info!(
        endpoint_1 = %endpoint_id1.fmt_short(),
        endpoint_2 = %endpoint_id2.fmt_short(),
        "endpoints ready"
    );

    let topic: TopicId = blake3::hash(b"can_reconnect").into();
    tracing::info!(%topic, "joining");

    let ct2 = ct.child_token();
    // channel used to signal the second gossip instance to advance the test
    let (tx, mut rx) = mpsc::channel::<()>(1);
    let addr1 = EndpointAddr::new(endpoint_id1).with_relay_url(relay_url.clone());
    let memory_lookup = MemoryLookup::new();
    memory_lookup.add_endpoint_info(addr1);
    ep2.address_lookup()?.add(memory_lookup.clone());
    let go2_task = async move {
        let mut sub = go2.subscribe(topic, Vec::new()).await?;
        sub.joined().await?;

        rx.recv().await.expect("signal to unsubscribe");
        tracing::info!("unsubscribing");
        drop(sub);

        rx.recv().await.expect("signal to subscribe again");
        tracing::info!("resubscribing");
        let mut sub = go2.subscribe(topic, vec![endpoint_id1]).await?;

        sub.joined().await?;
        tracing::info!("subscription successful!");

        ct2.cancelled().await;

        Ok::<_, Error>(())
    }
    .instrument(tracing::debug_span!("endpoint_2", %endpoint_id2));
    let go2_handle = task::spawn(go2_task);

    let addr2 = EndpointAddr::new(endpoint_id2).with_relay_url(relay_url);
    memory_lookup.add_endpoint_info(addr2);
    ep1.address_lookup()?.add(memory_lookup);

    let mut sub = go1.subscribe(topic, vec![endpoint_id2]).await?;
    // wait for subscribed notification
    sub.joined().await?;

    // signal endpoint_2 to unsubscribe
    tx.send(()).await.std_context("signal unsubscribe")?;

    // we should receive a Neighbor down event
    let conn_timeout = Duration::from_millis(500);
    let ev = timeout(conn_timeout, sub.try_next())
        .await
        .std_context("wait neighbor down")??;
    assert_eq!(ev, Some(Event::NeighborDown(endpoint_id2)));
    tracing::info!("endpoint 2 left");

    // signal endpoint_2 to subscribe again
    tx.send(()).await.std_context("signal resubscribe")?;

    let conn_timeout = Duration::from_millis(500);
    let ev = timeout(conn_timeout, sub.try_next())
        .await
        .std_context("wait neighbor up")??;
    assert_eq!(ev, Some(Event::NeighborUp(endpoint_id2)));
    tracing::info!("endpoint 2 rejoined!");

    // cleanup and ensure everything went as expected
    ct.cancel();
    let wait = Duration::from_secs(2);
    timeout(wait, ep1_handle)
        .await
        .std_context("wait endpoint1 task")?
        .std_context("join endpoint1 task")??;
    timeout(wait, ep2_handle)
        .await
        .std_context("wait endpoint2 task")?
        .std_context("join endpoint2 task")??;
    timeout(wait, go2_handle)
        .await
        .std_context("wait gossip2 task")?
        .std_context("join gossip2 task")??;

    Result::Ok(())
}

#[tokio::test]
#[traced_test]
async fn can_die_and_reconnect() -> Result {
    /// Runs a future in a separate runtime on its own thread.
    ///
    /// Everything stops at once when `cancel` is cancelled.
    fn run_in_thread<T: Send + 'static>(
        cancel: CancellationToken,
        fut: impl std::future::Future<Output = T> + Send + 'static,
    ) -> std::thread::JoinHandle<Option<T>> {
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move { cancel.run_until_cancelled(fut).await })
        })
    }

    /// Spawns a new endpoint and gossip instance.
    async fn spawn_gossip(
        secret_key: SecretKey,
        relay_map: RelayMap,
    ) -> Result<(Router, Gossip), BindError> {
        let ep = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Custom(relay_map))
            .secret_key(secret_key)
            .ca_tls_config(CaTlsConfig::insecure_skip_verify())
            .bind()
            .await?;
        let gossip = Gossip::builder().spawn(ep.clone());
        let router = Router::builder(ep).accept(ALPN, gossip.clone()).spawn();
        Ok((router, gossip))
    }

    /// Spawns a gossip endpoint, and broadcasts a single message, then sleep until cancelled externally.
    async fn broadcast_once(
        secret_key: SecretKey,
        relay_map: RelayMap,
        bootstrap_addr: EndpointAddr,
        topic_id: TopicId,
        message: String,
    ) -> Result {
        let (router, gossip) = spawn_gossip(secret_key, relay_map).await?;
        info!(endpoint_id = %router.endpoint().id().fmt_short(), "broadcast endpoint spawned");
        let bootstrap = vec![bootstrap_addr.id];
        let memory_lookup = MemoryLookup::new();
        memory_lookup.add_endpoint_info(bootstrap_addr);
        router.endpoint().address_lookup()?.add(memory_lookup);
        let mut topic = gossip.subscribe_and_join(topic_id, bootstrap).await?;
        topic.broadcast(message.as_bytes().to_vec().into()).await?;
        std::future::pending::<()>().await;
        Ok(())
    }

    let (relay_map, _relay_url, _guard) = iroh::test_utils::run_relay_server().await.unwrap();
    let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);
    let topic_id = TopicId::from_bytes(rng.random());

    // spawn a gossip endpoint, send the endpoint's address on addr_tx,
    // then wait to receive `count` messages, and terminate.
    let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();
    let (msgs_recv_tx, mut msgs_recv_rx) = tokio::sync::mpsc::channel(3);
    let recv_task = tokio::task::spawn({
        let relay_map = relay_map.clone();
        let secret_key = SecretKey::from_bytes(&rng.random());
        async move {
            let (router, gossip) = spawn_gossip(secret_key, relay_map).await?;
            // wait for the relay to be set. iroh currently has issues when trying
            // to immediately reconnect with changed direct addresses, but when the
            // relay path is available it works.
            // See https://github.com/n0-computer/iroh/pull/3372
            router.endpoint().online().await;
            let addr = router.endpoint().addr();
            info!(endpoint_id = %addr.id.fmt_short(), "recv endpoint spawned");
            addr_tx.send(addr).unwrap();
            let mut topic = gossip.subscribe_and_join(topic_id, vec![]).await?;
            while let Some(event) = topic.try_next().await.unwrap() {
                if let Event::Received(message) = event {
                    let message = std::str::from_utf8(&message.content)
                        .std_context("decode broadcast message")?
                        .to_string();
                    msgs_recv_tx
                        .send(message)
                        .await
                        .std_context("forward received message")?;
                }
            }
            Ok::<_, AnyError>(())
        }
    });

    let endpoint0_addr = addr_rx.await.std_context("receive endpoint address")?;
    let max_wait = Duration::from_secs(5);

    // spawn a endpoint, send a message, and then abruptly terminate the endpoint ungracefully
    // after the message was received on our receiver endpoint.
    let cancel = CancellationToken::new();
    let secret = SecretKey::from_bytes(&rng.random());
    let join_handle_1 = run_in_thread(
        cancel.clone(),
        broadcast_once(
            secret.clone(),
            relay_map.clone(),
            endpoint0_addr.clone(),
            topic_id,
            "msg1".to_string(),
        ),
    );
    // assert that we received the message on the receiver endpoint.
    let msg = timeout(max_wait, msgs_recv_rx.recv())
        .await
        .std_context("wait for first broadcast")?
        .std_context("receiver dropped channel")?;
    assert_eq!(&msg, "msg1");
    info!("kill broadcast endpoint");
    cancel.cancel();

    // spawns the endpoint again with the same endpoint id, and send another message
    let cancel = CancellationToken::new();
    let join_handle_2 = run_in_thread(
        cancel.clone(),
        broadcast_once(
            secret.clone(),
            relay_map.clone(),
            endpoint0_addr.clone(),
            topic_id,
            "msg2".to_string(),
        ),
    );
    // assert that we received the message on the receiver endpoint.
    // this means that the reconnect with the same endpoint id worked.
    let msg = timeout(max_wait, msgs_recv_rx.recv())
        .await
        .std_context("wait for second broadcast")?
        .std_context("receiver dropped channel")?;
    assert_eq!(&msg, "msg2");
    info!("kill broadcast endpoint");
    cancel.cancel();

    info!("kill recv endpoint");
    recv_task.abort();
    assert!(join_handle_1.join().unwrap().is_none());
    assert!(join_handle_2.join().unwrap().is_none());

    Ok(())
}

#[tokio::test]
#[traced_test]
async fn gossip_change_alpn() -> n0_error::Result<()> {
    let alpn = b"my-gossip-alpn";
    let topic_id = TopicId::from([0u8; 32]);

    let ep1 = Endpoint::bind(presets::Minimal).await?;
    let ep2 = Endpoint::bind(presets::Minimal).await?;
    let gossip1 = Gossip::builder().alpn(alpn).spawn(ep1.clone());
    let gossip2 = Gossip::builder().alpn(alpn).spawn(ep2.clone());
    let router1 = Router::builder(ep1).accept(alpn, gossip1.clone()).spawn();
    let router2 = Router::builder(ep2).accept(alpn, gossip2.clone()).spawn();

    let addr1 = router1.endpoint().addr();
    let id1 = addr1.id;
    let memory_lookup = MemoryLookup::new();
    memory_lookup.add_endpoint_info(addr1);
    router2.endpoint().address_lookup()?.add(memory_lookup);

    let mut topic1 = gossip1.subscribe(topic_id, vec![]).await?;
    let mut topic2 = gossip2.subscribe(topic_id, vec![id1]).await?;

    timeout(Duration::from_secs(3), topic1.joined())
        .await
        .std_context("wait topic1 join")??;
    timeout(Duration::from_secs(3), topic2.joined())
        .await
        .std_context("wait topic2 join")??;
    router1.shutdown().await.std_context("shutdown router1")?;
    router2.shutdown().await.std_context("shutdown router2")?;
    Ok(())
}

#[tokio::test]
#[traced_test]
async fn gossip_rely_on_gossip_address_lookup() -> n0_error::Result<()> {
    let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(1);

    async fn spawn(
        rng: &mut impl CryptoRng,
    ) -> n0_error::Result<(EndpointId, Router, Gossip, GossipSender, GossipReceiver)> {
        let topic_id = TopicId::from([0u8; 32]);
        let ep = Endpoint::builder(presets::Minimal)
            .secret_key(SecretKey::from_bytes(&rng.random()))
            .bind()
            .await?;
        let endpoint_id = ep.id();
        let gossip = Gossip::builder().spawn(ep.clone());
        let router = Router::builder(ep).accept(ALPN, gossip.clone()).spawn();
        let topic = gossip.subscribe(topic_id, vec![]).await?;
        let (sender, receiver) = topic.split();
        Ok((endpoint_id, router, gossip, sender, receiver))
    }

    // spawn 3 endpoints without relay or address lookup
    let (n1, r1, _g1, _tx1, mut rx1) = spawn(rng).await?;
    let (n2, r2, _g2, tx2, mut rx2) = spawn(rng).await?;
    let (n3, r3, _g3, tx3, mut rx3) = spawn(rng).await?;

    println!("endpoints {:?}", [n1, n2, n3]);

    // create a mem lookup that has only endpoint 1 addr info set
    let addr1 = r1.endpoint().addr();
    let lookup = MemoryLookup::new();
    lookup.add_endpoint_info(addr1);

    // add addr info of endpoint1 to endpoint2 and join endpoint1
    r2.endpoint().address_lookup()?.add(lookup.clone());
    tx2.join_peers(vec![n1]).await?;

    // await join endpoint2 -> nodde1
    timeout(Duration::from_secs(3), rx1.joined())
        .await
        .std_context("wait rx1 join")??;
    timeout(Duration::from_secs(3), rx2.joined())
        .await
        .std_context("wait rx2 join")??;

    // add addr info of endpoint1 to endpoint3 and join endpoint1
    r3.endpoint().address_lookup()?.add(lookup.clone());
    tx3.join_peers(vec![n1]).await?;

    // await join at endpoint3: n1 and n2
    // n2 only works because because we use gossip address lookup!
    let ev = timeout(Duration::from_secs(3), rx3.next())
        .await
        .std_context("wait rx3 first neighbor")?;
    assert!(matches!(ev, Some(Ok(Event::NeighborUp(_)))));
    let ev = timeout(Duration::from_secs(3), rx3.next())
        .await
        .std_context("wait rx3 second neighbor")?;
    assert!(matches!(ev, Some(Ok(Event::NeighborUp(_)))));

    assert_eq!(sorted(rx3.neighbors()), sorted([n1, n2]));

    let ev = timeout(Duration::from_secs(3), rx2.next())
        .await
        .std_context("wait rx2 neighbor")?;
    assert!(matches!(ev, Some(Ok(Event::NeighborUp(n))) if n == n3));

    let ev = timeout(Duration::from_secs(3), rx1.next())
        .await
        .std_context("wait rx1 neighbor")?;
    assert!(matches!(ev, Some(Ok(Event::NeighborUp(n))) if n == n3));

    tokio::try_join!(r1.shutdown(), r2.shutdown(), r3.shutdown())
        .std_context("shutdown routers")?;
    Ok(())
}

fn sorted<T: Ord>(input: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut out: Vec<_> = input.into_iter().collect();
    out.sort();
    out
}

/// Dropping the sender does not close the topic while the receiver still listens.
///
/// This is a common footgun: users split a GossipTopic, drop the sender early,
/// and expect the receiver to keep working. With the bug (using || in is_empty),
/// the topic closes immediately when sender is dropped.
#[tokio::test]
#[traced_test]
async fn topic_stays_alive_after_sender_drop() -> n0_error::Result<()> {
    let topic_id = TopicId::from([99u8; 32]);

    let ep1 = Endpoint::bind(presets::Minimal).await?;
    let ep2 = Endpoint::bind(presets::Minimal).await?;
    let gossip1 = Gossip::builder().spawn(ep1.clone());
    let gossip2 = Gossip::builder().spawn(ep2.clone());
    let router1 = Router::builder(ep1)
        .accept(crate::ALPN, gossip1.clone())
        .spawn();
    let router2 = Router::builder(ep2)
        .accept(crate::ALPN, gossip2.clone())
        .spawn();

    let addr1 = router1.endpoint().addr();
    let id1 = addr1.id;
    let mem_lookup = MemoryLookup::new();
    mem_lookup.add_endpoint_info(addr1);
    router2.endpoint().address_lookup()?.add(mem_lookup);

    let topic1 = gossip1.subscribe(topic_id, vec![]).await?;
    let topic2 = gossip2.subscribe(topic_id, vec![id1]).await?;

    let (tx1, mut rx1) = topic1.split();
    let (tx2, mut rx2) = topic2.split();

    // Wait for mesh to form
    timeout(Duration::from_secs(3), rx1.joined())
        .await
        .std_context("wait rx1 join")??;
    timeout(Duration::from_secs(3), rx2.joined())
        .await
        .std_context("wait rx2 join")??;

    // Node 1 drops its sender - simulating the footgun where user drops sender early
    drop(tx1);

    // Node 2 sends a message - receiver on node 1 should still get it
    tx2.broadcast(b"hello from node2".to_vec().into()).await?;

    // Node 1's receiver should still work and receive the message
    let event = timeout(Duration::from_secs(3), rx1.next())
        .await
        .std_context("wait for message on rx1")?;

    match event {
        Some(Ok(Event::Received(msg))) => {
            assert_eq!(&msg.content[..], b"hello from node2");
        }
        other => panic!("expected Received event, got {:?}", other),
    }

    drop(tx2);
    drop(rx1);
    drop(rx2);
    router1.shutdown().await.std_context("shutdown router1")?;
    router2.shutdown().await.std_context("shutdown router2")?;
    Ok(())
}
