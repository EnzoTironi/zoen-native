use super::*;
use crate::{AgentRequestDto, AgentToolApprovalInput};
use types::TrustLevel;

pub struct ApprovedSceneTurn {
    turn: AgentSceneTurn,
    proposal: ToolProposal,
}
impl ApprovedSceneTurn {
    #[must_use]
    pub fn turn(&self) -> &AgentSceneTurn {
        &self.turn
    }
    #[must_use]
    pub fn proposal(&self) -> &ToolProposal {
        &self.proposal
    }
}
pub(crate) fn parsed(p: &ToolProposal) -> R<(Value, Option<Value>)> {
    if p.schema != SCHEMA
        || p.engine_revision != tools::REVISION
        || p.worker_binding != tools::executor_binding()
        || !tools::digest_valid(&p.source_sha256)
        || !tools::digest_valid(&p.task_sha256)
        || !tools::digest_valid(&p.sha256)
        || !tools::token(&p.device)
        || !tools::token(&p.owner)
        || !tools::token(&p.source)
        || !tools::token(&p.serving)
        || !tools::token(&p.artifact)
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
    let action = match tools::parse(&p.canonical_tool, &args)? {
        tools::Command::Read { artifact } if artifact == p.artifact => None,
        tools::Command::Replace(c)
            if c.artifact_id == p.artifact
                && c.version == p.version
                && c.sha256 == p.sha256
                && tools::digest(c.scene_json.as_bytes()) != p.sha256 =>
        {
            Some(args.clone())
        }
        _ => return Err(invalid()),
    };
    let ask: Value = serde_json::from_str(&p.continuation_json).map_err(|_| invalid())?;
    let call = &ask["call"];
    if ask["version"] != 1
        || ask["rule"] != p.gate_rule
        || ask["policy_sha256"]
            .as_str()
            .is_none_or(|h| !tools::digest_valid(h))
        || ask["steps"]
            .as_array()
            .is_none_or(|s| u32::try_from(s.len()).ok() != Some(p.step))
        || ask["remaining"].as_array().is_none_or(|s| s.len() > 3)
        || ask["round"].as_u64().is_none_or(|r| r >= 4)
        || call["name"].as_str() != tools::model_name(&p.canonical_tool, &p.artifact).as_deref()
        || call["id"]
            .as_str()
            .is_none_or(|s| s.is_empty() || s.len() > 128)
        || call["arguments"] != args
        || ask["definition"]["name"] != call["name"]
    {
        return Err(invalid());
    }
    Ok((ask, action))
}
pub(crate) fn access(state: &State, p: &ToolProposal, at: i64) -> bool {
    let Ok((ask, _)) = parsed(p) else {
        return false;
    };
    let (Some(space), Some(actor)) = (
        ask["envelope"]["space"].as_str(),
        ask["envelope"]["agent"].as_str(),
    ) else {
        return false;
    };
    state.writer(space, &p.owner) && state.writer(space, &p.serving) && state.writer(space, actor)
        && state.latest_trust(space, &p.owner, actor).is_some_and(|g| g.grant.expires_at_ms.is_none_or(|expiry| expiry > at)
            && matches!(g.grant.capability, types::Capability::Trust(level) if level >= TrustLevel::Suggest))
        && state.app_trust_baseline(&p.artifact).is_some_and(|g| !g.revoked
            && g.grant.expires_at_ms.is_none_or(|expiry| expiry > at)
            && matches!(g.grant.capability, types::Capability::Trust(level) if level >= TrustLevel::Act))
}
pub(crate) fn matches(state: &State, req: &AgentRequest, space: &str) -> bool {
    let Some(p) = req.tool_proposal.as_deref() else {
        return false;
    };
    let Ok((ask, action)) = parsed(p) else {
        return false;
    };
    let Some((source, inbound)) = shared::source_hash(state, space, &p.source, &p.serving) else {
        return false;
    };
    let Some(item) = state.items.get(&p.artifact) else {
        return false;
    };
    let ItemContent::App(app) = &item.current().content else {
        return false;
    };
    let pin_matches = action
        .as_ref()
        .is_none_or(|args| args["resourceUri"].as_str() == Some(app.resource_uri.as_str()));
    item.kind == types::ItemKind::App
        && item.space == space
        && item.current().number == p.version
        && app.view.is_some()
        && crate::app_resources::valid(app)
        && pin_matches
        && app
            .document
            .as_ref()
            .is_some_and(|d| d.sha256 == p.sha256 && d.bytes <= tools::SCENE_LIMIT as u64)
        && req.proposal.is_none()
        && req.item.is_none()
        && req.line.is_none()
        && req.action == ActionClass::Reversible
        && req.detail == shared::review_detail(p, &app.title)
        && state.writer(space, &req.agent)
        && state.writer(space, &p.owner)
        && state.writer(space, &p.serving)
        && p.serving != req.agent
        && source == p.source_sha256
        && req.tool_proposal_hash(space).as_deref() == Some(&req.content_hash)
        && serde_json::to_vec(&("zoen.tool-task.v1", space, &req.agent, &p.source))
            .is_ok_and(|b| p.task_sha256 == tools::digest(&b))
        && ask["envelope"]
            == json!({"agent":req.agent,"device":p.device,"owner":p.owner,"space":space,"from":p.serving,"inbound_sha256":inbound})
}
pub(crate) fn intent(p: &ToolProposal) -> R<String> {
    let (ask, _) = parsed(p)?;
    let canonical = miniapps::deckcraft::agent_tools::canonical_json(&json!([
        tools::REVISION,
        &ask["envelope"],
        &p.source,
        &p.source_sha256,
        p.step,
        &p.artifact,
        p.version,
        &p.sha256,
        &p.canonical_tool,
        &p.arguments_json,
    ]))
    .map_err(|_| invalid())?;
    Ok(tools::digest(canonical.as_bytes()))
}
pub(super) fn mutation(p: &ToolProposal) -> R<String> {
    Ok(format!("scenecmd_{}", intent(p)?))
}

