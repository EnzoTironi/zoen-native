use crate::{hash, RuntimeError};
use serde::{Deserialize, Serialize};
use zoen_models::{DispatchRequest, ModelAttemptResult, Operation, UsageEvidence, CHAT_PROFILE};

/// A configured obligation rule, not a provider invoice or a token estimator.
/// The host must supply a defensible finite input bound for this exact profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceProfile {
    pub version: String,
    pub endpoint: String,
    pub model: String,
    pub credential_ref: String,
    pub currency: String,
    pub scale: u8,
    pub input_units_per_million: u64,
    pub output_units_per_million: u64,
    pub max_billable_input_tokens: u64,
    pub max_output_tokens: u64,
    pub max_request_bytes: u64,
    pub max_response_bytes: u64,
}

impl PriceProfile {
    pub fn digest(&self) -> Result<String, RuntimeError> {
        self.validate()?;
        Ok(hash(
            &serde_json::to_vec(self).map_err(|_| RuntimeError::InvalidProfile)?,
        ))
    }

    pub(crate) fn validate(&self) -> Result<(), RuntimeError> {
        if !crate::id(&self.version)
            || !crate::id(&self.model)
            || !crate::id(&self.credential_ref)
            || self.endpoint.len() > 2048
            || !self.endpoint.ends_with("/v1/chat/completions")
            || self.currency != "USD"
            || self.scale != 6
            || self.max_billable_input_tokens == 0
            || self.max_output_tokens == 0
            || !(1..=32768).contains(&self.max_request_bytes)
            || !(1..=16384).contains(&self.max_response_bytes)
        {
            return Err(RuntimeError::InvalidProfile);
        }
        self.cost(self.max_billable_input_tokens, self.max_output_tokens)?;
        Ok(())
    }

    pub(crate) fn quote(&self, request: &DispatchRequest) -> Result<i64, RuntimeError> {
        self.validate()?;
        let d = &request.descriptor;
        if request.profile != CHAT_PROFILE
            || d.operation != Operation::ChatCompletion
            || request.context.price_version != self.version
            || d.endpoint != self.endpoint
            || d.model != self.model
            || d.credential_ref != self.credential_ref
            || d.max_request_bytes > self.max_request_bytes
            || d.encoded_request_bytes > d.max_request_bytes
            || d.max_response_bytes > self.max_response_bytes
            || d.requested_output_tokens == 0
            || d.requested_output_tokens > self.max_output_tokens
        {
            return Err(RuntimeError::InvalidProfile);
        }
        self.reserve(d.requested_output_tokens)
    }
    pub(crate) fn reserve(&self, output: u64) -> Result<i64, RuntimeError> {
        self.validate()?;
        if output == 0 || output > self.max_output_tokens {
            return Err(RuntimeError::InvalidProfile);
        }
        self.cost(self.max_billable_input_tokens, output)
    }

    fn cost(&self, input: u64, output: u64) -> Result<i64, RuntimeError> {
        let value = u128::from(input)
            .checked_mul(u128::from(self.input_units_per_million))
            .and_then(|a| {
                u128::from(output)
                    .checked_mul(u128::from(self.output_units_per_million))
                    .and_then(|b| a.checked_add(b))
            })
            .and_then(|v| v.checked_add(999_999))
            .ok_or(RuntimeError::InvalidProfile)?
            / 1_000_000;
        i64::try_from(value).map_err(|_| RuntimeError::InvalidProfile)
    }

    pub(crate) fn charge(&self, result: &ModelAttemptResult) -> Result<Option<i64>, RuntimeError> {
        if !result.evidence.complete
            || !result
                .receipt
                .http_status
                .is_some_and(|s| (200..300).contains(&s))
            || result.receipt.reported_model.as_deref() != Some(&self.model)
        {
            return Ok(None);
        }
        let UsageEvidence::Reported(usage) = &result.receipt.usage else {
            return Ok(None);
        };
        let (Some(input), Some(output)) = (usage.input_tokens, usage.output_tokens) else {
            return Ok(None);
        };
        // Differential cache/reasoning tariffs require a separate supported profile.
        if usage.cache_read_tokens.is_some_and(|v| v != 0)
            || usage.reasoning_tokens.is_some_and(|v| v != 0)
            || input.checked_add(output).is_none()
            || usage
                .total_tokens
                .is_some_and(|v| Some(v) != input.checked_add(output))
        {
            return Ok(None);
        }
        // A supported overrun records the full charge; it is never capped at the hold.
        self.cost(input, output).map(Some)
    }
}
