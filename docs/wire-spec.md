# The iroh-gossip protocol

Status: Draft for the iroh-gossip 1.0 series\
Revision: 7\
ALPN: `iroh-gossip/2-canary.4`\
Date: 2026-11-10

## Abstract

iroh-gossip lets iroh endpoints form a swarm per topic and broadcast
messages to all members of the swarm. It runs over QUIC connections
that iroh authenticates with the endpoint id of each side.

The protocol has three layers. The connection layer keeps at most one
connection that writes to each peer. It names each run of a node with a
random session id, and closes idle connections without a loss. The
membership layer is HyParView: per topic, it keeps a small active view
of neighbors and a larger passive view of known peers. The broadcast
layer is Plumtree: it pushes each message along a spanning tree of the
neighbors. It announces the message on the other links, so that a
receiver can repair a gap. Each sender that keeps sending gets a tree
of its own.

The protocol gives ordered delivery per topic and connection, bounded
resources per peer, and repair of lost broadcasts while a copy is
cached. It gives no storage, no acknowledgement of delivery, and no
proof of the author of a message.

## 1. Introduction

### 1.1 Scope

This document specifies the bytes on the wire and the behavior that a
peer can observe. It also documents local policy of the implementation
where that policy shapes what a peer sees, such as timeouts. Appendix
B lists all defaults and constants.

The reference implementation is the Rust crate `iroh-gossip` at commit
`abbf4a19` (branch `next/caps`). That branch is `feat/1.0` (`46d2c019`)
and one commit on top, which limits the content of a broadcast and caps
control messages and peer data. Where this document says "the
implementation", it means that code. Code references in the form
`file.rs:line` point into `src/` at that commit.

The commit `abbf4a19` changes wire rules (section 9.8), but keeps the
ALPN `iroh-gossip/2-canary.4` of the commits before it (since
`dccab553`). As a commit of
its own, it needs a new ALPN (section 8.4).

Revision 7 describes the frames of the implementation: each message is
a frame with a length, and its fields are postcard. Two earlier designs
are shelved: messages without a length (revision 6), and an encoding
of its own with a length per message (`encoding-hybrid.md`). This
revision has no pull mode (section 6.10).

### 1.2 Conventions

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and
"OPTIONAL" in this document are to be interpreted as described in BCP
14 [RFC2119] [RFC8174] when, and only when, they appear in all
capitals, as shown here.

A requirement marked *(local)* concerns resources or local decisions.
A peer cannot test it from the wire. A value marked *(policy)* is a
default of the implementation. Another implementation can choose a
different value and still interoperate.

Some rules permit more than the implementation does. A sentence that
starts with "The implementation" then says what the code does.

Byte layouts use this notation:

```text
Name {
  field   type
}
```

Section 9.2 defines the types. Hex bytes appear in lowercase. `x*n`
means the byte `x` repeated `n` times.

Each term is defined at its first use, and the glossary (section 12)
lists all of them.

## 2. Overview

### 2.1 Roles

A *node* is one running instance of the protocol on one iroh endpoint.
A *peer* is another node, as seen from a node. The *endpoint id* is
the 32-byte Ed25519 public key that iroh authenticates for each side of
a connection. It identifies a node.

Every node has the same role. A node can dial a peer and accept a
connection from a peer. A *topic* is a swarm and its broadcasts, named
by a 32-byte topic id. A node joins a topic through one or more
bootstrap peers, which the application supplies. It then learns other
members of the topic from the protocol.

Each topic is independent. A node keeps a membership state and a
broadcast state per topic. Topics share connections and nothing else:
one connection to a peer carries all topics that the two nodes share.

### 2.2 Layers

```{#fig-layers .figure}
+---------------------------------------------------------+
| Broadcast (Plumtree): GOSSIP_* messages                 |
|   eager push, IHave, Graft, Prune, trees                |
+---------------------------------------------------------+
| Membership (HyParView): SWARM_* messages                |
|   join, forward join, neighbor request/reply, shuffle   |
+---------------------------------------------------------+
| Topic streams: one unidirectional stream per topic      |
|   header with topic id and token, then message frames   |
+---------------------------------------------------------+
| Connection: control stream, session id, GOAWAY, closes  |
+---------------------------------------------------------+
| QUIC (iroh), ALPN iroh-gossip/2-canary.4                |
+---------------------------------------------------------+
```

Figure 1: The layers of the protocol.

- The connection layer (section 3) decides which connection writes to
  a peer, and closes connections that are idle or stale.
- The topic stream layer (section 4) maps the messages of a topic onto
  QUIC streams [RFC9000], and checks the auth token of the sender.
- The membership layer (section 5) keeps the neighbors.
- The broadcast layer (section 6) spreads content through the
  neighbors.

### 2.3 An example

The example uses terms that sections 5 and 6 define, and section 12
lists. Node A joins topic `T` through node B (figure 2):

1. A dials B and opens the control stream. Both sides write a control
   header with their session ids.
2. A opens a topic stream for `T` and sends `SWARM_JOIN`.
3. B adds A to its active view. It answers with `SWARM_NEIGHBOR_REPLY`
   on its own topic stream for `T`, and sends `SWARM_FORWARD_JOIN` for
   A to its other neighbors.
4. A forward join ends at node C, a neighbor of B. C sends
   `SWARM_NEIGHBOR_REQUEST` to A, and A answers with
   `SWARM_NEIGHBOR_REPLY`. A and C become neighbors.

A then broadcasts the content `hi`. A pushes `GOSSIP_MESSAGE` with
round 0 on the shared tree to its eager peers B and C. B forwards it
with round 1 to its eager peers, and sends `GOSSIP_IHAVE` to its lazy
peers. C thus gets a second copy from B. C keeps the first copy, and
sends `GOSSIP_PRUNE` to B, so B becomes a lazy peer of C in that tree.
The next broadcast of A uses A's own tree. Each node creates that tree
as a copy of its shared tree.

```{#fig-example .figure}
   A                             B                             C
   |                             |                             |
   |--- control header --------->|                             |
   |<--------- control header ---|                             |
   |--- SWARM_JOIN ------------->|                             |
   |<--- SWARM_NEIGHBOR_REPLY ---|                             |
   |                             |--- SWARM_FORWARD_JOIN ----->|
   |<------------------------------- SWARM_NEIGHBOR_REQUEST ---|
   |--- SWARM_NEIGHBOR_REPLY --------------------------------->|
   |                             |                             |
   |--- GOSSIP_MESSAGE r0 ------>|                             |
   |--- GOSSIP_MESSAGE r0 ------------------------------------>|
   |                             |--- GOSSIP_MESSAGE r1 ------>|  duplicate
   |                             |<----------- GOSSIP_PRUNE ---|
```

Figure 2: A joins through B, then broadcasts. C has only B as a
neighbor, so the walk of the forward join ends at C (section 5.4).

## 3. Connections

### 3.1 ALPN and identity

A connection of this protocol MUST use the ALPN
`iroh-gossip/2-canary.4`. The implementation lets the application set
another ALPN (`Builder::alpn`). All nodes of a network then MUST use the
same one. Each release candidate of the 1.0 series has its own ALPN,
and two versions with different ALPNs do not interoperate.

iroh authenticates the endpoint id of each side during the QUIC
handshake. The protocol uses that id as the identity of the peer. No
message carries the identity of its sender.

Either node can dial the other. The side that dials is the *dialer*.
The other side is the *acceptor*.

### 3.2 Control stream

The *control stream* is the first bidirectional stream of a
connection. The dialer MUST open it as the first stream of the
connection. Each side MUST write its control header (section 9.4) as
the first bytes of its half of the control stream. A side writes its
header without waiting for the header of the peer.

The control header carries the session id of the side that sends it,
and the connection-scope extensions that this side declares. An
*extension* is a set of additions to the protocol, with one header
parameter that declares support for it (section 8). After the header, the control stream carries control messages,
then `GOAWAY` (section 3.6), then the end of the stream (FIN). The base
protocol has no control message other than `GOAWAY`. Other control
messages come from negotiated extensions.

Timing rules:

- The dialer fails the connection with `PROTOCOL` if it cannot open the
  control stream within `HEADER_TIMEOUT` (10 s).
- The acceptor fails the connection with `PROTOCOL` if no control
  stream arrives within `HEADER_TIMEOUT`.
- A side fails the connection with `PROTOCOL` if the control header of
  the peer does not arrive in full within `HEADER_TIMEOUT` after it
  starts to read it.

A *topic stream* is a unidirectional stream that carries the messages
of one topic (section 4). A side reads topic streams of the peer only
after it read the control header of the peer. A dialer MAY open topic
streams before it read the header of the acceptor. The implementation
does so unless it saw a restart of the peer (section 3.4).

A node opens no other bidirectional stream, unless a negotiated
extension defines one. The implementation never accepts a second
bidirectional stream. Such a stream stays unaccepted until the
connection ends.

### 3.3 Sessions

A *session* is one run of a node, from start to shutdown. A node MUST
draw a new random session id of 8 bytes each time it starts. It MUST
send the same session id on all its connections in that run. A session
id needs no secrecy.

A node tracks the current session id of each peer, and the latest
older ones. The implementation keeps at most 16 older session ids per
peer (`MAX_OLD_SESSIONS`, *local*). It keeps them only while it has
state for the peer: a connection, a queued message, a dial, or a topic
that wants the peer. A topic *wants* a peer while it needs a
connection to it (section 5.1). When none of these is left, the
implementation forgets the session ids (`net/actor.rs:697-702`). The next
connection then starts as in rule 1 below.

When the control header of a peer arrives on a connection, the node
compares its session id:

1. The id is the current session of the peer, or the node knows no
   session of the peer yet. The node records the id on the connection.
2. The id is new, and the node knows another current session. The peer
   restarted. The node MUST close each connection that carries the old
   session with `STALE`. It discards its send queue to the peer, and
   reports to every topic that the peer disconnected. The old id moves
   to the list of older sessions, and the new id becomes current.
3. The id is in the list of older sessions. The connection leads to a
   former instance of the peer. The node MUST close that connection
   with `STALE`. Then:
   - If the node dialed this connection, it treats the dial as failed.
     If it has no other connection to the peer, it discards its send
     queue to the peer and reports a disconnect.
   - If the peer dialed it, the node has no other connection to the
     peer, and messages are queued, the node dials the peer again.

A *former instance* is a process that still runs with an old session
of a peer. An example is a restart that did not end the old process.
Rule 3 keeps the messages of the live instance away from it.

Rule 3 applies only while the node remembers the older sessions. After
the node forgot them, a former instance that dials in looks like the
current session (rule 1). Its connection becomes the writer (section
3.4) after its header, so messages to the peer go to the former instance and are
lost. This ends when the live instance connects: rule 2 then closes
the connection to the former instance with `STALE`. The case needs two
processes with the same endpoint id, and a time in which the node had
no state for the peer.

### 3.4 Writer selection

A node writes the messages to a peer on one connection at a time: the
*writer*. A node keeps one send queue per peer, shared by all topics.
The queue moves to the writer, and moves back when the writer's
connection ends. Thus at most one connection writes the messages of a
peer at a time.

A connection is *ready* to become the writer when:

- the node read the control header of the peer on it, or
- the node dialed it, and the node has seen no restart of the peer
  (no older session id is known).

A connection that retires is never ready. When no connection writes,
the node picks the first ready connection as the writer. After a
restart of the peer, a dialed connection waits for the header, because
it may lead to a former instance. Anything that the node wrote there
would be lost.

The writer opens one topic stream for each topic that wants the peer,
at once, even without a message to send (section 4.1). The other
connections of the peer only read.

### 3.5 Crossed dials

Two nodes can dial each other at the same time. Each node then has two
connections to the peer. The connection that the node with the higher
endpoint id dialed loses. Endpoint ids compare as 32-byte strings,
byte by byte, from the first byte.

A node decides on crossed dials once it read the control headers of
all its connections to the peer that do not retire. If at least one of
them wins, the node retires every loser (section 3.6). A connection
that already retires takes no part. Thus a dial during a retire is not
a crossed dial.

Both sides compare the same two ids, so they reach the same decision
when they see the same connections that do not retire. But a node
marks a connection as retiring only when it sends its own `GOAWAY`.
For a short time, a dial during a retire can thus look like a crossed
dial to the peer, which did not answer the `GOAWAY` yet. The peer can
then retire the new connection as a loser. No message is lost: the
queue waits, and the node dials again (section 3.6).

The retire of the loser keeps the order of messages, because the
writer moves to the winner only after the loser closed (section 4.4).
The loser thus closes with `RETIRED`, not with `STALE`. Only the
session check of section 3.3 closes with `STALE`.

### 3.6 Retire with GOAWAY

A *retire* is the orderly end of a connection that both sides agree on
with `GOAWAY`. It ends a connection without a loss. Either side can
start it. A side starts a retire when:

- the connection was idle for `retire_after` (*policy*, 10 s),
- the connection lost a crossed dial, or
- it read all streams that the peer counted in the `GOAWAY` of the peer
  (it then answers).

A connection is *idle* when all of these conditions are true:

- No topic stream is open in either direction.
- The node waits for no acknowledgement of a topic stream.
- The send queue is empty.
- The writer has no topic stream left to open.

Any change restarts the idle time. A topic stream that the node reads
counts as open, also while the node still reads its header.

To retire, a side does these steps in order:

1. It finishes all its topic streams, and opens no new one.
2. It sends `GOAWAY` on the control stream. `GOAWAY` carries the total
   number of topic streams that the side opened on this connection.
3. It writes nothing more on the connection, except the FIN of the
   control stream.

A side that receives `GOAWAY(n)` reads on until `n` streams of the
peer ended. A stream that the side stopped (section 4.3) counts as
ended. Then the side retires too, if it did not yet, and it finishes
its half of the control stream.

