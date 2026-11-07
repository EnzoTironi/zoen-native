//! The same signed conversation approval contract inside retained native custody.
use super::*;
use crate::{AgentRequestDto, AgentToolApprovalInput, MessageSearchPage};
use protocol::{ClientFrame, Envelope};
use serde_json::Value;

/// Retained read data and its original review locators, never an approval permit.
pub struct ReviewedReplyHistory {
    pub request: String,
    pub content_hash: String,
    pub read: ReplyHistoryRead,
}

/// Native proposal and post-sealing image, never cloud publication or approval
/// authority. The runtime must bind the input to the actual retained model call
/// and atomically retain this image and original wire before any transport.
pub struct StagedConversationReview {
    request: AgentRequestDto,
    envelope: Option<Envelope>,
    image: NativeImage,
}
impl StagedConversationReview {
    #[must_use]
    pub fn request(&self) -> &AgentRequestDto {
        &self.request
    }
    /// Absent when the original review is already in authenticated history.
    #[must_use]
    pub fn envelope(&self) -> Option<&Envelope> {
        self.envelope.as_ref()
    }
    #[must_use]
    pub fn image(&self) -> &NativeImage {
        &self.image
    }
}

impl DeviceCore {
    /// Reuse Engine's canonical request/task/query/device binding. An uncertain
    /// publish retries the same sealed outbox original rather than a fresh Ask.
    pub fn stage_reply_search_review(
        &mut self,
        original: &ReplyIntent,
        input: AgentToolApprovalInput,
        at_ms: i64,
    ) -> Result<StagedConversationReview> {
        let result = (|| {
            self.refresh(original, at_ms)?;
            if self
                .engine
                .store
                .outbox_len()
                .map_err(|_| NativeError::Storage)?
                > 1
            {
                return Err(NativeError::NotReady);
            }
            let previous = self
                .engine
                .store
                .outbox()
                .map_err(|_| NativeError::Storage)?
                .into_iter()
                .next()
                .map(|row| row.event);
            let source = self.review_source(original)?;
            let request = self
                .engine
                .stage_conversation_search_approval_at(
                    original.space(),
                    &source,
                    &original.owner,
                    input,
                    at_ms,
                )
                .map_err(|_| NativeError::Permission)?;
            let hash = request
                .tool_approval
                .as_ref()
                .ok_or(NativeError::History)?
                .content_hash
                .clone();
            self.review_record(original, &request.id, &hash)?;
            if let Some(previous) = previous {
                if !matches!(&previous.body, EventBody::RequestOpened { request: pending } if pending.id == request.id)
                {
                    return Err(NativeError::NotReady);
                }
            } else {
                self.engine
                    .mls_seal_outbox()
                    .map_err(|_| NativeError::Storage)?;
            }
            let envelopes = self.engine.outbox_envelopes();
            let envelope = match envelopes.as_slice() {
                [envelope] => {
                    let pending = self
                        .engine
                        .store
                        .outbox_get(envelope.client_id())
                        .map_err(|_| NativeError::Storage)?
                        .ok_or(NativeError::History)?;
                    self.retain_conversation_review_reference(
                        original,
                        &request.id,
                        &pending,
                        envelope,
                    )?;
                    self.reply_search_review_ready(original, &request.id, &hash, envelope, at_ms)?;
                    Some(envelope.clone())
                }
                [] if self
                    .engine
                    .store
                    .outbox_len()
                    .map_err(|_| NativeError::Storage)?
                    == 0 =>
                {
                    let record = self
                        .engine
                        .state
                        .requests
                        .get(&request.id)
                        .ok_or(NativeError::History)?;
                    if self.engine.logs.get(original.space()).is_none_or(|log| {
                        !log.events().iter().any(|event| {
                            event.author == original.agent() && event.device.as_deref() == Some(record.req.tool_proposal.as_ref().map_or("", |p| p.device.as_str()))
                                && matches!(&event.body, EventBody::RequestOpened { request } if request == &record.req)
                        })
                    }) {
                        return Err(NativeError::History);
                    }
                    None
                }
                _ => return Err(NativeError::NotReady),
            };
            Ok(StagedConversationReview {
                request,
                envelope,
                image: self.image()?,
            })
        })();
        if result.is_err() {
            self.usable = false;
        }
        result
    }

    /// Current native authority for only this exact pending review wire. This
    /// is neither owner consent nor permission to publish from a cloud host.
    pub fn reply_search_review_ready(
        &self,
        original: &ReplyIntent,
        request: &str,
        content_hash: &str,
        envelope: &Envelope,
        at_ms: i64,
    ) -> Result<()> {
        self.refresh(original, at_ms)?;
        let record = self.review_record(original, request, content_hash)?;
        let proposal = record
            .req
            .tool_proposal
            .as_ref()
            .ok_or(NativeError::History)?;
        if record.status != crate::engine::ReqStatus::Pending
            || proposal.expires_at_ms <= at_ms
            || !self.engine.state.tool_review_current(record)
            || !crate::agent_conversation_tools::matches(
                &self.engine.state,
                &record.req,
                original.space(),
            )
        {
            return Err(NativeError::Permission);
        }
        let pending = self
            .engine
            .store
            .outbox_get(envelope.client_id())
            .map_err(|_| NativeError::Storage)?
            .ok_or(NativeError::NotReady)?;
        if !matches!(&pending.body, EventBody::RequestOpened { request } if request == &record.req)
            || self.pending_envelope(&pending, None)? != *envelope
            || !self.engine.outbox_envelopes().contains(envelope)
            || envelope.author() != self.agent
            || envelope.space() != original.space()
            || envelope.device() != Some(proposal.device.as_str())
            || !envelope.is_sealed()
            || envelope.is_pruned()
            || envelope.invite.is_some()
            || envelope.recovery().is_some()
            || envelope.content().len() > MAX_EVENT_BYTES
            || (ClientFrame::Publish {
                env: envelope.clone(),
            })
            .encode()
            .len()
                > super::publication::MAX_PUBLISH_BYTES
            || envelope.verify().is_err()
        {
            return Err(NativeError::History);
        }
        Ok(())
    }

