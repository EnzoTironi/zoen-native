use rig_core::completion::{CompletionResponse, FinishReason};
use rig_core::message::AssistantContent;
use serde_json::Value;
use std::collections::BTreeSet;

use crate::{ModelRequest, OutputBlock, OutputFailure};

/// Validate the exact wire shape before accepting Rig's deliberately lenient
/// tool parsing. In particular, no partial/double-decoded/null arguments and no
/// generated replacement IDs may become an executable Zoen proposal.
fn strict_calls(value: &Value, request: &ModelRequest) -> Result<(), OutputFailure> {
    let choices = value
        .get("choices")
        .and_then(Value::as_array)
        .ok_or(OutputFailure::MalformedResponse)?;
    if choices.len() != 1 {
        return Err(OutputFailure::UnsupportedOutput);
    }
    let message = choices[0]
        .get("message")
        .ok_or(OutputFailure::MalformedResponse)?;
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return Err(OutputFailure::MalformedResponse);
    }
    let Some(calls) = message.get("tool_calls").filter(|value| !value.is_null()) else {
        return Ok(());
    };
    let calls = calls.as_array().ok_or(OutputFailure::InvalidToolCall)?;
    if calls.len() > 64 {
        return Err(OutputFailure::InvalidToolCall);
    }
    let mut ids = BTreeSet::new();
    for call in calls {
        let id = call
            .get("id")
            .and_then(Value::as_str)
            .ok_or(OutputFailure::InvalidToolCall)?;
        let function = call.get("function").ok_or(OutputFailure::InvalidToolCall)?;
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .ok_or(OutputFailure::InvalidToolCall)?;
        let arguments = function
            .get("arguments")
            .and_then(Value::as_str)
            .ok_or(OutputFailure::InvalidToolCall)?;
        if call.get("type").and_then(Value::as_str) != Some("function")
            || !crate::validation::identifier(id)
            || !ids.insert(id)
            || !request.tools.iter().any(|tool| tool.name == name)
            || arguments.len() > 65_536
            || !serde_json::from_str::<Value>(arguments).is_ok_and(|value| value.is_object())
        {
            return Err(OutputFailure::InvalidToolCall);
        }
    }
    Ok(())
}

pub(crate) fn output(
    response: CompletionResponse,
    raw: &Value,
    request: &ModelRequest,
) -> Result<Vec<OutputBlock>, OutputFailure> {
    strict_calls(raw, request)?;
    if response.error.is_some() || response.aborted.is_some() {
        return Err(OutputFailure::ProviderRejected);
    }
    if !matches!(
        response.finish_reason(),
        Some(FinishReason::Stop | FinishReason::ToolCalls)
    ) {
        return Err(OutputFailure::IncompleteOutput);
    }
    if response.choice.is_empty() || response.choice.len() > 64 {
        return Err(OutputFailure::UnsupportedOutput);
    }
    response
        .choice
        .into_iter()
        .map(|part| match part {
            AssistantContent::Text(text) => Ok(OutputBlock::Text { text: text.text }),
            AssistantContent::ToolCall(call)
                if call.function.invalid_arguments.is_none() && call.id.provider().is_some() =>
            {
                Ok(OutputBlock::ToolCall {
                    call_id: call.id.to_string(),
                    name: call.function.name.to_string(),
                    arguments: Value::Object(call.function.arguments),
                })
            }
            AssistantContent::ToolCall(_) => Err(OutputFailure::InvalidToolCall),
            _ => Err(OutputFailure::UnsupportedOutput),
        })
        .collect()
}
