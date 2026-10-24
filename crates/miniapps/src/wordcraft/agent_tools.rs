//! Closed rich-document operations; the canonical host owns scope/review/CAS.
use crate::deckcraft::agent_tools::{Definition, ParsedCommand};
use serde_json::{json, Value};
use zoen_word_profile::action::Action;
#[path = "agent_tools/schema.rs"]
mod schema;
pub fn tools() -> Vec<Definition> {
    schema::tools()
}
pub fn recognizes(tool: &str) -> bool {
    matches!(
        tool,
        "read_wordcraft"
            | "wordcraft_text_replace"
            | "wordcraft_edit_range"
            | "wordcraft_format_range"
    )
}
pub fn command(tool: &str, value: &Value) -> Result<ParsedCommand, String> {
    if !recognizes(tool) {
        return Err("Unsupported document operation".into());
    }
    if crate::deckcraft::agent_tools::canonical_json(value)
        .map_err(|_| "Invalid document arguments")?
        .len()
        > 32 * 1024
    {
        return Err("Document arguments exceed the signed review limit".into());
    }
    let args = value
        .as_object()
        .ok_or("Document arguments must be an object")?;
    let fields: &[&str] = match tool {
        "read_wordcraft" => &["artifactId"],
        "wordcraft_edit_range" => &["artifactId", "version", "sha256", "range", "text"],
        "wordcraft_format_range" => &["artifactId", "version", "sha256", "range", "format"],
        _ => &["artifactId", "version", "sha256", "text"],
    };
    if args.len() != fields.len() || args.keys().any(|k| !fields.contains(&k.as_str())) {
        return Err("Unexpected or missing document argument".into());
    }
    args.get("artifactId")
        .and_then(Value::as_str)
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        })
        .ok_or("An exact artifactId is required")?;
    if tool == "read_wordcraft" {
        return Ok((None, None, None));
    }
    let version = args["version"]
        .as_u64()
        .filter(|n| *n > 0 && *n < u64::from(u32::MAX))
        .and_then(|n| u32::try_from(n).ok())
        .ok_or("An exact observed version is required")?;
    let hash = args["sha256"]
        .as_str()
        .filter(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or("An exact observed hash is required")?;
    if tool != "wordcraft_format_range" && args["text"].as_str().is_none_or(|s| s.len() > 16 * 1024)
    {
        return Err("Unsupported document text".into());
    }
    let (id, params) = match tool {
        "wordcraft_edit_range" => (
            "text.edit",
            json!({"range":args["range"],"text":args["text"]}),
        ),
        "wordcraft_format_range" => (
            "range.format",
            json!({"range":args["range"],"format":args["format"]}),
        ),
        _ => ("text.replace", json!({"text":args["text"]})),
    };
    let action: Action = serde_json::from_value(json!({"command":id,"params":params}))
        .map_err(|_| "Invalid document operation")?;
    action
        .validate()
        .map_err(|_| "Invalid document operation")?;
    let mut action = serde_json::to_value(action).map_err(|_| "Invalid document operation")?;
    action.sort_all_objects();
    Ok((Some(version), Some(hash.into()), Some(action)))
}
