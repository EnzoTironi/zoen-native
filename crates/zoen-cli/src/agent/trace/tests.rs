//! Actual outcome encoding with synthetic canaries; no account, provider or filesystem.
use super::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use zoen_agentd::{
    AgentKind, AgentSpec, Definition, ModelTier, ToolAsk, ToolAskEnvelope, ToolCall, ToolDef,
    ToolStep,
};

const CANARY: &str = "TRACE_PRIVATE_CANARY_7e63\n\"provider/token/persona\"";
const SPACE: &str = "sp_trace_synthetic";
const SOURCE: &str = "ev_trace_synthetic";

fn ask(rule: &str) -> ToolAsk {
    let call = ToolCall {
        id: CANARY.into(),
        name: CANARY.into(),
        arguments: json!({"private": CANARY}),
    };
    ToolAsk {
        version: 1,
        policy_sha256: CANARY.into(),
        envelope: ToolAskEnvelope {
            agent: CANARY.into(),
            device: CANARY.into(),
            owner: CANARY.into(),
            space: CANARY.into(),
            from: CANARY.into(),
            inbound_sha256: CANARY.into(),
        },
        definition: ToolDef {
            name: CANARY.into(),
            description: CANARY.into(),
            parameters: json!({"private": CANARY}),
        },
        steps: vec![ToolStep {
            call: call.clone(),
            result: CANARY.into(),
            is_error: false,
        }],
        remaining: vec![call.clone()],
        call,
        round: 1,
        rule: rule.into(),
    }
}
fn spec() -> AgentSpec {
    AgentSpec {
        name: CANARY.into(),
        handle: CANARY.into(),
        glyph: CANARY.into(),
        tint_hex: CANARY.into(),
        definition: Definition {
            version: 1,
            kind: AgentKind::User,
            persona: CANARY.into(),
            rules: vec![CANARY.into()],
            state: json!({"private": CANARY}),
            model_tier: ModelTier::Cheap,
        },
    }
}
fn encoded(record: &Record) -> (String, Value) {
    let mut bytes = Vec::new();
    record.write_to(&mut bytes).unwrap();
    let line = String::from_utf8(bytes).unwrap();
    assert_eq!(line.lines().count(), 1);
    assert!(line.ends_with('\n'));
    let value: Value = serde_json::from_str(&line).unwrap();
    (line, value)
}
fn assert_schema(value: &Value) {
    let object = value.as_object().unwrap();
    assert!(object.keys().all(|key| matches!(
        key.as_str(),
        "event"
            | "correlation"
            | "phase"
            | "outcome"
            | "ruleClass"
            | "completedSteps"
            | "failedSteps"
            | "remainingCalls"
            | "roundIndex"
            | "requestCorrelation"
            | "requestStatus"
            | "stepIndex"
    )));
    assert!(object.len() >= 4);
    assert_eq!(value["event"], EVENT);
    let correlation = value["correlation"].as_str().unwrap();
    assert_eq!(correlation.len(), 64);
    assert!(correlation
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
}

#[test]
fn actual_turn_variants_are_distinct_and_cannot_disclose_their_payloads() {
    let cases = [
        (Ok(TurnOutcome::Silence), "silence"),
        (
            Ok(TurnOutcome::Blocked {
                reason: CANARY.into(),
            }),
            "blocked",
        ),
        (
            Ok(TurnOutcome::AskOwner {
                draft: CANARY.into(),
                rule: CANARY.into(),
            }),
            "ask_owner",
        ),
        (
            Ok(TurnOutcome::Say {
                text: CANARY.into(),
            }),
            "say",
        ),
        (Ok(TurnOutcome::ToolAsk { ask: ask(CANARY) }), "tool_ask"),
        (
            Ok(TurnOutcome::ProposeAgent {
                spec: spec(),
                confirmation: Some(CANARY.into()),
            }),
            "propose_agent",
        ),
        (Err(HostError::Brain(CANARY.into())), "runtime_error"),
    ];
    for (result, expected) in cases {
        let unchanged = result.clone();
        let (line, value) = encoded(&Record::turn(SPACE, SOURCE, &result));
        assert_schema(&value);
        assert_eq!(value["phase"], "turn");
        assert_eq!(value["outcome"], expected);
        assert!(!line.contains("TRACE_PRIVATE_CANARY"));
        assert!(!line.contains("provider/token/persona"));
        assert!(!line.contains(SPACE));
        assert!(!line.contains(SOURCE));
        assert_eq!(result, unchanged, "tracing must not modify the outcome");
    }
}

#[test]
fn only_exact_known_rules_are_classified() {
    for (rule, expected) in [
        ("hard:injection-marker", "hard_injection_marker"),
        ("hard:hidden-characters", "hard_hidden_characters"),
        ("model:injection", "model_injection"),
        ("model:low-confidence", "model_low_confidence"),
        ("model:injection TRACE_PRIVATE_CANARY", "other"),
        ("fail:ask (TRACE_PRIVATE_CANARY)", "other"),
        ("unknown", "other"),
    ] {
        let result = Ok(TurnOutcome::Blocked {
            reason: rule.into(),
        });
        let (line, value) = encoded(&Record::turn(SPACE, SOURCE, &result));
        assert_eq!(value["ruleClass"], expected);
        assert!(!line.contains("TRACE_PRIVATE_CANARY"));
    }
}

#[test]
fn held_tool_and_owner_note_keep_only_the_closed_rule_class() {
    for result in [
        Ok(TurnOutcome::ToolAsk {
            ask: ask("model:low-confidence"),
        }),
        Ok(TurnOutcome::AskOwner {
            draft: CANARY.into(),
            rule: "model:low-confidence".into(),
        }),
    ] {
        let (line, value) = encoded(&Record::turn(SPACE, SOURCE, &result));
        assert_eq!(value["ruleClass"], "model_low_confidence");
        assert!(!line.contains("TRACE_PRIVATE_CANARY"));
    }
}

#[test]
fn all_runtime_errors_are_closed_without_displaying_the_error() {
    for error in [
        HostError::Unknown,
        HostError::NotMember,
        HostError::Brain(CANARY.into()),
        HostError::ToolContinuation(CANARY.into()),
        HostError::ToolAdmission(CANARY.into()),
    ] {
        let (line, value) = encoded(&Record::turn(SPACE, SOURCE, &Err(error)));
        assert_schema(&value);
        assert_eq!(value["outcome"], "runtime_error");
        assert!(value.get("ruleClass").is_none());
        assert!(!line.contains("TRACE_PRIVATE_CANARY"));
    }
}

#[test]
fn successful_or_failed_delivery_is_not_a_completed_tool_or_mutation() {
    for (kind, phase) in [
        (Delivery::Reply, "reply"),
        (Delivery::OwnerNote, "owner_note"),
        (Delivery::ApprovalStaging, "approval_staging"),
    ] {
        for succeeded in [true, false] {
            let (line, value) = encoded(&Record::delivery(SPACE, SOURCE, kind, succeeded));
            assert_schema(&value);
            assert_eq!(value["phase"], phase);
            let expected = match (phase, succeeded) {
                (_, false) => "failed",
                ("approval_staging", true) => "ok",
                (_, true) => "queued",
            };
            assert_eq!(value["outcome"], expected);
            assert_eq!(value["correlation"], correlate(SPACE, SOURCE));
            assert!(value.get("ruleClass").is_none());
            for unobserved in ["mutation", "completed", "accepted", "delivered", "replayed"] {
                assert!(!line.contains(unobserved));
            }
        }
    }
}

#[test]
fn correlation_is_domain_separated_length_framed_and_uses_actual_locators() {
    assert_eq!(correlate(SPACE, SOURCE), correlate(SPACE, SOURCE));
    assert_ne!(correlate("ab", "c"), correlate("a", "bc"));
    assert_ne!(correlate(SPACE, SOURCE), correlate(SOURCE, SPACE));
    assert_ne!(correlate(SPACE, SOURCE), correlate(SPACE, "another-source"));
    assert_ne!(correlate(SPACE, SOURCE), correlate("another-space", SOURCE));
    assert_ne!(correlate("", "x"), correlate("x", ""));
    assert_eq!(
        correlate("sp_ä", "event-☃"),
        "eae7bda2f0d2a0f5a34a59801af6b3a2dfa9fec31a1ddc1f946a21ab609164af"
    );
    let (line, _) = encoded(&Record::turn(CANARY, CANARY, &Ok(TurnOutcome::Silence)));
    assert!(!line.contains("TRACE_PRIVATE_CANARY"));
}

struct BrokenWriter {
    calls: usize,
}
impl Write for BrokenWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        Err(io::Error::from(io::ErrorKind::BrokenPipe))
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::BrokenPipe))
    }
}
#[test]
fn writer_errors_do_not_panic_retry_or_modify_an_outcome() {
    let result = Err(HostError::Brain(CANARY.into()));
    let unchanged = result.clone();
    let record = Record::turn(SPACE, SOURCE, &result);
    let mut writer = BrokenWriter { calls: 0 };
    emit_to(&record, &mut writer);
    assert_eq!(writer.calls, 1, "diagnostics must not retry a failed write");
    assert_eq!(result, unchanged);
}

