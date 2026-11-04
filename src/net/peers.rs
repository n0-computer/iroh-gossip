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

use bytes::Bytes;
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
    pub(super) fn push(
        &mut self,
        message: ProtoMessage,
        len: Option<usize>,
        token: Option<Bytes>,
    ) -> Pushed {
        self.queue.open();
        self.queue.push(message, len, token)
    }

    /// Queues a test message with its encoded size.
    #[cfg(test)]
    pub(super) fn send(&mut self, message: ProtoMessage) -> Pushed {
        let len = super::wire::message_len(&message.message).ok();
        self.push(message, len, None)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{actor::tests::join_message, queue::MIN_QUEUE_SIZE};

    /// The state of a peer with an open queue.
    fn state() -> PeerState {
        let state = PeerState::new(MIN_QUEUE_SIZE, true);
        state.queue.open();
        state
    }

    /// Adds a connection, and its header unless we dialed it.
    fn add(state: &mut PeerState, id: ConnId, dialed: bool, session: u8) -> Vec<Action> {
        let mut out = Vec::new();
        let side = if dialed { Side::Client } else { Side::Server };
        state.add_conn(id, side, &mut out);
        if !dialed {
            state.on_header(id, Session([session; 8]), &mut out);
        }
        out
    }

    /// A dialed connection is the writer at once, an accepted one after its header.
    #[test]
    fn dialed_writes_at_once_accepted_after_header() {
        let mut out = Vec::new();
        state().add_conn(1, Side::Client, &mut out);
        assert_eq!(out, vec![Action::TakeQueue(1)]);
        let mut at_me = state();
        let mut out = Vec::new();
        at_me.add_conn(2, Side::Server, &mut out);
        assert!(out.is_empty(), "{out:?}");
        at_me.on_header(2, Session([1; 8]), &mut out);
        assert_eq!(out, vec![Action::TakeQueue(2)]);
    }

    /// A peer that restarts again and again leaves only the latest sessions in the state.
    #[test]
    fn old_sessions_keep_the_latest() {
        let mut state = state();
        let count = MAX_OLD_SESSIONS as u8 + 4;
        for i in 0..count {
            add(&mut state, i.into(), false, i);
        }
        assert_eq!(state.old_sessions.len(), MAX_OLD_SESSIONS);
        // The newest older session still closes its connection as stale.
        let out = add(&mut state, 100, false, count - 2);
        assert!(
            out.contains(&Action::Close(100, CloseCode::Stale)),
            "{out:?}"
        );
    }

    /// The connection that the higher endpoint id dialed loses a crossed dial, seen from both ends.
    #[test]
    fn crossed_dials_retire_the_loser_of_the_higher_dialer() {
        // We are the lower id: our dialed connection 1 wins, the accepted 2 loses.
        let mut lower = state();
        let mut out = Vec::new();
        lower.add_conn(1, Side::Client, &mut out);
        lower.add_conn(2, Side::Server, &mut out);
        assert_eq!(out, vec![Action::TakeQueue(1)]);
        out.clear();
        lower.on_header(1, Session([1; 8]), &mut out);
        assert!(out.is_empty(), "wait for all headers: {out:?}");
        lower.on_header(2, Session([1; 8]), &mut out);
        assert_eq!(out, vec![Action::Retire(2)]);
        // The winner stays, and keeps the writer.
        out.clear();
        lower.on_closed(2, ConnEnd::Retired, None, &mut out);
        assert!(out.is_empty(), "{out:?}");
        assert_eq!((lower.conn_count(), lower.writer()), (1, Some(1)));

        // We are the higher id: the connection that we dialed loses, and its writer moves.
        let mut higher = PeerState::new(MIN_QUEUE_SIZE, false);
        higher.queue.open();
        let mut out = Vec::new();
        higher.add_conn(1, Side::Client, &mut out);
        let reader = higher.reader_for(1).expect("the writer gets the reader");
        higher.on_header(1, Session([1; 8]), &mut out);
        assert!(out.is_empty() || out == vec![Action::TakeQueue(1)]);
        out.clear();
        higher.add_conn(2, Side::Server, &mut out);
        higher.on_header(2, Session([1; 8]), &mut out);
        assert_eq!(out, vec![Action::Retire(1)]);
        // The loser keeps the reader until it closes, so the peer reads a topic in order.
        higher.send(join_message([2; 32].into(), 0));
        out.clear();
        higher.on_header(2, Session([1; 8]), &mut out);
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(higher.writer(), Some(1));
        out.clear();
        higher.on_closed(1, ConnEnd::Retired, Some(reader), &mut out);
        assert_eq!(out, vec![Action::TakeQueue(2)]);
        assert_eq!((higher.conn_count(), higher.writer()), (1, Some(2)));
        let reader = higher.reader_for(2).expect("the reader came back");
        assert!(reader.reads(&higher.queue) && !reader.is_empty());
    }

    /// No retiring connection becomes the writer: neither a crossed-dial loser nor an idle one.
    #[test]
    fn settle_skips_a_retiring_connection() {
        // We are the lower id, so the accepted connection 2 loses.
        let mut lower = state();
        let mut out = Vec::new();
        lower.add_conn(1, Side::Client, &mut out);
        lower.add_conn(2, Side::Server, &mut out);
        lower.on_header(1, Session([1; 8]), &mut out);
        lower.on_header(2, Session([1; 8]), &mut out);
        assert_eq!(out, vec![Action::TakeQueue(1), Action::Retire(2)]);
        out.clear();
        lower.on_closed(1, ConnEnd::Failed, None, &mut out);
        assert_eq!(out, vec![Action::Disconnect]);
        assert_eq!(lower.writer(), None);

        // Connection 2 retires on its own.
        let mut at_me = state();
        add(&mut at_me, 1, false, 1);
        add(&mut at_me, 2, false, 1);
        let mut out = Vec::new();
        at_me.on_retiring(2, &mut out);
        assert!(out.is_empty(), "{out:?}");
        at_me.on_closed(1, ConnEnd::Failed, None, &mut out);
        assert_eq!(out, vec![Action::Disconnect]);
        assert_eq!(at_me.writer(), None);
        // A message dials, and the new connection writes.
        at_me.send(join_message([2; 32].into(), 0));
        assert!(at_me.needs_dial());
        out.clear();
        at_me.add_conn(3, Side::Client, &mut out);
        assert_eq!(out, vec![Action::TakeQueue(3)]);
    }

    /// A message while the only connection retires dials at once.
    ///
    /// The new connection waits for the reader.
    #[test]
    fn message_while_the_only_connection_retires_dials() {
        let mut at_me = state();
        add(&mut at_me, 1, true, 1);
        let reader = at_me.reader_for(1).expect("the writer gets the reader");
        let mut out = Vec::new();
        at_me.on_retiring(1, &mut out);
        assert!(out.is_empty(), "an empty queue needs no dial: {out:?}");
        at_me.send(join_message([2; 32].into(), 0));
        assert!(at_me.needs_dial());
        // The retiring writer keeps the reader until its connection closes.
        at_me.add_conn(2, Side::Client, &mut out);
        assert!(out.is_empty(), "{out:?}");
        assert!(!at_me.needs_dial());
        at_me.on_closed(1, ConnEnd::Retired, Some(reader), &mut out);
        assert_eq!(out, vec![Action::TakeQueue(2)]);

        // A retire that starts after the message dials too.
        let mut at_me = state();
        add(&mut at_me, 1, true, 1);
        at_me.send(join_message([2; 32].into(), 0));
        let mut out = Vec::new();
        at_me.on_retiring(1, &mut out);
        assert_eq!(out, vec![Action::Dial]);
    }

    /// A connection that waits for the reader of a retiring writer, and retires as idle, does not dial.
    ///
    /// Otherwise each idle retire dials again, until the writer closes.
    #[test]
    fn idle_retire_of_a_waiting_connection_does_not_dial() {
        let mut at_me = state();
        add(&mut at_me, 1, true, 1);
        let reader = at_me.reader_for(1).expect("the writer gets the reader");
        let mut out = Vec::new();
        at_me.on_retiring(1, &mut out);
        at_me.send(join_message([2; 32].into(), 0));
        assert!(at_me.needs_dial());
        // The dial gives connection 2, which waits, and retires as idle.
        at_me.add_conn(2, Side::Client, &mut out);
        at_me.on_retiring(2, &mut out);
        assert!(out.is_empty(), "{out:?}");
        // The close of the writer dials once, and the next connection writes.
        at_me.on_closed(1, ConnEnd::Retired, Some(reader), &mut out);
        assert_eq!(out, vec![Action::Dial]);
        assert_eq!(at_me.writer(), None);
        out.clear();
        at_me.add_conn(3, Side::Client, &mut out);
        assert_eq!(out, vec![Action::TakeQueue(3)]);
    }

    /// A waiting connection that retires and closes before the retiring writer does not dial.
    #[test]
    fn close_of_a_waiting_connection_does_not_dial() {
        let mut at_me = state();
        add(&mut at_me, 1, true, 1);
        let reader = at_me.reader_for(1).expect("the writer gets the reader");
        let mut out = Vec::new();
        at_me.on_retiring(1, &mut out);
        at_me.send(join_message([2; 32].into(), 0));
        at_me.add_conn(2, Side::Client, &mut out);
        at_me.on_retiring(2, &mut out);
        // The peer answers the retire of connection 2 at once.
        at_me.on_closed(2, ConnEnd::Retired, None, &mut out);
        assert!(out.is_empty(), "{out:?}");
        at_me.on_closed(1, ConnEnd::Retired, Some(reader), &mut out);
        assert_eq!(out, vec![Action::Dial]);
    }

    /// Steady messages during a stalled retire dial once until the writer closes.
    ///
    /// Each connection that the dial gives waits for the reader, retires as idle, and closes.
    #[test]
    fn steady_messages_during_a_stalled_retire_dial_once() {
        let mut at_me = state();
        add(&mut at_me, 1, true, 1);
        let reader = at_me.reader_for(1).expect("the writer gets the reader");
        let mut out = Vec::new();
        at_me.on_retiring(1, &mut out);
        let mut dials = 0;
        for id in 2..8 {
            at_me.send(join_message([2; 32].into(), 0));
            if at_me.needs_dial() {
                at_me.on_dial();
                dials += 1;
            }
            at_me.add_conn(id, Side::Client, &mut out);
            at_me.on_retiring(id, &mut out);
            at_me.on_closed(id, ConnEnd::Retired, None, &mut out);
        }
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(dials, 1);
        // The close of the writer dials again, for the next writer.
        at_me.on_closed(1, ConnEnd::Retired, Some(reader), &mut out);
        assert_eq!(out, vec![Action::Dial]);
    }

    /// A dial during a retire is not a crossed dial on the side that knows of the retire.
    #[test]
    fn dial_during_a_retire_is_kept() {
        // We are the higher id, so our dialed connection 2 would lose to the accepted 1.
        let mut higher = PeerState::new(MIN_QUEUE_SIZE, false);
        higher.queue.open();
        let mut out = Vec::new();
        higher.add_conn(1, Side::Server, &mut out);
        higher.on_header(1, Session([1; 8]), &mut out);
        higher.on_retiring(1, &mut out);
        out.clear();
        higher.add_conn(2, Side::Client, &mut out);
        higher.on_header(2, Session([1; 8]), &mut out);
        assert!(out.is_empty(), "{out:?}");
    }

    /// The state never retires a lone connection, whoever dialed it.
    #[test]
    fn a_lone_connection_is_kept() {
        let mut at_me = state();
        let mut out = Vec::new();
        at_me.add_conn(1, Side::Server, &mut out);
        at_me.on_header(1, Session([1; 8]), &mut out);
        assert_eq!(out, vec![Action::TakeQueue(1)]);
    }

    /// After a restart the new connection is not the loser of a crossed dial with the old one.
    #[test]
    fn restart_does_not_retire_the_new_connection() {
        let mut at_me = state();
        let mut out = Vec::new();
        at_me.add_conn(1, Side::Client, &mut out);
        at_me.on_header(1, Session([1; 8]), &mut out);
        out.clear();
        let out = add(&mut at_me, 2, false, 2);
        assert!(out.contains(&Action::Close(1, CloseCode::Stale)));
        assert!(
            !out.iter().any(|a| matches!(a, Action::Retire(_))),
            "{out:?}"
        );
    }

    /// A session that differs from the one of a dialed connection is a restart, not a crossed dial.
    #[test]
    fn restarted_peer_replaces_a_dead_dialed_connection() {
        let mut at_a = state();
        let mut out = Vec::new();
        at_a.add_conn(1, Side::Client, &mut out);
        at_a.on_header(1, Session([1; 8]), &mut out);
        let out = add(&mut at_a, 2, false, 2);
        assert!(out.contains(&Action::Close(1, CloseCode::Stale)));
        assert!(out.contains(&Action::Disconnect));
        assert_eq!(at_a.writer(), Some(2));
    }

    /// A new session replaces every connection of the old one, and the protocol sees a disconnect.
    #[test]
    fn restart_replaces_and_disconnects() {
        let mut at_me = state();
        add(&mut at_me, 1, false, 1);
        add(&mut at_me, 2, false, 1);
        at_me.send(join_message([1; 32].into(), 0));
        let out = add(&mut at_me, 3, false, 2);
        assert!(out.contains(&Action::Close(1, CloseCode::Stale)));
        assert!(out.contains(&Action::Close(2, CloseCode::Stale)));
        assert!(out.contains(&Action::Disconnect));
        assert_eq!(at_me.conn_count(), 1);
        assert_eq!(at_me.writer(), Some(3));
        assert!(
            at_me.queue.is_empty(),
            "the queue of the old process stayed"
        );
        assert!(out.ends_with(&[Action::TakeQueue(3)]), "{out:?}");
    }

    /// A connection with an older session belongs to a former instance of the peer.
    ///
    /// Only that connection closes.
    #[test]
    fn former_instance_is_refused() {
        let mut at_me = state();
        add(&mut at_me, 1, false, 1);
        add(&mut at_me, 2, false, 2);
        let mut out = Vec::new();
        at_me.add_conn(3, Side::Server, &mut out);
        at_me.on_header(3, Session([1; 8]), &mut out);
        assert_eq!(out, vec![Action::Close(3, CloseCode::Stale)]);
        assert_eq!((at_me.conn_count(), at_me.writer()), (1, Some(2)));
    }

    /// After a restart, a dialed connection writes only once its header shows the new session.
    #[test]
    fn restart_makes_a_dialed_connection_wait_for_its_header() {
        let mut at_me = state();
        add(&mut at_me, 1, false, 1);
        add(&mut at_me, 2, true, 1);
        // Connection 2 may lead to the former instance.
        let out = add(&mut at_me, 3, false, 2);
        assert!(out.ends_with(&[Action::TakeQueue(3)]), "{out:?}");
        let mut out = Vec::new();
        at_me.on_header(2, Session([1; 8]), &mut out);
        assert_eq!(out, vec![Action::Close(2, CloseCode::Stale)]);
        // A later dial waits too.
        at_me.on_closed(3, ConnEnd::Failed, None, &mut out);
        out.clear();
        at_me.add_conn(4, Side::Client, &mut out);
        assert!(out.is_empty(), "{out:?}");
        at_me.on_header(4, Session([2; 8]), &mut out);
        assert_eq!(out, vec![Action::TakeQueue(4)]);
    }

    /// An accepted last connection of a former instance dials for the queued messages.
    #[test]
    fn accepted_former_instance_on_the_last_connection_dials() {
        let mut at_me = state();
        add(&mut at_me, 1, false, 1);
        add(&mut at_me, 2, false, 2);
        let mut out = Vec::new();
        at_me.on_closed(2, ConnEnd::Failed, None, &mut out);
        at_me.send(join_message([2; 32].into(), 0));
        let mut out = Vec::new();
        at_me.add_conn(3, Side::Server, &mut out);
        at_me.on_header(3, Session([1; 8]), &mut out);
        assert_eq!(out, vec![Action::Close(3, CloseCode::Stale), Action::Dial]);
        // The dial reaches the current instance, which gets the queue.
        out.clear();
        at_me.add_conn(4, Side::Client, &mut out);
        at_me.on_closed(3, ConnEnd::Quiet, None, &mut out);
        at_me.on_header(4, Session([2; 8]), &mut out);
        assert_eq!(out, vec![Action::TakeQueue(4)]);
        assert!(!at_me.queue.is_empty());
    }

    /// A dial that reaches a former instance on the last connection disconnects, and dials no more.
    #[test]
    fn dialed_former_instance_on_the_last_connection_disconnects() {
        let mut at_me = state();
        add(&mut at_me, 1, false, 1);
        add(&mut at_me, 2, false, 2);
        let mut out = Vec::new();
        at_me.on_closed(2, ConnEnd::Failed, None, &mut out);
        at_me.send(join_message([2; 32].into(), 0));
        let mut out = Vec::new();
        at_me.add_conn(3, Side::Client, &mut out);
        at_me.on_header(3, Session([1; 8]), &mut out);
        assert_eq!(
            out,
            vec![Action::Close(3, CloseCode::Stale), Action::Disconnect]
        );
        out.clear();
        at_me.on_closed(3, ConnEnd::Quiet, None, &mut out);
        assert!(out.is_empty(), "{out:?}");
        assert!(at_me.is_idle());
    }

    /// The state drops a reader of a discarded queue that comes back, and the current writer stays.
    #[test]
    fn reader_of_a_discarded_queue_is_dropped() {
        let mut at_me = state();
        add(&mut at_me, 1, false, 1);
        let old = at_me.reader_for(1).expect("the writer gets the reader");
        // A restart of the peer discards the queue, and the new connection writes.
        add(&mut at_me, 2, false, 2);
        let current = at_me
            .reader_for(2)
            .expect("the new writer gets a new reader");
        assert!(!old.reads(&at_me.queue) && current.reads(&at_me.queue));
        let mut out = Vec::new();
        at_me.on_closed(1, ConnEnd::Quiet, Some(old), &mut out);
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(at_me.writer(), Some(2));
        // The current one comes back.
        at_me.on_closed(2, ConnEnd::Retired, Some(current), &mut out);
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(at_me.writer(), None);
        assert!(at_me
            .take_reader()
            .is_some_and(|reader| reader.reads(&at_me.queue)));
    }

    /// A writer that fails returns a reader of the queue that its failure discards.
    #[test]
    fn failed_writer_returns_a_discarded_reader() {
        let mut at_me = state();
        add(&mut at_me, 1, false, 1);
        add(&mut at_me, 2, false, 1);
        let reader = at_me.reader_for(1).expect("the writer gets the reader");
        let mut out = Vec::new();
        at_me.on_closed(1, ConnEnd::Failed, Some(reader), &mut out);
        assert_eq!(out, vec![Action::Disconnect, Action::TakeQueue(2)]);
        assert!(at_me
            .reader_for(2)
            .is_some_and(|reader| reader.reads(&at_me.queue)));
    }

    /// Events of a connection that the state dropped change nothing.
    #[test]
    fn events_of_a_dropped_connection_are_ignored() {
        let mut at_me = state();
        add(&mut at_me, 1, false, 1);
        add(&mut at_me, 2, false, 2);
        let mut out = Vec::new();
        at_me.on_header(1, Session([1; 8]), &mut out);
        at_me.on_closed(1, ConnEnd::Failed, None, &mut out);
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(at_me.conn_count(), 1);
    }

    /// The end of the last connection while a topic uses the peer is a disconnect.
    #[test]
    fn last_connection_with_topic_disconnects() {
        // Open queue: a topic uses the peer.
        let mut at_me = state();
        add(&mut at_me, 1, true, 1);
        let mut out = Vec::new();
        at_me.on_closed(1, ConnEnd::Failed, None, &mut out);
        assert_eq!(out, vec![Action::Disconnect]);
        // A non-writer that fails changes nothing while another connection lives.
        let mut at_me = state();
        add(&mut at_me, 1, false, 1);
        add(&mut at_me, 2, false, 1);
        let mut out = Vec::new();
        at_me.on_closed(2, ConnEnd::Failed, None, &mut out);
        assert!(out.is_empty(), "{out:?}");
        // A closed queue: nobody uses the peer.
        let mut at_me = state();
        at_me.queue.close();
        let mut out = Vec::new();
        at_me.add_conn(1, Side::Server, &mut out);
        at_me.on_closed(1, ConnEnd::Failed, None, &mut out);
        assert!(out.is_empty(), "{out:?}");
    }

    /// The writer that fails drops the queue, and the writer goes to another ready connection.
    #[test]
    fn failed_writer_hands_over_to_a_ready_connection() {
        let mut at_me = state();
        add(&mut at_me, 1, false, 1);
        add(&mut at_me, 2, false, 1);
        assert_eq!(at_me.writer(), Some(1));
        let mut out = Vec::new();
        at_me.on_closed(1, ConnEnd::Failed, None, &mut out);
        assert_eq!(out, vec![Action::Disconnect, Action::TakeQueue(2)]);
    }

    /// A finished retire of the last connection dials while the queue is open, also when it is empty.
    ///
    /// An open queue means that a topic wants the peer, so the end is no disconnect.
    #[test]
    fn retired_with_an_open_queue_redials() {
        let retire_last = |queued: bool, closed: bool| {
            let mut at_me = state();
            add(&mut at_me, 1, true, 1);
            if queued {
                at_me.send(join_message([2; 32].into(), 0));
            }
            if closed {
                at_me.queue.close();
            }
            let mut out = Vec::new();
            at_me.on_closed(1, ConnEnd::Retired, None, &mut out);
            out
        };
        assert_eq!(retire_last(true, false), vec![Action::Dial]);
        assert_eq!(retire_last(false, false), vec![Action::Dial]);
        assert!(retire_last(false, true).is_empty());
    }

    /// After an abort, a message dials, and the end of the old connection keeps it queued.
    #[test]
    fn abort_keeps_the_next_queue() {
        let mut at_me = state();
        add(&mut at_me, 1, true, 1);
        at_me.abort();
        at_me.send(join_message([2; 32].into(), 0));
        assert!(at_me.needs_dial());
        let mut out = Vec::new();
        at_me.on_closed(1, ConnEnd::Failed, None, &mut out);
        assert!(out.is_empty(), "{out:?}");
        assert!(!at_me.queue.is_empty());
        at_me.add_conn(2, Side::Client, &mut out);
        assert_eq!(out, vec![Action::TakeQueue(2)]);
    }

    /// A connection that the peer closed as stale ends without a report, also with the queue open.
    #[test]
    fn quiet_end_reports_nothing() {
        let mut at_me = state();
        add(&mut at_me, 1, true, 1);
        let mut out = Vec::new();
        at_me.on_closed(1, ConnEnd::Quiet, None, &mut out);
        assert!(out.is_empty(), "{out:?}");
    }

    /// A failed dial without a connection disconnects, and one with a connection does not.
    #[test]
    fn dial_failure_needs_no_connection_to_matter() {
        let mut at_me = state();
        let mut out = Vec::new();
        at_me.on_dial_failed(&mut out);
        assert_eq!(out, vec![Action::Disconnect]);
        let mut at_me = state();
        add(&mut at_me, 1, false, 1);
        let mut out = Vec::new();
        at_me.on_dial_failed(&mut out);
        assert!(out.is_empty());
    }
}
