//! Continuation recovery validates existing effects; it cannot execute a call.
use super::*;
impl Client {
    pub fn recover_site_tool_results(&self, request: &str, steps_json: &str) -> R<String> {
        if steps_json.len() > shared::CONTINUATION_LIMIT {
            return Err(invalid());
        }
        let mut steps: Value = serde_json::from_str(steps_json).map_err(|_| invalid())?;
        let list = steps.as_array_mut().ok_or_else(invalid)?;
        let e = self.lock();
        let r = e.state.requests.get(request).ok_or_else(stale)?;
        let p = e.checked_site_proposal(request, &r.req.content_hash, &e.me_id()?)?;
        let (ask, action) = review::parsed(&p)?;
        let prefix = ask["steps"].as_array().ok_or_else(invalid)?;
        if list.len() != prefix.len() + 1
            || list.len() > 16
            || list[..prefix.len()] != prefix[..]
            || list[prefix.len()]["call"] != ask["call"]
            || list[prefix.len()]["is_error"] != false
        {
            return Err(stale());
        }
        // Every earlier successful Site result is checked against that step's
        // canonical approved request and receipt, not trusted journal text.
        for (index, step) in list[..prefix.len()].iter().enumerate() {
            let name = step["call"]["name"].as_str().ok_or_else(invalid)?;
            if step["is_error"] == true {
                continue;
            }
            if step["is_error"] != false || !is_model_name(name) {
                return Err(stale());
            }
            e.verify_site_prefix(&r.space, &p.source, index, &step["call"], &step["result"])?;
        }
        if action.is_none() {
            let value: SiteSnapshot =
                serde_json::from_str(list[prefix.len()]["result"].as_str().ok_or_else(invalid)?)
                    .map_err(|_| invalid())?;
            if value != e.agent_site_snapshot(&p.artifact, p.version, &p.sha256)? {
                return Err(stale());
            }
        } else {
            let effect = e.site_effect(&p)?.ok_or_else(stale)?;
            let actual = e.site_tool_result(&effect, true)?;
            validate_result(&list[prefix.len()]["result"], &actual)?;
            list[prefix.len()]["result"] = json!(actual);
        }
        serde_json::to_string(&steps).map_err(|_| invalid())
    }
}
fn validate_result(claim: &Value, actual: &str) -> R<()> {
    let raw = claim.as_str().ok_or_else(invalid)?;
    if raw.is_empty() {
        return Ok(());
    }
    let mut claim: Value = serde_json::from_str(raw).map_err(|_| invalid())?;
    let mut actual: Value = serde_json::from_str(actual).map_err(|_| invalid())?;
    claim
        .as_object_mut()
        .ok_or_else(invalid)?
        .remove("replayed");
    actual
        .as_object_mut()
        .ok_or_else(invalid)?
        .remove("replayed");
    if claim != actual {
        return Err(stale());
    }
    Ok(())
}
impl Engine {
    fn verify_site_prefix(
        &self,
        space: &str,
        source: &str,
        step: usize,
        call: &Value,
        result: &Value,
    ) -> R<()> {
        let actor = self.me_id()?;
        if let Ok(Command::Read(c)) = parse("read_site", &call["arguments"]) {
            if call["name"] != model_name("read_site", &c.artifact_id).ok_or_else(invalid)? {
                return Err(stale());
            }
            let snapshot = self.agent_site_snapshot(&c.artifact_id, c.version, &c.sha256)?;
            if snapshot.space_id != space {
                return Err(stale());
            }
            let claimed: SiteSnapshot = serde_json::from_str(result.as_str().ok_or_else(invalid)?)
                .map_err(|_| invalid())?;
            if claimed != snapshot {
                return Err(stale());
            }
            return Ok(());
        }
        let r = self
            .state
            .requests
            .values()
            .filter(|r| {
                r.req.agent == actor
                    && r.space == space
                    && r.req.tool_proposal.as_ref().is_some_and(|p| {
                        p.schema == SCHEMA
                            && p.source == source
                            && usize::try_from(p.step).ok() == Some(step)
                    })
            })
            .max_by_key(|r| self.state.tool_approval_order(&r.req))
            .ok_or_else(stale)?;
        let p = self.checked_site_proposal(&r.req.id, &r.req.content_hash, &actor)?;
        let (ask, action) = review::parsed(&p)?;
        if ask["call"] != *call {
            return Err(stale());
        }
        if action.is_some() {
            let effect = self.site_effect(&p)?.ok_or_else(stale)?;
            validate_result(result, &self.site_tool_result(&effect, true)?)?;
        } else {
            let s: SiteSnapshot = serde_json::from_str(result.as_str().ok_or_else(invalid)?)
                .map_err(|_| invalid())?;
            if s != self.agent_site_snapshot(&p.artifact, p.version, &p.sha256)? {
                return Err(stale());
            }
        }
        Ok(())
    }
}
