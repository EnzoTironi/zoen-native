//! In-process durable host: gates inbound/outbound, runs a brain, claims children.
//! Exact code only for crypto, sync membership and permissions; the brain is prompts.

use super::profile::{
    claim_citizen, AgentKind, Avatar, Citizen, CitizenRecord, Definition, ModelTier,
};
use roda_gate::{Check, Gate, Outcome, Point};
use roda_log::Author;
use roda_types::{Identity, IdentityKind};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub space: String,
    pub from: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TurnOutcome {
    Silence,
    Say { text: String },
    AskOwner { text: String },
    Blocked { reason: String },
    Created {
        child: CitizenRecord,
        reply: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HostError {
    #[error("unknown agent")]
    Unknown,
    #[error("agent is not a member of that Space")]
    NotMember,
    #[error("only a Person owner can authorize agents")]
    Owner,
    #[error("invalid create_agent payload")]
    Create,
    #[error("definition encode failed")]
    Definition,
}

/// Produces the next action from persona + history. Production uses zoen-models;
/// journeys use [`ScriptedBrain`].
pub trait Brain: Send + Sync {
    fn think(&self, def: &Definition, history: &[Turn], inbound: &Turn) -> BrainOut;
}

#[derive(Clone, Debug, PartialEq)]
pub enum BrainOut {
    Say(String),
    CreateAgent {
        name: String,
        handle: String,
        persona: String,
        rules: Vec<String>,
        state: Value,
        glyph: String,
        tint_hex: String,
    },
}

/// Parses a `create_agent` JSON fence from model text, else treats the whole reply as say.
#[derive(Default)]
pub struct ScriptedBrain {
    /// Fixed replies keyed by substring match on the inbound text (journey only).
    pub replies: BTreeMap<String, String>,
}

impl Brain for ScriptedBrain {
    fn think(&self, def: &Definition, _history: &[Turn], inbound: &Turn) -> BrainOut {
        let reply = self
            .replies
            .iter()
            .find(|(k, _)| inbound.text.to_lowercase().contains(&k.to_lowercase()))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| match def.kind {
                AgentKind::Creator => {
                    "Tell me the agent's name and what it should do.".into()
                }
                AgentKind::User => def
                    .state
                    .get("greeting")
                    .and_then(|g| g.as_str())
                    .unwrap_or("How can I help?")
                    .to_string(),
            });
        if let Some(created) = parse_create_agent(&reply) {
            return created;
        }
        BrainOut::Say(reply)
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

/// Model text → a create_agent action or plain speech.
pub fn parse_reply(text: &str) -> BrainOut {
    parse_create_agent(text).unwrap_or_else(|| BrainOut::Say(text.trim().to_string()))
}

fn parse_create_agent(text: &str) -> Option<BrainOut> {
    let json_str = extract_json_fence(text)?;
    let wire: CreateAgentWire = serde_json::from_str(&json_str).ok()?;
    let b = wire.create_agent;
    if b.name.trim().is_empty() || b.handle.trim().is_empty() {
        return None;
    }
    Some(BrainOut::CreateAgent {
        name: b.name,
        handle: b.handle,
        persona: b.persona,
        rules: b.rules,
        state: b.state,
        glyph: b.glyph,
        tint_hex: b.tint_hex,
    })
}

fn extract_json_fence(text: &str) -> Option<String> {
    if let Some(start) = text.find("```json") {
        let rest = &text[start + 7..];
        let end = rest.find("```")?;
        return Some(rest[..end].trim().to_string());
    }
    let t = text.trim();
    if t.starts_with('{') && t.contains("create_agent") {
        return Some(t.to_string());
    }
    None
}

pub struct Host {
    gate: Gate,
    brain: Box<dyn Brain>,
    citizens: BTreeMap<String, Hosted>,
    /// Owner person profiles keyed by identity id (for authorizing children).
    owners: BTreeMap<String, Identity>,
    /// Owner authors (certified devices) for claim_citizen.
    owner_authors: BTreeMap<String, Author>,
    history: BTreeMap<String, Vec<Turn>>,
}

struct Hosted {
    citizen: Citizen,
    memberships: Vec<String>,
}

impl Host {
    pub fn new(gate: Gate, brain: Box<dyn Brain>) -> Self {
        Self {
            gate,
            brain,
            citizens: BTreeMap::new(),
            owners: BTreeMap::new(),
            owner_authors: BTreeMap::new(),
            history: BTreeMap::new(),
        }
    }

    pub fn remember_owner(&mut self, person: Identity, author: Author) -> Result<(), HostError> {
        if person.kind != IdentityKind::Person || author.identity != person.id {
            return Err(HostError::Owner);
        }
        self.owner_authors.insert(person.id.clone(), author);
        self.owners.insert(person.id.clone(), person);
        Ok(())
    }

    pub fn enroll(&mut self, citizen: Citizen) -> CitizenRecord {
        let record = CitizenRecord {
            profile: citizen.profile.clone(),
            device: citizen.device_id().to_string(),
            cert: citizen.author.cert.clone().unwrap_or_default(),
            definition: citizen.definition.clone(),
            avatar: citizen.avatar.clone(),
            memberships: Vec::new(),
        };
        self.citizens.insert(
            citizen.id().to_string(),
            Hosted {
                citizen,
                memberships: Vec::new(),
            },
        );
        record
    }

    pub fn invite(&mut self, agent: &str, space: &str) -> Result<(), HostError> {
        let h = self.citizens.get_mut(agent).ok_or(HostError::Unknown)?;
        if !h.memberships.iter().any(|s| s == space) {
            h.memberships.push(space.into());
        }
        Ok(())
    }

    pub fn memberships(&self, agent: &str) -> Result<&[String], HostError> {
        Ok(&self
            .citizens
            .get(agent)
            .ok_or(HostError::Unknown)?
            .memberships)
    }

    pub fn record(&self, agent: &str) -> Result<CitizenRecord, HostError> {
        let h = self.citizens.get(agent).ok_or(HostError::Unknown)?;
        Ok(CitizenRecord {
            profile: h.citizen.profile.clone(),
            device: h.citizen.device_id().to_string(),
            cert: h.citizen.author.cert.clone().unwrap_or_default(),
            definition: h.citizen.definition.clone(),
            avatar: h.citizen.avatar.clone(),
            memberships: h.memberships.clone(),
        })
    }

    /// Owner offline: the host still answers as the agent in Spaces it belongs to.
    pub fn handle_turn(&mut self, agent: &str, inbound: Turn) -> Result<TurnOutcome, HostError> {
        let member = {
            let h = self.citizens.get(agent).ok_or(HostError::Unknown)?;
            h.memberships.iter().any(|s| s == &inbound.space)
        };
        if !member {
            return Err(HostError::NotMember);
        }

        let inbound_check = self.gate.check(&Check::new(Point::Inbound, &inbound.text));
        if inbound_check.outcome == Outcome::Block {
            return Ok(TurnOutcome::Blocked {
                reason: inbound_check.rule,
            });
        }

        let def = self
            .citizens
            .get(agent)
            .ok_or(HostError::Unknown)?
            .citizen
            .definition
            .clone();
        let hist_key = format!("{agent}:{}", inbound.space);
        let history = self.history.get(&hist_key).cloned().unwrap_or_default();
        let out = self.brain.think(&def, &history, &inbound);
        self.history.entry(hist_key.clone()).or_default().push(inbound.clone());

        match out {
            BrainOut::Say(text) => self.say_gated(agent, &inbound.space, text),
            BrainOut::CreateAgent {
                name,
                handle,
                persona,
                rules,
                state,
                glyph,
                tint_hex,
            } => self.create_child(
                agent,
                &inbound.space,
                name,
                handle,
                persona,
                rules,
                state,
                glyph,
                tint_hex,
            ),
        }
    }

    fn say_gated(
        &mut self,
        agent: &str,
        space: &str,
        text: String,
    ) -> Result<TurnOutcome, HostError> {
        let d = self.gate.check(&Check::new(Point::Outbound, &text));
        match d.outcome {
            Outcome::Allow => {
                let hist_key = format!("{agent}:{space}");
                self.history.entry(hist_key).or_default().push(Turn {
                    space: space.into(),
                    from: agent.into(),
                    text: text.clone(),
                });
                Ok(TurnOutcome::Say { text })
            }
            Outcome::Ask => Ok(TurnOutcome::AskOwner { text }),
            Outcome::Block => Ok(TurnOutcome::Blocked { reason: d.rule }),
        }
    }

    fn create_child(
        &mut self,
        creator_id: &str,
        space: &str,
        name: String,
        handle: String,
        persona: String,
        rules: Vec<String>,
        state: Value,
        glyph: String,
        tint_hex: String,
    ) -> Result<TurnOutcome, HostError> {
        let creator = self.citizens.get(creator_id).ok_or(HostError::Unknown)?;
        if creator.citizen.definition.kind != AgentKind::Creator {
            return Err(HostError::Create);
        }
        let owner_id = creator
            .citizen
            .profile
            .owner
            .clone()
            .ok_or(HostError::Owner)?;
        let owner = self.owners.get(&owner_id).ok_or(HostError::Owner)?.clone();
        let owner_author = self
            .owner_authors
            .get(&owner_id)
            .ok_or(HostError::Owner)?
            .clone();

        let definition = Definition {
            version: 1,
            kind: AgentKind::User,
            persona,
            rules,
            state,
            model_tier: ModelTier::Cheap,
        };
        let avatar = Avatar::hand_drawn(tint_hex, glyph);
        let child = claim_citizen(&owner_author, &owner, name, handle, avatar, definition)
            .map_err(|_| HostError::Create)?;
        let reply = format!(
            "Pronto: {} (@{}) já é um cidadão Zoen. Convide-o para o chat quando quiser.",
            child.profile.name, child.profile.handle
        );
        // Track in creator state without deterministic booking logic.
        if let Some(h) = self.citizens.get_mut(creator_id) {
            let list = h
                .citizen
                .definition
                .state
                .as_object_mut()
                .map(|o| o.entry("created_agents").or_insert_with(|| json!([])));
            if let Some(Value::Array(arr)) = list {
                arr.push(json!({
                    "id": child.profile.id,
                    "handle": child.profile.handle,
                }));
            }
        }
        let record = self.enroll(child);
        let say = self.say_gated(creator_id, space, reply)?;
        match say {
            TurnOutcome::Say { text } => Ok(TurnOutcome::Created {
                child: record,
                reply: text,
            }),
            other => Ok(other),
        }
    }
}
