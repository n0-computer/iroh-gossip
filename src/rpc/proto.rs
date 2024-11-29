//! The RPC protocol between client and node
use std::collections::BTreeSet;

use iroh::NodeId;
use nested_enum_utils::enum_conversions;
use quic_rpc_derive::rpc_requests;
use serde::{Deserialize, Serialize};

pub use crate::net::{Command as SubscribeUpdate, Event as SubscribeResponse};
use crate::proto::TopicId;

/// The RPC service type for the gossip protocol
#[derive(Debug, Clone)]
pub struct RpcService;

impl quic_rpc::Service for RpcService {
    type Req = Request;
    type Res = Response;
}

type RpcResult<T> = std::result::Result<T, serde_error::Error>;

#[allow(missing_docs)]
#[derive(strum::Display, Debug, Serialize, Deserialize)]
#[enum_conversions]
#[rpc_requests(RpcService)]
pub enum Request {
    #[bidi_streaming(update = SubscribeUpdate, response = RpcResult<SubscribeResponse>)]
    Subscribe(SubscribeRequest),
    Update(SubscribeUpdate),
}

#[allow(missing_docs)]
#[derive(strum::Display, Debug, Serialize, Deserialize)]
#[enum_conversions]
pub enum Response {
    Subscribe(RpcResult<SubscribeResponse>),
}

/// A request to the node to subscribe to gossip events.
///
/// This is basically a topic and additional options
#[derive(Serialize, Deserialize, Debug)]
pub struct SubscribeRequest {
    /// The topic to subscribe to
    pub topic: TopicId,
    /// The nodes to bootstrap the subscription from
    pub bootstrap: BTreeSet<NodeId>,
    /// The capacity of the subscription
    pub subscription_capacity: usize,
}
