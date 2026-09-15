# Syzygy iroh-gossip patch

Base: `iroh-gossip v0.101.0` (`2ce78afe09d89d41d123f28eac19bdc831609cc8`).

## Problem

The network actor and broadcast protocol have coupled connection ownership
failures:

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
6. `Gossip::handle_connection` has no ownership handshake with an external
   dialer. Syzygy's authenticated pair probe can publish a route and start its
   own gossip connection while an actor-owned dial for the same peer is still
   pending. Whichever result reaches the actor second then creates a duplicate
   connection and can supersede a different connection on each side of the link.
7. A reused transport session does not run the protocol activation callback.
   Reserving external ownership before that reuse therefore leaves a pending
   peer permanently owned by `External` unless the caller explicitly releases
   the unconsumed reservation.
8. Externally dialed connections enter through the incoming-connection API and
   lose their `Dial` origin. Together with last-arrival-wins replacement, two
   peers dialing concurrently can select opposite physical connections and
   repeatedly close each other's active sender generation.
9. Incoming connections unconditionally cancel the same-peer actor dial. When
   both peers dial concurrently, each side can accept the other peer's
   non-preferred connection and cancel the local preferred connection, causing
    the two physical connections to be closed from opposite ends.
10. Recovering a failed active send into `Pending` hides the terminated active
    connection from its later task completion. Without an immediate
    `PeerDisconnected`, the protocol continues broadcasting to that retired
    neighbor and the pending queue grows until a replacement connection drains it.
11. `NeighborDown` removes eager/lazy membership but leaves that peer's scheduled
    lazy announcements. A later dispatch can recreate its pending connection and
    advertise cached messages from the retired membership.
12. Connection receive loops enqueue protocol input without a stable connection
    id. Frames already queued by a retired connection can therefore restore
    membership or trigger cached replies after its cleanup. Replacing the active
    sender alone is not retirement: the old receive loop and its queued Neighbor
    reply remain valid during the handoff. An active-sender-only input gate drops
    that reply and can prevent the topic handshake from completing.
13. A peer connection is shared by all its topics, but the protocol's disconnect
    reducer treats a successful `HashSet::remove(topic)` as if no topics remain.
    The first topic quit therefore closes the shared connection prematurely.
    Remaining topics keep their eager membership while their final disconnect
    messages lose the registered connection; subsequent broadcasts accumulate in
    a new `Pending` queue and are replayed when the peer reconnects.
14. An explicit `Join` can be silenced by an older pending `Neighbor` request.
    One side may consume a `Neighbor` as a reply and stop responding while the
    other side retains its pending flag. If only the first side observes a
    connection replacement as `PeerDisconnected`, its rejoin reaches a peer
    that is still active and pending. `send_neighbor` suppresses the response,
    leaving the rejoining side without `NeighborUp`. This Join path has no
    pending-request timer, so additional Join attempts cannot repair it.

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
- Recover the message from `mpsc::SendError`, transition `Active -> Pending`, and
  immediately notify the protocol of the disconnect before processing another
  actor input. Discard remaining same-peer outputs from the retired event batch.
  An established membership's `DisconnectPeer` removes its obsolete pending
  messages. A first Join that has not established membership retains its pending
  intent and queues one reconnect attempt, preserving initial dial recovery.
- Remove the departing peer's queued lazy announcements on `NeighborDown`.
  Other peers' announcements and the shared message cache remain available for
  normal Graft recovery. `NeighborUp` enables future broadcasts without replaying
  the disconnected interval.
- Remove only the departing topic from the shared peer's topic set and emit a
  network-level `DisconnectPeer` only when that set is empty. Other topics keep
  using the same connection and can deliver their own final disconnect messages.
  A network-level disconnect still retires every topic's membership.
- Treat an explicit `Join` as a new handshake: clear that peer's old pending
  Neighbor request before the existing `add_active` path sends its response.
  Keep active-view membership and normal Neighbor reply suppression unchanged,
  so repeated or simultaneous joins do not emit duplicate `NeighborUp` events
  or create an unbounded reply exchange.
