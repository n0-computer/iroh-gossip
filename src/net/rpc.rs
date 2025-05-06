use std::collections::BTreeSet;

use iroh::NodeId;
use irpc::{channel::spsc, Client, LocalSender};
use irpc_derive::rpc_requests;
use serde::{Deserialize, Serialize};

use crate::proto::TopicId;

use super::{Command, Error, Event, GossipTopic, JoinOptions, ReceiverId};

#[derive(Debug, Clone, Copy)]
pub(super) struct Service;

impl irpc::Service for Service {}

/// Input messages for the gossip [`Actor`].
#[rpc_requests(Service, message = Message)]
#[derive(Debug, Serialize, Deserialize)]
pub enum Protocol {
    #[rpc(tx=spsc::Sender<Event>, rx=spsc::Receiver<Command>)]
    Join(JoinRequest),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JoinRequest {
    pub topic_id: TopicId,
    pub bootstrap: BTreeSet<NodeId>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DropRequest {
    topic: TopicId,
    receiver_id: ReceiverId,
}

#[derive(Debug, Clone)]
pub struct GossipApi {
    inner: Client<Message, Protocol, Service>,
}

impl GossipApi {
    pub(crate) fn local(tx: tokio::sync::mpsc::Sender<Message>) -> Self {
        let local = LocalSender::<Message, Service>::from(tx);
        Self {
            inner: local.into(),
        }
    }

    /// Join a gossip topic with options.
    ///
    /// Returns a [`GossipTopic`] instantly. To wait for at least one connection to be established,
    /// you can await [`GossipTopic::joined`].
    ///
    /// Messages will be queued until a first connection is available. If the internal channel becomes full,
    /// the oldest messages will be dropped from the channel.
    pub async fn subscribe_with_opts(
        &self,
        topic_id: TopicId,
        opts: JoinOptions,
    ) -> anyhow::Result<GossipTopic> {
        let req = JoinRequest {
            topic_id,
            bootstrap: opts.bootstrap,
        };
        let (tx, rx) = self
            .inner
            .bidi_streaming(req, 16, opts.subscription_capacity)
            .await?;
        Ok(GossipTopic::new(tx, rx))
    }

    /// Join a gossip topic with the default options and wait for at least one active connection.
    pub async fn subscribe_and_join(
        &self,
        topic_id: TopicId,
        bootstrap: Vec<NodeId>,
    ) -> Result<GossipTopic, Error> {
        let mut sub = self
            .subscribe_with_opts(topic_id, JoinOptions::with_bootstrap(bootstrap))
            .await?;
        sub.joined().await?;
        Ok(sub)
    }

    /// Join a gossip topic with the default options.
    ///
    /// Note that this will not wait for any bootstrap node to be available.
    /// To ensure the topic is connected to at least one node, use [`GossipTopic::joined`]
    /// or [`Gossip::subscribe_and_join`]
    pub async fn subscribe(
        &self,
        topic_id: TopicId,
        bootstrap: Vec<NodeId>,
    ) -> Result<GossipTopic, Error> {
        let sub = self
            .subscribe_with_opts(topic_id, JoinOptions::with_bootstrap(bootstrap))
            .await?;

        Ok(sub)
    }
}