    /// Read-only status for the same original, including after revocation. It
    /// grants no read, continuation, new request, transport or financial effect.
    pub fn reply_search_review_status(
        &self,
        original: &ReplyIntent,
        request: &str,
        content_hash: &str,
    ) -> Result<AgentRequestDto> {
        let record = self.review_record(original, request, content_hash)?;
        Ok(self.engine.request_dto(record))
    }

    /// Consume a scratch view: exact approved arguments, fresh native permission
    /// and retained-page verification are all required on every read/replay.
    pub fn approved_reply_history(
        self,
        original: &ReplyIntent,
        request: &str,
        content_hash: &str,
        reads: &[ReviewedReplyHistory],
        arguments: &Value,
        at_ms: i64,
    ) -> Result<MessageSearchPage> {
        self.refresh(original, at_ms)?;
        self.approved_review_args(original, request, content_hash, arguments, at_ms)?;
        if reads.len() > 2 {
            return Err(NativeError::Bounds);
        }
        let reads = self.approved_reads(original, reads, at_ms)?;
        self.continue_reply_history(original, &reads, arguments, at_ms)
    }

    /// Revalidate each original owner decision and actual page before further
    /// inference/publication. Historical read text never renews expired consent.
    pub fn verify_approved_reply_history(
        self,
        original: &ReplyIntent,
        reads: &[ReviewedReplyHistory],
        at_ms: i64,
    ) -> Result<()> {
        self.refresh(original, at_ms)?;
        let reads = self.approved_reads(original, reads, at_ms)?;
        self.verify_reply_history(original, &reads, at_ms)
    }

    fn approved_review_args(
        &self,
        original: &ReplyIntent,
        request: &str,
        content_hash: &str,
        arguments: &Value,
        at_ms: i64,
    ) -> Result<()> {
        self.review_record(original, request, content_hash)?;
        let (_, proposal) = self
            .engine
            .conversation_search_permit(request, content_hash)
            .map_err(|_| NativeError::Permission)?;
        if proposal.expires_at_ms <= at_ms
            || serde_json::from_str::<Value>(&proposal.arguments_json)
                .map_err(|_| NativeError::History)?
                != *arguments
        {
            return Err(NativeError::Permission);
        }
        Ok(())
    }

    fn approved_reads(
        &self,
        original: &ReplyIntent,
        reads: &[ReviewedReplyHistory],
        at_ms: i64,
    ) -> Result<Vec<ReplyHistoryRead>> {
        if reads.len() > 3 {
            return Err(NativeError::Bounds);
        }
        reads
            .iter()
            .map(|read| {
                self.approved_review_args(
                    original,
                    &read.request,
                    &read.content_hash,
                    &read.read.arguments,
                    at_ms,
                )?;
                Ok(ReplyHistoryRead {
                    arguments: read.read.arguments.clone(),
                    page: read.read.page.clone(),
                })
            })
            .collect()
    }

    fn review_source(&self, original: &ReplyIntent) -> Result<String> {
        if original.agent() != self.agent
            || original.owner != self.owner
            || !matches!(original.profile.as_str(), PROFILE | LEGACY_PROFILE)
        {
            return Err(NativeError::History);
        }
        self.verify_original_history(original)?;
        self.engine
            .logs
            .get(original.space())
            .and_then(|log| {
                log.events()
                    .iter()
                    .find(|event| event.hash == original.trigger())
                    .map(|event| event.client_id.clone())
            })
            .ok_or(NativeError::History)
    }

    pub(super) fn review_record(
        &self,
        original: &ReplyIntent,
        request: &str,
        content_hash: &str,
    ) -> Result<&crate::engine::RequestState> {
        if !self.usable
            || request.is_empty()
            || request.len() > 256
            || !crate::agent_page_tools::valid_digest(content_hash)
        {
            return Err(NativeError::Bounds);
        }
        let source = self.review_source(original)?;
        let record = self
            .engine
            .state
            .requests
            .get(request)
            .ok_or(NativeError::History)?;
        let proposal = record
            .req
            .tool_proposal
            .as_ref()
            .ok_or(NativeError::History)?;
        let account = self
            .engine
            .net
            .account
            .as_ref()
            .ok_or(NativeError::Credential)?;
        if record.space != original.space()
            || record.req.agent != self.agent
            || record.req.content_hash != content_hash
            || proposal.source != source
            || proposal.device != account.device
            || proposal.owner != self.owner
            || proposal.serving != self.owner
            || record.req.tool_proposal_hash(original.space()).as_deref() != Some(content_hash)
            || crate::agent_conversation_tools::parsed(proposal).is_err()
        {
            return Err(NativeError::Permission);
        }
        Ok(record)
    }
}
