//! One turn of an agent citizen, shared by every host (in-process and `zoen agent-host`):
//! gate inbound → brain (prompt + free JSON state) → gate the action → outcome.
//!
//! The host never holds the owner's keys. Creating an agent produces an [`AgentSpec`] the
//! owner's own device approves and signs (ADR 0049); only the owner's requests count.

use super::profile::{AgentKind, Avatar, CitizenRecord, Definition, ModelTier};
use async_trait::async_trait;
use gate::{Check, Gate, Outcome, Point};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

mod brain;
pub use brain::{Brain, BrainError};

/// The capability a Creator's `create_agent` action is gated as.
pub const CREATE_AGENT_CAPABILITY: &str = "agent.create";

/// A stricter native review after Gate Allow; it never overrides Gate Block or Ask.
pub const NATIVE_REVIEW_REQUIRED: &str = "native:review-required";

/// A conversation message admitted by the host from its authenticated timeline.
/// The host establishes authenticity; constructing this value is not proof.
/// The author and text do not grant tool access or bypass the owner's policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub space: String,
    pub from: String,
    pub text: String,
}

/// Who is thinking: the agent, its runtime device, the Person it answers to, its definition.
#[derive(Clone, Copy, Debug)]
pub struct TurnContext<'a> {
    pub agent: &'a str,
    pub device: &'a str,
    pub owner: &'a str,
    pub definition: &'a Definition,
}

/// A new agent, as the Creator proposes it. Only the owner's device turns it into a
/// citizen ([`super::approve_agent`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentSpec {
    pub name: String,
    pub handle: String,
    pub glyph: String,
    pub tint_hex: String,
    pub definition: Definition,
}

impl AgentSpec {
    pub fn avatar(&self) -> Avatar {
        Avatar::hand_drawn(self.tint_hex.clone(), self.glyph.clone())
    }
}

#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum TurnOutcome {
    /// Nothing to say (or the agent is paused).
    Silence,
    Say {
        text: String,
    },
    /// The gate wants the owner to decide: the draft (or held inbound) goes to the owner.
    AskOwner {
        draft: String,
        rule: String,
    },
    /// A tool was held before execution. This is inert continuation data, never a grant.
    ToolAsk {
        ask: ToolAsk,
    },
    Blocked {
        reason: String,
    },
    /// A pending agent for the owner to approve, and the Creator's own words to the chat.
    ProposeAgent {
        spec: AgentSpec,
        confirmation: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HostError {
    #[error("unknown agent")]
    Unknown,
    #[error("agent is not a member of that Space")]
    NotMember,
    #[error("the brain failed: {0}")]
    Brain(String),
    #[error("the brain did not dispatch: {0}")]
    BrainNotDispatched(String),
    #[error("invalid tool continuation: {0}")]
    ToolContinuation(String),
    #[error("tool admission refused: {0}")]
    ToolAdmission(String),
}

/// What the brain wants to do. Production is [`super::ModelBrain`].
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum BrainOut {
    Silence,
    Say(String),
    CreateAgent {
        spec: AgentSpec,
        /// The model's own words around the action, if any.
        confirmation: Option<String>,
    },
    /// Calls to the tools it was offered; the results come back on its next think.
    CallTools(Vec<ToolCall>),
}

/// A tool the model may call this turn (a mini-app tool from its chat, as its manifest
/// declares it).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    /// A stricter request, never an execution permit. False preserves legacy bytes.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub requires_review: bool,
}

mod public_references;
pub use public_references::PublicArtifactReferences;
mod observation;
pub use observation::ToolObservation;

/// A call and what came back, in the order they ran.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolStep {
    pub call: ToolCall,
    pub result: String,
    pub is_error: bool,
}

/// The runtime context of a held call. The canonical host additionally binds its source
/// event, task, grants, artifact head and signed worker; these locators confer no authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolAskEnvelope {
    pub agent: String,
    pub device: String,
    pub owner: String,
    pub space: String,
    pub from: String,
    pub inbound_sha256: String,
}

