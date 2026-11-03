use super::*;
use crate::{
    agent_binary_documents::tool_approvals as shared, agent_page_tools as hash, engine::Engine,
    AgentRequestDto, AgentToolApprovalInput,
};
use types::{ActionClass, AgentRequest, EventBody, ToolProposal};

/// Rust-only permit. Every use checks current signed authority again.
pub struct ApprovedConversationSearch {
    request: String,
    content_hash: String,
    proposal: ToolProposal,
}
impl ApprovedConversationSearch {
    #[must_use]
    pub fn proposal(&self) -> &ToolProposal {
        &self.proposal
    }
}

impl Engine {
    pub(super) fn conversation_search_permit(
        &self,
        request: &str,
        content_hash: &str,
    ) -> R<(String, ToolProposal)> {
        let (r, p) = self.tool_resolution(request, content_hash)?;
        parsed(p)?;
        let (device, owner, space, source) =
            self.agent_source_authority(&r.space, &p.source, &p.serving, false)?;
        if self.me_id()? != r.req.agent
            || device != p.device
            || owner != p.owner
            || source != p.source_sha256
            || p.expires_at_ms <= crate::engine::now_ms()
            || !self.state.tool_review_current(r)
            || !self.state.tool_request_matches(&r.req, &r.space)
            || !access(&self.state, p)
        {
            return Err(stale());
        }
        Ok((space, p.clone()))
    }

    pub(super) fn conversation_search_result(&mut self, space: &str, args: &Value) -> R<String> {
        let query = parse(args)?;
        let page =
            self.search_messages(space, &query.query, query.cursor.as_deref(), query.limit)?;
        let encoded = serde_json::to_string(&page).map_err(|_| invalid())?;
        if encoded.len() > 32 * 1024 {
            return Err(invalid());
        }
        Ok(encoded)
    }
}

impl Client {
    pub fn stage_conversation_search_approval(
        &self,
        space: &str,
        source: &str,
        serving: &str,
        input: AgentToolApprovalInput,
    ) -> R<AgentRequestDto> {
        let mut e = self.lock();
        let (device, owner, _, source_sha256) =
            e.agent_source_authority(space, source, serving, false)?;
        let actor = e.me_id()?;
        let query = parse(&input.arguments)?;
        let ask: Value = serde_json::from_str(&input.continuation_json).map_err(|_| invalid())?;
        let p = ToolProposal {
            schema: SCHEMA,
            source: source.into(),
            serving: serving.into(),
            sha256: source_sha256.clone(),
            source_sha256,
            device,
            owner,
            task_sha256: hash::digest(
                &serde_json::to_vec(&("zoen.tool-task.v1", space, &actor, source))
                    .map_err(|_| invalid())?,
            ),
            step: ask["steps"]
                .as_array()
                .and_then(|steps| u32::try_from(steps.len()).ok())
                .ok_or_else(invalid)?,
            artifact: String::new(),
            version: 0,
            canonical_tool: input.canonical_tool,
            arguments_json: miniapps::binary::agent_tools::canonical_json(&input.arguments)
                .map_err(|_| invalid())?,
            worker_binding: input.worker_binding,
            engine_revision: REVISION.into(),
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
                    r.req.agent == actor
                        && r.req.tool_proposal.as_ref().is_some_and(|old| {
                            old.task_sha256 == p.task_sha256 && old.step == p.step
                        })
                })
                .max_by_key(|r| {
                    r.req
                        .tool_proposal
                        .as_ref()
                        .and_then(|p| p.review.as_ref())
                        .map_or(0, |review| review.generation)
                })
        {
            let prior = r.req.tool_proposal.as_ref().ok_or_else(stale)?;
            let mut same = p.clone();
            same.expires_at_ms = prior.expires_at_ms;
            same.review = prior.review.clone();
            if r.req.tool_proposal.as_deref() != Some(&same) {
                return Err(stale());
            }
            return Ok(e.request_dto(r));
        }
        let page = if query.cursor.is_some() {
            crate::i18n::t("Próxima página", "Next page")
        } else {
            crate::i18n::t("Primeira página", "First page")
        };
        let mut req = AgentRequest {
            id: types::new_id("rq")?,
            agent: actor.clone(),
            title: shared::operation(TOOL),
            detail: format!(
                "“{}”\n{} · {} {}",
                query.query,
                page,
                query.limit,
                crate::i18n::t("resultados no máximo", "results maximum")
            ),
            audience: "This conversation".into(),
            action: ActionClass::Reversible,
            content_hash: String::new(),
            item: None,
            line: None,
            proposal: None,
            tool_proposal: Some(Box::new(p)),
        };
        req.content_hash = req.tool_proposal_hash(space).ok_or_else(invalid)?;
        if !e.state.tool_request_matches(&req, space) || !e.tool_non_blocked(&req) {
            return Err(stale());
        }
        let id = req.id.clone();
        e.append(space, &actor, EventBody::RequestOpened { request: req })?;
        Ok(e.request_dto(e.state.requests.get(&id).ok_or_else(stale)?))
    }

    pub fn approved_conversation_search(&self, request: &str) -> R<ApprovedConversationSearch> {
        let e = self.lock();
        let hash = &e
            .state
            .requests
            .get(request)
            .ok_or_else(stale)?
            .req
            .content_hash;
        let (_, proposal) = e.conversation_search_permit(request, hash)?;
        Ok(ApprovedConversationSearch {
            request: request.into(),
            content_hash: hash.clone(),
            proposal,
        })
    }

    pub fn execute_conversation_search(&self, permit: &ApprovedConversationSearch) -> R<String> {
        let mut e = self.lock();
        let (space, p) = e.conversation_search_permit(&permit.request, &permit.content_hash)?;
        if p != permit.proposal {
            return Err(stale());
        }
        let args = serde_json::from_str(&p.arguments_json).map_err(|_| invalid())?;
        e.conversation_search_result(&space, &args)
    }
}
