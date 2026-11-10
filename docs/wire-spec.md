# The iroh-gossip protocol

Status: Draft for iroh-gossip 1.0\
ALPN: `iroh-gossip/2-canary.4`\
Date: 2026-11-10

## Abstract

iroh-gossip lets iroh endpoints form a swarm per topic and broadcast
messages to all members of the swarm. It runs over QUIC connections
that iroh authenticates with the endpoint id of each side.

The protocol has three layers. The connection layer keeps at most one
connection that writes to each peer, and closes idle connections
without a loss. The membership layer is HyParView: per topic, it keeps
a small active view of neighbors and a larger passive view of known
peers. The broadcast layer is Plumtree: it pushes each message along a
spanning tree of the neighbors, and announces it on the other links so
that a receiver can repair a gap. Each sender that keeps sending gets a
tree of its own.

The protocol gives ordered delivery per topic and connection, bounded
resources per peer, and repair of lost broadcasts while a copy is
cached. It gives no storage, no acknowledgement of delivery, and no
proof of the author of a message.

## 1. Introduction

### 1.1 Scope

This document specifies the bytes on the wire and the behavior that a
peer can observe. It also gives the defaults of local policy where
that policy shapes what a peer sees, such as timeouts. Appendix B lists
the defaults.

### 1.2 Conventions

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and
"OPTIONAL" in this document are to be interpreted as described in BCP
14 [RFC2119] [RFC8174] when, and only when, they appear in all
capitals, as shown here.

A value marked *(policy)* is a local default. Another implementation
can choose a different value and still interoperate.

Byte layouts use this notation:

```text
Name {
  field   type
}
```

Section 9.2 defines the types. Hex bytes appear in lowercase. `x*n`
means the byte `x` repeated `n` times.

## 2. Overview

### 2.1 Roles

A *node* is one running instance of the protocol on one iroh endpoint.
A *peer* is another node, as seen from a node. The *endpoint id* is
the 32-byte Ed25519 public key that iroh authenticates for each side of
a connection. It identifies a node.

Every node has the same role. A *topic* is a swarm and its broadcasts,
named by a 32-byte topic id. A node joins a topic through one or more
bootstrap peers, which the application supplies. It then learns other
members of the topic from the protocol.

Topics are independent and share only connections: one connection to
a peer carries all topics that the two nodes share.

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

Node A joins topic `T` through node B (figure 2):

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
round 0 to its eager peers B and C. B forwards it with round 1, so C
gets a second copy. C keeps the first copy, and sends `GOSSIP_PRUNE` to
B. B is now a lazy peer of C in that tree.

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
`iroh-gossip/2-canary.4`. An application can choose another ALPN. All
nodes of a network then MUST use the same one.

iroh authenticates the endpoint id of each side during the QUIC
handshake. The protocol uses that id as the identity of the peer. No
message carries the identity of its sender.

The side that dials is the *dialer*. The other side is the *acceptor*.

### 3.2 Control stream

The *control stream* is the first bidirectional stream of a
connection. The dialer MUST open it as the first stream. Each side MUST
write its control header (section 9.4) as the first bytes of its half
of the stream, without waiting for the header of the peer.

The control header carries the session id of the sender, and the
connection-scope extensions that the sender declares (section 8). After
the header, the control stream carries control messages, then `GOAWAY`
(section 3.6), then the FIN. The base protocol has no control message
other than `GOAWAY`.

A side fails the connection with `PROTOCOL` when one of these takes
longer than `HEADER_TIMEOUT` (10 s):

- the dialer opens the control stream,
- the control stream of the dialer arrives at the acceptor, or
- the control header of the peer arrives in full, from the time the
  side starts to read it.

A *topic stream* is a unidirectional stream that carries the messages
of one topic (section 4). A side reads the topic streams of the peer
only after it read the control header of the peer. A dialer MAY open
topic streams before it read the header of the acceptor.

A node opens no other bidirectional stream, unless a negotiated
extension defines one.

### 3.3 Sessions

A *session* is one run of a node, from start to shutdown. A node MUST
draw a new random session id of 8 bytes each time it starts, and MUST
send it on all its connections in that run. A session id needs no
secrecy.

A node tracks the current session id of each peer, and some older
ones (16, *policy*). It can forget them when it has no state left for
the peer: no connection, no queued message, no dial, and no topic that
wants the peer (section 5.1).

When the control header of a peer arrives, the node compares its
session id:

1. The id is the current session of the peer, or the node knows no
   session of the peer. The node records the id on the connection.
2. The id is new, and the node knows another current session. The peer
   restarted. The node MUST close each connection with the old session
   with `STALE`. It discards its send queue to the peer, and reports to
   every topic that the peer disconnected. The new id becomes current.
3. The id is an older session. The connection leads to a *former
   instance* of the peer: a process that still runs with an old
   session, for example after a restart that did not end it. The node
   MUST close that connection with `STALE`. If the node dialed it, the
   dial failed. If the peer dialed it and messages are queued, the node
   dials the peer again.

Rule 3 keeps messages away from a former instance only while the node
remembers the older sessions. After that, a former instance that dials
in can receive messages, which are then lost. This ends when the live
instance connects, because rule 2 then closes the old connection.

### 3.4 Writer selection

A node keeps one send queue per peer, shared by all topics. It writes
the queue on one connection at a time: the *writer*. The queue moves
to the writer, and back when the writer's connection ends.

A connection is *ready* to become the writer when:

- the node read the control header of the peer on it, or
- the node dialed it, and the node has seen no restart of the peer.

A connection that retires is never ready. When no connection writes,
the node picks the first ready connection. After a restart of the
peer, a dialed connection waits for the header, because it may lead to
a former instance.

The writer opens a topic stream for each topic that wants the peer at
once, also without a message to send (section 4.1). The other
connections to the peer only read.

### 3.5 Crossed dials

Two nodes can dial each other at the same time. Each node then has two
connections to the peer. The connection that the node with the higher
endpoint id dialed loses. Endpoint ids compare as 32-byte strings,
byte by byte.

A node decides once it read the control headers of all its connections
to the peer that do not retire. If at least one of them wins, the node
retires every loser (section 3.6). The retire keeps the order of
messages, because the writer moves to the winner only after the loser
closed.

A node marks a connection as retiring only when it sends its own
`GOAWAY`. For a short time, a new dial during a retire can thus look
like a crossed dial to the peer, which can then retire the new
connection. No message is lost: the queue waits, and the node dials
again.

