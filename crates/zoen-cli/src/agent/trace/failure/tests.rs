//! Synthetic errors only; never provider execution, accounts or private output.
use super::*;
use serde_json::Value;
use std::collections::BTreeSet;

const SPACE: &str = "sp_failure_synthetic";
const SOURCE: &str = "ev_failure_synthetic";
const CANARY: &str = "PRIVATE_FAILURE_CANARY/token\n\"model/payload\"";

fn encoded(error: HostError) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_result(true, SPACE, SOURCE, &Err(error), &mut bytes);
    bytes
}

fn unknown_cases(message: &str) -> [HostError; 5] {
    [
        HostError::Unknown,
        HostError::NotMember,
        HostError::Brain(message.into()),
        HostError::ToolContinuation(message.into()),
        HostError::ToolAdmission(message.into()),
    ]
}

#[test]
fn opt_in_is_exact_and_default_off_emits_no_additional_bytes() {
    for flag in [
        None,
        Some(""),
        Some("0"),
        Some("true"),
        Some(" 1"),
        Some("1\n"),
    ] {
        assert!(!enabled(flag));
        for error in unknown_cases(CANARY) {
            let result = Err(error);
            let mut before = Vec::new();
            super::super::Record::turn(SPACE, SOURCE, &result)
                .write_to(&mut before)
                .unwrap();
            let mut after = before.clone();
            write_result(enabled(flag), SPACE, SOURCE, &result, &mut after);
            assert_eq!(before, after, "outcome.v1 stays byte-exact when disabled");
        }
    }
    assert!(enabled(Some("1")));
    let mut bytes = Vec::new();
    write_result(true, SPACE, SOURCE, &Ok(TurnOutcome::Silence), &mut bytes);
    assert!(bytes.is_empty(), "successful outcomes add no failure event");
}

#[test]
fn all_variants_are_closed_without_displaying_any_error_or_locator() {
    for (error, variant) in unknown_cases(CANARY).into_iter().zip([
        "unknown_agent",
        "not_member",
        "brain",
        "tool_continuation",
        "tool_admission",
    ]) {
        let unchanged = error.clone();
        let line = encoded(error.clone());
        assert_eq!(error, unchanged);
        assert!(line.len() <= MAX_LINE && line.ends_with(b"\n"));
        let text = std::str::from_utf8(&line).unwrap();
        assert!(!text.contains("PRIVATE_FAILURE_CANARY"));
        assert!(!text.contains("model/payload"));
        assert!(!text.contains(SPACE) && !text.contains(SOURCE));
        let value: Value = serde_json::from_slice(&line).unwrap();
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), 5);
        assert_eq!(value["event"], EVENT);
        assert_eq!(value["phase"], "turn_result");
        assert_eq!(value["variant"], variant);
        assert_eq!(value["code"], "unknown");
        assert_eq!(value["correlation"], correlate(SPACE, SOURCE));
    }
}

#[test]
fn full_adapter_constants_only_never_prefix_substring_or_cross_variant() {
    let adapter = include_str!("../../../../../zoen-agentd/src/citizen/codex.rs");
    let session = include_str!("../../../../../zoen-agentd/src/citizen/codex/session.rs");
    let mut codes = BTreeSet::new();
    let mut messages = BTreeSet::new();
    for (message, code) in BRAIN_CODES {
        // This checks public source literals, not any runtime/provider content.
        let literal = format!("\"{message}\"");
        assert!(
            adapter.contains(&literal) || session.contains(&literal),
            "unverified adapter constant"
        );
        assert!(messages.insert(*message) && codes.insert(*code));
        assert_eq!(
            classification(&HostError::Brain((*message).into())),
            ("brain", *code)
        );
        for changed in [
            format!("{message}{CANARY}"),
            format!("{CANARY}{message}"),
            format!(" {message}"),
        ] {
            assert_eq!(
                classification(&HostError::Brain(changed)),
                ("brain", "unknown")
            );
        }
        assert_eq!(
            classification(&HostError::ToolAdmission((*message).into())),
            ("tool_admission", "unknown")
        );
        assert_eq!(
            classification(&HostError::ToolContinuation((*message).into())),
            ("tool_continuation", "unknown")
        );
    }
}

#[test]
fn huge_unknown_error_stays_bounded_and_write_failure_cannot_change_result() {
    let result = Err(HostError::Brain(CANARY.repeat(32_768)));
    let unchanged = result.clone();
    let mut bytes = Vec::new();
    write_result(true, SPACE, SOURCE, &result, &mut bytes);
    assert!(bytes.len() <= MAX_LINE);
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap()["code"],
        "unknown"
    );
    struct Failing;
    impl Write for Failing {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    write_result(true, SPACE, SOURCE, &result, &mut Failing);
    assert_eq!(result, unchanged);
}

#[test]
fn actual_encoder_matches_shared_closed_collector_fixture() {
    let mut bytes = Vec::new();
    for error in unknown_cases("unknown adapter error") {
        bytes.extend(encoded(error));
    }
    for (message, _) in BRAIN_CODES {
        bytes.extend(encoded(HostError::Brain((*message).into())));
    }
    assert_eq!(bytes, include_bytes!("fixtures/failure-v1.ndjson"));
}
