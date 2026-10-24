//! Schema2 is a closed native Page contract inside the existing signed envelope.
use super::*;
use crate::{
    agent_binary_documents::tool_approvals as shared,
    agent_page_tools as tools,
    engine::{ReqStatus, State},
    AgentRequestDto, AgentToolApprovalInput,
};
use serde_json::{json, Value};
use types::{ActionClass, AgentRequest, EventBody, ToolProposal};

pub struct ApprovedPageTurn {
    turn: AgentPageTurn,
    proposal: ToolProposal,
}
impl ApprovedPageTurn {
    pub fn turn(&self) -> &AgentPageTurn {
        &self.turn
    }
    pub fn proposal(&self) -> &ToolProposal {
        &self.proposal
    }
}

pub(crate) fn parsed(p: &ToolProposal) -> R<(Value, Option<Value>)> {
    if p.schema != 2
        || p.engine_revision != tools::REVISION
        || p.worker_binding != tools::executor_binding()
        || !tools::valid_digest(&p.source_sha256)
        || !tools::valid_digest(&p.task_sha256)
        || !tools::valid_digest(&p.sha256)
        || p.version == 0
        || p.step >= 16
        || p.arguments_json.len() > tools::ARGUMENT_LIMIT
        || p.continuation_json.len() > shared::CONTINUATION_LIMIT
        || p.gate_rule.is_empty()
        || p.gate_rule.len() > 256
    {
        return Err(invalid());
    }
    let args: Value = serde_json::from_str(&p.arguments_json).map_err(|_| invalid())?;
    if miniapps::deckcraft::agent_tools::canonical_json(&args).map_err(|_| invalid())?
        != p.arguments_json
    {
        return Err(invalid());
    }
    let command = tools::parse(&p.canonical_tool, &args).map_err(|_| invalid())?;
    if command.artifact() != p.artifact {
        return Err(invalid());
    }
    let action = match command {
        tools::Command::Read { .. } => None,
        tools::Command::Replace(command) => {
            if command.version != p.version || command.prefix_sha256 != p.sha256 {
                return Err(invalid());
            }
            Some(args.clone())
        }
    };
    let ask: Value = serde_json::from_str(&p.continuation_json).map_err(|_| invalid())?;
    let call = &ask["call"];
    if ask["policy_sha256"]
        .as_str()
        .is_none_or(|hash| !tools::valid_digest(hash))
        || ask["version"] != 1
        || ask["rule"].as_str() != Some(&p.gate_rule)
        || ask["steps"]
            .as_array()
            .is_none_or(|steps| steps.len() != p.step as usize)
        || ask["remaining"]
            .as_array()
            .is_none_or(|calls| calls.len() > 3)
        || ask["round"].as_u64().is_none_or(|round| round >= 4)
        || call["name"].as_str() != tools::model_name(&p.canonical_tool, &p.artifact).as_deref()
        || call["id"]
            .as_str()
            .is_none_or(|id| id.is_empty() || id.len() > 128)
        || call["arguments"] != args
        || ask["definition"]["name"] != call["name"]
    {
        return Err(invalid());
    }
    Ok((ask, action))
}

