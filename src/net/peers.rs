//! The state of each peer, and the dials to peers.
//!
//! [`PeerState`] owns every decision about the connections of one peer: which
//! connection writes, and when a restart of the peer resets the protocol. It
//! does no I/O. Each method returns [`Action`]s, which the actor performs.
//!
//! The reader of the queue moves between the state and the writers. The state
//! keeps it while no connection writes. [`Action::TakeQueue`] moves it to a
//! writer, which gives it back when its connection ends.

use std::collections::VecDeque;

use iroh::{endpoint::Side, EndpointId};
use rustc_hash::FxHashMap;
use tokio_util::sync::CancellationToken;

use super::{
    queue::{Pushed, QueueReader, SendQueue},
    wire::{CloseCode, Session},
    ProtoMessage,
};

/// The maximum number of older sessions that the state of a peer keeps.
const MAX_OLD_SESSIONS: usize = 16;

/// The id of a connection. A lower id belongs to an older connection.
pub(super) type ConnId = u64;

/// How a connection ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConnEnd {
    /// The retire finished: the close had the code `Retired`.
    Retired,
    /// The peer or the network ended the connection in another way.
    Failed,
    /// The peer closed with `Stale`, as it took us for a former instance.
    ///
    /// The actor reports nothing.
    Quiet,
}

/// A connection of a peer, with the data that [`PeerState`] needs for its decisions.
#[derive(Debug, Clone)]
struct ConnEntry {
    id: ConnId,
    side: Side,
    /// The session of the peer, after we read its control header.
    session: Option<Session>,
    /// Whether the connection retires. It writes nothing more, so `settle` never picks it.
    retiring: bool,
}

/// An action that the actor performs for a [`PeerState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    /// Closes the connection with `code`. The state removed the connection already.
    Close(ConnId, CloseCode),
    /// Moves the reader of the queue to this connection, see [`PeerState::reader_for`].
    TakeQueue(ConnId),
    /// Tells the protocol that the peer disconnected.
    Disconnect,
    /// Retires this connection, which lost a crossed dial.
    Retire(ConnId),
    /// Dials the peer.
    Dial,
}

/// The state of one peer: its queue, its connections, and its session.
///
/// The queue always exists. When it is closed, the writer sends what is left
/// and finishes its streams.
#[derive(Debug)]
pub(super) struct PeerState {
    pub(super) queue: SendQueue,
    /// Closes every connection of the peer when cancelled, also the receive-only ones.
    ///
    /// An abort cancels it, and puts a new token in its place for later connections.
    pub(super) aborted: CancellationToken,
    /// The reader of the queue, while no connection has it.
    reader: Option<QueueReader>,
    conns: Vec<ConnEntry>,
    /// Whether our endpoint id is lower than the endpoint id of the peer.
    we_are_lower: bool,
    /// The connection that has the reader of the queue.
    ///
    /// If it is `None`, the state has the reader.
    writer: Option<ConnId>,
    /// Whether we dialed while every connection retires. The next writer clears it.
    retire_dialed: bool,
    /// The current session of the peer.
    session: Option<Session>,
    /// The latest older sessions of the peer, which a former instance may still use.
    ///
    /// A peer that restarts before its old connection closes adds one each
    /// time, and the state of the peer never becomes idle. Hence the cap.
    old_sessions: VecDeque<Session>,
}

impl PeerState {
    /// Creates the state of a peer with a closed, empty queue.
    fn new(queue_size: usize, we_are_lower: bool) -> Self {
        let (queue, reader) = SendQueue::new(queue_size);
        queue.close();
        Self {
            queue,
            reader: Some(reader),
            aborted: CancellationToken::new(),
            conns: Vec::new(),
            we_are_lower,
            writer: None,
            retire_dialed: false,
            session: None,
            old_sessions: VecDeque::new(),
        }
    }

