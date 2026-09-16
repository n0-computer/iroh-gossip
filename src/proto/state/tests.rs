use bytes::Bytes;
use rand::{rngs::ChaCha12Rng, SeedableRng};

use super::*;
use crate::proto::{hyparview, Event, Scope};

const PEER: u32 = 2;
const TOPICS: [TopicId; 2] = [TopicId([1; 32]), TopicId([2; 32])];

fn connected_topics() -> State<u32, ChaCha12Rng> {
    let mut state = State::new(
        1,
        PeerData::default(),
        Config::default(),
        ChaCha12Rng::seed_from_u64(7),
    );
    for topic in TOPICS {
        state
            .handle(
                InEvent::Command(topic, Command::Join(Vec::new())),
                Instant::now(),
                None,
            )
            .for_each(drop);
        let events: Vec<_> = state
            .handle(
                InEvent::RecvMessage(
                    PEER,
                    Message {
                        topic,
                        message: topic::Message::Swarm(hyparview::Message::Join(None)),
                    },
                ),
                Instant::now(),
                None,
            )
            .collect();
        assert!(events.iter().any(|event| matches!(event,
            OutEvent::EmitEvent(event_topic, Event::NeighborUp(PEER)) if *event_topic == topic)));
    }
    state
}

#[test]
fn quitting_one_topic_keeps_the_shared_peer_live_for_other_topics() {
    let mut state = connected_topics();
    let [quitting, remaining] = TOPICS;
    let events: Vec<_> = state
        .handle(
            InEvent::Command(quitting, Command::Quit),
            Instant::now(),
            None,
        )
        .collect();
    assert!(events.iter().any(|event| matches!(event,
        OutEvent::SendMessage(PEER, message)
        if message.topic == quitting && message.message.is_disconnect())));
    assert!(!events
        .iter()
        .any(|event| matches!(event, OutEvent::DisconnectPeer(PEER))));
    assert!(state.state(&quitting).is_none());
    assert!(state.has_active_peers(&remaining));

    let messages: Vec<_> = state
        .handle(
            InEvent::Command(
                remaining,
                Command::Broadcast(Bytes::from_static(b"remaining-topic"), Scope::Swarm),
            ),
            Instant::now(),
            None,
        )
        .filter_map(|event| match event {
            OutEvent::SendMessage(PEER, message) => Some(message),
            _ => None,
        })
        .collect();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].topic, remaining);
    assert!(matches!(messages[0].kind(), MessageKind::Data));
}

#[test]
fn quitting_the_last_topic_disconnects_the_shared_peer_once() {
    let mut state = connected_topics();
    state
        .handle(
            InEvent::Command(TOPICS[0], Command::Quit),
            Instant::now(),
            None,
        )
        .for_each(drop);
    let events: Vec<_> = state
        .handle(
            InEvent::Command(TOPICS[1], Command::Quit),
            Instant::now(),
            None,
        )
        .collect();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, OutEvent::DisconnectPeer(PEER)))
            .count(),
        1
    );
    let disconnect_message = events
        .iter()
        .position(|event| {
            matches!(event,
            OutEvent::SendMessage(PEER, message) if message.message.is_disconnect())
        })
        .expect("the final topic must send its disconnect before closing the peer");
    let disconnect_peer = events
        .iter()
        .position(|event| matches!(event, OutEvent::DisconnectPeer(PEER)))
        .unwrap();
    assert!(disconnect_message < disconnect_peer);
    assert!(state.topics().next().is_none());
    assert!(state
        .handle(
            InEvent::Command(TOPICS[1], Command::Quit),
            Instant::now(),
            None
        )
        .next()
        .is_none());
}

#[test]
fn network_disconnect_retires_every_topic_before_future_broadcasts() {
    let mut state = connected_topics();
    let events: Vec<_> = state
        .handle(InEvent::PeerDisconnected(PEER), Instant::now(), None)
        .collect();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, OutEvent::DisconnectPeer(PEER)))
            .count(),
        1
    );
    for topic in TOPICS {
        assert!(!state.has_active_peers(&topic));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event,
                OutEvent::EmitEvent(event_topic, Event::NeighborDown(PEER))
                if *event_topic == topic))
                .count(),
            1
        );
        assert!(state
            .handle(
                InEvent::Command(
                    topic,
                    Command::Broadcast(Bytes::from_static(b"offline"), Scope::Swarm),
                ),
                Instant::now(),
                None,
            )
            .all(|event| !matches!(event, OutEvent::SendMessage(PEER, _))));
    }
    assert!(state
        .handle(InEvent::PeerDisconnected(PEER), Instant::now(), None)
        .all(|event| !matches!(event, OutEvent::DisconnectPeer(PEER))));
}