A side closes the connection with `RETIRED` when both conditions are
true:

- it read to the end of all streams that the peer counted in its
  `GOAWAY`, and
- the peer finished its half of the control stream.

Either side can close first. The close is orderly: each side read all
that the other sent. The queued messages stay in the queue, and go out
on the next connection. Figure 3 shows a retire that X starts.

```{#fig-retire .figure}
   X                                               Y
   |                                               |
   |  starts a retire                              |
   |--- FIN on each open topic stream ------------>|
   |--- GOAWAY(2) -------------------------------->|
   |                                               |
   |<------------ FIN on each open topic stream ---|  after 2 streams of X ended
   |<-------------------------------- GOAWAY(1) ---|
   |<---------------- FIN on the control stream ---|
   |  reads until 1 stream of Y ended              |
   |--- FIN on the control stream ---------------->|
   |                                               |
   |  each side closes with RETIRED                |
```

Figure 3: A retire that X starts. X opened 2 topic streams on the
connection, and Y opened 1. A side closes with `RETIRED` when it read
all streams that the peer counted, and the peer sent the FIN of the
control stream. Either side can close first.

Rules:

- A side MUST NOT open a topic stream after it sent `GOAWAY`.
- The count of `GOAWAY` MUST equal the number of topic streams that
  the side opened on the connection. A receiver fails the connection with `PROTOCOL` when it accepts a
  stream of the peer over that count (`net/conn.rs:456-458`). It also
  fails the connection with `PROTOCOL` when the count is lower than the
  number of streams of the peer that it accepted already
  (`net/conn.rs:512-514`).
- A side MUST NOT send any byte on the control stream after `GOAWAY`,
  except the FIN. A receiver fails the connection with `PROTOCOL` if
  it reads a byte after `GOAWAY` (`net/conn.rs:433`). This is by design:
  `GOAWAY` is the last message of a side, so the FIN after it has one
  meaning. A control message of an extension goes before `GOAWAY`.
- A side MUST NOT finish its control stream before it sent `GOAWAY`. A
  receiver fails the connection with `PROTOCOL` if the stream ends
  before `GOAWAY`.
- A side SHOULD bound the time of a retire. The implementation fails
  the connection with `BLOCKED` if the retire did not end within
  `RETIRE_TIMEOUT` (120 s, *policy*) after the side stopped writing.

A stream that the peer opened before its `GOAWAY` can arrive after it.
The implementation reads such a stream as usual.

While all connections to a peer retire, a node keeps new messages in
the queue. The first such message makes the node dial the peer once,
so that a new connection is ready when the retire ends.

### 3.7 Close codes

A node closes a connection with a QUIC application close (frame type
`0x1d` [RFC9000]). The error code tells the peer why. The reason phrase is free
text for logs, and a receiver MUST NOT act on it.

| Code | Name | The sender closes because |
|---|---|---|
| 0 | (none) | Never sent on purpose. The QUIC stack sends code 0 when the last handle of a connection drops without a close. Examples are a crash, and an application that drops the node without a shutdown. |
| 1 | `RETIRED` | Both sides finished a retire (section 3.6). |
| 2 | `STALE` | The connection carries an old session of the peer (section 3.3). |
| 3 | `SHUTDOWN` | The node shuts down, or its protocol state stopped. |
| 4 | `PROTOCOL` | The peer broke the protocol (section 7). |
| 5 | `SEND_FAILED` | A write to the peer failed, or the peer stopped a topic stream with a code other than `REJECTED`. Or an own message other than a payload was over its frame limit (section 9.8). |
| 6 | `BLOCKED` | A write, a stream open, or a retire made no progress in time. Or a control message did not fit into the send queue to the peer. |

`RETIRED` is not 0 on purpose. A crash closes with 0, and a crash must
not look like an orderly retire.

The receiver of a close acts on the code:

- `STALE`: the node reports nothing to its topics.
- `RETIRED`: the connection ended without a loss. The send queue and
  the membership state stay. The node dials the peer in two cases:
  - messages wait, and every other connection retires, or
  - no connection is left, and a topic still wants the peer.

  Thus a retire never reports a disconnect.
- Any other code, an unknown code, or a transport error: the
  connection failed. If it was the writer, the node discards its send
  queue to the peer and reports to every topic that the peer
  disconnected.

A node also discards the queue and reports a disconnect when the last
connection to a peer fails while a topic still wants the peer.

An extension that both sides declared in their control headers can
define a new close code (section 8). A node sends such a code only on
a connection where the extension is negotiated.

### 3.8 Shutdown

When a node shuts down, it leaves every topic (section 5.10). Each
writer sends what is left in its queue, finishes its streams, and
closes with `SHUTDOWN`. The implementation waits for this at most
`SHUTDOWN_DRAIN` (1 s, *policy*), and then closes the remaining
connections with `SHUTDOWN`.

A peer sees `SHUTDOWN` as a failed connection. The `SWARM_CLOSE`
messages that went out before tell it that the node left.

### 3.9 Send queue and abort

A node keeps one send queue per peer (*local*). The implementation
gives it `QueueOptions::size` bytes (1 MiB by default). The queue is at
least `MIN_QUEUE_SIZE` (eight frames of `MAX_CONTROL_SIZE`, about
132 KiB), and at least eight payload frames with content of size C. C
is the content limit of a node (section 9.8).

The queue has three sections (*local*):

- Data: payloads (`GOSSIP_MESSAGE`, `GOSSIP_MESSAGE_NEIGHBOR`) and the
  messages of a random walk (`SWARM_FORWARD_JOIN`, `SWARM_SHUFFLE`,
  `SWARM_SHUFFLE_REPLY`) may use three quarters of the queue.
- Announcements: `GOSSIP_IHAVE` may use one eighth of the queue.
- Control: every other message may use the whole queue.

A node MAY drop a payload, an announcement, or a message of a random
walk to a peer that stays connected, for example when its queue to the
peer is full. The implementation drops such a message when its section
is full:

- Plumtree announces a dropped swarm broadcast to the peer again later
  (section 6.9). A dropped neighbor message is lost.
- A dropped random walk ends early. This costs only some passive peers.
  Without the drop, a neighbor could fill the queue to an honest peer
  with shuffles that the node forwards or answers, and so abort that
  peer.

A node MUST NOT drop any other message and send the messages after it.
Examples are `SWARM_JOIN`, `SWARM_NEIGHBOR_REQUEST`,
`SWARM_NEIGHBOR_REPLY`, `SWARM_CLOSE`, `GOSSIP_PRUNE`, and
`GOSSIP_GRAFT`. The peer would act on the later messages without the
dropped one, and the two views of the link would differ. The
implementation aborts the peer instead when such a message does not
fit: it closes every connection of the peer with `BLOCKED`, discards
the queue, and reports a disconnect to every topic. Both sides then
start over.

### 3.10 Timeouts

All timeouts are *policy*. Only `HEADER_TIMEOUT` sets a limit that a
peer must meet: it must send each header in time (section 4.2).

| Name | Value | Effect when it expires |
|---|---|---|
| `HEADER_TIMEOUT` | 10 s | Control stream open or accept, control header, or topic stream header: close with `PROTOCOL`. |
| `WRITE_TIMEOUT` | 60 s | A write that makes no progress: close with `BLOCKED`. Each write that takes some bytes restarts it. |
| `OPEN_TIMEOUT` | 15 s | A stream open that waits at the stream limit of the peer. For a queued message: close with `BLOCKED`. For a topic stream that the writer opens without a message: the writer stops opening such streams on this connection. |
| `RETIRE_TIMEOUT` | 120 s | A retire that did not end: close with `BLOCKED`. |
| `retire_after` | 10 s | An idle connection starts a retire. |

A slow reader keeps its connection as long as it takes some bytes at
least once per `WRITE_TIMEOUT`.

`OPEN_TIMEOUT` fails the connection only for a queued message, on
purpose. A topic stream without a message only keeps the topic ready.
A topic without traffic can do without it when the peer allows no more
streams. A queued message must not wait without end.

## 4. Topic streams

### 4.1 Opening

A node sends the messages of a topic to a peer on a topic stream. The
stream starts with a topic header (section 9.4), then carries messages
(section 9.3).

The writer opens a topic stream:

- for each topic that wants the peer, when the connection becomes the
  writer, also without a message, and
- for a message whose topic has no open stream on the connection.

A node SHOULD have at most one open topic stream per topic and
connection. The implementation opens a new stream for a topic only
after it finished the previous one. It does not wait until the peer
read the previous one, because the reader keeps the order (section
4.4). A node keeps the stream of a topic open while the topic wants
the peer. An open stream keeps the connection from being idle (section
3.6).

The receiver reads the headers of new streams one at a time, in the
order in which it accepts the streams. It accepts the next stream only
after it read the header of the current one.

### 4.2 Header

The topic header names the topic of the stream, and can carry an auth
token. A header MUST arrive in full within `HEADER_TIMEOUT` after the
receiver accepted the stream, and MUST NOT be longer than
`MAX_HEADER_LEN` (4096 bytes). Otherwise the receiver fails the
connection with `PROTOCOL`.

The sender of a topic header MUST send the topic parameter. It MAY send
the token parameter, and the parameters of the topic-scope extensions
that it declares (section 8.2). The implementation sends the topic
first, then the token. A reader accepts the parameters in any order,
skips unknown ones, and takes the first of a repeated one. An empty
token is a token.

### 4.3 Auth tokens and refusal

A node can set one auth token per topic. It sends the token in the
header of each topic stream of that topic, to every peer. A node can
also install a verifier. The verifier gets the topic id, the
authenticated endpoint id of the peer, and the token (or none). It
returns accept or reject. Without a verifier, a node accepts every
stream.

The receiver runs the verifier on each new topic stream, after the
header and before it reads the first message. On reject:

- The receiver MUST stop the stream with the QUIC stop code `REJECTED`
  (1), and MUST NOT hand any message of the stream to the protocol.
- The stream counts as ended for `GOAWAY` (section 3.6).
- The connection stays open. Other topics are not affected.
- The receiver reports the reject to the topic. The topic drops the
  peer as for a disconnect, and cancels a pending join to it without a
  retry. It emits no event to the application, because `Refused(peer)`
  is for a refusal by the peer. The receiver reports each rejected
  stream, also one that it stops for a remembered reject.

The implementation remembers up to 1024 rejected topics per connection
(`MAX_REJECTED_TOPICS`, *local*). It stops a later stream of such a
topic without a new verifier call. Past the cap, it calls the verifier
again.

The sender of a stream that the peer stopped with `REJECTED`:

- loses the messages of that stream,
- opens no new stream for the topic on this connection, and drops the
  queued messages of the topic, and
- reports the refusal to the topic. The topic drops the peer as for a
  disconnect, cancels a pending join to it without a retry, and emits
  `Refused(peer)` to the application. If the topic later queues a
  membership message other than `SWARM_CLOSE` to the peer, the writer
  drops it and reports the refusal again (`net/conn.rs:853-861`).

A new connection starts without refusals, so the node tries the topic
again there. The membership timers limit the rate of such retries.

A node MUST NOT stop a topic stream with a code other than `REJECTED`,
unless a negotiated extension defines the code. The sender of such a
stream fails the connection with `SEND_FAILED`, not with `PROTOCOL`.

Both sides of a reject drop the link, and the node that rejects stops
sending to the peer. Thus a neighbor whose stream the node rejects
cannot stay in the active view of the node without a stream back. The
topic can later contact the peer again, for example for a forward
join. The new stream of the peer then meets the verifier, or the
remembered reject, again.

### 4.4 Ordering

The protocol keeps the order of the messages of one topic from one
sender:

- Within a stream, QUIC delivers in order.
- Within a connection, the receiver reads a newer stream of a topic
  only after the older stream of the topic ended. A newer stream waits
  until then.
- Across connections, a node writes on one writer at a time. After a
  retire, the next writer starts only after the peer read all that the
  previous writer sent, because the retire ends only then.

When a connection fails, the queue may lose messages. The node then
reports a disconnect, so the topics start over with the peer. The
protocol gives no order between topics, and no order between senders.

### 4.5 The end of a stream

A node finishes (FIN) a topic stream:

- after it sent `SWARM_CLOSE` on it, as the last message of the topic
  to that peer,
- when no topic wants the peer, after the queue is empty,
- before it sends `GOAWAY`, and
- when the node shuts down, after the queue is empty.

A reader treats the FIN as the end of the stream. The end of a stream
is not a membership event. Only `SWARM_CLOSE` and the end of a
connection change the views.

A node MUST NOT reset a topic stream. A reader fails the connection
with `PROTOCOL` when a topic stream of the peer is reset.

### 4.6 Messages for a topic the node did not join

A node can receive a stream for a topic it did not join, for example
after it left. The node accepts the stream if the verifier accepts it.
For each membership message other than `SWARM_CLOSE`, the node answers
with `SWARM_CLOSE(Shutdown)`. It ignores broadcast messages.

## 5. Membership (HyParView)

The membership layer follows HyParView [HYPARVIEW], with changes that
this section states. It runs per topic.

### 5.1 Views

Each node keeps two views of a topic:

- The *active view* holds the *neighbors* of the node: at most
  `active_view_capacity` peers (default 5).
- The *passive view* holds peers that the node knows but does not use
  as neighbors: at most `passive_view_capacity` peers (default 30).

A peer is in at most one view. A node never puts itself into a view.

Links in the active view are symmetric. A node adds a peer when it
accepts a `SWARM_JOIN` or a `SWARM_NEIGHBOR_REQUEST` of the peer, or
when the peer answers its own `SWARM_JOIN` or request. Two crossed
requests add the link on both sides without a reply. Both sides drop
the link with `SWARM_CLOSE`, or when the connection fails.

