use std::{future::Future, sync::mpsc as blocking_mpsc};

use n0_future::future::poll_once;

use super::*;

const WAIT: Duration = Duration::from_secs(10);

struct DropSignal(Option<oneshot::Sender<()>>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            sender.send(()).ok();
        }
    }
}

struct TaskGate {
    release: Option<blocking_mpsc::Sender<()>>,
    started: oneshot::Receiver<()>,
    stopped: oneshot::Receiver<()>,
    cancelled: oneshot::Receiver<()>,
}

impl TaskGate {
    async fn release(mut self) {
        self.release.take().unwrap().send(()).unwrap();
        timeout(WAIT, &mut self.stopped).await.unwrap().unwrap();
    }
}

impl Drop for TaskGate {
    fn drop(&mut self) {
        // Release blocking tasks even if an assertion or timeout fails.
        if let Some(release) = self.release.take() {
            release.send(()).ok();
        }
    }
}

fn hold_task<T: Send + 'static>(tasks: &mut JoinSet<T>, endpoint: Endpoint, output: T) -> TaskGate {
    let (release, released) = blocking_mpsc::channel();
    let (started, started_rx) = oneshot::channel();
    let (stopped, stopped_rx) = oneshot::channel();
    tasks.spawn_blocking(move || {
        let _stopped = DropSignal(Some(stopped));
        let endpoint_owner = endpoint;
        started.send(()).ok();
        released.recv().ok();
        drop(endpoint_owner);
        output
    });

    // This companion task acknowledges cancellation independently of the
    // blocking task, whose actual completion still requires `release`.
    let (cancelled, cancelled_rx) = oneshot::channel();
    let cancelled = DropSignal(Some(cancelled));
    tasks.spawn(async move {
        let _cancelled = cancelled;
        std::future::pending().await
    });

    TaskGate {
        release: Some(release),
        started: started_rx,
        stopped: stopped_rx,
        cancelled: cancelled_rx,
    }
}

struct Fixture {
    gossip: Gossip,
    actor: Option<Actor>,
    gates: Vec<TaskGate>,
    endpoint: Endpoint,
    peer_endpoint: Endpoint,
    peer_connection: Connection,
}

impl Fixture {
    async fn new() -> Self {
        async fn make_endpoint() -> Endpoint {
            Endpoint::builder(presets::Minimal)
                .relay_mode(RelayMode::Disabled)
                .clear_ip_transports()
                .bind_addr("127.0.0.1:0")
                .unwrap()
                .alpns(vec![GOSSIP_ALPN.to_vec()])
                .bind()
                .await
                .unwrap()
        }

        let endpoint = make_endpoint().await;
        let peer_endpoint = make_endpoint().await;
        let peer_addr =
            EndpointAddr::new(peer_endpoint.id()).with_ip_addr(peer_endpoint.bound_sockets()[0]);
        let (connection, peer_connection) = timeout(WAIT, async {
            tokio::join!(endpoint.connect(peer_addr, GOSSIP_ALPN), async {
                peer_endpoint
                    .accept()
                    .await
                    .unwrap()
                    .accept()
                    .unwrap()
                    .await
            })
        })
        .await
        .unwrap();
        let metrics = Arc::new(Metrics::default());
        let (mut actor, rpc_tx, local_tx) = Actor::new(
            endpoint.clone(),
            Default::default(),
            metrics.clone(),
            None,
            GossipAddressLookup::default(),
        );
        let max_message_size = actor.state.max_message_size();
        let mut gates = vec![
            hold_task(
                &mut actor.connection_tasks,
                endpoint.clone(),
                (peer_endpoint.id(), connection.unwrap(), Ok(())),
            ),
            hold_task(
                &mut actor.topic_event_forwarders,
                endpoint.clone(),
                TopicId::from_bytes([7; 32]),
            ),
            hold_task(
                &mut actor.dialer.pending,
                endpoint.clone(),
                (peer_endpoint.id(), None),
            ),
        ];
        for gate in &mut gates {
            timeout(WAIT, &mut gate.started).await.unwrap().unwrap();
        }

        let gossip = Gossip {
            inner: Arc::new(Inner {
                api: GossipApi::local(rpc_tx),
                local_tx,
                actor_handle: Mutex::new(None),
                actor: Arc::new(Mutex::new(None)),
                max_message_size,
                metrics,
            }),
        };
        Self {
            gossip,
            actor: Some(actor),
            gates,
            endpoint,
            peer_endpoint,
            peer_connection: peer_connection.unwrap(),
        }
    }

