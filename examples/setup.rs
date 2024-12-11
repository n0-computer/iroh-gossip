use iroh::{protocol::Router, Endpoint};
use iroh_gossip::{net::Gossip, ALPN};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // create an iroh endpoint that includes the standard discovery mechanisms
    // we've built at number0
    let endpoint = Endpoint::builder().discovery_n0().bind().await?;

    // build gossip protocol
    let gossip = Gossip::builder().spawn(endpoint.clone()).await?;

    // setup router
    let router = Router::builder(endpoint.clone())
        .accept(ALPN, gossip.clone())
        .spawn()
        .await?;
    // do fun stuff with the gossip protocol
    router.shutdown().await?;
    Ok(())
}
