//! Derive one explicitly approved task from the same authenticated native state.
mod delivery;
mod output;
mod reference;
use super::*;
use crate::image_tasks::{self, ImageTaskInput};
pub use output::PreparedImageTask;

/// Not constructible/deserializable by hosts, models or WebViews. These facts
/// still require live runtime source, directory, budget and continuity fences.
pub struct ImageTaskFacts {
    pub(super) request: String,
    pub(super) proposal: ToolProposal,
    pub(super) content_hash: String,
    pub(super) space: String,
    pub(super) agent: String,
    pub(super) frontier: NativeFrontier,
    pub(super) input: ImageTaskInput,
    expires_at_ms: i64,
}
impl std::fmt::Debug for ImageTaskFacts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ImageTaskFacts { redacted }")
    }
}
impl ImageTaskFacts {
    pub fn request(&self) -> &str {
        &self.request
    }
    pub fn proposal(&self) -> &ToolProposal {
        &self.proposal
    }
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
    pub fn space(&self) -> &str {
        &self.space
    }
    pub fn agent(&self) -> &str {
        &self.agent
    }
    pub fn frontier(&self) -> &NativeFrontier {
        &self.frontier
    }
    pub fn expires_at_ms(&self) -> i64 {
        self.expires_at_ms
    }
    pub fn input(&self) -> &ImageTaskInput {
        &self.input
    }
}
impl DeviceCore {
    /// Retain the signed proposal and its original wire before any host delivers
    /// it. This is an approval request, not paid execution or output authority.
    pub fn stage_image_task(
        &mut self,
        space: &str,
        source: &str,
        input: ImageTaskInput,
    ) -> Result<(crate::AgentRequestDto, protocol::Envelope, NativeImage)> {
        let result = (|| {
            if !self.usable
                || self
                    .engine
                    .store
                    .outbox_len()
                    .map_err(|_| NativeError::Storage)?
                    != 0
            {
                return Err(NativeError::NotReady);
            }
            let owner = self.owner.clone();
            let request = self
                .engine
                .stage_image_generation(space, source, &owner, input)
                .map_err(|_| NativeError::Permission)?;
            self.engine
                .mls_seal_outbox()
                .map_err(|_| NativeError::Storage)?;
            let envelopes = self.engine.outbox_envelopes();
            let [envelope] = envelopes.as_slice() else {
                return Err(NativeError::NotReady);
            };
            let event = self
                .engine
                .store
                .outbox_get(envelope.client_id())
                .map_err(|_| NativeError::Storage)?
                .ok_or(NativeError::History)?;
            if envelope.content().len() > MAX_EVENT_BYTES {
                return Err(NativeError::Bounds);
            }
            self.retain_image_task_reference(&request.id, &event, envelope)?;
            Ok((request, envelope.clone(), self.image()?))
        })();
        if result.is_err() {
            self.usable = false;
        }
        result
    }
    pub fn image_task(&self, request: &str, at_ms: i64) -> Result<ImageTaskFacts> {
        if !self.usable || at_ms < 0 {
            return Err(NativeError::History);
        }
        let r = self
            .engine
            .state
            .requests
            .get(request)
            .ok_or(NativeError::Permission)?;
        let (_, p) = self
            .engine
            .tool_resolution(request, &r.req.content_hash)
            .map_err(|_| NativeError::Permission)?;
        image_tasks::parsed(p).map_err(|_| NativeError::Permission)?;
        let (agent, device, source_sha) = self
            .engine
            .image_task_actor_at(&r.space, &p.source, &p.serving, at_ms)
            .map_err(|_| NativeError::Permission)?;
        if agent != self.agent
            || r.req.agent != agent
            || p.device != device
            || p.owner != self.owner
            || p.source_sha256 != source_sha
            || p.expires_at_ms <= at_ms
            || !image_tasks::matches(&self.engine.state, &r.req, &r.space)
            || !direct_writers(&self.engine.state, &r.space, &self.owner, &self.agent)
            || self
                .engine
                .state
                .spaces
                .get(&r.space)
                .is_none_or(|s| s.integrity_error.is_some())
        {
            return Err(NativeError::Permission);
        }
        let epoch = self
            .engine
            .ready_epoch(&r.space)
            .ok_or(NativeError::NotReady)?;
        let head = self
            .engine
            .logs
            .get(&r.space)
            .and_then(SpaceLog::head)
            .ok_or(NativeError::History)?;
        let trust_expiry = self
            .engine
            .image_task_trust_deadline(&self.agent, &self.owner, &r.space, at_ms)
            .map_err(|_| NativeError::Permission)?;
        let expires_at_ms = p.expires_at_ms.min(trust_expiry.unwrap_or(p.expires_at_ms));
        let input = serde_json::from_str(&p.arguments_json).map_err(|_| NativeError::Bounds)?;
        Ok(ImageTaskFacts {
            request: request.into(),
            proposal: p.clone(),
            content_hash: r.req.content_hash.clone(),
            space: r.space.clone(),
            agent,
            frontier: NativeFrontier {
                seq: head.seq,
                hash: head.hash,
                epoch,
            },
            input,
            expires_at_ms,
        })
    }
}
