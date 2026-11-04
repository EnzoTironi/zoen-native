//! Inert original publication facts for historical receipt/outcome observation.
use super::{DeviceCore, NativeError, ReplyIntent, Result};
use protocol::Envelope;

pub struct RetainedReply {
    original: ReplyIntent,
    envelope: Envelope,
    message: String,
    event_digest: String,
    reference_digest: String,
    publish_digest: String,
}
impl std::fmt::Debug for RetainedReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetainedReply").finish_non_exhaustive()
    }
}
impl RetainedReply {
    #[must_use]
    pub fn original(&self) -> &ReplyIntent {
        &self.original
    }
    #[must_use]
    pub fn envelope(&self) -> &Envelope {
        &self.envelope
    }
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
    #[must_use]
    pub fn event_digest(&self) -> &str {
        &self.event_digest
    }
    #[must_use]
    pub fn reference_digest(&self) -> &str {
        &self.reference_digest
    }
    #[must_use]
    pub fn publish_digest(&self) -> &str {
        &self.publish_digest
    }
}
impl DeviceCore {
    /// Reads the original native association without current grant or clock.
    /// It cannot authorize transport, clear pending output or infer acceptance.
    pub fn retained_reply(&self, original: &ReplyIntent, client_id: &str) -> Result<RetainedReply> {
        if !self.usable {
            return Err(NativeError::History);
        }
        let reference = self
            .reply_reference(client_id)?
            .ok_or(NativeError::History)?;
        if reference.original != *original {
            return Err(NativeError::History);
        }
        let envelope = reference.envelope(self)?;
        if let Some(pending) = self
            .engine
            .store
            .outbox_get(client_id)
            .map_err(|_| NativeError::Storage)?
        {
            if self.pending_envelope(&pending, Some(&reference))? != envelope
                || self
                    .engine
                    .logs
                    .get(original.space())
                    .is_none_or(|log| log.events().iter().any(|e| e.client_id == client_id))
            {
                return Err(NativeError::History);
            }
        } else {
            self.accepted_reply_event(&reference, &envelope)?;
        }
        let (reference_digest, publish_digest) = reference.digests()?;
        Ok(RetainedReply {
            original: reference.original,
            envelope,
            message: reference.message,
            event_digest: reference.inner_digest,
            reference_digest,
            publish_digest,
        })
    }
}