A topic *wants* a peer while it needs a connection to it, that is:

- while the peer is a neighbor,
- while the topic waits for the reply of the peer to a `SWARM_JOIN` or
  a `SWARM_NEIGHBOR_REQUEST`, or
- while a join connects to the peer.

The writer keeps a stream open for each topic that wants the peer.
When no topic wants a peer, the node finishes the streams to it, and
the connection can retire.

### 5.2 Peer info and peer data

Messages that introduce a peer carry a `PeerInfo`: the endpoint id and
optional peer data (section 9.7). Peer data is opaque to the
membership layer. It has a kind and bytes:

- Kind 0 holds the addresses of the peer: an optional relay URL and a
  set of socket addresses. A node uses them to dial the peer.
- A node MUST keep peer data of an unknown kind unchanged, and MUST
  forward it unchanged when it forwards the `PeerInfo`. It cannot use
  such data to dial.

A node sends its own peer data in `SWARM_JOIN`,
`SWARM_NEIGHBOR_REQUEST`, `SWARM_NEIGHBOR_REPLY`, and in its own entry
of `SWARM_SHUFFLE`. Peer data of kind 0 that does not decode is no
protocol error. The node ignores it.

The data of one `PeerData` MUST NOT be longer than `MAX_PEER_DATA_LEN`
(1024 bytes, section 9.8). A receiver fails the connection with
`PROTOCOL` on longer data. A node MUST keep its own peer data within
the limit. The implementation drops a relay URL that does not fit
alone, and keeps the first 32 addresses of its sorted set (31 with a
relay URL). It then drops direct addresses from the end, until its own
data of kind 0 fits (`net/address_lookup.rs:162`).

The implementation stores up to 32 addresses per peer from peer data
for 5 minutes (*local*).

### 5.3 Join

The application starts a join with a list of bootstrap peers. The
implementation does not send `SWARM_JOIN` to every bootstrap peer. It
connects first, and joins only a few:

1. It connects to up to `join_dials` bootstrap peers (default 5) at
   once, without a message. It skips itself, and peers that are
   neighbors already.
2. The first `join_contacts` peers that connect (default 2) become
   *contacts*. Each contact gets `SWARM_JOIN`. A peer that became a
   neighbor meanwhile counts as a contact without a `SWARM_JOIN`.
3. A peer is *lost* when its dial fails, or when a contact sends
   `SWARM_CLOSE`, refuses the stream, or does not answer within
   `JOIN_RETRY_DELAY` (10 s). The end of the connection to a contact
   does not count, because the reply can come on the next connection.
   For each lost peer, the join takes the next bootstrap peer. It
   connects to a new peer only while fewer peers are being dialed than
   contacts are missing.
4. Peers that connect after the join has its contacts go to the
   passive view. When the join ends, the peers that it did not try, or
   still dials, also go to the passive view.

A contact answers a `SWARM_JOIN` with `SWARM_NEIGHBOR_REPLY` (section
5.4). On that reply, the joiner adds the contact to its active view
with high priority: if the view is full, it evicts a random neighbor.

If no reply comes within `JOIN_RETRY_DELAY`, the joiner sends
`SWARM_JOIN` once more, and waits another `JOIN_RETRY_DELAY` for a
reply. The join counts the contact as lost at the retry, but a reply to
the retry still makes the contact a neighbor. A refused join gets no
retry.

A join request during a join adds its new peers to the list. The join
keeps its contacts, so no peer gets a second `SWARM_JOIN`. The node
remembers bootstrap peers for a later rejoin (section 5.8). It takes
the first 8 peers of each request (`MAX_BOOTSTRAP_PEERS`), and keeps
the latest 8 in total.

When the join has no peer left to try and the node has no neighbor,
the join failed. The implementation then emits `JoinFailed` to the
application, but not earlier than `JOIN_RETRY_DELAY` after the latest
request, and only if the node still has no neighbor then. The node
keeps trying to rejoin in the background. `JoinFailed` is local and
has no wire message.

### 5.4 Contact and forward join

A node that receives `SWARM_JOIN` from peer J:

1. cancels a pending join retry to J, and a pending
   `SWARM_NEIGHBOR_REQUEST` to J, as J restarted or the two messages
   crossed,
2. adds J to its active view with high priority, and evicts a random
   neighbor if the view is full (section 5.6),
3. sends `SWARM_NEIGHBOR_REPLY` with its peer data to J, also if J was
   a neighbor already, and
4. if J was not a neighbor before, sends `SWARM_FORWARD_JOIN` with J's
   `PeerInfo` and `ttl = active_random_walk_length` (ARWL, default 6)
   to each other neighbor.

A repeated `SWARM_JOIN` from a neighbor thus starts no new walks. Such
a `SWARM_JOIN` can come from a join retry whose first reply was late. A
peer that restarted is no neighbor at its `SWARM_JOIN`, because the
session check of section 3.3 reported its disconnect before.

A node P can receive `SWARM_FORWARD_JOIN` for peer J from peer S. P
ignores it if S is not a neighbor of P, or if J is P itself. Else:

1. If J is a neighbor, P stores J's info and sends
   `SWARM_NEIGHBOR_REQUEST(High)` to J. This renews the link.
2. Else, if the ttl is 0, or P has at most one neighbor, P stores J's
   info and sends `SWARM_NEIGHBOR_REQUEST(High)` to J. P adds J to its
   active view only when J replies. The paper adds J at once. The
   change keeps unreachable peers out of the active view. If P reached
   its contact cap (section 11.4), P adds J to its passive view and
   sends nothing.
3. Else, P forwards the message with `ttl - 1` to a random neighbor
   other than S, unless P has a pending request to J. Before that, if
   the ttl equals `passive_random_walk_length` (PRWL, default 3), P
   adds J to its passive view.

A `SWARM_NEIGHBOR_REQUEST` goes out only if no request to the same
peer is pending (section 5.5). Figure 4 shows one walk.

```{#fig-walk .figure}
          SWARM_JOIN        ttl 6         ttl 5         ttl 3          ttl 0
     J -------------> B ----------> P1 ---------> ... --------> P4 ... ---------> P7
     ^                                                          adds J to        |
     |                                                          passive view     |
     +-------------------- SWARM_NEIGHBOR_REQUEST(High) -------------------------+
     J -------------------- SWARM_NEIGHBOR_REPLY ------------------------------> P7
```

Figure 4: One random walk of a forward join, with ARWL 6 and PRWL 3.
Each arrow from B on is a `SWARM_FORWARD_JOIN` with its ttl. B starts
such a walk at each of its other neighbors. A walk ends early at a node
with at most one neighbor, or at a neighbor of J.

### 5.5 Neighbor request and reply

`SWARM_NEIGHBOR_REQUEST` asks the receiver to add the sender to its
active view. It has a priority:

- `High` (0): the receiver MUST accept. If its active view is full, it
  evicts a random neighbor. A node sends `High` when its active view is
  empty, and for a forward join (section 5.4).
- `Low` (1): the receiver accepts only if its active view has a free
  slot. Else it refuses with `SWARM_CLOSE(NoCapacity)`. An unknown
  priority reads as `Low`.

The receiver of a request:

1. cancels a pending join retry to the sender,
2. accepts or refuses as above, and
3. on accept, answers with `SWARM_NEIGHBOR_REPLY`, unless the request
   crossed its own pending request to the sender. In that case, neither
   side replies, and each side accepts or refuses by the priority rules
   above.

A node also answers a request from a neighbor with a reply.

`SWARM_NEIGHBOR_REPLY` answers a `SWARM_JOIN` or a
`SWARM_NEIGHBOR_REQUEST`. It carries the peer data of the sender. Its
receiver:

- adds the sender to its active view with high priority, if the reply
  matches a pending join or request, and sends no answer,
- ignores the reply if it does not match and the sender is a neighbor,
  and
- answers with `SWARM_CLOSE(NoCapacity)` if it does not match and the
  sender is not a neighbor.

A node keeps a request pending for `neighbor_request_timeout` (default
3 s, *policy*) from the time it sent it. When the request times out,
the node removes the peer from its passive view and tries another
passive peer (section 5.8). A failed dial or a closed connection ends
the request earlier.

### 5.6 Close and reasons

`SWARM_CLOSE` asks the receiver to drop the link. It carries a reason:

- `Shutdown` (0): the sender leaves the topic, or is not on it. An
  unknown reason reads as `Shutdown`.
- `NoCapacity` (1): the sender stays on the topic and only drops the
  link. The receiver can keep the sender in its passive view.

A node sends `SWARM_CLOSE` with `NoCapacity`:

- when it evicts a neighbor to make room,
- when it refuses a `Low` request, and
- for an unmatched reply from a peer that is not a neighbor.

A node sends `SWARM_CLOSE` with `Shutdown`:

- to each neighbor when it leaves the topic, and
- for a membership message of a topic it did not join (section 4.6).

A node on the topic sends `SWARM_SHUFFLE_REPLY` before each
`SWARM_CLOSE` to the same peer. The reply has up to
`shuffle_active_view_count + shuffle_passive_view_count` peers (default
7), at most `MAX_SHUFFLE_NODES` (15), passive peers first. This keeps
a peer that just joined from running out of contacts. The answer of section 4.6 has no such reply.

After `SWARM_CLOSE`, a node finishes the topic stream (section 4.5).
`SWARM_CLOSE` is thus the last message of the topic on that stream.

The receiver of `SWARM_CLOSE`:

- cancels a pending join retry to the sender, and counts the sender as
  lost for a running join,
- removes the sender from its active view if it is there, and keeps it
  in the passive view if the reason is `NoCapacity`,
- with `Shutdown`, removes the sender from its passive view and from
  the list of recent neighbors, and
- with `NoCapacity` and a pending request to the sender that the
  sender did not accept, notes the sender as *refused*.

Later `Low` requests skip refused peers until a neighbor leaves, or
until a maintenance pass clears the set. A pass clears it when the node
is not short, and when it starts a refill of a short node (section
5.8).

### 5.7 Shuffle

A node shuffles to keep its passive view a live sample of the swarm.
The first shuffle comes 1 s after the topic starts. The interval
doubles after each shuffle up to `shuffle_interval` (default 60 s), and
each delay varies by up to 25 percent either way (*policy*).

To shuffle, a node picks a random neighbor N and sends it
`SWARM_SHUFFLE` with:

- `origin`: its own endpoint id,
- `nodes`: up to `shuffle_active_view_count` (3) neighbors and up to
  `shuffle_passive_view_count` (4) passive peers, none of them N, and
  its own `PeerInfo` last. A shuffle MUST NOT carry more than
  `MAX_SHUFFLE_NODES` (15) nodes, its own entry included (section
  9.8). The implementation lowers the two counts when it starts, so
  that their sum is at most 14. It keeps the share of each view, and
  logs a warning (`proto/hyparview.rs:532-545`).
- `ttl`: `shuffle_random_walk_length` (default 6).

A node that receives `SWARM_SHUFFLE` from peer S ignores it in two
cases: S is not a neighbor, or the origin is the node itself. Else:

- If the ttl is 0, or the node has at most one neighbor, it adds the
  nodes to its passive view. It then sends `SWARM_SHUFFLE_REPLY` to the
  origin, with up to as many peers as the shuffle carried, at most
  `MAX_SHUFFLE_NODES`. Passive peers come first, and neighbors fill up the rest. The node picks them
  after it added the received nodes, so the reply can name peers of the
  shuffle, the origin included. If the node reached its contact cap
  (section 11.4), it sends no reply.
- Else it forwards the shuffle with `ttl - 1` to a random neighbor
  other than the origin and S. If no such neighbor exists, it drops the
  shuffle.

The reply goes to the origin directly, which can need a new connection.
The node caps the number of such contacts (section 11.4). The origin
adds the peers of the reply to its passive view. A node with a full
passive view evicts a random passive peer for each new one.

### 5.8 Failure handling and refill

A node detects the failure of a neighbor by the end of the connection
(section 3.7), or by a `SWARM_CLOSE`. It then removes the neighbor and
starts a *refill*: it sends `SWARM_NEIGHBOR_REQUEST` to a random
passive peer.

A node is *short* when it has less than half of its active view
(`is_short`). Refill rules (*policy*, except the priority):

- The priority is `High` if the active view is empty, else `Low`.
- A refill never counts more neighbors and open requests than the
  active view can take.
- When a dial fails or a request times out, the refill asks the next
  passive peer at once.
- After a refusal, only a node without neighbors or a short node asks
  the next passive peer. Only such a node thus asks until a peer
  accepts, as in the paper.
- A node that is not short stops at the first refusal, also in a
  refill after a neighbor left. One free slot is normal, and the next
  maintenance pass ends that refill.
- After a neighbor left, the refill has no count limit. A short node
  sends at most 3 requests per refill (`MAX_REQUESTS_PER_REFILL`).
- A `SWARM_SHUFFLE_REPLY` also starts a refill. If the reply answers a
  shuffle and brings new peers, a short node with no requests left gets
  3 more.

A maintenance pass runs every 5 s (`MAINTENANCE_INTERVAL`):

- A node with both views empty rejoins through its remembered bootstrap
  peers and one random recent neighbor. It does so in the passes 1, 2,
  4, and 8 of such a period, and then in every 12th pass.
- A node with passive peers but no neighbor starts a refill with high
  priority in every pass.
- A short node starts a refill of up to 3 requests. It uses the same
  backoff schedule, counted from the last time it gained a neighbor. It
  counts no pass while a refill after a neighbor left runs.

A node remembers its latest 8 neighbors (`MAX_RECENT_NEIGHBORS`) for
rejoins. A peer leaves this list when it closes with `Shutdown`, or
when newer neighbors push it out. A failed connection does not remove
it, because the node itself may be the one that lost its network.

