//! Provides a rpc protocol as well as a client for the protocol
use std::sync::Arc;

use client::MemClient;
use proto::{Request, Response, RpcService};
use quic_rpc::{server::ChannelTypes, transport::flume::FlumeConnector, RpcClient, RpcServer};
use tokio_util::task::AbortOnDropHandle;

pub use crate::net::{Command as SubscribeUpdate, Event as SubscribeResponse};
use crate::net::{Gossip, Inner};
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
    fn new(gossip: Arc<Inner>) -> Self {
        let gossip = gossip.clone();
        let (listener, connector) = quic_rpc::transport::flume::channel(1);
        let listener = RpcServer::new(listener);
        let client = MemClient::new(RpcClient::new(connector));
        let _handler = listener
            .spawn_accept_loop(move |req, chan| gossip.clone().handle_rpc_request(req, chan));

        Self { client, _handler }
    }
}

impl Inner {
    pub async fn handle_rpc_request<C: ChannelTypes<RpcService>>(
        self: Arc<Self>,
        msg: Request,
        chan: quic_rpc::server::RpcChannel<RpcService, C>,
    ) -> Result<(), quic_rpc::server::RpcServerError<C>> {
        use quic_rpc::server::RpcServerError;
        use Request::*;
        match msg {
            Subscribe(msg) => {
                let this = self.clone();
                chan.bidi_streaming(msg, this, move |handler, req, updates| {
                    let stream = handler.subscribe_with_stream(
                        req.topic,
                        crate::net::JoinOptions {
                            bootstrap: req.bootstrap,
                            subscription_capacity: req.subscription_capacity,
                        },
                        Box::pin(updates),
                    );
                    futures_util::TryStreamExt::map_err(stream, |e| serde_error::Error::new(&e))
                })
                .await
            }
            Update(_msg) => Err(RpcServerError::UnexpectedUpdateMessage),
        }
    }
}

impl Gossip {
    /// Get an in-memory gossip client
    pub fn client(&self) -> &client::Client<FlumeConnector<Response, Request>> {
        let handler = self
            .rpc_handler
            .get_or_init(|| RpcHandler::new(self.inner.clone()));
        &handler.client
    }

    /// Handle a gossip request from the RPC server.
    pub async fn handle_rpc_request<C: ChannelTypes<RpcService>>(
        self,
        msg: Request,
        chan: quic_rpc::server::RpcChannel<RpcService, C>,
    ) -> Result<(), quic_rpc::server::RpcServerError<C>> {
        self.inner.handle_rpc_request(msg, chan).await
    }
}
