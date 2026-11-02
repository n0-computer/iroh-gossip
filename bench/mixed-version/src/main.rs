//! Runs nodes of iroh-gossip 0.101.0 and of current main in one swarm.

use std::{collections::BTreeSet, time::Duration};

use anyhow::{bail, Context, Result};
use bytes::Bytes;
use iroh::{
    address_lookup::memory::MemoryLookup, endpoint::presets, protocol::Router, Endpoint,
    EndpointId, RelayMode,
};
use n0_future::StreamExt;
use tokio::time::{sleep, timeout};

/// What a topic handle of either version reports.
#[derive(Debug)]
enum Ev {
    Received(Bytes),
    Other,
}

enum Gossip {
    Old(old::net::Gossip),
    New(new::net::Gossip),
}

enum Topic {
    Old(old::api::GossipTopic),
    New(new::api::GossipTopic),
}

struct Node {
    name: &'static str,
    endpoint: Endpoint,
    gossip: Gossip,
    router: Router,
}

impl Node {
    async fn spawn(name: &'static str, old: bool, lookup: &MemoryLookup) -> Result<Self> {
        let endpoint = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .bind()
            .await?;
        endpoint.address_lookup()?.add(lookup.clone());
        lookup.set_endpoint_info(endpoint.addr());
        let (gossip, router) = if old {
            let gossip = old::net::Gossip::builder().spawn(endpoint.clone());
            let router = Router::builder(endpoint.clone())
                .accept(old::net::GOSSIP_ALPN, gossip.clone())
                .spawn();
            (Gossip::Old(gossip), router)
        } else {
            let gossip = new::net::Gossip::builder().spawn(endpoint.clone());
            let router = Router::builder(endpoint.clone())
                .accept(new::net::GOSSIP_ALPN, gossip.clone())
                .spawn();
            (Gossip::New(gossip), router)
        };
        Ok(Self {
            name,
            endpoint,
            gossip,
            router,
        })
    }

    fn id(&self) -> EndpointId {
        self.endpoint.id()
    }

    async fn subscribe(&self, topic: [u8; 32], bootstrap: Vec<EndpointId>) -> Result<Topic> {
        Ok(match &self.gossip {
            Gossip::Old(g) => Topic::Old(g.subscribe(old::proto::TopicId::from_bytes(topic), bootstrap).await?),
            Gossip::New(g) => Topic::New(g.subscribe(new::proto::TopicId::from_bytes(topic), bootstrap).await?),
        })
    }
}

impl Topic {
    async fn joined(&mut self) -> Result<()> {
        match self {
            Topic::Old(t) => t.joined().await?,
            Topic::New(t) => t.joined().await?,
        }
        Ok(())
    }

    async fn broadcast(&mut self, msg: Bytes) -> Result<()> {
        match self {
            Topic::Old(t) => t.broadcast(msg).await?,
            Topic::New(t) => t.broadcast(msg).await?,
        }
        Ok(())
    }

    async fn next(&mut self) -> Result<Option<Ev>> {
        Ok(match self {
            Topic::Old(t) => match t.next().await {
                None => None,
                Some(ev) => Some(match ev? {
                    old::api::Event::Received(m) => Ev::Received(m.content),
                    _ => Ev::Other,
                }),
            },
            Topic::New(t) => match t.next().await {
                None => None,
                Some(ev) => Some(match ev? {
                    new::api::Event::Received(m) => Ev::Received(m.content),
                    _ => Ev::Other,
                }),
            },
        })
    }
}

/// Waits until `topic` received every message of `expected`.
async fn receive_all(name: &str, topic: &mut Topic, expected: &BTreeSet<Bytes>) -> Result<()> {
    let mut missing = expected.clone();
    let wait = async {
        while !missing.is_empty() {
            match topic.next().await? {
                Some(Ev::Received(m)) => {
                    missing.remove(&m);
                }
                Some(Ev::Other) => {}
                None => bail!("{name}: topic ended"),
            }
        }
        Ok(())
    };
    match timeout(Duration::from_secs(15), wait).await {
        Ok(res) => res,
        Err(_) => bail!("{name}: missing {} of {} messages", missing.len(), expected.len()),
    }
}