impl ToolAskEnvelope {
    fn for_turn(cx: TurnContext<'_>, inbound: &Turn) -> Self {
        Self {
            agent: cx.agent.into(),
            device: cx.device.into(),
            owner: cx.owner.into(),
            space: inbound.space.clone(),
            from: inbound.from.clone(),
            inbound_sha256: hex::encode(Sha256::digest(inbound.text.as_bytes())),
        }
    }
}

/// Exact interrupted model batch. Deserializing or constructing it does not approve it.
/// Only a trusted host can admit the hash-bound, canonically approved frozen call.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolAsk {
    pub version: u32,
    /// Exact threshold/provider/question configuration that classified this call.
    pub policy_sha256: String,
    pub envelope: ToolAskEnvelope,
    pub call: ToolCall,
    pub definition: ToolDef,
    pub steps: Vec<ToolStep>,
    pub remaining: Vec<ToolCall>,
    pub round: usize,
    pub rule: String,
}

impl ToolAsk {
    fn validate(&self, cx: TurnContext<'_>, inbound: &Turn) -> Result<(), HostError> {
        if self.version != 1
            || self.round >= MAX_TOOL_ROUNDS
            || self.steps.len() >= MAX_TOOL_STEPS
            || self.remaining.len() >= MAX_CALLS_PER_ROUND
            || self.steps.len() + 1 + self.remaining.len() > MAX_TOOL_STEPS
            || self.call.name != self.definition.name
            || self.policy_sha256.len() != 64
            || !self
                .policy_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(HostError::ToolContinuation("shape or budget".into()));
        }
        let encoded =
            serde_json::to_vec(self).map_err(|_| HostError::ToolContinuation("encoding".into()))?;
        if encoded.len() > MAX_TOOL_CONTINUATION_BYTES {
            return Err(HostError::ToolContinuation("size limit".into()));
        }
        if self.envelope != ToolAskEnvelope::for_turn(cx, inbound) {
            return Err(HostError::ToolContinuation("turn context changed".into()));
        }
        Ok(())
    }
}

/// Result data from an admitted execution. An error from the tool itself is returned to
/// the model; refusal to admit the operation instead stops the continuation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolResult {
    pub result: String,
    pub is_error: bool,
}

/// Inert progress after an execution. Recovery hosts must verify completed effects
/// against native receipts and current access before passing this back to Runtime.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolProgress {
    pub round: usize,
    pub steps: Vec<ToolStep>,
    pub remaining: Vec<ToolCall>,
}

impl ToolProgress {
    fn bounds(&self) -> Result<(), HostError> {
        if self.round >= MAX_TOOL_ROUNDS
            || self.steps.is_empty()
            || self.steps.len() > MAX_TOOL_STEPS
            || self.remaining.len() >= MAX_CALLS_PER_ROUND
            || self.steps.len() + self.remaining.len() > MAX_TOOL_STEPS
            || serde_json::to_vec(self)
                .map_err(|_| HostError::ToolContinuation("progress encoding".into()))?
                .len()
                > MAX_TOOL_CONTINUATION_BYTES
        {
            return Err(HostError::ToolContinuation(
                "progress shape or size limit".into(),
            ));
        }
        Ok(())
    }
    fn validate(&self, original: &ToolAsk) -> Result<(), HostError> {
        self.bounds()?;
        if self.round < original.round
            || self.round >= MAX_TOOL_ROUNDS
            || self.steps.len() <= original.steps.len()
            || self.steps.len() > MAX_TOOL_STEPS
            || self.remaining.len() >= MAX_CALLS_PER_ROUND
            || self.steps.len() + self.remaining.len() > MAX_TOOL_STEPS
            || !self.steps.starts_with(&original.steps)
            || self.steps[original.steps.len()].call != original.call
        {
            return Err(HostError::ToolContinuation(
                "invalid completed progress".into(),
            ));
        }
        if self.round == original.round {
            let completed = self.steps.len() - original.steps.len() - 1;
            if completed > original.remaining.len()
                || self.steps[original.steps.len() + 1..]
                    .iter()
                    .map(|step| &step.call)
                    .ne(original.remaining[..completed].iter())
                || self.remaining != original.remaining[completed..]
            {
                return Err(HostError::ToolContinuation(
                    "completed batch changed".into(),
                ));
            }
        }
        Ok(())
    }
}

