//! Closed native Page tools. These schemas never turn a Page into an App.
use crate::{engine::State, pages::PageBlockDto};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use types::{ItemContent, ItemKind};

pub const REVISION: &str = "zoen.native-page.blocks.v1";
pub const ARGUMENT_LIMIT: usize = 32 * 1024;
pub const SNAPSHOT_LIMIT: usize = 64 * 1024;

pub fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
pub fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A binding to this native tool contract, not a worker binary or guest-supplied ABI.
pub fn executor_binding() -> String {
    digest(b"zoen.native-page.blocks.v1\0read_page\0replace_page_block\0exact-before-after-v1")
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReplaceBlock {
    pub artifact_id: String,
    pub version: u32,
    pub prefix_sha256: String,
    pub content_sha256: String,
    pub before: PageBlockDto,
    pub after: PageBlockDto,
}

#[derive(Clone, Debug)]
pub enum Command {
    Read { artifact: String },
    Replace(Box<ReplaceBlock>),
}
impl Command {
    pub fn artifact(&self) -> &str {
        match self {
            Self::Read { artifact } => artifact,
            Self::Replace(command) => &command.artifact_id,
        }
    }
}

pub fn recognizes(name: &str) -> bool {
    matches!(name, "read_page" | "replace_page_block")
}
pub fn model_name(tool: &str, item: &str) -> Option<String> {
    recognizes(tool).then(|| format!("{tool}_{}", &digest(item.as_bytes())[..16]))
}

/// Native projection can verify this prefix without opening a local editor or blob.
/// The materialized canonical shadow is independently checked before returning bytes.
pub(crate) fn prefix(state: &State, item: &str, version: u32) -> Result<String, String> {
    let it = state.items.get(item).ok_or("Page not found")?;
    if it.kind != ItemKind::Page || version == 0 || version as usize > it.versions.len() {
        return Err("Invalid Page prefix".into());
    }
    let mut rows = Vec::new();
    for v in &it.versions[..version as usize] {
        let ItemContent::Page(page) = &v.content else {
            return Err("The Page prefix changed kind".into());
        };
        if page.manifest.is_some() {
            return Err("A Plan is not an ordinary Page".into());
        }
        rows.push((v.number, &v.author, v.at_ms, &v.note, v.is_undo, page));
    }
    let bytes = serde_json::to_vec(&(REVISION, &it.space, &it.id, rows))
        .map_err(|_| "Invalid Page prefix")?;
    Ok(digest(&bytes))
}

pub fn parse(tool: &str, args: &Value) -> Result<Command, String> {
    if serde_json::to_vec(args)
        .map_err(|_| "Invalid Page arguments")?
        .len()
        > ARGUMENT_LIMIT
    {
        return Err("The Page command exceeds its capacity".into());
    }
    if tool == "read_page" {
        let object = args.as_object().ok_or("Invalid Page read")?;
        if object.len() != 1 {
            return Err("Invalid Page read fields".into());
        }
        let artifact = args["artifactId"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 128)
            .ok_or("Invalid Page locator")?;
        return Ok(Command::Read {
            artifact: artifact.into(),
        });
    }
    if tool != "replace_page_block" {
        return Err("Unsupported native Page tool".into());
    }
    let command: ReplaceBlock =
        serde_json::from_value(args.clone()).map_err(|_| "Invalid Page block replacement")?;
    // PageBlockDto is an existing public record. Round-trip equality also rejects
    // unknown nested fields instead of silently discarding reviewed bytes.
    if serde_json::to_value(&command).map_err(|_| "Invalid Page replacement")? != *args
        || command.artifact_id.is_empty()
        || command.artifact_id.len() > 128
        || command.version == 0
        || !valid_digest(&command.prefix_sha256)
        || !valid_digest(&command.content_sha256)
        || command.before.id.is_empty()
        || command.before.id != command.after.id
        || command.before == command.after
    {
        return Err("Invalid or unchanged Page block replacement".into());
    }
    let mut structural = command.after.clone();
    structural.text.clone_from(&command.before.text);
    if structural != command.before {
        return Err(
            "This Page tool replaces text while preserving the block structure and formatting"
                .into(),
        );
    }
    Ok(Command::Replace(Box::new(command)))
}

pub fn tools() -> Vec<Value> {
    let block = json!({"type":"object","additionalProperties":false,"required":["id","kind","level","indent","number","checked","lang","url","alt","text","spans"],"properties":{
        "id":{"type":"string"},"kind":{"type":"string"},"level":{"type":"integer"},"indent":{"type":"integer"},"number":{"type":"integer"},"checked":{"type":"boolean"},"lang":{"type":"string"},"url":{"type":"string"},"alt":{"type":"string"},"text":{"type":"string"},"spans":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["start","end","key","value"],"properties":{"start":{"type":"integer"},"end":{"type":"integer"},"key":{"type":"string"},"value":{"type":"string"}}}}
    }});
    vec![
        json!({"name":"read_page","description":"Read this published Page's exact blocks and observed head. Private drafts are never returned.","parameters":{"type":"object","additionalProperties":false,"required":["artifactId"],"properties":{"artifactId":{"type":"string"}}}}),
        json!({"name":"replace_page_block","description":"Propose exact before/after text for one published Page block just read. Preserve its id, structure and formatting spans. Requires owner review; cannot publish another draft.","parameters":{"type":"object","additionalProperties":false,"required":["artifactId","version","prefixSha256","contentSha256","before","after"],"properties":{"artifactId":{"type":"string"},"version":{"type":"integer","minimum":1},"prefixSha256":{"type":"string"},"contentSha256":{"type":"string"},"before":block,"after":block}}}),
    ]
}
