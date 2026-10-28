//! Cached search plaintext is never authority or evidence of current history.
use super::*;

impl Client {
    pub fn recover_conversation_search_results(
        &self,
        request: &str,
        steps_json: &str,
    ) -> R<String> {
        if steps_json.len() > crate::agent_binary_documents::tool_approvals::CONTINUATION_LIMIT {
            return Err(invalid());
        }
        let mut steps: Value = serde_json::from_str(steps_json).map_err(|_| invalid())?;
        let rows = steps
            .as_array_mut()
            .filter(|s| !s.is_empty() && s.len() <= 16)
            .ok_or_else(invalid)?;
        let mut e = self.lock();
        let hash = e
            .state
            .requests
            .get(request)
            .ok_or_else(stale)?
            .req
            .content_hash
            .clone();
        let (space, proposal) = e.conversation_search_permit(request, &hash)?;
        let (ask, _) = parsed(&proposal)?;
        let prefix = ask["steps"].as_array().ok_or_else(invalid)?;
        if rows.len() <= prefix.len()
            || rows[..prefix.len()] != prefix[..]
            || rows[prefix.len()]["call"] != ask["call"]
        {
            return Err(stale());
        }
        for (index, step) in rows.iter_mut().enumerate() {
            let call = &step["call"];
            if call["name"] != TOOL || step["is_error"] != false {
                return Err(invalid());
            }
            if index != prefix.len() {
                // Earlier/later reviewed reads retain their own exact request;
                // this approval never grants a different query or pagination.
                let reviewed = e
                    .state
                    .requests
                    .values()
                    .filter(|r| {
                        r.space == space
                            && r.req.tool_proposal.as_ref().is_some_and(|p| {
                                p.task_sha256 == proposal.task_sha256
                                    && p.step as usize == index
                                    && serde_json::from_str::<Value>(&p.continuation_json)
                                        .is_ok_and(|ask| ask["call"] == *call)
                                    && e.state.tool_review_current(r)
                            })
                    })
                    .max_by_key(|r| e.state.tool_approval_order(&r.req));
                if let Some(r) = reviewed {
                    e.conversation_search_permit(&r.req.id, &r.req.content_hash)?;
                } else if call["requires_review"] == true {
                    return Err(stale());
                }
            }
            let fresh = e.conversation_search_result(&space, &call["arguments"])?;
            // Runtime's signed prefix is immutable; changed earlier evidence
            // refuses continuation instead of replaying old message text.
            if index < prefix.len() && step["result"].as_str() != Some(&fresh) {
                return Err(stale());
            }
            step["result"] = fresh.into();
        }
        serde_json::to_string(&steps).map_err(|_| invalid())
    }
}