### 5.9 Timers

All values are *policy*. Appendix B has the full list.

| Timer | Value | Action |
|---|---|---|
| First shuffle | 1 s, doubling up to 60 s, jitter ±25 % | Send `SWARM_SHUFFLE`. |
| Maintenance | 5 s | Rejoin or refill (section 5.8). |
| Neighbor request | 3 s | Drop the peer from the passive view, refill. |
| Join retry | 10 s | Send `SWARM_JOIN` once more, take the next bootstrap peer. |
| Forget join | 10 s after the retry | Stop waiting for a reply. |
| Join deadline | 10 s after the latest request | Emit `JoinFailed` if the join failed and no neighbor exists. |

### 5.10 Leaving a topic

To leave a topic, a node sends `SWARM_SHUFFLE_REPLY` and then
`SWARM_CLOSE(Shutdown)` to each neighbor. It then finishes the streams
of the topic, and drops all state of the topic. It cancels its pending
requests without a message.

## 6. Broadcast (Plumtree)

The broadcast layer follows Plumtree [PLUMTREE], with a tree per
sender, and changes that this section
states. It runs per topic, over the active view of the membership
layer.

### 6.1 Message ids and dedup

The *content* of a broadcast is the bytes that the application gave.
The *origin* of a broadcast is the node that broadcast it first. A
*swarm broadcast* reaches every member of the topic. Section 6.2 gives
the scopes. The *message id* of a broadcast is the BLAKE3 hash
[BLAKE3] of its content, 32 bytes. The id is not on the wire. Each receiver MUST
compute it from the content. Two broadcasts with the same content are
the same message.

A node remembers the ids of the swarm broadcasts it saw for
`message_id_retention` (default 90 s, *policy*). A copy of a known id
is a *duplicate*. A node delivers each swarm broadcast to the
application at most once within that time.

To answer grafts, a node caches the content of each swarm broadcast it
accepted for `message_cache_retention` (default 30 s, *policy*).

### 6.2 Scopes

A broadcast has one of two scopes. Each scope has its own message type.

- Swarm (`GOSSIP_MESSAGE`, 0x10): the message reaches every member of
  the topic. It has a round and a tree.
- Neighbors (`GOSSIP_MESSAGE_NEIGHBOR`, 0x14): the message goes to the
  direct neighbors of the origin only. It has no round and no tree. A
  receiver delivers it and does not forward it.

A *neighbor message* is a broadcast of the neighbors scope.

A node does not deduplicate neighbor messages, does not cache them,
and does not repair them. A resent neighbor message is thus delivered
again.

The id covers only the content, not the scope. So a neighbor message
whose id matches a swarm broadcast that the node saw is a duplicate,
and the node drops it without delivery. An application that sends the
same content in both scopes sees this.

A new scope needs a new message type, from a negotiated extension
(section 8). A node sends a message of that scope only to peers with
which the extension is negotiated.

### 6.3 Rounds

The *round* of a swarm broadcast counts the hops before the current
send. The origin sends round 0. A node that forwards a message sends
the round it received plus one. The round stops at 65535. Announcements
carry the round that the node would send.

The round is not authenticated, and a peer can send any round. Rounds
only steer the tree optimization (section 6.8).

### 6.4 Eager and lazy push

A *tree* is one broadcast tree of a topic, named by an 8-byte tree id
(section 6.7). Each tree of a node splits its neighbors into two sets:

- An *eager peer* gets the content of the messages of the tree
  (`GOSSIP_MESSAGE`).
- A *lazy peer* gets only announcements of the ids
  (`GOSSIP_IHAVE`).

A new neighbor starts as an eager peer in every tree. A neighbor that
leaves leaves every tree. Figure 5 shows one tree.

```{#fig-tree .figure}
                 O   (origin)
                / \
               /   \
              A . . B
             / \     \
            /   \     \
           D . . E     F

     ----   eager link: GOSSIP_MESSAGE with the content
     . . .  lazy link: GOSSIP_IHAVE with the id, round, and tree
```

Figure 5: One tree. Eager links form a spanning tree. The other links
of the active views are lazy, and carry announcements that repair a
gap.

When a node broadcasts content to the swarm, it:

1. picks the tree (section 6.7),
2. records the id as seen, and caches the content,
3. sends `GOSSIP_MESSAGE` with round 0 to each eager peer of the tree,
   and
4. queues an announcement for each lazy peer of the tree.

When a node receives a new swarm broadcast from S, it:

1. asks the local filter, if any (section 6.11),
2. records the id as seen, and stops here if the filter rejected it,
3. caches it with the round plus one,
4. creates the tree if it does not know it (section 6.7),
5. makes each peer that it grafted for this message eager in the tree,
6. sends `GOSSIP_MESSAGE` to each eager peer of the tree, except S and
   the grafted peers, as these have it,
7. queues an announcement for each lazy peer of the tree, except S,
   and
8. delivers the content to the application.

A node batches its announcements. After `dispatch_timeout` (default
5 ms, *policy*), it sends each lazy peer one `GOSSIP_IHAVE` with all
announcements queued for it. A batch that does not fit
`MAX_CONTROL_SIZE` splits into several messages (section 6.5).

A node delivers and forwards a swarm broadcast from a peer that is not
a neighbor. Such a peer is often a former neighbor whose messages were
still in flight. Such a message changes no eager or lazy set.

A neighbor message goes to every neighbor, eager or lazy.

### 6.5 IHave and Graft

`GOSSIP_IHAVE` announces messages. Each entry has the id, the round,
and the tree. A `GOSSIP_IHAVE` MUST fit `MAX_CONTROL_SIZE` (section
9.8). The protocol has no other limit on the number of its entries. An
entry takes at most 43 bytes (32 id, 3 round, 8 tree). The
implementation thus sends at most `floor((16384 - 6) / 43) = 380`
entries in one `GOSSIP_IHAVE`, and splits a longer batch
(`proto/plumtree.rs:708-727`). For each entry with an id that the node
did not see:

1. The node records the sender as an announcer of the id, with its
   round, once per sender.
2. If no graft chain runs for the id, the node starts one: it starts a
   graft timer of `graft_timeout_1` (default 80 ms, *policy*).

A *graft chain* asks the announcers of one id, one after the other.
When its timer expires and the message did not arrive, the node:

1. takes the first announcer in the order of arrival. An announcer
   leaves this list when the node asks it, and comes back only if it
   announces the id again,
2. makes it an eager peer in the tree of the announcement,
3. sends it `GOSSIP_GRAFT` with the id and the tree, and
4. starts a second timer of `graft_timeout_2` (default 40 ms), after
   which it asks the next announcer the same way.

The chain ends when the message arrives, or when no announcer is left
at a timer. A node runs at most one graft chain per id. An
announcement during a chain only adds its sender to the list. Figure 6
shows a chain.

```{#fig-graft .figure}
   node                        L1                          L2
   |                           |                           |
   |<--------- GOSSIP_IHAVE ---|                           |
   |  start graft_timeout_1    |                           |
   |<------------------------------------- GOSSIP_IHAVE ---|  L2 joins the list
   |  80 ms, no message        |                           |
   |--- GOSSIP_GRAFT --------->|                           |  L1 becomes eager
   |  start graft_timeout_2    |                           |
   |  40 ms, no message        |                           |
   |--- GOSSIP_GRAFT ------------------------------------->|  L2 becomes eager
   |<----------------------------------- GOSSIP_MESSAGE ---|  the chain ends
```

Figure 6: A graft chain with two announcers and the default timers.
The first graft got no answer in time, so the node asks the next
announcer.

A node that receives `GOSSIP_GRAFT` from a neighbor:

1. makes the sender an eager peer in the tree. It does not do so if
   the tree is unknown and not the shared tree.
2. if the graft names an id and the node has the content in its cache,
   sends `GOSSIP_MESSAGE` with the cached content, round, and tree.

A graft without an id only changes the tree. A graft for an id that
the node no longer caches gets no answer. The requester then asks the
next announcer.

A node ignores `GOSSIP_IHAVE`, `GOSSIP_GRAFT`, and `GOSSIP_PRUNE` from
a peer that is not a neighbor.

### 6.6 Prune and the prune rule

`GOSSIP_PRUNE` names a tree. Its receiver makes the sender a lazy peer
in that tree. It does so only if the sender is a neighbor, and the
receiver knows the tree or it is the shared tree. The sender of a
prune makes the receiver lazy in the same tree.

A node that receives a duplicate swarm broadcast from S applies these
rules in order:

1. If the duplicate is a late reply to a graft, the node handles it as
   in section 6.8. It sends no prune to S.
2. If S is not a neighbor, the node sends `GOSSIP_PRUNE` for the tree
   of the duplicate.
3. If the node does not know the tree, or S is not eager in it, the
   node sends `GOSSIP_PRUNE`.
4. If S is eager, the node prunes S, as in the paper, with one
   exception. The exception is a message that the node first saw less
   than `graft_timeout_1` after its last prune by this rule in that
   tree.
   That message was in flight during the prune. Its duplicate may come
   from the cycle that the last prune cut, and a second cut would split
   the tree. The exception does not hold for the message that created
   the tree, as its duplicates come from its flood.

A duplicate creates no tree. A copy can name any tree, because the id
covers only the content.

### 6.7 Trees per sender

A topic has many trees. Each swarm broadcast names its tree in its
`tree` field. A node keeps an eager set and a lazy set per tree.

The *shared tree* has the id `00*8`. Every other tree id is a value
that an origin picks for its own broadcasts. Every node that forwards
the message, or announces it, MUST copy the tree id unchanged. A
receiver does not derive or check the id. Only the origin computes it,
so the derivation is local to the origin, and any derivation
interoperates.

An origin SHOULD use a tree id that is stable across its broadcasts,
not `00*8`, and unlikely to match the id of another origin. The
implementation uses the first 8 bytes of the BLAKE3 hash of the
endpoint id of the origin (*policy*, appendix A.4). Two origins whose
ids collide share a tree. This costs latency, not delivery.

The switch rule (*policy*): a node names the shared tree for its first
`source_tree_after` swarm broadcasts (default 1), and its own tree from
then on. The count starts again when the node did not broadcast for
300 s. A value of 0 gives each sender its own tree at once. The first
message of each sender then floods at each node that has no shared
tree.

Tree creation:

- Only the first copy of a swarm broadcast creates a tree that the
  node does not know. The new tree is a copy of the eager and lazy
  sets of the shared tree. Without a shared tree, every neighbor is
  eager.
- `GOSSIP_IHAVE`, `GOSSIP_GRAFT`, and `GOSSIP_PRUNE` act only on known
  trees and on the shared tree. They never create another tree.

A node keeps at most `MAX_TREES` (1024, *local*) trees per topic. A new
tree past the cap evicts the least recently used tree, never the shared
one. A node drops a tree other than the shared tree when no message
used it for 300 s (`TREE_RETENTION`). The shared tree stays as long as
the topic. In a steady topic, most traffic uses the trees of the
origins. If the shared tree expired, the next new tree would start
with every neighbor eager, and its first message would flood.

A dropped tree starts again as a copy of the shared tree. Only a node
that has no shared tree yet starts a new tree with every neighbor
eager.

The copy makes the switch of a sender to its own tree cheap. Its first
message on the new tree follows the shared tree, so no flood happens.
The tree then follows the prunes and grafts of that sender's traffic.

### 6.8 Tree optimization and late graft replies

Tree optimization follows the paper. A node can receive a new swarm
broadcast from neighbor S after announcements of it. The node then
compares rounds. An announcement carries the round that its sender
would forward. So the node compares the best announced round with the
round that it forwards: the round of the eager copy plus one. If the
best announced round is lower by `optimization_threshold` (default 7)
or more, the node:

1. sends `GOSSIP_GRAFT` without an id to that announcer and makes it
   eager, if it is not eager yet, and
2. sends `GOSSIP_PRUNE` to S and makes S lazy.

The node skips this step in two cases:

- the message came from a peer that is not a neighbor, or
- the node grafted a peer for this message, and that peer announced a
  round no higher than the round of the eager copy. The eager copy came
  from another peer, and the late reply of the grafted peer decides.

Late graft replies: a node can graft peer G for a message, and then
get the eager copy from F before G answers. When G's copy arrives, the
node checks three conditions:

- G's copy took no more hops than F's copy,
- F is not a grafted peer of this message, and
- both F and G are eager in the tree.

If all are true, the node keeps G eager, and sends `GOSSIP_PRUNE` to
F. G announced the message more than a graft timeout before F's copy
came, so its path is faster. Else the copy of G is a plain duplicate.
A second reply from the same peer, after a retried graft, needs no
action.

### 6.9 Dropped messages

A full send queue can drop a `GOSSIP_MESSAGE`, a
`GOSSIP_MESSAGE_NEIGHBOR`, or a `GOSSIP_IHAVE` to a neighbor (section
3.9). The node then announces the id of each such
swarm broadcast that it still caches to that neighbor again. It does
so at the next tick of 1 s, with at most 64 ids per neighbor and tick.
It keeps at most 256 dropped ids per neighbor and 4096 per topic
(*local*). A dropped neighbor message is lost.

### 6.10 Pull mode

This revision has no pull mode. Revision 6 described one, with the
message type `GOSSIP_PULL` (`0x15`). The draft PR #239 adds it on top
of the series of section 1.1, with a new ALPN. In this revision, `0x15`
is an unknown type, and a reader fails the connection on it (section
7.1).

### 6.11 Message filter

A node can install a filter (*local*). The filter sees the topic, the
id, the content, the scope, and the peer that delivered the message.
That peer is not always a neighbor (section 6.4). The filter runs once
per new id, before the node caches, forwards, announces, or delivers
the message. Own broadcasts skip the filter.

