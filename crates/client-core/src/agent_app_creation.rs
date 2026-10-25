//! A creation tool uses the same signed review/journal as an edit. The model
//! supplies a title, never bytes, code, an executor or another actor's authority.
use crate::{
    agent_binary_documents::tool_approvals as shared,
    engine::{Engine, State, R},
    AppBinaryDocumentDto, Client, CoreError,
};
use miniapps::binary::{agent_tools, Format};
#[cfg(test)]
use miniapps::deckcraft::registration;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use types::{ActionClass, AgentRequest, ToolProposal};

mod execution;
pub use execution::ApprovedAppCreation;
#[cfg(test)]
mod gridcraft_tests;
#[cfg(test)]
mod photocraft_tests;
mod recovery;
mod staging;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod wordcraft_tests;

pub const TOOL: &str = "create_deckcraft";
pub const GRID_TOOL: &str = "create_gridcraft";
pub const WORD_TOOL: &str = "create_wordcraft";
pub const PHOTO_TOOL: &str = "create_photocraft";
pub fn creation_format(tool: &str) -> Option<Format> {
    match tool {
        TOOL => Some(Format::Deckcraft),
        GRID_TOOL => Some(Format::Gridcraft),
        WORD_TOOL => Some(Format::Wordcraft),
        PHOTO_TOOL => Some(Format::Photocraft),
        _ => None,
    }
}
const SCHEMA: u8 = 3;

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: "Invalid binary App creation review.".into(),
    }
}
fn stale() -> CoreError {
    CoreError::Stale {
        reason: "The reviewed creation, source or access changed.".into(),
    }
}
fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

pub fn executor_binding() -> String {
    binding_for(Format::Deckcraft)
}
pub fn executor_binding_for(tool: &str) -> Option<String> {
    creation_format(tool).map(binding_for)
}
fn binding_for(format: Format) -> String {
    let binding = Value::Array(vec![
        Value::String("zoen.native-app.create.v1".into()),
        Value::String(format.revision().into()),
        Value::String(sha(format.seed())),
    ]);
    sha(binding.to_string().as_bytes())
}
// Each concrete App has one closed creation operation. Scope remains
// in the signed envelope and future-item intent, not a credential-shaped alias.
pub fn model_name(space: &str) -> Option<String> {
    model_name_for(TOOL, space)
}
pub fn model_name_for(tool: &str, space: &str) -> Option<String> {
    (token(space) && creation_format(tool).is_some()).then(|| tool.into())
}
pub fn is_model_name(name: &str) -> bool {
    creation_format(name).is_some()
}
pub fn definition(space: &str) -> Option<Value> {
    definition_for(TOOL, space)
}
pub fn definition_for(tool: &str, space: &str) -> Option<Value> {
    if !creation_format(tool)?.available() {
        return None;
    }
    definition_unchecked_for(tool, space)
}
#[cfg(test)]
fn definition_unchecked(space: &str) -> Option<Value> {
    definition_unchecked_for(TOOL, space)
}
fn definition_unchecked_for(tool: &str, space: &str) -> Option<Value> {
    let description=match creation_format(tool)? {
        Format::Photocraft=>"Create one shared editable raster image in this conversation. Call alone and wait for the owner's exact review before requesting its image tools. Does not replace an existing image.",
        Format::Wordcraft=>"Create one shared editable plain-text document in this conversation. Call alone and wait for the owner's exact review before requesting its document tools. Does not replace an existing document.",
        Format::Deckcraft=>"Create one shared editable presentation in this conversation. Call alone and wait for the owner's exact review before requesting its document tools. Does not replace an existing document.",
        Format::Gridcraft=>"Create one shared editable spreadsheet in this conversation. Call alone and wait for the owner's exact review before requesting its document tools. Does not replace an existing document.",
    };
    Some(
        json!({"name":model_name_for(tool,space)?,"description":description,"parameters":{"type":"object","properties":{"title":{"type":"string","minLength":1,"maxLength":160}},"required":["title"],"additionalProperties":false}}),
    )
}
fn title(arguments: &Value, format: Format) -> R<String> {
    let object = arguments
        .as_object()
        .filter(|object| object.len() == 1 && object.contains_key("title"))
        .ok_or_else(invalid)?;
    let raw = object["title"].as_str().ok_or_else(invalid)?;
    // Native storage remains usable independently of UI asset configuration.
    // Discovery separately requires the genuine configured editor to be available.
    if raw.trim().is_empty()
        || raw.trim() != raw
        || raw.chars().count() > 160
        || raw.chars().any(char::is_control)
        || format.metadata(format.seed(), raw).is_err()
    {
        return Err(invalid());
    }
    Ok(raw.into())
}
fn ask_space(ask: &Value) -> R<&str> {
    ask["envelope"]["space"]
        .as_str()
        .filter(|space| token(space))
        .ok_or_else(invalid)
}

