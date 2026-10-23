//! A creation tool uses the same signed review/journal as an edit. The model
//! supplies a title, never bytes, code, an executor or another actor's authority.
use crate::{
    agent_binary_documents::tool_approvals as shared,
    engine::{Engine, State, R},
    AppBinaryDocumentDto, Client, CoreError,
};
use miniapps::deckcraft::{self, agent_tools, registration};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use types::{ActionClass, AgentRequest, ToolProposal};

mod execution;
pub use execution::ApprovedAppCreation;
mod recovery;
mod staging;
#[cfg(test)]
mod tests;

pub const TOOL: &str = "create_deckcraft";
const SCHEMA: u8 = 3;

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: "Invalid presentation creation review.".into(),
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
    sha(&serde_json::to_vec(&(
        "zoen.native-app.create.v1",
        deckcraft::ENGINE_REVISION,
        sha(registration::EMPTY),
    ))
    .expect("fixed creation binding"))
}
pub fn model_name(space: &str) -> Option<String> {
    token(space).then(|| format!("{TOOL}__{}", &sha(space.as_bytes())[..40]))
}
pub fn is_model_name(name: &str) -> bool {
    name.strip_prefix("create_deckcraft__").is_some_and(|hash| {
        hash.len() == 40
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
pub fn definition(space: &str) -> Option<Value> {
    if !registration::available() {
        return None;
    }
    definition_unchecked(space)
}
fn definition_unchecked(space: &str) -> Option<Value> {
    Some(
        json!({"name":model_name(space)?,"description":"Create one shared editable presentation in this conversation. Call alone and wait for the owner's exact review before requesting its document tools. Does not replace an existing document.","parameters":{"type":"object","properties":{"title":{"type":"string","minLength":1,"maxLength":160}},"required":["title"],"additionalProperties":false}}),
    )
}
fn title(arguments: &Value) -> R<String> {
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
        || deckcraft::Document::parse(registration::EMPTY).is_err()
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
    if p.schema != SCHEMA
        || p.canonical_tool != TOOL
        || p.engine_revision != deckcraft::ENGINE_REVISION
        || p.worker_binding != executor_binding()
        || p.version != 1
        || p.sha256 != sha(registration::EMPTY)
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
    title(&args)?;
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
        || call["name"].as_str() != model_name(space).as_deref()
        || call["id"]
            .as_str()
            .is_none_or(|id| id.is_empty() || id.len() > 128)
        || call["arguments"] != args
        || Some(&ask["definition"]) != definition_unchecked(space).as_ref()
    {
        return Err(invalid());
    }
    let actor = ask["envelope"]["agent"].as_str().ok_or_else(invalid)?;
    let mutation = mutation(p, space, actor)?;
    if p.artifact
        != crate::app_binary_documents::creation_item_id(actor, &p.device, space, &mutation)?
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
        self.app_binary_creation_result(
            space,
            &title(&args.ok_or_else(invalid)?)?,
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
