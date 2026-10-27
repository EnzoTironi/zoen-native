//! Explicit image generation in the existing signed request/ApproveOnce flow.
//! Rust-only: a browser or model payload cannot supply an execution capability.
use crate::{
    agent_binary_documents::tool_approvals as shared,
    engine::{Engine, State, R},
    AgentRequestDto, Client, CoreError,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use types::{ActionClass, AgentRequest, Capability, EventBody, IdentityKind, ToolProposal};

mod review;

pub const REVISION: &str = "zoen-image-task/1";
pub const SCHEMA: u8 = 4;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ImageTaskScope {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub space_id: String,
}

/// Exact host-configured obligation reviewed by the owner. No credentials.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ImageTaskDefinition {
    pub version: String,
    pub price_digest: String,
    pub size: String,
    pub quality: String,
    pub background: String,
    // Input is validated to at most 12MiB, exactly representable by JavaScript.
    #[cfg_attr(feature = "client-types", ts(type = "number"))]
    pub max_image_bytes: u64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ImageTaskInput {
    pub prompt: String,
    pub definition: ImageTaskDefinition,
}
impl std::fmt::Debug for ImageTaskInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ImageTaskInput { redacted }")
    }
}
fn denied() -> CoreError {
    CoreError::Forbidden {
        reason: "The reviewed image task or conversation access changed.".into(),
    }
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: "Unsupported image generation task.".into(),
    }
}
fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
fn canonical(value: &Value) -> R<String> {
    miniapps::deckcraft::agent_tools::canonical_json(value).map_err(|_| invalid())
}