A rejected swarm id counts as seen. The node stops its grafts for it,
and does nothing more. The node keeps the verdict for a neighbor
message for `message_id_retention`, so that a copy does not run the
filter again. A swarm broadcast with the id of such a neighbor message
gets the kept verdict without a filter call.

A filter is not visible on the wire, except that a rejected message
does not travel on.

## 7. Error handling

### 7.1 Classes

A reader handles each problem in one of four ways:

1. **Skip.** The reader ignores the item and reads on. Only three
   items are skipped: an unknown header parameter, the bytes after the
   known fields of a message, and a payload over the limits of the
   reader (section 9.8). A payload is a `GOSSIP_MESSAGE` or a
   `GOSSIP_MESSAGE_NEIGHBOR`. The reader discards the frame of such a
   payload, and only that message is lost.
2. **Default.** The reader takes an unknown value as a defined default.
3. **Refuse the stream.** The verifier rejects the stream (section 4.3).
   Only that stream ends.
4. **Fail the connection.** The node closes the connection with a close
   code. The other topics on the connection fail with it.

A *protocol error* is a violation of the encoding or of the connection
rules. A protocol error always fails the connection with `PROTOCOL`.

A stream *permits* the message types of the base protocol for that
stream. It also permits the types of the extensions that are
negotiated for it (section 8.2). A message type that is not permitted on a stream is a
protocol error. A peer sends a new type only after both sides declared
its extension. So an unknown type means that the peer broke this rule.

A membership or broadcast message that is well formed but makes no
sense in the current state is no protocol error. The receiver handles
it as sections 5 and 6 say. Often it ignores the message.

### 7.2 Conditions

| Condition | Reaction | Code |
|---|---|---|
| Message type not permitted on the stream: unknown, of an extension that is not negotiated, or of the other stream | Fail | `PROTOCOL` |
| Payload frame over the payload limit of the reader (section 9.8) | Skip the rest of the frame without keeping it, lose only that message | |
| Payload frame within the payload limit, with content longer than C | Skip, lose only that message | |
| Frame of any other type over `MAX_CONTROL_SIZE` (16384 bytes), on a topic stream or on the control stream, unknown types included | Fail | `PROTOCOL` |
| Peer data longer than 1024 bytes, or a `SWARM_SHUFFLE` or `SWARM_SHUFFLE_REPLY` with more than 15 nodes (section 9.8) | Fail | `PROTOCOL` |
| Bytes after the known fields of a message | Ignore them | |
| Frame of length 0, or with a length shorter than its type code | Fail | `PROTOCOL` |
| Fields that do not decode within the frame: cut short, an option byte other than `00` or `01`, a varint over its type (section 9.1), or an endpoint id that is no valid Ed25519 point | Fail | `PROTOCOL` |
| Frame length that is no valid `u64` varint | Fail | `PROTOCOL` |
| Stream that ends inside a frame | Fail | `PROTOCOL` |
| Header parameter unknown | Skip | |
| Header parameter repeated | Take the first | |
| `Priority` or `CloseReason` value unknown | Default: `Low`, `Shutdown` | |
| `Priority` value that does not fit a `u32` | Fail | `PROTOCOL` |
| Peer data of an unknown kind | Keep and forward unchanged | |
| Peer data of kind 0 that does not decode | Ignore the addresses | |
| Verifier rejects a stream | Stop the stream with `REJECTED` | stop code 1 |
| No control stream within 10 s | Fail | `PROTOCOL` |
| Control header: wrong kind, no session, session not 8 bytes, longer than 4096 bytes, late, or cut short | Fail | `PROTOCOL` |
| `GOAWAY` without its count | Fail | `PROTOCOL` |
| Control stream ends before `GOAWAY`, has bytes after `GOAWAY`, or is reset | Fail | `PROTOCOL` |
| Stream of the peer over the count of its `GOAWAY`, or a `GOAWAY` count below the streams already accepted | Fail | `PROTOCOL` |
| Unidirectional stream with a header kind other than `TOPIC`, and of no negotiated extension | Fail | `PROTOCOL` |
| Topic header: no topic, topic not 32 bytes, longer than 4096 bytes, late, or cut short | Fail | `PROTOCOL` |
| Header parameter of a known extension with a value that does not decode | Fail | `PROTOCOL` |
| Topic stream reset by the peer | Fail | `PROTOCOL` |
| Peer stops a topic stream with a code other than 1, and of no negotiated extension | Fail | `SEND_FAILED` |
| Write to the peer fails | Fail | `SEND_FAILED` |
| Own message other than a payload over `MAX_CONTROL_SIZE` | Fail before writing it | `SEND_FAILED` |
| Own payload over the own payload limit | Drop it before writing (*local*) | |
| Own broadcast with content longer than C | Refuse it in the API, or drop it before sending (*local*) | |
| No write progress for 60 s, or an open for a queued message waits 15 s | Fail | `BLOCKED` |
| Retire not done in 120 s | Fail | `BLOCKED` |
| Message other than a payload, an announcement, or a random walk does not fit the send queue | Abort the peer | `BLOCKED` |

A node closes with the first failure it sees. If the peer closed the
connection first, the close code of the peer stands.

## 8. Extensibility

### 8.1 Model

The protocol grows by extensions, not by new ALPNs. An extension adds
items that an older reader does not know, such as message types,
control messages, stream kinds, or codes. An older reader fails the
connection on such an item. So a node sends the item only to a peer
that declared support for the extension.

A node *declares* an extension with a parameter in a header. An older
reader skips the unknown parameter, and thus declares nothing in its
own header. A newer node then sees no declaration, and keeps to the
base protocol. Both nodes interoperate on the same ALPN.

### 8.2 Declaration and negotiation

Each extension has one header parameter id, which its specification
assigns, and a scope:

- **Connection scope.** The parameter goes in the control header. The
  extension is negotiated on the connection when both control headers
  carry it. It then applies to every stream of the connection.
- **Topic scope.** The parameter goes in topic headers. The extension
  is negotiated for one topic on one connection when a topic header of
  that topic from each side carries it. It then applies to the topic
  streams of that topic on that connection.

The value of the parameter is empty, unless the extension defines a
value, such as a version or a limit. A reader that knows the extension
and cannot decode the value fails the connection with `PROTOCOL`.

Rules:

- A node MUST NOT declare an extension that it does not implement.
- A node MUST send the same topic-scope parameters in every topic
  header of one topic on one connection.
- A node MUST NOT send an item of an extension before the extension
  is negotiated. A node knows this only after it read the header of
  the peer that carries the declaration. A side writes its own header
  without waiting for the header of the peer (section 3.2). But it
  waits for that header before it sends such an item.
- A topic-scope extension needs a topic stream from the peer, because
  the declaration of the peer comes in its header. Without such a
  stream, the extension is not negotiated.
- An extension MUST leave the base protocol valid on a link where it
  is not negotiated.
- An extension that defines a stream kind MUST say how its streams
  count for the idle time and for `GOAWAY` (section 3.6).

An extension can define these items:

- message types on topic streams,
- control messages before `GOAWAY`,
- stream kinds, unidirectional or bidirectional. An extension that
  defines a stream kind has connection scope,
- close codes, with connection scope,
- stop codes of topic streams, and
- new rules for existing messages, if the old and the new rule can
  both apply on one link.

Negotiation covers one link. A node can relay data of an extension,
such as a broadcast of a new scope (section 6.2). It sends such data
only to peers with which the extension is negotiated. A change that
every node of a topic must apply cannot be negotiated per link.

The reader rules follow from the model, by design:

- A message of a type that is not permitted on the stream fails the
  connection (section 7.1).
- A unidirectional stream with an unknown kind fails the connection.
- Any byte after `GOAWAY` fails the connection (section 3.6).

### 8.3 Changes without negotiation

Some changes are safe without a declaration, because an older reader
skips or defaults the new item. Each message has a frame with a length
(section 9.3), so a reader can skip the bytes after the fields that it
knows.

- **Fields at the end of a message.** A new field after the last field
  of an existing type, whose absence leaves the base behavior valid. An
  older reader ignores it. The message with the new field MUST stay
  within the limit of its type (section 9.8). A field in another place,
  or a field that a reader must not ignore, needs a new type or a new
  ALPN.
- **Header parameters.** A parameter whose absence leaves the base
  behavior valid. The parameters that declare extensions are of this
  kind.
- **Enum values.** A new `Priority` or `CloseReason`
  value that is safe to read as the default of section 7.2. A new
  `Priority` value MUST fit a `u32`. A value that is not safe to read
  as the default needs an extension.
- **Peer data kinds.** A new kind. Peer data travels over many links,
  so it cannot wait for a negotiation. Older nodes keep and forward it,
  but cannot use it to dial.

### 8.4 Changes that need a new ALPN

These changes need a new ALPN, because no declaration can negotiate
them:

- A change that a reader needs before it can read the declarations of
  the peer. Examples are the varints, the frame layout, and the header
  layout. Others are the header kinds `CONTROL` and `TOPIC`, and the
  parameters `SESSION` and `TOPIC_ID`.
- A change of a fixed limit: `MAX_CONTROL_SIZE`, `MAX_PEER_DATA_LEN`,
  `MAX_SHUFFLE_NODES`, or `MAX_HEADER_LEN` (section 9.8). With a higher
  limit, an older reader fails the connection on a larger item. With a
  lower limit, a newer reader fails the connection on the items of an
  older sender.
- A change to the set of types that a reader skips over its limit
  (only the payloads), or to the rule that a payload frame may be as
  large as a broadcast of C at the largest round.
- A change to the meaning or the layout of an existing message, field,
  limit, or code, if the old and the new form cannot both stay valid on
  one link. An extension can add a new type next to the old one
  instead.
- A change that every node of a topic must apply, such as the message
  id (section 6.1).
- A change to the session and retire rules that a peer must apply
  before the control headers are read.

The tree id of an origin is no wire value (section 6.7). A change of
its derivation needs no new ALPN.

The implementation mints a new ALPN (`iroh-gossip/2-canary.N`) for
each release candidate of the 1.0 series.

## 9. Encoding

### 9.1 Varints

All integers are unsigned LEB128 varints. Each byte holds seven value
bits, the least significant group first. Bit 7 is set on each byte
except the last.

| Type | Max bytes | Max value of the last byte |
|---|---|---|
| `u16` | 3 | `0x03` |
| `u32` | 5 | `0x0f` |
| `u64` | 10 | `0x01` |

A sender MUST use the shortest form. A reader MUST reject a varint
that is longer than the maximum, or that does not fit its type. A
reader MAY accept a form that is longer than needed but within the
maximum. The implementation accepts such forms.

Examples: `0` is `00`. `127` is `7f`. `128` is `80 01`. `300` is
`ac 02`. `16383` is `ff 7f`. `16384` is `80 80 01`. `2^64 - 1` takes
10 bytes.

These are the varints of postcard [POSTCARD]. Frame lengths, header
kinds, parameter ids, and parameter lengths are `u64` varints.

### 9.2 Field types

The fields of a message are the postcard encoding [POSTCARD] of the
fields of its type, in order. They use these types:

| Type | Encoding |
|---|---|
| `varint` | A `u64` varint. |
| `u32` | A `u32` varint, at most 5 bytes. |
| `u16` | A `u16` varint, at most 3 bytes. |
| `[u8; N]` | N bytes, no length. |
| `bytes` | `length: varint`, then that many bytes. |
| `Vec<T>` | `count: varint`, then that many values of `T`. |
| `Option<T>` | One byte: `00` for none, or `01` then `T`. Any other byte is an error. |
| `EndpointId` | 32 bytes. They MUST be a valid compressed Ed25519 point. |
| `PeerData` | `kind: varint`, then `data: bytes` (section 9.7). |
| `PeerInfo` | `id: EndpointId`, then `data: Option<PeerData>`. |
| `TreeId` | `[u8; 8]`. |
| `MessageId` | `[u8; 32]`. |

A fixed-size field has no length. A variable field carries its own
length or count. The frame of the message bounds each of them (section
9.3). Peer data and the nodes of a shuffle also have limits of their
own (section 9.8).

### 9.3 Frames and messages

Each stream carries a sequence of frames. A frame is a length, then
that many bytes of payload. The first frame of a stream is the header.
Each later frame carries exactly one message:

```text
Stream {
  header    Frame        (payload: header, section 9.4)
  messages  Frame*       (payload: Message, until the end of the stream)
}

Frame {
  length   varint        (the number of bytes of the payload)
  payload  [u8; length]
}

Message {
  type    varint
  fields  ...            (postcard, layout per type, sections 9.5 and 9.6)
}
```

A sender MUST NOT send a message frame of length 0, because it has no
type. A sender of the base protocol sends no bytes after the fields of
a message. A reader ignores the bytes after the fields that it knows
(section 8.3). A type that is not permitted on the stream fails the
connection (section 7.1).

A reader reads each message frame in these steps:

1. It reads the length and the type, and picks the limit of the type
   (section 9.8). A frame of length 0, or with a length shorter than its
   type, fails the connection.
2. If the length is over the limit, and the type is a payload type
   (`GOSSIP_MESSAGE` or `GOSSIP_MESSAGE_NEIGHBOR`), the reader skips the
   rest of the frame without keeping it. It then reads the next frame.
   For any other type, the length over the limit fails the connection
   before the reader reads the rest.
3. Else the reader reads the rest of the frame, and decodes the fields.
   Fields that do not decode within the frame fail the connection. So
   does a message of an unknown type.
4. A payload with content longer than C is skipped. The frame limit
   assumes the largest round (section 9.8), so a frame within its
   limit can still carry too much content.