    /// Creates the state of a peer with an open, empty queue, with us as the lower id.
    #[cfg(test)]
    pub(super) fn open(queue_size: usize) -> Self {
        let state = Self::new(queue_size, true);
        state.queue.open();
        state
    }

    /// Returns whether we must dial: no connection will write what the peer needs.
    ///
    /// Without a connection, we dial while a topic uses the peer. While every
    /// connection retires, we dial for a queued message, so that the next
    /// connection is ready when the retire ends. That dial happens once per
    /// writer: the next connection waits for the reader, and may retire as idle.
    pub(super) fn needs_dial(&self) -> bool {
        if self.conns.is_empty() {
            !self.queue.is_closed()
        } else {
            let all_retire = self.conns.iter().all(|entry| entry.retiring);
            !self.retire_dialed && !self.queue.is_empty() && all_retire
        }
    }

    /// Notes a dial, so that [`Self::needs_dial`] asks for no other one until the writer changes.
    pub(super) fn on_dial(&mut self) {
        self.retire_dialed = true;
    }

    /// Returns whether the peer has no connection, and a closed, empty queue.
    pub(super) fn is_idle(&self) -> bool {
        self.conns.is_empty() && self.queue.is_closed() && self.queue.is_empty()
    }

    pub(super) fn has_conn(&self, id: ConnId) -> bool {
        self.conns.iter().any(|entry| entry.id == id)
    }

    pub(super) fn conn_count(&self) -> usize {
        self.conns.len()
    }

    #[cfg(test)]
    pub(super) fn writer(&self) -> Option<ConnId> {
        self.writer
    }

    /// Takes the reader of the queue, as a writer would.
    #[cfg(test)]
    pub(super) fn take_reader(&mut self) -> Option<QueueReader> {
        self.reader.take()
    }

    /// Returns the reader for the writer of `id`, which [`Action::TakeQueue`] named.
    ///
    /// Returns `None` if the state picked another writer since, or discarded the queue.
    pub(super) fn reader_for(&mut self, id: ConnId) -> Option<QueueReader> {
        if self.writer == Some(id) {
            self.reader.take()
        } else {
            None
        }
    }

    /// Queues a message without waiting, and opens the queue.
    ///
    /// Waiting for room would stop the actor, and with it every topic, for as
    /// long as this one peer does not read. On [`Pushed::Overflow`], the caller
    /// aborts the peer.
    pub(super) fn push(&mut self, message: ProtoMessage, len: Option<usize>) -> Pushed {
        self.queue.open();
        self.queue.push(message, len)
    }

    /// Queues a test message with its encoded size.
    #[cfg(test)]
    pub(super) fn send(&mut self, message: ProtoMessage) -> Pushed {
        let len = super::wire::message_len(&message.message).ok();
        self.push(message, len)
    }

    /// Replaces the queue with a closed, empty queue, whose reader the state keeps.
    ///
    /// A writer of the old queue still sends what it has, until it takes a new
    /// queue or its connection ends. The state drops the old reader if it comes back.
    fn discard_queue(&mut self) {
        self.retire_dialed = false;
        let (queue, reader) = SendQueue::new(self.queue.size());
        queue.close();
        self.queue = queue;
        self.reader = Some(reader);
        self.writer = None;
    }

    /// Discards the queue, and closes every connection of the peer.
    ///
    /// The caller aborts a peer when a control message does not fit into its
    /// queue. If we dropped only that message and sent the ones after it, the
    /// peer would act on the later messages without the dropped one. Its view
    /// of us would then differ from our view of it, and neither side would
    /// notice. So no writer may send the rest of the queue. Instead, both sides
    /// see a disconnect and start over. The connections that only receive also
    /// close, so that the peer learns of the abort on the connection that it
    /// writes on.
    ///
    /// The state removes the closing connections at once. Thus a later message
    /// dials, and the end of an old connection does not discard the new queue.
    pub(super) fn abort(&mut self) {
        self.aborted.cancel();
        self.aborted = CancellationToken::new();
        self.conns.clear();
        self.discard_queue();
    }

