//! A held tool is an existing signed Request, never an independent permission store.
use super::*;
use crate::{
    dto::*,
    engine::{ReqStatus, State},
};
use miniapps::binary::{self, agent_tools};
#[cfg(test)]
use miniapps::deckcraft;
use serde_json::{json, Value};
use types::{
    ActionClass, AgentRequest, Capability, EventBody, RequestResolution, ToolDecision,
    ToolProposal, TrustLevel,
};

pub(crate) const CONTINUATION_LIMIT: usize = 128 * 1024;
const REQUEST_LIFETIME_MS: i64 = 15 * 60 * 1000;
fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn stale() -> CoreError {
    CoreError::Stale {
        reason: "The reviewed action, source or access changed. Request a new proposal.".into(),
    }
}
fn malformed() -> CoreError {
    CoreError::Invalid {
        reason: "Unsupported or invalid reviewed binary App action.".into(),
    }
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Rust-only input from the trusted host after a Gate Ask. Locators are checked against
/// the actual signed native actor/source; neither UI nor model can call this port.
pub struct AgentToolApprovalInput {
    pub canonical_tool: String,
    pub arguments: Value,
    pub worker_binding: String,
    pub continuation_json: String,
    pub gate_rule: String,
}
/// Not serializable or FFI-exported. Its embedded turn rechecks the canonical approval
/// and exact command intent at every read/result/save boundary.
pub struct ApprovedToolTurn {
    turn: AgentBinaryTurn,
    proposal: ToolProposal,
}
impl ApprovedToolTurn {
    pub fn turn(&self) -> &AgentBinaryTurn {
        &self.turn
    }
    pub fn proposal(&self) -> &ToolProposal {
        &self.proposal
    }
}
#[derive(Clone)]
pub(super) struct ApprovalBinding {
    request: String,
    content_hash: String,
}

fn parsed(p: &ToolProposal) -> R<(Value, Option<Value>)> {
    if p.schema == 5 {
        return crate::agent_sites::parsed(p);
    }
    if p.schema == crate::image_tasks::SCHEMA {
        return crate::image_tasks::parsed(p);
    }
    if p.schema == 3 {
        return crate::agent_app_creation::parsed(p);
    }
    if p.schema == 2 {
        return crate::agent_pages::approvals::parsed(p);
    }
    let format = agent_tools::format(&p.canonical_tool).ok_or_else(malformed)?;
    if p.schema != 1
        || p.engine_revision != format.revision()
        || !digest(&p.worker_binding)
        || !digest(&p.source_sha256)
        || !digest(&p.task_sha256)
        || !digest(&p.sha256)
        || p.version == 0
        || p.step >= 16
        || p.arguments_json.len() > 32 * 1024
        || p.continuation_json.len() > CONTINUATION_LIMIT
        || p.gate_rule.is_empty()
        || p.gate_rule.len() > 256
    {
        return Err(malformed());
    }
    let args: Value = serde_json::from_str(&p.arguments_json).map_err(|_| malformed())?;
    if agent_tools::canonical_json(&args).map_err(|_| malformed())? != p.arguments_json
        || args["artifactId"].as_str() != Some(p.artifact.as_str())
    {
        return Err(malformed());
    }
    let (version, head, command) =
        agent_tools::command(&p.canonical_tool, &args).map_err(|_| malformed())?;
    if version.is_some_and(|v| v != p.version) || head.as_ref().is_some_and(|h| h != &p.sha256) {
        return Err(malformed());
    }
    let ask: Value = serde_json::from_str(&p.continuation_json).map_err(|_| malformed())?;
    let call = &ask["call"];
    if ask["policy_sha256"].as_str().is_none_or(|fp| !digest(fp))
        || ask["version"] != 1
        || ask["rule"].as_str() != Some(p.gate_rule.as_str())
        || ask["steps"]
            .as_array()
            .is_none_or(|steps| steps.len() != p.step as usize)
        || ask["remaining"]
            .as_array()
            .is_none_or(|calls| calls.len() > 3)
        || ask["round"].as_u64().is_none_or(|round| round >= 4)
        || call["name"].as_str()
            != agent_tools::model_name(&p.canonical_tool, &p.artifact).as_deref()
        || call["id"]
            .as_str()
            .is_none_or(|id| id.is_empty() || id.len() > 128)
        || call["arguments"] != args
        || ask["definition"]["name"] != call["name"]
    {
        return Err(malformed());
    }
    Ok((ask, command))
}
pub(crate) fn source_hash(
    state: &State,
    space: &str,
    source: &str,
    serving: &str,
) -> Option<(String, String)> {
    let entry = state
        .spaces
        .get(space)?
        .entries
        .iter()
        .find(|entry| entry.id() == source)?;
    let EntryBody::Message {
        text,
        attaches,
        reply,
        marks,
    } = &entry.body
    else {
        return None;
    };
    if entry.author != serving || marks.deleted || text.trim().is_empty() {
        return None;
    }
    let bytes = serde_json::to_vec(&(entry.id(), &entry.author, text, attaches, reply)).ok()?;
    Some((sha(&bytes), sha(text.as_bytes())))
}

pub(crate) fn command_intent(p: &ToolProposal, action: Option<&Value>) -> R<String> {
    Ok(sha(&serde_json::to_vec(&(
        &p.artifact,
        &p.source,
        &p.serving,
        &p.source_sha256,
        p.version,
        &p.sha256,
        &p.engine_revision,
        &p.canonical_tool,
        action,
    ))
    .map_err(|_| malformed())?))
}

pub(crate) fn mutation_id(p: &ToolProposal, intent: &str) -> String {
    let prefix = match agent_tools::format(&p.canonical_tool) {
        Some(binary::Format::Gridcraft) => "gridcmd",
        Some(binary::Format::Wordcraft) => "wordcmd",
        _ => "deckcmd",
    };
    format!("{prefix}_{intent}")
}
pub(crate) fn operation(tool: &str) -> String {
    use crate::i18n::t;
    match tool {
        "create_site" => t("Criar site", "Create Site"),
        "read_site" => t("Ler site", "Read Site"),
        "replace_site_source" => t("Atualizar site", "Update Site"),
        "generate_image" => t("Criar imagem", "Create image"),
        "read_page" => t("Ler página", "Read Page"),
        "create_gridcraft" => t("Criar planilha", "Create spreadsheet"),
        "create_wordcraft" => t("Criar documento", "Create document"),
        "read_wordcraft" => t("Ler documento", "Read document"),
        "wordcraft_text_replace" => t("Substituir texto", "Replace text"),
        "create_deckcraft" => t("Criar apresentação", "Create presentation"),
        "replace_page_block" => t("Substituir bloco da página", "Replace Page block"),
        "read_gridcraft" => t("Ler planilha", "Read spreadsheet"),
        "gridcraft_cell_set" => t("Alterar célula", "Set cell"),
        "read_deckcraft" => t("Ler apresentação", "Read presentation"),
        "deckcraft_text_set" => t("Substituir texto", "Replace shape text"),
        "deckcraft_slide_notes" => t("Atualizar notas do slide", "Update slide notes"),
        "deckcraft_shape_insert" => t("Inserir uma forma", "Insert a shape"),
        "deckcraft_slide_new" => t("Inserir um slide", "Insert a slide"),
        _ => t("Ação incompatível", "Unsupported action"),
    }
}
pub(crate) fn review_detail(p: &ToolProposal, title: &str) -> String {
    let args: Value = serde_json::from_str(&p.arguments_json).unwrap_or(Value::Null);
    let heading = format!("{} · v{}", title, p.version);
    match p.canonical_tool.as_str() {
        "read_page" => heading,
        "replace_page_block" => format!(
            "{}\n{}\n{}\n{}\n{}",
            heading,
            crate::i18n::t("Antes", "Before"),
            args["before"]["text"].as_str().unwrap_or_default(),
            crate::i18n::t("Depois", "After"),
            args["after"]["text"].as_str().unwrap_or_default()
        ),
        "read_deckcraft" | "read_gridcraft" | "read_wordcraft" => heading,
        "gridcraft_cell_set" => format!(
            "{}\n{}!{}\n{}",
            heading,
            args["sheet"].as_str().unwrap_or_default(),
            args["cell"].as_str().unwrap_or_default(),
            args["input"].as_str().unwrap_or_default()
        ),
        "deckcraft_slide_new" => format!(
            "{}\n{}\n{}",
            heading,
            args["title"].as_str().unwrap_or_default(),
            args["body"].as_str().unwrap_or_default()
        ),
        _ => format!("{}\n{}", heading, args["text"].as_str().unwrap_or_default()),
    }
}

#[cfg(test)]
pub(crate) mod tests;

mod authorization;
mod projection;
mod recovery;
pub(crate) mod renewal;
mod staging;
