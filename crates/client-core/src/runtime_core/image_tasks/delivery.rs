//! Transport changes only the original pending publication, never its creation.
use super::*;
use crate::blob_transport::BlobUploadReceipt;
use protocol::Envelope;

impl DeviceCore {
    /// Recheck the reviewed original after the local ItemCreated projection.
    /// Unlike task preparation this permits that exact pending artifact only.
    pub fn image_output_file(
        &self,
        request: &str,
        approval: &str,
        envelope: &Envelope,
        at_ms: i64,
    ) -> Result<FileDoc> {
        if !self.usable || at_ms < 0 {
            return Err(NativeError::History);
        }
        let (r, proposal) = self
            .engine
            .tool_resolution(request, approval)
            .map_err(|_| NativeError::Permission)?;
        image_tasks::parsed(proposal).map_err(|_| NativeError::Permission)?;
        let (agent, device, source_sha) = self
            .engine
            .image_task_actor_at(&r.space, &proposal.source, &proposal.serving, at_ms)
            .map_err(|_| NativeError::Permission)?;
        if agent != self.agent
            || device != proposal.device
            || proposal.owner != self.owner
            || source_sha != proposal.source_sha256
            || proposal.expires_at_ms <= at_ms
            || !direct_writers(&self.engine.state, &r.space, &self.owner, &self.agent)
            || self
                .engine
                .state
                .spaces
                .get(&r.space)
                .is_none_or(|s| s.integrity_error.is_some())
            || self.engine.ready_epoch(&r.space).is_none()
        {
            return Err(NativeError::Permission);
        }
        let pending = self
            .engine
            .store
            .outbox_get(envelope.client_id())
            .map_err(|_| NativeError::Storage)?
            .ok_or(NativeError::NotReady)?;
        if self.image_task_pending_envelope(&pending)?.as_ref() != Some(envelope) {
            return Err(NativeError::History);
        }
        let EventBody::ItemCreated {
            item,
            kind: ItemKind::File,
            content: ItemContent::File(file),
            ..
        } = &pending.body
        else {
            return Err(NativeError::History);
        };
        let state = self
            .engine
            .state
            .items
            .get(item)
            .ok_or(NativeError::History)?;
        let result = self
            .engine
            .store
            .meta(&format!("runtime.image-task.result/1/{request}"))
            .map_err(|_| NativeError::Storage)?
            .ok_or(NativeError::History)?;
        let original: (String, String, String) =
            serde_json::from_str(&result).map_err(|_| NativeError::History)?;
        if item != &proposal.artifact
            || state.space != r.space
            || state.current().number != 1
            || state.current().content != ItemContent::File(file.clone())
            || original != (approval.into(), item.clone(), file.sha256.clone())
            || file.mime != "image/png"
        {
            return Err(NativeError::History);
        }
        Ok(file.clone())
    }

    pub fn image_output_upload_pending(&self, blob: &str) -> Result<bool> {
        self.engine
            .store
            .upload_pending(blob)
            .map_err(|_| NativeError::Storage)
    }

    /// The receipt's private constructor belongs to the actual signed HTTP PUT.
    /// It must match a canonical chunk in this already-authorized original.
    pub fn acknowledge_image_upload(
        &mut self,
        request: &str,
        approval: &str,
        envelope: &Envelope,
        receipt: &BlobUploadReceipt,
        at_ms: i64,
    ) -> Result<NativeImage> {
        let file = self.image_output_file(request, approval, envelope, at_ms)?;
        let chunk = file
            .chunks
            .iter()
            .find(|chunk| chunk.blob == receipt.blob())
            .ok_or(NativeError::History)?;
        if receipt.device() != envelope.device().ok_or(NativeError::Credential)?
            || chunk.bytes.checked_add(40) != Some(receipt.bytes())
        {
            return Err(NativeError::History);
        }
        self.engine
            .store
            .upload_done(receipt.blob())
            .map_err(|_| NativeError::Storage)?;
        self.image()
    }

    pub fn image_output_ready(
        &self,
        request: &str,
        approval: &str,
        envelope: &Envelope,
        at_ms: i64,
    ) -> Result<()> {
        self.image_output_file(request, approval, envelope, at_ms)?;
        if !self.engine.outbox_envelopes().contains(envelope) {
            return Err(NativeError::NotReady);
        }
        Ok(())
    }
}
