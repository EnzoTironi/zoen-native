//! A signed first-append proposal and the original canonical genesis, not a ledger.
use crate::{Envelope, Sequenced};

pub const CAPABILITY: &str = "saved_messages_owner_genesis_v1";

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct Genesis {
    #[prost(bytes = "vec", tag = "1")]
    pub content: Vec<u8>,
    #[prost(string, tag = "2")]
    pub signature: String,
    #[prost(string, tag = "3")]
    pub certificate: String,
}
impl Genesis {
    pub fn from_envelope(env: &Envelope) -> Self {
        Self {
            content: env.content().to_vec(),
            signature: env.sig.clone(),
            certificate: env.cert.clone().unwrap_or_default(),
        }
    }
    pub fn envelope(&self) -> Option<Envelope> {
        if self.content.len() > 4096
            || self.signature.len() > 256
            || self.certificate.is_empty()
            || self.certificate.len() > 4096
        {
            return None;
        }
        let env = Envelope::new(
            self.content.clone(),
            self.signature.clone(),
            Some(self.certificate.clone()),
            None,
        )?;
        let body = env.body()?;
        (types::saved_messages::genesis(env.space(), env.author(), &body) && env.seen().is_none())
            .then_some(env)
    }
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct Receipt {
    #[prost(message, optional, tag = "1")]
    pub genesis: Option<Genesis>,
    #[prost(uint64, tag = "2")]
    pub seq: u64,
    #[prost(string, tag = "3")]
    pub prev: String,
    #[prost(string, tag = "4")]
    pub hash: String,
}
impl Receipt {
    pub fn from_sequenced(event: &Sequenced) -> Self {
        Self {
            genesis: Some(Genesis::from_envelope(&event.env)),
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
        }
    }
    pub fn sequenced(&self) -> Option<Sequenced> {
        (self.seq == 0).then_some(())?;
        Some(Sequenced {
            seq: self.seq,
            prev: self.prev.clone(),
            hash: self.hash.clone(),
            env: self.genesis.as_ref()?.envelope()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClientFrame, Op, Reply, ServerFrame};
    use event_log::{Author, Signer, SpaceLog};
    use types::{EventBody, Privacy, SpaceKind};

    #[test]
    fn saved_messages_preserves_original_genesis_and_receipt_bytes() {
        let root = Signer::from_secret(&[71; 32]);
        let author = Author::device(&root, Signer::from_secret(&[72; 32]));
        let space = types::saved_messages::space_id(&root.id()).unwrap();
        let event = author
            .sign_event(
                &space,
                "original",
                1,
                None,
                EventBody::SpaceCreated {
                    title: String::new(),
                    kind: SpaceKind::Direct,
                    privacy: Privacy::EndToEnd,
                },
            )
            .unwrap();
        let env = Envelope::plain(&event).unwrap();
        let proposal = Genesis::from_envelope(&env);
        assert_eq!(proposal.envelope().unwrap(), env);
        let req = ClientFrame::Req {
            id: 19,
            op: Op::EnsureSavedMessages(proposal.clone()),
        };
        assert_eq!(ClientFrame::decode(&req.encode()).unwrap(), req);
        let mut log = SpaceLog::new(&space);
        log.sequence(event).unwrap();
        let event = &log.events()[0];
        let canonical = Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: env.clone(),
        };
        let receipt = Receipt::from_sequenced(&canonical);
        assert_eq!(receipt.sequenced(), Some(canonical));
        let response = ServerFrame::Res {
            id: 19,
            result: Ok(Reply::SavedMessages(receipt.clone())),
        };
        assert_eq!(ServerFrame::decode(&response.encode()).unwrap(), response);
        let mut wrong = receipt;
        wrong.seq = 1;
        assert!(wrong.sequenced().is_none());
        let mut ordinary = proposal;
        ordinary.certificate.clear();
        assert!(ordinary.envelope().is_none());
    }

    #[test]
    fn saved_messages_capability_is_optional_and_never_inferred_for_legacy_peers() {
        assert!(!crate::negotiate(&["blobs".into()])
            .iter()
            .any(|c| c == CAPABILITY));
        assert_eq!(crate::negotiate(&[CAPABILITY.into()]), vec![CAPABILITY]);
    }
}