5. Peer data over `MAX_PEER_DATA_LEN`, or a shuffle over
   `MAX_SHUFFLE_NODES`, fails the connection.

A topic stream that ends between two frames ends normally (section
4.5). A stream that ends inside a frame, the header included, fails
the connection.

The implementation reads the rest of a frame into one buffer per
stream (*local*). It reserves at most 64 KiB of it before the bytes
arrive, and grows it as they arrive. It does not keep the bytes of a
skipped frame.

### 9.4 Headers

```text
HeaderFrame {
  length      varint       (the number of bytes after this field)
  kind        varint
  parameters  Parameter*   (until the end of the frame)
}

Parameter {
  id      varint
  length  varint
  value   [u8; length]
}
```

A header is a frame. Its payload is a kind and a list of parameters
that a reader can skip. The kind MUST match the stream: kind 0 on the
control stream, kind 1 on a topic stream. A unidirectional stream of
another kind is a stream of an extension (section 8.2). A reader fails
the connection with `PROTOCOL` on a kind that no negotiated extension
defines.

A reader skips a parameter with an unknown id. Each extension adds a
parameter to one of the two headers (section 8.2).

Control header (kind `0x00`):

| Param | Name | Value | Rule |
|---|---|---|---|
| `0x01` | `SESSION` | 8 bytes | Required. Exactly 8 bytes. |

Topic header (kind `0x01`):

| Param | Name | Value | Rule |
|---|---|---|---|
| `0x00` | `TOPIC_ID` | 32 bytes | Required. Exactly 32 bytes. |
| `0x02` | `TOKEN` | opaque bytes | Optional. Any length, empty included. |

The payload of a header frame MUST NOT exceed 4096 bytes
(`MAX_HEADER_LEN`). A reader checks the length before it reads the
payload. Headers do not count against `MAX_CONTROL_SIZE`. The largest
token that fits next to a topic is 4058 bytes (`MAX_TOKEN_LEN`). The
rest is 1 byte kind, 34 bytes topic parameter, 1 byte token id, 2 bytes
token length.

### 9.5 Control messages

```text
GOAWAY {
  type    varint = 0x20
  opened  varint     (topic streams the sender opened on this connection)
}
```

`GOAWAY` goes in a frame, as every message does: `GOAWAY(5)` is the
frame `02 20 05`. A frame on the control stream has the limit
`MAX_CONTROL_SIZE`. A frame over it, or a message of an unknown type,
fails the connection.

### 9.6 Topic messages

Each message below is the payload of one frame (section 9.3).

Membership messages:

```text
SWARM_JOIN {
  type  varint = 0x01
  data  Option<PeerData>       (the sender's peer data)
}

SWARM_FORWARD_JOIN {
  type  varint = 0x02
  peer  PeerInfo               (the peer that joined)
  ttl   u16
}

SWARM_SHUFFLE {
  type    varint = 0x03
  origin  EndpointId           (the node that started the shuffle)
  nodes   Vec<PeerInfo>        (at most MAX_SHUFFLE_NODES)
  ttl     u16
}

SWARM_SHUFFLE_REPLY {
  type   varint = 0x04
  nodes  Vec<PeerInfo>         (at most MAX_SHUFFLE_NODES)
}

SWARM_NEIGHBOR_REQUEST {
  type      varint = 0x05
  priority  u32                (0 High, 1 Low, unknown reads as Low)
  data      Option<PeerData>
}

SWARM_NEIGHBOR_REPLY {
  type  varint = 0x06
  data  Option<PeerData>
}

SWARM_CLOSE {
  type    varint = 0x07
  reason  varint               (0 Shutdown, 1 NoCapacity, unknown reads as Shutdown)
}
```

Broadcast messages:

```text
GOSSIP_MESSAGE {
  type     varint = 0x10
  content  bytes               (over C: skipped, section 9.3)
  round    u16
  tree     TreeId
}

GOSSIP_PRUNE {
  type  varint = 0x11
  tree  TreeId
}

GOSSIP_GRAFT {
  type  varint = 0x12
  id    Option<MessageId>      (the message to send back, if any)
  tree  TreeId
}

GOSSIP_IHAVE {
  type     varint = 0x13
  entries  Vec<IHaveEntry>
}

IHaveEntry {
  id     MessageId
  round  u16
  tree   TreeId
}

GOSSIP_MESSAGE_NEIGHBOR {
  type     varint = 0x14
  content  bytes               (over C: skipped, section 9.3)
}
```

No message carries a message id for the content it carries. A receiver
computes it (section 6.1).

### 9.7 Peer data

```text
PeerData {
  kind    varint
  length  varint             (at most MAX_PEER_DATA_LEN)
  data    [u8; length]
}
```

Kind 0 (`KIND_ADDRESSES`) holds the postcard encoding [POSTCARD] of
this structure. The protocol treats it as opaque bytes, and only the
dial logic decodes it:

```text
AddrInfo {
  relay_url         Option<string>      (a varint length and UTF-8 bytes of a URL)
  direct_addresses  Vec<SocketAddr>     (a varint count, then the addresses)
}
```

```text
SocketAddr {
  family  varint             (0 IPv4, 1 IPv6)
  ip      [u8; 4] or [u8; 16] (by family)
  port    u16
}
```

A sender of kind 0 MUST use this layout. An IPv6 address carries no
flow label and no scope id. A sender SHOULD sort the addresses by
family, then by address bytes, then by port, and SHOULD NOT repeat
one. A reader MUST NOT depend on the order.

The implementation gets this layout from the serde encoding of the
Rust types `RelayUrl` and `SocketAddr`. A test of the implementation
(`peer_data_vectors`) pins the bytes of the examples in appendix A.6.
Thus a change in those types cannot change the layout without notice.

For example, no relay and the address `127.0.0.1:4433` give the
`AddrInfo` bytes `00 01 00 7f000001 d122` (9 bytes), and the
`PeerData` bytes `00 09 00 01 00 7f000001 d122`.

A node that sends empty data of kind 0 gives no addresses.

### 9.8 Limits

Each message frame has a limit, which depends on its type. A reader
checks the length of a frame against that limit before it reads the
rest of the frame (section 9.3).

The fixed limits are wire values. All nodes use the same ones. A
sender MUST NOT exceed one, and a reader fails the connection with
`PROTOCOL` on an item over one:

| Name | Value | Limits |
|---|---|---|
| `MAX_CONTROL_SIZE` | 16384 bytes | The payload of a frame of each type other than a payload type, on a topic stream and on the control stream. |
| `MAX_PEER_DATA_LEN` | 1024 bytes | The data of one `PeerData`. |
| `MAX_SHUFFLE_NODES` | 15 | The nodes of one `SWARM_SHUFFLE` (the own entry of its origin included) or `SWARM_SHUFFLE_REPLY`. |
| `MAX_HEADER_LEN` | 4096 bytes | The payload of a header frame (section 9.4). |

The limits fit together. A message that keeps to the limits of peer
data and shuffle nodes fits `MAX_CONTROL_SIZE`:

- A `PeerInfo` takes at most 1069 bytes: 32 id, 1 option, 10 kind, 2
  length, and 1024 data.
- A `SWARM_SHUFFLE` with 15 nodes thus takes at most
  1 + 32 + 1 + 15 * 1069 + 3 = 16072 bytes.
- A `GOSSIP_IHAVE` entry takes at most 43 bytes (32 id, 3 round, 8
  tree). A node splits its announcements so that each `GOSSIP_IHAVE`
  fits (section 6.5). The implementation sends at most 380 entries,
  16343 bytes.

A sender MUST keep its own messages within these limits. The
implementation trims its own peer data (section 5.2), lowers its
shuffle counts (section 5.7), caps each shuffle reply at 15 nodes, and
splits announcements. A message of its own other than a payload that
is still over `MAX_CONTROL_SIZE` fails the connection with
`SEND_FAILED` before the node writes it.

The *content limit* C (`max_content_size`) is local. It bounds the
content of `GOSSIP_MESSAGE` and `GOSSIP_MESSAGE_NEIGHBOR`, exactly.
The default is 4096 bytes (`Builder::max_content_size`, *local*). C is
not announced. The implementation refuses a broadcast with more content
in its API, and the protocol state drops one before it sends it. A node
relays only content that it read, so it never relays content over its
own C.

The *payload limit* P(C) bounds the frame of a payload. It is the size
of a `GOSSIP_MESSAGE` with C bytes of content at the largest round:

```text
P(C) = 1 + len(varint(C)) + C + 3 + 8
```

The type takes 1 byte, the round takes up to 3, and the tree 8. The
round grows on the way, so the frame of one broadcast grows too. P(C)
covers every round. The same limit applies to
`GOSSIP_MESSAGE_NEIGHBOR`. For C = 4096, P(C) is 4110 bytes.

A reader with content limit C skips a payload frame over P(C) without
reading it into memory. It also skips a payload whose frame fits P(C)
but whose content is longer than C, as at round 0 (section 9.3). Only
that message is lost, and the connection stays. So nodes of one
network SHOULD use the same C. A sender drops its own payload over its
own P(C) before it writes it (*local*).

## 10. Registries

### 10.1 Stream kinds (header kind)

| Value | Name | Stream |
|---|---|---|
| `0x00` | `CONTROL` | The bidirectional control stream |
| `0x01` | `TOPIC` | A unidirectional topic stream |

Other kinds belong to extensions (section 8.2). A reader fails the
connection on a kind that no negotiated extension defines.

### 10.2 Header parameters

| Value | Name | Kind | Value | Presence |
|---|---|---|---|---|
| `0x00` | `TOPIC_ID` | `TOPIC` | 32 bytes | Required |
| `0x01` | `SESSION` | `CONTROL` | 8 bytes | Required |
| `0x02` | `TOKEN` | `TOPIC` | opaque | Optional |

A reader skips an unknown id.

### 10.3 Message types

| Value | Name | Stream | Layer | Queue section | Limits (section 9.8) |
|---|---|---|---|---|---|
| `0x01` | `SWARM_JOIN` | topic | membership | control | 16384 bytes, peer data 1024 |
| `0x02` | `SWARM_FORWARD_JOIN` | topic | membership | data (random walk) | 16384 bytes, peer data 1024 |
| `0x03` | `SWARM_SHUFFLE` | topic | membership | data (random walk) | 16384 bytes, 15 nodes, peer data 1024 each |
| `0x04` | `SWARM_SHUFFLE_REPLY` | topic | membership | data (random walk) | 16384 bytes, 15 nodes, peer data 1024 each |
| `0x05` | `SWARM_NEIGHBOR_REQUEST` | topic | membership | control | 16384 bytes, peer data 1024 |
| `0x06` | `SWARM_NEIGHBOR_REPLY` | topic | membership | control | 16384 bytes, peer data 1024 |
| `0x07` | `SWARM_CLOSE` | topic | membership | control | 16384 bytes |
| `0x10` | `GOSSIP_MESSAGE` | topic | broadcast | data | frame P(C), content C, skipped over them |
| `0x11` | `GOSSIP_PRUNE` | topic | broadcast | control | 16384 bytes |
| `0x12` | `GOSSIP_GRAFT` | topic | broadcast | control | 16384 bytes |
| `0x13` | `GOSSIP_IHAVE` | topic | broadcast | announcements | 16384 bytes |
| `0x14` | `GOSSIP_MESSAGE_NEIGHBOR` | topic | broadcast | data | frame P(C), content C, skipped over them |
| `0x20` | `GOAWAY` | control | connection | | 16384 bytes |

The queue section is local (section 3.9). A full queue drops a message
of the data and announcement sections, and aborts the peer for a
message of the control section.

Each type belongs to one stream. A type that is not in this table, and
that no negotiated extension defines, is a protocol error. A type of
this table on the other stream is a protocol error too. `0x15` is the
`GOSSIP_PULL` of revision 6, and is unknown in this revision (section
6.10).

### 10.4 Enumerations

| Field | Values | Unknown |
|---|---|---|
| `Priority` (`u32`) | 0 `High`, 1 `Low` | `Low` |
| `CloseReason` (`varint`) | 0 `Shutdown`, 1 `NoCapacity` | `Shutdown` |
| `PeerData.kind` (`varint`) | 0 addresses | kept unchanged |

### 10.5 Close and stop codes

| Value | Name | Space |
|---|---|---|
| 1 | `RETIRED` | Connection close |
| 2 | `STALE` | Connection close |
| 3 | `SHUTDOWN` | Connection close |
| 4 | `PROTOCOL` | Connection close |
| 5 | `SEND_FAILED` | Connection close |
| 6 | `BLOCKED` | Connection close |
| 1 | `REJECTED` | Stop code of a topic stream |

A connection close with code 0 or an unknown code means a failure.
New close and stop codes belong to extensions (section 8.2). A node
sends such a code only on a connection where its extension is
negotiated.

## 11. Security considerations

### 11.1 Identity and authorization

iroh authenticates the endpoint id of a peer. The protocol trusts
nothing else about a peer. Messages carry no signature, and a peer can
forward any content. The message id proves the integrity of the
content against its id, not its author. A tree id hints at an origin,
but anyone can name any tree.

An auth token gates what a node reads on a topic, per stream and per
connection. The scope is narrow:

- The token goes to every peer to which the node opens a stream of the
  topic. A peer that receives a token can replay it to other nodes. A
  verifier SHOULD bind a token to the endpoint id of the peer, which the
  verifier sees.
- A verdict holds for one stream. A rejection holds for the
  connection, at most for 1024 topics. A new connection asks the
  verifier again.
- A reject also drops the peer from the topic, so the node stops
  sending to it (section 4.3). A verifier does not stop an accepted
  member from leaking, dropping, or flooding content.
- The implementation keeps tokens out of debug output.

### 11.2 Resource use by a peer