### 3.6 Retire with GOAWAY

A *retire* is the orderly end of a connection that both sides agree on
with `GOAWAY`. It loses no message. A side starts a retire when:

- the connection was idle for `retire_after` (10 s, *policy*),
- the connection lost a crossed dial, or
- it read all streams that the peer counted in its `GOAWAY`.

A connection is *idle* when no topic stream is open in either
direction, the send queue is empty, and the writer has no topic stream
left to open. A topic stream whose header the node still reads counts
as open.

To retire, a side:

1. finishes all its topic streams, and opens no new one,
2. sends `GOAWAY` on the control stream, with the number of topic
   streams that the side opened on this connection, and
3. writes nothing more on the connection, except the FIN of the
   control stream.

A side that receives `GOAWAY(n)` reads on until `n` streams of the
peer ended. A stream that the side stopped (section 4.3) counts as
ended. Then the side retires too, if it did not yet.

A side closes the connection with `RETIRED` when it read all streams
that the peer counted, and the peer finished its half of the control
stream. Either side can close first. The queued messages go out on the
next connection.

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
connection, and Y opened 1.

Rules:

- A side MUST NOT open a topic stream after it sent `GOAWAY`. A stream
  that the side opened before can arrive after the `GOAWAY`. The
  receiver reads it as usual.
- The count of `GOAWAY` MUST equal the number of topic streams that
  the side opened. A receiver fails the connection with `PROTOCOL` when
  the peer opens more streams than the count, or when the count is
  lower than the streams it accepted already.
- A side MUST NOT send any byte on the control stream after `GOAWAY`,
  except the FIN, and MUST NOT finish the control stream before
  `GOAWAY`. A receiver fails the connection with `PROTOCOL` otherwise.
- A side SHOULD bound the time of a retire. The default is 120 s
  (`RETIRE_TIMEOUT`, *policy*), after which the side fails the
  connection with `BLOCKED`.

While all connections to a peer retire, a node keeps new messages in
the queue, and dials the peer once.

### 3.7 Close codes

A node closes a connection with a QUIC application close [RFC9000].
The error code tells the peer why. A receiver MUST NOT act on the
reason phrase.

| Code | Name | The sender closes because |
|---|---|---|
| 1 | `RETIRED` | Both sides finished a retire (section 3.6). |
| 2 | `STALE` | The connection carries an old session of the peer (section 3.3). |
| 3 | `SHUTDOWN` | The node shuts down. |
| 4 | `PROTOCOL` | The peer broke the protocol (section 7). |
| 5 | `SEND_FAILED` | A write failed, the peer stopped a topic stream with a code other than `REJECTED`, or an own message other than a payload was over its limit. |
| 6 | `BLOCKED` | A write, a stream open, or a retire made no progress in time, or a control message did not fit the send queue. |

Code 0 is never sent on purpose. The QUIC stack sends it when a node
drops a connection without a close, for example in a crash. `RETIRED`
is not 0, so that a crash does not look like a retire.

The receiver of a close acts on the code:

- `STALE`: the node reports nothing to its topics.
- `RETIRED`: no message was lost. The send queue and the membership
  state stay. The node dials the peer again if messages wait, or if a
  topic still wants the peer.
- Any other code, or a transport error: the connection failed. If it
  was the writer, the node discards the send queue to the peer and
  reports a disconnect to every topic.

### 3.8 Shutdown

When a node shuts down, it leaves every topic (section 5.9). Each
writer sends what is left in its queue, finishes its streams, and
closes with `SHUTDOWN`. A node SHOULD bound this wait (1 s, *policy*).

### 3.9 Send queue

A node keeps one send queue per peer, with a size limit (1 MiB,
*policy*). A node MAY drop these messages to a peer that stays
connected, for example when its queue is full:

- payloads (`GOSSIP_MESSAGE`, `GOSSIP_MESSAGE_NEIGHBOR`). Plumtree
  announces a dropped swarm broadcast again (section 6.9).
- announcements (`GOSSIP_IHAVE`).
- the messages of a random walk (`SWARM_FORWARD_JOIN`,
  `SWARM_SHUFFLE`, `SWARM_SHUFFLE_REPLY`). A dropped walk only ends
  early. Without the drop, a neighbor could fill the queue to an honest
  peer with shuffles that the node forwards.

A node MUST NOT drop any other message and then send the messages
after it. The peer would act on the later messages without the dropped
one, and the two sides would see the link differently. A node aborts
the peer instead: it closes every connection to the peer with
`BLOCKED`, discards the queue, and reports a disconnect. Both sides
then start over.

A node can split its queue into sections, so that payloads and
announcements cannot take the room of other messages. The default
gives payloads and walks three quarters of the queue, and
announcements one eighth.

### 3.10 Timeouts

All timeouts are *policy*. Only `HEADER_TIMEOUT` sets a limit that a
peer must meet.

| Name | Value | Effect when it expires |
|---|---|---|
| `HEADER_TIMEOUT` | 10 s | A control stream or a header did not arrive in time: close with `PROTOCOL`. |
| `WRITE_TIMEOUT` | 60 s | A write made no progress: close with `BLOCKED`. |
| `OPEN_TIMEOUT` | 15 s | A stream open for a queued message waits at the stream limit of the peer: close with `BLOCKED`. |
| `RETIRE_TIMEOUT` | 120 s | A retire did not end: close with `BLOCKED`. |
| `retire_after` | 10 s | An idle connection starts a retire. |

A slow reader keeps its connection as long as it takes some bytes at
least once per `WRITE_TIMEOUT`.

## 4. Topic streams

### 4.1 Opening

A topic stream starts with a topic header (section 9.4), then carries
messages (section 9.3). The writer opens a topic stream:

- for each topic that wants the peer, when the connection becomes the
  writer, also without a message, and
- for a message whose topic has no open stream on the connection.

A node SHOULD have at most one open topic stream per topic and
connection. It opens a new stream for a topic only after it finished
the previous one. It keeps the stream of a topic open while the topic
wants the peer.

The receiver reads the headers of new streams one at a time, in the
order in which it accepts the streams.

### 4.2 Header

The topic header names the topic, and can carry an auth token. A
header MUST arrive in full within `HEADER_TIMEOUT` after the receiver
accepted the stream, and MUST NOT be longer than `MAX_HEADER_LEN`
(4096 bytes). Otherwise the receiver fails the connection with
`PROTOCOL`.