impl ImageTaskInput {
    pub(crate) fn validate(&self) -> R<()> {
        let d = &self.definition;
        if self.prompt.trim().is_empty()
            || self.prompt.contains('\0')
            || self.prompt.chars().count() > 32_000
            || d.version.is_empty()
            || d.version.len() > 128
            || !d
                .version
                .bytes()
                .all(|v| v.is_ascii_alphanumeric() || matches!(v, b'-' | b'_' | b'.' | b'/'))
            || !digest(&d.price_digest)
            || !matches!(d.size.as_str(), "1024x1024" | "1536x1024" | "1024x1536")
            || !matches!(d.quality.as_str(), "low" | "medium" | "high")
            || !matches!(d.background.as_str(), "opaque" | "transparent")
            || !(1..=12 * 1024 * 1024).contains(&d.max_image_bytes)
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn binding(&self) -> R<String> {
        self.validate()?;
        Ok(sha(
            &serde_json::to_vec(&(REVISION, &self.definition)).map_err(|_| invalid())?
        ))
    }
}

pub(crate) fn parsed(p: &ToolProposal) -> R<(Value, Option<Value>)> {
    if p.schema != SCHEMA
        || p.engine_revision != REVISION
        || p.canonical_tool != "generate_image"
        || p.version != 1
        || p.step != 0
        || p.review.is_some()
        || p.gate_rule != "image.generate.reviewed.v1"
        || !digest(&p.source_sha256)
        || !digest(&p.task_sha256)
        || p.artifact != format!("image_{}", p.task_sha256)
        || p.arguments_json.len() > 256 * 1024
        || p.continuation_json.len() > 4096
        || p.sha256 != sha(p.arguments_json.as_bytes())
    {
        return Err(invalid());
    }
    let input: ImageTaskInput = serde_json::from_str(&p.arguments_json).map_err(|_| invalid())?;
    input.validate()?;
    let args = serde_json::to_value(&input).map_err(|_| invalid())?;
    if canonical(&args)? != p.arguments_json || p.worker_binding != input.binding()? {
        return Err(invalid());
    }
    let ask: Value = serde_json::from_str(&p.continuation_json).map_err(|_| invalid())?;
    if canonical(&ask)? != p.continuation_json
        || ask["version"] != 1
        || ask["operation"] != "generate_image"
    {
        return Err(invalid());
    }
    Ok((ask, Some(args)))
}

pub(crate) fn matches(state: &State, req: &AgentRequest, space: &str) -> bool {
    let Some(p) = req.tool_proposal.as_deref() else {
        return false;
    };
    let Ok((ask, _)) = parsed(p) else {
        return false;
    };
    let Some((source_sha, inbound_sha)) = shared::source_hash(state, space, &p.source, &p.serving)
    else {
        return false;
    };
    let Ok(task_bytes) = serde_json::to_vec(&(REVISION, space, &req.agent, &p.source)) else {
        return false;
    };
    req.proposal.is_none()
        && req.item.is_none()
        && req.line.is_none()
        && req.action == ActionClass::Reversible
        && p.owner == p.serving
        && req.agent != p.owner
        && state.writer(space, &req.agent)
        && state.writer(space, &p.owner)
        && !state.items.contains_key(&p.artifact)
        && source_sha == p.source_sha256
        && req.tool_proposal_hash(space).as_deref() == Some(req.content_hash.as_str())
        && p.task_sha256 == sha(&task_bytes)
        && ask
            == json!({"version":1,"operation":"generate_image","agent":req.agent,"device":p.device,
            "owner":p.owner,"space":space,"source":p.source,"inbound_sha256":inbound_sha})
}
pub(crate) fn access(state: &State, p: &ToolProposal) -> bool {
    let Ok((ask, _)) = parsed(p) else {
        return false;
    };
    let Some(space) = ask["space"].as_str() else {
        return false;
    };
    state.writer(space, &p.owner) && !state.items.contains_key(&p.artifact)
}

impl Engine {
    pub(crate) fn image_task_trust_deadline(
        &self,
        agent: &str,
        owner: &str,
        space: &str,
        at_ms: i64,
    ) -> R<Option<i64>> {
        // Shared with native replies: the newest signed baseline cannot revive
        // an earlier grant. A paid task never inherits a presentation default.
        let grant = self
            .state
            .latest_trust(space, owner, agent)
            .ok_or_else(denied)?;
        let Capability::Trust(level) = grant.grant.capability else {
            return Err(denied());
        };
        if grant.revoked
            || grant
                .grant
                .expires_at_ms
                .is_some_and(|expiry| expiry <= at_ms)
            || matches!(
                grants::evaluate_permission(level, &ActionClass::Reversible, &self.policy),
                grants::Decision::Block(_)
            )
        {
            return Err(denied());
        }
        Ok(grant.grant.expires_at_ms)
    }
    pub(crate) fn image_task_actor(
        &self,
        space: &str,
        source: &str,
        serving: &str,
    ) -> R<(String, String, String)> {
        self.image_task_actor_at(space, source, serving, crate::engine::now_ms())
    }
    pub(crate) fn image_task_actor_at(
        &self,
        space: &str,
        source: &str,
        serving: &str,
        at_ms: i64,
    ) -> R<(String, String, String)> {
        if self.artifact_request(space, source)?.is_some() {
            return Err(denied());
        }
        let actor = self.me_id()?;
        let account = self.account().ok_or_else(denied)?;
        let identity = self.identities.get(&actor).ok_or_else(denied)?;
        let owner = identity.owner.as_deref().ok_or_else(denied)?;
        self.image_task_trust_deadline(&actor, owner, space, at_ms)?;
        if identity.kind != IdentityKind::Agent
            || !event_log::agent_owner::profile_authorized(identity)
            || account.identity != actor
            || account.device.is_empty()
            || serving != owner
            || !self.state.writer(space, &actor)
            || !self.state.writer(space, owner)
            || matches!(
                self.decide(&actor, space, &ActionClass::Reversible, 0),
                grants::Decision::Block(_)
            )
        {
            return Err(denied());
        }
        let (source_sha, _) =
            shared::source_hash(&self.state, space, source, serving).ok_or_else(denied)?;
        Ok((actor, account.device.clone(), source_sha))
    }
}

impl Engine {
    /// Opens an explicit review, never a provider send or a financial reservation.
    pub(crate) fn stage_image_generation(
        &mut self,
        space: &str,
        source: &str,
        serving: &str,
        input: ImageTaskInput,
    ) -> R<AgentRequestDto> {
        input.validate()?;
        let e = self;
        let (actor, device, source_sha) = e.image_task_actor(space, source, serving)?;
        let (_, inbound_sha) =
            shared::source_hash(&e.state, space, source, serving).ok_or_else(denied)?;
        let task =
            sha(&serde_json::to_vec(&(REVISION, space, &actor, source)).map_err(|_| invalid())?);
        let args = canonical(&serde_json::to_value(&input).map_err(|_| invalid())?)?;
        let mut proposal = ToolProposal {
            schema: SCHEMA,
            source: source.into(),
            serving: serving.into(),
            source_sha256: source_sha,
            device,
            owner: serving.into(),
            task_sha256: task.clone(),
            step: 0,
            artifact: format!("image_{task}"),
            version: 1,
            sha256: sha(args.as_bytes()),
            canonical_tool: "generate_image".into(),
            arguments_json: args,
            worker_binding: input.binding()?,
            engine_revision: REVISION.into(),
            continuation_json: String::new(),
            expires_at_ms: crate::engine::now_ms() + 15 * 60 * 1000,
            gate_rule: "image.generate.reviewed.v1".into(),
            review: None,
        };
        proposal.continuation_json = canonical(
            &json!({"version":1,"operation":"generate_image","agent":actor,
            "device":proposal.device,"owner":proposal.owner,"space":space,"source":source,"inbound_sha256":inbound_sha}),
        )?;
        if let Some((old, signed)) = e.state.requests.values().find_map(|r| {
            let signed = r.req.tool_proposal.as_ref()?;
            (signed.schema == SCHEMA && signed.task_sha256 == task).then_some((r, signed))
        }) {
            proposal.expires_at_ms = signed.expires_at_ms;
            if old.req.tool_proposal.as_deref() != Some(&proposal) {
                return Err(denied());
            }
            return Ok(e.request_dto(old));
        }
        let mut request = AgentRequest {
            id: format!("rq_image_{task}"),
            agent: actor.clone(),
            title: crate::i18n::t("Criar imagem", "Create image"),
            detail: input.prompt,
            audience: crate::i18n::t("Esta conversa", "This conversation"),
            action: ActionClass::Reversible,
            content_hash: String::new(),
            item: None,
            line: None,
            proposal: None,
            tool_proposal: Some(Box::new(proposal)),
        };
        request.content_hash = request.tool_proposal_hash(space).ok_or_else(invalid)?;
        if !matches(&e.state, &request, space) {
            return Err(denied());
        }
        let id = request.id.clone();
        e.append(space, &actor, EventBody::RequestOpened { request })?;
        Ok(e.request_dto(&e.state.requests[&id]))
    }
}

impl Client {
    /// Opens a signed review. It does not reserve a budget or send to a provider.
    pub fn stage_image_generation(
        &self,
        space: &str,
        source: &str,
        serving: &str,
        input: ImageTaskInput,
    ) -> R<AgentRequestDto> {
        self.lock()
            .stage_image_generation(space, source, serving, input)
    }
}