/// Every node broadcasts one message, and every node must receive all the others.
async fn round(label: &str, nodes: &[Node], topics: &mut [Topic]) -> Result<()> {
    let messages: Vec<Bytes> = nodes
        .iter()
        .map(|n| Bytes::from(format!("{label} from {}", n.name)))
        .collect();
    for (topic, msg) in topics.iter_mut().zip(&messages) {
        topic.broadcast(msg.clone()).await?;
    }
    for (i, (node, topic)) in nodes.iter().zip(topics.iter_mut()).enumerate() {
        let expected: BTreeSet<Bytes> = messages
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, m)| m.clone())
            .collect();
        receive_all(node.name, topic, &expected)
            .await
            .with_context(|| format!("round {label}"))?;
    }
    println!("ok: {label}");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let lookup = MemoryLookup::new();
    let nodes = vec![
        Node::spawn("old1", true, &lookup).await?,
        Node::spawn("new1", false, &lookup).await?,
        Node::spawn("old2", true, &lookup).await?,
        Node::spawn("new2", false, &lookup).await?,
    ];
    let t1 = [1u8; 32];

    // Joins across versions: old1 → new1 → (start), new2 → old1, old2 → new2.
    let mut topics = Vec::new();
    topics.push(nodes[0].subscribe(t1, vec![]).await?);
    topics.push(nodes[1].subscribe(t1, vec![nodes[0].id()]).await?);
    topics.push(nodes[2].subscribe(t1, vec![nodes[1].id()]).await?);
    topics.push(nodes[3].subscribe(t1, vec![nodes[2].id()]).await?);
    for (node, topic) in nodes.iter().zip(topics.iter_mut()).skip(1) {
        timeout(Duration::from_secs(15), topic.joined())
            .await
            .with_context(|| format!("{} did not join", node.name))??;
    }
    sleep(Duration::from_secs(2)).await;
    round("mixed swarm", &nodes, &mut topics).await?;

    // A new node leaves and rejoins at once through an old node.
    topics[3] = nodes[3].subscribe([9u8; 32], vec![]).await?;
    topics[3] = nodes[3].subscribe(t1, vec![nodes[0].id()]).await?;
    timeout(Duration::from_secs(15), topics[3].joined()).await??;
    sleep(Duration::from_secs(2)).await;
    round("quick rejoin of a new node", &nodes, &mut topics).await?;

    // Quiet for longer than the grace of a new node, then everyone sends again.
    sleep(Duration::from_secs(15)).await;
    round("after 15 s of quiet", &nodes, &mut topics).await?;

    // An old node leaves, the swarm is quiet past the grace, and it rejoins through a new node.
    topics[2] = nodes[2].subscribe([8u8; 32], vec![]).await?;
    sleep(Duration::from_secs(15)).await;
    topics[2] = nodes[2].subscribe(t1, vec![nodes[3].id()]).await?;
    timeout(Duration::from_secs(15), topics[2].joined()).await??;
    sleep(Duration::from_secs(2)).await;
    round("late rejoin of an old node", &nodes, &mut topics).await?;

    // A second topic after the quiet period, joined both ways between versions.
    let t2 = [2u8; 32];
    let mut topics2 = Vec::new();
    topics2.push(nodes[1].subscribe(t2, vec![]).await?);
    topics2.push(nodes[0].subscribe(t2, vec![nodes[1].id()]).await?);
    topics2.push(nodes[3].subscribe(t2, vec![nodes[0].id()]).await?);
    topics2.push(nodes[2].subscribe(t2, vec![nodes[3].id()]).await?);
    for topic in topics2.iter_mut().skip(1) {
        timeout(Duration::from_secs(15), topic.joined()).await??;
    }
    sleep(Duration::from_secs(2)).await;
    let order = [&nodes[1], &nodes[0], &nodes[3], &nodes[2]];
    let messages: Vec<Bytes> = order
        .iter()
        .map(|n| Bytes::from(format!("t2 from {}", n.name)))
        .collect();
    for (topic, msg) in topics2.iter_mut().zip(&messages) {
        topic.broadcast(msg.clone()).await?;
    }
    for (i, (node, topic)) in order.iter().zip(topics2.iter_mut()).enumerate() {
        let expected = messages
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, m)| m.clone())
            .collect();
        receive_all(node.name, topic, &expected).await.context("second topic")?;
    }
    println!("ok: second topic after quiet");

    drop(topics);
    drop(topics2);
    for node in nodes {
        timeout(Duration::from_secs(10), node.router.shutdown())
            .await
            .with_context(|| format!("{} did not shut down", node.name))??;
    }
    println!("ok: shutdown");
    Ok(())
}
