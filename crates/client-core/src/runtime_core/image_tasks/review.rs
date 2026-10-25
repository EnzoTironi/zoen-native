//! Admission for this capsule's exact original pending approval publication.
use super::{direct_writers, image_tasks, DeviceCore, EventBody, NativeError, Result};
use protocol::Envelope;

impl DeviceCore {
    /// A host can deliver this original review only while its current native
    /// source, owner trust, device and pending outbox still authorize it.
    /// This grants neither a model dispatch nor owner approval.
    pub fn image_review_ready(
        &self,
        request: &str,
        content_hash: &str,
        envelope: &Envelope,
        at_ms: i64,
    ) -> Result<()> {
        if !self.usable || at_ms < 0 {
            return Err(NativeError::History);
        }
        let record = self
            .engine
            .state
            .requests
            .get(request)
            .ok_or(NativeError::Permission)?;
        let proposal = record
            .req
            .tool_proposal
            .as_ref()
            .ok_or(NativeError::Permission)?;
        image_tasks::parsed(proposal).map_err(|_| NativeError::Permission)?;
        let (agent, device, source_sha) = self
            .engine
            .image_task_actor_at(&record.space, &proposal.source, &proposal.serving, at_ms)
            .map_err(|_| NativeError::Permission)?;
        if agent != self.agent
            || record.req.agent != self.agent
            || proposal.device != device
            || proposal.owner != self.owner
            || source_sha != proposal.source_sha256
            || record.req.content_hash != content_hash
            || record.status != crate::engine::ReqStatus::Pending
            || proposal.expires_at_ms <= at_ms
            || !image_tasks::matches(&self.engine.state, &record.req, &record.space)
            || !direct_writers(&self.engine.state, &record.space, &self.owner, &self.agent)
            || self
                .engine
                .state
                .spaces
                .get(&record.space)
                .is_none_or(|s| s.integrity_error.is_some())
            || self.engine.ready_epoch(&record.space).is_none()
        {
            return Err(NativeError::Permission);
        }
        let pending = self
            .engine
            .store
            .outbox_get(envelope.client_id())
            .map_err(|_| NativeError::Storage)?
            .ok_or(NativeError::NotReady)?;
        if !matches!(&pending.body, EventBody::RequestOpened { request: original } if original == &record.req)
            || self.image_task_pending_envelope(&pending)?.as_ref() != Some(envelope)
            || !self.engine.outbox_envelopes().contains(envelope)
        {
            return Err(NativeError::History);
        }
        Ok(())
    }
}
