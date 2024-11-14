//! Provides a rpc protocol as well as a client for the protocol
use std::{
    fmt::Display,
    future::Future,
    marker::PhantomData,
    sync::{Arc, OnceLock},
};

use client::MemClient;
use proto::{Request, Response, RpcService};
use quic_rpc::{
    server::{ChannelTypes, RpcChannel},
    transport::flume::{FlumeConnector, FlumeListener},
    RpcClient, RpcServer,
};
use tokio::task::JoinSet;
use tokio_util::task::AbortOnDropHandle;
use tracing::{error, warn};

use crate::net::Gossip;
pub use crate::net::{Command as SubscribeUpdate, Event as SubscribeResponse};
pub mod client;
pub mod proto;

#[derive(Debug)]
pub(crate) struct RpcHandler {
    /// Client to hand out
    client: MemClient,
    /// Handler task
    _handler: AbortOnDropHandle<()>,
}

trait RpcServerExt<S, C>: Sized {
    /// Accept and handle incoming RPC requests in a loop.
    ///
    /// It is the caller's responsibility to poll the returned future to drive the server.
    fn accept_loop<Fun, Fut, E>(self, handler: Fun) -> impl Future<Output = ()> + Send + 'static
    where
        S: quic_rpc::Service,
        C: quic_rpc::Listener<S>,
        Fun: Fn(S::Req, RpcChannel<S, C>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = std::result::Result<(), E>> + Send + 'static,
        E: Display + 'static;

    /// Spawn a task to accept and handle incoming RPC requests in a loop.
    fn spawn_accept_loop<Fun, Fut, E>(self, handler: Fun) -> AbortOnDropHandle<()>
    where
        S: quic_rpc::Service,
        C: quic_rpc::Listener<S>,
        Fun: Fn(S::Req, RpcChannel<S, C>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = std::result::Result<(), E>> + Send + 'static,
        E: Display + 'static,
    {
        let task = tokio::spawn(self.accept_loop(handler));
        AbortOnDropHandle::new(task)
    }
}

impl<S, C> RpcServerExt<S, C> for RpcServer<S, C> {
    async fn accept_loop<Fun, Fut, E>(self, handler: Fun)
    where
        S: quic_rpc::Service,
        C: quic_rpc::Listener<S>,
        Fun: Fn(S::Req, RpcChannel<S, C>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = std::result::Result<(), E>> + Send + 'static,
        E: Display + 'static,
    {
        let handler = Arc::new(handler);
        let mut tasks = JoinSet::new();
        loop {
            tokio::select! {
                Some(res) = tasks.join_next(), if !tasks.is_empty() => {
                    if let Err(e) = res {
                        if e.is_panic() {
                            error!("Panic handling RPC request: {e}");
                        }
                    }
                }
                req = self.accept() => {
                    let req = match req {
                        Ok(req) => req,
                        Err(e) => {
                            warn!("Error accepting RPC request: {e}");
                            continue;
                        }
                    };
                    let handler = handler.clone();
                    tasks.spawn(async move {
                        let (req, chan) = match req.read_first().await {
                            Ok((req, chan)) => (req, chan),
                            Err(e) => {
                                warn!("Error reading first message: {e}");
                                return;
                            }
                        };
                        if let Err(cause) = handler(req, chan).await {
                            warn!("Error handling RPC request: {cause}");
                        }
                    });
                }
            }
        }
    }
}

/// A lazily initialized in-memory handler for RPC clients
#[derive(Clone)]
struct MemHandler<C> {
    inner: Arc<OnceLock<MemHandlerInner<C>>>,
}

impl<C> MemHandler<C> {
    fn new() -> Self {
        Self {
            inner: Arc::new(OnceLock::new()),
        }
    }

    fn client<S, Fun, Fut, E>(&self, handler: Fun) -> &C
    where
        S: quic_rpc::Service,
        Fun:
            Fn(S::Req, RpcChannel<S, FlumeListener<S::Req, S::Res>>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = std::result::Result<(), E>> + Send + 'static,
        E: Display + 'static,
        C: From<RpcClient<S, FlumeConnector<S::Res, S::Req>>>,
    {
        let inner = self.inner.get_or_init(|| {
            let (listener, connector) = quic_rpc::transport::flume::channel(1);
            let listener = RpcServer::new(listener);
            let handler = listener.spawn_accept_loop(handler);
            let connector = RpcClient::new(connector);
            let client = C::from(connector);
            MemHandlerInner { client, handler }
        });
        &inner.client
    }
}

struct MemHandlerInner<C> {
    client: C,
    handler: AbortOnDropHandle<()>,
}

impl RpcHandler {
    fn new(gossip: &Gossip) -> Self {
        let gossip = gossip.clone();
        let (listener, connector) = quic_rpc::transport::flume::channel(1);
        let listener = RpcServer::new(listener);
        let client = MemClient::new(RpcClient::new(connector));
        let handler = move |msg, chan| gossip.clone().handle_rpc_request_owned(msg, chan);
        Self {
            client,
            _handler: listener.spawn_accept_loop(handler),
        }
    }
}

impl Gossip {
    /// Get an in-memory gossip client
    pub fn client(&self) -> &client::Client<FlumeConnector<Response, Request>> {
        let handler = self
            .rpc_handler
            .get_or_init(|| Arc::new(RpcHandler::new(self)));
        &handler.client
    }

    /// Handle a gossip request from the RPC server.
    pub async fn handle_rpc_request<C: ChannelTypes<RpcService>>(
        &self,
        msg: Request,
        chan: quic_rpc::server::RpcChannel<RpcService, C>,
    ) -> Result<(), quic_rpc::server::RpcServerError<C>> {
        self.clone().handle_rpc_request_owned(msg, chan).await
    }

    async fn handle_rpc_request_owned<C: ChannelTypes<RpcService>>(
        self,
        msg: Request,
        chan: quic_rpc::server::RpcChannel<RpcService, C>,
    ) -> Result<(), quic_rpc::server::RpcServerError<C>> {
        use quic_rpc::server::RpcServerError;
        use Request::*;
        match msg {
            Subscribe(msg) => {
                chan.bidi_streaming(msg, self, move |handler, req, updates| {
                    let stream = handler.join_with_stream(
                        req.topic,
                        crate::net::JoinOptions {
                            bootstrap: req.bootstrap,
                            subscription_capacity: req.subscription_capacity,
                        },
                        Box::pin(updates),
                    );
                    futures_util::TryStreamExt::map_err(stream, |e| serde_error::Error::new(&*e))
                })
                .await
            }
            Update(_msg) => Err(RpcServerError::UnexpectedUpdateMessage),
        }
    }
}