pub(crate) fn matches(state: &State, req: &AgentRequest, space: &str) -> bool {
    let Some(p) = req.tool_proposal.as_deref() else {
        return false;
    };
    let Ok((ask, _)) = parsed(p) else {
        return false;
    };
    let Some((source_sha, inbound_sha)) = shared::source_hash(state, space, &p.source, &p.serving)
    else {
        return false;
    };
    let Some(item) = state.items.get(&p.artifact) else {
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
        && item.space == space
        && item.current().number == p.version
        && tools::prefix_matches(state, &p.artifact, p.version, &p.sha256)
        && source_sha == p.source_sha256
        && req.tool_proposal_hash(space).as_deref() == Some(&req.content_hash)
        && p.task_sha256
            == tools::digest(
                &serde_json::to_vec(&("zoen.tool-task.v1", space, &req.agent, &p.source))
                    .unwrap_or_default(),
            )
        && ask["envelope"]
            == json!({"agent":req.agent,"device":p.device,"owner":p.owner,"space":space,"from":p.serving,"inbound_sha256":inbound_sha})
}

/// Pages use current Page membership plus a single signed approval. There is no
/// App principal or implicit app:<id> Trust::Act grant for a native document.
pub(crate) fn access(state: &State, p: &ToolProposal) -> bool {
    parsed(p).is_ok()
        && state.items.get(&p.artifact).is_some_and(|item| {
            item.kind == types::ItemKind::Page
                && state.writer(&item.space, &p.owner)
                && state.writer(&item.space, &p.serving)
        })
}

pub(crate) fn intent(p: &ToolProposal) -> R<String> {
    let (ask, _) = parsed(p)?;
    Ok(tools::digest(
        &serde_json::to_vec(&(
            "zoen.native-page.command.v1",
            &ask["envelope"]["agent"],
            &ask["envelope"]["space"],
            &p.owner,
            &p.device,
            &p.artifact,
            &p.source,
            &p.serving,
            &p.source_sha256,
            p.version,
            &p.sha256,
            &p.engine_revision,
            &p.canonical_tool,
            &p.arguments_json,
        ))
        .map_err(|_| invalid())?,
    ))
}

impl Client {
    pub fn stage_page_tool_approval(
        &self,
        turn: &AgentPageTurn,
        input: AgentToolApprovalInput,
    ) -> R<AgentRequestDto> {
        let mut e = self.lock();
        e.recheck_agent_page_turn(turn, input.canonical_tool != "read_page")?;
        let current = e.agent_page_snapshot(&turn.snapshot.artifact_id)?;
        if current != turn.snapshot || input.worker_binding != tools::executor_binding() {
            return Err(stale());
        }
        match tools::parse(&input.canonical_tool, &input.arguments).map_err(|_| invalid())? {
            tools::Command::Read { artifact } if artifact == current.artifact_id => {}
            tools::Command::Replace(command) if command.artifact_id == current.artifact_id => {
                e.agent_page_validate_write(&command)?
            }
            _ => return Err(stale()),
        }
        let ask: Value = serde_json::from_str(&input.continuation_json).map_err(|_| invalid())?;
        let p = ToolProposal {
            schema: 2,
            source: turn.source.clone(),
            serving: turn.serving.clone(),
            source_sha256: turn.source_sha256.clone(),
            device: turn.device.clone(),
            owner: turn.owner.clone(),
            task_sha256: tools::digest(
                &serde_json::to_vec(&(
                    "zoen.tool-task.v1",
                    &current.space_id,
                    &current.actor_id,
                    &turn.source,
                ))
                .map_err(|_| invalid())?,
            ),
            step: ask["steps"]
                .as_array()
                .map(|steps| steps.len() as u32)
                .ok_or_else(invalid)?,
            artifact: current.artifact_id.clone(),
            version: current.version,
            sha256: current.prefix_sha256,
            canonical_tool: input.canonical_tool,
            arguments_json: miniapps::deckcraft::agent_tools::canonical_json(&input.arguments)
                .map_err(|_| invalid())?,
            worker_binding: input.worker_binding,
            engine_revision: tools::REVISION.into(),
            continuation_json: input.continuation_json,
            expires_at_ms: crate::engine::now_ms() + 15 * 60 * 1000,
            gate_rule: input.gate_rule,
            review: None,
        };
        parsed(&p)?;
        if let Some(r) =
            e.state
                .requests
                .values()
                .filter(|r| {
                    r.req.agent == current.actor_id
                        && r.req.tool_proposal.as_ref().is_some_and(|old| {
                            old.task_sha256 == p.task_sha256 && old.step == p.step
                        })
                })
                .max_by_key(|r| {
                    r.req
                        .tool_proposal
                        .as_ref()
                        .and_then(|p| p.review.as_ref())
                        .map_or(0, |r| r.generation)
                })
        {
            let mut same = p.clone();
            same.expires_at_ms = r.req.tool_proposal.as_ref().unwrap().expires_at_ms;
            same.review = r.req.tool_proposal.as_ref().unwrap().review.clone();
            if r.req.tool_proposal.as_deref() != Some(&same) {
                return Err(stale());
            }
            return Ok(e.request_dto(r));
        }
        let operation = shared::operation(&p.canonical_tool);
        let mut detail = shared::review_detail(&p, &current.title);
        if p.canonical_tool == "replace_page_block" {
            let args: Value = serde_json::from_str(&p.arguments_json).map_err(|_| invalid())?;
            let index = current
                .blocks
                .iter()
                .position(|block| Some(block.id.as_str()) == args["before"]["id"].as_str())
                .ok_or_else(stale)?;
            detail = format!(
                "{} {}\n{}",
                crate::i18n::t("Bloco", "Block"),
                index + 1,
                detail
            );
        }
        let mut req = AgentRequest {
            id: types::new_id("rq"),
            agent: current.actor_id.clone(),
            title: operation,
            detail,
            audience: "This conversation".into(),
            action: ActionClass::Reversible,
            content_hash: String::new(),
            item: None,
            line: None,
            proposal: None,
            tool_proposal: Some(Box::new(p)),
        };
        req.content_hash = req
            .tool_proposal_hash(&current.space_id)
            .ok_or_else(invalid)?;
        if !matches(&e.state, &req, &current.space_id) || !e.tool_non_blocked(&req) {
            return Err(stale());
        }
        let id = req.id.clone();
        e.append(
            &current.space_id,
            &current.actor_id,
            EventBody::RequestOpened { request: req },
        )?;
        Ok(e.request_dto(&e.state.requests[&id]))
    }

    pub fn approved_page_tool_turn(&self, request: &str) -> R<ApprovedPageTurn> {
        let e = self.lock();
        let r = e.state.requests.get(request).ok_or_else(stale)?;
        let (_, p) = e.tool_resolution(request, &r.req.content_hash)?;
        parsed(p)?;
        if e.me_id()? != r.req.agent || r.status != ReqStatus::Approved {
            return Err(stale());
        }
        let (device, owner, space, source_sha256) = e.agent_binary_authority(
            &p.artifact,
            &p.source,
            &p.serving,
            p.canonical_tool != "read_page",
        )?;
        if device != p.device
            || owner != p.owner
            || space != r.space
            || source_sha256 != p.source_sha256
        {
            return Err(stale());
        }
        let mut snapshot = e.agent_page_snapshot_at(&p.artifact, p.version)?;
        if !tools::prefix_matches(&e.state, &p.artifact, p.version, &p.sha256) {
            return Err(stale());
        }
        // Retain the original signed legacy binding, never rewrite the proposal.
        snapshot.prefix_sha256.clone_from(&p.sha256);
        let turn = AgentPageTurn {
            snapshot,
            device,
            owner,
            source: p.source.clone(),
            serving: p.serving.clone(),
            source_sha256,
            approval: Some((request.into(), r.req.content_hash.clone())),
        };
        Ok(ApprovedPageTurn {
            turn,
            proposal: p.clone(),
        })
    }
}
