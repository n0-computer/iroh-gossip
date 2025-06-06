use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    sync::Arc,
};

#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Context;
use bytes::BytesMut;
use iroh::{watchable::Watcher, NodeId};
use irpc::channel::spsc;
use n0_future::{time::Instant, StreamExt};
use rand::rngs::StdRng;
use tokio::{
    sync::{
        broadcast,
        mpsc::{self, error::TrySendError},
    },
    task::{AbortHandle, JoinSet},
};
use tracing::{debug, error_span, instrument, trace, warn, Instrument};

use crate::{
    api,
    proto::{self, PeerData, TopicId, DEFAULT_MAX_MESSAGE_SIZE},
};

use super::{
    connections::{ConnectionPool, RemoteRecvStream},
    util::{read_message, write_message, Timers, WriteError},
};

pub(super) type SubscribeChannels = (spsc::Sender<api::Event>, spsc::Receiver<api::Command>);
type CommandReceiverStream = n0_future::stream::Boxed<api::Command>;

const NEIGHBOR_RECV_CAPACITY: usize = 16;
const TO_TOPIC_CAP: usize = 16;

///
#[derive(Debug, Clone)]
pub(crate) struct Config {
    ///
    pub(crate) peer_channel_cap: usize,
    ///
    pub(crate) event_channel_cap: usize,
    ///
    pub(crate) proto: proto::Config,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            peer_channel_cap: 256,
            event_channel_cap: 256,
            proto: Default::default(),
        }
    }
}

#[derive(derive_more::Debug)]
pub(crate) enum ToTopic {
    Subscribe(#[debug("Channels")] SubscribeChannels),
}

type Message = proto::topic::Message<NodeId>;
type Event = proto::topic::Event<NodeId>;
type Timer = proto::topic::Timer<NodeId>;
type ProtoEvent = proto::topic::Event<NodeId>;
type OutEvent = proto::topic::OutEvent<NodeId>;
type InEvent = proto::topic::InEvent<NodeId>;
type Command = proto::topic::Command<NodeId>;

type OnPeerData = Arc<dyn Fn(NodeId, PeerData) + Send + Sync>;

#[derive(Debug)]
pub(crate) struct TopicHandle {
    tx: mpsc::Sender<ToTopic>,
    #[cfg(test)]
    joined: Arc<AtomicBool>,
    // handle: AbortOnDropHandle<()>,
    handle: AbortHandle,
    queue: Vec<ToTopic>,
}

impl TopicHandle {
    pub(crate) fn spawn_into(
        join_set: &mut JoinSet<Topic>,
        me: NodeId,
        topic_id: TopicId,
        config: Arc<Config>,
        on_peer_data: OnPeerData,
        peer_data_updates: Watcher<PeerData>,
        connection_pool: ConnectionPool,
    ) -> Self {
        let (to_topic_tx, to_topic_rx) = mpsc::channel(TO_TOPIC_CAP);
        let topic = Topic::new(
            me,
            topic_id,
            config,
            to_topic_rx,
            on_peer_data,
            peer_data_updates,
            connection_pool,
        );
        #[cfg(test)]
        let joined = topic.joined.clone();

        let handle = join_set.spawn(
            topic
                .run()
                .instrument(error_span!("topic", topic=%topic_id.fmt_short())),
        );

        Self {
            tx: to_topic_tx,
            #[cfg(test)]
            joined,
            handle,
            queue: Vec::new(),
        }
    }
    // pub(crate) fn spawn(
    //     me: NodeId,
    //     topic_id: TopicId,
    //     config: Arc<Config>,
    //     from_topic_tx: mpsc::Sender<FromTopic>,
    //     on_peer_data: OnPeerData,
    //     peer_data_updates: Watcher<PeerData>,
    //     connection_pool: ConnectionPool,
    // ) -> TopicHandle {
    //     let (to_topic_tx, to_topic_rx) = mpsc::channel(TO_TOPIC_CAP);
    //     let topic = Topic::new(
    //         me,
    //         topic_id,
    //         config,
    //         to_topic_rx,
    //         from_topic_tx,
    //         on_peer_data,
    //         peer_data_updates,
    //         connection_pool,
    //     );
    //     #[cfg(test)]
    //     let joined = topic.joined.clone();
    //     let handle = topic.spawn();
    //     TopicHandle {
    //         tx: to_topic_tx,
    //         #[cfg(test)]
    //         joined,
    //         handle,
    //         queue: Vec::new(),
    //     }
    // }

