//! Parameterless native canvas transforms; Core supplies scope and review authority.
use crate::deckcraft::agent_tools::{Definition, ParsedCommand};
use serde_json::{json, Value};
const ACTIONS: [&str; 5] = [
    "flip_horizontal",
    "flip_vertical",
    "rotate_clockwise",
    "rotate_counterclockwise",
    "rotate180",
];
pub fn tools() -> Vec<Definition> {
    let artifact = json!({"type":"string","minLength":1,"maxLength":128});
    vec![
        Definition { name:"read_photocraft".into(), description:"Read the accepted image dimensions and exact version/hash. Managed RGB8 raster profile only; isolated executor capacity is 512 KiB. Does not expose raw pixels or grant access.".into(), parameters:json!({"type":"object","properties":{"artifactId":artifact},"required":["artifactId"],"additionalProperties":false}) },
        Definition { name:"photocraft_transform".into(), description:"Propose one reversible image flip or rotation against the exact observed version/hash. Requires the owner's exact review. No arbitrary commands, paths, URLs or external assets.".into(), parameters:json!({"type":"object","properties":{"artifactId":artifact,"version":{"type":"integer","minimum":1,"maximum":4294967294_u32},"sha256":{"type":"string","pattern":"^[a-f0-9]{64}$"},"action":{"type":"string","enum":ACTIONS}},"required":["artifactId","version","sha256","action"],"additionalProperties":false}) },
    ]
}
pub fn recognizes(tool: &str) -> bool {
    matches!(tool, "read_photocraft" | "photocraft_transform")
}
pub fn command(tool: &str, value: &Value) -> Result<ParsedCommand, String> {
    if !recognizes(tool) {
        return Err("Unsupported image operation".into());
    }
    let args = value
        .as_object()
        .ok_or("Image arguments must be an object")?;
    let fields: &[&str] = if tool == "read_photocraft" {
        &["artifactId"]
    } else {
        &["artifactId", "version", "sha256", "action"]
    };
    if args.len() != fields.len() || args.keys().any(|k| !fields.contains(&k.as_str())) {
        return Err("Unexpected or missing image argument".into());
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
    if tool == "read_photocraft" {
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
    let action = args["action"]
        .as_str()
        .filter(|s| ACTIONS.contains(s))
        .ok_or("Unsupported image transform")?;
    Ok((Some(version), Some(hash.into()), Some(json!(action))))
}