    async fn spawn(&mut self) -> task::AbortHandle {
        *self.gossip.inner.actor.lock().await = self.actor.take();
        let handle = task::spawn(Actor::run(self.gossip.inner.actor.clone()));
        let abort = handle.abort_handle();
        *self.gossip.inner.actor_handle.lock().await = Some(AbortOnDropHandle::new(handle));
        abort
    }

    async fn cancelled(&mut self) {
        for gate in &mut self.gates {
            timeout(WAIT, &mut gate.cancelled).await.unwrap().unwrap();
        }
    }

    async fn release_tasks(&mut self) {
        for gate in self.gates.drain(..) {
            gate.release().await;
        }
    }

    async fn assert_drained(self) {
        assert!(self.gossip.inner.actor_handle.lock().await.is_none());
        assert!(self.gossip.inner.actor.lock().await.is_none());
        assert!(self.gossip.inner.local_tx.is_closed());
        assert!(self.gates.is_empty());
        timeout(WAIT, self.endpoint.close()).await.unwrap();
        timeout(WAIT, self.peer_endpoint.close()).await.unwrap();
        drop(self.peer_connection);
    }
}

async fn assert_pending(future: impl Future) {
    assert!(poll_once(future).await.is_none(), "shutdown returned early");
}

#[tokio::test]
async fn shutdown_joins_each_task_group() {
    for held_group in 0..3 {
        let mut fixture = Fixture::new().await;
        fixture.spawn().await;
        let gossip = fixture.gossip.clone();
        let mut shutdown = Box::pin(gossip.shutdown());
        assert_pending(shutdown.as_mut()).await;
        fixture.cancelled().await;

        let held = fixture.gates.swap_remove(held_group);
        fixture.release_tasks().await;
        assert_pending(shutdown.as_mut()).await;
        held.release().await;
        timeout(WAIT, shutdown).await.unwrap().unwrap();
        fixture.assert_drained().await;
    }
}

#[tokio::test]
async fn shutdown_reply_follows_task_drain_and_actor_drop() {
    let mut fixture = Fixture::new().await;
    fixture.spawn().await;
    let (reply, mut reply_rx) = oneshot::channel();
    fixture
        .gossip
        .inner
        .local_tx
        .send(LocalActorMessage::Shutdown { reply })
        .await
        .unwrap();
    fixture.cancelled().await;
    assert_pending(&mut reply_rx).await;
    assert!(!fixture.gossip.inner.local_tx.is_closed());
    fixture.release_tasks().await;
    timeout(WAIT, reply_rx).await.unwrap().unwrap();
    assert!(fixture.gossip.inner.local_tx.is_closed());
    assert!(fixture.gossip.inner.actor.lock().await.is_none());
    assert!(fixture.gossip.shutdown().await.is_err());
    fixture.assert_drained().await;
}

#[tokio::test]
async fn actor_panic_drains_all_children_before_shutdown_error() {
    let mut fixture = Fixture::new().await;
    fixture
        .actor
        .as_mut()
        .unwrap()
        .connection_tasks
        .spawn(async { panic!("gossip shutdown actor panic regression") });
    fixture.spawn().await;
    fixture.cancelled().await;
    let gossip = fixture.gossip.clone();
    let mut shutdown = Box::pin(gossip.shutdown());
    assert_pending(shutdown.as_mut()).await;
    fixture.release_tasks().await;
    assert!(timeout(WAIT, shutdown).await.unwrap().is_err());
    fixture.assert_drained().await;
}

#[tokio::test]
async fn failed_shutdown_send_still_joins_actor_and_children() {
    let mut fixture = Fixture::new().await;
    fixture.actor.as_mut().unwrap().local_rx.close();
    fixture.spawn().await;
    fixture.cancelled().await;
    let gossip = fixture.gossip.clone();
    let mut shutdown = Box::pin(gossip.shutdown());
    assert_pending(shutdown.as_mut()).await;
    fixture.release_tasks().await;
    assert!(timeout(WAIT, shutdown).await.unwrap().is_err());
    fixture.assert_drained().await;
}

