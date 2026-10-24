//! Preparation creates one signed pending review; it never creates the document.
use super::*;
use crate::{AgentRequestDto, AgentToolApprovalInput};
use types::EventBody;

impl Client {
    pub fn stage_app_creation_approval(
        &self,
        space: &str,
        source: &str,
        serving: &str,
        input: AgentToolApprovalInput,
    ) -> R<AgentRequestDto> {
        let mut e = self.lock();
        let (device, owner, _, source_sha256) = e.creation_authority(space, source, serving)?;
        if input.canonical_tool != TOOL || input.worker_binding != executor_binding() {
            return Err(invalid());
        }
        let title = title(&input.arguments)?;
        let ask: Value = serde_json::from_str(&input.continuation_json).map_err(|_| invalid())?;
        let actor = e.me_id()?;
        let mut p = ToolProposal {
            schema: SCHEMA,
            source: source.into(),
            serving: serving.into(),
            source_sha256,
            device,
            owner,
            task_sha256: sha(
                &serde_json::to_vec(&("zoen.tool-task.v1", space, &actor, source))
                    .map_err(|_| invalid())?,
            ),
            step: ask["steps"]
                .as_array()
                .map(|steps| steps.len() as u32)
                .ok_or_else(invalid)?,
            artifact: String::new(),
            version: 1,
            sha256: sha(registration::EMPTY),
            canonical_tool: TOOL.into(),
            arguments_json: agent_tools::canonical_json(&input.arguments).map_err(|_| invalid())?,
            worker_binding: input.worker_binding,
            engine_revision: deckcraft::ENGINE_REVISION.into(),
            continuation_json: input.continuation_json,
            expires_at_ms: crate::engine::now_ms()
                .checked_add(15 * 60 * 1000)
                .ok_or_else(invalid)?,
            gate_rule: input.gate_rule,
            review: None,
        };
        let mutation = mutation(&p, space, &actor)?;
        p.artifact =
            crate::app_binary_documents::creation_item_id(&actor, &p.device, space, &mutation)?;
        parsed(&p)?;
        if let Some(existing) =
            e.state
                .requests
                .values()
                .filter(|r| {
                    r.req.agent == actor
                        && r.req.tool_proposal.as_ref().is_some_and(|old| {
                            old.task_sha256 == p.task_sha256 && old.step == p.step
                        })
                })
                .max_by_key(|r| e.state.tool_approval_order(&r.req))
        {
            let mut same = p.clone();
            let old = existing.req.tool_proposal.as_ref().ok_or_else(stale)?;
            same.expires_at_ms = old.expires_at_ms;
            same.review = old.review.clone();
            if old.as_ref() != &same {
                return Err(stale());
            }
            return Ok(e.request_dto(existing));
        }
        let mut request = AgentRequest {
            id: types::new_id("rq")?,
            agent: actor.clone(),
            title: shared::operation(TOOL),
            detail: format!(
                "{}: {title}",
                crate::i18n::t("Criar apresentação", "Create presentation")
            ),
            audience: "This conversation".into(),
            action: ActionClass::Reversible,
            content_hash: String::new(),
            item: None,
            line: None,
            proposal: None,
            tool_proposal: Some(Box::new(p)),
        };
        request.content_hash = request.tool_proposal_hash(space).ok_or_else(invalid)?;
        if !matches(&e.state, &request, space) || !e.tool_non_blocked(&request) {
            return Err(stale());
        }
        let id = request.id.clone();
        e.append(space, &actor, EventBody::RequestOpened { request })?;
        Ok(e.request_dto(e.state.requests.get(&id).ok_or_else(stale)?))
    }
}
