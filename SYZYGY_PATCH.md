# Syzygy iroh-gossip patch

Base: `iroh-gossip v0.101.0` (`2ce78afe09d89d41d123f28eac19bdc831609cc8`).

## Problem

`iroh-gossip` 0.101.0 has five coupled ownership failures in its network actor:

1. `Pending.queue.is_empty()` is used as an implicit "dial in flight" flag.
   After a dial fails, the queued protocol message remains, so a later protocol
   intent appends to the non-empty queue without starting another dial.
2. Both a returned dial error and a disconnected dial task must release dial
   ownership. The upstream state has no explicit ownership bit to release.
3. `PeerState::Active` can retain a sender after the connection send loop has
   terminated. `mpsc::Sender::send` then returns the protocol message, but the
   upstream path discards it and waits for asynchronous connection cleanup.
4. Connection cleanup races with replacement connections. Only the task whose
   stable connection id still owns the active sender may clear active state;
   completion of an older connection must not remove a newer generation.
5. Dropping the last sender does not end `SendLoop`: the `Some(msg) = recv()`
   select pattern disables that branch on `None`. `connection_loop` also waits
   for both halves with `join!`, so a superseded connection can remain alive
   indefinitely and never reach stable-id cleanup.

The observed failure starts with `No addressing information available`. The
first dial fails, the queue remains non-empty, and later join/repair intents no
longer trigger a dial. In the closed-sender variant, the intent is lost before
it can even remain queued. Both paths prevent `NeighborUp`, which in turn makes
Syzygy's Trust Domain readiness barrier time out.

This is below Syzygy's transport and trust-domain boundaries, so an outer retry
cannot recover a discarded `ProtoMessage` or release actor-private dial
ownership. Adding a timeout, catch-up fallback, or Syzygy-side reconnect loop
would hide the broken state machine and create competing connection owners.

## Patch

- Add explicit `dial_in_flight` ownership to `PeerState::Pending`; queued data no
  longer doubles as scheduler state.
- On dial `Err` or `None`, clear pending dial ownership without directly deleting
  the queue. The next protocol intent can start a new dial without a busy retry
  loop. If the protocol state subsequently emits `DisconnectPeer`, its obsolete
  messages are intentionally discarded with that peer state.
- Recover the message from `mpsc::SendError`, transition `Active -> Pending`,
  retain that returned message, and queue one reconnect attempt.
- Classify connection completion by stable connection id. A matching active
  generation is removed before notifying the protocol state; an older
  connection only leaves `other_conns` and cannot clear the replacement sender.
- End `SendLoop` when all actor senders are dropped, and use `select!` in
  `connection_loop` so either the send or receive half can release the task.
- Keep focused state-transition tests in the upstream test module and retain
  Syzygy's restart/bootstrap, Docker, and commercial-role tests as consumer
  regressions.

The intended transitions are:

```text
Pending(dial=false) + message -> Pending(dial=true, queued) + queue_dial
Pending(dial=true)  + message -> Pending(dial=true, queued)
Dial Err / None               -> Pending(dial=false, queue retained)
Active + SendError(message)   -> Pending(dial=true, [message]) + queue_dial
Connection success            -> Active + drain retained queue
Last sender dropped           -> end SendLoop + reap connection task
Matching active close         -> remove active generation + PeerDisconnected
Stale connection close        -> retain the current active generation
```

This patch preserves the specific message returned by `SendError`; it does not
change iroh-gossip into a lossless transport. Messages already accepted by the
bounded Tokio channel remain subject to its best-effort disconnection semantics.
Syzygy's durable Trust Domain and history-sync guarantees remain above this
gossip transport boundary and are proven through their own intent/ACK ledgers.

## Verification

- Patch crate: 24 unit tests, four simulation integration tests, and one
  doctest pass.
- Patch crate: `cargo fmt --check` and clippy with `-D warnings` pass.
- Raw restarted-peer stress probe: 30/30 passes, with every iteration observing
  `No addressing information available -> start to dial -> NeighborUp`.
- Syzygy focused suites: gossip 5/5, trust-domain control 29/29,
  `runner_script_peers` 15/15, DB delivery 21/21, trust-domain runtime 78/78.
- Docker release image resolves `/app/crates/patches/iroh-gossip` and builds the
  production CLI.
- Fresh PRO PC / FREE Phone authority pairing E2E passes 1 scenario / 41 steps,
  including bidirectional realtime, delivery ACK, catch-up, and foreground
  `Sync Now` without repair timeout.

## Upstream tracking

- Reconnect groundwork: https://github.com/n0-computer/iroh-gossip/pull/43
- Unexpected disconnect cleanup: https://github.com/n0-computer/iroh-gossip/pull/117
- Connection-task leak report: https://github.com/n0-computer/iroh-gossip/issues/145
- Open churn cleanup PR: https://github.com/n0-computer/iroh-gossip/pull/146
- Current upstream network actor:
  https://github.com/n0-computer/iroh-gossip/blob/main/src/net.rs

PR #43 and #117 are already present in `v0.101.0`; neither handles a failed
`active_send_tx.send(...)` or separates pending queue data from dial ownership.
PR #146 is still open. This patch ports only its SendLoop/connection-task reaping
portion; it deliberately does not copy its dial-failure peer deletion because
that would discard the pending queue before a later route/intention can retry.
As of 2026-07-17, upstream `main` still has the affected paths.

This local patch is not a substitute for an upstream PR. Before removing it,
an upstream release must provide all observable guarantees: the current failed
send is retained, failed dial releases explicit ownership, superseded connection
loops are reaped, and stale completion cannot clear a newer sender. Then remove
the Cargo patch and Docker path copy, update the lockfile, confirm `cargo tree`
points to the registry release, and rerun the contract tests, 30-iteration
restart probe, and 41-step commercial-role E2E.
