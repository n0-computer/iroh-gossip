//! Per-topic auth: the verifier of the tokens that we receive.

use std::{fmt, sync::Arc};

use iroh::EndpointId;

use super::Verdict;
use crate::{api::AuthToken, proto::TopicId};

type VerifyFn = dyn Fn(AuthContext<'_>) -> Verdict + Send + Sync;

/// A stream that a peer opened for a topic, as a [`Builder::topic_auth`] verifier sees it.
///
/// The debug output hides the token, see [`AuthToken`].
///
/// [`Builder::topic_auth`]: super::Builder::topic_auth
#[derive(Debug)]
pub struct AuthContext<'a> {
    topic: TopicId,
    peer: EndpointId,
    token: Option<&'a AuthToken>,
}

impl<'a> AuthContext<'a> {
    /// Returns the topic of the stream.
    pub fn topic(&self) -> TopicId {
        self.topic
    }

    /// Returns the authenticated id of the peer that opened the stream.
    pub fn peer(&self) -> EndpointId {
        self.peer
    }

    /// Returns the token that the peer sent, or `None` if it sent none.
    pub fn token(&self) -> Option<&'a [u8]> {
        self.token.map(AuthToken::as_bytes)
    }
}

/// The verifier of the tokens of peers.
///
/// Without one, the node accepts every stream.
#[derive(Clone, Default)]
pub(super) struct TopicAuth(Option<Arc<VerifyFn>>);

impl TopicAuth {
    pub(super) fn new(verify: impl Fn(AuthContext<'_>) -> Verdict + Send + Sync + 'static) -> Self {
        Self(Some(Arc::new(verify)))
    }

    /// Returns whether to accept a stream that `peer` opened for `topic` with `token`.
    pub(super) fn verify(
        &self,
        topic: TopicId,
        peer: EndpointId,
        token: Option<&AuthToken>,
    ) -> Verdict {
        match &self.0 {
            Some(verify) => verify(AuthContext { topic, peer, token }),
            None => Verdict::Accept,
        }
    }
}

impl fmt::Debug for TopicAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TopicAuth")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer() -> EndpointId {
        iroh::SecretKey::from_bytes(&[1u8; 32]).public()
    }

    #[test]
    fn no_verifier_accepts_everything() {
        let auth = TopicAuth::default();
        let topic = TopicId::from_bytes([0; 32]);
        assert_eq!(auth.verify(topic, peer(), None), Verdict::Accept);
        let token = AuthToken::new(&b"x"[..]);
        assert_eq!(auth.verify(topic, peer(), Some(&token)), Verdict::Accept);
    }

    #[test]
    fn verifier_sees_topic_peer_and_token() {
        let (open, closed) = (TopicId::from_bytes([1; 32]), TopicId::from_bytes([2; 32]));
        let me = peer();
        // The token is the topic's first byte then the peer's first byte, and only for `open`.
        let auth = TopicAuth::new(move |auth| {
            let expected = [auth.topic().as_bytes()[0], auth.peer().as_bytes()[0]];
            match auth.token() {
                Some(token) if token == expected && auth.topic() == open => Verdict::Accept,
                _ => Verdict::Reject,
            }
        });
        let good = AuthToken::new(vec![1, me.as_bytes()[0]]);
        assert_eq!(auth.verify(open, me, Some(&good)), Verdict::Accept);
        assert_eq!(auth.verify(open, me, None), Verdict::Reject);
        let bad = AuthToken::new(&b"bad"[..]);
        assert_eq!(auth.verify(open, me, Some(&bad)), Verdict::Reject);
        let other = AuthToken::new(vec![2, me.as_bytes()[0]]);
        assert_eq!(auth.verify(closed, me, Some(&other)), Verdict::Reject);
    }

    #[test]
    fn debug_hides_the_token() {
        let token = AuthToken::new(&b"hunter2"[..]);
        let auth = AuthContext {
            topic: TopicId::from_bytes([0; 32]),
            peer: peer(),
            token: Some(&token),
        };
        let debug = format!("{auth:?}");
        assert!(
            debug.contains("token") && !debug.contains("hunter2"),
            "{debug}"
        );
    }
}