    pub(crate) async fn maybe_respawn_into(
        mut self,
        join_set: &mut JoinSet<Topic>,
        mut topic: Topic,
    ) -> Option<Self> {
        debug!("maybe_respawn");
        let mut queue = vec![];
        while let Ok(msg) = topic.to_topic_rx.try_recv() {
            queue.push(msg);
        }
        debug!("queue from state {}", queue.len());
        queue.extend(self.queue.drain(..));
        debug!("queue from handle {}", queue.len());
        if !queue.is_empty() {
            debug!("respawn!");
            let (to_topic_tx, to_topic_rx) = mpsc::channel(TO_TOPIC_CAP);
            topic.reset(to_topic_rx);
            let id = topic.id();
            self.handle = join_set.spawn(
                topic
                    .run()
                    .instrument(error_span!("topic", topic=%id.fmt_short())),
            );
            self.tx = to_topic_tx;
            for msg in queue.drain(..) {
                self.send(msg).await;
            }
            Some(self)
        } else {
            None
        }
    }

    pub(crate) async fn send(&mut self, msg: ToTopic) {
        if let Err(err) = self.tx.send(msg).await {
            self.queue.push(err.0);
        }
    }

    #[cfg(test)]
    pub(crate) fn joined(&self) -> bool {
        self.joined.load(Ordering::SeqCst)
    }
}

#[derive(derive_more::Debug)]
pub(crate) struct Topic {
    id: TopicId,

    state: crate::proto::topic::State<NodeId, StdRng>,
    out_events: VecDeque<OutEvent>,

    timers: Timers<Timer>,

    neighbors: BTreeSet<NodeId>,
    peers: HashMap<NodeId, PeerState>,
    peer_send_tasks: JoinSet<(NodeId, anyhow::Result<()>)>,
    peer_recv_tasks: JoinSet<NodeId>,
    peer_recv_rx: mpsc::Receiver<(NodeId, Message)>,
    peer_recv_tx: mpsc::Sender<(NodeId, Message)>,

    accept_rx: mpsc::Receiver<RemoteRecvStream>,

    config: Arc<Config>,
    closed: bool,

    event_sender: broadcast::Sender<Event>,
    #[debug("MergeUnbounded<CommandReceiverStream>")]
    command_receivers: n0_future::MergeUnbounded<CommandReceiverStream>,
    event_send_tasks: JoinSet<()>,

    to_topic_rx: mpsc::Receiver<ToTopic>,
    #[cfg(test)]
    joined: Arc<AtomicBool>,
    #[debug(skip)]
    on_peer_data: OnPeerData,
    peer_data_updates: Watcher<PeerData>,
    init: bool,
    connection_pool: ConnectionPool,
}

#[derive(Debug, Default)]
struct PeerState {
    send_tasks: usize,
    recv_tasks: Vec<AbortHandle>,
    sender: Option<mpsc::Sender<Message>>,
}

impl Topic {
    pub fn id(&self) -> TopicId {
        self.id
    }

    fn new(
        me: NodeId,
        topic_id: TopicId,
        config: Arc<Config>,
        to_topic_rx: mpsc::Receiver<ToTopic>,
        on_peer_data: OnPeerData,
        peer_data_updates: Watcher<PeerData>,
        connection_pool: ConnectionPool,
    ) -> Self {
        // This channel can be small: Network should wait so that we have flow control.
        let (neighbor_recv_tx, neighbor_recv_rx) = mpsc::channel(NEIGHBOR_RECV_CAPACITY);
        let peer_data = peer_data_updates.get().ok();
        let state = proto::topic::State::new(me, peer_data, config.proto.clone());
        let (event_sender, _event_recveiver) = broadcast::channel(config.event_channel_cap);
        let accept_rx = connection_pool.accept_topic(topic_id, 16);
        Self {
            peer_recv_tx: neighbor_recv_tx,
            peer_recv_rx: neighbor_recv_rx,
            id: topic_id,
            out_events: Default::default(),
            state,
            timers: Default::default(),
            peers: Default::default(),
            peer_send_tasks: Default::default(),
            peer_recv_tasks: Default::default(),
            config,
            closed: false,
            event_sender,
            command_receivers: Default::default(),
            event_send_tasks: Default::default(),
            neighbors: Default::default(),
            to_topic_rx,
            #[cfg(test)]
            joined: Default::default(),
            on_peer_data,
            peer_data_updates,
            init: false,
            connection_pool,
            accept_rx,
        }
    }

