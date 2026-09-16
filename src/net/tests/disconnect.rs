use super::*;

struct Fixture {
    actor: Actor,
    endpoint: Endpoint,
    peer: EndpointId,
    topic: TopicId,
    events: broadcast::Receiver<ProtoEvent>,
    _rpc_tx: mpsc::Sender<RpcMessage>,
    _local_tx: mpsc::Sender<LocalActorMessage>,
}

impl Fixture {
    async fn new() -> Self {
        let endpoint = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .clear_ip_transports()
            .bind_addr("127.0.0.1:0")
            .unwrap()
            .alpns(vec![GOSSIP_ALPN.to_vec()])
            .bind()
            .await
            .unwrap();
        let (mut actor, rpc_tx, local_tx) = Actor::new(
            endpoint.clone(),
            Default::default(),
            Arc::new(Metrics::default()),
            None,
            GossipAddressLookup::default(),
        );
        let topic = TopicId::from_bytes([3u8; 32]);
        let topic_state = TopicState::default();
        let events = topic_state.event_sender.subscribe();
        actor.topics.insert(topic, topic_state);
        actor
            .handle_in_event(
                InEvent::Command(topic, ProtoCommand::Join(Vec::new())),
                Instant::now(),
            )
            .await;
        Self {
            actor,
            endpoint,
            peer: SecretKey::from_bytes(&[2u8; 32]).public(),
            topic,
            events,
            _rpc_tx: rpc_tx,
            _local_tx: local_tx,
        }
    }

    fn activate_sender(&mut self, conn_id: ConnId) -> mpsc::Receiver<ProtoMessage> {
        let (active_send_tx, receiver) = mpsc::channel(SEND_QUEUE_CAP);
        self.actor.peers.insert(
            self.peer,
            PeerState::Active {
                active_send_tx,
                active_conn_id: conn_id,
                active_origin: ConnOrigin::Dial,
                other_conns: Vec::new(),
            },
        );
        receiver
    }

    async fn receive_join(&mut self, conn_id: ConnId) {
        self.actor
            .in_event_tx
            .send(ConnectionMessage {
                peer_id: self.peer,
                conn_id,
                message: test_proto_message(),
            })
            .await
            .unwrap();
        assert!(
            self.actor
                .event_loop(&mut n0_future::stream::pending(), 0)
                .await
        );
    }

    async fn broadcast(&mut self, content: impl Into<Bytes>) {
        self.actor
            .handle_in_event(
                InEvent::Command(
                    self.topic,
                    ProtoCommand::Broadcast(content.into(), Scope::Swarm),
                ),
                Instant::now(),
            )
            .await;
    }

    async fn shutdown(mut self) {
        self.actor.shutdown_tasks().await;
        drop(self.actor);
        self.endpoint.close().await;
    }
}