- Tag connection input with its stable connection id and accept it while the
  peer has a live active sender and the id is registered as either its active
  connection or one of `other_conns`. Sender handoff preserves valid queued
  input from those other receive loops. Completing a connection removes its id;
  retiring the peer removes all its ids, so their delayed frames cannot restore
  membership after a disconnect or leak into a newly connected peer state.
- Classify connection completion by stable connection id. A matching active
  generation is removed before notifying the protocol state; an older
  connection only leaves `other_conns` and cannot clear the replacement sender.
- End `SendLoop` when all actor senders are dropped, and use `select!` in
  `connection_loop` so either the send or receive half can release the task.
- Model pending dial ownership explicitly as `Idle`, `Actor`, or `External`.
  The pair probe reserves `External` ownership before publishing a route, so
  protocol messages remain queued without starting a competing actor dial.
- Cancel a same-peer actor dial when external ownership is reserved. Keep the
  cancelled task registered until it is reaped, and let cancellation override
  a successful result that completed before the actor consumed it.
- Consume the reservation when the external connection is handed off. If the
  external dial fails, release the reservation and either remove an empty peer
  state or resume exactly one actor dial for its retained queue.
- Treat reuse as an unconsumed reservation: release it so a pending actor dial
  can resume, while an already-active peer remains unchanged.
- Preserve `Dial` origin for externally established connections and choose a
  deterministic winner for simultaneous dials: both endpoints keep the
  connection initiated by the lower endpoint id. Duplicate handoff of the same
  connection is idempotent, preferred connections are never downgraded, and a
  newer connection with the same origin can replace a stale restart generation.
- Do not cancel an actor dial when a non-preferred incoming connection arrives.
  Keep that incoming connection as a fallback until the preferred local dial
  succeeds or fails. Preferred incoming connections and externally established
  dial connections still cancel the redundant actor dial.
- Keep focused state-transition tests in the upstream test module and retain
  Syzygy's restart/bootstrap, Docker, and commercial-role tests as consumer
  regressions.

The intended transitions are:

```text
Pending(Idle) + message          -> Pending(Actor, queued) + queue_dial
Pending(Actor) + message         -> Pending(Actor, queued)
Pending(External) + message      -> Pending(External, queued)
Actor dial Err                   -> Pending(Idle, queue retained)
Reserve external connection      -> Pending(External) + cancel/reap actor dial
External connection success      -> Active + drain retained queue
External connection failure      -> Pending(Actor) + queue_dial, or remove empty state
External connection reused       -> release reservation; Active unchanged or Pending(Actor)
Non-preferred incoming Accept    -> activate fallback + keep preferred actor dial
Preferred incoming Accept        -> activate connection + cancel actor dial
Concurrent Dial + Accept         -> both peers retain lower-endpoint-initiated connection
Active + SendError(message)      -> Pending + immediate PeerDisconnected
Established membership down      -> discard retired outputs and Pending messages
Unestablished first Join failed  -> retain Pending(Actor, [Join]) + queue_dial
NeighborDown                     -> remove pending lazy announcements for that peer
Explicit Join with old pending   -> renew Neighbor handshake without duplicate membership
Topic quit with other topics     -> retain shared connection for remaining topics
Last topic quit                  -> send its final message, then disconnect peer
Registered input during handoff  -> finish the handshake using the new sender
Retired connection input         -> discard without changing protocol membership
Last sender dropped              -> end SendLoop + reap connection task
Matching active close            -> remove active generation + PeerDisconnected
Stale connection close           -> retain the current active generation
```

This patch preserves unfinished initial connection intents; it does not replay
messages owned by a disconnected established membership or change iroh-gossip
into a lossless transport. Messages already accepted by the bounded Tokio channel
remain subject to its best-effort disconnection semantics.
Syzygy's durable Trust Domain and history-sync guarantees remain above this
gossip transport boundary and are proven through their own intent/ACK ledgers.

