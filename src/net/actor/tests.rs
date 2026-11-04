use std::{
    any::Any,
    collections::{BTreeSet, HashMap, HashSet},
    time::Duration,
};

use bytes::{Bytes, BytesMut};
use futures_concurrency::future::TryJoin;
use iroh::{
    address_lookup::memory::MemoryLookup,
    endpoint::{presets, BindError, Connection, QuicTransportConfig, Side, VarInt},
    protocol::Router,
    tls::CaTlsConfig,
    EndpointAddr, EndpointId, RelayMap, RelayMode, SecretKey,
};
use n0_error::{AnyError, Result, StdResultExt};
use n0_future::{task::JoinSet, time::Instant};
use n0_tracing_test::traced_test;
use rand::{rngs::StdRng, CryptoRng, RngExt, SeedableRng};
use tokio::{spawn, sync::watch, time::timeout};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, instrument};

use super::*;
use crate::{
    api::{self, Command, Event, GossipReceiver, GossipSender},
    net::{
        conn::{self, connection_loop, retire, task_output, RecvLoop, SendLoop, RETIRE_GRACE},
        peers::{ConnId, Finished, PeerState, PEER_RETENTION},
        queue::{Pushed, QueueLease, SendQueue},
        subscriptions::{
            all_eager_peers_busy, SubscriptionCommands, TopicHandles, BROADCAST_RECHECK_INTERVAL,
            BROADCAST_WAIT_LIMIT,
        },
        wire, *,
    },
    proto::{PeerData, Scope, TopicId},
    Error,
};

mod dials;
mod direct;
mod gossip;
mod retire;
mod scheduling;
mod shutdown;
mod streams;
mod writes;

struct ManualActorLoop {
    actor: Actor,
    step: usize,
}

impl std::ops::Deref for ManualActorLoop {
    type Target = Actor;

    fn deref(&self) -> &Self::Target {
        &self.actor
    }
}

impl std::ops::DerefMut for ManualActorLoop {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.actor
    }
}

type EndpointHandle = tokio::task::JoinHandle<Result<()>>;

impl ManualActorLoop {
    #[instrument(skip_all, fields(me = %actor.endpoint.id().fmt_short()))]
    async fn new(mut actor: Actor) -> Self {
        let _ = actor.setup();
        Self { actor, step: 0 }
    }

    #[instrument(skip_all, fields(me = %self.endpoint.id().fmt_short()))]
    async fn step(&mut self) -> bool {
        let ManualActorLoop { actor, step } = self;
        *step += 1;
        // ignore updates that change our published address. This gives us better control over
        // events since the endpoint it no longer emitting changes
        let addr_update_stream = &mut n0_future::stream::pending();
        actor.event_loop(addr_update_stream).await
    }

    /// Steps until `done` holds, at most `max` times, and returns whether it holds.
    ///
    /// The actor picks its branches in no fixed order, so a count of steps can
    /// miss an event.
    async fn steps_until(&mut self, max: usize, done: impl Fn(&Actor) -> bool) -> bool {
        for _ in 0..max {
            if done(&self.actor) {
                return true;
            }
            self.step().await;
        }
        done(&self.actor)
    }

    async fn finish(mut self) {
        while self.step().await {}
    }
}

impl Gossip {
    /// Creates a testing gossip instance and its actor without spawning it.
    ///
    /// This creates the endpoint and spawns the endpoint loop as well. The handle for the
    /// endpoing task is returned along the gossip instance and actor. Since the actor is not
    /// actually spawned as [`Builder::spawn`] would, the gossip instance will have a
    /// handle to a dummy task instead.
    async fn t_new_with_actor(
        rng: &mut rand::rngs::ChaCha12Rng,
        config: proto::Config,
        relay_map: RelayMap,
        cancel: &CancellationToken,
    ) -> Result<(Self, Actor, EndpointHandle), BindError> {
        let endpoint = create_endpoint(rng, relay_map, None).await?;
        let metrics = Arc::new(Metrics::default());
        let address_lookup = GossipAddressLookup::default();
        endpoint
            .address_lookup()
            .expect("endpoint is not closed")
            .add(address_lookup.clone());

        let (actor, to_actor) = Actor::new(
            endpoint,
            ActorConfig {
                proto: config,
                ..Default::default()
            },
            metrics.clone(),
            address_lookup,
        );
        let max_message_size = actor.state.max_message_size();

        let actor_handle = AbortOnDropHandle::new(task::spawn(n0_future::future::pending()));
        let gossip = Self::new(to_actor, actor_handle, max_message_size, metrics);

        let endpoint_task = task::spawn(endpoint_loop(
            actor.endpoint.clone(),
            gossip.clone(),
            cancel.child_token(),
        ));

        Ok((gossip, actor, endpoint_task))
    }

