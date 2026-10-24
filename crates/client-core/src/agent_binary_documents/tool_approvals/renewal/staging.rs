//! Only the certified host can turn a signed owner intent into another pending review.
use super::*;

impl Engine {
    fn tool_review_accepted(&self, p: &ToolProposal) -> R<bool> {
        let (_, action) = parsed(p)?;
        if action.is_none() {
            return Ok(false);
        }
        if p.schema == 5 {
            return self.site_tool_accepted(p);
        }
        if p.schema == 3 {
            return self.creation_result(p).map(|result| result.is_some());
        }
        if p.schema == 2 {
            let intent = crate::agent_pages::approvals::intent(p)?;
            return self
                .agent_page_command_result(&p.artifact, &format!("pagecmd_{intent}"), &intent)
                .map(|result| result.is_some());
        }
        let intent = command_intent(p, action.as_ref())?;
        self.app_binary_result(
            &p.artifact,
            p.version,
            &p.sha256,
            &mutation_id(p, &intent),
            &intent,
        )
        .map(|result| result.is_some())
    }
}

impl Client {
    /// No execution authority. None means reconcile an existing accepted result;
    /// it is never permission to retry. The host must also require its durable
    /// never-started witness before invoking this preparation port.
    pub fn stage_tool_review_successor(
        &self,
        predecessor: &str,
        owner_intent: &str,
    ) -> R<Option<AgentRequestDto>> {
        let mut e = self.lock();
        let r = e
            .state
            .requests
            .get(predecessor)
            .cloned()
            .ok_or_else(stale)?;
        let old = r.req.tool_proposal.as_ref().ok_or_else(malformed)?;
        let intent = r.review.as_ref().ok_or_else(stale)?;
        if intent.id != owner_intent || e.me_id()? != r.req.agent {
            return Err(denied());
        }
        // Existing authority is checked before even revealing acceptance metadata.
        e.tool_resolution(predecessor, &r.req.content_hash)?;
        let (device, owner, space, source) = if old.schema == 5 {
            e.site_proposal_authority(old)?
        } else if old.schema == 3 {
            e.creation_proposal_authority(old)?
        } else {
            e.agent_binary_authority(&old.artifact, &old.source, &old.serving, false)?
        };
        if device != old.device
            || owner != old.owner
            || space != r.space
            || source != old.source_sha256
            || !e.state.tool_app_act(old, crate::engine::now_ms())
            || !e.tool_non_blocked(&r.req)
        {
            return Err(denied());
        }
        // A stable command receipt is checked before requiring the old base to
        // remain current. Accepted effects reconcile without fresh staging/IO.
        if e.tool_review_accepted(old)? {
            return Ok(None);
        }
        if let Some(successor) = &intent.successor {
            let next = e.state.requests.get(successor).ok_or_else(stale)?;
            if !e.state.tool_review_active_leaf(next) {
                return Err(stale());
            }
            return Ok(Some(e.request_dto(next)));
        }
        if e.approval_replay_needed
            || !e.state.tool_review_active_leaf(&r)
            || !e.state.tool_request_matches(&r.req, &r.space)
            || e.state.tool_review_source_replied(&r)
        {
            return Err(stale());
        }
        let generation = old
            .review
            .as_ref()
            .map_or(Some(1), |r| r.generation.checked_add(1))
            .ok_or_else(stale)?;
        let mut request = r.req.clone();
        request.id = types::new_id("rq");
        let proposal = request.tool_proposal.as_mut().unwrap();
        proposal.expires_at_ms = crate::engine::now_ms()
            .checked_add(REQUEST_LIFETIME_MS)
            .ok_or_else(stale)?;
        proposal.review = Some(ToolReviewLineage {
            root: root(&r).into(),
            predecessor: predecessor.into(),
            intent: owner_intent.into(),
            generation,
        });
        request.content_hash = request.tool_proposal_hash(&r.space).ok_or_else(malformed)?;
        if !e
            .state
            .tool_review_successor_valid(&request, crate::engine::now_ms())
        {
            return Err(stale());
        }
        let id = request.id.clone();
        e.append(&r.space, &r.req.agent, EventBody::RequestOpened { request })?;
        Ok(Some(
            e.request_dto(e.state.requests.get(&id).ok_or_else(stale)?),
        ))
    }

    /// Public signed metadata only. An intent does not approve the frozen call.
    pub fn tool_review_pending_intent(&self, request: &str) -> Option<String> {
        let e = self.lock();
        let r = e.state.requests.get(request)?;
        if e.me_id().ok().as_deref() != Some(r.req.agent.as_str()) {
            return None;
        }
        if !e.state.tool_review_active_leaf(r) {
            return None;
        }
        r.review
            .as_ref()
            .filter(|intent| intent.successor.is_none())
            .map(|intent| intent.id.clone())
    }

    /// Preserve one operational journal key across signed review generations.
    /// These identifiers/hashes grant no authority and contain no journal data.
    pub fn tool_review_journal_root(&self, request: &str, observed: &str) -> R<(String, String)> {
        let e = self.lock();
        let r = e.state.requests.get(request).ok_or_else(stale)?;
        if r.req.content_hash != observed || e.me_id()? != r.req.agent {
            return Err(denied());
        }
        let root = e.state.requests.get(root(r)).ok_or_else(stale)?;
        let p = r.req.tool_proposal.as_ref().ok_or_else(malformed)?;
        let original = root.req.tool_proposal.as_ref().ok_or_else(malformed)?;
        if root.req.agent != r.req.agent
            || root.space != r.space
            || original.task_sha256 != p.task_sha256
            || original.step != p.step
            || original.source != p.source
            || original.arguments_json != p.arguments_json
            || original.continuation_json != p.continuation_json
        {
            return Err(stale());
        }
        Ok((root.req.id.clone(), root.req.content_hash.clone()))
    }
}