## Verification

The September disconnect regressions exercise immediate actor retirement,
offline broadcasts followed by rejoin, retained first-Join dial ownership,
external reservations, queued Neighbor replies during sender handoff, rejection
of completed connections and retired input after reconnect, lazy announcement
cleanup, retained live-peer Graft recovery, and shared connections across topics.
The multi-topic regressions check continued delivery after one topic quits,
last-topic cleanup, network-wide membership retirement, and that offline
broadcasts after a peer leaves all topics cannot create a replay queue. Tests
live in `src/net/tests/disconnect.rs`, `src/proto/plumtree/test/disconnect.rs`,
and `src/proto/state/tests.rs`.

The owner-topic rejoin regression is in the Syzygy consumer's
`crates/infra/syzygy-net-trust-domain/src/tests/gossip_owner_rejoin.rs`. It routes
real `proto::topic::State` messages through two peers at fixed time, including
repeated and simultaneous joins and a one-sided disconnect. It checks restored
`NeighborUp`, the actual inbound readiness handler, and fresh broadcast delivery
without network timing, retries, or access to private protocol state. Run it with
`cargo test -p syzygy-net-trust-domain tests::gossip_owner_rejoin`, then rerun
`catchup_recent_headers_materializes_existing_pending_realtime_deferred_files`
in `syzygy-node` against this patch.

On 2026-09-15 the consumer's nine-test regression group passed, including all
three owner rejoin cases, deferred-file catch-up, and pairing commit retry.
The refreshed isolated harness retained the root dependency pins and passed
all 50 Gossip library tests and `clippy --all-targets -- -D warnings` on
`aarch64-apple-darwin`. These results do not imply a new cross-platform or
live multi-device validation run.

The following results predate these additional disconnect regressions:

- Patch crate: 28 unit tests, four simulation integration tests, and one
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
- Pending Neighbor implementation at the pinned base:
  https://github.com/n0-computer/iroh-gossip/blob/2ce78afe09d89d41d123f28eac19bdc831609cc8/src/proto/hyparview.rs
- Connection-task leak report: https://github.com/n0-computer/iroh-gossip/issues/145
- Open churn cleanup PR: https://github.com/n0-computer/iroh-gossip/pull/146
- Related open churn cleanup PR: https://github.com/n0-computer/iroh-gossip/pull/147
- Current upstream network actor:
  https://github.com/n0-computer/iroh-gossip/blob/main/src/net.rs

PR #43 and #117 are already present in `v0.101.0`; neither handles a failed
`active_send_tx.send(...)` or separates pending queue data from dial ownership.
PRs #146 and #147 are still open. This patch ports only the SendLoop/connection-task
reaping portion; it deliberately does not copy #146's dial-failure peer deletion
because that would discard the pending queue before a later route/intention can
retry. As verified on 2026-07-25, upstream `main` remains
`2ce78afe09d89d41d123f28eac19bdc831609cc8` (`v0.101.0`) and still has the
affected paths.

This local patch is not a substitute for an upstream PR. Before removing it,
an upstream release must provide all observable guarantees: unfinished initial
connection intents survive a failed send, established disconnects retire pending
and lazy output, valid input survives sender handoff while retired connection
input cannot recreate membership,
explicit Join renews a stale pending Neighbor handshake after a one-sided disconnect,
quitting one topic preserves a connection needed by another topic,
failed dial releases explicit ownership, an externally supplied
connection cancels its same-peer pending dial, reused sessions release unconsumed
reservations, simultaneous dials converge on one connection, superseded connection
loops are reaped, and stale completion cannot clear a newer sender. Then remove
the Cargo patch and Docker path copy, update the lockfile, confirm `cargo tree`
points to the registry release, and rerun the contract tests, 30-iteration
restart probe, and 41-step commercial-role E2E.