/// Trusted host boundary backed by its opaque canonical approval permit. The host must
/// bind that permit to the entire exact serialized `ask`, revalidate current authority,
/// and execute only `ask.call` through its ordinary tool executor/receipt path. A boolean,
/// model value or generic request DTO is not an admission implementation.
pub trait ApprovedToolAdmission: Send + Sync {
    fn execute(&self, ask: &ToolAsk) -> Result<ToolResult, String>;
}

/// Where a turn's tool calls run. The host owns permissions and exactness (who the call
/// is for, the mini-app's own rules); the model only picks the tool and the arguments.
pub trait ToolHost: Send + Sync {
    fn tools(&self) -> Vec<ToolDef>;
    /// A native host may refresh discovery after a completed effect creates an
    /// artifact. Held/remaining calls retain their original offered definitions;
    /// every newly proposed call still passes the normal Gate and native checks.
    fn refreshed_tools(&self) -> Option<Vec<ToolDef>> {
        None
    }
    /// Diagnostics only, at the actual Brain boundary. Default has no side effects.
    fn observe_model_input(&self, _tools: &[ToolDef], _steps: &[ToolStep], _round: usize) {}
    /// A closed class, never a raw result or an execution/approval permit.
    fn observe_tool_outcome(&self, _call: &ToolCall, _outcome: ToolObservation) {}
    /// Native first-delivery policy may require a canonical review after Gate Allow.
    /// This cannot override Block, repeat an Ask or authorize execution itself.
    fn requires_native_review(&self, _call: &ToolCall) -> bool {
        false
    }
    /// Default: inspect the complete raw call. A trusted host may classify only
    /// exact validated public artifact/head references; Runtime still owns the Gate
    /// and retains every other argument and the full user-facing description.
    fn gate_public_references(&self, _call: &ToolCall) -> Option<PublicArtifactReferences> {
        None
    }
    /// Runs one call that already passed the gate. `Err` is a result for the model to
    /// read (refused, invalid), not a failed turn.
    fn call(&self, call: &ToolCall) -> Result<String, String>;
    /// Persist an exact Gate-allowed batch before its effect. This is a recovery hint,
    /// never permission to execute a prepared operation without normal native admission.
    fn prepare(&self, _prepared: &ToolAsk) -> Result<(), String> {
        Ok(())
    }
    /// Persist actual completed results before another model turn or tool effect.
    fn checkpoint(&self, _progress: &ToolProgress) -> Result<(), String> {
        Ok(())
    }
}

/// Model rounds with tools in one turn; the round after the last only may speak.
pub const MAX_TOOL_ROUNDS: usize = 4;
/// Calls the model may make in one round.
pub const MAX_CALLS_PER_ROUND: usize = 4;
/// Cumulative call budget, including steps recovered after interruption.
pub const MAX_TOOL_STEPS: usize = MAX_TOOL_ROUNDS * MAX_CALLS_PER_ROUND;
/// Upper bound shared with the canonical host's persisted continuation payload.
pub const MAX_TOOL_CONTINUATION_BYTES: usize = 128 * 1024;

#[derive(Deserialize)]
struct CreateAgentWire {
    create_agent: CreateAgentBody,
}

#[derive(Deserialize)]
struct CreateAgentBody {
    name: String,
    handle: String,
    persona: String,
    #[serde(default)]
    rules: Vec<String>,
    #[serde(default)]
    state: Value,
    #[serde(default = "default_glyph")]
    glyph: String,
    #[serde(default = "default_tint")]
    tint_hex: String,
}

fn default_glyph() -> String {
    "sparkles".into()
}
fn default_tint() -> String {
    "#7C5CFF".into()
}

