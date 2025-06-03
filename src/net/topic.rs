use std::{
    collections::{hash_map, BTreeSet, HashMap, VecDeque},
    ops::Deref,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
};

use bytes::BytesMut;
use futures_util::FutureExt;
use iroh::{
    endpoint::{Connection, ConnectionError, RecvStream, SendStream},
    protocol::ProtocolHandler,
    Endpoint, NodeId,
};
use irpc::{channel::spsc, WithChannels};
use n0_future::{task::AbortOnDropHandle, time::Instant, StreamExt};
use rand::rngs::StdRng;
use tokio::{
    sync::{
        broadcast,
        mpsc::{self},
        oneshot, Mutex, Notify,
    },
    task::{AbortHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error_span, info, trace, warn, Instrument};

use crate::{
    api,
    proto::{self, TopicId, DEFAULT_MAX_MESSAGE_SIZE},
};

use super::{
    connections::{StreamHeader, StrongConnection, TopicRecvStream},
    util::{read_message, write_message, Timers},
};

type SubscribeChannels = (spsc::Sender<api::Event>, spsc::Receiver<api::Command>);
type CommandReceiverStream = n0_future::stream::Boxed<api::Command>;

///
#[derive(Debug, Clone)]
pub struct Config {
    ///
    pub peer_channel_cap: usize,
    ///
    pub event_channel_cap: usize,
    ///
    pub proto: proto::Config,
}

#[derive(derive_more::Debug)]
enum ToTopic {
    Subscribe(#[debug("SubscribeChannels")] SubscribeChannels),
    RemoteStream(
        #[debug("{}", _0.fmt_short())] NodeId,
        #[debug("Stream")] TopicRecvStream,
    ),
    Quit {
        reply: oneshot::Sender<()>,
    },
}

enum FromTopic {
    Connect {
        node_id: NodeId,
        reply: oneshot::Sender<anyhow::Result<StrongConnection>>,
    },
}

type Message = proto::topic::Message<NodeId>;
type Event = proto::topic::Event<NodeId>;
type Timer = proto::topic::Timer<NodeId>;
type ProtoEvent = proto::topic::Event<NodeId>;
type OutEvent = proto::topic::OutEvent<NodeId>;
type InEvent = proto::topic::InEvent<NodeId>;
type Command = proto::topic::Command<NodeId>;

#[derive(Debug)]
pub struct TopicHandle {
    tx: mpsc::Sender<ToTopic>,
    _handle: AbortOnDropHandle<()>,
}

impl TopicHandle {
    pub fn spawn(
        me: NodeId,
        topic_id: TopicId,
        config: Arc<Config>,
        from_topic_tx: mpsc::Sender<FromTopic>,
    ) -> TopicHandle {
        let (to_topic_tx, to_topic_rx) = mpsc::channel(8);
        let topic = Topic::new(me, topic_id, config, to_topic_rx, from_topic_tx);
        let task = n0_future::task::spawn(
            async move {
                topic.run().await;
                info!("topic closed.");
            }
            .instrument(error_span!("topic", me = %me, topic = %topic_id.fmt_short())),
        );
        TopicHandle {
            tx: to_topic_tx,
            _handle: AbortOnDropHandle::new(task),
        }
    }
}

#[derive(derive_more::Debug)]
struct Topic {
    id: TopicId,

    state: crate::proto::topic::State<NodeId, StdRng>,
    out_events: VecDeque<OutEvent>,

    timers: Timers<Timer>,

    neighbors: BTreeSet<NodeId>,
    neighbor_senders: HashMap<NodeId, NeighborSender>,
    neighbor_send_task: JoinSet<NodeId>,

    neighbor_recv_tasks: JoinSet<NodeId>,
    neighbor_recv_abort_handles: HashMap<NodeId, Vec<AbortHandle>>,
    neighbor_recv_rx: mpsc::Receiver<(NodeId, Message)>,
    neighbor_recv_tx: mpsc::Sender<(NodeId, Message)>,

    config: Arc<Config>,
    closed: bool,

    event_sender: broadcast::Sender<Event>,
    #[debug("MergeUnbounded<CommandReceiverStream>")]
    command_receivers: n0_future::MergeUnbounded<CommandReceiverStream>,

    to_topic_rx: mpsc::Receiver<ToTopic>,
    from_topic_tx: mpsc::Sender<FromTopic>,
}

impl Topic {
    fn new(
        me: NodeId,
        topic_id: TopicId,
        config: Arc<Config>,
        to_topic_rx: mpsc::Receiver<ToTopic>,
        from_topic_tx: mpsc::Sender<FromTopic>,
    ) -> Self {
        // This channel can be small: Network should wait so that we have flow control.
        let (recv_tx, recv) = mpsc::channel(16);
        let state = proto::topic::State::new(me, None, config.proto.clone());
        let (event_sender, _event_recveiver) = broadcast::channel(config.event_channel_cap);
        Self {
            neighbor_recv_tx: recv_tx,
            neighbor_recv_rx: recv,
            id: topic_id,
            out_events: Default::default(),
            state,
            timers: Default::default(),
            neighbor_senders: Default::default(),
            neighbor_recv_abort_handles: Default::default(),
            config,
            neighbor_send_task: Default::default(),
            neighbor_recv_tasks: Default::default(),
            closed: false,
            event_sender,
            command_receivers: Default::default(),
            neighbors: Default::default(),
            to_topic_rx,
            from_topic_tx,
        }
    }
    async fn run(mut self) {
        let reply = loop {
            if let Some(reply) = self.tick().await {
                break Some(reply);
            }
            if self.should_close() {
                self.handle_in_event(InEvent::Command(Command::Quit));
                self.closed = true;
                break None;
            }
        };
        trace!("actor closing");
        // Wait until all remaining messages are sent.
        while let Some(_) = self.neighbor_send_task.join_next().await {}
        trace!("send tasks closed");
        if let Some(reply) = reply {
            reply.send(()).ok();
        }
        // Drain queue for potential further quit messages.
        // TODO: remove this
        while let Ok(msg) = self.to_topic_rx.try_recv() {
            if let ToTopic::Quit { reply } = msg {
                reply.send(()).ok();
            }
        }
        trace!("actor closed");
    }
    async fn tick(&mut self) -> Option<oneshot::Sender<()>> {
        tokio::select! {
            Some(msg) = self.to_topic_rx.recv() => {
                trace!("tick: to_topic {msg:?}");
                match msg {
                    ToTopic::Subscribe(channels) => self.handle_subscribe(channels),
                    ToTopic::RemoteStream(node_id, recv_stream) => self.handle_remote_stream(node_id, recv_stream),
                    ToTopic::Quit { reply }=> {
                        self.handle_in_event(InEvent::Command(Command::Quit));
                        return Some(reply);
                    }
                }
            }
            Some(command) = self.command_receivers.next(), if !self.command_receivers.is_empty() => {
                trace!("tick: command {command:?}");
                self.handle_in_event(InEvent::Command(command.into()));
            }
            Some((node_id, message)) = self.neighbor_recv_rx.recv() => {
                trace!(node=%node_id.fmt_short(), "tick: recv from remote {message:?}");
                self.handle_in_event(InEvent::RecvMessage(node_id, message));
            }
            Some(node_id) = self.neighbor_send_task.join_next(), if !self.neighbor_send_task.is_empty() => {
                let node_id = node_id.expect("sender task panicked");
                trace!(node=%node_id.fmt_short(), "tick: remote sender closed");
                let peer = self.neighbor_senders.remove(&node_id).expect("sender state to be present");
                if peer.channel.is_some() {
                    self.handle_in_event(InEvent::PeerDisconnected(node_id));
                }
            }
            _ = self.timers.wait_next() => {
                let now = Instant::now();
                while let Some((_instant, timer)) = self.timers.pop_before(now) {
                    self.handle_in_event(InEvent::TimerExpired(timer));
                }
            }
        }
        None
    }
    fn should_close(&self) -> bool {
        self.command_receivers.is_empty()
            && self.event_sender.receiver_count() == 0
            && self.neighbor_recv_rx.sender_strong_count() == 1
    }

    fn handle_remote_stream(&mut self, node_id: NodeId, mut stream: TopicRecvStream) {
        let tx = self.neighbor_recv_tx.clone();
        let fut = async move {
            let mut buffer = BytesMut::new();
            loop {
                // read message
                // TODO: max message size
                match read_message(
                    &mut stream.recv_stream,
                    &mut buffer,
                    DEFAULT_MAX_MESSAGE_SIZE,
                )
                .await
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
            node_id
        };
        let fut = fut.instrument(error_span!("recv", remote=%node_id.fmt_short()));
        let abort_handle = self.neighbor_recv_tasks.spawn(fut);
        self.neighbor_recv_abort_handles
            .entry(node_id)
            .or_default()
            .push(abort_handle);
    }

    fn handle_subscribe(&mut self, channels: SubscribeChannels) {
        let (mut tx, rx) = channels;
        let rx = rx.into_stream().filter_map(Result::ok);
        self.command_receivers.push(Box::pin(rx));
        let mut sub = self.event_sender.subscribe();
        // TODO: track this task? Maybe it's fine, the task terminates reliably.
        tokio::task::spawn(async move {
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
                    // if !self.connections.send(node_id, !message) {
                    //     self.disconnect_queue.push(node_id);
                    // }
                }
                OutEvent::EmitEvent(event) => {
                    self.handle_event(event);
                }
                OutEvent::ScheduleTimer(delay, timer) => {
                    self.timers.insert(now + delay, timer);
                }
                OutEvent::DisconnectPeer(node_id) => {
                    if let Some(sender) = self.neighbor_senders.get_mut(&node_id) {
                        debug!(remote=%node_id.fmt_short(), "disable sender");
                        sender.disable();
                    }
                    // if let Some(mut handles) = self.receivers.remove(&node_id) {
                    //     handles.drain(..).for_each(|handle| handle.abort());
                    // }
                }
                OutEvent::PeerData(_node_id, _peer_data) => todo!(),
            }
            // if self.should_close() {
            //     break;
            // }
        }
    }

    fn handle_event(&mut self, event: ProtoEvent) {
        match &event {
            ProtoEvent::NeighborUp(n) => {
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
        let sender = self.neighbor_senders.entry(node_id).or_insert_with(|| {
            let (tx, rx) = mpsc::channel(self.config.peer_channel_cap);
            let actor = SendActor {
                from_topic_tx: self.from_topic_tx.clone(),
                node_id,
                rx,
                topic_id: self.id,
            };

            let fut = actor
                .run()
                .instrument(error_span!("send", remote=%node_id.fmt_short()));
            self.neighbor_send_task.spawn(fut);
            NeighborSender { channel: Some(tx) }
        });
        if !sender.send(message) {
            warn!(remote=%node_id.fmt_short(), "failed to send to peer: queue full");
            // disconnect?
            // queue final message in permit?
        }
    }
}

struct SendActor {
    rx: mpsc::Receiver<Message>,
    node_id: NodeId,
    topic_id: TopicId,
    from_topic_tx: mpsc::Sender<FromTopic>,
}
impl SendActor {
    async fn run(mut self) -> NodeId {
        if let Err(err) = self.run2().await {
            warn!("send loop failed: {err:?}");
        }
        self.node_id
    }

    async fn run2(&mut self) -> anyhow::Result<()> {
        let mut buffer = BytesMut::new();
        'reconnect: loop {
            if self.rx.is_closed() {
                debug!("sender closing: channel dropped (state says quit)");
                return Ok(());
            }
            debug!("connect");
            let (reply, reply_rx) = oneshot::channel();
            self.from_topic_tx
                .send(FromTopic::Connect {
                    node_id: self.node_id,
                    reply,
                })
                .await?;
            let conn = reply_rx.await??;
            // let conn = self.connections.get(self.node_id).await?;
            if self.rx.is_closed() {
                debug!("sender closing: channel dropped (state says quit)");
                return Ok(());
            }
            debug!("connected");
            let mut send_stream = match open_stream(&conn, self.topic_id).await {
                Ok(stream) => stream,
                Err(err) => {
                    debug!("failed to open stream: {err:?}");
                    continue 'reconnect;
                }
            };
            debug!("stream opened");
            if self.rx.is_closed() {
                debug!("sender closing: channel dropped (state says quit)");
                return Ok(());
            }

            loop {
                tokio::select! {
                    _ = conn.should_replace() => {
                        debug!("replace connection");
                        finish_send(send_stream).await;
                        continue 'reconnect;
                    }
                    message = self.rx.recv() => {
                        let Some(message) = message else {
                            debug!("sender closing: channel dropped (state says quit)");
                            finish_send(send_stream).await;
                            return Ok(())
                        };
                        if let Err(err) = write_message(&mut send_stream, &mut buffer, &message).await {
                            warn!("sender closing with error: {err:#}");
                            continue 'reconnect;
                        }
                    }
                }
            }
        }
    }
}

async fn finish_send(mut send_stream: SendStream) {
    send_stream.finish().ok();
    send_stream.stopped().await.ok();
    // let mut buf = [0u8; 1];
    // recv_stream.read_exact(&mut buf).await.ok();
    // recv_stream.stop(1u32.into()).ok();
}

async fn open_stream(conn: &StrongConnection, topic_id: TopicId) -> anyhow::Result<SendStream> {
    let mut stream = conn.open_uni().await?;
    let header = StreamHeader { topic_id };
    header.write(&mut stream).await?;
    Ok(stream)
}

#[derive(Debug)]
struct NeighborSender {
    channel: Option<mpsc::Sender<Message>>,
}

impl NeighborSender {
    fn disable(&mut self) {
        let _ = self.channel.take();
    }
    fn send(&self, message: Message) -> bool {
        if let Some(ref channel) = self.channel {
            channel.try_send(message).is_ok()
        } else {
            false
        }
    }
}