    /// Adds a connection, and picks the writer with [`Self::settle`].
    pub(super) fn add_conn(&mut self, id: ConnId, side: Side, out: &mut Vec<Action>) {
        self.conns.push(ConnEntry {
            id,
            side,
            session: None,
            retiring: false,
        });
        self.settle(out);
    }

    /// Handles the control header of the peer on a connection.
    ///
    /// A new session means that the peer restarted. An older session belongs
    /// to a former instance of the peer, and only its connection closes.
    ///
    /// Crossed dials end in one connection. Once we have the headers of all
    /// connections, the connection that the higher endpoint id dialed loses, and
    /// both sides retire it. The writer moves to the other connection after the
    /// loser closed, so the peer reads the messages of a topic in order.
    pub(super) fn on_header(&mut self, id: ConnId, session: Session, out: &mut Vec<Action>) {
        if !self.has_conn(id) {
            return;
        }
        if self.old_sessions.contains(&session) {
            // After a restart, a dialed connection writes only after its header.
            debug_assert_ne!(self.writer, Some(id), "a former instance has the reader");
            let dialed = self
                .conns
                .iter()
                .any(|entry| entry.id == id && entry.side.is_client());
            self.conns.retain(|entry| entry.id != id);
            out.push(Action::Close(id, CloseCode::Stale));
            // `on_closed` does not find the connection, so it cannot act.
            if dialed {
                // Our dial reached a former instance. The protocol decides on a retry.
                self.on_dial_failed(out);
            } else if self.conns.is_empty() && !self.queue.is_empty() {
                // The former instance dialed us. A dial of ours reaches the live instance, or
                // fails as above.
                out.push(Action::Dial);
            }
            return;
        }
        if let Some(old) = self.session.filter(|old| *old != session) {
            self.conns.retain(|entry| {
                let stale = entry.session == Some(old);
                if stale {
                    out.push(Action::Close(entry.id, CloseCode::Stale));
                }
                !stale
            });
            self.discard_queue();
            if self.old_sessions.len() == MAX_OLD_SESSIONS {
                self.old_sessions.pop_front();
            }
            self.old_sessions.push_back(old);
            out.push(Action::Disconnect);
        }
        self.session = Some(session);
        if let Some(entry) = self.conns.iter_mut().find(|entry| entry.id == id) {
            entry.session = Some(session);
        }
        // A retiring connection takes no part. A dial during its retire is not a crossed dial.
        let we_are_lower = self.we_are_lower;
        let loses =
            move |entry: &ConnEntry| !entry.retiring && entry.side.is_client() != we_are_lower;
        let live = || self.conns.iter().filter(|entry| !entry.retiring);
        if live().all(|entry| entry.session.is_some()) && !live().all(loses) {
            for entry in self.conns.iter_mut().filter(|entry| loses(entry)) {
                entry.retiring = true;
                out.push(Action::Retire(entry.id));
            }
        }
        self.settle(out);
    }

    /// Marks a connection that started to retire, and dials for a queued message if all retire.
    ///
    /// Only the retire of the writer, or one without a writer, dials. A
    /// connection that waits for the reader of a retiring writer is idle, and
    /// retires too. The close of the writer then dials once.
    pub(super) fn on_retiring(&mut self, id: ConnId, out: &mut Vec<Action>) {
        let Some(entry) = self.conns.iter_mut().find(|entry| entry.id == id) else {
            return;
        };
        entry.retiring = true;
        if self.writer.is_none_or(|writer| writer == id) && self.needs_dial() {
            self.on_dial();
            out.push(Action::Dial);
        }
    }

