# Review: `refactor/topic-actors` (origin/main..HEAD)

Reviewed range: `f128b98` (origin/main, v0.101.0) .. `b389947`, four commits,
+2004/-1379 across 13 files.

```
b389947 fix(net): close connections superseded by a newer one from the same peer
468af70 fix: port the proto fixes from Frando/fixes
e149245 test: port the proto regression tests from Frando/fixes
1ca70d9 refactor: per-topic actors and a connection pool
```

Method: read the full diff; built and tested native (`--all-features`,
`--no-default-features`), wasm32, and clippy exactly as CI invokes them.

---

## 1. What the branch does

**Before.** One `Actor` owned everything: the endpoint, a `Dialer`, a
`HashMap<EndpointId, PeerState>`, the `proto::state::State` for *all* topics,
and a `connection_loop` task per peer with a send half and a recv half. Peer
connection state and topic membership were entangled in a single map, so
transport events (a dial failing, a connection closing) were translated
directly into protocol events for every topic at once.

**After.** Three layers:

- `ConnectionPool` (`src/net/connection_pool.rs`) — one connection per peer,
  refcounted via `ConnectionRef`, with idle timeout and a connection cap.
  Shared by all topics.
- `TopicActor` (`src/net.rs`) — one actor per topic, owning that topic's
  `proto::topic::State`, timers, neighbours, subscribers, and its own
  `remote_senders` map. Registered in a `TopicMap` shared with the accept path.
- `net_proto.rs` — `GossipSender`/`GossipReceiver` over a uni stream per
  (topic, peer), split out of the old `net/util.rs`.

The top-level `Actor` shrank to a router: endpoint address updates, API
messages, and reaping finished topic actors.

---

## 2. Wire compatibility

**Fully maintained.** Checked at every layer:

| | main | branch |
|---|---|---|
| ALPN | `/iroh-gossip/1` | unchanged (`src/net.rs:57`) |
| Stream type | uni, one per (topic, peer) | same |
| Stream preamble | length-prefixed postcard `StreamHeader { topic_id }` | same |
| Frame | `u32` big-endian length + postcard `ProtoMessage` | same |
| `ProtoMessage` | `proto::topic::Message<EndpointId>` | same type, no variant changes |

The `StreamHeader` gained `#[non_exhaustive]`, which affects downstream
construction only, not its serde representation. `proto/topic.rs` added
`Message::size()` and swapped the default RNG; neither touches the encoding.
`hyparview.rs` and `plumtree.rs` changed *when* messages are sent, never their
shape.

One deliberate off-by-one difference in the size guard: main rejected writes
where `len >= max_message_size` and reads where `size > max_message_size`; the
branch allows `len <= max` on both sides (`net_proto.rs:105`, `:122`). The
branch can therefore emit an exactly-`max`-sized message, which main's reader
already accepts. Compatible in both directions, but worth a deliberate decision
rather than an accident.

### Not wire, but breaking

- **Public API.** `Gossip` changed from a named-field struct to
  `Gossip(Arc<Inner>)`; `proto::topic::State::new` now returns
  `State<PI, StdRng>` instead of `State<PI, ThreadRng>`.
- **Metrics.** Seven `actor_tick_*` counters were removed. Any dashboard or
  alert on them breaks. See finding 4 — the replacements are not all wired up.

---

## 3. Pros

1. **The layering is right.** Connection lifecycle, topic protocol state, and
   framing were one tangle; they are now three things with clear boundaries.
   This is the change that makes the rest possible.
2. **Topics no longer share fate.** On main a single peer's connection closing
   produced `PeerDisconnected` for every topic that peer was in, because peer
   state was global. Each `TopicActor` now tracks its own senders, so a topic
   that is not using a connection is unaffected by its loss.
3. **Connection reuse across topics is now real.** The pool is keyed by peer and
   refcounted, so N topics sharing a peer share one QUIC connection and it stays
   up as long as any topic holds a `ConnectionRef`.
4. **Parallelism.** Topic actors are independent tasks; on main every topic's
   protocol work was serialized through one actor loop.
5. **The pool is borrowed from a known-good source.** It tracks
   `n0-computer/iroh-util`'s `connection_pool.rs` closely (see §6), so the core
   refcounting, idle handling, and shutdown logic are shared with upstream.
6. **`net_proto.rs` is a genuine simplification.** `net/util.rs` went from 405
   lines of hand-rolled `RecvLoop`/`SendLoop`/`RecvStreamState` machinery to a
   ~60-line `PostcardCodec` plus two thin wrappers, with no loss of function.
7. **The proto-layer fixes are real wins**, independent of the refactor: three
   unbounded-growth/spurious-send bugs fixed with tests (`468af70`).

---

## 4. Cons and risks

