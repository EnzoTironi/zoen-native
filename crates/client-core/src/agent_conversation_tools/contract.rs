//! Review binds the exact query and source; no artifact or standing grant exists.
use super::*;
use crate::{
    agent_binary_documents::tool_approvals as shared, agent_page_tools as hash, engine::State,
};
use serde_json::json;
use types::{ActionClass, AgentRequest, ToolProposal};

/// Independent captured calls share source authority, not approval identity.
pub(super) fn task(ask: &Value, space: &str, actor: &str, source: &str) -> R<String> {
    let bytes = if ask["version"] == 1 {
        serde_json::to_vec(&("zoen.tool-task.v1", space, actor, source))
    } else if ask["version"] == 2 {
        let native = ask["native"].as_object().ok_or_else(invalid)?;
        if native.len() != 3
            || !["original", "capture"].into_iter().all(|key| {
                native
                    .get(key)
                    .and_then(Value::as_str)
                    .is_some_and(hash::valid_digest)
            })
            || native
                .get("call")
                .and_then(Value::as_u64)
                .is_none_or(|index| index >= 3)
            || ask["steps"].as_array().is_none_or(|v| !v.is_empty())
            || ask["remaining"].as_array().is_none_or(|v| !v.is_empty())
            || ask["round"] != 0
        {
            return Err(invalid());
        }
        serde_json::to_vec(&(
            "zoen.native-tool-task.v1",
            space,
            actor,
            source,
            &ask["native"],
        ))
    } else {
        return Err(invalid());
    };
    Ok(hash::digest(&bytes.map_err(|_| invalid())?))
}

pub(crate) fn parsed(p: &ToolProposal) -> R<(Value, Option<Value>)> {
    if p.schema != SCHEMA
        || p.canonical_tool != TOOL
        || p.engine_revision != REVISION
        || p.worker_binding != executor_binding()
        || !hash::valid_digest(&p.source_sha256)
        || !hash::valid_digest(&p.task_sha256)
        || p.sha256 != p.source_sha256
        || !p.artifact.is_empty()
        || p.version != 0
        || p.step >= 16
        || p.arguments_json.len() > 32 * 1024
        || p.continuation_json.len() > shared::CONTINUATION_LIMIT
        || p.gate_rule.is_empty()
        || p.gate_rule.len() > 256
    {
        return Err(invalid());
    }
    let args: Value = serde_json::from_str(&p.arguments_json).map_err(|_| invalid())?;
    parse(&args)?;
    if miniapps::binary::agent_tools::canonical_json(&args).map_err(|_| invalid())?
        != p.arguments_json
    {
        return Err(invalid());
    }
    let ask: Value = serde_json::from_str(&p.continuation_json).map_err(|_| invalid())?;
    let call = &ask["call"];
    if ask["policy_sha256"]
        .as_str()
        .is_none_or(|v| !hash::valid_digest(v))
        || !matches!(ask["version"].as_u64(), Some(1 | 2))
        || ask["rule"].as_str() != Some(&p.gate_rule)
        || ask["steps"]
            .as_array()
            .is_none_or(|steps| steps.len() != p.step as usize)
        || ask["remaining"]
            .as_array()
            .is_none_or(|calls| calls.len() > 3)
        || ask["round"].as_u64().is_none_or(|round| round >= 4)
        || call["name"] != TOOL
        || call["id"]
            .as_str()
            .is_none_or(|id| id.is_empty() || id.len() > 128)
        || call["arguments"] != args
        || ask["definition"]["name"] != TOOL
    {
        return Err(invalid());
    }
    Ok((ask, None))
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
    let Ok(task) = task(&ask, space, &req.agent, &p.source) else {
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
        && source == p.source_sha256
        && req.tool_proposal_hash(space).as_deref() == Some(&req.content_hash)
        && p.task_sha256 == task
        && ask["envelope"]
            == json!({"agent":req.agent,"device":p.device,"owner":p.owner,"space":space,"from":p.serving,"inbound_sha256":inbound})
}

pub(crate) fn access(state: &State, p: &ToolProposal) -> bool {
    let Ok((ask, _)) = parsed(p) else {
        return false;
    };
    ask["envelope"]["space"]
        .as_str()
        .is_some_and(|space| state.writer(space, &p.owner) && state.writer(space, &p.serving))
}