// Test-only tap of actual emitted records, filtered by synthetic fixture locators.
// It never opens an account, intercepts stdout, or exists in a production build.
type Records = Arc<Mutex<Vec<Value>>>;
static CAPTURES: Mutex<BTreeMap<String, Records>> = Mutex::new(BTreeMap::new());
pub(in crate::agent) struct Capture {
    key: String,
    records: Records,
}
impl Capture {
    pub(in crate::agent) fn values(&self) -> Vec<Value> {
        self.records.lock().unwrap().clone()
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        CAPTURES.lock().unwrap().remove(&self.key);
    }
}
pub(in crate::agent) fn capture(space: &str, source: &str) -> Capture {
    let key = correlate(space, source);
    let records = Arc::new(Mutex::new(Vec::new()));
    assert!(CAPTURES
        .lock()
        .unwrap()
        .insert(key.clone(), records.clone())
        .is_none());
    Capture { key, records }
}
pub(super) fn capture_record(record: &Record) {
    let target = CAPTURES.lock().unwrap().get(&record.correlation).cloned();
    if let Some(target) = target {
        target.lock().unwrap().push(encoded(record).1);
    }
}

fn dto(status: RequestStatus) -> AgentRequestDto {
    serde_json::from_value(json!({
        "tool_approval":{"content_hash":CANARY,"artifact_id":CANARY,"artifact_title":CANARY,
            "version":1,"operation":CANARY,"expires_ms":1},
        "id":CANARY,"agent":{"id":CANARY,"kind":"Agent","name":CANARY,"handle":CANARY,
            "initials":CANARY,"tint_hex":CANARY,"glyph":CANARY,"bio":CANARY,"owner_id":CANARY,
            "owner_name":CANARY,"owner_tint_hex":CANARY,"photo_sha256":CANARY,"photo_ready":true,
            "photo_mime":CANARY,"is_me":true,"is_mine":true},
        "space_id":CANARY,"space_title":CANARY,"title":CANARY,"detail":CANARY,"audience":CANARY,
        "action_label":CANARY,"cost_cents":null,"reason":CANARY,"status":status,
        "opened_ms":1,"resolved_ms":null,"item_id":CANARY,"line_id":CANARY,"action_key":CANARY,
        "can_always_approve":false,"by_standing":false
    }))
    .unwrap()
}