    fn reset(&mut self, to_topic_rx: mpsc::Receiver<ToTopic>) {
        let (neighbor_recv_tx, neighbor_recv_rx) = mpsc::channel(NEIGHBOR_RECV_CAPACITY);
        self.to_topic_rx = to_topic_rx;
        self.peer_recv_tx = neighbor_recv_tx;
        self.peer_recv_rx = neighbor_recv_rx;
        self.command_receivers = Default::default();
        self.event_send_tasks = Default::default();
        self.init = false;
        // self.spawn()
    }

    // fn spawn(mut self) -> AbortOnDropHandle<()> {
    //     let id = self.id;
    //     let task = n0_future::task::spawn(
    //         async move {
    //             self.run().await;
    //             self.to_gossip_actor
    //                 .clone()
    //                 .send(FromTopic::Closed {
    //                     topic_id: id,
    //                     state: self,
    //                 })
    //                 .await
    //                 .ok();
    //         }
    //         .instrument(error_span!("topic", topic = %id.fmt_short())),
    //     );
    //     AbortOnDropHandle::new(task)
    // }

    async fn run(mut self) -> Self {
        debug!("actor start");
        for i in 0.. {
            if !self.tick(i).await || self.should_close() {
                self.to_topic_rx.close();
                debug!("close: should close");
                self.handle_in_event(InEvent::Command(Command::Quit));
                self.closed = true;
                break;
            } else {
                trace!("tick end: should not close");
            }
        }
        trace!("actor closing");
        // Wait until all remaining messages are sent.
        while let Some(_) = self.peer_send_tasks.join_next().await {}
        trace!("send tasks closed");
        // if let Some(reply) = reply {
        //     reply.send(()).ok();
        // }
        // Drain queue for potential further quit messages.
        // TODO: remove this
        trace!("actor closed");
        self
    }

    async fn tick(&mut self, i: usize) -> bool {
        trace!("tick {i}: wait. send tasks: {}", self.peer_send_tasks.len());
        tokio::select! {
            biased;
            msg = self.to_topic_rx.recv() => {
                trace!(tick=i, "tick: to_topic {msg:?}");
                let Some(msg) = msg else {
                    self.handle_in_event(InEvent::Command(Command::Quit));
                    return false;
                };
                match msg {
                    ToTopic::Subscribe(channels) => self.handle_subscribe(channels),
                    // ToTopic::RemoteStream(recv_stream) => self.handle_remote_stream(recv_stream),
                }
            }
            remote_stream = self.accept_rx.recv() => {
                if let Some(stream) = remote_stream {
                    self.handle_remote_stream(stream);
                } else {
                    // TODO: connection actor died, or topic was reregsitered
                    warn!("connection actor died");
                    return false;
                }
            }
            Ok(our_peer_data) = self.peer_data_updates.updated() => {
                trace!(tick=i, "tick: update_peer_data");
                self.handle_in_event(InEvent::UpdatePeerData(our_peer_data));
            }
            command = self.command_receivers.next(), if !self.command_receivers.is_empty() => {
                trace!(tick=i, "tick: command {command:?}");
                if let Some(command) = command {
                    self.handle_in_event(InEvent::Command(command.into()));
                }
            }
            _ = self.event_send_tasks.join_next(), if !self.event_send_tasks.is_empty() => {
                trace!(tick=i, "tick: event sender closed");
            }
            Some((node_id, message)) = self.peer_recv_rx.recv() => {
                trace!(tick=i, node=%node_id.fmt_short(), "tick: recv from remote {message:?}");
                self.handle_in_event(InEvent::RecvMessage(node_id, message));
            }
            Some(res) = self.peer_send_tasks.join_next(), if !self.peer_send_tasks.is_empty() => {
                let (node_id, _res) = res.expect("sender task panicked");
                trace!(tick=i, node=%node_id.fmt_short(), "tick: sender to remote closed");
                let peer = self.peers.get_mut(&node_id).expect("sender state to be present");
                peer.send_tasks = peer.send_tasks.saturating_sub(1);
                if peer.send_tasks == 0 {
                    self.handle_in_event(InEvent::PeerDisconnected(node_id));
                    self.peers.remove(&node_id);
                }
            }
            _ = self.timers.wait_next() => {
                let now = Instant::now();
                while let Some((_instant, timer)) = self.timers.pop_before(now) {
                    self.handle_in_event(InEvent::TimerExpired(timer));
                }
            }
        }
        true
    }
    fn should_close(&self) -> bool {
        self.init && self.command_receivers.is_empty() && self.event_sender.receiver_count() == 0
    }

