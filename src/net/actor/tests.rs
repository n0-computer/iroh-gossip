use std::{
    any::Any,
    collections::{BTreeSet, HashSet},
    time::Duration,
};

use bytes::Bytes;
use futures_concurrency::future::TryJoin;
use iroh::{
    address_lookup::memory::MemoryLookup,
    endpoint::{
        presets, BindError, Connection, QuicTransportConfig, RecvStream, SendStream, VarInt,
    },
    protocol::Router,
    tls::CaTlsConfig,
    EndpointAddr, EndpointId, RelayMap, RelayMode, SecretKey,
};
use n0_error::{AnyError, Result, StdResultExt};
use n0_future::time::Instant;
use n0_tracing_test::traced_test;
use rand::{rngs::StdRng, CryptoRng, RngExt, SeedableRng};
use tokio::{spawn, sync::oneshot, time::timeout};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, instrument};

use super::*;
use crate::{
    api::{self, Command, Event, GossipReceiver, GossipSender},
    net::{
        conn::{
            self, connection_loop, task_output, ConnArgs, ConnEvent, ConnEventKind, ConnExit,
            ConnShared, Mailbox, TakeQueue,
        },
        peers::{ConnEnd, PeerState, MAX_ACCEPTED_PER_PEER},
        queue::{Pushed, QueueReader, SendQueue},
        subscriptions::TopicHandles,
        wire::{self, CloseCode, ControlMessage, Session},
        *,
    },
    proto::{hyparview, plumtree, PeerData, Scope, TopicId},
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
        let max_message_size = actor.proto.max_message_size();

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
    wire::StreamHeader::write(topic, &mut stream, &mut buffer)
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
    let mut buffer = Vec::new();
    let header = wire::StreamHeader::read(&mut stream, &mut buffer)
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

/// Reads the rest of `stream` in the background.
///
/// A reader that drops an unfinished stream stops it, which fails the connection of the writer.
fn drain(mut stream: RecvStream) {
    spawn(async move {
        stream.read_to_end(usize::MAX).await.ok();
    });
}

/// Accepts the peer's next stream and reads its first message.
async fn read_next_message(conn: &Connection) -> Result<(TopicId, TopicMessage)> {
    let mut stream = conn.accept_uni().await.std_context("accept stream")?;
    let mut buffer = Vec::new();
    let header = wire::StreamHeader::read(&mut stream, &mut buffer)
        .await
        .std_context("read header")?;
    let message = wire::read_message(&mut stream, &mut buffer, TEST_FRAME_LIMIT)
        .await
        .std_context("read frame")?
        .std_context("the stream ended without a message")?;
    drain(stream);
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
async fn closed_with(conn: &Connection, code: CloseCode) -> Result {
    let reason = timeout(Duration::from_secs(2), conn.closed())
        .await
        .std_context("the connection stayed open")?;
    assert!(
        matches!(
            &reason,
            iroh::endpoint::ConnectionError::ApplicationClosed(close)
                if close.error_code == code.code()
        ),
        "{reason:?}"
    );
    Ok(())
}

/// The session id that raw test peers send.
const PEER_SESSION: Session = Session([9; 8]);

/// Opens the control stream as the dialer of `conn`, and writes the header of a raw test peer.
async fn control_open(conn: &Connection) -> Result<(SendStream, RecvStream)> {
    control_open_with(conn, PEER_SESSION).await
}

async fn control_open_with(
    conn: &Connection,
    session: Session,
) -> Result<(SendStream, RecvStream)> {
    let (mut send, recv) = conn.open_bi().await.std_context("open control")?;
    send.write_all(&session.to_frame())
        .await
        .std_context("write control header")?;
    Ok((send, recv))
}

/// Accepts the control stream as the acceptor of `conn`, reads the header, and answers with ours.
async fn control_accept(conn: &Connection) -> Result<(SendStream, RecvStream, Session)> {
    let (mut send, mut recv) = conn.accept_bi().await.std_context("accept control")?;
    let session = Session::read(&mut recv, &mut Vec::new())
        .await
        .std_context("read control header")?;
    send.write_all(&PEER_SESSION.to_frame())
        .await
        .std_context("write control header")?;
    Ok((send, recv, session))
}

/// Reads the next control message of a raw test peer.
async fn read_control(recv: &mut RecvStream) -> Result<ControlMessage> {
    timeout(
        Duration::from_secs(10),
        ControlMessage::read(recv, &mut Vec::new(), TEST_FRAME_LIMIT),
    )
    .await
    .std_context("no control message")?
    .std_context("read control message")
}

/// The control stream of a raw test peer, after the handshake.
///
/// `ours` is the connection of the node under test, and `theirs` is the connection of the raw peer.
struct RawControl {
    send: SendStream,
    recv: RecvStream,
}

impl RawControl {
    /// Plays the acceptor: waits for the control stream of the node, and answers.
    async fn accept(theirs: &Connection) -> Result<Self> {
        let (send, recv, _) = control_accept(theirs).await?;
        Ok(Self { send, recv })
    }

    /// Plays the dialer: opens the control stream, and reads the header of the node.
    async fn open(theirs: &Connection) -> Result<Self> {
        let (send, mut recv) = control_open(theirs).await?;
        Session::read(&mut recv, &mut Vec::new())
            .await
            .std_context("read control header")?;
        Ok(Self { send, recv })
    }

    async fn write(&mut self, message: ControlMessage) -> Result {
        self.send
            .write_all(&message.to_frame())
            .await
            .std_context("write control message")
    }

    async fn read(&mut self) -> Result<ControlMessage> {
        read_control(&mut self.recv).await
    }

    /// Finishes our half: we read all that the node wrote.
    fn finish(&mut self) -> Result {
        self.send.finish().std_context("finish control")
    }
}

/// A connection task on its own, without an actor.
struct TestConn {
    events: mpsc::Receiver<ConnEvent>,
    retire: CancellationToken,
    queue: Arc<Mailbox<TakeQueue>>,
    aborted: CancellationToken,
    shutdown: CancellationToken,
    task: AbortOnDropHandle<ConnExit>,
}

/// Runs `connection_loop` for `conn`.
fn start_conn(conn: Connection, max_message_size: usize, retire_after: Duration) -> TestConn {
    let (events_tx, events) = mpsc::channel::<ConnEvent>(64);
    let queue = Arc::new(Mailbox::default());
    let (aborted, retire, shutdown) = (
        CancellationToken::new(),
        CancellationToken::new(),
        CancellationToken::new(),
    );
    let shared = ConnShared {
        events: events_tx,
        max_message_size,
        session: Session([1; 8]),
        retire_after,
        shutdown: shutdown.clone(),
    };
    let args = ConnArgs {
        id: 0,
        conn,
        aborted: aborted.clone(),
        retire: retire.clone(),
        take_queue: queue.clone(),
        shared,
    };
    TestConn {
        events,
        retire,
        queue,
        aborted,
        shutdown,
        task: AbortOnDropHandle::new(spawn(connection_loop(args))),
    }
}

impl TestConn {
    /// Moves `reader` to the writer, and gives it the topics that get a stream at once.
    fn take_queue(&self, reader: QueueReader, topics: Vec<TopicId>) {
        self.queue.put(TakeQueue { reader, topics });
    }

    /// Returns the next message that the receive loop dispatches.
    async fn next(&mut self) -> Result<ProtoMessage> {
        loop {
            let event = timeout(Duration::from_secs(5), self.events.recv())
                .await
                .std_context("no message")?
                .std_context("the connection task ended")?;
            if let ConnEventKind::Message(message) = event.kind {
                return Ok(message);
            }
        }
    }

    /// Returns whether a message arrives within `wait`.
    async fn gets_message_within(&mut self, wait: Duration) -> bool {
        timeout(wait, self.next())
            .await
            .is_ok_and(|res| res.is_ok())
    }
}

/// Runs `connection_loop` for `conn`, and plays the control stream of the raw peer `theirs`.
///
/// The peer of a dialer accepts the control stream, and the peer of an acceptor opens it.
async fn start_conn_peered(
    conn: Connection,
    theirs: &Connection,
    max_message_size: usize,
) -> Result<(TestConn, RawControl)> {
    let dialer = conn.side().is_client();
    let test = start_conn(conn, max_message_size, Duration::from_secs(60));
    let control = match dialer {
        true => RawControl::accept(theirs).await?,
        false => RawControl::open(theirs).await?,
    };
    Ok((test, control))
}

/// A dialed test connection that writes a queue, with a raw acceptor on the other side.
struct Writer {
    test: TestConn,
    _control: RawControl,
}

async fn start_writer(
    conn: Connection,
    theirs: &Connection,
    reader: QueueReader,
    max_message_size: usize,
    topics: Vec<TopicId>,
) -> Result<Writer> {
    let (test, control) = start_conn_peered(conn, theirs, max_message_size).await?;
    test.take_queue(reader, topics);
    Ok(Writer {
        test,
        _control: control,
    })
}

/// Handles the events of the connection tasks until `done` holds, or fails after 5 seconds.
async fn pump_until(
    actor: &mut ManualActorLoop,
    what: &str,
    done: impl Fn(&Actor) -> bool,
) -> Result {
    timeout(Duration::from_secs(5), async {
        while !done(actor) {
            tokio::select! {
                _ = actor.step() => {}
                _ = tokio::time::sleep(Duration::from_millis(20)) => {}
            }
        }
    })
    .await
    .with_std_context(|_| format!("timed out: {what}"))
}

/// Returns a join for `topic` carrying `data_len` bytes of peer data.
///
/// The protocol state builds it, as the message types are private to `proto`.
pub(in crate::net) fn join_message(topic: TopicId, data_len: usize) -> ProtoMessage {
    let me = SecretKey::generate().public();
    let peer = SecretKey::generate().public();
    let mut state = proto::State::new(
        me,
        PeerData::new(vec![0u8; data_len]),
        Default::default(),
        StdRng::seed_from_u64(1),
    );
    let join = InEvent::Command(topic, proto::Command::Join(vec![peer]));
    let mut out = Vec::new();
    state.handle(join, Instant::now(), &mut out);
    let message = out.into_iter().find_map(|event| match event {
        proto::state::tests::OutEvent::SendMessage(_, message) => Some(message),
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
    actor.state.topics.insert(topic, TopicHandles::default());
    actor.handle_in_event(
        InEvent::Command(topic, ProtoCommand::Join(vec![])),
        Instant::now(),
    );
    let commands = commands_of(vec![Command::Broadcast(Bytes::new()); 4096]);
    actor.command_rx.push(super::next_command(topic, commands));
    actor
        .state
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

/// Returns how a connection task ended, or fails after `wait`.
async fn ended(task: AbortOnDropHandle<ConnExit>, wait: Duration) -> Result<ConnEnd> {
    Ok(exited(task, wait).await?.end)
}

/// Returns the result of a connection task, or fails after `wait`.
async fn exited(task: AbortOnDropHandle<ConnExit>, wait: Duration) -> Result<ConnExit> {
    timeout(wait, task)
        .await
        .std_context("the connection did not end")?
        .std_context("connection task")
}

/// Waits until `conn` closed, and returns the code of an application close.
async fn close_code(conn: &Connection, wait: Duration) -> Result<Option<CloseCode>> {
    let reason = timeout(wait, conn.closed())
        .await
        .std_context("the connection stayed open")?;
    Ok(match reason {
        iroh::endpoint::ConnectionError::ApplicationClosed(close) => {
            CloseCode::from_code(close.error_code)
        }
        _ => None,
    })
}

const SECS_10: Duration = Duration::from_secs(10);
