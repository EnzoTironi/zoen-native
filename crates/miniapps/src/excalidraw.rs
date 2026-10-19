//! Shared validation for canonical encrypted Excalidraw documents.
//!
//! Validation never rewrites the input. Hosts retain the original UTF-8 bytes for
//! hashing and encrypted storage; passive unknown fields survive unchanged.
use base64::Engine as _;
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[cfg(test)]
mod tests;

pub const MAX_SCENE_BYTES: usize = 16 * 1024 * 1024;
pub const RESOURCE: &str = "ui://zoen/excalidraw.html";
pub const MIME: &str = "application/vnd.excalidraw+json";
pub const EMPTY: &str =
    r#"{"type":"excalidraw","version":2,"source":"zoen","elements":[],"appState":{},"files":{}}"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidScene;

impl std::fmt::Display for InvalidScene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid or oversized drawing document")
    }
}
impl std::error::Error for InvalidScene {}

/// A validated scene. Private construction keeps compact metadata safe to read.
#[derive(Debug)]
pub struct Scene(Value);
impl Scene {
    pub fn parse(text: &str) -> Result<Self, InvalidScene> {
        validate_scene(text).map(Self)
    }

    pub fn metadata(&self) -> String {
        let scene = &self.0;
        let live = scene["elements"]
            .as_array()
            .expect("validated")
            .iter()
            .filter(|e| e["isDeleted"] != true)
            .collect::<Vec<_>>();
        json!({"elementCount":live.len(),"textCount":live.iter().filter(|e| e["type"]=="text").count()})
        .to_string()
    }
}

fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn bounded_json(v: &Value, depth: usize, nodes: &mut usize) -> Result<(), InvalidScene> {
    *nodes += 1;
    if depth > 64 || *nodes > 500_000 {
        return Err(InvalidScene);
    }
    match v {
        Value::Number(n)
            if n.as_f64()
                .is_none_or(|n| !n.is_finite() || n.abs() > 9_007_199_254_740_991.0) =>
        {
            return Err(InvalidScene)
        }
        Value::Array(a) => {
            for child in a {
                bounded_json(child, depth + 1, nodes)?;
            }
        }
        Value::Object(o) => {
            for (key, child) in o {
                if key.contains('\0') {
                    return Err(InvalidScene);
                }
                bounded_json(child, depth + 1, nodes)?;
            }
        }
        Value::String(s) if s.contains('\0') => return Err(InvalidScene),
        _ => {}
    }
    Ok(())
}

/// Unknown passive JSON fields survive byte-for-byte; executable embeds and remote assets do not.
fn validate_scene(text: &str) -> Result<Value, InvalidScene> {
    if text.is_empty() || text.len() > MAX_SCENE_BYTES {
        return Err(InvalidScene);
    }
    let scene: Value = serde_json::from_str(text).map_err(|_| InvalidScene)?;
    bounded_json(&scene, 0, &mut 0)?;
    if scene["type"] != "excalidraw" || scene["version"] != 2 || !scene["appState"].is_object() {
        return Err(InvalidScene);
    }
    let elements = scene["elements"]
        .as_array()
        .filter(|e| e.len() <= 10_000)
        .ok_or(InvalidScene)?;
    let files = scene["files"]
        .as_object()
        .filter(|f| f.len() <= 256)
        .ok_or(InvalidScene)?;
    for (id, file) in files {
        if !valid_id(id) || file["id"].as_str() != Some(id) {
            return Err(InvalidScene);
        }
        let mime = file["mimeType"].as_str().ok_or(InvalidScene)?;
        if !matches!(
            mime,
            "image/png" | "image/jpeg" | "image/webp" | "image/gif"
        ) {
            return Err(InvalidScene);
        }
        let url = file["dataURL"].as_str().ok_or(InvalidScene)?;
        let data = url
            .strip_prefix(&format!("data:{mime};base64,"))
            .ok_or(InvalidScene)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|_| InvalidScene)?;
        let signature = match mime {
            "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "image/jpeg" => bytes.starts_with(b"\xff\xd8\xff"),
            "image/webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
            _ => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        };
        if !signature || bytes.len() > MAX_SCENE_BYTES {
            return Err(InvalidScene);
        }
    }
    let mut ids = BTreeSet::new();
    for e in elements {
        let id = e["id"]
            .as_str()
            .filter(|id| valid_id(id))
            .ok_or(InvalidScene)?;
        if !ids.insert(id) || !e.is_object() {
            return Err(InvalidScene);
        }
        let kind = e["type"].as_str().ok_or(InvalidScene)?;
        if !matches!(
            kind,
            "rectangle"
                | "diamond"
                | "ellipse"
                | "line"
                | "arrow"
                | "freedraw"
                | "text"
                | "image"
                | "frame"
                | "magicframe"
        ) {
            return Err(InvalidScene);
        }
        for field in ["x", "y", "width", "height"] {
            let n = e[field]
                .as_f64()
                .filter(|n| n.is_finite() && n.abs() <= 1_000_000_000.0)
                .ok_or(InvalidScene)?;
            if matches!(field, "width" | "height") && n < 0.0 {
                return Err(InvalidScene);
            }
        }
        if kind == "image"
            && !e["fileId"]
                .as_str()
                .is_some_and(|id| files.contains_key(id))
        {
            return Err(InvalidScene);
        }
        if e.get("text")
            .is_some_and(|v| !v.is_string() || v.as_str().is_some_and(|s| s.len() > 65_536))
        {
            return Err(InvalidScene);
        }
        if e.get("isDeleted").is_some_and(|v| !v.is_boolean()) {
            return Err(InvalidScene);
        }
        if e.get("points").is_some_and(|v| {
            !v.is_array()
                || v.as_array().is_some_and(|a| {
                    a.len() > 100_000
                        || a.iter().any(|p| {
                            p.as_array()
                                .is_none_or(|p| p.len() != 2 || p.iter().any(|n| !n.is_number()))
                        })
                })
        }) {
            return Err(InvalidScene);
        }
    }
    for e in elements {
        for field in ["containerId", "frameId"] {
            if let Some(id) = e.get(field).filter(|v| !v.is_null()) {
                if !id.as_str().is_some_and(|id| ids.contains(id)) {
                    return Err(InvalidScene);
                }
            }
        }
        for field in ["startBinding", "endBinding"] {
            if let Some(binding) = e.get(field).filter(|v| !v.is_null()) {
                if !binding["elementId"]
                    .as_str()
                    .is_some_and(|id| ids.contains(id))
                {
                    return Err(InvalidScene);
                }
            }
        }
        if let Some(bound) = e.get("boundElements").filter(|v| !v.is_null()) {
            let bound = bound.as_array().ok_or(InvalidScene)?;
            if bound
                .iter()
                .any(|b| !b["id"].as_str().is_some_and(|id| ids.contains(id)))
            {
                return Err(InvalidScene);
            }
        }
    }
    Ok(scene)
}
