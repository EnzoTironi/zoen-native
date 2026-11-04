//! Read the original sealed reply for transport; never create or reseal it.
use super::{DeviceCore, NativeError, ReplyFacts, ReplyIntent, Result};
use protocol::Envelope;

/// Native facts at the supplied clock. The Runtime still owns source-head,
/// directory, continuity and publication admission immediately before transport.
pub struct PendingReply {
    facts: ReplyFacts,
    envelope: Envelope,
    message: String,
    event_digest: String,
    reference_digest: String,
    publish_digest: String,
}

impl std::fmt::Debug for PendingReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingReply").finish_non_exhaustive()
    }
}

impl PendingReply {
    #[must_use]
    pub fn facts(&self) -> &ReplyFacts {
        &self.facts
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
    /// Read-only retry of one original pending publication under current native
    /// permission. A historical reference alone cannot authorize another send.
    pub fn pending_reply(
        &self,
        original: &ReplyIntent,
        client_id: &str,
        at_ms: i64,
    ) -> Result<PendingReply> {
        let facts = self.refresh(original, at_ms)?;
        let reference = self
            .reply_reference(client_id)?
            .ok_or(NativeError::History)?;
        if reference.original != *original {
            return Err(NativeError::History);
        }
        let heads = self
            .engine
            .store
            .outbox_heads()
            .map_err(|_| NativeError::Storage)?;
        // Preserve the actual queue's per-conversation order, including failed
        // heads. Later effects in other conversations do not block this one.
        if heads
            .iter()
            .find(|head| head.space == original.space())
            .is_none_or(|head| head.client_id != client_id || head.failed)
        {
            return Err(NativeError::NotReady);
        }
        let pending = self
            .engine
            .store
            .outbox_active_event(client_id)
            .map_err(|_| NativeError::Storage)?
            .ok_or(NativeError::NotReady)?;
        let envelope = reference.envelope(self)?;
        if self.pending_envelope(&pending, Some(&reference))? != envelope
            || self.engine.logs.get(original.space()).is_none_or(|log| {
                log.events()
                    .iter()
                    .any(|event| event.client_id == client_id)
            })
        {
            return Err(NativeError::History);
        }
        let (reference_digest, publish_digest) = reference.digests()?;
        Ok(PendingReply {
            facts,
            envelope,
            message: reference.message,
            event_digest: reference.inner_digest,
            reference_digest,
            publish_digest,
        })
    }
}
