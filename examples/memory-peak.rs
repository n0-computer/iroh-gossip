use std::time::Duration;

use iroh::Endpoint;
use iroh_gossip::{
    net::Gossip,
    proto::{PlumtreeConfig, TopicId},
};
use rand::RngCore;
use tokio::time::Instant;

#[tokio::main]
async fn main() -> n0_snafu::Result {
    let endpoint = Endpoint::builder().bind().await?;
    let mut config = PlumtreeConfig::default();
    let retention: u64 = std::env::var("RETENTION")
        .as_deref()
        .unwrap_or("1")
        .parse()
        .unwrap();
    config.message_cache_retention = Duration::from_secs(retention);
    config.message_id_retention = Duration::from_secs(retention * 4);
    println!("retention: {:?}", config.message_cache_retention);

    let gossip = Gossip::builder()
        .broadcast_config(config)
        .spawn(endpoint.clone());
    let topic_id = TopicId::from_bytes([0u8; 32]);
    let (sender, _receiver) = gossip.subscribe(topic_id, vec![]).await?.split();

    let limit = 5_000_000;
    let start = Instant::now();
    let fut = async {
        let mut start = Instant::now();
        let mut message = [0u8; 512];
        let mut last = 0;
        for i in 0..limit {
            rand::thread_rng().fill_bytes(&mut message[0..32]);
            sender.broadcast(message.to_vec().into()).await.unwrap();
            if start.elapsed() > Duration::from_secs(1) {
                println!(
                    "pushed {} in {:?} (remaining {})",
                    i - last,
                    start.elapsed(),
                    limit - i
                );
                last = i;
                start = Instant::now();
            }
        }
    };
    tokio::pin!(fut);
    tokio::select! {
        _ = &mut fut => {},
        _ = tokio::signal::ctrl_c() => {
            println!("cancelling");
        }
    }
    println!("took {:?}", start.elapsed());
    gossip.shutdown().await?;
    endpoint.close().await;
    Ok(())
}