A peer can make a node spend memory, bandwidth, and dials. These
limits bound the cost:

| Resource | Bound |
|---|---|
| Content | C per broadcast, and P(C) per payload frame. A larger payload is skipped. A frame over P(C) is skipped without memory. |
| Other messages | `MAX_CONTROL_SIZE` (16384 bytes) per frame, on topic streams and on the control stream. A larger frame fails the connection before the reader reads it. |
| Shuffle nodes | 15 per message. More nodes fail the connection. |
| Peer data | 1024 bytes per entry. Longer data fails the connection. |
| Read memory per stream | One frame, and at most 64 KiB reserved before its bytes arrive (*local*). |
| Header size and time | 4096 bytes, 10 s, one header read at a time per connection. |
| Control messages | `GOAWAY` only, on one control stream per connection. Any byte after it fails the connection. |
| Streams | The QUIC stream limit of the endpoint (`max_concurrent_uni_streams`, 100 by default in iroh). A second stream of a topic waits for the first. |
| Rejected topics | 1024 per connection. |
| Older sessions | 16 per peer. |
| Trees | 1024 per topic, least recently used first out. |
| Re-announced ids | 256 per neighbor, 4096 per topic. |
| Send queue | `QueueOptions::size` per peer, 1 MiB by default. A neighbor cannot fill it with random walks that the node forwards or answers, as a full data section drops them (section 3.9). |
| Addresses from peer data | 32 per peer, kept 5 minutes. |
| Dials caused by neighbors | 32 peers per topic in each maintenance pass of 5 s. |
| Slow reader | 60 s without progress, 120 s for a retire. |

The message cache, the seen ids, the filter verdicts, and the records
of announced ids have no count limit. Time bounds them: by default
30 s for the cache and the announced ids, and 90 s for the others. A
peer that sends many distinct broadcasts fills them at the rate of its
link. A node that takes data from peers it does not trust SHOULD
install a filter, or shorten the retention.

### 11.3 Made-up tree ids

A peer can name any tree id. Two cases matter:

- A swarm broadcast with a new tree id creates a tree, after the
  filter accepted it. A peer that sends broadcasts with many tree ids
  evicts real trees. The cap of 1024 bounds the memory. Neither the
  cap nor `TREE_RETENTION` evicts the shared tree. An evicted tree
  comes back as a copy of the shared tree. The cost is latency and
  some duplicates, not lost messages.
- Control messages (`GOSSIP_IHAVE`, `GOSSIP_GRAFT`, `GOSSIP_PRUNE`)
  never create a tree other than the shared one. A peer cannot evict
  trees with small control messages.

A peer can also name the tree of another origin. It then takes part in
that tree like any forwarder.

### 11.4 Dials caused by peers

Some messages make a node contact a peer that the sender names:

- `SWARM_SHUFFLE` makes the end of the walk reply to the origin.
- `SWARM_FORWARD_JOIN` makes the end of the walk send a request to the
  joined peer.
- Peer data in `PeerInfo` adds addresses for the named endpoint id.

A peer could thus make a node dial endpoint ids of its choice, at the
rate of its messages. Each dial also loads address lookup. iroh
authenticates the endpoint at the other end, so a dial gives the peer
no access. Two rules bound the cost:

- A node MUST ignore `SWARM_FORWARD_JOIN` and `SWARM_SHUFFLE` from a
  peer that is not a neighbor (sections 5.4 and 5.7). Only a neighbor
  can thus cause such a dial.
- A node SHOULD limit the rate of the dials that these messages cause.
  The implementation contacts at most 32 such peers per topic in each
  maintenance pass of 5 s (`MAX_NAMED_CONTACTS_PER_PASS`, *policy*).
  Walks and shuffles share this cap, and each pass resets it. A peer
  that is a neighbor already needs no dial and does not count. At the
  cap, the node adds the peer of a walk to its passive view and sends
  nothing, and it sends no shuffle reply. The node also keeps at most
  one pending `SWARM_NEIGHBOR_REQUEST` per peer.

A node also ignores both messages when they name the node itself as
the peer to contact. It thus never sends a message to itself.

### 11.5 Rounds and the tree

Rounds are not authenticated. A peer that sends low rounds in
announcements can attract grafts, and make itself eager in many trees.
The worst outcome is a slower or less efficient tree. The repair
through announcements still delivers every message that some neighbor
caches.

Announcements are not checked either. A neighbor can announce ids
that it does not have. Each such id starts a graft chain that ends
without a message. The cost is a few grafts per id.

### 11.6 0-RTT

A node MUST NOT process gossip messages that arrive as QUIC 0-RTT data
before the handshake completes. The implementation dials with
`Endpoint::connect`, and gets accepted connections from the
application as `Connection` values. Both exist only after the
handshake.


## 12. Glossary

This section lists the terms of this document. The section in
parentheses defines each term in full.

Acceptor
:   The side of a connection that did not dial it (section 3.1).

Active view
:   The neighbors of a node in a topic (section 5.1).

Announcement
:   An entry of `GOSSIP_IHAVE`: the id, the round, and the tree of a
    swarm broadcast (section 6.5).

C
:   The content limit of a node, `max_content_size`. Default 4096
    bytes (section 9.8).

Contact
:   A bootstrap peer that a join sends `SWARM_JOIN` to (section 5.3).

Control stream
:   The first bidirectional QUIC stream of a connection (section 3.2).

Dialer
:   The side of a connection that dialed it (section 3.1).

Duplicate
:   A copy of a swarm broadcast whose id the node knows (section 6.1).

Eager peer
:   A neighbor to which a node pushes the content of the messages of a
    tree (section 6.4).

Endpoint id
:   The 32-byte Ed25519 public key that iroh authenticates for each
    side of a connection. It identifies a node (section 2.1).

Extension
:   A set of additions to the protocol, with one header parameter that
    declares support for it (section 8).

Fixed limit
:   A wire limit, the same on all nodes: `MAX_CONTROL_SIZE`,
    `MAX_PEER_DATA_LEN`, `MAX_SHUFFLE_NODES`, or `MAX_HEADER_LEN`. An
    item over it is a protocol error (section 9.8).

Former instance
:   A process that still runs with an old session of a peer. A later
    session of the same endpoint id replaced that session (section
    3.3).

Frame
:   A length, then that many bytes of payload. A header or one message
    (section 9.3).

Graft chain
:   The requests of a node for one missing message, to its announcers
    one after the other (section 6.5).

Idle
:   A connection with an empty send queue, on which no topic stream is
    open, waits for an acknowledgement, or waits to be opened (section
    3.6).

Lazy peer
:   A neighbor to which a node only announces the ids of the messages
    of a tree (section 6.4).

Message id
:   The BLAKE3 hash of the content of a broadcast (section 6.1).

Negotiated
:   An extension is negotiated on a connection, or on a topic of a
    connection, when both sides declared it there (section 8.2).

Neighbor
:   A peer in the active view (section 5.1).

Neighbor message
:   A broadcast that goes to the direct neighbors of the origin only
    (section 6.2).

Node
:   One running instance of the protocol on one iroh endpoint (section
    2.1).

Origin
:   The node that broadcast a message first (section 6.1). In
    `SWARM_SHUFFLE`, the node that started the shuffle (section 5.7).

Passive view
:   Peers that a node knows in a topic but does not use as neighbors
    (section 5.1).

Payload
:   A message that carries content: `GOSSIP_MESSAGE` or
    `GOSSIP_MESSAGE_NEIGHBOR` (section 9.3).

Payload limit
:   P(C), the largest frame of a payload for the content limit C
    (section 9.8).

Peer
:   Another node, as seen from a node (section 2.1).

Permitted
:   A message type is permitted on a stream when the base protocol or
    a negotiated extension defines it for that stream (section 7.1).

Protocol error
:   A violation of the encoding or of the connection rules. It fails
    the connection with `PROTOCOL` (section 7.1).

Ready
:   A connection that can become the writer (section 3.4).

Refill
:   The requests that a node sends to passive peers to fill its active
    view (section 5.8).

Retire
:   The orderly end of a connection that both sides agree on with
    `GOAWAY` (section 3.6).

Round
:   The number of hops a swarm broadcast took before the current send
    (section 6.3).

Session
:   One run of a node, from start to shutdown. A node draws an 8-byte
    random session id for each run (section 3.3).

Shared tree
:   The tree with the id `00*8` (section 6.7).

Short
:   A node with less than half of its active view (section 5.8).

Swarm broadcast
:   A broadcast that reaches every member of the topic (section 6.2).

Topic
:   A swarm and its broadcasts, named by a 32-byte topic id (section
    2.1).

Topic stream
:   A unidirectional QUIC stream that carries the messages of one
    topic (section 4).

Tree
:   One broadcast tree of a topic, named by an 8-byte tree id (section
    6.7).

Want
:   A topic wants a peer while it needs a connection to the peer. That
    is the case while the peer is a neighbor, while the topic waits for
    its reply to a `SWARM_JOIN` or `SWARM_NEIGHBOR_REQUEST`, or while a
    join connects to it (section 5.1).

Writer
:   The one connection to a peer on which a node writes topic messages
    (section 3.4).

## 13. References

### 13.1 Normative references

[BLAKE3]
:   O'Connor, J., Aumasson, J.-P., Neves, S., Wilcox-O'Hearn, Z.,
    "BLAKE3: one function, fast everywhere", 2020,
    <https://github.com/BLAKE3-team/BLAKE3-specs/blob/master/blake3.pdf>.

[POSTCARD]
:   The postcard wire format specification,
    <https://postcard.jamesmunns.com/wire-format>.

[RFC2119]
:   Bradner, S., "Key words for use in RFCs to Indicate Requirement
    Levels", BCP 14, RFC 2119, DOI 10.17487/RFC2119, March 1997,
    <https://www.rfc-editor.org/info/rfc2119>.

[RFC8174]
:   Leiba, B., "Ambiguity of Uppercase vs Lowercase in RFC 2119 Key
    Words", BCP 14, RFC 8174, DOI 10.17487/RFC8174, May 2017,
    <https://www.rfc-editor.org/info/rfc8174>.

[RFC9000]
:   Iyengar, J., Thomson, M., "QUIC: A UDP-Based Multiplexed and Secure
    Transport", RFC 9000, DOI 10.17487/RFC9000, May 2021,
    <https://www.rfc-editor.org/info/rfc9000>.

### 13.2 Informative references

[HYPARVIEW]
:   Leitão, J., Pereira, J., Rodrigues, L., "HyParView: a membership
    protocol for reliable gossip-based broadcast", DSN 2007,
    DOI 10.1109/DSN.2007.56.

[PLUMTREE]
:   Leitão, J., Pereira, J., Rodrigues, L., "Epidemic broadcast trees",
    SRDS 2007, DOI 10.1109/SRDS.2007.27.

## Appendix A. Test vectors

The values below come from the tests in `src/net/wire.rs`, except
where the text says otherwise. The tests decode each vector, encode it
again, and compare the bytes. A vector without a test name has no test
at the commit of section 1.1.

Symbols:

- `P` is the endpoint id of the secret key `05*32`:
  `6e7a1cdd29b0b78fd13af4c5598feff4ef2a97166e3ca6f2e4fbfccd80505bf1`.
- `I` is the message id of the content `hi` (`6869`), BLAKE3:
  `85052e9aab1b67b6622d94a08441b09fd5b7aca61ee360416d70de5da67d86ca`
  (`gossip_id_is_the_content_hash`).
- `T` is the tree id `33*8`.

### A.1 Headers

Topic header frame for topic `11*32` (`header_roundtrips`):

```text
23                  length 35
01                  kind TOPIC
00 20 11*32         TOPIC_ID, length 32, topic
```

The same header payload with the token `secret` (`header_with_a_token`):

```text
01 00 20 11*32 02 06 736563726574
```

A token of 300 bytes takes a 2-byte length (`ac 02`), so the payload
is 35 + 1 + 2 + 300 bytes. A token of 4058 bytes fills the payload to
exactly 4096 bytes (`max_token_fills_the_header`). A header frame with
a payload of 4096 bytes reads. A frame that announces 4097 bytes fails
on its length alone, before the reader allocates for the payload
(`oversized_header_is_rejected`).

Control header frame for session `0102030405060708`
(`control_header_vectors`):

```text
0b 00 01 08 0102030405060708
```

A reader skips unknown parameters (`unknown_parameters_are_skipped`,
`control_header_vectors`). An empty payload, a cut payload, a topic of
31 bytes, a header without a topic, and a header of an unknown kind
fail (`malformed_headers_fail`). A session of 7 bytes, a header without
a session, and a control header with the kind `TOPIC` fail
(`malformed_control_headers_fail`).

### A.2 Control messages

Frames of `GOAWAY` (`control_message_vectors`):

| Message | Frame |
|---|---|
| `GOAWAY(0)` | `02 20 00` |
| `GOAWAY(5)` | `02 20 05` |
| `GOAWAY(300)` | `03 20 ac02` |

The payload `20` alone, without its count, fails to decode. The payload
`7f 01` is of an unknown type.

On the control stream (`control_reader_fails_on_unknown_types_and_limits_frames`):
the frame `02 2f01` fails as an unknown type, and the reader does not
read the `GOAWAY` after it. A frame of 16385 bytes fails as over
`MAX_CONTROL_SIZE`. The end of the stream before a message fails too.

### A.3 Topic messages

Message payloads (`every_message_type_roundtrips`). Each goes in a frame
after its length. For example, `SWARM_JOIN(None)` is the frame
`02 01 00`.

