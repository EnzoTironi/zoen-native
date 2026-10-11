use crate::transport::Capture;
use crate::{ProviderReceipt, ReportedUsage, UsageEvidence};
use serde_json::Value;

fn optional_counter(value: &Value, key: &str) -> Result<Option<u64>, ()> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value.as_u64().map(Some).ok_or(()),
    }
}

fn detail(value: &Value, object: &str, key: &str) -> Result<Option<u64>, ()> {
    match value.get(object) {
        None | Some(Value::Null) => Ok(None),
        Some(value) if value.is_object() => optional_counter(value, key),
        Some(_) => Err(()),
    }
}

fn usage(value: Option<&Value>) -> UsageEvidence {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return UsageEvidence::Missing;
    };
    let parsed = (|| {
        if !value.is_object() {
            return Err(());
        }
        let counters = ReportedUsage {
            input_tokens: optional_counter(value, "prompt_tokens")?,
            output_tokens: optional_counter(value, "completion_tokens")?,
            total_tokens: optional_counter(value, "total_tokens")?,
            cache_read_tokens: detail(value, "prompt_tokens_details", "cached_tokens")?,
            reasoning_tokens: detail(value, "completion_tokens_details", "reasoning_tokens")?,
        };
        if let (Some(input), Some(output)) = (counters.input_tokens, counters.output_tokens) {
            let sum = input.checked_add(output).ok_or(())?;
            if counters.total_tokens.is_some_and(|total| total != sum) {
                return Err(());
            }
        }
        if counters.total_tokens.is_some_and(|total| {
            [
                counters.input_tokens,
                counters.output_tokens,
                counters.cache_read_tokens,
                counters.reasoning_tokens,
            ]
            .into_iter()
            .flatten()
            .any(|counter| counter > total)
        }) {
            return Err(());
        }
        if let (Some(cache), Some(input)) = (counters.cache_read_tokens, counters.input_tokens) {
            if cache > input {
                return Err(());
            }
        }
        if let (Some(reasoning), Some(output)) = (counters.reasoning_tokens, counters.output_tokens)
        {
            if reasoning > output {
                return Err(());
            }
        }
        Ok(counters)
    })();
    match parsed {
        Ok(counters) => UsageEvidence::Reported(counters),
        Err(()) => UsageEvidence::Invalid,
    }
}

pub(crate) fn receipt(capture: &Capture) -> (ProviderReceipt, Option<Value>) {
    let value = capture
        .complete
        .then(|| serde_json::from_slice::<Value>(&capture.body).ok())
        .flatten();
    let bounded_id = |key: &str| {
        value
            .as_ref()
            .and_then(|value| value.get(key))
            .and_then(Value::as_str)
            .filter(|id| crate::validation::identifier(id))
            .map(str::to_owned)
    };
    let receipt = ProviderReceipt {
        http_status: capture.status,
        request_id: capture.request_id.clone(),
        response_id: bounded_id("id"),
        reported_model: bounded_id("model"),
        usage: usage(value.as_ref().and_then(|value| value.get("usage"))),
    };
    (receipt, value)
}
