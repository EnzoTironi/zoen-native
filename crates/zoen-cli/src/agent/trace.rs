//! Public operational outcomes only. No payload, authority, liveness or provider meter.
use sha2::{Digest, Sha256};
use std::io::{self, Write};
use zoen_agentd::{HostError, TurnOutcome};

const EVENT: &str = "zoen.agent.outcome.v1";
const LOCATOR_DOMAIN: &[u8] = b"zoen.agent.outcome.locators.v1\0";

#[derive(Clone, Copy)]
pub(super) enum Delivery {
    ApprovalStaging,
    Reply,
    OwnerNote,
}
impl Delivery {
    fn phase(self) -> &'static str {
        match self {
            Self::ApprovalStaging => "approval_staging",
            Self::Reply => "reply",
            Self::OwnerNote => "owner_note",
        }
    }
}

#[derive(Clone, Copy)]
enum TurnKind {
    Silence,
    Blocked,
    AskOwner,
    Say,
    ToolAsk,
    ProposeAgent,
    RuntimeError,
}
impl TurnKind {
    fn id(self) -> &'static str {
        match self {
            Self::Silence => "silence",
            Self::Blocked => "blocked",
            Self::AskOwner => "ask_owner",
            Self::Say => "say",
            Self::ToolAsk => "tool_ask",
            Self::ProposeAgent => "propose_agent",
            Self::RuntimeError => "runtime_error",
        }
    }
}

#[derive(Clone, Copy)]
enum RuleClass {
    HardInjectionMarker,
    HardHiddenCharacters,
    ModelInjection,
    ModelLowConfidence,
    Other,
}
impl RuleClass {
    fn from_rule(rule: &str) -> Self {
        // Exact matches only. Failure rules can contain arbitrary provider error text.
        match rule {
            "hard:injection-marker" => Self::HardInjectionMarker,
            "hard:hidden-characters" => Self::HardHiddenCharacters,
            "model:injection" => Self::ModelInjection,
            "model:low-confidence" => Self::ModelLowConfidence,
            _ => Self::Other,
        }
    }
    fn id(self) -> &'static str {
        match self {
            Self::HardInjectionMarker => "hard_injection_marker",
            Self::HardHiddenCharacters => "hard_hidden_characters",
            Self::ModelInjection => "model_injection",
            Self::ModelLowConfidence => "model_low_confidence",
            Self::Other => "other",
        }
    }
}

enum Outcome {
    Turn(TurnKind),
    Delivery { kind: Delivery, succeeded: bool },
}
struct Record {
    correlation: String,
    outcome: Outcome,
    rule: Option<RuleClass>,
}
impl Record {
    fn turn(space: &str, source: &str, result: &Result<TurnOutcome, HostError>) -> Self {
        let (kind, rule) = match result {
            Ok(TurnOutcome::Silence) => (TurnKind::Silence, None),
            Ok(TurnOutcome::Blocked { reason }) => (TurnKind::Blocked, Some(reason.as_str())),
            Ok(TurnOutcome::AskOwner { rule, .. }) => (TurnKind::AskOwner, Some(rule.as_str())),
            Ok(TurnOutcome::Say { .. }) => (TurnKind::Say, None),
            Ok(TurnOutcome::ToolAsk { ask }) => (TurnKind::ToolAsk, Some(ask.rule.as_str())),
            Ok(TurnOutcome::ProposeAgent { .. }) => (TurnKind::ProposeAgent, None),
            Err(_) => (TurnKind::RuntimeError, None),
        };
        Self {
            correlation: correlate(space, source),
            outcome: Outcome::Turn(kind),
            rule: rule.map(RuleClass::from_rule),
        }
    }
    fn delivery(space: &str, source: &str, kind: Delivery, succeeded: bool) -> Self {
        Self {
            correlation: correlate(space, source),
            outcome: Outcome::Delivery { kind, succeeded },
            rule: None,
        }
    }
    fn write_to(&self, writer: &mut impl Write) -> io::Result<()> {
        let (phase, outcome) = match self.outcome {
            Outcome::Turn(kind) => ("turn", kind.id()),
            Outcome::Delivery { kind, succeeded } => (
                kind.phase(),
                match (kind, succeeded) {
                    (_, false) => "failed",
                    // An exact existing Request can also satisfy staging.
                    (Delivery::ApprovalStaging, true) => "ok",
                    (_, true) => "queued",
                },
            ),
        };
        let mut value = serde_json::json!({
            "event": EVENT,
            "correlation": self.correlation,
            "phase": phase,
            "outcome": outcome,
        });
        if let Some(rule) = self.rule {
            value["ruleClass"] = rule.id().into();
        }
        serde_json::to_writer(&mut *writer, &value)?;
        writer.write_all(b"\n")
    }
}

fn correlate(space: &str, source: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(LOCATOR_DOMAIN);
    // Domain separation and byte lengths distinguish different pairs of locators.
    for locator in [space, source] {
        hash.update((locator.len() as u64).to_le_bytes());
        hash.update(locator.as_bytes());
    }
    hex::encode(hash.finalize())
}

fn emit_to(record: &Record, writer: &mut impl Write) {
    // Diagnostics cannot change task results or trigger a retry/approval.
    let _ = record.write_to(writer);
}
fn emit(record: &Record) {
    emit_to(record, &mut io::stdout().lock());
}

pub(super) fn turn_result(space: &str, source: &str, result: &Result<TurnOutcome, HostError>) {
    emit(&Record::turn(space, source, result));
}
pub(super) fn delivery(space: &str, source: &str, kind: Delivery, succeeded: bool) {
    emit(&Record::delivery(space, source, kind, succeeded));
}

#[cfg(test)]
mod tests;