/// Existing ToolProposal fields pin the not-yet-created native seed and exact
/// arguments. Schema3 is a closed family inside the unchanged signed envelope.
pub(crate) fn parsed(p: &ToolProposal) -> R<(Value, Option<Value>)> {
    let format = creation_format(&p.canonical_tool).ok_or_else(invalid)?;
    if p.schema != SCHEMA
        || p.engine_revision != format.revision()
        || p.worker_binding != binding_for(format)
        || p.version != 1
        || p.sha256 != sha(format.seed())
        || !token(&p.artifact)
        || p.step >= 16
        || p.arguments_json.len() > 2048
        || p.continuation_json.len() > shared::CONTINUATION_LIMIT
        || p.gate_rule.is_empty()
        || p.gate_rule.len() > 256
    {
        return Err(invalid());
    }
    let args: Value = serde_json::from_str(&p.arguments_json).map_err(|_| invalid())?;
    title(&args, format)?;
    if agent_tools::canonical_json(&args).map_err(|_| invalid())? != p.arguments_json {
        return Err(invalid());
    }
    let ask: Value = serde_json::from_str(&p.continuation_json).map_err(|_| invalid())?;
    let space = ask_space(&ask)?;
    let call = &ask["call"];
    if ask["version"] != 1
        || ask["rule"].as_str() != Some(&p.gate_rule)
        || ask["policy_sha256"].as_str().is_none_or(|value| {
            value.len() != 64
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        || ask["steps"]
            .as_array()
            .is_none_or(|steps| steps.len() != p.step as usize)
        || ask["remaining"]
            .as_array()
            .is_none_or(|calls| !calls.is_empty())
        || ask["round"].as_u64().is_none_or(|round| round >= 4)
        || call["name"].as_str() != model_name_for(&p.canonical_tool, space).as_deref()
        || call["id"]
            .as_str()
            .is_none_or(|id| id.is_empty() || id.len() > 128)
        || call["arguments"] != args
        || Some(&ask["definition"]) != definition_unchecked_for(&p.canonical_tool, space).as_ref()
    {
        return Err(invalid());
    }
    let actor = ask["envelope"]["agent"].as_str().ok_or_else(invalid)?;
    let mutation = mutation(p, space, actor)?;
    if p.artifact
        != crate::app_binary_documents::creation_item_id_for(
            format, actor, &p.device, space, &mutation,
        )?
    {
        return Err(invalid());
    }
    Ok((ask, Some(args)))
}
fn mutation(p: &ToolProposal, space: &str, actor: &str) -> R<String> {
    Ok(format!(
        "create_{}",
        sha(&serde_json::to_vec(&(
            "zoen.native-app.create.v1",
            actor,
            &p.device,
            &p.owner,
            space,
            &p.source,
            &p.serving,
            &p.source_sha256,
            &p.task_sha256,
            p.step,
            &p.arguments_json,
            &p.worker_binding,
            &p.engine_revision,
            &p.sha256
        ))
        .map_err(|_| invalid())?)
    ))
}
pub(crate) fn matches(state: &State, req: &AgentRequest, space: &str) -> bool {
    let Some(p) = req.tool_proposal.as_deref() else {
        return false;
    };
    let Ok((ask, _)) = parsed(p) else {
        return false;
    };
    let Some((source, inbound)) = shared::source_hash(state, space, &p.source, &p.serving) else {
        return false;
    };
    req.proposal.is_none()
        && req.item.is_none()
        && req.line.is_none()
        && req.action == ActionClass::Reversible
        && state.writer(space, &req.agent)
        && state.writer(space, &p.owner)
        && state.writer(space, &p.serving)
        && p.serving != req.agent
        && !state.items.contains_key(&p.artifact)
        && source == p.source_sha256
        && p.task_sha256
            == sha(
                &serde_json::to_vec(&("zoen.tool-task.v1", space, &req.agent, &p.source))
                    .unwrap_or_default(),
            )
        && req.tool_proposal_hash(space).as_deref() == Some(&req.content_hash)
        && ask["envelope"]
            == json!({"agent":req.agent,"device":p.device,"owner":p.owner,"space":space,"from":p.serving,"inbound_sha256":inbound})
}
pub(crate) fn access(state: &State, p: &ToolProposal) -> bool {
    parsed(p).is_ok_and(|(ask, _)| {
        ask_space(&ask)
            .is_ok_and(|space| state.writer(space, &p.owner) && state.writer(space, &p.serving))
    })
}
impl Engine {
    pub(crate) fn creation_proposal_authority(
        &self,
        p: &ToolProposal,
    ) -> R<(String, String, String, String)> {
        let (ask, _) = parsed(p)?;
        self.creation_authority(ask_space(&ask)?, &p.source, &p.serving)
    }
    pub(crate) fn creation_authority(
        &self,
        space: &str,
        source: &str,
        serving: &str,
    ) -> R<(String, String, String, String)> {
        let binding = self.agent_source_authority(space, source, serving, true)?;
        let me = self.me_id()?;
        if !self.state.writer(space, &me)
            || !self.state.writer(space, &binding.1)
            || matches!(
                self.decide(&me, space, &ActionClass::Reversible, 0),
                grants::Decision::Block(_)
            )
        {
            return Err(stale());
        }
        Ok(binding)
    }
    pub(crate) fn creation_result(&self, p: &ToolProposal) -> R<Option<AppBinaryDocumentDto>> {
        let (ask, args) = parsed(p)?;
        let space = ask_space(&ask)?;
        let actor = ask["envelope"]["agent"].as_str().ok_or_else(invalid)?;
        let (device, owner, _, source) = self.creation_authority(space, &p.source, &p.serving)?;
        if self.me_id()? != actor
            || device != p.device
            || owner != p.owner
            || source != p.source_sha256
        {
            return Err(stale());
        }
        let format = creation_format(&p.canonical_tool).ok_or_else(invalid)?;
        self.app_binary_creation_result_for(
            format,
            space,
            &title(&args.ok_or_else(invalid)?, format)?,
            &p.sha256,
            &mutation(p, space, actor)?,
        )
    }
}
impl Client {
    pub fn can_create_agent_app(&self, space: &str, source: &str, serving: &str) -> bool {
        self.lock()
            .creation_authority(space, source, serving)
            .is_ok()
    }
}