    /// Crates a new testing gossip instance with the normal actor loop.
    async fn t_new(
        rng: &mut rand::rngs::ChaCha12Rng,
        config: proto::Config,
        relay_map: RelayMap,
        cancel: &CancellationToken,
    ) -> Result<(Self, Endpoint, EndpointHandle, impl Drop + use<>), BindError> {
        let (g, actor, ep_handle) =
            Gossip::t_new_with_actor(rng, config, relay_map, cancel).await?;
        let ep = actor.endpoint.clone();
        let me = ep.id().fmt_short();
        let actor_handle = task::spawn(actor.run().instrument(tracing::error_span!("gossip", %me)));
        Ok((g, ep, ep_handle, AbortOnDropHandle::new(actor_handle)))
    }
}

pub(crate) async fn create_endpoint(
    rng: &mut rand::rngs::ChaCha12Rng,
    relay_map: RelayMap,
    memory_lookup: Option<MemoryLookup>,
) -> Result<Endpoint, BindError> {
    let ep = Endpoint::builder(presets::Minimal)
        .relay_mode(RelayMode::Custom(relay_map))
        .secret_key(SecretKey::from_bytes(&rng.random()))
        .alpns(vec![GOSSIP_ALPN.to_vec()])
        .ca_tls_config(CaTlsConfig::insecure_skip_verify())
        .bind()
        .await?;

    if let Some(memory_lookup) = memory_lookup {
        ep.address_lookup()
            .expect("endpoint is not closed")
            .add(memory_lookup);
    }
    ep.online().await;
    Ok(ep)
}

async fn endpoint_loop(
    endpoint: Endpoint,
    gossip: Gossip,
    cancel: CancellationToken,
) -> Result<()> {
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            incoming = endpoint.accept() => match incoming {
                None => break,
                Some(incoming) => {
                    let connecting = match incoming.accept() {
                        Ok(connecting) => connecting,
                        Err(err) => {
                            warn!("incoming connection failed: {err:#}");
                            // we can carry on in these cases:
                            // this can be caused by retransmitted datagrams
                            continue;
                        }
                    };
                    let connection = connecting
                        .await
                        .std_context("await incoming connection")?;
                    gossip.handle_connection(connection).await?
                }
            }
        }
    }
    Ok(())
}

/// A message of the topic layer, as it goes over the wire.
type TopicMessage = proto::topic::Message<PublicKey>;

/// The frame size limit when tests read or write raw streams.
const TEST_FRAME_LIMIT: usize = 1 << 16;

/// Two endpoints on one test relay, the second reachable by its address.
struct TestPair {
    ep1: Endpoint,
    ep2: Endpoint,
    ep1_addr: EndpointAddr,
    ep2_addr: EndpointAddr,
    _relay: Box<dyn Any + Send>,
}

impl TestPair {
    async fn new(seed: u64) -> Result<Self> {
        let rng = &mut rand::rngs::ChaCha12Rng::seed_from_u64(seed);
        let (relay_map, relay_url, relay) = iroh::test_utils::run_relay_server()
            .await
            .std_context("relay")?;
        let ep1 = create_endpoint(rng, relay_map.clone(), None).await?;
        let ep2 = create_endpoint(rng, relay_map, None).await?;
        let ep1_addr = EndpointAddr::new(ep1.id()).with_relay_url(relay_url.clone());
        let ep2_addr = EndpointAddr::new(ep2.id()).with_relay_url(relay_url);
        Ok(Self {
            ep1,
            ep2,
            ep1_addr,
            ep2_addr,
            _relay: Box::new(relay),
        })
    }

    /// Connects the first endpoint to the second, returning both sides.
    async fn connect(&self) -> Result<(Connection, Connection)> {
        let ours = async {
            self.ep1
                .connect(self.ep2_addr.clone(), GOSSIP_ALPN)
                .await
                .std_context("connect")
        };
        let theirs = async {
            let incoming = self.ep2.accept().await.std_context("endpoint closed")?;
            incoming.await.std_context("accept")
        };
        (ours, theirs).try_join().await
    }

