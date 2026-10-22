//! A held tool is an existing signed Request, never an independent permission store.
use super::*;
use crate::{
    dto::*,
    engine::{ReqStatus, State},
};
use miniapps::deckcraft::{self, agent_tools};
use serde_json::{json, Value};
use types::{
    ActionClass, AgentRequest, Capability, EventBody, GrantScope, RequestResolution, ToolDecision,
    ToolProposal, TrustLevel,
};

const CONTINUATION_LIMIT: usize = 128 * 1024;
const REQUEST_LIFETIME_MS: i64 = 15 * 60 * 1000;
fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn stale() -> CoreError {
    CoreError::Stale {
        reason: "The reviewed action, source or access changed. Request a new proposal.".into(),
    }
}
fn malformed() -> CoreError {
    CoreError::Invalid {
        reason: "Unsupported or invalid reviewed presentation action.".into(),
    }
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Rust-only input from the trusted host after a Gate Ask. Locators are checked against
/// the actual signed native actor/source; neither UI nor model can call this port.
pub struct AgentToolApprovalInput {
    pub canonical_tool: String,
    pub arguments: Value,
    pub worker_binding: String,
    pub continuation_json: String,
    pub gate_rule: String,
}
/// Not serializable or FFI-exported. Its embedded turn rechecks the canonical approval
/// and exact command intent at every read/result/save boundary.
pub struct ApprovedToolTurn {
    turn: AgentBinaryTurn,
    proposal: ToolProposal,
}
impl ApprovedToolTurn {
    pub fn turn(&self) -> &AgentBinaryTurn {
        &self.turn
    }
    pub fn proposal(&self) -> &ToolProposal {
        &self.proposal
    }
}
#[derive(Clone)]
pub(super) struct ApprovalBinding {
    request: String,
    content_hash: String,
}

fn parsed(p: &ToolProposal) -> R<(Value, Option<Value>)> {
    if p.schema != 1
        || p.engine_revision != deckcraft::ENGINE_REVISION
        || !digest(&p.worker_binding)
        || !digest(&p.source_sha256)
        || !digest(&p.task_sha256)
        || !digest(&p.sha256)
        || p.version == 0
        || p.step >= 16
        || p.arguments_json.len() > 32 * 1024
        || p.continuation_json.len() > CONTINUATION_LIMIT
        || p.gate_rule.is_empty()
        || p.gate_rule.len() > 256
    {
        return Err(malformed());
    }
    let args: Value = serde_json::from_str(&p.arguments_json).map_err(|_| malformed())?;
    if agent_tools::canonical_json(&args).map_err(|_| malformed())? != p.arguments_json
        || args["artifactId"].as_str() != Some(p.artifact.as_str())
    {
        return Err(malformed());
    }
    let (version, head, command) =
        agent_tools::command(&p.canonical_tool, &args).map_err(|_| malformed())?;
    if version.is_some_and(|v| v != p.version) || head.as_ref().is_some_and(|h| h != &p.sha256) {
        return Err(malformed());
    }
    let ask: Value = serde_json::from_str(&p.continuation_json).map_err(|_| malformed())?;
    let call = &ask["call"];
    if ask["policy_sha256"].as_str().is_none_or(|fp| !digest(fp))
        || ask["version"] != 1
        || ask["rule"].as_str() != Some(p.gate_rule.as_str())
        || ask["steps"]
            .as_array()
            .is_none_or(|steps| steps.len() != p.step as usize)
        || ask["remaining"]
            .as_array()
            .is_none_or(|calls| calls.len() > 3)
        || ask["round"].as_u64().is_none_or(|round| round >= 4)
        || call["name"].as_str()
            != agent_tools::model_name(&p.canonical_tool, &p.artifact).as_deref()
        || call["id"]
            .as_str()
            .is_none_or(|id| id.is_empty() || id.len() > 128)
        || call["arguments"] != args
        || ask["definition"]["name"] != call["name"]
    {
        return Err(malformed());
    }
    Ok((ask, command))
}
fn source_hash(
    state: &State,
    space: &str,
    source: &str,
    serving: &str,
) -> Option<(String, String)> {
    let entry = state
        .spaces
        .get(space)?
        .entries
        .iter()
        .find(|entry| entry.id() == source)?;
    let EntryBody::Message {
        text,
        attaches,
        reply,
        marks,
    } = &entry.body
    else {
        return None;
    };
    if entry.author != serving || marks.deleted || text.trim().is_empty() {
        return None;
    }
    let bytes = serde_json::to_vec(&(entry.id(), &entry.author, text, attaches, reply)).ok()?;
    Some((sha(&bytes), sha(text.as_bytes())))
}
impl State {
    pub(crate) fn tool_request_matches(&self, req: &AgentRequest, space: &str) -> bool {
        let Some(p) = &req.tool_proposal else {
            return false;
        };
        let Ok((ask, _)) = parsed(p) else {
            return false;
        };
        let Some((source_sha, inbound_sha)) = source_hash(self, space, &p.source, &p.serving)
        else {
            return false;
        };
        let Some(item) = self.items.get(&p.artifact) else {
            return false;
        };
        let ItemContent::App(app) = &item.current().content else {
            return false;
        };
        req.proposal.is_none()
            && req.item.is_none()
            && req.line.is_none()
            && req.action == ActionClass::Reversible
            && self.writer(space, &req.agent)
            && self.writer(space, &p.owner)
            && self.writer(space, &p.serving)
            && p.serving != req.agent
            && item.space == space
            && item.current().number == p.version
            && app.app == deckcraft::APP
            && app.resource_uri == deckcraft::RESOURCE
            && app.document.as_ref().is_some_and(|file| {
                file.sha256 == p.sha256 && file.bytes <= AGENT_BINARY_DOCUMENT_LIMIT as u64
            })
            && source_sha == p.source_sha256
            && req.tool_proposal_hash(space).as_deref() == Some(req.content_hash.as_str())
            && p.task_sha256
                == sha(
                    &serde_json::to_vec(&("zoen.tool-task.v1", space, &req.agent, &p.source))
                        .unwrap_or_default(),
                )
            && ask["envelope"]
                == json!({"agent":req.agent,"device":p.device,"owner":p.owner,"space":space,"from":p.serving,"inbound_sha256":inbound_sha})
    }
    pub(crate) fn tool_app_act(&self, p: &ToolProposal, at: i64) -> bool {
        self.grants
            .iter()
            .rev()
            .filter(|g| {
                !g.revoked
                    && g.grant.scope == GrantScope::Item(p.artifact.clone())
                    && g.grant.grantee.as_deref() == Some(format!("app:{}", p.artifact).as_str())
                    && g.grant.expires_at_ms.is_none_or(|expiry| expiry > at)
            })
            .find_map(|g| {
                if let Capability::Trust(level) = g.grant.capability {
                    Some(level)
                } else {
                    None
                }
            })
            .is_some_and(|level| level >= TrustLevel::Act)
    }
}
impl Engine {
    fn tool_non_blocked(&self, req: &AgentRequest) -> bool {
        let Some(p) = &req.tool_proposal else {
            return false;
        };
        let action = if p.canonical_tool == "read_deckcraft" {
            ActionClass::Reply
        } else {
            ActionClass::Reversible
        };
        !matches!(
            self.decide(
                &req.agent,
                self.state
                    .requests
                    .get(&req.id)
                    .map(|r| r.space.as_str())
                    .unwrap_or(""),
                &action,
                0
            ),
            grants::Decision::Block(_)
        )
    }
    fn tool_resolution(
        &self,
        id: &str,
        hash: &str,
    ) -> R<(&crate::engine::RequestState, &ToolProposal)> {
        let r = self.state.requests.get(id).ok_or_else(stale)?;
        let p = r.req.tool_proposal.as_deref().ok_or_else(malformed)?;
        if r.status != ReqStatus::Approved
            || r.req.content_hash != hash
            || r.resolution.as_ref().and_then(|r| r.tool_decision)
                != Some(ToolDecision::ApproveOnce)
            || r.req.tool_proposal_hash(&r.space).as_deref() != Some(hash)
            || parsed(p).is_err()
            || !self.tool_non_blocked(&r.req)
        {
            return Err(denied());
        }
        Ok((r, p))
    }
    pub(super) fn recheck_tool_turn(
        &self,
        turn: &AgentBinaryTurn,
        mutation: Option<(&str, &str)>,
    ) -> R<()> {
        let Some(binding) = &turn.approval else {
            return Ok(());
        };
        let (r, p) = self.tool_resolution(&binding.request, &binding.content_hash)?;
        if p.device != turn.device
            || p.owner != turn.owner
            || p.artifact != turn.document.item_id
            || p.source != turn.source
            || p.serving != turn.serving
            || p.source_sha256 != turn.source_sha256
            || p.version != turn.document.version
            || p.sha256 != turn.document.sha256
            || !self.state.tool_app_act(p, crate::engine::now_ms())
        {
            return Err(denied());
        }
        let (_, action) = parsed(p)?;
        if mutation.is_some() && action.is_none() {
            return Err(denied());
        }
        let intent = command_intent(p, action.as_ref())?;
        if mutation.is_some_and(|(id, hash)| id != format!("deckcmd_{intent}") || hash != intent) {
            return Err(denied());
        }
        // Accepted-result recovery is read-only and uses the existing durable receipt.
        // It does not authorize fresh compute against an old head or expired Ask.
        if action.is_some()
            && self
                .app_binary_result(
                    &p.artifact,
                    p.version,
                    &p.sha256,
                    &format!("deckcmd_{intent}"),
                    &intent,
                )?
                .is_some()
        {
            return Ok(());
        }
        if p.expires_at_ms <= crate::engine::now_ms()
            || !self.state.tool_request_matches(&r.req, &r.space)
        {
            return Err(stale());
        }
        Ok(())
    }
    pub(crate) fn decide_tool_request(
        &mut self,
        id: &str,
        observed: &str,
        decision: ToolRequestDecision,
    ) -> R<DecideOutcome> {
        let r = self.state.requests.get(id).cloned().ok_or_else(stale)?;
        let p = r.req.tool_proposal.as_ref().ok_or_else(malformed)?;
        let me = self.me_id()?;
        if me != p.owner
            || self
                .identities
                .get(&r.req.agent)
                .and_then(|a| a.owner.as_deref())
                != Some(me.as_str())
            || !self.state.writer(&r.space, &me)
            || observed != r.req.content_hash
        {
            return Err(denied());
        }
        let decision = match decision {
            ToolRequestDecision::ApproveOnce => ToolDecision::ApproveOnce,
            ToolRequestDecision::Deny => ToolDecision::Deny,
            ToolRequestDecision::Cancel => ToolDecision::Cancel,
        };
        if r.status != ReqStatus::Pending {
            if r.resolution.as_ref().and_then(|r| r.tool_decision) == Some(decision) {
                return Ok(DecideOutcome {
                    request: self.request_dto(&r),
                    message: r.resolution.as_ref().unwrap().message.clone(),
                    standing_grant_id: None,
                    also_resolved: 0,
                });
            }
            return Err(stale());
        }
        let approved = decision == ToolDecision::ApproveOnce;
        if approved
            && (p.expires_at_ms <= crate::engine::now_ms()
                || !self.state.tool_request_matches(&r.req, &r.space)
                || !self.state.tool_app_act(p, crate::engine::now_ms())
                || !self.tool_non_blocked(&r.req))
        {
            return Err(stale());
        }
        let message = match decision {
            ToolDecision::ApproveOnce => "Approved once. Waiting for the reviewed action.",
            ToolDecision::Deny => "Action denied.",
            ToolDecision::Cancel => "Action cancelled.",
        }
        .to_string();
        self.append(
            &r.space,
            &me,
            EventBody::RequestResolved {
                request: id.into(),
                approved,
                content_hash: observed.into(),
                resolution: Some(RequestResolution {
                    message: message.clone(),
                    created_item: None,
                    ai_cost_cents: 0,
                    standing_grant: None,
                    tool_decision: Some(decision),
                }),
            },
        )?;
        Ok(DecideOutcome {
            request: self.request_dto(&self.state.requests[id]),
            message,
            standing_grant_id: None,
            also_resolved: 0,
        })
    }
}
fn command_intent(p: &ToolProposal, action: Option<&Value>) -> R<String> {
    Ok(sha(&serde_json::to_vec(&(
        &p.artifact,
        &p.source,
        &p.serving,
        &p.source_sha256,
        p.version,
        &p.sha256,
        &p.engine_revision,
        &p.canonical_tool,
        action,
    ))
    .map_err(|_| malformed())?))
}
impl Client {
    pub fn stage_tool_approval(
        &self,
        turn: &AgentBinaryTurn,
        input: AgentToolApprovalInput,
    ) -> R<AgentRequestDto> {
        let mut e = self.lock();
        e.recheck_agent_binary_turn(turn, false)?;
        let (version, head, _) = agent_tools::command(&input.canonical_tool, &input.arguments)
            .map_err(|_| malformed())?;
        if version.is_some_and(|v| v != turn.document.version)
            || head.as_ref().is_some_and(|h| h != &turn.document.sha256)
        {
            return Err(stale());
        }
        let ask: Value = serde_json::from_str(&input.continuation_json).map_err(|_| malformed())?;
        let p = ToolProposal {
            schema: 1,
            source: turn.source.clone(),
            serving: turn.serving.clone(),
            source_sha256: turn.source_sha256.clone(),
            device: turn.device.clone(),
            owner: turn.owner.clone(),
            task_sha256: sha(&serde_json::to_vec(&(
                "zoen.tool-task.v1",
                &turn.space,
                &turn.document.actor_id,
                &turn.source,
            ))
            .map_err(|_| malformed())?),
            step: ask["steps"]
                .as_array()
                .map(|a| a.len() as u32)
                .ok_or_else(malformed)?,
            artifact: turn.document.item_id.clone(),
            version: turn.document.version,
            sha256: turn.document.sha256.clone(),
            canonical_tool: input.canonical_tool,
            arguments_json: agent_tools::canonical_json(&input.arguments)
                .map_err(|_| malformed())?,
            worker_binding: input.worker_binding,
            engine_revision: turn.document.engine_revision.clone(),
            continuation_json: input.continuation_json,
            expires_at_ms: crate::engine::now_ms() + REQUEST_LIFETIME_MS,
            gate_rule: input.gate_rule,
        };
        parsed(&p)?;
        // A stable source/step cannot mint a second intent after approval or restart.
        if let Some(r) = e.state.requests.values().find(|r| {
            r.req.agent == turn.document.actor_id
                && r.req
                    .tool_proposal
                    .as_ref()
                    .is_some_and(|old| old.task_sha256 == p.task_sha256 && old.step == p.step)
        }) {
            let mut same = p.clone();
            same.expires_at_ms = r.req.tool_proposal.as_ref().unwrap().expires_at_ms;
            if r.req.tool_proposal.as_deref() != Some(&same) {
                return Err(stale());
            }
            return Ok(e.request_dto(r));
        }
        let operation = operation(&p.canonical_tool);
        let artifact_title = e.item_state(&p.artifact)?.current().content.title();
        let detail = review_detail(&p, &artifact_title);
        let mut req = AgentRequest {
            id: types::new_id("rq"),
            agent: turn.document.actor_id.clone(),
            title: operation,
            detail,
            audience: "This conversation".into(),
            action: ActionClass::Reversible,
            content_hash: String::new(),
            item: None,
            line: None,
            proposal: None,
            tool_proposal: Some(Box::new(p)),
        };
        req.content_hash = req.tool_proposal_hash(&turn.space).ok_or_else(malformed)?;
        if !e.state.tool_request_matches(&req, &turn.space)
            || !e
                .state
                .tool_app_act(req.tool_proposal.as_ref().unwrap(), crate::engine::now_ms())
        {
            return Err(stale());
        }
        // Core Block remains non-overridable. No budget or standing grant is added.
        let action = if req.tool_proposal.as_ref().unwrap().canonical_tool == "read_deckcraft" {
            ActionClass::Reply
        } else {
            ActionClass::Reversible
        };
        if matches!(
            e.decide(&req.agent, &turn.space, &action, 0),
            grants::Decision::Block(_)
        ) {
            return Err(denied());
        }
        let id = req.id.clone();
        e.append(
            &turn.space,
            &req.agent.clone(),
            EventBody::RequestOpened { request: req },
        )?;
        Ok(e.request_dto(&e.state.requests[&id]))
    }
    pub fn approved_tool_turn(
        &self,
        request_id: &str,
        worker_binding: &str,
    ) -> R<ApprovedToolTurn> {
        let (proposal, hash) = {
            let e = self.lock();
            let r = e.state.requests.get(request_id).ok_or_else(stale)?;
            let (_, p) = e.tool_resolution(request_id, &r.req.content_hash)?;
            if p.worker_binding != worker_binding || r.req.agent != e.me_id()? {
                return Err(denied());
            }
            (p.clone(), r.req.content_hash.clone())
        };
        let mut turn = self.agent_binary_turn(
            &proposal.artifact,
            &proposal.source,
            &proposal.serving,
            Some(proposal.version),
            Some(&proposal.sha256),
        )?;
        turn.approval = Some(ApprovalBinding {
            request: request_id.into(),
            content_hash: hash,
        });
        {
            let e = self.lock();
            e.recheck_agent_binary_turn(&turn, false)?;
            e.recheck_tool_turn(&turn, None)?;
        }
        Ok(ApprovedToolTurn { turn, proposal })
    }
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
        requests.sort_by_key(|r| r.tool_proposal.as_ref().map_or(0, |p| p.step));
        requests
    }
}
pub(crate) fn operation(tool: &str) -> String {
    use crate::i18n::t;
    match tool {
        "read_deckcraft" => t("Ler apresentação", "Read presentation"),
        "deckcraft_text_set" => t("Substituir texto", "Replace shape text"),
        "deckcraft_slide_notes" => t("Atualizar notas do slide", "Update slide notes"),
        "deckcraft_shape_insert" => t("Inserir uma forma", "Insert a shape"),
        "deckcraft_slide_new" => t("Inserir um slide", "Insert a slide"),
        _ => t("Ação incompatível", "Unsupported action"),
    }
}
fn review_detail(p: &ToolProposal, title: &str) -> String {
    let args: Value = serde_json::from_str(&p.arguments_json).unwrap_or(Value::Null);
    let heading = format!("{} · v{}", title, p.version);
    match p.canonical_tool.as_str() {
        "read_deckcraft" => heading,
        "deckcraft_slide_new" => format!(
            "{}\n{}\n{}",
            heading,
            args["title"].as_str().unwrap_or_default(),
            args["body"].as_str().unwrap_or_default()
        ),
        _ => format!("{}\n{}", heading, args["text"].as_str().unwrap_or_default()),
    }
}

#[cfg(test)]
mod tests;
