use bytes::Bytes;
use iroh::endpoint::{Connection, RecvStream, SendStream};
use n0_error::{Result, StackResultExt, StdResultExt};
use n0_future::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use crate::proto::TopicId;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[non_exhaustive]
pub struct JoinRequest {
    pub topic_id: TopicId,
}

#[derive(Debug)]
pub(crate) struct GossipSender {
    send: FramedWrite<SendStream, LengthDelimitedCodec>,
    header: Option<JoinRequest>,
}

impl GossipSender {
    pub(crate) async fn init(conn: &Connection, topic_id: TopicId) -> Result<Self> {
        let header = JoinRequest { topic_id };
        let send = conn.open_uni().await.anyerr()?;
        let mut send = FramedWrite::new(send, LengthDelimitedCodec::new());
        let msg = postcard::to_stdvec(&header).unwrap();
        send.send(Bytes::from(msg)).await.anyerr()?;
        Ok(Self { send, header: None })
    }

    pub(crate) async fn send(&mut self, msg: super::ProtoMessage) -> Result<()> {
        if let Some(msg) = self.header.take() {
            let msg = postcard::to_stdvec(&msg).unwrap();
            self.send.send(Bytes::from(msg)).await.anyerr()?;
        }
        let msg = postcard::to_stdvec(&msg).unwrap();
        self.send.send(Bytes::from(msg)).await.anyerr()?;
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct GossipReceiver {
    recv: FramedRead<RecvStream, LengthDelimitedCodec>,
    header: JoinRequest,
    conn_id: usize,
}

impl GossipReceiver {
    pub(crate) fn topic_id(&self) -> TopicId {
        self.header.topic_id
    }

    pub(crate) fn is_same_conn(&self, conn: &Connection) -> bool {
        self.conn_id == conn.stable_id()
    }

    pub(crate) async fn accept(conn: &Connection) -> Result<Option<Self>> {
        let stream = match conn.accept_uni().await {
            Ok(stream) => stream,
            Err(_) => return Ok(None),
        };
        let conn_id = conn.stable_id();
        let mut recv = FramedRead::new(stream, LengthDelimitedCodec::new());
        let header = recv.next().await.context("Unexpected EOF")??;
        let header: JoinRequest =
            postcard::from_bytes(&header).std_context("Failed to parse header")?;
        Ok(Some(Self {
            header,
            recv,
            conn_id,
        }))
    }

    pub(crate) async fn recv(&mut self) -> Result<Option<super::ProtoMessage>> {
        let Some(msg) = self.recv.next().await.transpose().anyerr()? else {
            return Ok(None);
        };
        let msg: super::ProtoMessage =
            postcard::from_bytes(&msg).std_context("Failed to parse header")?;
        Ok(Some(msg))
    }
}
