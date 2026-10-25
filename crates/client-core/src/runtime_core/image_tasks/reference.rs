//! This producer's immutable original association; never inferred from a sealed copy.
use super::{
    image_tasks, DeviceCore, Event, EventBody, ItemContent, ItemKind, NativeError, Result,
    MAX_EVENT_BYTES,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use protocol::{ClientFrame, Envelope};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    request: String,
    content_hash: String,
    event_digest: String,
    wire: String,
}
fn key(client_id: &str) -> String {
    format!("runtime.image-task.reference/1/{client_id}")
}
impl DeviceCore {
    pub(super) fn retain_image_task_reference(
        &self,
        request: &str,
        event: &Event,
        envelope: &Envelope,
    ) -> Result<()> {
        let r = self
            .engine
            .state
            .requests
            .get(request)
            .ok_or(NativeError::History)?;
        let reference = Reference {
            request: request.into(),
            content_hash: r.req.content_hash.clone(),
            event_digest: event_log::content_hash_of(event),
            wire: STANDARD.encode(
                ClientFrame::Publish {
                    env: envelope.clone(),
                }
                .encode(),
            ),
        };
        let bytes = serde_json::to_string(&reference).map_err(|_| NativeError::Storage)?;
        if bytes.len() > super::super::reference::MAX_REFERENCE_BYTES {
            return Err(NativeError::Bounds);
        }
        self.engine
            .store
            .set_meta(&key(&event.client_id), &bytes)
            .map_err(|_| NativeError::Storage)
    }
    pub(in crate::runtime_core) fn image_task_pending_envelope(
        &self,
        pending: &Event,
    ) -> Result<Option<Envelope>> {
        let Some(raw) = self
            .engine
            .store
            .meta(&key(&pending.client_id))
            .map_err(|_| NativeError::Storage)?
        else {
            return Ok(None);
        };
        if raw.len() > super::super::reference::MAX_REFERENCE_BYTES {
            return Err(NativeError::Bounds);
        }
        let reference: Reference = serde_json::from_str(&raw).map_err(|_| NativeError::History)?;
        let r = self
            .engine
            .state
            .requests
            .get(&reference.request)
            .ok_or(NativeError::History)?;
        let p = r.req.tool_proposal.as_ref().ok_or(NativeError::History)?;
        image_tasks::parsed(p).map_err(|_| NativeError::History)?;
        let bytes = STANDARD
            .decode(&reference.wire)
            .map_err(|_| NativeError::History)?;
        let frame = ClientFrame::decode(&bytes).map_err(|_| NativeError::History)?;
        if frame.encode() != bytes || STANDARD.encode(&bytes) != reference.wire {
            return Err(NativeError::History);
        }
        let ClientFrame::Publish { env } = frame else {
            return Err(NativeError::History);
        };
        let shape = match &pending.body {
            EventBody::RequestOpened { request } => request == &r.req,
            EventBody::ItemCreated {
                item,
                kind: ItemKind::File,
                content: ItemContent::File(file),
                ..
            } => {
                item == &p.artifact
                    && file.mime == "image/png"
                    && self.engine.state.requests[&reference.request].status
                        == crate::engine::ReqStatus::Approved
            }
            _ => false,
        };
        if !shape
            || r.req.content_hash != reference.content_hash
            || r.req.agent != self.agent
            || p.device != pending.device.as_deref().unwrap_or("")
            || p.owner != self.owner
            || r.space != pending.space
            || event_log::content_hash_of(pending) != reference.event_digest
            || env.client_id() != pending.client_id
            || env.author() != self.agent
            || env.space() != pending.space
            || env.device() != pending.device.as_deref()
            || env.cert != pending.cert
            || env.at_ms() != pending.at_ms
            || env.seen() != pending.seen
            || !env.is_sealed()
            || env.sealed_kind() != Some(event_log::content::SealedKind::Application)
            || env.is_pruned()
            || env.invite.is_some()
            || env.recovery().is_some()
            || env.verify().is_err()
            || env.content().len() > MAX_EVENT_BYTES
        {
            return Err(NativeError::History);
        }
        let (_, copy) = self
            .engine
            .sealed_copy(pending)
            .ok_or(NativeError::History)?;
        if copy != env {
            return Err(NativeError::History);
        }
        Ok(Some(env))
    }
}