1. **It is unfinished.** The original history was `wip use irpc` → `fixup` →
   `improve` → `wip topicmap` → `wip progress` → `progress`. That shows: CI
   would fail on three separate jobs today (findings 1 and 2), three new metrics
   are declared and never incremented (finding 4), and there is no test anywhere
   for the pool beyond the one added in `b389947`.
2. **Test coverage did not follow the code.** `net.rs` gained a connection pool,
   a topic map, and a per-topic actor — roughly 900 lines of new concurrency —
   and the test module *shrank*. The `ManualActorLoop` harness, which let tests
   step the actor deterministically, was deleted and not replaced. The surviving
   `net::tests` are end-to-end and cannot target the new seams.
3. **New concurrency surface.** `TopicMap` is a `Mutex<HashMap + JoinSet>`
   touched from the accept path, the main actor, and topic actor completion.
   Finding 3 is one race this creates; the pattern invites more.
4. **The upstream pool was not designed for inbound connections.** The `Mode::Handle`
   path is this fork's addition and is where the one real leak lived (fixed in
   `b389947`). It remains the least-exercised part of the pool.

---

## 5. Findings

### 1. `BLOCKING` — the wasm32 build is broken

`main` builds for `wasm32-unknown-unknown`; this branch fails with 8 errors.
CI job `wasm_build` (`.github/workflows/ci.yaml:150`) runs exactly this.

```
error: future cannot be sent between threads safely
  --> src/net.rs:265  (TopicMap::get_or_init spawning TopicActor::run)
     future returned by `run` is not `Send`
     has type `TopicActor` which is not `Send`
  --> src/net.rs:342  (Options::with_on_connected)
     captured value `TopicMap` is not `Send`
```

Root cause: `TopicActor` stores `n0_future::boxed::BoxFuture`s (`connecting`,
`sender_stopped`, `remote_receivers`, `api_receivers`), and on wasm `BoxFuture`
is the non-`Send` local variant. That makes `TopicActor` non-`Send`, hence
`JoinSet<TopicActor>` and `TopicMap` non-`Send`, which then fails the
`Send + 'static` bound on `Options::with_on_connected`
(`src/net/connection_pool.rs:74`).

Reproduce:
`RUSTFLAGS='--cfg getrandom_backend="wasm_js"' cargo build --target wasm32-unknown-unknown`

### 2. `BLOCKING` — clippy fails under CI's `-Dwarnings`

`ci.yaml:17` sets `RUSTFLAGS: -Dwarnings` for the whole workflow. Three lints
fire, so all three `clippy_check` steps fail:

- `src/net.rs:380` `while_let_loop` — the `tick()` driver loop.
- `src/net.rs:521` `result_large_err` — `TopicHandle::send` returns
  `SendError<TopicMessage>`, whose `Err` variant is ≥152 bytes.
- `src/net.rs:668` `needless_borrow` — `stream.is_same_conn(&sender.conn())`.

### 3. `SHOULD FIX` — a subscribe racing a topic shutdown is silently dropped

A `TopicActor` exits when its last subscriber goes away
(`src/net.rs:625`) and returns itself into the `JoinSet`. It is only removed
from `TopicMap::topics` later, when the main actor polls `join_next`
(`src/net.rs:288`).

In that window `get_or_init` hands out the dying actor's handle, and
`handle_api_message` sends the `ApiJoin` into a channel that is still open. The
message buffers, then the `TopicActor` value is dropped by `join_next`, taking
`rx` and the buffered request with it. The caller's `subscribe()` sees its irpc
channel close with no answer. The `warn!("Topic actor dead")` at `src/net.rs:454`
does not even fire, because `send` succeeded.

On main this could not happen: `process_quit_queue` removed the topic
synchronously inside the same actor that handled `Join`.

Suggested fix: have the topic actor signal shutdown to the `TopicMap` before it
stops accepting, or re-check liveness and re-init in `handle_api_message` when
the send fails or the actor has finished.

### 4. `SHOULD FIX` — three new metrics are never incremented

`peers_dialed_success` (`src/metrics.rs:45`), `peers_dialed_failure` (`:47`) and
`peers_accepted` (`:49`) are declared and exported but never touched anywhere in
the crate. They replaced main's `actor_tick_dialer{,_success,_failure}`, which
*were* incremented. Net effect: dial observability was removed, not moved.

### 5. `SHOULD FIX` — the EOF sentinel in `PostcardCodec::recv` is the wrong kind

`src/net/net_proto.rs:120` treats `io::ErrorKind::NotConnected` as clean
end-of-stream. Per `iroh-quinn-0.14.0/src/recv_stream.rs:562`, `NotConnected` is
what `ConnectionLost`/`ClosedStream` map to. A *cleanly finished* stream yields
`Ok(0)` from `AsyncRead`, so `read_u32` returns `UnexpectedEof` — which main
correctly used as its sentinel (`main:src/net/util.rs:361`).

