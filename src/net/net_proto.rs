use irpc::{channel::mpsc, rpc_requests};
use serde::{Deserialize, Serialize};

use crate::proto::TopicId;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[non_exhaustive]
pub struct JoinRequest {
    pub topic_id: TopicId,
}

#[rpc_requests(message = GossipMessage)]
#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    #[rpc(tx=mpsc::Sender<super::ProtoMessage>, rx=mpsc::Receiver<super::ProtoMessage>)]
    Join(JoinRequest),
}
