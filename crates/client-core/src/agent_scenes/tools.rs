//! Bounded scene JSON is frozen in the existing signed proposal.
use super::{invalid, R};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
pub const REVISION: &str = "zoen.native-scene.excalidraw.v1";
pub const SCENE_LIMIT: usize = 32 * 1024;
pub const ARGUMENT_LIMIT: usize = SCENE_LIMIT * 2 + 2048;
pub(crate) fn digest(bytes: &[u8]) -> String {
    event_log::sha256_hex(bytes)
}
pub(crate) fn digest_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
pub(crate) fn scene_valid(scene: &str) -> R<()> {
    if scene.len() > SCENE_LIMIT {
        return Err(invalid());
    }
    miniapps::excalidraw::Scene::parse(scene)
        .map(|_| ())
        .map_err(|_| invalid())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReplaceScene {
    pub artifact_id: String,
    pub version: u32,
    pub sha256: String,
    pub resource_uri: String,
    pub scene_json: String,
}
#[derive(Clone, Debug)]
pub enum Command {
    Read { artifact: String },
    Replace(Box<ReplaceScene>),
}
impl Command {
    #[must_use]
    pub fn artifact(&self) -> &str {
        match self {
            Self::Read { artifact } => artifact,
            Self::Replace(c) => &c.artifact_id,
        }
    }
}
#[must_use]
pub fn recognizes(name: &str) -> bool {
    matches!(name, "read_scene" | "replace_scene")
}
#[must_use]
pub fn model_name(tool: &str, item: &str) -> Option<String> {
    recognizes(tool).then(|| format!("{tool}_{}", &digest(item.as_bytes())[..16]))
}
#[must_use]
pub fn executor_binding() -> String {
    digest(b"zoen.native-scene.excalidraw.v1\0read_scene\0replace_scene\0pinned-cas-v1")
}
pub fn parse(tool: &str, args: &Value) -> R<Command> {
    if serde_json::to_vec(args).map_err(|_| invalid())?.len() > ARGUMENT_LIMIT {
        return Err(invalid());
    }
    if tool == "read_scene" {
        if args.as_object().is_none_or(|o| o.len() != 1) {
            return Err(invalid());
        }
        let id = args["artifactId"]
            .as_str()
            .filter(|s| token(s))
            .ok_or_else(invalid)?;
        return Ok(Command::Read {
            artifact: id.into(),
        });
    }
    if tool != "replace_scene" {
        return Err(invalid());
    }
    let c: ReplaceScene = serde_json::from_value(args.clone()).map_err(|_| invalid())?;
    if !token(&c.artifact_id)
        || c.version == 0
        || !digest_valid(&c.sha256)
        || !c.resource_uri.starts_with("ui://zoen/package/")
        || c.resource_uri.len() > 256
    {
        return Err(invalid());
    }
    scene_valid(&c.scene_json)?;
    Ok(Command::Replace(Box::new(c)))
}
#[must_use]
pub fn tools() -> Vec<Value> {
    vec![
        json!({"name":"read_scene","description":"Read this owned drawing and its exact renderer pin and version. No other artifact is returned.","parameters":{"type":"object","additionalProperties":false,"required":["artifactId"],"properties":{"artifactId":{"type":"string"}}}}),
        json!({"name":"replace_scene","description":"Propose the complete Excalidraw scene just read, preserving every unrelated element, file and unknown field. Requires owner review; never changes the renderer.","parameters":{"type":"object","additionalProperties":false,"required":["artifactId","version","sha256","resourceUri","sceneJson"],"properties":{"artifactId":{"type":"string"},"version":{"type":"integer","minimum":1},"sha256":{"type":"string"},"resourceUri":{"type":"string"},"sceneJson":{"type":"string","maxLength":SCENE_LIMIT}}}}),
    ]
}
