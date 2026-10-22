//! Actual outcome encoding with synthetic canaries; no account, provider or filesystem.
use super::*;
use serde_json::{json, Value};
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
        "event" | "correlation" | "phase" | "outcome" | "ruleClass"
    )));
    assert!(object.len() == 4 || object.len() == 5);
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
