//! Existing recovery on the same canonical Client authority.

use super::*;

impl Client {
    /// Recover completed results only. This port never returns a turn/permission
    /// and never executes a prepared call lacking an exact canonical receipt.
    /// A cached read is context after fresh access checks; it cannot authorize IO.
    pub fn recover_tool_results(
        &self,
        request_id: &str,
        worker_binding: &str,
        steps_json: &str,
    ) -> R<String> {
        if steps_json.len() > CONTINUATION_LIMIT {
            return Err(malformed());
        }
        let mut steps: Value = serde_json::from_str(steps_json).map_err(|_| malformed())?;
        let list = steps
            .as_array_mut()
            .filter(|steps| !steps.is_empty() && steps.len() <= 16)
            .ok_or_else(malformed)?;
        let e = self.lock();
        let r = e.state.requests.get(request_id).ok_or_else(stale)?;
        let (_, p) = e.tool_resolution(request_id, &r.req.content_hash)?;
        if e.me_id()? != r.req.agent
            || p.worker_binding != worker_binding
            || !e.state.tool_app_act(p, crate::engine::now_ms())
        {
            return Err(denied());
        }
        let (device, owner, space, source) =
            e.agent_binary_authority(&p.artifact, &p.source, &p.serving, false)?;
        if device != p.device || owner != p.owner || space != r.space || source != p.source_sha256 {
            return Err(denied());
        }
        let (ask, _) = parsed(p)?;
        let prefix = ask["steps"].as_array().ok_or_else(malformed)?;
        if list.len() <= prefix.len()
            || list[..prefix.len()] != prefix[..]
            || list[prefix.len()]["call"] != ask["call"]
        {
            return Err(stale());
        }
        let mut accepted_tail_heads = std::collections::BTreeSet::new();
        for (index, step) in list.iter_mut().enumerate() {
            let call = step["call"].clone();
            let name = call["name"].as_str().ok_or_else(malformed)?;
            let tool = agent_tools::tools()
                .into_iter()
                .find(|tool| {
                    agent_tools::model_name(&tool.name, &p.artifact).as_deref() == Some(name)
                })
                .ok_or_else(malformed)?
                .name;
            let args = &call["arguments"];
            if args["artifactId"].as_str() != Some(p.artifact.as_str()) {
                return Err(denied());
            }
            let (version, head, action) =
                agent_tools::command(&tool, args).map_err(|_| malformed())?;
            if step["is_error"].as_bool() == Some(true) {
                continue;
            }
            if step["is_error"] != false {
                return Err(malformed());
            }
            if let Some(action) = action {
                e.agent_binary_authority(&p.artifact, &p.source, &p.serving, true)?;
                if matches!(
                    e.decide(&r.req.agent, &r.space, &ActionClass::Reversible, 0),
                    grants::Decision::Block(_)
                ) {
                    return Err(denied());
                }
                let mut command = p.clone();
                command.canonical_tool = tool;
                command.version = version.ok_or_else(malformed)?;
                command.sha256 = head.ok_or_else(malformed)?;
                let intent = command_intent(&command, Some(&action))?;
                let accepted = e
                    .app_binary_result(
                        &p.artifact,
                        command.version,
                        &command.sha256,
                        &format!("deckcmd_{intent}"),
                        &intent,
                    )?
                    .ok_or_else(stale)?;
                if index >= prefix.len() {
                    accepted_tail_heads.insert((accepted.version, accepted.sha256.clone()));
                }
                let previous = step["result"].as_str().ok_or_else(malformed)?;
                if !previous.is_empty() {
                    let claimed: Value = serde_json::from_str(previous).map_err(|_| malformed())?;
                    if claimed["itemId"] != accepted.item_id
                        || claimed["version"] != accepted.version
                        || claimed["sha256"] != accepted.sha256
                        || claimed["authorId"] != accepted.author_id
                        || claimed["engineRevision"] != accepted.engine_revision
                    {
                        return Err(stale());
                    }
                }
                if index >= prefix.len() {
                    step["result"]=json!(json!({"itemId":accepted.item_id,"version":accepted.version,"currentVersion":accepted.current_version,"sha256":accepted.sha256,"authorId":accepted.author_id,"engineRevision":accepted.engine_revision,"replayed":true}).to_string());
                }
            } else {
                let cached: Value =
                    serde_json::from_str(step["result"].as_str().ok_or_else(malformed)?)
                        .map_err(|_| malformed())?;
                let v = cached["head"]["version"]
                    .as_str()
                    .and_then(|v| v.parse::<u32>().ok())
                    .ok_or_else(malformed)?;
                let sha = cached["head"]["sha256"].as_str().ok_or_else(malformed)?;
                if cached["scope"]
                    != json!({"artifactId":p.artifact,"actorId":r.req.agent,"actorKind":"agent"})
                    || (index == prefix.len() && (v != p.version || sha != p.sha256))
                {
                    return Err(stale());
                }
                // Authorize access to the exact historical head, without exposing
                // fresh bytes or minting a permit to execute/read/write anything.
                e.app_binary_read(&p.artifact, Some(v), Some(sha))?;
            }
        }
        let head = e.item_state(&p.artifact)?.current();
        let ItemContent::App(app) = &head.content else {
            return Err(stale());
        };
        let current_sha = &app.document.as_ref().ok_or_else(stale)?.sha256;
        if (head.number != p.version || current_sha != &p.sha256)
            && !accepted_tail_heads.contains(&(head.number, current_sha.clone()))
        {
            return Err(stale());
        }
        serde_json::to_string(&steps).map_err(|_| malformed())
    }

    pub fn approved_tool_sources(&self, space: &str) -> Vec<String> {
        let e = self.lock();
        let Ok(me) = e.me_id() else {
            return Vec::new();
        };
        e.state
            .requests
            .values()
            .filter(|r| r.space == space && r.req.agent == me && r.status == ReqStatus::Approved)
            .filter_map(|r| r.req.tool_proposal.as_ref().map(|p| p.source.clone()))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Existing canonical Requests are the only durable continuation discovery source.
    pub fn tool_approval_requests(&self, space: &str, source: &str) -> Vec<AgentRequest> {
        let e = self.lock();
        let Ok(me) = e.me_id() else {
            return Vec::new();
        };
        let mut requests: Vec<_> = e
            .state
            .requests
            .values()
            .filter(|r| {
                r.space == space
                    && r.req.agent == me
                    && r.req
                        .tool_proposal
                        .as_ref()
                        .is_some_and(|p| p.source == source)
            })
            .map(|r| r.req.clone())
            .collect();
        requests.sort_by_key(|r| e.state.tool_approval_order(r));
        requests
    }
}
