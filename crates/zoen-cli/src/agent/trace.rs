//! Public operational outcomes only. No payload, authority, liveness or provider meter.
use client_core::{AgentRequestDto, RequestStatus};
use sha2::{Digest, Sha256};
use std::io::{self, Write};
use zoen_agentd::{HostError, ToolAsk, ToolProgress, ToolStep, TurnOutcome};

const EVENT: &str = "zoen.agent.outcome.v1";
const LOCATOR_DOMAIN: &[u8] = b"zoen.agent.outcome.locators.v1\0";
const REQUEST_DOMAIN: &[u8] = b"zoen.agent.outcome.request-locators.v1\0";

#[derive(Clone, Copy)]
pub(super) enum Path {
    Normal,
    Approved,
    Progress,
}
impl Path {
    fn id(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Approved => "approved",
            Self::Progress => "progress",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Stop {
    AlreadyReplied,
    RequestUnavailable,
    Pending,
    Denied,
    Cancelled,
    Stale,
    ExecutorUnavailable,
    InvalidContinuation,
    CheckpointRefused,
    RecoveryRefused,
    AttemptLimit,
    ReservationRefused,
    ApprovalRefused,
}
impl Stop {
    pub(super) fn request_status(status: RequestStatus) -> Option<Self> {
        match status {
            RequestStatus::Approved => None,
            RequestStatus::Pending => Some(Self::Pending),
            RequestStatus::Denied => Some(Self::Denied),
            RequestStatus::Cancelled => Some(Self::Cancelled),
            RequestStatus::Stale => Some(Self::Stale),
        }
    }
    fn id(self) -> &'static str {
        match self {
            Self::AlreadyReplied => "already_replied",
            Self::RequestUnavailable => "request_unavailable",
            Self::Pending => "pending",
            Self::Denied => "denied",
            Self::Cancelled => "cancelled",
            Self::Stale => "stale",
            Self::ExecutorUnavailable => "executor_unavailable",
            Self::InvalidContinuation => "invalid_continuation",
            Self::CheckpointRefused => "checkpoint_refused",
            Self::RecoveryRefused => "recovery_refused",
            Self::AttemptLimit => "attempt_limit",
            Self::ReservationRefused => "reservation_refused",
            Self::ApprovalRefused => "approval_refused",
        }
    }
}

/// Counts describe retained/returned context, never provider calls or new effects.
struct Counts {
    completed: usize,
    failed: usize,
    remaining: Option<usize>,
    round: Option<usize>,
}
impl Counts {
    fn steps(steps: &[ToolStep]) -> Option<Self> {
        (steps.len() <= zoen_agentd::MAX_TOOL_STEPS).then(|| Self {
            completed: steps.len(),
            failed: steps.iter().filter(|step| step.is_error).count(),
            remaining: None,
            round: None,
        })
    }
    fn batch(steps: &[ToolStep], remaining: usize, round: usize, held: bool) -> Option<Self> {
        let mut counts = Self::steps(steps)?;
        if round >= zoen_agentd::citizen::MAX_TOOL_ROUNDS
            || remaining >= zoen_agentd::citizen::MAX_CALLS_PER_ROUND
            || steps
                .len()
                .checked_add(remaining)?
                .checked_add(usize::from(held))?
                > zoen_agentd::MAX_TOOL_STEPS
        {
            return None;
        }
        counts.remaining = Some(remaining);
        counts.round = Some(round);
        Some(counts)
    }
    fn ask(ask: &ToolAsk) -> Option<Self> {
        Self::batch(&ask.steps, ask.remaining.len(), ask.round, true)
    }
    fn progress(progress: &ToolProgress) -> Option<Self> {
        Self::batch(
            &progress.steps,
            progress.remaining.len(),
            progress.round,
            false,
        )
    }
    fn add_to(&self, value: &mut serde_json::Value) {
        value["completedSteps"] = self.completed.into();
        value["failedSteps"] = self.failed.into();
        if let Some(remaining) = self.remaining {
            value["remainingCalls"] = remaining.into();
        }
        if let Some(round) = self.round {
            value["roundIndex"] = round.into();
        }
    }
}

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
    Path(Path),
    Stop(Stop),
    Staged,
}
struct Record {
    correlation: String,
    outcome: Outcome,
    rule: Option<RuleClass>,
    counts: Option<Counts>,
    request: Option<String>,
    status: Option<&'static str>,
    step: Option<usize>,
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
            counts: match result {
                Ok(TurnOutcome::ToolAsk { ask }) => Counts::ask(ask),
                _ => None,
            },
            request: None,
            status: None,
            step: None,
        }
    }
    fn delivery(space: &str, source: &str, kind: Delivery, succeeded: bool) -> Self {
        Self {
            correlation: correlate(space, source),
            outcome: Outcome::Delivery { kind, succeeded },
            rule: None,
            counts: None,
            request: None,
            status: None,
            step: None,
        }
    }
    fn path(
        space: &str,
        source: &str,
        path: Path,
        request: Option<&str>,
        counts: Option<Counts>,
    ) -> Self {
        Self {
            correlation: correlate(space, source),
            outcome: Outcome::Path(path),
            rule: None,
            counts,
            request: request.map(|id| correlate_request(space, source, id)),
            status: None,
            step: None,
        }
    }
    fn stop(space: &str, source: &str, stop: Stop, request: Option<&str>) -> Self {
        let mut record = Self::path(space, source, Path::Normal, request, None);
        record.outcome = Outcome::Stop(stop);
        record
    }
    fn staged(space: &str, source: &str, ask: &ToolAsk, dto: &AgentRequestDto) -> Self {
        let mut record = Self::path(space, source, Path::Normal, Some(&dto.id), Counts::ask(ask));
        record.outcome = Outcome::Staged;
        record.status = Some(match dto.status {
            RequestStatus::Cancelled => "cancelled",
            RequestStatus::Pending => "pending",
            RequestStatus::Approved => "approved",
            RequestStatus::Denied => "denied",
            RequestStatus::Stale => "stale",
        });
        record.step = record.counts.as_ref().map(|counts| counts.completed);
        record
    }
    fn write_to(&self, writer: &mut impl Write) -> io::Result<()> {
        let (phase, outcome) = match self.outcome {
            Outcome::Turn(kind) => ("turn", kind.id()),
            Outcome::Path(path) => ("continuation", path.id()),
            Outcome::Stop(stop) => ("continuation_stop", stop.id()),
            Outcome::Staged => ("approval_staging", "ok"),
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
        if let Some(counts) = &self.counts {
            counts.add_to(&mut value);
        }
        if let Some(request) = &self.request {
            value["requestCorrelation"] = request.clone().into();
        }
        if let Some(status) = self.status {
            value["requestStatus"] = status.into();
        }
        if let Some(step) = self.step {
            value["stepIndex"] = step.into();
        }
        serde_json::to_writer(&mut *writer, &value)?;
        writer.write_all(b"\n")
    }
}

