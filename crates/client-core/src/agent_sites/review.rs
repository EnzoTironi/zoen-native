//! The unchanged signed Request/ToolProposal envelope owns review and consent.
use super::*;
use crate::{AgentRequestDto, AgentToolApprovalInput};
use types::EventBody;

fn creation_artifact(space: &str, args: &Value) -> R<String> {
    Ok(format!(
        "future_site_{}",
        sha(&serde_json::to_vec(&(REVISION, space, args)).map_err(|_| invalid())?)
    ))
}
pub(crate) fn parsed(p: &ToolProposal) -> R<(Value, Option<Value>)> {
    if p.schema != SCHEMA
        || p.engine_revision != REVISION
        || p.worker_binding != executor_binding()
        || !digest(&p.source_sha256)
        || !digest(&p.task_sha256)
        || !digest(&p.sha256)
        || p.version == 0
        || p.step >= 16
        || p.arguments_json.len() > ARGUMENT_LIMIT
        || p.continuation_json.len() > shared::CONTINUATION_LIMIT
        || p.gate_rule.is_empty()
        || p.gate_rule.len() > 256
    {
        return Err(invalid());
    }
    let args: Value = serde_json::from_str(&p.arguments_json).map_err(|_| invalid())?;
    if miniapps::deckcraft::agent_tools::canonical_json(&args).map_err(|_| invalid())?
        != p.arguments_json
    {
        return Err(invalid());
    }
    let command = parse(&p.canonical_tool, &args)?;
    let ask: Value = serde_json::from_str(&p.continuation_json).map_err(|_| invalid())?;
    let space = ask["envelope"]["space"]
        .as_str()
        .filter(|s| token(s))
        .ok_or_else(invalid)?;
    match &command {
        Command::Create(c)
            if p.version == 1
                && p.sha256 == sha(c.html.as_bytes())
                && p.artifact == creation_artifact(space, &args)? => {}
        Command::Read(c)
            if c.artifact_id == p.artifact && c.version == p.version && c.sha256 == p.sha256 => {}
        Command::Replace(c)
            if c.artifact_id == p.artifact && c.version == p.version && c.sha256 == p.sha256 => {}
        _ => return Err(invalid()),
    }
    let locator = if command.artifact().is_none() {
        space
    } else {
        &p.artifact
    };
    if ask["version"] != 1
        || ask["rule"] != p.gate_rule
        || ask["policy_sha256"].as_str().is_none_or(|v| !digest(v))
        || ask["steps"]
            .as_array()
            .is_none_or(|v| v.len() != p.step as usize)
        || ask["remaining"].as_array().is_none_or(|v| !v.is_empty())
        || ask["round"].as_u64().is_none_or(|v| v >= 4)
        || ask["call"]["name"] != model_name(&p.canonical_tool, locator).ok_or_else(invalid)?
        || ask["call"]["id"]
            .as_str()
            .is_none_or(|v| v.is_empty() || v.len() > 128)
        || ask["call"]["arguments"] != args
        || ask["definition"]
            != if command.artifact().is_none() {
                contract::definition(&p.canonical_tool, locator)?
            } else {
                contract::selected_definition(&p.canonical_tool, locator, p.version, &p.sha256)?
            }
    {
        return Err(invalid());
    }
    Ok((ask, command.is_write().then_some(args)))
}
pub(super) fn intent(p: &ToolProposal) -> R<String> {
    let (ask, _) = parsed(p)?;
    Ok(sha(&serde_json::to_vec(&(
        REVISION,
        &ask["envelope"],
        &p.source,
        &p.source_sha256,
        p.step,
        &p.canonical_tool,
        &p.arguments_json,
    ))
    .map_err(|_| invalid())?))
}
pub(crate) fn access(state: &State, p: &ToolProposal) -> bool {
    let Ok((ask, _)) = parsed(p) else {
        return false;
    };
    let Some(space) = ask["envelope"]["space"].as_str() else {
        return false;
    };
    state.writer(space, &p.owner) && state.writer(space, &p.serving)
}
pub(crate) fn matches(state: &State, req: &AgentRequest, space: &str) -> bool {
    let Some(p) = req.tool_proposal.as_deref() else {
        return false;
    };
    let Ok((ask, _)) = parsed(p) else {
        return false;
    };
    let Some((source, inbound)) = shared::source_hash(state, space, &p.source, &p.serving) else {
        return false;
    };
    let Ok(args) = serde_json::from_str(&p.arguments_json) else {
        return false;
    };
    let Ok(command) = parse(&p.canonical_tool, &args) else {
        return false;
    };
    let current=match command{
        Command::Create(_)=>!state.items.contains_key(&p.artifact),
        Command::Read(_)|Command::Replace(_)=>state.items.get(&p.artifact).is_some_and(|it|{
            it.kind==ItemKind::File && it.space==space && !state.is_recovery_item(&it.id)
                && it.current().number==p.version && matches!(&it.current().content,ItemContent::File(f) if f.mime=="text/html"&&f.sha256==p.sha256&&f.bytes>0&&f.bytes<=HTML_LIMIT as u64)
        }),
    };
    current
        && req.item.is_none()
        && req.line.is_none()
        && req.proposal.is_none()
        && req.action == ActionClass::Reversible
        && state.writer(space, &req.agent)
        && access(state, p)
        && p.serving != req.agent
        && source == p.source_sha256
        && req.tool_proposal_hash(space).as_deref() == Some(&req.content_hash)
        && p.task_sha256
            == sha(
                &serde_json::to_vec(&("zoen.tool-task.v1", space, &req.agent, &p.source))
                    .unwrap_or_default(),
            )
        && ask["envelope"]
            == json!({"agent":req.agent,"device":p.device,"owner":p.owner,"space":space,"from":p.serving,"inbound_sha256":inbound})
}
impl Client {
    pub fn stage_site_tool_approval(
        &self,
        space: &str,
        source: &str,
        serving: &str,
        input: AgentToolApprovalInput,
    ) -> R<AgentRequestDto> {
        let mut e = self.lock();
        let command = parse(&input.canonical_tool, &input.arguments)?;
        let (device, owner, _, source_sha256) =
            e.agent_site_authority(space, source, serving, command.is_write())?;
        if input.worker_binding != executor_binding() {
            return Err(invalid());
        }
        let actor = e.me_id()?;
        let (artifact, version, hash) = match &command {
            Command::Create(c) => (
                creation_artifact(space, &input.arguments)?,
                1,
                sha(c.html.as_bytes()),
            ),
            Command::Read(c) => {
                let s = e.agent_site_snapshot(&c.artifact_id, c.version, &c.sha256)?;
                if s.space_id != space {
                    return Err(stale());
                }
                (c.artifact_id.clone(), c.version, c.sha256.clone())
            }
            Command::Replace(c) => {
                let s = e.agent_site_snapshot(&c.artifact_id, c.version, &c.sha256)?;
                if s.space_id != space
                    || !e.file_edit_allowed(&c.artifact_id, c.version, &c.sha256)?
                    || s.html == c.html
                {
                    return Err(stale());
                }
                (c.artifact_id.clone(), c.version, c.sha256.clone())
            }
        };
        let ask: Value = serde_json::from_str(&input.continuation_json).map_err(|_| invalid())?;
        let p = ToolProposal {
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
                .map(|v| v.len() as u32)
                .ok_or_else(invalid)?,
            artifact,
            version,
            sha256: hash,
            canonical_tool: input.canonical_tool,
            arguments_json: miniapps::deckcraft::agent_tools::canonical_json(&input.arguments)
                .map_err(|_| invalid())?,
            worker_binding: input.worker_binding,
            engine_revision: REVISION.into(),
            continuation_json: input.continuation_json,
            expires_at_ms: crate::engine::now_ms()
                .checked_add(15 * 60 * 1000)
                .ok_or_else(invalid)?,
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
                .max_by_key(|r| e.state.tool_approval_order(&r.req))
        {
            let old = r.req.tool_proposal.as_ref().ok_or_else(stale)?;
            let mut same = p.clone();
            same.expires_at_ms = old.expires_at_ms;
            same.review = old.review.clone();
            if old.as_ref() != &same {
                return Err(stale());
            }
            return Ok(e.request_dto(r));
        }
        let label = match command {
            Command::Create(_) => crate::i18n::t("Criar site", "Create Site"),
            Command::Read(_) => crate::i18n::t("Ler site", "Read Site"),
            Command::Replace(_) => crate::i18n::t("Atualizar site", "Update Site"),
        };
        let mut request = AgentRequest {
            id: types::new_id("rq"),
            agent: actor.clone(),
            title: label.clone(),
            detail: label,
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
