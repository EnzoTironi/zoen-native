//! Read the original sealed reply for transport; never create or reseal it.
use super::{DeviceCore, NativeError, ReplyFacts, ReplyIntent, Result, RetainedReply};
use protocol::Envelope;

/// Native facts at the supplied clock. The Runtime still owns source-head,
/// directory, continuity and publication admission immediately before transport.
pub struct PendingReply {
    facts: ReplyFacts,
    retained: RetainedReply,
}

impl std::fmt::Debug for PendingReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingReply").finish_non_exhaustive()
    }
}

impl PendingReply {
    #[must_use]
    pub fn retained(&self) -> &RetainedReply {
        &self.retained
    }
    #[must_use]
    pub fn facts(&self) -> &ReplyFacts {
        &self.facts
    }
    #[must_use]
    pub fn envelope(&self) -> &Envelope {
        self.retained.envelope()
    }
    #[must_use]
    pub fn message(&self) -> &str {
        self.retained.message()
    }
    #[must_use]
    pub fn event_digest(&self) -> &str {
        self.retained.event_digest()
    }
    #[must_use]
    pub fn reference_digest(&self) -> &str {
        self.retained.reference_digest()
    }
    #[must_use]
    pub fn publish_digest(&self) -> &str {
        self.retained.publish_digest()
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
        let retained = self.retained_reply(original, client_id)?;
        if pending.client_id != retained.envelope().client_id() {
            return Err(NativeError::History);
        }
        Ok(PendingReply { facts, retained })
    }
}
