//! The wire format of gossip streams.
//!
//! Each topic a peer sends to gets one unidirectional stream. It starts with a
//! [`StreamHeader`] naming the topic, followed by the protocol messages. Every
//! item is postcard-encoded and prefixed with its length as a big-endian `u32`.

use std::future::Future;

use bytes::{BufMut, Bytes, BytesMut};
use iroh::endpoint::{Connection, RecvStream, SendStream};
use n0_error::{ensure_any, Result, StackResultExt, StdResultExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::proto::TopicId;

/// The first item on a gossip stream, naming the topic it is for.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[non_exhaustive]
pub struct StreamHeader {
    /// The topic the stream is for.
    pub topic_id: TopicId,
}

/// The sending end of a gossip stream for one topic.
#[derive(Debug)]
pub(crate) struct TopicSender {
    send: PostcardCodec<SendStream>,
}

impl TopicSender {
    /// Opens a stream for `topic_id` on `conn` and writes its header.
    pub(crate) async fn open(
        conn: &Connection,
        topic_id: TopicId,
        max_message_size: usize,
    ) -> Result<Self> {
        let header = StreamHeader { topic_id };
        let send = conn.open_uni().await.anyerr()?;
        let mut send = PostcardCodec::new(send, max_message_size);
        send.send(&header).await.anyerr()?;
        Ok(Self { send })
    }

    /// Writes a protocol message to the stream.
    pub(crate) async fn send(&mut self, msg: &super::ProtoMessage) -> Result<()> {
        self.send.send(&msg).await
    }

    /// Resolves when the peer stops reading this stream.
    ///
    /// `SendStream::stopped` already returns an owned future, so this does not
    /// need a task to detach it from the borrow.
    pub(crate) fn closed(&self) -> impl Future<Output = ()> + Send + Sync + 'static + use<> {
        let stopped = self.send.inner.stopped();
        async move {
            stopped.await.ok();
        }
    }
}

/// The receiving end of a gossip stream for one topic.
#[derive(Debug)]
pub(crate) struct TopicReceiver {
    recv: PostcardCodec<RecvStream>,
    header: StreamHeader,
}

impl TopicReceiver {
    /// Returns the topic the stream is for, as named by its header.
    pub(crate) fn topic_id(&self) -> TopicId {
        self.header.topic_id
    }

    /// Accepts the next stream the peer opens on `conn` and reads its header.
    ///
    /// Returns `None` once the connection is closed.
    pub(crate) async fn accept(conn: &Connection, max_message_size: usize) -> Result<Option<Self>> {
        let stream = match conn.accept_uni().await {
            Ok(stream) => stream,
            Err(_) => return Ok(None),
        };
        let mut recv = PostcardCodec::new(stream, max_message_size);
        let header: StreamHeader = recv.recv().await?.context("Unexpected EOF")?;
        Ok(Some(Self { recv, header }))
    }

    /// Reads the next protocol message, or `None` once the stream is finished.
    pub(crate) async fn recv(&mut self) -> Result<Option<super::ProtoMessage>> {
        self.recv.recv().await
    }
}

/// Length-prefixed postcard framing over a QUIC stream.
#[derive(Debug)]
struct PostcardCodec<I> {
    max_message_size: usize,
    inner: I,
    buf: BytesMut,
}

impl<I> PostcardCodec<I> {
    fn new(inner: I, max_message_size: usize) -> Self {
        Self {
            inner,
            max_message_size,
            buf: BytesMut::new(),
        }
    }
}

impl PostcardCodec<SendStream> {
    /// Encodes `msg` and writes it with its length prefix.
    async fn send<T: Serialize>(&mut self, msg: &T) -> Result<()> {
        self.buf.clear();
        postcard::to_io(msg, (&mut self.buf).writer()).anyerr()?;
        ensure_any!(
            self.buf.len() <= self.max_message_size,
            "Message exceeds max message length"
        );
        self.inner.write_u32(self.buf.len() as u32).await.anyerr()?;
        let bytes = Bytes::copy_from_slice(&self.buf);
        self.inner.write_chunk(bytes).await.anyerr()?;
        Ok(())
    }
}

impl PostcardCodec<RecvStream> {
    /// Reads and decodes the next item, or returns `None` if the stream finished.
    async fn recv<T: DeserializeOwned>(&mut self) -> Result<Option<T>> {
        let len = match self.inner.read_u32().await {
            Ok(len) => len as usize,
            // A cleanly finished stream reads as EOF. This used to match on
            // `NotConnected`, which is what quinn maps a lost connection to, so
            // every normal stream close was reported as an error and a lost
            // connection as a clean end.
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        ensure_any!(
            len <= self.max_message_size,
            "Received message exceeds max message length"
        );
        self.buf.clear();
        self.buf.resize(len, 0);
        self.inner.read_exact(&mut self.buf[..len]).await.anyerr()?;
        let item = postcard::from_bytes(&self.buf[..len]).anyerr()?;
        Ok(Some(item))
    }
}

#[cfg(test)]
mod tests {
    use iroh::{endpoint::presets, Endpoint};
    use n0_error::{Result, StdResultExt};
    use n0_tracing_test::traced_test;

    use super::*;

    const TEST_ALPN: &[u8] = b"iroh-gossip/net-proto-test/0";

    /// Returns the two ends of one connection, plus the endpoints to keep alive.
    ///
    /// Dropping an endpoint closes its connections, so they have to outlive the
    /// test body.
    async fn connected_pair() -> Result<(Connection, Connection, (Endpoint, Endpoint))> {
        let server = Endpoint::builder(presets::Minimal)
            .alpns(vec![TEST_ALPN.to_vec()])
            .bind()
            .await?;
        let addr = server.addr();
        let client = Endpoint::bind(presets::Minimal).await?;

        let accept = n0_future::task::spawn({
            let server = server.clone();
            async move {
                let incoming = server.accept().await.expect("endpoint closed");
                incoming.await.expect("accept failed")
            }
        });
        let out = client
            .connect(addr, TEST_ALPN)
            .await
            .std_context("connect")?;
        let inc = accept.await.std_context("accept task")?;
        Ok((out, inc, (client, server)))
    }

    /// A cleanly finished stream must read as end of stream, not as an error.
    ///
    /// The two are handled differently one layer up: a clean end is an expected
    /// peer going away, an error is worth a warning. Matching on the wrong
    /// `ErrorKind` swapped them, so ordinary shutdowns were logged as failures.
    #[tokio::test]
    #[traced_test]
    async fn clean_stream_end_reads_as_end_of_stream() -> Result {
        let (out, inc, _endpoints) = connected_pair().await?;
        let topic_id = TopicId::from([7u8; 32]);

        let mut tx = TopicSender::open(&out, topic_id, 1024).await?;
        let mut rx = TopicReceiver::accept(&inc, 1024)
            .await?
            .expect("stream was opened");
        assert_eq!(rx.topic_id(), topic_id);

        tx.send.inner.finish().std_context("finish")?;
        assert!(
            rx.recv().await?.is_none(),
            "a cleanly finished stream was reported as an error"
        );
        Ok(())
    }
}
