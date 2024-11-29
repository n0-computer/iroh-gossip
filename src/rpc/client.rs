//! Iroh gossip client.
//!
//! Create a [`Client`] with a [`quic_rpc::RpcClient`] and use it to interact
//! with a node that runs gossip.
use std::collections::BTreeSet;

use anyhow::Result;
use futures_lite::{Stream, StreamExt};
use futures_util::{Sink, SinkExt};
use iroh::NodeId;
use quic_rpc::{client::BoxedConnector, transport::flume::FlumeConnector};

use crate::{
    net::{Command as SubscribeUpdate, Event as SubscribeResponse},
    proto::TopicId,
    rpc::proto::{RpcService, SubscribeRequest},
};

/// Iroh gossip client.
#[derive(Debug, Clone)]
pub struct Client<C = BoxedConnector<RpcService>> {
    pub(super) rpc: quic_rpc::RpcClient<RpcService, C>,
}

/// Type alias for a memory-backed client.
pub type MemClient =
    Client<FlumeConnector<crate::rpc::proto::Response, crate::rpc::proto::Request>>;

/// Options for subscribing to a gossip topic.
#[derive(Debug, Clone)]
pub struct SubscribeOpts {
    /// Bootstrap nodes to connect to.
    pub bootstrap: BTreeSet<NodeId>,
    /// Subscription capacity.
    pub subscription_capacity: usize,
}

impl Default for SubscribeOpts {
    fn default() -> Self {
        Self {
            bootstrap: BTreeSet::new(),
            subscription_capacity: 256,
        }
    }
}

impl<C> Client<C>
where
    C: quic_rpc::Connector<RpcService>,
{
    /// Creates a new gossip client.
    pub fn new(rpc: quic_rpc::RpcClient<RpcService, C>) -> Self {
        Self { rpc }
    }

    /// Subscribes to a gossip topic.
    ///
    /// Returns a sink to send updates to the topic and a stream of responses.
    ///
    /// Updates are either [Broadcast](crate::net::Command::Broadcast)
    /// or [BroadcastNeighbors](crate::net::Command::BroadcastNeighbors).
    ///
    /// Broadcasts are gossiped to the entire swarm, while BroadcastNeighbors are sent to
    /// just the immediate neighbors of the node.
    ///
    /// Responses are either [Gossip](crate::net::Event::Gossip) or
    /// [Lagged](crate::net::Event::Lagged).
    ///
    /// Gossip events contain the actual message content, as well as information about the
    /// immediate neighbors of the node.
    ///
    /// A Lagged event indicates that the gossip stream has not been consumed quickly enough.
    /// You can adjust the buffer size with the [`SubscribeOpts::subscription_capacity`] option.
    pub async fn subscribe_with_opts(
        &self,
        topic: TopicId,
        opts: SubscribeOpts,
    ) -> Result<(
        impl Sink<SubscribeUpdate, Error = anyhow::Error>,
        impl Stream<Item = Result<SubscribeResponse>>,
    )> {
        let (sink, stream) = self
            .rpc
            .bidi(SubscribeRequest {
                topic,
                bootstrap: opts.bootstrap,
                subscription_capacity: opts.subscription_capacity,
            })
            .await?;
        let stream = stream.map(|item| anyhow::Ok(item??));
        let sink = sink.sink_map_err(|_| anyhow::anyhow!("send error"));
        Ok((sink, stream))
    }

    /// Subscribes to a gossip topic with default options.
    pub async fn subscribe(
        &self,
        topic: impl Into<TopicId>,
        bootstrap: impl IntoIterator<Item = impl Into<NodeId>>,
    ) -> Result<(
        impl Sink<SubscribeUpdate, Error = anyhow::Error>,
        impl Stream<Item = Result<SubscribeResponse>>,
    )> {
        let bootstrap = bootstrap.into_iter().map(Into::into).collect();
        self.subscribe_with_opts(
            topic.into(),
            SubscribeOpts {
                bootstrap,
                ..Default::default()
            },
        )
        .await
    }
}