    fn handle_remote_stream(&mut self, mut stream: RemoteRecvStream) {
        let tx = self.peer_recv_tx.clone();
        let node_id = stream.node_id;
        let fut = async move {
            let mut buffer = BytesMut::new();
            loop {
                // read message
                // TODO: max message size
                match read_message(&mut stream.stream, &mut buffer, DEFAULT_MAX_MESSAGE_SIZE).await
                {
                    Err(err) => {
                        debug!("remote recv stream closed: {err:?}");
                        break;
                    }
                    Ok(None) => {
                        debug!("remote recv stream closed: EOF");
                        // stream.send_stream.write_all(&[1u8]).await.ok();
                        // stream.send_stream.stopped().await.ok();
                        break;
                    }
                    Ok(Some(message)) => {
                        if let Err(_) = tx.send((node_id, message)).await {
                            debug!("remote recv: closing because topic closed");
                            break;
                        }
                    }
                }
            }
            drop(stream.conn);
            stream.node_id
        };
        let fut = fut.instrument(error_span!("recv", remote=%node_id.fmt_short()));
        let abort_handle = self.peer_recv_tasks.spawn(fut);
        self.peers
            .entry(node_id)
            .or_default()
            .recv_tasks
            .push(abort_handle);
    }

    fn handle_subscribe(&mut self, channels: SubscribeChannels) {
        self.init = true;
        let (mut tx, rx) = channels;
        let rx = rx.into_stream().filter_map(Result::ok);
        self.command_receivers.push(Box::pin(rx));
        let initial_neighbors = self.neighbors.clone();
        let mut sub = self.event_sender.subscribe();
        // TODO: track this task? Maybe it's fine, the task terminates reliably.
        self.event_send_tasks.spawn(async move {
            for neighbor in initial_neighbors {
                if let Err(_err) = tx.send(api::Event::NeighborUp(neighbor)).await {
                    break;
                }
            }
            loop {
                let event = tokio::select! {
                    biased;
                    event = sub.recv() => event,
                    _ = tx.closed() => break
                };
                let event: api::Event = match event {
                    Ok(event) => event.into(),
                    Err(broadcast::error::RecvError::Lagged(_)) => api::Event::Lagged,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                if let Err(_err) = tx.send(event).await {
                    break;
                }
            }
        });
    }

    fn handle_in_event(&mut self, event: InEvent) {
        trace!("tick: in event {event:?}");
        let now = Instant::now();
        self.out_events.extend(self.state.handle(event, now));

        while let Some(event) = self.out_events.pop_front() {
            trace!("tick: out event {event:?}");
            match event {
                OutEvent::SendMessage(node_id, message) => {
                    self.send(node_id, message);
                }
                OutEvent::EmitEvent(event) => {
                    self.handle_event(event);
                }
                OutEvent::ScheduleTimer(delay, timer) => {
                    self.timers.insert(now + delay, timer);
                }
                OutEvent::DisconnectPeer(node_id) => {
                    if let Some(peer) = self.peers.get_mut(&node_id) {
                        debug!(remote=%node_id.fmt_short(), "disable sender");
                        let _ = peer.sender.take();
                        for handle in peer.recv_tasks.drain(..) {
                            handle.abort();
                        }
                    }
                }
                OutEvent::PeerData(node_id, peer_data) => (self.on_peer_data)(node_id, peer_data),
            }
        }
    }

    fn handle_event(&mut self, event: ProtoEvent) {
        match &event {
            ProtoEvent::NeighborUp(n) => {
                // TODO: Remove.
                #[cfg(test)]
                self.joined.store(true, Ordering::SeqCst);
                self.neighbors.insert(*n);
            }
            ProtoEvent::NeighborDown(n) => {
                self.neighbors.remove(n);
            }
            ProtoEvent::Received(_gossip_event) => {}
        }
        self.event_sender.send(event).ok();
    }

    fn send(&mut self, node_id: NodeId, message: Message) {
        let peer = self.peers.entry(node_id).or_default();
        let sender = if let Some(ref mut channel) = peer.sender {
            debug!(peer=%node_id.fmt_short(), "get sender");
            channel
        } else {
            debug!(peer=%node_id.fmt_short(), cap=self.config.peer_channel_cap, "spawn new sender");
            let (tx, rx) = mpsc::channel(self.config.peer_channel_cap);
            let actor = SendLoop {
                connection_pool: self.connection_pool.clone(),
                node_id,
                from_topic: rx,
                topic_id: self.id,
            };
            let fut = actor
                .run()
                .instrument(error_span!("send", remote=%node_id.fmt_short()));
            self.peer_send_tasks.spawn(fut);
            peer.send_tasks += 1;
            peer.sender.insert(tx)
        };
        // TODO: Make async
        if let Err(err) = sender.try_send(message) {
            match err {
                TrySendError::Full(_) => {
                    warn!(remote=%node_id.fmt_short(), "failed to send to peer channel: queue full");
                }
                TrySendError::Closed(_msg) => {
                    warn!(remote=%node_id.fmt_short(), "failed to send to peer channel: closed");
                    let _ = self
                        .peers
                        .get_mut(&node_id)
                        .expect("just checked")
                        .sender
                        .take();
                    self.handle_in_event(InEvent::PeerDisconnected(node_id));
                }
            }
        }
    }
}

struct SendLoop {
    connection_pool: ConnectionPool,
    from_topic: mpsc::Receiver<Message>,
    node_id: NodeId,
    topic_id: TopicId,
}

#[derive(Debug)]
enum SendLoopError {
    Reconnect,
    OpenStream(anyhow::Error),
    Write(WriteError),
}

impl SendLoop {
    async fn run(mut self) -> (NodeId, anyhow::Result<()>) {
        let res = loop {
            debug!("start send loop");
            match self.run_inner().await {
                Ok(()) => break Ok(()),
                Err(SendLoopError::Reconnect) => {
                    debug!("reconnect requested");
                    continue;
                }
                Err(SendLoopError::OpenStream(err)) => {
                    break Err(err).context("open stream");
                }
                Err(SendLoopError::Write(err)) => {
                    break Err(err.into());
                }
            }
        };
        tracing::info!(?res, "send loop close!");
        (self.node_id, res)
    }