/// Model text → an action or speech. Only a fenced ```json block can be an action; any
/// other JSON-looking text is just speech.
pub fn parse_reply(text: &str) -> BrainOut {
    if let Some((before, json, after)) = json_fence(text) {
        if let Ok(wire) = serde_json::from_str::<CreateAgentWire>(json) {
            let b = wire.create_agent;
            if !b.name.trim().is_empty() && !b.handle.trim().is_empty() {
                let words = format!("{} {}", before.trim(), after.trim());
                let words = words.trim();
                return BrainOut::CreateAgent {
                    spec: AgentSpec {
                        name: b.name,
                        handle: b.handle,
                        glyph: b.glyph,
                        tint_hex: b.tint_hex,
                        definition: Definition {
                            version: 1,
                            kind: AgentKind::User,
                            persona: b.persona,
                            rules: b.rules,
                            state: b.state,
                            model_tier: ModelTier::Cheap,
                        },
                    },
                    confirmation: (!words.is_empty()).then(|| words.to_string()),
                };
            }
        }
    }
    let t = text.trim();
    if t.is_empty() {
        BrainOut::Silence
    } else {
        BrainOut::Say(t.to_string())
    }
}

fn json_fence(text: &str) -> Option<(&str, &str, &str)> {
    let start = text.find("```json")?;
    let rest = &text[start + 7..];
    let end = rest.find("```")?;
    Some((&text[..start], rest[..end].trim(), &rest[end + 3..]))
}

/// The gate and the brain: one turn, the same for every host.
pub struct Runtime {
    pub gate: Gate,
    pub brain: Box<dyn Brain>,
}

mod runtime;

/// In-process host for many citizens: membership is exact code, turns are [`Runtime`].
/// History is the Space's own timeline as this host saw it.
pub struct Host {
    runtime: Runtime,
    citizens: BTreeMap<String, Hosted>,
    history: BTreeMap<(String, String), Vec<Turn>>,
}

struct Hosted {
    record: CitizenRecord,
}

impl Host {
    pub fn new(gate: Gate, brain: Box<dyn Brain>) -> Self {
        Self {
            runtime: Runtime::new(gate, brain),
            citizens: BTreeMap::new(),
            history: BTreeMap::new(),
        }
    }

    /// Hosts a citizen. The host keeps its public record only; keys stay with its device.
    pub fn enroll(&mut self, record: CitizenRecord) {
        self.citizens
            .insert(record.profile.id.clone(), Hosted { record });
    }

    pub fn invite(&mut self, agent: &str, space: &str) -> Result<(), HostError> {
        let h = self.citizens.get_mut(agent).ok_or(HostError::Unknown)?;
        if !h.record.memberships.iter().any(|s| s == space) {
            h.record.memberships.push(space.into());
        }
        Ok(())
    }

    pub fn memberships(&self, agent: &str) -> Result<&[String], HostError> {
        Ok(&self
            .citizens
            .get(agent)
            .ok_or(HostError::Unknown)?
            .record
            .memberships)
    }

    pub fn record(&self, agent: &str) -> Result<CitizenRecord, HostError> {
        Ok(self
            .citizens
            .get(agent)
            .ok_or(HostError::Unknown)?
            .record
            .clone())
    }

    /// Owner offline: the host still answers as the agent in Spaces it belongs to.
    pub async fn handle_turn(
        &mut self,
        agent: &str,
        inbound: Turn,
    ) -> Result<TurnOutcome, HostError> {
        let h = self.citizens.get(agent).ok_or(HostError::Unknown)?;
        if !h.record.memberships.iter().any(|s| s == &inbound.space) {
            return Err(HostError::NotMember);
        }
        let owner = h.record.profile.owner.clone().unwrap_or_default();
        let key = (agent.to_string(), inbound.space.clone());
        let history = self.history.get(&key).cloned().unwrap_or_default();
        let cx = TurnContext {
            agent,
            device: &h.record.device,
            owner: &owner,
            definition: &h.record.definition,
        };
        let out = self.runtime.turn(cx, &history, &inbound).await?;
        let log = self.history.entry(key).or_default();
        log.push(inbound.clone());
        if let TurnOutcome::Say { text } = &out {
            log.push(Turn {
                space: inbound.space,
                from: agent.to_string(),
                text: text.clone(),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
#[path = "conversation_gate_tests.rs"]
mod conversation_gate_tests;

#[cfg(test)]
mod continuation_tests;
#[cfg(test)]
mod native_review_tests;
#[cfg(test)]
mod round_limit_tests;