#[test]
fn held_counts_come_from_actual_steps_and_preserve_errors_without_payloads() {
    let mut held = ask(CANARY);
    held.steps.push(ToolStep {
        is_error: true,
        ..held.steps[0].clone()
    });
    held.remaining.push(held.call.clone());
    held.round = 3;
    let original = held.clone();
    let (line, value) = encoded(&Record::turn(
        SPACE,
        SOURCE,
        &Ok(TurnOutcome::ToolAsk { ask: held.clone() }),
    ));
    assert_schema(&value);
    assert_eq!(value["completedSteps"], 2);
    assert_eq!(value["failedSteps"], 1);
    assert_eq!(value["remainingCalls"], 2);
    assert_eq!(value["roundIndex"], 3);
    assert_eq!(held, original);
    assert!(!line.contains("TRACE_PRIVATE_CANARY"));
}

#[test]
fn path_and_staging_correlate_the_same_request_without_claiming_it_is_new() {
    let held = ask(CANARY);
    for status in [
        RequestStatus::Pending,
        RequestStatus::Approved,
        RequestStatus::Denied,
        RequestStatus::Cancelled,
        RequestStatus::Stale,
    ] {
        let request = dto(status);
        let original = request.clone();
        let (line, staged) = encoded(&Record::staged(SPACE, SOURCE, &held, &request));
        let (_, approved) = encoded(&Record::path(
            SPACE,
            SOURCE,
            Path::Approved,
            Some(&request.id),
            Counts::ask(&held),
        ));
        assert_schema(&staged);
        assert_eq!(staged["requestCorrelation"], approved["requestCorrelation"]);
        assert_ne!(staged["requestCorrelation"], staged["correlation"]);
        assert_eq!(staged["phase"], "approval_staging");
        assert_eq!(staged["outcome"], "ok");
        assert_eq!(staged["stepIndex"], held.steps.len());
        assert_eq!(
            staged["requestStatus"],
            serde_json::to_value(status)
                .unwrap()
                .as_str()
                .unwrap()
                .to_lowercase()
        );
        assert_eq!(request, original);
        for forbidden in [
            "TRACE_PRIVATE_CANARY",
            "created",
            "new",
            "delivered",
            "mutation",
            "replayed",
        ] {
            assert!(!line.contains(forbidden));
        }
    }
    assert_ne!(
        correlate_request("ab", "c", "d"),
        correlate_request("a", "bc", "d")
    );
    assert_ne!(
        correlate_request(SPACE, SOURCE, "a"),
        correlate_request(SPACE, SOURCE, "b")
    );
    assert_ne!(
        correlate_request(SPACE, SOURCE, "a"),
        correlate_request(SPACE, "other", "a")
    );
    assert_ne!(
        correlate_request(SPACE, SOURCE, "a"),
        correlate_request("other", SOURCE, "a")
    );
}