The sender MUST send the topic parameter. It MAY send the token
parameter, and the parameters of the topic-scope extensions that it
declares (section 8.2). A reader accepts the parameters in any order,
skips unknown ones, and takes the first of a repeated one. An empty
token is a token.

### 4.3 Auth tokens and refusal

A node can set an auth token per topic. It sends the token in the
header of each topic stream of that topic. A node can also install a
verifier. The verifier gets the topic id, the endpoint id of the peer,
and the token, and accepts or rejects the stream. Without a verifier, a
node accepts every stream.

The receiver runs the verifier on each new topic stream, after the
header and before the first message. On reject, the receiver:

- MUST stop the stream with the stop code `REJECTED` (1), and MUST NOT
  hand any message of the stream to the protocol,
- drops the peer from the topic as for a disconnect, and cancels a
  pending join to it.

The connection stays open, and other topics are not affected. The
stopped stream counts as ended for `GOAWAY` (section 3.6). A node MAY
remember the reject for the rest of the connection, and stop later
streams of the topic without a new verifier call.

The sender of a stream that the peer stopped with `REJECTED` loses the
messages of that stream. On this connection, it opens no new stream
for the topic, and drops the queued messages of the topic. The topic
drops the peer as for a disconnect, and tells the application that the
peer refused it. A new connection starts without refusals.

A node MUST NOT stop a topic stream with another code, unless a
negotiated extension defines the code. The sender of such a stream
fails the connection with `SEND_FAILED`.

### 4.4 Ordering

The protocol keeps the order of the messages of one topic from one
sender:

- Within a stream, QUIC delivers in order.
- Within a connection, the receiver reads a newer stream of a topic
  only after the older stream of the topic ended.
- Across connections, a node writes on one writer at a time. After a
  retire, the next writer starts only after the peer read all that the
  previous writer sent.

When a connection fails, the queue may lose messages. The node then
reports a disconnect, so the topics start over with the peer. The
protocol gives no order between topics, and no order between senders.

### 4.5 The end of a stream

A node finishes (FIN) a topic stream:

- after `SWARM_CLOSE`, which is thus the last message of the topic to
  that peer,
- when its topic no longer wants the peer, after the messages queued
  before,
- before it sends `GOAWAY`, and
- when the node shuts down.

The end of a stream is not a membership event. Only `SWARM_CLOSE` and
the end of a connection change the views.

A node MUST NOT reset a topic stream. A reader fails the connection
with `PROTOCOL` when the peer resets a topic stream.

### 4.6 Messages for a topic the node did not join

A node can receive a stream for a topic it did not join, for example
after it left. It answers each membership message other than
`SWARM_CLOSE` with `SWARM_CLOSE(Shutdown)`, and ignores broadcast
messages.

## 5. Membership (HyParView)

The membership layer follows HyParView [HYPARVIEW], with the changes
that this section states. It runs per topic.

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

A topic *wants* a peer while the peer is a neighbor, while the topic
waits for the reply of the peer to a `SWARM_JOIN` or a
`SWARM_NEIGHBOR_REQUEST`, or while a join connects to the peer. When no
topic wants a peer, the node finishes its streams to the peer, and the
connection can retire.

### 5.2 Peer info and peer data

Messages that introduce a peer carry a `PeerInfo`: the endpoint id and
optional peer data (section 9.7). Peer data has a kind and bytes:

- Kind 0 holds the addresses of the peer: an optional relay URL and a
  set of socket addresses. A node uses them to dial the peer.
- A node MUST keep peer data of an unknown kind, and MUST forward it
  unchanged when it forwards the `PeerInfo`.

A node sends its own peer data in `SWARM_JOIN`,
`SWARM_NEIGHBOR_REQUEST`, `SWARM_NEIGHBOR_REPLY`, and in its own entry
of `SWARM_SHUFFLE`. Peer data of kind 0 that does not decode is no
protocol error. The node ignores it.

The data of one `PeerData` MUST NOT be longer than `MAX_PEER_DATA_LEN`
(1024 bytes). A node with more addresses MUST drop some of them.

### 5.3 Join

The application starts a join with a list of bootstrap peers. A node
connects first, and sends `SWARM_JOIN` only to a few of them:

1. It connects to up to `join_dials` bootstrap peers (default 5) at
   once, without a message.
2. The first `join_contacts` peers that connect (default 2) become
   *contacts*. Each contact gets `SWARM_JOIN`.
3. When a dial fails, or a contact sends `SWARM_CLOSE`, refuses the
   stream, or does not answer in time, the join tries the next
   bootstrap peer.
4. The other peers go to the passive view.

A contact answers with `SWARM_NEIGHBOR_REPLY` (section 5.4). The
joiner then adds the contact to its active view with high priority: if
the view is full, it evicts a random neighbor.

If no reply comes within `JOIN_RETRY_DELAY` (10 s), the joiner sends
`SWARM_JOIN` once more. A refused join gets no retry. A node remembers
some bootstrap peers (8, *policy*) for a later rejoin (section 5.8).

### 5.4 Contact and forward join

A node that receives `SWARM_JOIN` from peer J:

1. cancels a pending join retry or `SWARM_NEIGHBOR_REQUEST` to J,
2. adds J to its active view with high priority, and evicts a random
   neighbor if the view is full (section 5.6),
3. sends `SWARM_NEIGHBOR_REPLY` with its peer data to J, also if J was
   a neighbor already, and
4. if J was not a neighbor before, sends `SWARM_FORWARD_JOIN` with J's
   `PeerInfo` and `ttl = active_random_walk_length` (ARWL, default 6)
   to each other neighbor.

A node P that receives `SWARM_FORWARD_JOIN` for peer J from peer S
ignores it if S is not a neighbor of P, or if J is P. Else:

1. If J is a neighbor, P sends `SWARM_NEIGHBOR_REQUEST(High)` to J.
   This renews the link.
2. Else, if the ttl is 0, or P has at most one neighbor, P sends
   `SWARM_NEIGHBOR_REQUEST(High)` to J. P adds J to its active view
   only when J replies. The paper adds J at once. The change keeps
   unreachable peers out of the active view. If P reached its contact
   limit (section 11.4), P adds J to its passive view instead.
