//! Provides a rpc protocol as well as a client for the protocol
use proto::RpcService;

use crate::net::Gossip;
pub use crate::net::{Command as SubscribeUpdate, Event as SubscribeResponse};
pub mod client;
pub mod proto;

impl Gossip {
    /// Handle a gossip request from the RPC server.
    pub async fn handle_rpc_request<
        C: quic_rpc::ServiceEndpoint<RpcService>,
    >(
        &self,
        msg: crate::rpc::proto::Request,
        chan: quic_rpc::server::RpcChannel<RpcService, C>,
    ) -> Result<(), quic_rpc::server::RpcServerError<C>> {
        use quic_rpc::server::RpcServerError;

        use crate::rpc::proto::Request::*;
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