#[test]
fn normal_and_recovered_paths_are_distinct_and_normal_can_have_prior_steps() {
    let held = ask(CANARY);
    let progress = ToolProgress {
        round: 2,
        steps: held.steps.clone(),
        remaining: held.remaining.clone(),
    };
    for (path, counts, expected) in [
        (Path::Normal, Counts::steps(&held.steps), "normal"),
        (Path::Approved, Counts::ask(&held), "approved"),
        (Path::Progress, Counts::progress(&progress), "progress"),
    ] {
        let record = Record::path(SPACE, SOURCE, path, Some(CANARY), counts);
        let (line, value) = encoded(&record);
        assert_schema(&value);
        assert_eq!(value["phase"], "continuation");
        assert_eq!(value["outcome"], expected);
        assert_eq!(value["completedSteps"], 1);
        assert_eq!(value["failedSteps"], 0);
        assert!(!line.contains("TRACE_PRIVATE_CANARY"));
        if expected == "normal" {
            assert!(value.get("roundIndex").is_none());
            assert!(value.get("remainingCalls").is_none());
        }
        let mut broken = BrokenWriter { calls: 0 };
        emit_to(&record, &mut broken);
        assert_eq!(broken.calls, 1);
    }
}

#[test]
fn invalid_counts_are_omitted_instead_of_clamped_or_panicking() {
    let held = ask(CANARY);
    let steps = vec![held.steps[0].clone(); 16];
    assert!(Counts::steps(&steps).is_some());
    assert!(Counts::steps(&vec![held.steps[0].clone(); 17]).is_none());
    assert!(Counts::batch(&steps, 0, 3, false).is_some());
    assert!(Counts::batch(&steps, 0, 3, true).is_none());
    assert!(Counts::batch(&steps[..12], 3, 3, true).is_some());
    for (remaining, round) in [(4, 0), (0, 4), (usize::MAX, 0), (0, usize::MAX)] {
        assert!(Counts::batch(&[], remaining, round, false).is_none());
    }
    let mut invalid = held;
    invalid.round = usize::MAX;
    let (_, value) = encoded(&Record::turn(
        SPACE,
        SOURCE,
        &Ok(TurnOutcome::ToolAsk { ask: invalid }),
    ));
    for key in [
        "completedSteps",
        "failedSteps",
        "remainingCalls",
        "roundIndex",
    ] {
        assert!(value.get(key).is_none());
    }
}

#[test]
fn early_stops_and_request_status_mapping_are_closed_and_never_run_a_turn() {
    for (status, expected) in [
        (RequestStatus::Pending, "pending"),
        (RequestStatus::Denied, "denied"),
        (RequestStatus::Cancelled, "cancelled"),
        (RequestStatus::Stale, "stale"),
    ] {
        assert_eq!(Stop::request_status(status).unwrap().id(), expected);
    }
    assert!(Stop::request_status(RequestStatus::Approved).is_none());
    for stop in [
        Stop::AlreadyReplied,
        Stop::RequestUnavailable,
        Stop::Pending,
        Stop::Denied,
        Stop::Cancelled,
        Stop::Stale,
        Stop::ExecutorUnavailable,
        Stop::InvalidContinuation,
        Stop::CheckpointRefused,
        Stop::RecoveryRefused,
        Stop::AttemptLimit,
        Stop::ReservationRefused,
        Stop::ApprovalRefused,
    ] {
        let record = Record::stop(SPACE, SOURCE, stop, Some(CANARY));
        let (line, value) = encoded(&record);
        assert_schema(&value);
        assert_eq!(value["phase"], "continuation_stop");
        assert_eq!(value["outcome"], stop.id());
        assert!(!line.contains("TRACE_PRIVATE_CANARY"));
        let mut broken = BrokenWriter { calls: 0 };
        emit_to(&record, &mut broken);
        assert_eq!(broken.calls, 1);
    }
}
