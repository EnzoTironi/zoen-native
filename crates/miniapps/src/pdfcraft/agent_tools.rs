//! Artifact-scoped PDF intents. No file paths, arbitrary commands or authority.
use super::action::Operation;
use crate::deckcraft::agent_tools::{canonical_json, Definition, ParsedCommand};
use serde_json::{json, Value};
#[must_use]
pub fn recognizes(tool: &str) -> bool {
    matches!(tool, "read_pdfcraft" | "pdfcraft_edit")
}
#[must_use]
pub fn tools() -> Vec<Definition> {
    let artifact = json!({"type":"string","minLength":1,"maxLength":128});
    let page = json!({"type":"integer","minimum":0,"maximum":31});
    let text = json!({"type":"string","maxLength":8192});
    let rect = json!({"type":"array","items":{"type":"number","minimum":0,"maximum":2048},"minItems":4,"maxItems":4});
    let closed = |properties: Value, required: &[&str]| json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
    let action = json!({"oneOf":[
        closed(json!({"op":{"const":"rotate_pages"},"pages":{"type":"array","items":page,"minItems":1,"maxItems":32,"uniqueItems":true},"degrees":{"enum":[-90,90,180]}}), &["op","pages","degrees"]),
        closed(json!({"op":{"const":"replace_text"},"page":page,"block":{"type":"integer","minimum":0,"maximum":8191},"expected":text,"text":text}), &["op","page","block","expected","text"]),
        closed(json!({"op":{"const":"add_text"},"page":page,"rect":rect,"text":text,"size":{"type":"number","minimum":6,"maximum":96}}), &["op","page","rect","text","size"]),
        closed(json!({"op":{"const":"annotate"},"page":page,"rect":rect,"text":text}), &["op","page","rect","text"]),
        closed(json!({"op":{"const":"insert_page"},"at":{"type":"integer","minimum":0,"maximum":32}}), &["op","at"])
    ]});
    vec![
        Definition { name:"read_pdfcraft".into(), description:"Read the exact accepted PDF head/hash, page geometry and a bounded page-text block window. Zero-based page and offset; nextOffset continues the same accepted version. Full paragraphs must be complete before replacing them. Isolated executor document capacity is512KiB; the App separately accepts up to8MiB.".into(), parameters:closed(json!({"artifactId":artifact,"page":page,"offset":{"type":"integer","minimum":0,"maximum":8192}}), &["artifactId"]) },
        Definition { name:"pdfcraft_edit".into(), description:"Propose one reversible PDF change against its exact observed version/hash. The owner reviews the exact intent; the pinned engine validates current pages, paragraph text, geometry and representability. No paths, URLs, external assets or PDF JavaScript.".into(), parameters:closed(json!({"artifactId":artifact,"version":{"type":"integer","minimum":1,"maximum":4_294_967_294_u32},"sha256":{"type":"string","pattern":"^[a-f0-9]{64}$"},"action":action}), &["artifactId","version","sha256","action"]) }
    ]
}

fn text(value: &str) -> bool {
    value.len() <= 8192
        && !value
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
}
fn rect(value: &[f64; 4]) -> bool {
    value
        .iter()
        .all(|n| n.is_finite() && (0.0..=2048.0).contains(n))
        && value[0] < value[2]
        && value[1] < value[3]
}
fn bounded(action: &Operation) -> bool {
    match action {
        Operation::RotatePages { pages, degrees } => {
            !pages.is_empty()
                && pages.len() <= 32
                && pages.iter().all(|p| *p < 32)
                && pages
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    == pages.len()
                && matches!(degrees, -90 | 90 | 180)
        }
        Operation::InsertPage { at } => *at <= 32,
        Operation::ReplaceText {
            page,
            block,
            expected,
            text: value,
        } => *page < 32 && *block < 8192 && text(expected) && text(value),
        Operation::Annotate {
            page,
            rect: area,
            text: value,
        } => *page < 32 && rect(area) && text(value),
        Operation::AddText {
            page,
            rect: area,
            text: value,
            size,
        } => {
            *page < 32
                && rect(area)
                && !value.is_empty()
                && text(value)
                && size.is_finite()
                && (6.0..=96.0).contains(size)
        }
    }
}
/// # Errors
/// Refuses unknown fields/operations, missing exact heads and structural bounds.
/// The actual pinned engine independently validates complete PDF semantics.
pub fn command(tool: &str, value: &Value) -> Result<ParsedCommand, String> {
    if !recognizes(tool)
        || canonical_json(value)
            .map_err(|_| "Invalid PDF arguments")?
            .len()
            > 32 * 1024
    {
        return Err("Unsupported or oversized PDF intent".into());
    }
    let args = value.as_object().ok_or("PDF arguments must be an object")?;
    let read = tool == "read_pdfcraft";
    let fields: &[&str] = if read {
        &["artifactId", "page", "offset"]
    } else {
        &["artifactId", "version", "sha256", "action"]
    };
    if args.keys().any(|k| !fields.contains(&k.as_str())) || (!read && args.len() != fields.len()) {
        return Err("Unexpected or missing PDF argument".into());
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
    if read {
        for (key, maximum) in [("page", 31), ("offset", 8192)] {
            if args
                .get(key)
                .is_some_and(|n| n.as_u64().is_none_or(|n| n > maximum))
            {
                return Err("Invalid PDF text window".into());
            }
        }
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
    let action: Operation =
        serde_json::from_value(args["action"].clone()).map_err(|_| "Unsupported PDF intent")?;
    if !bounded(&action) {
        return Err("PDF intent exceeds structural bounds".into());
    }
    let mut action = serde_json::to_value(action).map_err(|_| "Invalid PDF intent")?;
    action.sort_all_objects();
    Ok((Some(version), Some(hash.into()), Some(action)))
}
