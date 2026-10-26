use super::*;
impl Client {
    /// Completed bytes are rederived from canonical versions/receipts, never a turn.
    pub fn recover_scene_tool_results(&self, request: &str, steps_json: &str) -> R<String> {
        if steps_json.len() > shared::CONTINUATION_LIMIT {
            return Err(invalid());
        }
        let mut value: Value = serde_json::from_str(steps_json).map_err(|_| invalid())?;
        let steps = value
            .as_array_mut()
            .filter(|s| !s.is_empty() && s.len() <= 16)
            .ok_or_else(invalid)?;
        let e = self.lock();
        let r = e.state.requests.get(request).ok_or_else(stale)?;
        let (_, p) = e.tool_resolution(request, &r.req.content_hash)?;
        let (ask, _) = parsed(p)?;
        let prefix = ask["steps"].as_array().ok_or_else(invalid)?;
        if e.me_id()? != r.req.agent
            || steps.len() <= prefix.len()
            || steps[..prefix.len()] != prefix[..]
            || steps[prefix.len()]["call"] != ask["call"]
        {
            return Err(stale());
        }
        let (device, owner, space, source) =
            e.agent_binary_authority(&p.artifact, &p.source, &p.serving, false)?;
        if device != p.device
            || owner != p.owner
            || space != r.space
            || source != p.source_sha256
            || !access(&e.state, p, crate::engine::now_ms())
        {
            return Err(stale());
        }
        for (index, step) in steps.iter_mut().enumerate() {
            let call = &step["call"];
            let name = call["name"].as_str().ok_or_else(invalid)?;
            let tool = ["read_scene", "replace_scene"]
                .into_iter()
                .find(|tool| tools::model_name(tool, &p.artifact).as_deref() == Some(name))
                .ok_or_else(invalid)?;
            let command = tools::parse(tool, &call["arguments"])?;
            if command.artifact() != p.artifact {
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
                    let cached: SceneSnapshot =
                        serde_json::from_str(encoded).map_err(|_| invalid())?;
                    if cached.actor_id != r.req.agent
                        || cached.space_id != r.space
                        || cached.artifact_id != artifact
                        || e.scene_snapshot_at(&artifact, cached.version)? != cached
                    {
                        return Err(stale());
                    }
                }
                tools::Command::Replace(_) => {
                    let (prior, signed) = e
                        .state
                        .requests
                        .values()
                        .filter_map(|prior| {
                            prior
                                .req
                                .tool_proposal
                                .as_deref()
                                .map(|signed| (prior, signed))
                        })
                        .find(|(prior, signed)| {
                            prior.req.agent == r.req.agent
                                && prior.space == r.space
                                && signed.schema == SCHEMA
                                && signed.task_sha256 == p.task_sha256
                                && signed.step as usize == index
                                && serde_json::from_str::<Value>(&signed.continuation_json)
                                    .is_ok_and(|held| held["call"] == *call)
                        })
                        .ok_or_else(stale)?;
                    e.tool_resolution(&prior.req.id, &prior.req.content_hash)?;
                    let result = e.scene_result(signed)?.ok_or_else(stale)?;
                    if !encoded.is_empty() {
                        let old: AppDocumentDto =
                            serde_json::from_str(encoded).map_err(|_| invalid())?;
                        if old.item_id != result.item_id
                            || old.version != result.version
                            || old.sha256 != result.sha256
                            || old.scene_json != result.scene_json
                        {
                            return Err(stale());
                        }
                    }
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
