//! Provides a rpc protocol as well as a client for the protocol
use std::sync::Arc;

use client::MemClient;
use proto::{Request, Response, RpcService};
use quic_rpc::{server::ChannelTypes, transport::flume::FlumeConnector, RpcClient, RpcServer};
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

impl RpcHandler {
    fn new(gossip: &Gossip) -> Self {
        let gossip = gossip.clone();
        let (listener, connector) = quic_rpc::transport::flume::channel(1);
        let listener = RpcServer::new(listener);
        let client = MemClient::new(RpcClient::new(connector));
        let task = tokio::spawn(async move {
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
                    req = listener.accept() => {
                        let req = match req {
                            Ok(req) => req,
                            Err(e) => {
                                warn!("Error accepting RPC request: {e}");
                                continue;
                            }
                        };
                        let gossip = gossip.clone();
                        tasks.spawn(async move {
                            let (req, client) = match req.read_first().await {
                                Ok((req, client)) => (req, client),
                                Err(e) => {
                                    warn!("Error reading first message: {e}");
                                    return;
                                }
                            };
                            if let Err(cause) = gossip.handle_rpc_request(req, client).await {
                                warn!("Error handling RPC request: {:?}", cause);
                            }
                        });
                    }
                }
            }
        });
        Self {
            client,
            _handler: AbortOnDropHandle::new(task),
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
        use quic_rpc::server::RpcServerError;
        use Request::*;
        match msg {
            Subscribe(msg) => {
                let this = self.clone();
                chan.bidi_streaming(msg, this, move |handler, req, updates| {
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
