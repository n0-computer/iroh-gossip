use super::*;
use crate::proto::topic::{Message as TopicMessage, OutEvent as TopicOutEvent};

#[test]
fn neighbor_down_discards_only_its_pending_lazy_announcements() {
    let mut state = State::new(1u32, Config::default(), 4096);
    let mut io: VecDeque<TopicOutEvent<u32>> = VecDeque::new();
    let now = Instant::now();
    for peer in [2, 3] {
        state.handle(InEvent::NeighborUp(peer), now, &mut io);
        state.handle(InEvent::RecvMessage(peer, Message::Prune), now, &mut io);
    }
    for seq in 0..12 {
        state.handle(
            InEvent::Broadcast(Bytes::from(format!("queued-{seq}")), Scope::Swarm),
            now,
            &mut io,
        );
    }

    state.handle(InEvent::NeighborDown(2), now, &mut io);
    state.handle(InEvent::NeighborUp(2), now, &mut io);
    io.clear();
    state.handle(InEvent::TimerExpired(Timer::DispatchLazyPush), now, &mut io);

    let mut advertised = 0;
    for event in io.drain(..) {
        let TopicOutEvent::SendMessage(peer, TopicMessage::Gossip(Message::IHave(ihaves))) = event
        else {
            panic!("lazy dispatch must only send its retained announcements");
        };
        assert_eq!(peer, 3, "a new membership must not inherit old lazy work");
        advertised += ihaves.len();
    }
    assert_eq!(advertised, 12, "another live peer keeps its queued work");

    let content = Bytes::from_static(b"queued-0");
    state.handle(
        InEvent::RecvMessage(
            3,
            Message::Graft(Graft {
                id: Some(MessageId::from_content(&content)),
                round: Round(0),
            }),
        ),
        now,
        &mut io,
    );
    assert!(
        matches!(io.pop_front(), Some(TopicOutEvent::SendMessage(3, TopicMessage::Gossip(Message::Gossip(message))))
        if message.content == content)
    );
    assert!(
        io.is_empty(),
        "live peers must still recover cached messages"
    );
}

#[test]
fn neighbor_up_does_not_replay_broadcasts_from_the_disconnected_interval() {
    let mut state = State::new(1u32, Config::default(), 4096);
    let mut io: VecDeque<TopicOutEvent<u32>> = VecDeque::new();
    let now = Instant::now();
    state.handle(InEvent::NeighborUp(2), now, &mut io);
    state.handle(InEvent::NeighborDown(2), now, &mut io);
    for seq in 0..12 {
        state.handle(
            InEvent::Broadcast(Bytes::from(format!("offline-{seq}")), Scope::Swarm),
            now,
            &mut io,
        );
    }
    assert!(!io
        .iter()
        .any(|event| matches!(event, TopicOutEvent::SendMessage(2, _))));
    io.clear();

    state.handle(InEvent::NeighborUp(2), now, &mut io);
    state.handle(InEvent::TimerExpired(Timer::DispatchLazyPush), now, &mut io);
    assert!(io.is_empty(), "rejoining only enables future broadcasts");

    let fresh = Bytes::from_static(b"after-rejoin");
    state.handle(
        InEvent::Broadcast(fresh.clone(), Scope::Swarm),
        now,
        &mut io,
    );
    let delivered: Vec<_> = io
        .into_iter()
        .filter_map(|event| match event {
            TopicOutEvent::SendMessage(2, TopicMessage::Gossip(Message::Gossip(message))) => {
                Some(message.content)
            }
            _ => None,
        })
        .collect();
    assert_eq!(delivered, vec![fresh]);
}

#[test]
fn neighbor_down_removes_stale_graft_sources_and_preserves_live_sources() {
    let mut state = State::new(1u32, Config::default(), 4096);
    let mut io: VecDeque<TopicOutEvent<u32>> = VecDeque::new();
    let now = Instant::now();
    let id = MessageId::from_content(b"missing");
    for peer in [2, 3] {
        state.handle(InEvent::NeighborUp(peer), now, &mut io);
        state.handle(
            InEvent::RecvMessage(
                peer,
                Message::IHave(vec![IHave {
                    id,
                    round: Round(0),
                }]),
            ),
            now,
            &mut io,
        );
    }
    state.handle(InEvent::NeighborDown(2), now, &mut io);
    io.clear();
    state.handle(InEvent::TimerExpired(Timer::SendGraft(id)), now, &mut io);
    assert!(io.iter().any(|event| matches!(event,
        TopicOutEvent::SendMessage(3, TopicMessage::Gossip(Message::Graft(graft))) if graft.id == Some(id))));
    assert!(!io
        .iter()
        .any(|event| matches!(event, TopicOutEvent::SendMessage(2, _))));

    state.handle(InEvent::NeighborDown(3), now, &mut io);
    io.clear();
    state.handle(InEvent::TimerExpired(Timer::SendGraft(id)), now, &mut io);
    assert!(
        io.is_empty(),
        "a retired graft timer cannot recreate an old connection"
    );
}