3. Else, P forwards the message with `ttl - 1` to a random neighbor
   other than S. If the ttl equals `passive_random_walk_length` (PRWL,
   default 3), P first adds J to its passive view.

A node sends at most one pending request per peer (section 5.5).

```{#fig-walk .figure}
          SWARM_JOIN        ttl 6         ttl 5         ttl 3          ttl 0
     J -------------> B ----------> P1 ---------> ... --------> P4 ... ---------> P7
     ^                                                          adds J to        |
     |                                                          passive view     |
     +-------------------- SWARM_NEIGHBOR_REQUEST(High) -------------------------+
     J -------------------- SWARM_NEIGHBOR_REPLY ------------------------------> P7
```

Figure 4: One random walk of a forward join, with ARWL 6 and PRWL 3.
B starts such a walk at each of its other neighbors.

### 5.5 Neighbor request and reply

`SWARM_NEIGHBOR_REQUEST` asks the receiver to add the sender to its
active view. It has a priority:

- `High` (0): the receiver MUST accept. If its active view is full, it
  evicts a random neighbor. A node sends `High` when its active view is
  empty, and for a forward join.
- `Low` (1): the receiver accepts only if its active view has a free
  slot. Else it refuses with `SWARM_CLOSE(NoCapacity)`. An unknown
  priority reads as `Low`.

On accept, the receiver answers with `SWARM_NEIGHBOR_REPLY`, unless
the request crossed its own pending request to the sender. In that
case neither side replies, and each side applies the priority rules.

`SWARM_NEIGHBOR_REPLY` carries the peer data of the sender. Its
receiver:

- adds the sender to its active view with high priority, if the reply
  matches a pending join or request,
- ignores the reply if it does not match and the sender is a neighbor,
  and
- answers with `SWARM_CLOSE(NoCapacity)` otherwise.

A request times out after `neighbor_request_timeout` (3 s, *policy*).
The node then removes the peer from its passive view and tries another
passive peer.

### 5.6 Close and reasons

`SWARM_CLOSE` asks the receiver to drop the link. It carries a reason:

- `Shutdown` (0): the sender leaves the topic, or is not on it. An
  unknown reason reads as `Shutdown`.
- `NoCapacity` (1): the sender stays on the topic and only drops the
  link. The sender sends it when it evicts a neighbor, refuses a `Low`
  request, or gets a reply it did not ask for.

A node on the topic sends `SWARM_SHUFFLE_REPLY` before each
`SWARM_CLOSE`, with passive peers first. A peer that just joined thus
does not run out of contacts.

The receiver of `SWARM_CLOSE` removes the sender from its active view.
With `NoCapacity`, it keeps the sender in the passive view. If the
close refused a pending request, the node skips the sender for `Low`
requests for a while. With `Shutdown`, it forgets the sender.

### 5.7 Shuffle

A node shuffles to keep its passive view fresh. The first shuffle comes
1 s after the topic starts. The interval doubles up to
`shuffle_interval` (60 s, *policy*), with a random variation of 25
percent.

To shuffle, a node picks a random neighbor N and sends it
`SWARM_SHUFFLE` with:

- `origin`: its own endpoint id,
- `nodes`: up to `shuffle_active_view_count` (3) neighbors and up to
  `shuffle_passive_view_count` (4) passive peers, none of them N, and
  its own `PeerInfo` last. At most `MAX_SHUFFLE_NODES` (15) nodes in
  total.
- `ttl`: `shuffle_random_walk_length` (default 6).

A node that receives `SWARM_SHUFFLE` from peer S ignores it if S is
not a neighbor, or if the origin is the node itself. Else:

- If the ttl is 0, or the node has at most one neighbor, it adds the
  nodes to its passive view. It then sends `SWARM_SHUFFLE_REPLY` to the
  origin, with up to as many peers as the shuffle carried, passive
  peers first. If the node reached its contact limit (section 11.4), it
  sends no reply.
- Else it forwards the shuffle with `ttl - 1` to a random neighbor
  other than the origin and S, if one exists.

The reply goes to the origin directly, which can need a new
connection. The origin adds the peers of the reply to its passive view.
A full passive view evicts a random peer for each new one.

### 5.8 Failure handling and refill

A node detects the failure of a neighbor by the end of the connection
(section 3.7), or by `SWARM_CLOSE`. It then removes the neighbor and
starts a *refill*: it sends `SWARM_NEIGHBOR_REQUEST` to a random
passive peer. The priority is `High` if the active view is empty, else
`Low`. When a dial fails or a request times out, the refill asks the
next passive peer.

A maintenance pass runs every 5 s (*policy*):

- A node with both views empty rejoins through its remembered bootstrap
  peers and its recent neighbors, with a backoff.
- A node with passive peers but no neighbor sends a `High` request.
- A node with less than half of its active view sends a few `Low`
  requests, with a backoff.

A node remembers its recent neighbors (8, *policy*) for rejoins. A
failed connection does not remove a peer from this list, because the
node itself may have lost its network.

### 5.9 Leaving a topic

To leave a topic, a node sends `SWARM_SHUFFLE_REPLY` and then
`SWARM_CLOSE(Shutdown)` to each neighbor. It then finishes the streams
of the topic, and drops all state of the topic.

## 6. Broadcast (Plumtree)

The broadcast layer follows Plumtree [PLUMTREE], with a tree per
sender, and the changes that this section states. It runs per topic,
over the active view.

### 6.1 Message ids and dedup

The *content* of a broadcast is the bytes that the application gave.
The *origin* of a broadcast is the node that sent it first. The
*message id* is the BLAKE3 hash [BLAKE3] of the content, 32 bytes. The
id is not on the wire: each receiver MUST compute it. Two broadcasts
with the same content are the same message.

A node remembers the ids of the swarm broadcasts it saw for
`message_id_retention` (90 s, *policy*). A copy of a known id is a
*duplicate*. A node delivers each swarm broadcast at most once in that
time. To answer grafts, a node caches the content of each swarm
broadcast for `message_cache_retention` (30 s, *policy*).

### 6.2 Scopes

A broadcast has one of two scopes, each with its own message type:

- Swarm (`GOSSIP_MESSAGE`): the message reaches every member of the
  topic. It has a round and a tree.
- Neighbors (`GOSSIP_MESSAGE_NEIGHBOR`): the message goes to the
  neighbors of the origin only. A receiver delivers it and does not
  forward it.

