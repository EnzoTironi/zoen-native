//! One turn of an agent citizen, shared by every host (in-process and `zoen agent-host`):
//! gate inbound → brain (prompt + free JSON state) → gate the action → outcome.
//!
//! The host never holds the owner's keys. Creating an agent produces an [`AgentSpec`] the
//! owner's own device approves and signs (ADR 0049); only the owner's requests count.

use super::profile::{AgentKind, Avatar, CitizenRecord, Definition, ModelTier};
use async_trait::async_trait;
use roda_gate::{Check, Gate, Outcome, Point};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// The capability a Creator's `create_agent` action is gated as.
pub const CREATE_AGENT_CAPABILITY: &str = "agent.create";

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
}

/// A call and what came back, in the order they ran.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolStep {
    pub call: ToolCall,
    pub result: String,
    pub is_error: bool,
}

/// Where a turn's tool calls run. The host owns permissions and exactness (who the call
/// is for, the mini-app's own rules); the model only picks the tool and the arguments.
pub trait ToolHost: Send + Sync {
    fn tools(&self) -> Vec<ToolDef>;
    /// Runs one call that already passed the gate. `Err` is a result for the model to
    /// read (refused, invalid), not a failed turn.
    fn call(&self, call: &ToolCall) -> Result<String, String>;
}

/// Model rounds with tools in one turn; the round after the last only may speak.
pub const MAX_TOOL_ROUNDS: usize = 4;
/// Calls the model may make in one round.
pub const MAX_CALLS_PER_ROUND: usize = 4;

#[async_trait]
pub trait Brain: Send + Sync {
    async fn think(
        &self,
        cx: TurnContext<'_>,
        history: &[Turn],
        inbound: &Turn,
    ) -> Result<BrainOut, String>;

    /// One think with `tools` on offer and this turn's `steps` so far. A brain that can't
    /// call tools just thinks.
    async fn think_with_tools(
        &self,
        cx: TurnContext<'_>,
        history: &[Turn],
        inbound: &Turn,
        tools: &[ToolDef],
        steps: &[ToolStep],
    ) -> Result<BrainOut, String> {
        let _ = (tools, steps);
        self.think(cx, history, inbound).await
    }
}

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

impl Runtime {
    pub fn new(gate: Gate, brain: Box<dyn Brain>) -> Self {
        Self { gate, brain }
    }

    /// `Err` only when the brain failed: the caller retries rather than moving on.
    pub async fn turn(
        &self,
        cx: TurnContext<'_>,
        history: &[Turn],
        inbound: &Turn,
    ) -> Result<TurnOutcome, HostError> {
        if let Some(held) = self.inbound(inbound) {
            return Ok(held);
        }
        let out = self
            .brain
            .think(cx, history, inbound)
            .await
            .map_err(HostError::Brain)?;
        Ok(self.settle(cx, inbound, out))
    }

    /// A turn that may call `host`'s tools: up to [`MAX_TOOL_ROUNDS`] rounds, each call
    /// through the gate (as a tool call, against what the person just said) and then the
    /// host. `done` are steps an earlier, interrupted run of this same turn already took:
    /// the model reads them instead of running them again.
    pub async fn turn_with_tools(
        &self,
        cx: TurnContext<'_>,
        history: &[Turn],
        inbound: &Turn,
        host: &dyn ToolHost,
        done: Vec<ToolStep>,
    ) -> Result<TurnOutcome, HostError> {
        if let Some(held) = self.inbound(inbound) {
            return Ok(held);
        }
        let tools = host.tools();
        let mut steps = done;
        for round in 0..=MAX_TOOL_ROUNDS {
            let offer: &[ToolDef] = if round < MAX_TOOL_ROUNDS { &tools } else { &[] };
            let out = self
                .brain
                .think_with_tools(cx, history, inbound, offer, &steps)
                .await
                .map_err(HostError::Brain)?;
            let BrainOut::CallTools(calls) = out else {
                return Ok(self.settle(cx, inbound, out));
            };
            if offer.is_empty() {
                break;
            }
            for call in calls.into_iter().take(MAX_CALLS_PER_ROUND) {
                let (result, is_error) = match tools.iter().find(|t| t.name == call.name) {
                    None => (format!("unknown tool: {}", call.name), true),
                    Some(def) => {
                        let mut check = Check::new(
                            Point::ToolCall,
                            format!(
                                "mini-app tool {} ({}) {}",
                                call.name, def.description, call.arguments
                            ),
                        );
                        check.user_intent = Some(inbound.text.clone());
                        let d = self.gate.check(&check);
                        match d.outcome {
                            Outcome::Block => (format!("blocked: {}", d.rule), true),
                            Outcome::Ask => {
                                return Ok(TurnOutcome::AskOwner {
                                    draft: format!("{} {}", call.name, call.arguments),
                                    rule: d.rule,
                                })
                            }
                            Outcome::Allow => match host.call(&call) {
                                Ok(r) => (r, false),
                                Err(r) => (r, true),
                            },
                        }
                    }
                };
                steps.push(ToolStep {
                    call,
                    result,
                    is_error,
                });
            }
        }
        // Still calling tools with none left on offer: stop rather than loop.
        Ok(TurnOutcome::Blocked {
            reason: "tools:round-limit".into(),
        })
    }

