use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Review {
    pub(super) request: String,
    pub(super) content_hash: String,
    pub(super) client_id: String,
    pub(super) wire: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DecisionRecord {
    pub(super) format: u8,
    pub(super) run: String,
    pub(super) original: String,
    pub(super) capture: String,
    pub(super) index: u8,
    pub(super) policy: String,
    pub(super) audit: gate::AuditRecord,
    pub(super) review: Option<Review>,
    pub(super) source: SharedSource,
    pub(super) root: Root,
    pub(super) sequence: u64,
    pub(super) expires: Option<i64>,
}
impl DecisionRecord {
    pub(super) fn verify_core(
        &self,
        record: &RunRecord,
        core: &DeviceCore,
    ) -> Result<(), RuntimeError> {
        if let Some(review) = &self.review {
            core.reply_search_review_bound(&record.intent, &review.request, &review.content_hash,
                &serde_json::json!({"original":self.original,"capture":self.capture,"call":self.index})).map_err(native_error)?;
            self.envelope(record, core)?;
        }
        Ok(())
    }
    pub(super) fn validate(
        &self,
        record: &RunRecord,
        original: &str,
        capture: &RetainedCapture,
        blocks: &[OutputBlock],
        index: u8,
    ) -> Result<(), RuntimeError> {
        let calls = search_calls(blocks);
        let call = calls
            .get(usize::from(index))
            .ok_or(RuntimeError::InvalidBinding)?;
        let check = tool_check(record, call)?;
        if self.format != 1
            || self.run != record.locator.run
            || self.original != original
            || self.capture != capture.association.digest
            || self.index != index
            || !digest(&self.policy)
            || self.audit.point != gate::Point::ToolCall.id()
            || self.audit.input_hash != hash(check.text.as_bytes())
            || ![
                gate::Outcome::Allow.id(),
                gate::Outcome::Ask.id(),
                gate::Outcome::Block.id(),
            ]
            .contains(&self.audit.outcome.as_str())
            || self.review.is_some() != (self.audit.outcome == gate::Outcome::Ask.id())
            || self.source.cell != record.source
            || self.source.space != record.intent.space()
            || self.source.expected != self.source.through
            || self.sequence == 0
            || self.root.context.principal != record.original.context.principal
        {
            return Err(RuntimeError::InvalidBinding);
        }
        self.root.context.validate(
            &record.original.context.principal,
            self.root.context.key_version,
        )?;
        if let Some(review) = &self.review {
            if !id(&review.request)
                || !id(&review.client_id)
                || !digest(&review.content_hash)
                || !digest(&review.wire)
            {
                return Err(RuntimeError::InvalidBinding);
            }
        }
        Ok(())
    }
    pub(super) fn envelope(
        &self,
        record: &RunRecord,
        core: &DeviceCore,
    ) -> Result<Option<Envelope>, RuntimeError> {
        self.review
            .as_ref()
            .map(|review| {
                let wire = core
                    .retained_reply_search_review(
                        &record.intent,
                        &review.request,
                        &review.content_hash,
                        &review.client_id,
                    )
                    .map_err(native_error)?;
                if hash(&ClientFrame::Publish { env: wire.clone() }.encode()) != review.wire {
                    return Err(RuntimeError::InvalidBinding);
                }
                Ok(wire)
            })
            .transpose()
    }
}
