//! Frozen run records preserve the exact profile and original serialized input.
use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RunRecord {
    pub(super) format: u8,
    pub(super) source: String,
    pub(super) locator: ReplyRun,
    pub(super) request: ModelRequest,
    pub(super) binding: Binding,
    pub(super) intent: ReplyIntent,
    pub(super) original: Root,
    pub(super) head: Seen,
    pub(super) epoch: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) agent_context: Option<context::AgentContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) continuation: Option<turn::Continuation>,
}

impl RunRecord {
    pub(super) fn validate(&self, run: &str, source: &str) -> Result<(), RuntimeError> {
        let context = &self.request.context;
        let principal = &self.original.context.principal;
        self.original
            .context
            .validate(principal, self.original.context.key_version)?;
        let expected_profile = self.validate_context()?;
        if self.source != source
            || self.locator.run != run
            || ![&self.locator.run, &self.locator.attempt, &self.locator.wake]
                .into_iter()
                .all(|value| id(value))
            || context != &self.binding.context
            || context.run_id != run
            || context.attempt_id != self.locator.attempt
            || context.owner != principal.owner
            || context.agent != principal.agent
            || context.device != principal.device
            || hash(self.binding.device_cert.as_bytes()) != principal.certificate_digest
            || context.space != self.intent.space()
            || context.agent != self.intent.agent()
            || !digest(self.intent.trigger())
            || !digest(&self.head.hash)
            || !digest(&self.original.manifest_digest)
            || !digest(&self.binding.request_digest)
            || context.authority_version != self.head.hash
            || self.binding.frontier != self.head.hash
            || context.definition_version != expected_profile
            || context.price_version != self.binding.price.chat()?.version
            || self.request.operation != Operation::ChatCompletion
            || self.request.max_output_tokens != self.binding.requested_output_tokens
            || self.request.tools != turn::tools(self.format)
            || (self.request.tool_mode == zoen_models::ToolMode::FinalAnswer) != (self.format == 4)
            || self.binding.hold_units
                != self.binding.price.reserve(self.request.max_output_tokens)?
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct Wake {
    pub(super) locator: ReplyRun,
    pub(super) record_digest: String,
}

#[derive(Serialize, Deserialize)]
pub(super) struct PreparedRun {
    pub(super) trigger: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) parent: Option<String>,
    pub(super) locator: ReplyRun,
    pub(super) record: Vec<u8>,
    pub(super) binding: Vec<u8>,
    pub(super) wake: Vec<u8>,
}
