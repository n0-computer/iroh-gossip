//! Provides a rpc protocol as well as a client for the protocol
use crate::net::Gossip;
pub use crate::net::{Command as SubscribeUpdate, Event as SubscribeResponse};
pub mod client;
pub mod proto;

impl Gossip {
    /// Handle a gossip request from the RPC server.
    pub async fn handle_rpc_request<S: quic_rpc::Service, C: quic_rpc::ServiceEndpoint<S>>(
        &self,
        msg: crate::rpc::proto::Request,
        chan: quic_rpc::server::RpcChannel<crate::rpc::proto::RpcService, C, S>,
    ) -> Result<(), quic_rpc::server::RpcServerError<C>> {
        use iroh_base::rpc::RpcError;
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
                    futures_util::TryStreamExt::map_err(stream, RpcError::from)
                })
                .await
            }
            Update(_msg) => Err(RpcServerError::UnexpectedUpdateMessage),
        }
    }
}
