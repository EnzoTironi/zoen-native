//! Shared, closed `DeckCraft` command schema. Models supply data, never authority.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[derive(Clone, Debug)]
pub struct Definition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[must_use]
pub fn tools() -> Vec<Definition> {
    let base = json!({"artifactId":{"type":"string","minLength":1,"maxLength":128},"version":{"type":"integer","minimum":1,"maximum":4_294_967_294_u32},"sha256":{"type":"string","pattern":"^[a-f0-9]{64}$"}});
    let mut out = vec![Definition{name:"read_deckcraft".into(),description:"Read this presentation's accepted version/hash, slide and shape IDs before editing. Isolated executor supports at most 512 KiB.".into(),parameters:json!({"type":"object","properties":{"artifactId":{"type":"string","minLength":1,"maxLength":128}},"required":["artifactId"],"additionalProperties":false})}];
    for (name, description, properties, required) in [
        (
            "deckcraft_text_set",
            "Replace one shape's text",
            json!({"slideId":{"type":"integer","minimum":0},"id":{"type":"integer","minimum":0},"text":{"type":"string","maxLength":8192}}),
            vec!["slideId", "id", "text"],
        ),
        (
            "deckcraft_slide_notes",
            "Replace one slide's notes",
            json!({"slideId":{"type":"integer","minimum":0},"text":{"type":"string","maxLength":8192}}),
            vec!["slideId", "text"],
        ),
        (
            "deckcraft_shape_insert",
            "Insert one reviewed shape",
            json!({"slideId":{"type":"integer","minimum":0},"preset":{"enum":["rect","ellipse","roundRect"]},"rect":{"type":"array","minItems":4,"maxItems":4,"items":{"type":"number"}},"text":{"type":"string","maxLength":8192}}),
            vec!["slideId", "preset", "rect", "text"],
        ),
        (
            "deckcraft_slide_new",
            "Insert one slide after an existing slide",
            json!({"afterSlideId":{"type":"integer","minimum":0},"layout":{"enum":["blank","title","titleAndContent"]},"title":{"type":"string","maxLength":8192},"body":{"type":"string","maxLength":8192}}),
            vec!["afterSlideId", "layout", "title", "body"],
        ),
    ] {
        let mut properties = properties.as_object().cloned().unwrap_or_default();
        properties.extend(base.as_object().cloned().unwrap_or_default());
        let required: Vec<&str> = [vec!["artifactId", "version", "sha256"], required].concat();
        out.push(Definition{name:name.into(),description:format!("{description}; exact observed version/hash required. 512 KiB executor capacity; four explicit reversible commands only."),parameters:json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})});
    }
    out
}
#[must_use]
pub fn recognizes(tool: &str) -> bool {
    tools().iter().any(|t| t.name == tool)
}
/// Compact stable alias; the server mapping and required artifactId still
/// retain/check the entire canonical ID, including its 72-character form.
#[must_use]
pub fn model_name(tool: &str, item: &str) -> Option<String> {
    let digest = hash(item.as_bytes());
    let name = format!("{tool}__{}", &digest[..40]);
    (recognizes(tool)
        && !item.is_empty()
        && item.len() <= 128
        && item
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        && name.len() <= 64)
        .then_some(name)
}
#[must_use]
pub fn is_model_name(name: &str) -> bool {
    name.rsplit_once("__").is_some_and(|(tool, digest)| {
        recognizes(tool)
            && digest.len() == 40
            && digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
/// Compact JSON with recursively sorted object keys, independent of `serde_json`'s
/// map feature. Arrays, scalar values and the caller's frozen input are preserved.
/// # Errors
/// Returns an error if the recursively ordered JSON value cannot be serialized.
pub fn canonical_json(value: &Value) -> serde_json::Result<String> {
    let mut sorted = value.clone();
    sorted.sort_all_objects();
    serde_json::to_string(&sorted)
}

pub type ParsedCommand = (Option<u32>, Option<String>, Option<Value>);

/// # Errors
/// Refuses unknown tools, malformed or unexpected arguments, unsupported commands or missing exact artifact/version/hash bindings.
pub fn command(tool: &str, args: &Value) -> Result<ParsedCommand, String> {
    let definition = tools()
        .into_iter()
        .find(|t| t.name == tool)
        .ok_or("Unknown presentation tool")?;
    let args = args
        .as_object()
        .ok_or("Presentation arguments must be an object")?;
    let allowed = definition.parameters["properties"]
        .as_object()
        .ok_or("Invalid presentation schema")?;
    if args.keys().any(|key| !allowed.contains_key(key)) {
        return Err("Unexpected presentation argument".into());
    }
    if args
        .get("artifactId")
        .and_then(Value::as_str)
        .is_none_or(|id| {
            id.is_empty()
                || id.len() > 128
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        })
    {
        return Err("An exact canonical artifactId is required".into());
    }
    if tool == "read_deckcraft" {
        return Ok((None, None, None));
    }
    let version = args
        .get("version")
        .and_then(Value::as_u64)
        .filter(|n| *n >= 1 && *n < u64::from(u32::MAX))
        .and_then(|version| u32::try_from(version).ok())
        .ok_or("An observed version is required")?;
    let sha = args
        .get("sha256")
        .and_then(Value::as_str)
        .filter(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or("An observed hash is required")?
        .to_owned();
    validate_fields(args, allowed)?;

    let mut params = args.clone();
    params.remove("artifactId");
    params.remove("version");
    params.remove("sha256");
    if tool == "deckcraft_shape_insert" {
        let rect = params
            .get_mut("rect")
            .and_then(Value::as_array_mut)
            .filter(|rect| rect.len() == 4)
            .ok_or("A four-number rectangle is required")?;
        // Match the pinned bridge's f64 DTO. JSON integer coordinates and float
        // coordinates are the same geometric command and must hash/verify alike.
        for coordinate in rect {
            let number = coordinate
                .as_f64()
                .filter(|n| n.is_finite())
                .ok_or("Invalid rectangle coordinate")?;
            *coordinate = json!(number);
        }
    }
    let command = match tool {
        "deckcraft_text_set" => "text.set",
        "deckcraft_slide_notes" => "slide.notes",
        "deckcraft_shape_insert" => "shape.insert",
        "deckcraft_slide_new" => "slide.new",
        _ => return Err("Unknown presentation tool".into()),
    };
    // These exact action bytes also enter the native/host mutation intent hash.
    // Never let a dependency's map feature select a different command identity.
    let mut action = json!({"command":command,"params":params});
    action.sort_all_objects();
    Ok((Some(version), Some(sha), Some(action)))
}

fn validate_fields(
    args: &serde_json::Map<String, Value>,
    allowed: &serde_json::Map<String, Value>,
) -> Result<(), String> {
    for field in ["slideId", "id", "afterSlideId"] {
        if allowed.contains_key(field)
            && args
                .get(field)
                .and_then(Value::as_u64)
                .is_none_or(|id| id > u64::from(u32::MAX))
        {
            return Err("A native slide/shape ID is required".into());
        }
    }
    for field in ["text", "title", "body"] {
        if allowed.contains_key(field)
            && args
                .get(field)
                .and_then(Value::as_str)
                .is_none_or(|text| text.len() > 8192)
        {
            return Err("Presentation text must be a string up to 8192 UTF-8 bytes".into());
        }
    }
    for (field, values) in [
        ("preset", &["rect", "ellipse", "roundRect"][..]),
        ("layout", &["blank", "title", "titleAndContent"][..]),
    ] {
        if allowed.contains_key(field)
            && args
                .get(field)
                .and_then(Value::as_str)
                .is_none_or(|value| !values.contains(&value))
        {
            return Err("Unsupported presentation command variant".into());
        }
    }
    Ok(())
}