A node does not deduplicate, cache, or repair neighbor messages. The
id covers only the content, so a neighbor message with the id of a
known swarm broadcast is a duplicate, and the node drops it.

### 6.3 Rounds

The *round* of a swarm broadcast counts its hops. The origin sends
round 0. A node that forwards a message sends the round it received
plus one, up to 65535. Announcements carry the round that the node
would send. Rounds are not authenticated, and only steer the tree
optimization (section 6.8).

### 6.4 Eager and lazy push

A *tree* is one broadcast tree of a topic, named by an 8-byte tree id
(section 6.7). Each tree splits the neighbors of a node into two sets:

- An *eager peer* gets the content (`GOSSIP_MESSAGE`).
- A *lazy peer* gets only announcements of the ids (`GOSSIP_IHAVE`).

A new neighbor starts as an eager peer in every tree.

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

Figure 5: One tree. Eager links form a spanning tree. The lazy links
carry announcements that repair a gap.

When a node receives a new swarm broadcast from S, it:

1. asks the local filter, if any (section 6.10), and stops if the
   filter rejects it,
2. records the id, and caches the message,
3. creates the tree if it does not know it (section 6.7),
4. makes each peer that it grafted for this message eager,
5. sends `GOSSIP_MESSAGE` with the next round to each eager peer of
   the tree, except S and the grafted peers,
6. queues an announcement for each lazy peer of the tree, except S,
   and
7. delivers the content to the application.

The origin does steps 2, 5, and 6 with round 0. A node sends its
queued announcements to each lazy peer after `dispatch_timeout` (5 ms,
*policy*), as one `GOSSIP_IHAVE` or more.

A node also accepts a swarm broadcast from a peer that is not a
neighbor, often a former neighbor. Such a message changes no eager or
lazy set. A neighbor message goes to every neighbor.

### 6.5 IHave and Graft

`GOSSIP_IHAVE` announces messages. Each entry has the id, the round,
and the tree. For each entry with an id that the node did not see, the
node records the sender as an announcer of the id. If no graft chain
runs for the id, the node starts one, with a timer of `graft_timeout_1`
(80 ms, *policy*).

A *graft chain* asks the announcers of one id, one after the other.
When the timer expires and the message did not arrive, the node:

1. takes the first announcer that it did not ask yet,
2. makes it an eager peer in the tree of the announcement,
3. sends it `GOSSIP_GRAFT` with the id and the tree, and
4. starts a timer of `graft_timeout_2` (40 ms, *policy*), after which
   it asks the next announcer.

The chain ends when the message arrives, or when no announcer is left.

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

A node that receives `GOSSIP_GRAFT` from a neighbor makes the sender
an eager peer in the tree, if it knows the tree or it is the shared
tree. If the graft names an id that the node caches, the node sends
the message back. A graft without an id only changes the tree.

A node ignores `GOSSIP_IHAVE`, `GOSSIP_GRAFT`, and `GOSSIP_PRUNE` from
a peer that is not a neighbor.

### 6.6 Prune

`GOSSIP_PRUNE` names a tree. Its receiver makes the sender a lazy peer
in that tree, if it knows the tree or it is the shared tree. The sender
makes the receiver lazy in the same tree.

A node that receives a duplicate swarm broadcast from S applies these
rules in order:

1. If the duplicate is a late reply to a graft, the node handles it as
   in section 6.8.
2. If S is not a neighbor, the node does not know the tree, or S is
   not eager in it, the node sends `GOSSIP_PRUNE`.
3. If S is eager, the node prunes S, as in the paper. The exception is
   a message that the node first saw less than `graft_timeout_1` after
   its last prune in that tree. That message was in flight during the
   prune, and a second cut could split the tree.

A duplicate creates no tree.

### 6.7 Trees per sender

Each swarm broadcast names its tree in its `tree` field. A node keeps
an eager set and a lazy set per tree.

The *shared tree* has the id `00*8`. Any other tree id is a value that
an origin picks for its own broadcasts. A node that forwards or
announces a message MUST copy the tree id unchanged. A receiver does
not derive or check the id. An origin SHOULD use a tree id that is
stable across its broadcasts and unlikely to match the id of another
origin. The default is the first 8 bytes of the BLAKE3 hash of the
endpoint id of the origin. Two origins with the same tree id share a
tree, which costs latency, not delivery.

A node names the shared tree for its first broadcast (*policy*), and
its own tree from then on. The count starts again after the node did
not broadcast for 300 s.

The first copy of a swarm broadcast with an unknown tree id creates
the tree, as a copy of the eager and lazy sets of the shared tree.
Without a shared tree, every neighbor is eager. The copy makes the
switch of a sender to its own tree cheap: its first message on the new
tree follows the shared tree, so it does not flood. `GOSSIP_IHAVE`,
`GOSSIP_GRAFT`, and `GOSSIP_PRUNE` never create a tree.

A node keeps a limited number of trees per topic (1024, *policy*), and
evicts the least recently used one, never the shared tree. It drops a
tree that no message used for 300 s.

### 6.8 Tree optimization and late graft replies

Tree optimization follows the paper. When a new swarm broadcast from
neighbor S arrives after announcements of it, the node compares the
best announced round with the round that it forwards. If the announced
round is lower by `optimization_threshold` (default 7) or more, the
node sends `GOSSIP_GRAFT` without an id to that announcer, and
`GOSSIP_PRUNE` to S.

A node can graft peer G for a message, and then get the eager copy from
F before G answers. When G's copy arrives, the node keeps G eager and
sends `GOSSIP_PRUNE` to F if:

- G's copy took no more hops than F's copy,
- F is not a grafted peer of this message, and
- both F and G are eager in the tree.

G announced the message well before F's copy came, so its path is
faster.

### 6.9 Dropped messages

A full send queue can drop a payload or an announcement to a neighbor
(section 3.9). The node then announces each dropped swarm broadcast
that it still caches to that neighbor again, at a limited rate. A
dropped neighbor message is lost.

### 6.10 Message filter

A node can install a filter. The filter sees the topic, the id, the
content, the scope, and the peer that delivered the message. It runs
once per new id, before the node caches, forwards, announces, or
delivers the message. A rejected id counts as seen. The only effect on
the wire is that a rejected message does not travel on.

## 7. Error handling

A reader handles each problem in one of four ways:

1. **Skip.** The reader ignores the item and reads on.
2. **Default.** The reader reads an unknown value as a defined default.
3. **Refuse the stream.** The verifier rejects the stream (section 4.3).
4. **Fail the connection.** The node closes the connection with a close
   code. The other topics on the connection fail with it.

A *protocol error* is a violation of the encoding or of the connection
rules. It fails the connection with `PROTOCOL`. A membership or
broadcast message that is well formed but makes no sense in the current
state is no protocol error. The receiver handles it as sections 5 and
6 say, often by ignoring it.

A reader skips:

- an unknown header parameter,
- the bytes after the known fields of a message, and
- a payload over the content limit of the reader (section 9.8). Only
  that message is lost.

A reader reads an unknown `Priority` as `Low`, an unknown
`CloseReason` as `Shutdown`, and keeps peer data of an unknown kind.

A reader fails the connection with `PROTOCOL` on:

- a message type that is not permitted on the stream (section 8.2),
- a stream kind that no negotiated extension defines,
- a frame other than a payload over `MAX_CONTROL_SIZE`, peer data over
  `MAX_PEER_DATA_LEN`, or a shuffle over `MAX_SHUFFLE_NODES`,
- a frame or a header that does not decode, or a stream that ends
  inside a frame,
- a header that is late, too long, or misses a required parameter,
- a violation of the `GOAWAY` rules (section 3.6), and
- a reset topic stream.

A node fails the connection with `SEND_FAILED` or `BLOCKED` for the
local causes of section 3.7. A node closes with the first failure it
sees.

## 8. Extensibility

### 8.1 Model

The protocol grows by extensions, not by new ALPNs. An *extension*
adds items that an older reader does not know, such as message types,
control messages, stream kinds, or codes. An older reader fails the
connection on such an item. So a node sends the item only to a peer
that declared support for the extension.

A node *declares* an extension with a parameter in a header. An older
reader skips the unknown parameter, and declares nothing in its own
header. A newer node then sees no declaration, and keeps to the base
protocol.

### 8.2 Declaration and negotiation

Each extension has one header parameter id and a scope:

- **Connection scope.** The parameter goes in the control header. The
  extension is *negotiated* on the connection when both control
  headers carry it.
- **Topic scope.** The parameter goes in topic headers. The extension
  is negotiated for one topic on one connection when a topic header of
  that topic from each side carries it.

The value of the parameter is empty, unless the extension defines a
value. A stream *permits* the message types of the base protocol for
that stream, and the types of the extensions negotiated for it.

Rules:

- A node MUST NOT declare an extension that it does not implement.
- A node MUST send the same topic-scope parameters in every topic
  header of one topic on one connection.
- A node MUST NOT send an item of an extension before it read the
  declaration of the peer.
- An extension MUST leave the base protocol valid on a link where it
  is not negotiated.
- An extension that defines a stream kind MUST say how its streams
  count for the idle time and for `GOAWAY`.

Negotiation covers one link. A change that every node of a topic must
apply cannot be negotiated per link.

### 8.3 Changes without negotiation

Some changes need no declaration, because an older reader skips or
defaults the new item:

- A new field at the end of a message, if a reader can ignore it. The
  message MUST stay within the limit of its type.
- A new header parameter, if its absence leaves the base behavior
  valid.
- A new `Priority` or `CloseReason` value that is safe to read as the
  default.
- A new kind of peer data. Older nodes keep and forward it.

### 8.4 Changes that need a new ALPN

- A change that a reader needs before it can read the declarations of
  the peer, such as the varints, the frames, or the headers.
- A change of a fixed limit (section 9.8). With a higher limit, an
  older reader fails the connection on a larger item.
- A change to the meaning or layout of an existing item, if the old and
  the new form cannot both stay valid on one link.
- A change that every node of a topic must apply, such as the message
  id.

The tree id of an origin is no wire value, so a change of its
derivation needs no new ALPN.

## 9. Encoding

### 9.1 Varints

All integers are unsigned LEB128 varints, as in postcard [POSTCARD].
Each byte holds seven value bits, the least significant group first.
Bit 7 is set on each byte except the last.

| Type | Max bytes | Max value of the last byte |
|---|---|---|
| `u16` | 3 | `0x03` |
| `u32` | 5 | `0x0f` |
| `u64` | 10 | `0x01` |

A sender MUST use the shortest form. A reader MUST reject a varint
that is longer than the maximum, or that does not fit its type.

Examples: `0` is `00`. `127` is `7f`. `128` is `80 01`. `300` is
`ac 02`. `16384` is `80 80 01`.

Frame lengths, header kinds, parameter ids, and parameter lengths are
`u64` varints.

### 9.2 Field types

The fields of a message are the postcard encoding [POSTCARD] of the
fields of its type, in order:

| Type | Encoding |
|---|---|
| `varint` | A `u64` varint. |
| `u32`, `u16` | A varint of that type. |
| `[u8; N]` | N bytes, no length. |
| `bytes` | `length: varint`, then that many bytes. |
| `Vec<T>` | `count: varint`, then that many values of `T`. |
| `Option<T>` | `00` for none, or `01` then `T`. Any other byte is an error. |
| `EndpointId` | 32 bytes, a valid compressed Ed25519 point. |
| `PeerData` | `kind: varint`, then `data: bytes` (section 9.7). |
| `PeerInfo` | `id: EndpointId`, then `data: Option<PeerData>`. |
| `TreeId` | `[u8; 8]`. |
| `MessageId` | `[u8; 32]`. |

### 9.3 Frames and messages

Each stream carries a sequence of frames. A frame is a length, then
that many bytes. The first frame of a stream is the header. Each later
frame carries one message:

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

A sender MUST NOT send a message frame of length 0. A reader ignores
the bytes after the fields that it knows (section 8.3).

A reader reads each message frame in these steps:

1. It reads the length and the type, and picks the limit of the type
   (section 9.8).
2. If the length is over the limit, it skips the rest of a payload
   frame without keeping it. For any other type, it fails the
   connection before it reads the rest.
3. Else it reads the frame and decodes the fields.
4. It skips a payload with content longer than C, and fails the
   connection on peer data or shuffle nodes over their limits.

A topic stream that ends between two frames ends normally (section
4.5).

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

The kind MUST match the stream: kind 0 on the control stream, kind 1
on a topic stream. Another kind on a unidirectional stream belongs to
an extension (section 8.2). The payload of a header frame MUST NOT
exceed `MAX_HEADER_LEN` (4096 bytes). Headers do not count against
`MAX_CONTROL_SIZE`.