| Message | Payload |
|---|---|
| `SWARM_JOIN(None)` | `01 00` |
| `SWARM_JOIN(kind 0, aabbcc)` | `01 01 00 03 aabbcc` |
| `SWARM_JOIN(kind 7, beef)` | `01 01 07 02 beef` |
| `SWARM_FORWARD_JOIN(P, None, ttl 6)` | `02 P 00 06` |
| `SWARM_SHUFFLE(origin P, [P, None], ttl 6)` | `03 P 01 P 00 06` |
| `SWARM_SHUFFLE_REPLY([P, None])` | `04 01 P 00` |
| `SWARM_SHUFFLE_REPLY([P, kind 7, 01])` | `04 01 P 01 07 01 01` |
| `SWARM_NEIGHBOR_REQUEST(Low, None)` | `05 01 00` |
| `SWARM_NEIGHBOR_REQUEST(High, kind 0, 010203)` | `05 00 01 00 03 010203` |
| `SWARM_NEIGHBOR_REPLY(None)` | `06 00` |
| `SWARM_NEIGHBOR_REPLY(kind 0, aabbcc)` | `06 01 00 03 aabbcc` |
| `SWARM_CLOSE(Shutdown)` | `07 00` |
| `SWARM_CLOSE(NoCapacity)` | `07 01` |
| `GOSSIP_MESSAGE("hi", round 300, T)` | `10 02 6869 ac02 T` |
| `GOSSIP_MESSAGE_NEIGHBOR("hi")` | `14 02 6869` |
| `GOSSIP_PRUNE(T)` | `11 T` |
| `GOSSIP_GRAFT(Some(I), T)` | `12 01 I T` |
| `GOSSIP_GRAFT(None, T)` | `12 00 T` |
| `GOSSIP_IHAVE([(I, round 2, T)])` | `13 01 I 02 T` |

The first varint of each payload is its type code
(`type_codes_match_the_encoding`).

Bytes after the known fields (`trailing_bytes_are_ignored`): `01 00 dead`
reads as `01 00`, `11 T 01` as `11 T`, `07 01 09` as `07 01`, and
`12 01 I T 00` as `12 01 I T`.

Defaults for unknown values (`unknown_enum_values_read_as_defaults`):
`05 09 00` reads as `05 01 00` (Low). `07 09` reads as `07 00`
(Shutdown).

Invalid payloads (`invalid_messages_fail`), each a protocol error:

| Bytes | Error |
|---|---|
| (empty) | No type |
| `01` | No option byte |
| `05` | No priority |
| `10 01` | Content of 1 byte, cut short |
| `03 05` | Origin cut short |
| `13 05` | 5 entries, none present |
| `12 01` | Id cut short |
| `11` | No tree |
| `11 3333` | Tree cut short |
| `10 02 6869 ac02` | Swarm broadcast without its tree |
| `12 00` | Graft without its tree |
| `13 01 00` | Entry cut short |
| `10 00 808080808080808080` | Round in more than 3 varint bytes |
| `10 00 ffffff7f` | Round over a `u16` |

Also (`other_decode_errors_are_not_skipped`): `10 05 6869` and
`14 05 6869` have content cut short, and `10 02 6869` has no round.
These fail. A reader does not skip them as content over its limit.

Frames cut short (`truncated_frames_fail`): the frame `02 01 00` cut
after 2 bytes fails. A payload frame of 200 bytes, over the payload
limit for C = 10, fails when the stream ends inside it, although the
reader skips it.

Unknown types on a topic stream (`unknown_types_fail`): the frame
`04 7f010203` fails as an unknown type, and the reader does not read
the frames after it.

Limits, with C = 100 unless the text says otherwise:

- `control_frames_over_the_limit_fail`: a frame of 16385 bytes of each
  type other than a payload type, and of the unknown type `7f`, fails.
  The reader does not read the `GOSSIP_PRUNE` after it.
- `control_frames_have_their_own_limit`: a `SWARM_JOIN` with 1024 bytes
  of peer data reads, although it is larger than C.
- `peer_data_and_shuffles_over_their_limits_fail`: a `SWARM_JOIN` with
  1024 bytes of peer data reads, and one with 1025 bytes fails. A
  `SWARM_SHUFFLE_REPLY` with 15 nodes reads, and one with 16 nodes
  fails.
- `payloads_over_the_limit_are_skipped_without_allocating`: a frame of
  5000 bytes of type `10`, and one of type `14`, are skipped. The
  `GOSSIP_PRUNE` after them reads, and the read buffer stays at most
  200 bytes.
- `content_limit_is_exact`: a `GOSSIP_MESSAGE` at round 0 with 101 bytes
  of content is skipped, although its frame fits P(100). The one with
  100 bytes after it reads.
- `max_gossip_size_covers_every_round`: for content of 0, 127, 128, 4000,
  and 20000 bytes, P(C) is the size of a `GOSSIP_MESSAGE` at round
  65535, and no smaller round gives a larger size.

Tests on a live connection, in `src/net/actor/tests/streams.rs`:

- `frame_over_the_size_limit_is_skipped`: a payload over the limit is
  skipped, and the `GOSSIP_PRUNE` after it arrives.
- `payload_of_the_content_limit_goes_through`: with C = 20000, more than
  `MAX_CONTROL_SIZE`, a payload of 20000 bytes arrives, and one of
  20001 bytes is skipped.
- `oversized_message_is_dropped`: a writer with C = 1024 drops a payload
  of 4096 bytes, sends the message after it, and keeps the connection.
- `skips_what_a_later_version_adds`: unknown parameters in both
  headers, a `GOSSIP_MESSAGE_NEIGHBOR` frame over P(1024), and bytes
  after the fields of a known message keep the connection open.
- `unknown_message_type_fails_the_connection`: the frame `04 3f010203`
  closes the connection with `PROTOCOL`, on a topic stream and on the
  control stream.
- `oversized_control_message_fails_the_connection`: a writer with a
  queued `SWARM_JOIN` over `MAX_CONTROL_SIZE` closes the connection
  with `SEND_FAILED`.

### A.4 Tree ids of the implementation

These values are informative. The tree id of an origin is local to the
origin (section 6.7), so a receiver never computes it. The
implementation (`TreeId::of`) hashes the postcard encoding of the
origin. For an endpoint id, that is its 32 bytes, with no length
prefix. Both `b3sum` and `TreeId::of` give these values
(`tree_id_of_an_origin` in `src/proto/plumtree.rs`):

| Origin | Tree id |
|---|---|
| `05*32` (array) | `df6c9593e2faac45` |
| `P` | `90bdeef49d5d2664` |

### A.5 Varints

`varint_lengths` checks: `0` and `127` take 1 byte, `128` and `16383`
take 2, `16384` takes 3, `2^64 - 1` takes 10.

A round in more than 3 bytes, and a round over 65535, fail
(`invalid_messages_fail`, appendix A.3).

### A.6 Peer data

`PeerData` of kind 0 with no relay and the address `127.0.0.1:4433`
(section 9.7, `peer_data_vectors` in `src/net/address_lookup.rs`):

```text
00                  kind 0, addresses
09                  data length 9
00                  relay_url: none
01                  one address
00                  family IPv4
7f000001            127.0.0.1
d122                port 4433
```

The same test pins two more examples. No relay and no address give
`00 02 00 00`. The relay `https://relay.example/` and the address
`[::1]:7` give:

```text
00                  kind 0, addresses
2b                  data length 43
01 16 68747470733a2f2f72656c61792e6578616d706c652f
                    relay_url: some, 22 bytes
01                  one address
01                  family IPv6
00000000000000000000000000000001
                    ::1
07                  port 7
```

The tests `peer_data_stays_within_the_limit` and
`peer_data_trims_addresses_past_the_byte_limit` (same file) check that
the own peer data of a node stays within `MAX_PEER_DATA_LEN` (section
5.2).

## Appendix B. Defaults and constants

All values in this appendix are local policy unless the column "Wire"
says otherwise. A wire value MUST be the same on all nodes.

### B.1 Wire values

| Name | Value | Source |
|---|---|---|
| ALPN | `iroh-gossip/2-canary.4` | `net.rs:39` |
| `MAX_HEADER_LEN` | 4096 bytes | `net/wire.rs:66` |
| `MAX_TOKEN_LEN` | 4058 bytes | `net/wire.rs:289` |
| Session id length | 8 bytes | `net/wire.rs:341` |
| Message id | BLAKE3 of content, 32 bytes | `proto/plumtree.rs:38` |
| Shared tree id | `00*8` | `proto/plumtree.rs:194` |
| `MAX_CONTROL_SIZE` | 16384 bytes | `proto.rs:81` |
| `MAX_PEER_DATA_LEN` | 1024 bytes | `proto.rs:87` |
| `MAX_SHUFFLE_NODES` | 15 | `proto/hyparview.rs:92` |
| Payload limit P(C) | `1 + len(varint(C)) + C + 3 + 8` | `proto/plumtree.rs:291`, `net/wire.rs:498` |

### B.2 Connection layer

| Name | Value | Source |
|---|---|---|
| `DEFAULT_MAX_CONTENT_SIZE` (C) | 4096 bytes | `proto.rs:74` |
| Read reserve | 64 KiB reserved for a frame before its bytes arrive | `net/wire.rs:586` |
| `HEADER_TIMEOUT` | 10 s | `net/wire.rs:59` |
| `WRITE_TIMEOUT` | 60 s | `net/wire.rs:46` |
| `OPEN_TIMEOUT` | 15 s | `net/wire.rs:54` |
| `RETIRE_TIMEOUT` | 120 s | `net/conn.rs:43` |
| `DEFAULT_RETIRE_AFTER` | 10 s | `net/actor.rs:49` |
| `SHUTDOWN_DRAIN` | 1 s | `net/actor.rs:80` |
| `MAX_REJECTED_TOPICS` | 1024 | `net/conn.rs:48` |
| `MAX_OLD_SESSIONS` | 16 | `net/peers.rs:32` |
| `QUEUE_SIZE` | 1 MiB | `net/queue.rs:38` |
| `MIN_QUEUE_SIZE` | 8 frames of `MAX_CONTROL_SIZE`, about 132 KiB | `net/queue.rs:35` |
| Queue data section | 3/4 of the queue: payloads and random walks | `net/queue.rs:245` |
| Queue announcement section | 1/8 of the queue | `net/queue.rs:246` |
| `MAX_ADDRS` | 32 | `net/address_lookup.rs:30` |
| Address retention | 5 min | `net/address_lookup.rs:33` |

### B.3 Membership (`HyparviewConfig` and constants)

| Name | Default | Source |
|---|---|---|
| `active_view_capacity` | 5 | `proto/hyparview.rs:393` |
| `passive_view_capacity` | 30 | `proto/hyparview.rs:395` |
| `active_random_walk_length` (ARWL) | 6 | `proto/hyparview.rs:397` |
| `passive_random_walk_length` (PRWL) | 3 | `proto/hyparview.rs:399` |
| `shuffle_random_walk_length` | 6 | `proto/hyparview.rs:401` |
| `shuffle_active_view_count` | 3, the sum of both counts at most 14 | `proto/hyparview.rs:403` |
| `shuffle_passive_view_count` | 4 | `proto/hyparview.rs:405` |
| `shuffle_interval` | 60 s | `proto/hyparview.rs:407` |
| `neighbor_request_timeout` | 3 s | `proto/hyparview.rs:410` |
| `join_dials` | 5 | `proto/hyparview.rs:411` |
| `join_contacts` | 2 | `proto/hyparview.rs:412` |
| `INITIAL_SHUFFLE_INTERVAL` | 1 s | `proto/hyparview.rs:29` |
| `MAINTENANCE_INTERVAL` | 5 s | `proto/hyparview.rs:39` |
| `JOIN_RETRY_DELAY` | 10 s | `proto/hyparview.rs:44` |
| `MAX_BACKOFF_PASSES` | 12 | `proto/hyparview.rs:52` |
| `MAX_REQUESTS_PER_REFILL` | 3 | `proto/hyparview.rs:59` |
| `MAX_NAMED_CONTACTS_PER_PASS` | 32 | `proto/hyparview.rs:76` |
| `MAX_BOOTSTRAP_PEERS` | 8 | `proto/hyparview.rs:82` |
| `MAX_RECENT_NEIGHBORS` | 8 | `proto/hyparview.rs:85` |
| Shuffle jitter | ±25 % | `proto/hyparview.rs:1335` |

### B.4 Broadcast (`PlumtreeConfig` and constants)

| Name | Default | Source |
|---|---|---|
| `graft_timeout_1` | 80 ms | `proto/plumtree.rs:435` |
| `graft_timeout_2` | 40 ms | `proto/plumtree.rs:439` |
| `dispatch_timeout` | 5 ms | `proto/plumtree.rs:444` |
| `optimization_threshold` | 7 | `proto/plumtree.rs:447` |
| `message_cache_retention` | 30 s | `proto/plumtree.rs:450` |
| `message_id_retention` | 90 s | `proto/plumtree.rs:451` |
| `cache_evict_interval` | 1 s | `proto/plumtree.rs:452` |
| `source_tree_after` | 1 | `proto/plumtree.rs:457` |
| Tree id of an origin | first 8 bytes of BLAKE3 of its endpoint id | `proto/plumtree.rs:202` |
| `IHave` batch | at most 380 entries, to fit `MAX_CONTROL_SIZE` | `proto/plumtree.rs:709-714` |
| `MAX_TREES` | 1024 | `proto/plumtree.rs:514` |
| `TREE_RETENTION` | 300 s | `proto/plumtree.rs:519` |
| `MAX_DROPPED_PER_PEER` | 256 | `proto/plumtree.rs:499` |
| `MAX_DROPPED` | 4096 | `proto/plumtree.rs:503` |
| `DROPPED_PER_TICK` | 64 | `proto/plumtree.rs:505` |
| `ANNOUNCE_DROPPED_INTERVAL` | 1 s | `proto/plumtree.rs:507` |
