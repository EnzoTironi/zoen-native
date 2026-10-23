//! Only existing native receipts can recover a creation after an unknown outcome.
use super::*;

impl Client {
    pub fn recover_app_creation_results(&self, request: &str, steps_json: &str) -> R<String> {
        if steps_json.len() > shared::CONTINUATION_LIMIT {
            return Err(invalid());
        }
        let mut steps: Value = serde_json::from_str(steps_json).map_err(|_| invalid())?;
        let list = steps.as_array_mut().ok_or_else(invalid)?;
        let e = self.lock();
        let r = e.state.requests.get(request).ok_or_else(stale)?;
        let (_, p) = e.tool_resolution(request, &r.req.content_hash)?;
        if e.me_id()? != r.req.agent {
            return Err(stale());
        }
        let (ask, _) = parsed(p)?;
        let prefix = ask["steps"].as_array().ok_or_else(invalid)?;
        if list.len() <= prefix.len()
            || list.len() > 16
            || list[..prefix.len()] != prefix[..]
            || list[prefix.len()]["call"] != ask["call"]
            || list[prefix.len()]["is_error"] != false
        {
            return Err(stale());
        }
        let document = e.creation_result(p)?.ok_or_else(stale)?;
        validate_claim(&list[prefix.len()]["result"], &document)?;
        list[prefix.len()]["result"] = json!(execution::receipt(&document, true));
        let mut heads =
            std::collections::BTreeSet::from([(document.version, document.sha256.clone())]);
        for step in &mut list[prefix.len() + 1..] {
            if step["is_error"] == true {
                continue;
            }
            if step["is_error"] != false {
                return Err(invalid());
            }
            let call = &step["call"];
            let name = call["name"].as_str().ok_or_else(invalid)?;
            let tool = agent_tools::tools()
                .into_iter()
                .find(|tool| {
                    agent_tools::model_name(&tool.name, &p.artifact).as_deref() == Some(name)
                })
                .ok_or_else(invalid)?
                .name;
            let (version, head, action) =
                agent_tools::command(&tool, &call["arguments"]).map_err(|_| invalid())?;
            if call["arguments"]["artifactId"] != p.artifact {
                return Err(stale());
            }
            if let Some(action) = action {
                let mut command = p.clone();
                command.canonical_tool = tool;
                command.version = version.ok_or_else(invalid)?;
                command.sha256 = head.ok_or_else(invalid)?;
                let intent = shared::command_intent(&command, Some(&action))?;
                let accepted = e
                    .app_binary_result(
                        &p.artifact,
                        command.version,
                        &command.sha256,
                        &format!("deckcmd_{intent}"),
                        &intent,
                    )?
                    .ok_or_else(stale)?;
                validate_claim(&step["result"], &accepted)?;
                heads.insert((accepted.version, accepted.sha256.clone()));
                step["result"] = json!(execution::receipt(&accepted, true));
            } else {
                let cached: Value =
                    serde_json::from_str(step["result"].as_str().ok_or_else(invalid)?)
                        .map_err(|_| invalid())?;
                if cached["scope"]
                    != json!({"artifactId":p.artifact,"actorId":r.req.agent,"actorKind":"agent"})
                    || cached["engineRevision"] != p.engine_revision
                {
                    return Err(stale());
                }
                let version = cached["head"]["version"]
                    .as_str()
                    .and_then(|v| v.parse::<u32>().ok())
                    .ok_or_else(invalid)?;
                let hash = cached["head"]["sha256"].as_str().ok_or_else(invalid)?;
                e.app_binary_read(&p.artifact, Some(version), Some(hash))?;
            }
        }
        let current = e.app_binary_read(&p.artifact, None, None)?;
        if !heads.contains(&(current.version, current.sha256)) {
            return Err(stale());
        }
        serde_json::to_string(&steps).map_err(|_| invalid())
    }
}

fn validate_claim(value: &Value, document: &AppBinaryDocumentDto) -> R<()> {
    let raw = value.as_str().ok_or_else(invalid)?;
    if raw.is_empty() {
        return Ok(());
    }
    let claim: Value = serde_json::from_str(raw).map_err(|_| invalid())?;
    if claim["itemId"] != document.item_id
        || claim["version"] != document.version
        || claim["sha256"] != document.sha256
        || claim["authorId"] != document.author_id
        || claim["engineRevision"] != document.engine_revision
    {
        return Err(stale());
    }
    Ok(())
}

impl Engine {
    /// A later edit's frozen prefix may contain the earlier creation. Validate it
    /// against that task's signed approval and durable receipt, not journal text.
    pub(crate) fn recover_creation_prefix(
        &self,
        agent: &str,
        space: &str,
        source: &str,
        step: usize,
        call: &Value,
        result: &Value,
    ) -> R<()> {
        let r = self
            .state
            .requests
            .values()
            .filter(|r| {
                r.req.agent == agent
                    && r.space == space
                    && r.req.tool_proposal.as_ref().is_some_and(|p| {
                        p.schema == SCHEMA && p.source == source && p.step as usize == step
                    })
            })
            .max_by_key(|r| self.state.tool_approval_order(&r.req))
            .ok_or_else(stale)?;
        let (_, p) = self.tool_resolution(&r.req.id, &r.req.content_hash)?;
        let (ask, _) = parsed(p)?;
        if ask["call"] != *call {
            return Err(stale());
        }
        let document = self.creation_result(p)?.ok_or_else(stale)?;
        validate_claim(result, &document)
    }
}
