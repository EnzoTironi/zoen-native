//! Completed context is rederived from published Pages and linked receipts.
use super::{approvals, invalid, stale, AgentPageResult, AgentPageSnapshot, Client, R};
use crate::{agent_page_tools as tools, engine::ReqStatus};
use serde_json::Value;

impl Client {
    /// Read-only continuation recovery. No cached text can grant a new write.
    pub fn recover_page_tool_results(&self, request: &str, steps_json: &str) -> R<String> {
        if steps_json.len() > crate::agent_binary_documents::tool_approvals::CONTINUATION_LIMIT {
            return Err(invalid());
        }
        let mut steps: Value = serde_json::from_str(steps_json).map_err(|_| invalid())?;
        let steps = steps
            .as_array_mut()
            .filter(|s| !s.is_empty() && s.len() <= 16)
            .ok_or_else(invalid)?;
        let e = self.lock();
        let r = e.state.requests.get(request).ok_or_else(stale)?;
        let (_, proposal) = e.tool_resolution(request, &r.req.content_hash)?;
        let (ask, _) = approvals::parsed(proposal)?;
        let prefix = ask["steps"].as_array().ok_or_else(invalid)?;
        if e.me_id()? != r.req.agent
            || steps.len() <= prefix.len()
            || steps[..prefix.len()] != prefix[..]
            || steps[prefix.len()]["call"] != ask["call"]
        {
            return Err(stale());
        }
        let (device, owner, space, source) = e.agent_binary_authority(
            &proposal.artifact,
            &proposal.source,
            &proposal.serving,
            false,
        )?;
        if device != proposal.device
            || owner != proposal.owner
            || space != r.space
            || source != proposal.source_sha256
        {
            return Err(stale());
        }
        for (index, step) in steps.iter_mut().enumerate() {
            let call = &step["call"];
            let name = call["name"].as_str().ok_or_else(invalid)?;
            let tool = ["read_page", "replace_page_block"]
                .into_iter()
                .find(|tool| tools::model_name(tool, &proposal.artifact).as_deref() == Some(name))
                .ok_or_else(invalid)?;
            let command = tools::parse(tool, &call["arguments"]).map_err(|_| invalid())?;
            if command.artifact() != proposal.artifact {
                return Err(stale());
            }
            if step["is_error"] == true {
                continue;
            }
            if step["is_error"] != false {
                return Err(invalid());
            }
            let encoded = step["result"].as_str().ok_or_else(invalid)?;
            match command {
                tools::Command::Read { artifact } => {
                    // Reads may precede this approved write. Verify every cached byte
                    // against that exact historical shadow, never the private live doc.
                    let cached: AgentPageSnapshot =
                        serde_json::from_str(encoded).map_err(|_| invalid())?;
                    if cached.actor_id != r.req.agent
                        || cached.space_id != r.space
                        || cached.artifact_id != artifact
                        || e.agent_page_snapshot_at(&artifact, cached.version)? != cached
                    {
                        return Err(stale());
                    }
                }
                tools::Command::Replace(_) => {
                    let (prior, _) = e
                        .state
                        .requests
                        .values()
                        .filter(|prior| prior.status == ReqStatus::Approved)
                        .filter_map(|prior| prior.req.tool_proposal.as_deref().map(|p| (prior, p)))
                        .find(|(prior, p)| {
                            prior.req.agent == r.req.agent
                                && prior.space == r.space
                                && p.schema == 2
                                && p.task_sha256 == proposal.task_sha256
                                && p.step as usize == index
                                && serde_json::from_str::<Value>(&p.continuation_json)
                                    .is_ok_and(|held| held["call"] == *call)
                        })
                        .ok_or_else(stale)?;
                    let (_, signed) = e.tool_resolution(&prior.req.id, &prior.req.content_hash)?;
                    let (device, owner, space, source) = e.agent_binary_authority(
                        &signed.artifact,
                        &signed.source,
                        &signed.serving,
                        true,
                    )?;
                    if device != signed.device
                        || owner != signed.owner
                        || space != r.space
                        || source != signed.source_sha256
                        || !e.state.writer(&r.space, &r.req.agent)
                        || !e.state.writer(&r.space, &signed.owner)
                    {
                        return Err(stale());
                    }
                    let intent = approvals::intent(signed)?;
                    let result = e
                        .agent_page_command_result(
                            &signed.artifact,
                            &format!("pagecmd_{intent}"),
                            &intent,
                        )?
                        .ok_or_else(stale)?;
                    if !encoded.is_empty() {
                        let old: AgentPageResult =
                            serde_json::from_str(encoded).map_err(|_| invalid())?;
                        if old.artifact_id != result.artifact_id
                            || old.actor_id != result.actor_id
                            || old.device_id != result.device_id
                            || old.observed_base_version != result.observed_base_version
                            || old.applied_content_sha256 != result.applied_content_sha256
                            || old.client_id != result.client_id
                        {
                            return Err(stale());
                        }
                    }
                    // The Runtime validates the frozen completed prefix exactly.
                    // Recheck its receipts, but only rebuild the newly resumed tail.
                    if index >= prefix.len() {
                        step["result"] = serde_json::to_string(&result)
                            .map_err(|_| invalid())?
                            .into();
                    }
                }
            }
        }
        serde_json::to_string(steps).map_err(|_| invalid())
    }
}
