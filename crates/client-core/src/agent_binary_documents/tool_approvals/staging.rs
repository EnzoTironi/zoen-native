//! Existing staging on the same canonical Client authority.

use super::*;

impl Client {
    pub fn stage_tool_approval(
        &self,
        turn: &AgentBinaryTurn,
        input: AgentToolApprovalInput,
    ) -> R<AgentRequestDto> {
        let mut e = self.lock();
        e.recheck_agent_binary_turn(turn, false)?;
        let (version, head, _) = agent_tools::command(&input.canonical_tool, &input.arguments)
            .map_err(|_| malformed())?;
        if version.is_some_and(|v| v != turn.document.version)
            || head.as_ref().is_some_and(|h| h != &turn.document.sha256)
        {
            return Err(stale());
        }
        let ask: Value = serde_json::from_str(&input.continuation_json).map_err(|_| malformed())?;
        let p = ToolProposal {
            schema: 1,
            source: turn.source.clone(),
            serving: turn.serving.clone(),
            source_sha256: turn.source_sha256.clone(),
            device: turn.device.clone(),
            owner: turn.owner.clone(),
            task_sha256: sha(&serde_json::to_vec(&(
                "zoen.tool-task.v1",
                &turn.space,
                &turn.document.actor_id,
                &turn.source,
            ))
            .map_err(|_| malformed())?),
            step: ask["steps"]
                .as_array()
                .map(|a| a.len() as u32)
                .ok_or_else(malformed)?,
            artifact: turn.document.item_id.clone(),
            version: turn.document.version,
            sha256: turn.document.sha256.clone(),
            canonical_tool: input.canonical_tool,
            arguments_json: agent_tools::canonical_json(&input.arguments)
                .map_err(|_| malformed())?,
            worker_binding: input.worker_binding,
            engine_revision: turn.document.engine_revision.clone(),
            continuation_json: input.continuation_json,
            expires_at_ms: crate::engine::now_ms() + REQUEST_LIFETIME_MS,
            gate_rule: input.gate_rule,
        };
        parsed(&p)?;
        // A stable source/step cannot mint a second intent after approval or restart.
        if let Some(r) = e.state.requests.values().find(|r| {
            r.req.agent == turn.document.actor_id
                && r.req
                    .tool_proposal
                    .as_ref()
                    .is_some_and(|old| old.task_sha256 == p.task_sha256 && old.step == p.step)
        }) {
            let mut same = p.clone();
            same.expires_at_ms = r.req.tool_proposal.as_ref().unwrap().expires_at_ms;
            if r.req.tool_proposal.as_deref() != Some(&same) {
                return Err(stale());
            }
            return Ok(e.request_dto(r));
        }
        let operation = operation(&p.canonical_tool);
        let artifact_title = e.item_state(&p.artifact)?.current().content.title();
        let detail = review_detail(&p, &artifact_title);
        let mut req = AgentRequest {
            id: types::new_id("rq"),
            agent: turn.document.actor_id.clone(),
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
        req.content_hash = req.tool_proposal_hash(&turn.space).ok_or_else(malformed)?;
        if !e.state.tool_request_matches(&req, &turn.space)
            || !e
                .state
                .tool_app_act(req.tool_proposal.as_ref().unwrap(), crate::engine::now_ms())
        {
            return Err(stale());
        }
        // Core Block remains non-overridable. No budget or standing grant is added.
        let action = if req.tool_proposal.as_ref().unwrap().canonical_tool == "read_deckcraft" {
            ActionClass::Reply
        } else {
            ActionClass::Reversible
        };
        if matches!(
            e.decide(&req.agent, &turn.space, &action, 0),
            grants::Decision::Block(_)
        ) {
            return Err(denied());
        }
        let id = req.id.clone();
        e.append(
            &turn.space,
            &req.agent.clone(),
            EventBody::RequestOpened { request: req },
        )?;
        Ok(e.request_dto(&e.state.requests[&id]))
    }

    pub fn approved_tool_turn(
        &self,
        request_id: &str,
        worker_binding: &str,
    ) -> R<ApprovedToolTurn> {
        let (proposal, hash) = {
            let e = self.lock();
            let r = e.state.requests.get(request_id).ok_or_else(stale)?;
            let (_, p) = e.tool_resolution(request_id, &r.req.content_hash)?;
            if p.worker_binding != worker_binding || r.req.agent != e.me_id()? {
                return Err(denied());
            }
            (p.clone(), r.req.content_hash.clone())
        };
        let mut turn = self.agent_binary_turn(
            &proposal.artifact,
            &proposal.source,
            &proposal.serving,
            Some(proposal.version),
            Some(&proposal.sha256),
        )?;
        turn.approval = Some(ApprovalBinding {
            request: request_id.into(),
            content_hash: hash,
        });
        {
            let e = self.lock();
            e.recheck_agent_binary_turn(&turn, false)?;
            e.recheck_tool_turn(&turn, None)?;
        }
        Ok(ApprovedToolTurn { turn, proposal })
    }
}