#[tokio::test]
async fn failed_active_send_retires_membership_before_more_broadcasts() {
    let mut fixture = Fixture::new().await;
    let mut old_outbound = fixture.activate_sender(7);
    fixture.receive_join(7).await;
    assert_eq!(
        fixture.events.try_recv().unwrap(),
        ProtoEvent::NeighborUp(fixture.peer)
    );
    while old_outbound.try_recv().is_ok() {}
    drop(old_outbound);

    fixture.broadcast(Bytes::from_static(b"send-fails")).await;
    assert_eq!(
        fixture.events.try_recv().unwrap(),
        ProtoEvent::NeighborDown(fixture.peer)
    );
    assert!(!fixture.actor.peers.contains_key(&fixture.peer));
    assert!(!fixture.actor.topics[&fixture.topic]
        .neighbors
        .contains(&fixture.peer));
    for seq in 0..12 {
        fixture
            .broadcast(Bytes::from(format!("offline-{seq}")))
            .await;
    }
    assert!(!fixture.actor.peers.contains_key(&fixture.peer));
    assert!(fixture.actor.dialer.pending_dials.is_empty());
    assert!(
        fixture.events.try_recv().is_err(),
        "retirement is reported once"
    );

    let mut outbound = fixture.activate_sender(8);
    fixture.receive_join(8).await;
    assert_eq!(
        fixture.events.try_recv().unwrap(),
        ProtoEvent::NeighborUp(fixture.peer)
    );
    while outbound.try_recv().is_ok() {}
    fixture.broadcast(Bytes::from_static(b"after-rejoin")).await;
    let message = outbound
        .try_recv()
        .expect("a fresh broadcast must use the new sender");
    let mut peer_protocol = proto::State::new(
        fixture.peer,
        PeerData::default(),
        proto::Config::default(),
        rand::rngs::ChaCha12Rng::seed_from_u64(5),
    );
    peer_protocol
        .handle(
            InEvent::Command(fixture.topic, ProtoCommand::Join(Vec::new())),
            Instant::now(),
            None,
        )
        .for_each(drop);
    let delivered: Vec<_> = peer_protocol
        .handle(
            InEvent::RecvMessage(fixture.endpoint.id(), message),
            Instant::now(),
            None,
        )
        .filter_map(|event| match event {
            OutEvent::EmitEvent(_, ProtoEvent::Received(message)) => Some(message.content),
            _ => None,
        })
        .collect();
    assert_eq!(delivered, vec![Bytes::from_static(b"after-rejoin")]);
    assert!(
        outbound.try_recv().is_err(),
        "the new sender must not inherit offline payloads"
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn failed_first_join_keeps_its_pending_dial_intent() {
    let mut fixture = Fixture::new().await;
    drop(fixture.activate_sender(7));
    fixture
        .actor
        .handle_in_event(
            InEvent::Command(fixture.topic, ProtoCommand::Join(vec![fixture.peer])),
            Instant::now(),
        )
        .await;
    let Some(PeerState::Pending {
        queue,
        dial_ownership,
    }) = fixture.actor.peers.get(&fixture.peer)
    else {
        panic!("a first Join must remain pending until its connection succeeds");
    };
    assert_eq!(queue.len(), 1);
    assert_eq!(*dial_ownership, DialOwnership::Actor);
    assert_eq!(fixture.actor.dialer.pending_dials.len(), 1);
    assert!(fixture
        .actor
        .dialer
        .pending_dials
        .contains_key(&fixture.peer));
    assert!(fixture.events.try_recv().is_err());
    fixture.shutdown().await;
}

#[tokio::test]
async fn disconnect_preserves_external_ownership_but_discards_retired_messages() {
    let mut fixture = Fixture::new().await;
    let _outbound = fixture.activate_sender(7);
    fixture.receive_join(7).await;
    fixture.events.try_recv().unwrap();
    fixture.actor.peers.insert(
        fixture.peer,
        PeerState::Pending {
            queue: vec![test_proto_message()],
            dial_ownership: DialOwnership::External,
        },
    );

    fixture
        .actor
        .handle_in_event(InEvent::PeerDisconnected(fixture.peer), Instant::now())
        .await;
    assert_eq!(
        fixture.events.try_recv().unwrap(),
        ProtoEvent::NeighborDown(fixture.peer)
    );
    assert!(
        matches!(fixture.actor.peers.get(&fixture.peer), Some(PeerState::Pending {
        queue, dial_ownership: DialOwnership::External,
    }) if queue.is_empty())
    );
    fixture
        .actor
        .handle_in_event(
            InEvent::Command(fixture.topic, ProtoCommand::Join(vec![fixture.peer])),
            Instant::now(),
        )
        .await;
    assert!(
        matches!(fixture.actor.peers.get(&fixture.peer), Some(PeerState::Pending {
        queue, dial_ownership: DialOwnership::External,
    }) if queue.len() == 1)
    );
    assert!(fixture.actor.dialer.pending_dials.is_empty());
    fixture.shutdown().await;
}

#[tokio::test]
async fn queued_input_from_a_failed_sender_cannot_restore_retired_membership() {
    let mut fixture = Fixture::new().await;
    let old_outbound = fixture.activate_sender(7);
    fixture.receive_join(7).await;
    fixture.events.try_recv().unwrap();
    fixture
        .actor
        .in_event_tx
        .send(ConnectionMessage {
            peer_id: fixture.peer,
            conn_id: 7,
            message: test_proto_message(),
        })
        .await
        .unwrap();
    drop(old_outbound);
    fixture
        .broadcast(Bytes::from_static(b"detect-failed-sender"))
        .await;
    fixture.events.try_recv().unwrap();

    assert!(
        fixture
            .actor
            .event_loop(&mut n0_future::stream::pending(), 0)
            .await
    );
    assert!(!fixture.actor.peers.contains_key(&fixture.peer));
    assert!(!fixture.actor.topics[&fixture.topic]
        .neighbors
        .contains(&fixture.peer));
    assert!(fixture.events.try_recv().is_err());
    fixture.shutdown().await;
}

#[tokio::test]
async fn queued_neighbor_reply_survives_sender_handoff() {
    let mut fixture = Fixture::new().await;
    let _old_outbound = fixture.activate_sender(7);
    let mut peer_protocol = proto::State::new(
        fixture.peer,
        PeerData::default(),
        proto::Config::default(),
        rand::rngs::ChaCha12Rng::seed_from_u64(6),
    );
    peer_protocol
        .handle(
            InEvent::Command(fixture.topic, ProtoCommand::Join(Vec::new())),
            Instant::now(),
            None,
        )
        .for_each(drop);
    let neighbor_reply = peer_protocol
        .handle(
            InEvent::RecvMessage(fixture.endpoint.id(), test_proto_message()),
            Instant::now(),
            None,
        )
        .find_map(|event| match event {
            OutEvent::SendMessage(target, message) if target == fixture.endpoint.id() => {
                Some(message)
            }
            _ => None,
        })
        .expect("a Join must produce its Neighbor reply");
    fixture
        .actor
        .in_event_tx
        .send(ConnectionMessage {
            peer_id: fixture.peer,
            conn_id: 7,
            message: neighbor_reply,
        })
        .await
        .unwrap();

    // The actor can consume a preferred connection handoff before this queued
    // reply because local_rx has priority over in_event_rx.
    let (new_sender, mut new_outbound) = mpsc::channel(SEND_QUEUE_CAP);
    let admission = fixture
        .actor
        .peers
        .get_mut(&fixture.peer)
        .unwrap()
        .admit_conn(new_sender, 8, ConnOrigin::Accept, ConnOrigin::Accept);
    assert!(matches!(
        admission,
        PeerConnectionAdmission::Activate { .. }
    ));
    assert!(
        fixture
            .actor
            .event_loop(&mut n0_future::stream::pending(), 0)
            .await
    );
    assert_eq!(
        fixture.events.try_recv().unwrap(),
        ProtoEvent::NeighborUp(fixture.peer)
    );
    let reply = new_outbound
        .try_recv()
        .expect("the handshake response must use the replacement sender");
    assert!(peer_protocol
        .handle(
            InEvent::RecvMessage(fixture.endpoint.id(), reply),
            Instant::now(),
            None,
        )
        .all(|event| !matches!(event, OutEvent::SendMessage(_, _))));
    fixture.shutdown().await;
}

#[tokio::test]
async fn closed_handoff_connection_input_is_rejected_while_current_input_still_works() {
    let mut fixture = Fixture::new().await;
    let _old_outbound = fixture.activate_sender(7);
    let (new_sender, _new_outbound) = mpsc::channel(SEND_QUEUE_CAP);
    let admission = fixture
        .actor
        .peers
        .get_mut(&fixture.peer)
        .unwrap()
        .admit_conn(new_sender, 8, ConnOrigin::Dial, ConnOrigin::Dial);
    assert!(matches!(
        admission,
        PeerConnectionAdmission::Activate { .. }
    ));
    assert!(matches!(
        fixture
            .actor
            .peers
            .get_mut(&fixture.peer)
            .unwrap()
            .connection_closed(7),
        PeerConnectionClose::Other {
            remaining_connections: 1,
        }
    ));

    fixture.receive_join(7).await;
    assert!(
        fixture.events.try_recv().is_err(),
        "a completed connection must no longer restore membership"
    );
    fixture.receive_join(8).await;
    assert_eq!(
        fixture.events.try_recv().unwrap(),
        ProtoEvent::NeighborUp(fixture.peer)
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn retired_handoff_connections_cannot_restore_membership_after_reconnect() {
    let mut fixture = Fixture::new().await;
    let _old_outbound = fixture.activate_sender(7);
    fixture.receive_join(7).await;
    fixture.events.try_recv().unwrap();
    let (new_sender, _new_outbound) = mpsc::channel(SEND_QUEUE_CAP);
    let admission = fixture
        .actor
        .peers
        .get_mut(&fixture.peer)
        .unwrap()
        .admit_conn(new_sender, 8, ConnOrigin::Dial, ConnOrigin::Dial);
    assert!(matches!(
        admission,
        PeerConnectionAdmission::Activate { .. }
    ));

    fixture
        .actor
        .handle_in_event(InEvent::PeerDisconnected(fixture.peer), Instant::now())
        .await;
    assert_eq!(
        fixture.events.try_recv().unwrap(),
        ProtoEvent::NeighborDown(fixture.peer)
    );
    assert!(!fixture.actor.peers.contains_key(&fixture.peer));
    let _reconnected_outbound = fixture.activate_sender(9);
    for retired_conn_id in [7, 8] {
        fixture.receive_join(retired_conn_id).await;
        assert!(fixture.events.try_recv().is_err());
        assert!(!fixture.actor.topics[&fixture.topic]
            .neighbors
            .contains(&fixture.peer));
    }
    fixture.receive_join(9).await;
    assert_eq!(
        fixture.events.try_recv().unwrap(),
        ProtoEvent::NeighborUp(fixture.peer)
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn closed_sender_input_is_rejected_before_the_connection_task_is_reaped() {
    let mut fixture = Fixture::new().await;
    drop(fixture.activate_sender(7));
    fixture.receive_join(7).await;
    assert!(fixture.events.try_recv().is_err());
    assert!(!fixture.actor.topics[&fixture.topic]
        .neighbors
        .contains(&fixture.peer));
    fixture.shutdown().await;
}

#[tokio::test]
async fn leaving_all_topics_cannot_queue_offline_broadcasts_for_replay() {
    let mut fixture = Fixture::new().await;
    let second_topic = TopicId::from_bytes([4; 32]);
    let second_state = TopicState::default();
    let mut second_events = second_state.event_sender.subscribe();
    fixture.actor.topics.insert(second_topic, second_state);
    fixture
        .actor
        .handle_in_event(
            InEvent::Command(second_topic, ProtoCommand::Join(Vec::new())),
            Instant::now(),
        )
        .await;
    let topics = [fixture.topic, second_topic];
    let mut outbound = fixture.activate_sender(7);
    let mut peer_protocol = proto::State::new(
        fixture.peer,
        PeerData::default(),
        proto::Config::default(),
        rand::rngs::ChaCha12Rng::seed_from_u64(8),
    );
    for topic in topics {
        peer_protocol
            .handle(
                InEvent::Command(topic, ProtoCommand::Join(Vec::new())),
                Instant::now(),
                None,
            )
            .for_each(drop);
        let mut join = test_proto_message();
        join.topic = topic;
        let replies: Vec<_> = peer_protocol
            .handle(
                InEvent::RecvMessage(fixture.endpoint.id(), join),
                Instant::now(),
                None,
            )
            .filter_map(|event| match event {
                OutEvent::SendMessage(target, message) if target == fixture.endpoint.id() => {
                    Some(message)
                }
                _ => None,
            })
            .collect();
        for reply in replies {
            fixture
                .actor
                .handle_in_event(InEvent::RecvMessage(fixture.peer, reply), Instant::now())
                .await;
        }
        while let Ok(reply) = outbound.try_recv() {
            peer_protocol
                .handle(
                    InEvent::RecvMessage(fixture.endpoint.id(), reply),
                    Instant::now(),
                    None,
                )
                .for_each(drop);
        }
    }
    assert_eq!(
        fixture.events.try_recv().unwrap(),
        ProtoEvent::NeighborUp(fixture.peer)
    );
    assert_eq!(
        second_events.try_recv().unwrap(),
        ProtoEvent::NeighborUp(fixture.peer)
    );

    // Deliver both topics' final frames through the same registered connection.
    // Closing it after the first topic would fence the second topic's Disconnect
    // while leaving that topic's eager membership available to new broadcasts.
    for topic in topics {
        let messages: Vec<_> = peer_protocol
            .handle(
                InEvent::Command(topic, ProtoCommand::Quit),
                Instant::now(),
                None,
            )
            .filter_map(|event| match event {
                OutEvent::SendMessage(target, message) if target == fixture.endpoint.id() => {
                    Some(message)
                }
                _ => None,
            })
            .collect();
        assert!(messages
            .iter()
            .any(|message| message.message.is_disconnect()));
        for message in messages {
            fixture
                .actor
                .in_event_tx
                .send(ConnectionMessage {
                    peer_id: fixture.peer,
                    conn_id: 7,
                    message,
                })
                .await
                .unwrap();
            assert!(
                fixture
                    .actor
                    .event_loop(&mut n0_future::stream::pending(), 0)
                    .await
            );
        }
    }
    for seq in 0..12 {
        fixture
            .actor
            .handle_in_event(
                InEvent::Command(
                    second_topic,
                    ProtoCommand::Broadcast(Bytes::from(format!("offline-{seq}")), Scope::Swarm),
                ),
                Instant::now(),
            )
            .await;
    }
    assert!(
        !fixture.actor.peers.contains_key(&fixture.peer),
        "fully retired topic memberships must not create a Pending replay queue"
    );
    assert!(fixture.actor.dialer.pending_dials.is_empty());
    assert_eq!(
        fixture.events.try_recv().unwrap(),
        ProtoEvent::NeighborDown(fixture.peer)
    );
    assert_eq!(
        second_events.try_recv().unwrap(),
        ProtoEvent::NeighborDown(fixture.peer)
    );

    let mut reconnected_outbound = fixture.activate_sender(8);
    for topic in topics {
        let mut join = test_proto_message();
        join.topic = topic;
        fixture
            .actor
            .handle_in_event(InEvent::RecvMessage(fixture.peer, join), Instant::now())
            .await;
    }
    while let Ok(message) = reconnected_outbound.try_recv() {
        assert!(
            matches!(message.kind(), proto::state::MessageKind::Control),
            "rejoining must not replay offline broadcasts"
        );
    }
    fixture
        .actor
        .handle_in_event(
            InEvent::Command(
                second_topic,
                ProtoCommand::Broadcast(Bytes::from_static(b"after-rejoin"), Scope::Swarm),
            ),
            Instant::now(),
        )
        .await;
    let fresh = reconnected_outbound.try_recv().unwrap();
    assert_eq!(fresh.topic, second_topic);
    assert!(matches!(fresh.kind(), proto::state::MessageKind::Data));
    fixture.shutdown().await;
}
