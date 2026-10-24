use super::*;
use serde_json::{json, Value};
use zoen_models::{ImageUsageEvidence, UsageEvidence};

fn counter(value: &Value, key: &str, expected: Option<u64>) -> bool {
    match value.get(key) {
        None | Some(Value::Null) => expected.is_none(),
        Some(value) => value.as_u64().is_some_and(|v| Some(v) == expected),
    }
}

fn side(
    total: u64,
    text: Option<u64>,
    image: Option<u64>,
    text_rate: u64,
    image_rate: u64,
) -> Option<Vec<(u64, u64)>> {
    if [text, image].into_iter().flatten().any(|v| v > total) {
        return None;
    }
    if let (Some(text), Some(image)) = (text, image) {
        if text.checked_add(image) != Some(total) {
            return None;
        }
    }
    if text_rate == image_rate {
        return Some(vec![(total, text_rate)]);
    }
    let (Some(text), Some(image)) = (text, image) else {
        return None;
    };
    Some(vec![(text, text_rate), (image, image_rate)])
}

pub(super) fn charge(
    profile: &ImagePriceProfile,
    dispatch: &DispatchDescriptor,
    result: &ImageAttemptResult,
) -> Result<Option<i64>, RuntimeError> {
    if !result.evidence.complete
        || result.evidence.body.len() as u64 > dispatch.max_response_bytes
        || !result
            .receipt
            .http_status
            .is_some_and(|s| (200..300).contains(&s))
        || result
            .receipt
            .reported_model
            .as_ref()
            .is_some_and(|m| m != &profile.model)
    {
        return Ok(None);
    }
    let Ok(raw) = serde_json::from_slice::<Value>(&result.evidence.body) else {
        return Ok(None);
    };
    if !raw.is_object() {
        return Ok(None);
    }
    match raw.get("model").filter(|v| !v.is_null()) {
        Some(Value::String(model))
            if model == &profile.model && result.receipt.reported_model.as_ref() == Some(model) => {
        }
        None if result.receipt.reported_model.is_none() => {}
        _ => return Ok(None),
    }
    for (key, expected) in [
        ("output_format", json!(profile.image.format)),
        ("size", json!(profile.image.size)),
        ("quality", json!(profile.image.quality)),
        ("background", json!(profile.image.background)),
    ] {
        if raw.get(key).is_some_and(|value| value != &expected) {
            return Ok(None);
        }
    }
    // Differential cached-input tariffs are not part of this profile. Unknown
    // cache extensions cannot silently become ordinary-rate input tokens.
    if let Some(input) = raw.get("usage").and_then(|v| v.get("input_tokens_details")) {
        if input
            .get("cached_tokens")
            .is_some_and(|v| !v.is_null() && v.as_u64() != Some(0))
            || input
                .get("cached_tokens_details")
                .is_some_and(|v| !v.is_null())
        {
            return Ok(None);
        }
    }
    let UsageEvidence::Reported(usage) = &result.receipt.usage else {
        return Ok(None);
    };
    let ImageUsageEvidence::Reported(details) = &result.image_usage else {
        return Ok(None);
    };
    let Some(raw_usage) = raw.get("usage").filter(|v| v.is_object()) else {
        return Ok(None);
    };
    if ["cached_tokens", "cache_read_tokens"]
        .into_iter()
        .any(|key| {
            raw_usage
                .get(key)
                .is_some_and(|v| !v.is_null() && v.as_u64() != Some(0))
        })
        || raw_usage
            .get("output_tokens_details")
            .and_then(|v| v.get("reasoning_tokens"))
            .is_some_and(|v| !v.is_null() && v.as_u64() != Some(0))
    {
        return Ok(None);
    }
    if !counter(raw_usage, "input_tokens", usage.input_tokens)
        || !counter(raw_usage, "output_tokens", usage.output_tokens)
        || !counter(raw_usage, "total_tokens", usage.total_tokens)
    {
        return Ok(None);
    }
    for (group, text, image) in [
        (
            "input_tokens_details",
            details.input_text_tokens,
            details.input_image_tokens,
        ),
        (
            "output_tokens_details",
            details.output_text_tokens,
            details.output_image_tokens,
        ),
    ] {
        let value = raw_usage.get(group).unwrap_or(&Value::Null);
        if (!value.is_null() && !value.is_object())
            || !counter(value, "text_tokens", text)
            || !counter(value, "image_tokens", image)
        {
            return Ok(None);
        }
    }
    let (Some(input), Some(output)) = (usage.input_tokens, usage.output_tokens) else {
        return Ok(None);
    };
    let Some(total) = input.checked_add(output) else {
        return Ok(None);
    };
    if usage.total_tokens.is_some_and(|v| v != total)
        || usage.cache_read_tokens.is_some_and(|v| v != 0)
        || usage.reasoning_tokens.is_some_and(|v| v != 0)
    {
        return Ok(None);
    }
    let Some(mut terms) = side(
        input,
        details.input_text_tokens,
        details.input_image_tokens,
        profile.input_text_units_per_million,
        profile.input_image_units_per_million,
    ) else {
        return Ok(None);
    };
    let Some(output) = side(
        output,
        details.output_text_tokens,
        details.output_image_tokens,
        profile.output_text_units_per_million,
        profile.output_image_units_per_million,
    ) else {
        return Ok(None);
    };
    terms.extend(output);
    super::super::arithmetic::million_token_cost(&terms).map(Some)
}
