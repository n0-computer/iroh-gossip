# slowbench

Measures iroh-gossip with slow peers on a [patchbay](https://crates.io/crates/patchbay) lab.

One process runs every node, each in its own network namespace with its own
single-threaded runtime. Slow nodes get a rate-limited link. Node 0 publishes.
The harness uses only the public API, so it builds against any revision. It
prints one JSON line per run: delivery and age per group of nodes, per node,
the publisher rate, churn, the max RSS, and the sum of all gossip metrics.

Linux only, rootless (user namespaces).

```sh
./build-rev.sh main origin/main /tmp/slowbench
./build-rev.sh mine HEAD /tmp/slowbench
./run.sh /tmp/slowbench/out.jsonl 2 "/tmp/slowbench/bins/main /tmp/slowbench/bins/mine" \
  -- --nodes 10 --slow 1 --rate 250 --publish-secs 60 --drain-secs 30
python3 summarize.py /tmp/slowbench/out.jsonl
```

Results and their discussion: `plans/1015-slow-peers-measurements.md` in the
worktree where they were made.