Control header (kind `0x00`):

| Param | Name | Value | Rule |
|---|---|---|---|
| `0x01` | `SESSION` | 8 bytes | Required. |

Topic header (kind `0x01`):

| Param | Name | Value | Rule |
|---|---|---|---|
| `0x00` | `TOPIC_ID` | 32 bytes | Required. |
| `0x02` | `TOKEN` | opaque bytes | Optional. At most 4058 bytes next to the topic. |

### 9.5 Control messages

```text
GOAWAY {
  type    varint = 0x20
  opened  varint     (topic streams the sender opened on this connection)
}
```

`GOAWAY(5)` is the frame `02 20 05`.

### 9.6 Topic messages

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
  content  bytes
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
  content  bytes
}
```

`GOSSIP_MESSAGE` and `GOSSIP_MESSAGE_NEIGHBOR` are the *payloads*.

### 9.7 Peer data

```text
PeerData {
  kind    varint
  length  varint             (at most MAX_PEER_DATA_LEN)
  data    [u8; length]
}
```

Kind 0 holds the postcard encoding [POSTCARD] of:

```text
AddrInfo {
  relay_url         Option<string>      (a varint length and UTF-8 bytes of a URL)
  direct_addresses  Vec<SocketAddr>
}

SocketAddr {
  family  varint              (0 IPv4, 1 IPv6)
  ip      [u8; 4] or [u8; 16] (by family)
  port    u16
}
```

A sender SHOULD sort the addresses and SHOULD NOT repeat one. A reader
MUST NOT depend on the order. Empty data of kind 0 gives no addresses.

### 9.8 Limits

The fixed limits are wire values, the same on all nodes. A sender MUST
NOT exceed one, and a reader fails the connection with `PROTOCOL` on an
item over one:

| Name | Value | Limits |
|---|---|---|
| `MAX_CONTROL_SIZE` | 16384 bytes | The frame of each message other than a payload. |
| `MAX_PEER_DATA_LEN` | 1024 bytes | The data of one `PeerData`. |
| `MAX_SHUFFLE_NODES` | 15 | The nodes of one `SWARM_SHUFFLE` or `SWARM_SHUFFLE_REPLY`. |
| `MAX_HEADER_LEN` | 4096 bytes | The payload of a header frame. |

The limits fit together: a `SWARM_SHUFFLE` with 15 nodes of full peer
data takes at most 16072 bytes. A node splits its announcements so
that each `GOSSIP_IHAVE` fits `MAX_CONTROL_SIZE`.

The *content limit* C is local, with a default of 4096 bytes. It bounds
the content of a payload. C is not announced, so the nodes of one
network SHOULD use the same C. A node does not send a broadcast with
content over its own C.

The *payload limit* P(C) bounds the frame of a payload. It is the size
of a `GOSSIP_MESSAGE` with C bytes of content at the largest round:

```text
P(C) = 1 + len(varint(C)) + C + 3 + 8
```

For C = 4096, P(C) is 4110 bytes. A reader skips a payload frame over
P(C) without reading it into memory. It also skips a payload whose
content is longer than C. Only that message is lost, and the connection
stays open.

## 10. Registries

### 10.1 Stream kinds

| Value | Name | Stream |
|---|---|---|
| `0x00` | `CONTROL` | The bidirectional control stream |
| `0x01` | `TOPIC` | A unidirectional topic stream |

### 10.2 Header parameters

| Value | Name | Kind | Value | Presence |
|---|---|---|---|---|
| `0x00` | `TOPIC_ID` | `TOPIC` | 32 bytes | Required |
| `0x01` | `SESSION` | `CONTROL` | 8 bytes | Required |
| `0x02` | `TOKEN` | `TOPIC` | opaque | Optional |

### 10.3 Message types

| Value | Name | Stream |
|---|---|---|
| `0x01` | `SWARM_JOIN` | topic |
| `0x02` | `SWARM_FORWARD_JOIN` | topic |
| `0x03` | `SWARM_SHUFFLE` | topic |
| `0x04` | `SWARM_SHUFFLE_REPLY` | topic |
| `0x05` | `SWARM_NEIGHBOR_REQUEST` | topic |
| `0x06` | `SWARM_NEIGHBOR_REPLY` | topic |
| `0x07` | `SWARM_CLOSE` | topic |
| `0x10` | `GOSSIP_MESSAGE` | topic |
| `0x11` | `GOSSIP_PRUNE` | topic |
| `0x12` | `GOSSIP_GRAFT` | topic |
| `0x13` | `GOSSIP_IHAVE` | topic |
| `0x14` | `GOSSIP_MESSAGE_NEIGHBOR` | topic |
| `0x20` | `GOAWAY` | control |

### 10.4 Codes

Connection close codes: 1 `RETIRED`, 2 `STALE`, 3 `SHUTDOWN`, 4
`PROTOCOL`, 5 `SEND_FAILED`, 6 `BLOCKED` (section 3.7). Code 0 and
unknown codes mean a failure.

Stop codes of topic streams: 1 `REJECTED` (section 4.3).

## 11. Security considerations

### 11.1 Identity and authorization

iroh authenticates the endpoint id of a peer. The protocol trusts
nothing else about a peer. Messages carry no signature, and a peer can
forward any content. The message id proves the integrity of the
content, not its author. Anyone can name any tree.

An auth token gates what a node reads on a topic. The scope is narrow:

- The token goes to every peer to which the node opens a stream of the
  topic, and that peer can replay it. A verifier SHOULD bind a token to
  the endpoint id of the peer.
- A verdict holds for one stream. A new connection asks the verifier
  again.
- A verifier does not stop an accepted member from leaking, dropping,
  or flooding content.

### 11.2 Resource use by a peer

The fixed limits (section 9.8), the content limit, and the header
limits bound the memory of one message. The QUIC stream limit bounds
the streams of a peer. The send queue has a size limit, and walks that
the node forwards cannot fill it (section 3.9). Each table that a peer
can grow has a count limit: rejected topics, older sessions, trees, and
re-announced ids.

The message cache, the seen ids, and the filter verdicts have no count
limit. Time bounds them. A peer that sends many distinct broadcasts
fills them at the rate of its link. A node that takes data from peers
it does not trust SHOULD install a filter, or shorten the retention.

### 11.3 Made-up tree ids

A swarm broadcast with a new tree id creates a tree, after the filter
accepted it. A peer that sends broadcasts with many tree ids evicts
real trees. The tree limit bounds the memory, and never evicts the
shared tree. An evicted tree comes back as a copy of the shared tree.
The cost is latency, not lost messages. Announcements, grafts, and
prunes never create a tree.

### 11.4 Contacts caused by peers

`SWARM_SHUFFLE` and `SWARM_FORWARD_JOIN` make the end of a walk contact
a peer that the sender names. A peer could thus make a node dial
endpoint ids of its choice. iroh authenticates the other end, so a
dial gives no access, but it costs work. Two rules bound the cost:

- A node MUST ignore both messages from a peer that is not a neighbor.
- A node SHOULD limit the rate of the contacts that they cause. The
  default is 32 peers per topic in each maintenance pass. At the limit,
  the node adds the peer of a walk to its passive view, and sends no
  shuffle reply.

### 11.5 Rounds and announcements

Rounds are not authenticated. A peer that announces low rounds can
make itself eager in many trees. The worst outcome is a slower tree.
A neighbor can also announce ids that it does not have. Each such id
costs a few grafts.

### 11.6 0-RTT

A node MUST NOT process gossip messages that arrive as QUIC 0-RTT data
before the handshake completes.

## 12. References

### 12.1 Normative references

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

### 12.2 Informative references

[HYPARVIEW]
:   Leitão, J., Pereira, J., Rodrigues, L., "HyParView: a membership
    protocol for reliable gossip-based broadcast", DSN 2007,
    DOI 10.1109/DSN.2007.56.

[PLUMTREE]
:   Leitão, J., Pereira, J., Rodrigues, L., "Epidemic broadcast trees",
    SRDS 2007, DOI 10.1109/SRDS.2007.27.

## Appendix A. Test vectors

Symbols:

- `P` is the endpoint id of the secret key `05*32`:
  `6e7a1cdd29b0b78fd13af4c5598feff4ef2a97166e3ca6f2e4fbfccd80505bf1`.
- `I` is the message id of the content `hi` (`6869`):
  `85052e9aab1b67b6622d94a08441b09fd5b7aca61ee360416d70de5da67d86ca`.
- `T` is the tree id `33*8`.

### A.1 Headers

Topic header frame for topic `11*32`:

```text
23                  length 35
01                  kind TOPIC
00 20 11*32         TOPIC_ID, length 32, topic
```

The same header payload with the token `secret`:

```text
01 00 20 11*32 02 06 736563726574
```

Control header frame for session `0102030405060708`:

```text
0b 00 01 08 0102030405060708
```

### A.2 Messages

Each payload goes in a frame after its length. For example,
`SWARM_JOIN(None)` is the frame `02 01 00`.

| Message | Payload |
|---|---|
| `GOAWAY(300)` | `20 ac02` |
| `SWARM_JOIN(None)` | `01 00` |
| `SWARM_JOIN(kind 0, aabbcc)` | `01 01 00 03 aabbcc` |
| `SWARM_FORWARD_JOIN(P, None, ttl 6)` | `02 P 00 06` |
| `SWARM_SHUFFLE(origin P, [P, None], ttl 6)` | `03 P 01 P 00 06` |
| `SWARM_SHUFFLE_REPLY([P, kind 7, 01])` | `04 01 P 01 07 01 01` |
| `SWARM_NEIGHBOR_REQUEST(High, kind 0, 010203)` | `05 00 01 00 03 010203` |
| `SWARM_NEIGHBOR_REPLY(None)` | `06 00` |
| `SWARM_CLOSE(NoCapacity)` | `07 01` |
| `GOSSIP_MESSAGE("hi", round 300, T)` | `10 02 6869 ac02 T` |
| `GOSSIP_PRUNE(T)` | `11 T` |
| `GOSSIP_GRAFT(Some(I), T)` | `12 01 I T` |
| `GOSSIP_GRAFT(None, T)` | `12 00 T` |
| `GOSSIP_IHAVE([(I, round 2, T)])` | `13 01 I 02 T` |
| `GOSSIP_MESSAGE_NEIGHBOR("hi")` | `14 02 6869` |

A reader reads `01 00 dead` as `01 00` (bytes after the fields),
`05 09 00` as `05 01 00` (unknown priority), and `07 09` as `07 00`
(unknown reason). It fails on `10 02 6869` (no round) and on
`10 00 ffffff7f` (round over a `u16`).

### A.3 Peer data

Kind 0 with no relay and the address `127.0.0.1:4433`:

```text
00                  kind 0, addresses
09                  data length 9
00                  relay_url: none
01                  one address
00                  family IPv4
7f000001            127.0.0.1
d122                port 4433
```

## Appendix B. Defaults

All values are *policy*, except the fixed limits of section 9.8.

| Name | Default |
|---|---|
| Content limit C | 4096 bytes |
| Send queue per peer | 1 MiB |
| `HEADER_TIMEOUT` | 10 s |
| `WRITE_TIMEOUT` | 60 s |
| `OPEN_TIMEOUT` | 15 s |
| `RETIRE_TIMEOUT` | 120 s |
| `retire_after` | 10 s |
| Shutdown wait | 1 s |
| `active_view_capacity` | 5 |
| `passive_view_capacity` | 30 |
| `active_random_walk_length` (ARWL) | 6 |
| `passive_random_walk_length` (PRWL) | 3 |
| `shuffle_random_walk_length` | 6 |
| `shuffle_active_view_count` | 3 |
| `shuffle_passive_view_count` | 4 |
| `shuffle_interval` | 1 s at first, doubling up to 60 s |
| `neighbor_request_timeout` | 3 s |
| `join_dials` | 5 |
| `join_contacts` | 2 |
| `JOIN_RETRY_DELAY` | 10 s |
| Maintenance interval | 5 s |
| Contacts caused by peers | 32 per topic and maintenance pass |
| `graft_timeout_1` | 80 ms |
| `graft_timeout_2` | 40 ms |
| `dispatch_timeout` | 5 ms |
| `optimization_threshold` | 7 |
| `message_cache_retention` | 30 s |
| `message_id_retention` | 90 s |
| Tree of an origin | from its second broadcast |
| Trees per topic | 1024, each dropped after 300 s unused |
