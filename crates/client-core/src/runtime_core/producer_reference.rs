//! This producer's immutable original association; never inferred from a sealed copy.
use super::{
    DeviceCore, Event, EventBody, ItemContent, ItemKind, NativeError, ReplyIntent, Result,
    MAX_EVENT_BYTES,
};
use crate::image_tasks;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    original: Option<ReplyIntent>,
}
#[derive(Clone, Copy)]
enum Producer {
    ImageTask,
    ConversationReview,
}
impl Producer {
    fn key(self, client_id: &str) -> String {
        let prefix = match self {
            Self::ImageTask => "runtime.image-task.reference/1/",
            Self::ConversationReview => "runtime.conversation-review.reference/1/",
        };
        format!("{prefix}{client_id}")
    }
}
impl DeviceCore {
    pub(super) fn retain_image_task_reference(
        &self,
        request: &str,
        event: &Event,
        envelope: &Envelope,
    ) -> Result<()> {
        self.retain_producer_reference(Producer::ImageTask, None, request, event, envelope)
    }
    pub(super) fn retain_conversation_review_reference(
        &self,
        original: &ReplyIntent,
        request: &str,
        event: &Event,
        envelope: &Envelope,
    ) -> Result<()> {
        self.retain_producer_reference(
            Producer::ConversationReview,
            Some(original.clone()),
            request,
            event,
            envelope,
        )
    }
    fn retain_producer_reference(
        &self,
        producer: Producer,
        original: Option<ReplyIntent>,
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
            original,
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
        if bytes.len() > super::reference::MAX_REFERENCE_BYTES {
            return Err(NativeError::Bounds);
        }
        let key = producer.key(&event.client_id);
        if let Some(old) = self
            .engine
            .store
            .meta(&key)
            .map_err(|_| NativeError::Storage)?
        {
            if old != bytes {
                return Err(NativeError::History);
            }
            return Ok(());
        }
        self.engine
            .store
            .set_meta(&key, &bytes)
            .map_err(|_| NativeError::Storage)
    }
    pub(in crate::runtime_core) fn image_task_pending_envelope(
        &self,
        pending: &Event,
    ) -> Result<Option<Envelope>> {
        self.producer_pending_envelope(Producer::ImageTask, pending)
    }
    pub(in crate::runtime_core) fn conversation_review_pending_envelope(
        &self,
        pending: &Event,
    ) -> Result<Option<Envelope>> {
        self.producer_pending_envelope(Producer::ConversationReview, pending)
    }
    fn producer_pending_envelope(
        &self,
        producer: Producer,
        pending: &Event,
    ) -> Result<Option<Envelope>> {
        self.producer_reference_envelope(producer, pending, true)
    }
    fn producer_reference_envelope(
        &self,
        producer: Producer,
        pending: &Event,
        queued: bool,
    ) -> Result<Option<Envelope>> {
        let Some(raw) = self
            .engine
            .store
            .meta(&producer.key(&pending.client_id))
            .map_err(|_| NativeError::Storage)?
        else {
            return Ok(None);
        };
        if raw.len() > super::reference::MAX_REFERENCE_BYTES {
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
        match producer {
            Producer::ImageTask => {
                if reference.original.is_some() {
                    return Err(NativeError::History);
                }
                image_tasks::parsed(p).map_err(|_| NativeError::History)?;
            }
            Producer::ConversationReview => {
                let original = reference.original.as_ref().ok_or(NativeError::History)?;
                self.review_record(original, &reference.request, &reference.content_hash)?;
            }
        }
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
                matches!(producer, Producer::ImageTask)
                    && item == &p.artifact
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
        if queued {
            let (_, copy) = self
                .engine
                .sealed_copy(pending)
                .ok_or(NativeError::History)?;
            if copy != env {
                return Err(NativeError::History);
            }
        } else {
            let account = self
                .engine
                .net
                .account
                .as_ref()
                .ok_or(NativeError::Credential)?;
            if !crate::mls_opening::exact_own_echo(&self.engine.store, pending, account, &env) {
                return Err(NativeError::History);
            }
        }
        Ok(Some(env))
    }

    /// Historical original only, including after permission expiry or receipt.
    /// This cannot authorize transport, clear a queue, infer acceptance or spend.
    pub fn retained_reply_search_review(
        &self,
        original: &ReplyIntent,
        request: &str,
        content_hash: &str,
        client_id: &str,
    ) -> Result<Envelope> {
        self.review_record(original, request, content_hash)?;
        if client_id.is_empty() || client_id.len() > 256 {
            return Err(NativeError::Bounds);
        }
        let raw = self
            .engine
            .store
            .meta(&Producer::ConversationReview.key(client_id))
            .map_err(|_| NativeError::Storage)?
            .ok_or(NativeError::History)?;
        if raw.len() > super::reference::MAX_REFERENCE_BYTES {
            return Err(NativeError::Bounds);
        }
        let reference: Reference = serde_json::from_str(&raw).map_err(|_| NativeError::History)?;
        if reference.original.as_ref() != Some(original)
            || reference.request != request
            || reference.content_hash != content_hash
        {
            return Err(NativeError::History);
        }
        if let Some(event) = self
            .engine
            .store
            .outbox_get(client_id)
            .map_err(|_| NativeError::Storage)?
        {
            return self
                .producer_reference_envelope(Producer::ConversationReview, &event, true)?
                .ok_or(NativeError::History);
        }
        let event = self
            .engine
            .logs
            .get(original.space())
            .and_then(|log| {
                log.events()
                    .iter()
                    .find(|event| event.client_id == client_id)
            })
            .ok_or(NativeError::History)?;
        self.producer_reference_envelope(Producer::ConversationReview, event, false)?
            .ok_or(NativeError::History)
    }
    /// Native historical position only; the runtime must independently verify
    /// the actual relay envelope and activate the same source/capsule checkpoint.
    pub fn accepted_reply_search_review(
        &self,
        original: &ReplyIntent,
        request: &str,
        content_hash: &str,
        client_id: &str,
    ) -> Result<Option<types::Seen>> {
        self.retained_reply_search_review(original, request, content_hash, client_id)?;
        Ok(self.engine.logs.get(original.space()).and_then(|log| {
            log.events()
                .iter()
                .find(|event| event.client_id == client_id)
                .map(|event| types::Seen {
                    seq: event.seq,
                    hash: event.hash.clone(),
                })
        }))
    }
}
