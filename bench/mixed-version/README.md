# mixed-version

Runs nodes of iroh-gossip 0.101.0 from crates.io and of this checkout in one
swarm, and checks that every node receives every message.

Two nodes of each version join one topic across versions, and each node
broadcasts. Then:

- a new node leaves and rejoins at once through an old node,
- all nodes broadcast again after 15 s of quiet, longer than the grace after
  which a new node retires an idle connection,
- an old node leaves, the swarm stays quiet for 15 s, and the old node rejoins
  through a new node,
- a second topic opens after the quiet period, with joins both ways between
  the versions,
- all nodes shut down.

Each step prints `ok: <step>`. A missing message fails the run, with the node
and the count.

```sh
cargo run --release
```

The crate is its own workspace, so it does not build with the crate tests.
Both versions resolve to one `iroh` version, so they share the endpoint types.
To test another old version, change the `old` dependency.
