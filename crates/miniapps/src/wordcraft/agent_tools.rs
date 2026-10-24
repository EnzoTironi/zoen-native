//! Closed plain-text Word operations; scope and authority remain native.
use crate::deckcraft::agent_tools::{Definition, ParsedCommand};
use serde_json::{json, Value};
pub fn tools() -> Vec<Definition> {
    let artifact = json!({"type":"string","minLength":1,"maxLength":128});
    vec![
        Definition { name:"read_wordcraft".into(), description:"Read the exact accepted document head/hash and plain text. Supports the managed plain-text DOCX profile, up to 512 KiB.".into(), parameters:json!({"type":"object","properties":{"artifactId":artifact},"required":["artifactId"],"additionalProperties":false}) },
        Definition { name:"wordcraft_text_replace".into(), description:"Replace the plain text of an existing document using its exact observed version/hash. Requires the owner's exact review; no paths, URLs, rich formatting or arbitrary commands.".into(), parameters:json!({"type":"object","properties":{"artifactId":artifact,"version":{"type":"integer","minimum":1,"maximum":4294967294_u32},"sha256":{"type":"string","pattern":"^[a-f0-9]{64}$"},"text":{"type":"string","maxLength":16384}},"required":["artifactId","version","sha256","text"],"additionalProperties":false}) },
    ]
}
pub fn recognizes(tool: &str) -> bool {
    matches!(tool, "read_wordcraft" | "wordcraft_text_replace")
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
    let fields: &[&str] = if tool == "read_wordcraft" {
        &["artifactId"]
    } else {
        &["artifactId", "version", "sha256", "text"]
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
    let text = args["text"]
        .as_str()
        .filter(|s| s.len() <= 16 * 1024 && crate::wordcraft::valid_text(s))
        .ok_or("Unsupported document text")?;
    let mut action = json!({"command":"text.replace","params":{"text":text}});
    action.sort_all_objects();
    Ok((Some(version), Some(hash.into()), Some(action)))
}
