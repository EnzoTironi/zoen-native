//! Canonical text-only tool transcripts accepted by OpenAI-compatible providers.
//! Preflight and dispatch share these bytes; never repair a request after admission.
use crate::{validation, GatewayError};
use serde_json::Value;

pub(crate) fn encode(bytes: &[u8], maximum: usize) -> Result<Vec<u8>, GatewayError> {
    if bytes.len() > maximum {
        return Err(GatewayError::InvalidRequest);
    }
    let mut value: Value =
        serde_json::from_slice(bytes).map_err(|_| GatewayError::InvalidRequest)?;
    let messages = value["messages"]
        .as_array_mut()
        .ok_or(GatewayError::InvalidRequest)?;
    if !messages.iter().any(|message| {
        message["role"] == "assistant"
            && message["tool_calls"]
                .as_array()
                .is_some_and(|calls| !calls.is_empty())
    }) {
        return Ok(bytes.to_vec());
    }
    for message in messages {
        if let Some(parts) = message["content"].as_array() {
            let mut text = String::new();
            for part in parts {
                if part["type"] != "text" {
                    return Err(GatewayError::UnsupportedOperation);
                }
                text.push_str(part["text"].as_str().ok_or(GatewayError::InvalidRequest)?);
            }
            message["content"] = text.into();
        } else if message["role"] == "assistant" && message["content"].is_null() {
            // Rig omits content for tool-only assistant messages; some compatible
            // providers require this field even when tool_calls carries the turn.
            message["content"] = "".into();
        }
    }
    validation::bounded_json(&value, maximum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preserves_order_and_payload_and_is_idempotent() -> Result<(), GatewayError> {
        let original = json!({"messages":[
            {"role":"system","content":"instructions"},
            {"role":"user","content":[{"type":"text","text":"Olá "},{"type":"text","text":"🌥"}]},
            {"role":"assistant","tool_calls":[{"id":"call","type":"function","function":{"name":"lookup","arguments":"{\"q\":\"x\"}"}}]},
            {"role":"tool","tool_call_id":"call","content":"verified result"}
        ]});
        let raw = validation::bounded_json(&original, 4096)?;
        let encoded = encode(&raw, 4096)?;
        let parsed: Value =
            serde_json::from_slice(&encoded).map_err(|_| GatewayError::InvalidRequest)?;
        assert_eq!(parsed["messages"][1]["content"], "Olá 🌥");
        assert_eq!(parsed["messages"][2]["content"], "");
        assert_eq!(
            parsed["messages"][2]["tool_calls"],
            original["messages"][2]["tool_calls"]
        );
        assert_eq!(parsed["messages"][3], original["messages"][3]);
        assert_eq!(encode(&encoded, 4096)?, encoded);
        assert!(encode(&raw, raw.len() - 1).is_err());
        Ok(())
    }

    #[test]
    fn leaves_ordinary_chat_bytes_unchanged() -> Result<(), GatewayError> {
        let raw =
            br#"{ "messages": [{"role":"user","content":[{"type":"text","text":"hello"}]}] }"#;
        assert_eq!(encode(raw, 4096)?, raw);
        Ok(())
    }

    #[test]
    fn never_silently_drops_non_text_parts_from_a_tool_transcript() -> Result<(), GatewayError> {
        let raw = validation::bounded_json(
            &json!({"messages":[
                {"role":"user","content":[{"type":"image_url","image_url":{"url":"https://example.invalid/image"}}]},
                {"role":"assistant","tool_calls":[{"id":"call"}]}
            ]}),
            4096,
        )?;
        assert_eq!(encode(&raw, 4096), Err(GatewayError::UnsupportedOperation));
        Ok(())
    }
}
