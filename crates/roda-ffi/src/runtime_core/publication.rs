use std::fmt;

use roda_proto::{ClientFrame, Envelope};
use roda_types::{new_id, EventBody, Seen};

use super::{
    DeviceCore, NativeError, NativeImage, ReplyFacts, ReplyIntent, Result, MAX_EVENT_BYTES,
};

pub(super) const MAX_REPLY_BYTES: usize = 32768;
pub(super) const MAX_PUBLISH_BYTES: usize = 98_304;

/// One real pending native effect and its complete post-sealing device image.
/// The runtime must atomically activate both before authorizing any transport.
/// These native facts alone grant neither cloud activation nor relay delivery.
pub struct PreparedReply {
    facts: ReplyFacts,
    envelope: Envelope,
    message: String,
    event_digest: String,
    reference_digest: String,
    publish_digest: String,
    image: NativeImage,
}

impl fmt::Debug for PreparedReply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedReply").finish_non_exhaustive()
    }
}

impl PreparedReply {
    pub fn facts(&self) -> &ReplyFacts {
        &self.facts
    }

    pub fn envelope(&self) -> &Envelope {
        &self.envelope
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn event_digest(&self) -> &str {
        &self.event_digest
    }

    pub fn reference_digest(&self) -> &str {
        &self.reference_digest
    }

    pub fn publish_digest(&self) -> &str {
        &self.publish_digest
    }

    pub fn image(&self) -> &NativeImage {
        &self.image
    }
}

impl DeviceCore {
    /// Strict Direct/Trust reply preparation over the original ordered intent.
    /// A preparation error invalidates this scratch workspace; the caller must
    /// discard it and restore the previously committed complete image.
    pub fn prepare_reply(
        &mut self,
        original: &ReplyIntent,
        text: &str,
        at_ms: i64,
    ) -> Result<PreparedReply> {
        let result = (|| {
            if text.trim().is_empty() || text.len() > MAX_REPLY_BYTES {
                return Err(NativeError::Bounds);
            }
            let facts = self.refresh(original, at_ms)?;
            // This profile cannot activate unrelated queued effects or reseal a
            // publication retained by an earlier invocation.
            if self
                .engine
                .store
                .outbox_len()
                .map_err(|_| NativeError::Storage)?
                != 0
            {
                return Err(NativeError::NotReady);
            }
            let author = self
                .engine
                .net
                .author
                .clone()
                .ok_or(NativeError::Credential)?;
            let message = new_id("msg");
            let event = self
                .engine
                .append_synced(
                    original.space(),
                    &author,
                    at_ms,
                    EventBody::MessagePosted {
                        message: message.clone(),
                        text: text.into(),
                        attaches: None,
                        reply: None,
                    },
                )
                .map_err(|_| NativeError::Storage)?;
            self.engine
                .mls_seal_outbox()
                .map_err(|_| NativeError::Storage)?;
            let envelope = self
                .engine
                .outgoing_envelope_with(&event, &mut std::collections::HashMap::new())
                .ok_or(NativeError::NotReady)?;
            if !envelope.is_sealed()
                || envelope.is_pruned()
                || envelope.invite.is_some()
                || envelope.recovery().is_some()
                || envelope.content().len() > MAX_EVENT_BYTES
                || (ClientFrame::Publish {
                    env: envelope.clone(),
                })
                .encode()
                .len()
                    > MAX_PUBLISH_BYTES
                || envelope.client_id() != event.client_id
                || envelope.space() != original.space()
                || envelope.author() != original.agent()
                || envelope.seen()
                    != Some(Seen {
                        seq: facts.frontier().seq(),
                        hash: facts.frontier().hash().into(),
                    })
                || envelope.verify().is_err()
            {
                return Err(NativeError::Bounds);
            }
            let (reference_digest, publish_digest) =
                self.retain_reply_reference(facts.intent(), &event, &envelope)?;
            Ok(PreparedReply {
                facts,
                envelope,
                message,
                event_digest: roda_log::content_hash_of(&event),
                reference_digest,
                publish_digest,
                image: self.image()?,
            })
        })();
        if result.is_err() {
            self.usable = false;
        }
        result
    }
}