    /// The inbound gate: `Some` when the message never reaches the brain.
    fn inbound(&self, inbound: &Turn) -> Option<TurnOutcome> {
        let d = self.gate.check(&Check::new(Point::Inbound, &inbound.text));
        match d.outcome {
            Outcome::Block => Some(TurnOutcome::Blocked { reason: d.rule }),
            Outcome::Ask => Some(TurnOutcome::AskOwner {
                draft: inbound.text.clone(),
                rule: d.rule,
            }),
            Outcome::Allow => None,
        }
    }

    fn settle(&self, cx: TurnContext<'_>, inbound: &Turn, out: BrainOut) -> TurnOutcome {
        match out {
            BrainOut::Silence => TurnOutcome::Silence,
            BrainOut::Say(text) => self.outbound(text),
            BrainOut::CreateAgent { spec, confirmation } => {
                self.propose(cx, inbound, spec, confirmation)
            }
            // Nobody offered tools: a call is nothing to say.
            BrainOut::CallTools(_) => TurnOutcome::Silence,
        }
    }

    fn outbound(&self, text: String) -> TurnOutcome {
        let d = self.gate.check(&Check::new(Point::Outbound, &text));
        match d.outcome {
            Outcome::Allow => TurnOutcome::Say { text },
            Outcome::Ask => TurnOutcome::AskOwner {
                draft: text,
                rule: d.rule,
            },
            Outcome::Block => TurnOutcome::Blocked { reason: d.rule },
        }
    }

    /// `create_agent` is an action: only a Creator, only for its owner, through the gate.
    /// Allow and Ask both end at the owner's approval; the owner signs or nothing exists.
    fn propose(
        &self,
        cx: TurnContext<'_>,
        inbound: &Turn,
        spec: AgentSpec,
        confirmation: Option<String>,
    ) -> TurnOutcome {
        if cx.definition.kind != AgentKind::Creator {
            return TurnOutcome::Blocked {
                reason: "create-agent:not-a-creator".into(),
            };
        }
        if inbound.from != cx.owner {
            return TurnOutcome::Blocked {
                reason: "create-agent:not-the-owner".into(),
            };
        }
        let mut check = Check::new(
            Point::ToolCall,
            format!(
                "{CREATE_AGENT_CAPABILITY} @{}: {}",
                spec.handle,
                serde_json::to_string(&spec.definition).unwrap_or_default()
            ),
        );
        check.capability = Some(CREATE_AGENT_CAPABILITY.into());
        check.user_intent = Some(inbound.text.clone());
        let d = self.gate.check(&check);
        if d.outcome == Outcome::Block {
            return TurnOutcome::Blocked { reason: d.rule };
        }
        let confirmation = confirmation.and_then(|c| match self.outbound(c) {
            TurnOutcome::Say { text } => Some(text),
            _ => None,
        });
        TurnOutcome::ProposeAgent { spec, confirmation }
    }
}

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
