use rig_core::completion::{CompletionRequest, ToolDefinition as RigTool};
use rig_core::message::{AssistantContent, AssistantMessage, CallId, Message, ToolName};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

use crate::{GatewayError, InputMessage, ModelRequest, Operation, OutputBlock};

pub(crate) fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.is_ascii()
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
}

pub(crate) fn tool_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

pub(crate) fn bounded_json<T: serde::Serialize>(
    value: &T,
    limit: usize,
) -> Result<Vec<u8>, GatewayError> {
    struct Writer {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
                return Err(std::io::Error::other("request byte limit"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Writer {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| GatewayError::InvalidRequest)?;
    Ok(writer.bytes)
}

pub(crate) fn json_shape(value: &Value) -> bool {
    let mut pending = vec![(value, 0usize)];
    let mut nodes = 0;
    while let Some((value, depth)) = pending.pop() {
        nodes += 1;
        if depth > 64 || nodes > 100_000 {
            return false;
        }
        match value {
            Value::Array(values) => {
                if values.len() > 100_000usize.saturating_sub(nodes + pending.len()) {
                    return false;
                }
                pending.extend(values.iter().map(|v| (v, depth + 1)))
            }
            Value::Object(values) => {
                if values.len() > 100_000usize.saturating_sub(nodes + pending.len()) {
                    return false;
                }
                pending.extend(values.values().map(|v| (v, depth + 1)))
            }
            _ => {}
        }
    }
    true
}

fn canonicalize(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, canonicalize(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize).collect()),
        value => value,
    }
}

pub(crate) fn digest(
    request: &ModelRequest,
    profile: &Value,
    limit: usize,
) -> Result<String, GatewayError> {
    // Bound before creating/cloning a JSON tree. Normalize object key order so
    // equivalent tool schemas/arguments produce the same admission binding.
    let bytes = bounded_json(&(request, profile), limit)?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| GatewayError::InvalidRequest)?;
    let canonical = bounded_json(&canonicalize(value), limit)?;
    let mut hash = Sha256::new();
    hash.update(b"zoen-model-request-v1\0");
    hash.update(canonical);
    Ok(hex::encode(hash.finalize()))
}

pub(crate) fn wire_digest(bytes: &[u8], limit: usize) -> Result<String, GatewayError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| GatewayError::InvalidRequest)?;
    let canonical = bounded_json(&canonicalize(value), limit)?;
    Ok(hex::encode(Sha256::digest(canonical)))
}

pub(crate) fn prepare(
    request: &ModelRequest,
    max_tokens: u64,
    byte_limit: usize,
) -> Result<CompletionRequest, GatewayError> {
    if request.operation != Operation::ChatCompletion {
        return Err(GatewayError::UnsupportedOperation);
    }
    let context = &request.context;
    if [
        &context.attempt_id,
        &context.run_id,
        &context.owner,
        &context.agent,
        &context.device,
        &context.space,
        &context.authority_version,
        &context.definition_version,
        &context.price_version,
    ]
    .iter()
    .any(|value| !identifier(value))
        || context.owner == context.agent
        || request.messages.is_empty()
        || request.messages.len() > 256
        || request.tools.len() > 64
        || request.max_output_tokens == 0
        || request.max_output_tokens > max_tokens
    {
        return Err(GatewayError::InvalidRequest);
    }
    let mut names = BTreeSet::new();
    for tool in &request.tools {
        if !tool_name(&tool.name)
            || !names.insert(&tool.name)
            || !tool.parameters.is_object()
            || !json_shape(&tool.parameters)
        {
            return Err(GatewayError::InvalidRequest);
        }
    }
    let mut pending = BTreeMap::new();
    let mut used_ids = BTreeSet::new();
    for message in &request.messages {
        if !pending.is_empty() && !matches!(message, InputMessage::ToolResult { .. }) {
            return Err(GatewayError::InvalidRequest);
        }
        match message {
            InputMessage::Assistant { blocks } => {
                if blocks.is_empty() || blocks.len() > 64 {
                    return Err(GatewayError::InvalidRequest);
                }
                for block in blocks {
                    if let OutputBlock::ToolCall {
                        call_id,
                        name,
                        arguments,
                    } = block
                    {
                        if !identifier(call_id)
                            || !tool_name(name)
                            || !arguments.is_object()
                            || !json_shape(arguments)
                            || !used_ids.insert(call_id)
                        {
                            return Err(GatewayError::InvalidRequest);
                        }
                        pending.insert(call_id, name);
                    }
                }
            }
            InputMessage::ToolResult { call_id, name, .. }
                if pending.remove(call_id) != Some(name) =>
            {
                return Err(GatewayError::InvalidRequest);
            }
            _ => {}
        }
    }
    if !pending.is_empty() {
        return Err(GatewayError::InvalidRequest);
    }
    bounded_json(request, byte_limit)?;
    let mut messages = request
        .messages
        .iter()
        .map(|message| match message {
            InputMessage::System { text } => Message::system(text),
            InputMessage::User { text } => Message::user(text),
            InputMessage::Assistant { blocks } => Message::Assistant(AssistantMessage::new(
                blocks
                    .iter()
                    .map(|block| match block {
                        OutputBlock::Text { text } => AssistantContent::text(text),
                        OutputBlock::ToolCall {
                            call_id,
                            name,
                            arguments,
                        } => AssistantContent::tool_call(
                            call_id,
                            ToolName::new(name).expect("validated name"),
                            arguments.clone(),
                        ),
                    })
                    .collect(),
            )),
            InputMessage::ToolResult {
                call_id,
                name,
                text,
            } => Message::tool_result(
                CallId::from_wire(call_id),
                ToolName::new(name).expect("validated name"),
                text,
            ),
        })
        .collect::<Vec<_>>();
    let last = messages.pop().expect("nonempty messages");
    let tools = request
        .tools
        .iter()
        .map(|tool| {
            RigTool::new(
                ToolName::new(&tool.name).expect("validated name"),
                &tool.description,
                tool.parameters.clone(),
            )
        })
        .collect();
    Ok(CompletionRequest::new(last)
        .messages(messages)
        .tools(tools)
        .max_tokens(request.max_output_tokens))
}