The two cases are inverted: a normal stream close takes the `Err` branch of
`handle_remote_message` and logs at `warn!` with an error, while an actual
connection loss is reported as a clean end. Both still produce
`PeerDisconnected`, so this is log noise rather than a protocol bug — but it
will mislead anyone debugging from logs.

### 6. `CONSIDER` — inbound streams for a not-yet-joined topic are discarded

`src/net.rs:466`: if `topics.get()` misses, `accept_loop` drops the
`GossipReceiver`, which resets the uni stream. Main kept the stream open and
simply ignored messages for unknown topics, so a stream that arrived just before
a local `subscribe()` still worked afterwards. The peer will retry, so this is
recoverable, but it adds avoidable round trips on a startup race.

### 7. `MINOR` — stale comment on shuffle scheduling

`src/proto/hyparview.rs:291` still says "this will only happen on the first
call". Since the branch added `&& !self.active_view.is_empty()`, the block now
re-arms every time the active view becomes non-empty after being empty. The
change itself is correct and is what stops a solo node waking on a shuffle timer
forever — the comment just no longer describes it.

### 8. `MINOR` — unbounded `SendQueue::Pending`

`src/net.rs:850` queues messages in an unbounded `Vec` while a dial is in
flight, with a 10s connect timeout. Carried over from main's
`PeerState::Pending { queue }`, so not a regression — noting it because the
refactor was the opportunity to bound it, and there is now one such queue per
(topic, peer) rather than per peer.

### 9. `MINOR` — a spawned task per sender just to observe `stopped()`

`src/net/net_proto.rs:41`: `GossipSender::closed()` does
`tokio::spawn(self.send.inner.stopped())`. That is one task per (topic, peer)
whose only job is to await a future, and it uses `tokio::spawn` directly rather
than `n0_future::task::spawn` used elsewhere in the crate — a portability
inconsistency independent of finding 1.

---

## 6. Divergence from `n0-computer/iroh-util`'s connection pool

The vendored pool tracks upstream `src/connection_pool.rs` closely. Substantive
differences, beyond import ordering and log-level churn:

**Added here:**

- `Mode { Connect(EndpointId), Handle(Connection) }` and
  `ConnectionPool::handle_connection`. Upstream is **connect-only** — it has no
  way to adopt an inbound connection. This is the fork's main feature addition,
  and the superseding logic it requires is where the leak fixed in `b389947`
  lived.
- `PoolHandleConnectionError` (declared; not currently returned anywhere).
- `OneConnection: Clone`, incrementing the refcount, so `ConnectionRef` can be
  `Clone`. Upstream's `ConnectionRef` is not `Clone`.
- Tracing spans (`conn_actor`, `pool`, `connect`) on the actors.

**Changed:**

- `OnConnected` takes `Connection` by value instead of `&Connection`, so the
  callback can move it into a spawned task — which is exactly what `net.rs` does
  for `accept_loop`.
- `tasks` is a `tokio::task::JoinSet` rather than
  `FuturesUnordered<BoxFuture<()>>`, so connection actors run as real tasks and
  panics surface via `res.expect(...)` instead of being silently absorbed.

**Removed:**

- Upstream's entire ~330-line test module (`connection_pool_errors`,
  `watch_close`, the echo harness). Worth restoring; `watch_close` in particular
  covers the "connection closed ⇒ next request gets a fresh one" path, which
  this fork's superseding logic now interacts with.

---

## 7. Assessment

**The direction is right and the branch should land — but not as it stands.**

The layering this introduces is the correct one, and it is not a change that can
be made incrementally: peer state and topic state were entangled by construction
on main, and separating them requires exactly this kind of rewrite. Wire
compatibility is fully preserved, which removes the largest risk a change this
size could carry — it can be rolled out and rolled back without a flag day.

What holds it back is completeness, not design. Two findings are hard CI
failures (wasm, clippy) and are mechanical to fix. Finding 3 is a genuine
concurrency bug introduced by the split and needs a real fix. Finding 4 is a
silent observability regression.

The deeper concern is test coverage. This branch replaces the most
concurrency-sensitive code in the crate and deletes the one harness
(`ManualActorLoop`) that made that code deterministically testable, leaving only
end-to-end tests that cannot reach the new seams. Findings 3 and 6 are both
things a `TopicMap`-level test would have caught. Before merge I would want:

1. Findings 1, 2, 3 and 4 fixed.
2. Upstream's pool tests restored, adapted to the `handle_connection` path.
3. A replacement for `ManualActorLoop` at the `TopicActor` level, or at minimum
   direct tests for `TopicMap` join/quit/rejoin.

Suggested split for review: the pool (vendored + `Mode::Handle` + `b389947`) is
reviewable on its own and could land first; the topic-actor split is the part
that needs the test work.

Nothing here argues for abandoning the approach. The refactor buys real things —
shared connections across topics, per-topic fate isolation, a much smaller
framing layer — and the wire stayed put.