#[tokio::test]
async fn failed_shutdown_reply_still_joins_actor_and_children() {
    let mut fixture = Fixture::new().await;
    *fixture.gossip.inner.actor.lock().await = fixture.actor.take();
    let owner = fixture.gossip.inner.actor.clone();
    let (reply_dropped, reply_dropped_rx) = oneshot::channel();
    let (finish_task, task_finished) = oneshot::channel();
    let handle = task::spawn(async move {
        let mut owner = owner.lock().await;
        let actor = owner.as_mut().unwrap();
        let Some(LocalActorMessage::Shutdown { reply }) = actor.local_rx.recv().await else {
            panic!("expected shutdown request");
        };
        drop(reply);
        reply_dropped.send(()).unwrap();
        task_finished.await.unwrap();
        // Leave the actor in its owner to exercise cleanup after task exit.
    });
    *fixture.gossip.inner.actor_handle.lock().await = Some(AbortOnDropHandle::new(handle));
    let gossip = fixture.gossip.clone();
    let mut shutdown = Box::pin(gossip.shutdown());
    assert_pending(shutdown.as_mut()).await;
    timeout(WAIT, reply_dropped_rx).await.unwrap().unwrap();
    assert_pending(shutdown.as_mut()).await;
    finish_task.send(()).unwrap();

    // The caller now owns cleanup, so keep polling it while waiting for the
    // three cancellation acknowledgements.
    tokio::select! {
        result = &mut shutdown => panic!("shutdown returned before cleanup: {result:?}"),
        () = fixture.cancelled() => {}
    }
    fixture.release_tasks().await;
    assert!(timeout(WAIT, shutdown).await.unwrap().is_err());
    fixture.assert_drained().await;
}

#[tokio::test]
async fn cancelled_shutdown_waiter_keeps_the_actor_handle() {
    let mut fixture = Fixture::new().await;
    fixture.spawn().await;
    let gossip = fixture.gossip.clone();
    let mut shutdown = Box::pin(gossip.shutdown());
    assert_pending(shutdown.as_mut()).await;
    fixture.cancelled().await;
    drop(shutdown);
    assert!(gossip.inner.actor_handle.lock().await.is_some());

    let mut shutdown = Box::pin(gossip.shutdown());
    assert_pending(shutdown.as_mut()).await;
    fixture.release_tasks().await;
    // The first request was accepted; this later request loses its reply when
    // the actor drops. It must still wait for the same real actor task.
    assert!(timeout(WAIT, shutdown).await.unwrap().is_err());
    fixture.assert_drained().await;
}

#[tokio::test]
async fn concurrent_shutdown_callers_wait_for_the_same_actor() {
    let mut fixture = Fixture::new().await;
    fixture.spawn().await;
    let gossip = fixture.gossip.clone();
    let other = gossip.clone();
    let mut first = Box::pin(gossip.shutdown());
    let mut second = Box::pin(other.shutdown());
    assert_pending(first.as_mut()).await;
    assert_pending(second.as_mut()).await;
    fixture.cancelled().await;
    assert_pending(first.as_mut()).await;
    assert_pending(second.as_mut()).await;
    fixture.release_tasks().await;
    timeout(WAIT, first).await.unwrap().unwrap();
    assert!(timeout(WAIT, second).await.unwrap().is_err());
    fixture.assert_drained().await;
}

#[tokio::test]
async fn cancelled_recovery_waiter_keeps_the_actor_after_task_join() {
    let mut fixture = Fixture::new().await;
    let abort = fixture.spawn().await;
    timeout(
        WAIT,
        fixture
            .gossip
            .reserve_external_connection(fixture.peer_endpoint.id()),
    )
    .await
    .unwrap()
    .unwrap();
    // A stopped task can leave a live, full receiver behind. Shutdown must
    // observe task completion even when sending its request cannot finish.
    for _ in 0..fixture.gossip.inner.local_tx.capacity() {
        let (reply, _) = oneshot::channel();
        fixture
            .gossip
            .inner
            .local_tx
            .try_send(LocalActorMessage::ReserveExternalConnection {
                peer_id: fixture.peer_endpoint.id(),
                reply,
            })
            .unwrap();
    }
    abort.abort();
    let gossip = fixture.gossip.clone();
    let mut shutdown = Box::pin(gossip.shutdown());
    tokio::select! {
        result = &mut shutdown => panic!("shutdown returned before cleanup: {result:?}"),
        () = fixture.cancelled() => {}
    }
    drop(shutdown);
    assert!(gossip.inner.actor_handle.lock().await.is_none());
    assert!(gossip.inner.actor.lock().await.is_some());

    let mut shutdown = Box::pin(gossip.shutdown());
    assert_pending(shutdown.as_mut()).await;
    fixture.release_tasks().await;
    assert!(timeout(WAIT, shutdown).await.unwrap().is_err());
    fixture.assert_drained().await;
}