fn correlate(space: &str, source: &str) -> String {
    correlate_locators(LOCATOR_DOMAIN, &[space, source])
}
fn correlate_request(space: &str, source: &str, request: &str) -> String {
    correlate_locators(REQUEST_DOMAIN, &[space, source, request])
}
fn correlate_locators(domain: &[u8], locators: &[&str]) -> String {
    let mut hash = Sha256::new();
    hash.update(domain);
    // Domain separation and byte lengths distinguish different pairs of locators.
    for locator in locators {
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
    #[cfg(test)]
    tests::capture_record(record);
    emit_to(record, &mut io::stdout().lock());
}

pub(super) fn turn_result(space: &str, source: &str, result: &Result<TurnOutcome, HostError>) {
    emit(&Record::turn(space, source, result));
}
pub(super) fn delivery(space: &str, source: &str, kind: Delivery, succeeded: bool) {
    emit(&Record::delivery(space, source, kind, succeeded));
}
pub(super) fn normal(space: &str, source: &str, steps: &[ToolStep]) {
    emit(&Record::path(
        space,
        source,
        Path::Normal,
        None,
        Counts::steps(steps),
    ));
}
pub(super) fn approved(space: &str, source: &str, request: &str, ask: &ToolAsk) {
    emit(&Record::path(
        space,
        source,
        Path::Approved,
        Some(request),
        Counts::ask(ask),
    ));
}
pub(super) fn progress(space: &str, source: &str, request: &str, progress: &ToolProgress) {
    emit(&Record::path(
        space,
        source,
        Path::Progress,
        Some(request),
        Counts::progress(progress),
    ));
}
pub(super) fn stop(space: &str, source: &str, stop: Stop, request: Option<&str>) {
    emit(&Record::stop(space, source, stop, request));
}
pub(super) fn staged(space: &str, source: &str, ask: &ToolAsk, dto: &AgentRequestDto) {
    emit(&Record::staged(space, source, ask, dto));
}

#[cfg(test)]
pub(super) mod tests;
