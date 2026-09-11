use std::future::Future;

use bytes::{BufMut, Bytes, BytesMut};
use iroh::endpoint::{Connection, RecvStream, SendStream};
use n0_error::{ensure_any, Result, StackResultExt, StdResultExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::proto::TopicId;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[non_exhaustive]
pub struct StreamHeader {
    pub topic_id: TopicId,
}

#[derive(Debug)]
pub(crate) struct GossipSender {
    send: PostcardCodec<SendStream>,
}

impl GossipSender {
    pub(crate) async fn init(
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

    pub(crate) async fn send(&mut self, msg: &super::ProtoMessage) -> Result<()> {
        self.send.send(&msg).await
    }

    pub(crate) fn closed(&self) -> impl Future<Output = ()> + Send + Sync + 'static + use<> {
        let stopped = self.send.inner.stopped();
        let t = tokio::spawn(stopped);
        async move {
            t.await.expect("panicked").ok();
        }
    }
}

#[derive(Debug)]
pub(crate) struct GossipReceiver {
    recv: PostcardCodec<RecvStream>,
    header: StreamHeader,
    conn_id: usize,
}

impl GossipReceiver {
    pub(crate) fn topic_id(&self) -> TopicId {
        self.header.topic_id
    }

    pub(crate) fn is_same_conn(&self, conn: &Connection) -> bool {
        self.conn_id == conn.stable_id()
    }

    pub(crate) async fn accept(conn: &Connection, max_message_size: usize) -> Result<Option<Self>> {
        let stream = match conn.accept_uni().await {
            Ok(stream) => stream,
            Err(_) => return Ok(None),
        };
        let conn_id = conn.stable_id();
        let mut recv = PostcardCodec::new(stream, max_message_size);
        let header: StreamHeader = recv.recv().await?.context("Unexpected EOF")?;
        Ok(Some(Self {
            recv,
            header,
            conn_id,
        }))
    }

    pub(crate) async fn recv(&mut self) -> Result<Option<super::ProtoMessage>> {
        self.recv.recv().await
    }
}

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
    async fn recv<T: DeserializeOwned>(&mut self) -> Result<Option<T>> {
        let len = match self.inner.read_u32().await {
            Ok(len) => len as usize,
            Err(err) if err.kind() == std::io::ErrorKind::NotConnected => return Ok(None),
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
