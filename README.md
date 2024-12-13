# iroh-gossip

This crate implements the `iroh-gossip` protocol.
It is based on *epidemic broadcast trees* to disseminate messages among a swarm of peers interested in a *topic*. 
The implementation is based on the papers [HyParView](https://asc.di.fct.unl.pt/~jleitao/pdf/dsn07-leitao.pdf) and [PlumTree](https://asc.di.fct.unl.pt/~jleitao/pdf/srds07-leitao.pdf).

The crate is made up from two modules:
The `proto` module is the protocol implementation, as a state machine without any IO.
The `net` module connects the protocol to the networking stack from `iroh-net`.

The `net` module is optional behind the `net` feature flag (enabled by default).

# Getting Started

The `iroh-gossip` protocol was designed to be used in conjunction with `iroh`. [Iroh](https://docs.rs/iroh) is a networking library for making direct connections, these connections are how gossip messages are sent.

Iroh provides a [`Router`](https://docs.rs/iroh/latest/iroh/protocol/struct.Router.html) that takes an [`Endpoint`](https://docs.rs/iroh/latest/iroh/endpoint/struct.Endpoint.html) and any protocols needed for the application. Similar to a router in webserver library, it runs a loop accepting incoming connections and routes them to the specific protocol handler, based on `ALPN`.

Here is a basic example of how to set up `iroh-gossip` with `iroh`:
```rust
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
```

# License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   <http://www.apache.org/licenses/LICENSE-2.0>)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.