impl Client {
    pub fn stage_scene_tool_approval(
        &self,
        turn: &AgentSceneTurn,
        input: AgentToolApprovalInput,
    ) -> R<AgentRequestDto> {
        let mut e = self.lock();
        e.recheck_scene_turn(turn, input.canonical_tool != "read_scene")?;
        let current = e.scene_snapshot_at(
            &turn.snapshot.artifact_id,
            e.item_state(&turn.snapshot.artifact_id)?.current().number,
        )?;
        if current != turn.snapshot || input.worker_binding != tools::executor_binding() {
            return Err(stale());
        }
        match tools::parse(&input.canonical_tool, &input.arguments)? {
            tools::Command::Read { artifact } if artifact == current.artifact_id => {}
            tools::Command::Replace(c)
                if c.artifact_id == current.artifact_id
                    && c.resource_uri == current.resource_uri
                    && c.version == current.version
                    && c.sha256 == current.sha256 => {}
            _ => return Err(stale()),
        }
        let ask: Value = serde_json::from_str(&input.continuation_json).map_err(|_| invalid())?;
        let p = ToolProposal {
            schema: SCHEMA,
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
                .and_then(|s| u32::try_from(s.len()).ok())
                .ok_or_else(invalid)?,
            artifact: current.artifact_id.clone(),
            version: current.version,
            sha256: current.sha256,
            canonical_tool: input.canonical_tool,
            arguments_json: miniapps::deckcraft::agent_tools::canonical_json(&input.arguments)
                .map_err(|_| invalid())?,
            worker_binding: input.worker_binding,
            engine_revision: tools::REVISION.into(),
            continuation_json: input.continuation_json,
            expires_at_ms: crate::engine::now_ms()
                .checked_add(15 * 60 * 1000)
                .ok_or_else(invalid)?,
            gate_rule: input.gate_rule,
            review: None,
        };
        parsed(&p)?;
        if let Some(prior) =
            e.state
                .requests
                .values()
                .filter(|r| {
                    r.req.agent == current.actor_id
                        && r.req.tool_proposal.as_ref().is_some_and(|old| {
                            old.task_sha256 == p.task_sha256 && old.step == p.step
                        })
                })
                .max_by_key(|r| e.state.tool_approval_order(&r.req))
        {
            let old = prior.req.tool_proposal.as_ref().ok_or_else(stale)?;
            let mut same = p.clone();
            same.expires_at_ms = old.expires_at_ms;
            same.review = old.review.clone();
            if prior.req.tool_proposal.as_deref() != Some(&same) {
                return Err(stale());
            }
            return Ok(e.request_dto(prior));
        }
        let mut req = AgentRequest {
            id: types::new_id("rq")?,
            agent: current.actor_id.clone(),
            title: shared::operation(&p.canonical_tool),
            detail: shared::review_detail(&p, &current.title),
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
        if !matches(&e.state, &req, &current.space_id)
            || !access(
                &e.state,
                req.tool_proposal.as_ref().ok_or_else(invalid)?,
                crate::engine::now_ms(),
            )
            || !e.tool_non_blocked(&req)
        {
            return Err(stale());
        }
        let id = req.id.clone();
        e.append(
            &current.space_id,
            &current.actor_id,
            types::EventBody::RequestOpened { request: req },
        )?;
        Ok(e.request_dto(e.state.requests.get(&id).ok_or_else(stale)?))
    }
    pub fn approved_scene_tool_turn(&self, id: &str) -> R<ApprovedSceneTurn> {
        let e = self.lock();
        let r = e.state.requests.get(id).ok_or_else(stale)?;
        let (_, p) = e.tool_resolution(id, &r.req.content_hash)?;
        parsed(p)?;
        if e.me_id()? != r.req.agent {
            return Err(stale());
        }
        let (device, owner, space, source_sha256) =
            e.agent_binary_authority(&p.artifact, &p.source, &p.serving, false)?;
        if device != p.device
            || owner != p.owner
            || space != r.space
            || source_sha256 != p.source_sha256
        {
            return Err(stale());
        }
        let turn = AgentSceneTurn {
            snapshot: e.scene_snapshot_at(&p.artifact, p.version)?,
            device,
            owner,
            source: p.source.clone(),
            serving: p.serving.clone(),
            source_sha256,
            approval: Some((id.into(), r.req.content_hash.clone())),
        };
        e.recheck_scene_turn(&turn, false)?;
        Ok(ApprovedSceneTurn {
            turn,
            proposal: p.clone(),
        })
    }
    pub fn agent_scene_replace(
        &self,
        turn: &AgentSceneTurn,
        command: &tools::ReplaceScene,
    ) -> R<AppDocumentDto> {
        let mut e = self.lock();
        e.recheck_scene_turn(turn, true)?;
        let (request, hash) = turn.approval.as_ref().ok_or_else(stale)?;
        let (_, p) = e.tool_resolution(request, hash)?;
        let args = serde_json::to_value(command).map_err(|_| invalid())?;
        if p.schema != SCHEMA
            || p.canonical_tool != "replace_scene"
            || p.arguments_json
                != miniapps::deckcraft::agent_tools::canonical_json(&args).map_err(|_| invalid())?
            || p.artifact != turn.snapshot.artifact_id
            || p.device != turn.device
            || p.owner != turn.owner
            || p.source != turn.source
            || p.serving != turn.serving
            || p.source_sha256 != turn.source_sha256
        {
            return Err(stale());
        }
        let p = p.clone();
        if let Some(result) = e.scene_result(&p)? {
            return Ok(result);
        }
        if p.expires_at_ms <= crate::engine::now_ms()
            || !e.state.tool_review_current(&e.state.requests[request])
            || !matches(
                &e.state,
                &e.state.requests[request].req,
                &turn.snapshot.space_id,
            )
        {
            return Err(stale());
        }
        e.save_app_document(
            &command.artifact_id,
            command.version,
            (&command.sha256, Some(&command.resource_uri)),
            &command.scene_json,
            &mutation(&p)?,
        )
    }
}
