use grants::consent;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ───────────────────────────── questions and answers ─────────────────────────────

/// A typed question. `Predicate` is System One's `noul` and `OpenAI`'s `predicate`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Question {
    Predicate {
        name: String,
        instructions: String,
    },
    Choice {
        name: String,
        instructions: String,
        choices: Vec<(String, String)>,
    },
    Score {
        name: String,
        instructions: String,
        levels: Vec<String>,
    },
}

impl Question {
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Question::Predicate { name, .. }
            | Question::Choice { name, .. }
            | Question::Score { name, .. } => name,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Answer {
    /// Probability the condition is true.
    Predicate {
        p: f64,
    },
    Choice {
        choice: String,
        confidence: f64,
    },
    /// Expected level index (0 = lowest), may fall between levels.
    Score {
        score: f64,
        confidence: f64,
    },
    Refusal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DecisionRequest {
    pub input: String,
    pub questions: Vec<Question>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DecisionResponse {
    pub answers: BTreeMap<String, Answer>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GateError {
    Transport(String),
    BadResponse(String),
    Unavailable(String),
}

pub trait DecisionProvider: Send + Sync {
    fn name(&self) -> String;
    /// Runs on this device (may see sensitive categories).
    fn on_device(&self) -> bool;
    /// # Errors
    /// Returns an error when the provider is unavailable, transport fails, or its response is invalid.
    fn decide(&self, req: &DecisionRequest) -> Result<DecisionResponse, GateError>;
}

// ───────────────────────────── gate points ─────────────────────────────

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Point {
    /// A tool or capability call by a mini-app or agent.
    ToolCall,
    /// Tool results, web content and mini-app outputs coming into the agent's context.
    Inbound,
    /// A conversation message from the host's authenticated timeline. This identifies
    /// its source, not permission to bypass policy or execute an action.
    ConversationMessage,
    /// A marketplace agent or mini-app bundle at install or publish time.
    Bundle,
    /// An agent message before it posts to a group.
    Outbound,
}

impl Point {
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Point::ToolCall => "tool-call",
            Point::Inbound => "inbound",
            Point::ConversationMessage => "conversation-message",
            Point::Bundle => "bundle",
            Point::Outbound => "outbound",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Allow,
    Ask,
    Block,
}

impl Outcome {
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Outcome::Allow => "allow",
            Outcome::Ask => "ask",
            Outcome::Block => "block",
        }
    }
}

/// What's being checked.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Check {
    pub point: Point,
    /// The thing itself: "calendar.freebusy for Saturday hike", a web result, a bundle
    /// summary, a draft message.
    pub text: String,
    /// What you just said or tapped (tool calls).
    pub user_intent: Option<String>,
    /// Capability string, for tool calls (feeds the consent policy's always-ask list).
    pub capability: Option<String>,
    /// The consent policy's inputs, when the caller has them (tool calls).
    pub consent: Option<consent::Request>,
    /// Health, contacts, full calendar…: on-device only.
    pub sensitive: bool,
    /// Known irreversible from the action class (fail closed).
    pub irreversible: bool,
}

impl Check {
    pub fn new(point: Point, text: impl Into<String>) -> Self {
        Check {
            point,
            text: text.into(),
            user_intent: None,
            capability: None,
            consent: None,
            sensitive: false,
            irreversible: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AuditRecord {
    pub point: String,
    pub outcome: String,
    /// Stable rule id: `hard:*`, `always:*`, `model:*`, `fail:*`.
    pub rule: String,
    pub provider: String,
    /// Redacted, truncated description (never raw arguments).
    pub summary: String,
    /// sha256 of the raw input, so the decision can be matched later without storing it.
    pub input_hash: String,
    pub answers: BTreeMap<String, f64>,
    pub latency_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    pub outcome: Outcome,
    pub rule: String,
    pub provider: String,
    pub answers: BTreeMap<String, f64>,
    pub latency_ms: u64,
    pub audit: AuditRecord,
}

// ───────────────────────────── thresholds ─────────────────────────────

/// Mapping from answers to outcomes. The defaults lean towards asking.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Thresholds {
    pub min_confidence: f64,
    pub intent_min: f64,
    pub severity_ask: f64,
    pub severity_block: f64,
    pub irreversible_ask: f64,
    pub exfil_ask: f64,
    pub exfil_block: f64,
    pub outside_ask: f64,
    pub injection_ask: f64,
    pub injection_block: f64,
    pub malicious_block: f64,
    pub harvest_ask: f64,
    pub harvest_block: f64,
    pub harmful_block: f64,
    pub harmful_ask: f64,
    pub leak_ask: f64,
    pub leak_block: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            min_confidence: 0.35,
            intent_min: 0.5,
            severity_ask: 1.3,
            severity_block: 2.4,
            irreversible_ask: 0.5,
            exfil_ask: 0.5,
            exfil_block: 0.85,
            outside_ask: 0.6,
            injection_ask: 0.4,
            injection_block: 0.75,
            malicious_block: 0.6,
            harvest_ask: 0.5,
            harvest_block: 0.85,
            harmful_block: 2.0,
            harmful_ask: 1.2,
            leak_ask: 0.5,
            leak_block: 0.85,
        }
    }
}

impl Thresholds {
    /// Dev preset for Laya served locally (tools/gate-serve). Laya's score confidences are
    /// uncalibrated (a 4-level score rarely clears 0.35) and its `harmful_content` score rated
    /// a plain itinerary 0.80/3, so both are ignored; it leans on exfiltration, intent and
    /// leak predicates, which separated cleanly. FITTED ON THE SAME 21 RED-TEAM FIXTURES IT
    /// IS SCORED ON: re-fit on a held-out set before trusting it. Jev uses the defaults.
    #[must_use]
    pub fn laya_local() -> Self {
        Thresholds {
            min_confidence: 0.0,
            intent_min: 0.53,
            severity_ask: 1.6,
            severity_block: 9.0,
            irreversible_ask: 0.5,
            exfil_ask: 0.3,
            exfil_block: 0.85,
            outside_ask: 0.6,
            injection_ask: 0.4,
            injection_block: 0.75,
            harmful_ask: 9.0,
            harmful_block: 9.0,
            leak_ask: 0.5,
            leak_block: 0.85,
            ..Thresholds::default()
        }
    }

    /// "Mostly escalate to ask": for a decision model that hasn't been validated (or scores
    /// near random), never let it allow on its own a borderline answer, and never block on
    /// it alone either: the hard rules still block what must be blocked.
    #[must_use]
    pub fn escalate_to_ask() -> Self {
        Thresholds {
            min_confidence: 0.6,
            intent_min: 0.7,
            severity_ask: 0.8,
            severity_block: 9.0,
            irreversible_ask: 0.3,
            exfil_ask: 0.3,
            exfil_block: 9.0,
            outside_ask: 0.4,
            injection_ask: 0.25,
            injection_block: 9.0,
            malicious_block: 9.0,
            harvest_ask: 0.3,
            harvest_block: 9.0,
            harmful_block: 9.0,
            harmful_ask: 0.8,
            leak_ask: 0.3,
            leak_block: 9.0,
        }
    }
}