    /// Connects the second endpoint to the first, returning the first's side first.
    async fn connect_back(&self) -> Result<(Connection, Connection)> {
        let ours = async {
            let incoming = self.ep1.accept().await.std_context("endpoint closed")?;
            incoming.await.std_context("accept")
        };
        let theirs = async {
            self.ep2
                .connect(self.ep1_addr.clone(), GOSSIP_ALPN)
                .await
                .std_context("connect")
        };
        (ours, theirs).try_join().await
    }
}

/// Opens a stream for `topic` and writes `messages` to it, leaving it open.
async fn open_stream(
    conn: &Connection,
    topic: TopicId,
    messages: &[proto::topic::Message<PublicKey>],
) -> Result<iroh::endpoint::SendStream> {
    let mut stream = conn.open_uni().await.std_context("open stream")?;
    let mut buffer = Vec::new();
    wire::StreamHeader::write(topic, &mut stream, &mut buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("write header")?;
    for message in messages {
        wire::write_message(&mut stream, message, &mut buffer, TEST_FRAME_LIMIT)
            .await
            .std_context("write frame")?;
    }
    Ok(stream)
}

/// Accepts the peer's next stream and reads it to its end.
async fn read_next_stream(conn: &Connection) -> Result<(TopicId, Vec<TopicMessage>)> {
    let mut stream = conn.accept_uni().await.std_context("accept stream")?;
    let mut buffer = BytesMut::new();
    let header = wire::StreamHeader::read(&mut stream, &mut buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("read header")?;
    let mut messages = Vec::new();
    while let Some(message) = wire::read_message(&mut stream, &mut buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("read frame")?
    {
        messages.push(message);
    }
    Ok((header.topic, messages))
}

/// Returns a `Disconnect` for `topic`.
fn disconnect_message(topic: TopicId) -> ProtoMessage {
    ProtoMessage {
        topic,
        message: TopicMessage::test_disconnect(),
    }
}

/// Accepts the peer's next stream and reads its first message.
async fn read_next_message(conn: &Connection) -> Result<(TopicId, TopicMessage)> {
    let mut stream = conn.accept_uni().await.std_context("accept stream")?;
    let mut buffer = BytesMut::new();
    let header = wire::StreamHeader::read(&mut stream, &mut buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("read header")?;
    let message = wire::read_message(&mut stream, &mut buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("read frame")?
        .std_context("the stream ended without a message")?;
    Ok((header.topic, message))
}

/// Builds an actor without running its event loop.
async fn t_actor() -> Result<Actor, BindError> {
    let endpoint = Endpoint::builder(presets::Minimal)
        .relay_mode(RelayMode::Disabled)
        .bind()
        .await?;
    let metrics = Arc::new(Metrics::default());
    let (actor, _to_actor) = Actor::new(
        endpoint,
        ActorConfig {
            proto: Default::default(),
            ..Default::default()
        },
        metrics,
        Default::default(),
    );
    Ok(actor)
}

/// Waits until `conn` closed, and checks that our side closed it with `code`.
async fn closed_with(conn: &Connection, code: u32) -> Result {
    let reason = timeout(Duration::from_secs(2), conn.closed())
        .await
        .std_context("the connection stayed open")?;
    assert!(
        matches!(
            &reason,
            iroh::endpoint::ConnectionError::ApplicationClosed(close)
                if close.error_code == VarInt::from_u32(code)
        ),
        "{reason:?}"
    );
    Ok(())
}

fn unset_flag() -> watch::Sender<bool> {
    watch::channel(false).0
}

/// Opens a stream with our header, as a new node does on each stream.
///
/// The header tells the other side that the peer handles a close.
async fn show_that_it_handles_a_close(conn: &Connection, topic: TopicId) {
    let mut stream = conn.open_uni().await.expect("open stream");
    wire::StreamHeader::write(topic, &mut stream, &mut Vec::new(), TEST_FRAME_LIMIT)
        .await
        .expect("write header");
    stream.finish().expect("finish");
}

/// Joins new topics with `peer` until its queue had no room for a control message.
fn join_until_full(actor: &mut Actor, peer: EndpointId) {
    for i in 0..10_000u32 {
        let topic: TopicId = blake3::hash(&i.to_be_bytes()).into();
        let join = InEvent::Command(topic, ProtoCommand::Join(vec![peer]));
        actor.handle_in_event(join, Instant::now());
        if actor.metrics.peers_send_queue_full.get() > 0 {
            return;
        }
    }
}

/// Returns the queue of `peer_id`, which must have an active connection.
fn active_queue(actor: &Actor, peer_id: EndpointId) -> &SendQueue {
    let peer = &actor.peers[&peer_id];
    if peer.queue.is_closed() || peer.writer.is_none() {
        panic!("not active: {:?}, writer: {:?}", peer.queue, peer.writer);
    }
    &peer.queue
}

/// Pushes `message` to the queue of the active peer `peer_id`.
fn push_active(actor: &Actor, peer_id: EndpointId, message: ProtoMessage) {
    let pushed = active_queue(actor, peer_id).push(message);
    assert!(matches!(pushed, Pushed::Queued));
}

/// Pushes `count` payloads of distinct sizes, which may fill the windows of a connection.
fn push_many(actor: &Actor, peer_id: EndpointId, topic: TopicId, count: usize) {
    for i in 0..count {
        push_active(actor, peer_id, numbered(topic, i));
    }
}

/// Returns the payload number `i`, which differs from the others in its size.
///
/// Up to 3000 of them fit the frame limit of the actor.
fn numbered(topic: TopicId, i: usize) -> ProtoMessage {
    ProtoMessage {
        topic,
        message: TopicMessage::test_gossip(1000 + i),
    }
}

/// Returns the writer of the queue of `peer_id`, and whether the queue is draining.
fn send_state(actor: &Actor, peer_id: EndpointId) -> Option<(Option<ConnId>, bool)> {
    let peer = actor.peers.get(&peer_id)?;
    Some((peer.writer, peer.queue.is_closed()))
}

/// Whether the peer has no messages and its queue is closed.
fn is_idle(peer: &PeerState) -> bool {
    peer.queue.is_closed() && peer.queue.is_empty()
}

/// Returns the state of a peer with an open, empty queue of `size` bytes.
fn open_peer(size: usize) -> PeerState {
    let mut peer = PeerState::new(size);
    peer.open_queue();
    peer
}

/// Returns the state of a peer that we dial, with `messages` in its queue.
fn pending_with(messages: Vec<ProtoMessage>) -> PeerState {
    let state = open_peer(queue::QUEUE_SIZE);
    for message in messages {
        assert!(matches!(state.queue.push(message), Pushed::Queued));
    }
    state
}

/// Returns a join for `topic` carrying `data_len` bytes of peer data.
///
/// The protocol state builds it, as the message types are private to `proto`.
fn join_message(topic: TopicId, data_len: usize) -> ProtoMessage {
    let me = SecretKey::generate().public();
    let peer = SecretKey::generate().public();
    let mut state = proto::State::new(
        me,
        PeerData::new(vec![0u8; data_len]),
        Default::default(),
        StdRng::seed_from_u64(1),
    );
    let join = InEvent::Command(topic, proto::Command::Join(vec![peer]));
    let message = state
        .handle(join, Instant::now(), None)
        .find_map(|event| match event {
            proto::OutEvent::SendMessage(_, message) => Some(message),
            _ => None,
        });
    message.expect("a join sends a message")
}

/// Returns a receiver that yields `commands`, then ends.
fn commands_of(commands: Vec<Command>) -> mpsc::Receiver<Command> {
    let (tx, rx) = mpsc::channel(commands.len().max(1));
    for command in commands {
        tx.try_send(command).expect("fits the capacity");
    }
    rx
}

/// Joins a topic with a publisher that always has a command ready.
fn publish_forever(actor: &mut Actor) {
    let topic = [1; 32].into();
    actor.topics.insert(topic, TopicHandles::default());
    actor.handle_in_event(
        InEvent::Command(topic, ProtoCommand::Join(vec![])),
        Instant::now(),
    );
    let commands = commands_of(vec![Command::Broadcast(Bytes::new()); 4096]);
    actor.command_rx.insert(topic, commands);
    actor
        .topics
        .get_mut(&topic)
        .expect("inserted above")
        .command_receivers += 1;
}

/// Binds an endpoint without a relay, and adds its address to `lookup`.
async fn direct_endpoint(
    lookup: &MemoryLookup,
    transport: QuicTransportConfig,
) -> Result<Endpoint> {
    let endpoint = Endpoint::builder(presets::Minimal)
        .relay_mode(RelayMode::Disabled)
        .alpns(vec![GOSSIP_ALPN.to_vec()])
        .transport_config(transport)
        .bind()
        .await?;
    endpoint.address_lookup()?.add(lookup.clone());
    lookup.set_endpoint_info(endpoint.addr());
    Ok(endpoint)
}