    async fn run_inner(&mut self) -> Result<(), SendLoopError> {
        let mut buffer = BytesMut::new();

        let mut stream = self
            .connection_pool
            .open_topic(self.node_id, self.topic_id)
            .await
            .map_err(SendLoopError::OpenStream)?;

        loop {
            tokio::select! {
                biased;
                _ = stream.conn.should_replace() => {
                    debug!("replace connection");
                    stream.stream.finish().ok();
                    // we still want to give the old send stream a chance to finish.
                    tokio::task::spawn(async move {
                        stream.stream.stopped().await.ok();
                    });
                    return Err(SendLoopError::Reconnect);
                }
                message = self.from_topic.recv() => {
                    let Some(message) = message else {
                        debug!("sender closing: channel dropped (state says quit)");
                        debug!("finishing send stream..");
                        if let Ok(_) = stream.stream.finish() {
                            stream.stream.stopped().await.ok();
                        }
                        debug!("send stream finished");
                        return Ok(())
                    };
                    if let Err(err) = write_message(&mut stream.stream, &mut buffer, &message).await {
                        return if stream.conn.has_replacement() {
                            debug!("replace connection");
                            Err(SendLoopError::Reconnect)
                        } else {
                            warn!("sender closing with error: {err:#}");
                            Err(SendLoopError::Write(err))
                        };
                        // TODO: Is this what we want
                    }
                }
            }
        }
    }
}