    /// Handles the end of a connection, and takes back the reader that its writer still had.
    pub(super) fn on_closed(
        &mut self,
        id: ConnId,
        end: ConnEnd,
        reader: Option<QueueReader>,
        out: &mut Vec<Action>,
    ) {
        let was_writer = self.writer == Some(id);
        if let Some(reader) = reader {
            self.take_back(id, reader);
        }
        if was_writer {
            self.writer = None;
            self.retire_dialed = false;
        }
        let Some(pos) = self.conns.iter().position(|entry| entry.id == id) else {
            self.settle(out);
            return;
        };
        self.conns.remove(pos);
        let was_last = self.conns.is_empty();
        // Only the close of the writer, or one without a writer, dials. A
        // connection that waited for the reader leaves the dial to the writer.
        // Without a connection, an open queue dials too, as in `needs_dial`.
        let all_retire = self.conns.iter().all(|entry| entry.retiring);
        let dials = all_retire && self.writer.is_none() && !self.queue.is_empty();
        let wanted = was_last && !self.queue.is_closed();
        if end == ConnEnd::Retired && (dials || wanted) {
            self.on_dial();
            out.push(Action::Dial);
        } else if end != ConnEnd::Quiet
            && ((end == ConnEnd::Failed && was_writer) || (was_last && !self.queue.is_closed()))
        {
            self.discard_queue();
            out.push(Action::Disconnect);
        }
        self.settle(out);
    }

    /// Stores a reader that comes back from the writer of `id`.
    ///
    /// It does not store the reader if the state discarded its queue.
    fn take_back(&mut self, id: ConnId, reader: QueueReader) {
        if reader.reads(&self.queue) {
            debug_assert_eq!(
                self.writer,
                Some(id),
                "a reader came back from another connection"
            );
            debug_assert!(self.reader.is_none(), "the reader came back twice");
            self.writer = None;
            self.reader = Some(reader);
        }
    }

    /// Handles a failed dial.
    ///
    /// If the peer has no connection, it reports a disconnect.
    pub(super) fn on_dial_failed(&mut self, out: &mut Vec<Action>) {
        if self.conns.is_empty() {
            self.discard_queue();
            out.push(Action::Disconnect);
        }
    }

    /// Picks the first ready connection as the writer, if no connection has the reader.
    ///
    /// A connection is ready after the header of the peer. A dialed connection
    /// is ready at once, unless the peer restarted before. It may then lead to
    /// a former instance, and what it writes there is lost. A retiring
    /// connection is never ready.
    fn settle(&mut self, out: &mut Vec<Action>) {
        if self.writer.is_some() {
            return;
        }
        // Each end of a task returns the reader, so this is a safety net.
        debug_assert!(self.reader.is_some(), "a writer lost the reader");
        if self.reader.is_none() {
            self.discard_queue();
            out.push(Action::Disconnect);
        }
        let early = self.old_sessions.is_empty();
        let ready = self
            .conns
            .iter()
            .filter(|entry| !entry.retiring)
            .find(|entry| entry.session.is_some() || (early && entry.side.is_client()));
        if let Some(entry) = ready {
            self.writer = Some(entry.id);
            self.retire_dialed = false;
            out.push(Action::TakeQueue(entry.id));
        }
    }
}

/// The state of all peers.
#[derive(Debug)]
pub(super) struct Peers {
    map: FxHashMap<EndpointId, PeerState>,
    me: EndpointId,
    /// The size of the queue of a new peer.
    pub(super) queue_size: usize,
}

impl Peers {
    pub(super) fn new(me: EndpointId, queue_size: usize) -> Self {
        Self {
            map: Default::default(),
            me,
            queue_size,
        }
    }

    /// Returns the state of `peer`, and creates it if it does not exist.
    pub(super) fn get_or_insert(&mut self, peer: EndpointId) -> &mut PeerState {
        let (me, queue_size) = (self.me, self.queue_size);
        self.map
            .entry(peer)
            .or_insert_with(|| PeerState::new(queue_size, me < peer))
    }
}

impl std::ops::Deref for Peers {
    type Target = FxHashMap<EndpointId, PeerState>;

    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl std::ops::DerefMut for Peers {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.map
    }
}
