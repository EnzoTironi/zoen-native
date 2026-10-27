//! Synthetic typed errors only: no provider, account, journal or executor.
use super::*;
use serde_json::Value;

const CANARY: &str = "PRIVATE_ADMISSION_CANARY/token\n\"args/results\"";
const SPACE: &str = "sp_synthetic_admission";
const SOURCE: &str = "ev_synthetic_admission";
const REQUEST: &str = "rq_synthetic_admission";

fn errors(reason: &str) -> [CoreError; 6] {
    [
        CoreError::Storage {
            message: reason.into(),
        },
        CoreError::NotFound {
            what: reason.into(),
        },
        CoreError::Forbidden {
            reason: reason.into(),
        },
        CoreError::Invalid {
            reason: reason.into(),
        },
        CoreError::Stale {
            reason: reason.into(),
        },
        CoreError::DraftRejected {
            reason: reason.into(),
        },
    ]
}
fn encoded(refusal: Refusal) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_refusal(true, SPACE, SOURCE, REQUEST, refusal, &mut bytes);
    bytes
}

#[test]
fn admission_is_explicit_opt_in_and_default_off_preserves_existing_outcome_bytes() {
    for flag in [
        None,
        Some(""),
        Some("0"),
        Some("true"),
        Some(" 1"),
        Some("1\n"),
    ] {
        assert!(!enabled(flag));
        let mut before = Vec::new();
        super::super::Record::stop(
            SPACE,
            SOURCE,
            super::super::Stop::ApprovalRefused,
            Some(REQUEST),
        )
        .write_to(&mut before)
        .unwrap();
        for error in errors(CANARY) {
            let mut after = before.clone();
            write_refusal(
                enabled(flag),
                SPACE,
                SOURCE,
                REQUEST,
                Refusal::core(Phase::DeckPermit, error),
                &mut after,
            );
            assert_eq!(after, before);
        }
    }
    assert!(enabled(Some("1")));
}

#[test]
fn all_core_variants_are_typed_without_copying_display_or_error_fields() {
    for phase in [Phase::DeckPermit, Phase::PagePermit, Phase::ScenePermit] {
        for (error, variant) in errors(CANARY).into_iter().zip([
            "storage",
            "not_found",
            "forbidden",
            "invalid",
            "stale",
            "draft_rejected",
        ]) {
            let bytes = encoded(Refusal::core(phase, error));
            let text = std::str::from_utf8(&bytes).unwrap();
            assert!(!text.contains(CANARY));
            assert!(!text.contains("token"));
            let value: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(value["variant"], variant);
            assert_eq!(value["phase"], phase.id());
            assert_eq!(value["code"], "core_refused");
            assert_eq!(value["event"], EVENT);
            assert_eq!(value["correlation"], correlate(SPACE, SOURCE));
            assert_eq!(
                value["requestCorrelation"],
                correlate_request(SPACE, SOURCE, REQUEST)
            );
            assert_eq!(value.as_object().unwrap().len(), 6);
            assert!(bytes.len() <= MAX_LINE && bytes.ends_with(b"\n"));
        }
    }
}

#[test]
fn stale_or_forbidden_tags_never_guess_expiry_version_or_revocation_from_message() {
    for reason in ["expired", "version changed", "device revoked", CANARY] {
        let stale = encoded(Refusal::core(
            Phase::DeckPermit,
            CoreError::Stale {
                reason: reason.into(),
            },
        ));
        let denied = encoded(Refusal::core(
            Phase::DeckPermit,
            CoreError::Forbidden {
                reason: reason.into(),
            },
        ));
        assert_eq!(
            stale,
            encoded(Refusal::core(
                Phase::DeckPermit,
                CoreError::Stale {
                    reason: String::new()
                }
            ))
        );
        assert_eq!(
            denied,
            encoded(Refusal::core(
                Phase::DeckPermit,
                CoreError::Forbidden {
                    reason: String::new()
                }
            ))
        );
    }
}

#[test]
fn non_core_refusals_remain_closed_and_selection_cause_is_unknown() {
    for phase in [Phase::SelectionBefore, Phase::SelectionAfter] {
        let value: Value = serde_json::from_slice(&encoded(Refusal::selection(phase))).unwrap();
        assert_eq!(value["phase"], phase.id());
        assert_eq!(value["variant"], "unknown");
        assert_eq!(value["code"], "selection_refused");
    }
    for (refusal, phase, code) in [
        (Refusal::executor(), "deck_permit", "executor_unavailable"),
        (Refusal::family(), "family_selection", "unsupported_schema"),
    ] {
        let value: Value = serde_json::from_slice(&encoded(refusal)).unwrap();
        assert_eq!(value["phase"], phase);
        assert_eq!(value["variant"], "not_applicable");
        assert_eq!(value["code"], code);
    }
}

#[test]
fn arbitrary_locator_bytes_only_affect_fixed_size_hashes() {
    let locator = CANARY.repeat(4096);
    let mut bytes = Vec::new();
    write_refusal(
        true,
        &locator,
        &locator,
        &locator,
        Refusal::family(),
        &mut bytes,
    );
    assert!(bytes.len() <= MAX_LINE && bytes.iter().filter(|&&v| v == b'\n').count() == 1);
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["correlation"].as_str().unwrap().len(), 64);
    assert_eq!(value["requestCorrelation"].as_str().unwrap().len(), 64);
    assert!(!String::from_utf8(bytes).unwrap().contains("PRIVATE"));
}

#[test]
fn failed_diagnostic_io_never_panics_or_reclassifies_admission() {
    struct Failed;
    impl Write for Failed {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    write_refusal(true, SPACE, SOURCE, REQUEST, Refusal::family(), &mut Failed);
}
