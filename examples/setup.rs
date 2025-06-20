use iroh::{protocol::Router, Endpoint};
use iroh_gossip::{net::Gossip, ALPN};
use n0_snafu::ResultExt;

#[tokio::main]
async fn main() -> n0_snafu::Result<()> {
    // create an iroh endpoint that includes the standard discovery mechanisms
    // we've built at number0
    let endpoint = Endpoint::builder().discovery_n0().bind().await?;

    // build gossip protocol
    let gossip = Gossip::builder().spawn(endpoint.clone());

    // setup router
    let router = Router::builder(endpoint.clone())
        .accept(ALPN, gossip.clone())
        .spawn();
    // do fun stuff with the gossip protocol
    router.shutdown().await.e()?;
    Ok(())
}
