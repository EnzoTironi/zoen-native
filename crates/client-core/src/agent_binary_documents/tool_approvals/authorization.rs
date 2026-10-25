//! Existing authorization on the same canonical Engine authority.

use super::*;

impl Engine {
    pub(crate) fn tool_non_blocked(&self, req: &AgentRequest) -> bool {
        let Some(p) = &req.tool_proposal else {
            return false;
        };
        let action = if matches!(
            p.canonical_tool.as_str(),
            "read_deckcraft"
                | "read_gridcraft"
                | "read_wordcraft"
                | "read_photocraft"
                | "read_page"
                | "read_site"
        ) {
            ActionClass::Reply
        } else {
            ActionClass::Reversible
        };
        !matches!(
            self.decide(
                &req.agent,
                self.state
                    .requests
                    .get(&req.id)
                    .map(|r| r.space.as_str())
                    .unwrap_or(""),
                &action,
                0
            ),
            grants::Decision::Block(_)
        )
    }

    pub(crate) fn tool_resolution(
        &self,
        id: &str,
        hash: &str,
    ) -> R<(&crate::engine::RequestState, &ToolProposal)> {
        let r = self.state.requests.get(id).ok_or_else(stale)?;
        let p = r.req.tool_proposal.as_deref().ok_or_else(malformed)?;
        if r.status != ReqStatus::Approved
            || r.req.content_hash != hash
            || r.resolution.as_ref().and_then(|r| r.tool_decision)
                != Some(ToolDecision::ApproveOnce)
            || r.req.tool_proposal_hash(&r.space).as_deref() != Some(hash)
            || parsed(p).is_err()
            || !self.tool_non_blocked(&r.req)
            || !self.tool_review_confirmed(r)
        {
            return Err(denied());
        }
        Ok((r, p))
    }

    pub(in crate::agent_binary_documents) fn recheck_tool_turn(
        &self,
        turn: &AgentBinaryTurn,
        mutation: Option<(&str, &str)>,
    ) -> R<()> {
        let Some(binding) = &turn.approval else {
            return Ok(());
        };
        let (r, p) = self.tool_resolution(&binding.request, &binding.content_hash)?;
        if p.device != turn.device
            || p.owner != turn.owner
            || p.artifact != turn.document.item_id
            || p.source != turn.source
            || p.serving != turn.serving
            || p.source_sha256 != turn.source_sha256
            || p.version != turn.document.version
            || p.sha256 != turn.document.sha256
            || !self.state.tool_app_act(p, crate::engine::now_ms())
        {
            return Err(denied());
        }
        let (_, action) = parsed(p)?;
        if mutation.is_some() && action.is_none() {
            return Err(denied());
        }
        let intent = command_intent(p, action.as_ref())?;
        if mutation.is_some_and(|(id, hash)| id != mutation_id(p, &intent) || hash != intent) {
            return Err(denied());
        }
        // Accepted-result recovery is read-only and uses the existing durable receipt.
        // It does not authorize fresh compute against an old head or expired Ask.
        if action.is_some()
            && self
                .app_binary_result(
                    &p.artifact,
                    p.version,
                    &p.sha256,
                    &mutation_id(p, &intent),
                    &intent,
                )?
                .is_some()
        {
            return Ok(());
        }
        if p.expires_at_ms <= crate::engine::now_ms()
            || !self.state.tool_review_current(r)
            || !self.state.tool_request_matches(&r.req, &r.space)
        {
            return Err(stale());
        }
        Ok(())
    }

    pub(crate) fn decide_tool_request(
        &mut self,
        id: &str,
        observed: &str,
        decision: ToolRequestDecision,
    ) -> R<DecideOutcome> {
        if decision == ToolRequestDecision::RequestReview {
            return self.request_tool_review(id, observed);
        }
        let r = self.state.requests.get(id).cloned().ok_or_else(stale)?;
        let p = r.req.tool_proposal.as_ref().ok_or_else(malformed)?;
        let me = self.me_id()?;
        if me != p.owner
            || self
                .identities
                .get(&r.req.agent)
                .and_then(|a| a.owner.as_deref())
                != Some(me.as_str())
            || !self.state.writer(&r.space, &me)
            || observed != r.req.content_hash
        {
            return Err(denied());
        }
        let decision = match decision {
            ToolRequestDecision::ApproveOnce => ToolDecision::ApproveOnce,
            ToolRequestDecision::Deny => ToolDecision::Deny,
            ToolRequestDecision::Cancel => ToolDecision::Cancel,
            ToolRequestDecision::RequestReview => return Err(malformed()),
        };
        if r.status != ReqStatus::Pending {
            if let Some(resolution) = r
                .resolution
                .as_ref()
                .filter(|resolution| resolution.tool_decision == Some(decision))
            {
                return Ok(DecideOutcome {
                    request: self.request_dto(&r),
                    message: resolution.message.clone(),
                    standing_grant_id: None,
                    also_resolved: 0,
                });
            }
            return Err(stale());
        }
        let approved = decision == ToolDecision::ApproveOnce;
        if approved
            && (p.expires_at_ms <= crate::engine::now_ms()
                || !self.state.tool_review_current(&r)
                || !self.state.tool_request_matches(&r.req, &r.space)
                || !self.state.tool_app_act(p, crate::engine::now_ms())
                || !self.tool_non_blocked(&r.req))
        {
            return Err(stale());
        }
        let message = match decision {
            ToolDecision::ApproveOnce => "Approved once. Waiting for the reviewed action.",
            ToolDecision::Deny => "Action denied.",
            ToolDecision::Cancel => "Action cancelled.",
        }
        .to_string();
        self.append(
            &r.space,
            &me,
            EventBody::RequestResolved {
                request: id.into(),
                approved,
                content_hash: observed.into(),
                resolution: Some(RequestResolution {
                    message: message.clone(),
                    created_item: None,
                    ai_cost_cents: 0,
                    standing_grant: None,
                    tool_decision: Some(decision),
                }),
            },
        )?;
        Ok(DecideOutcome {
            request: self.request_dto(&self.state.requests[id]),
            message,
            standing_grant_id: None,
            also_resolved: 0,
        })
    }
}
