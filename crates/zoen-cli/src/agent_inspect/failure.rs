//! Opt-in closed failure facts; never return arbitrary Core/relay text or change admission.
use client_core::CoreError;
use serde::Serialize;
use std::{io::Write, time::Duration};

#[derive(Serialize)]
struct Fact {
    event: &'static str,
    version: u8,
    phase: &'static str,
    kind: &'static str,
    reported_tag: Option<&'static str>,
    budget_ms: u64,
    elapsed_ms: String,
}
fn fact(error: Option<&CoreError>, budget_ms: u64, elapsed: Duration) -> Fact {
    let (kind, reason) = match error {
        None => ("deadline", None),
        Some(CoreError::Invalid { reason }) => ("core_invalid", Some(reason.as_str())),
        Some(CoreError::Stale { .. }) => ("core_stale", None),
        Some(CoreError::Forbidden { .. }) => ("core_forbidden", None),
        Some(CoreError::Storage { .. }) => ("core_storage", None),
        Some(CoreError::NotFound { .. }) => ("core_not_found", None),
        Some(CoreError::DraftRejected { .. }) => ("core_draft_rejected", None),
    };
    // This is an observed tag, not proof of its alleged server-side cause. A
    // relay may itself return these strings; no authority derives from a fact.
    let reported_tag = match reason {
        Some("complete_read_busy") => Some("complete_read_busy"),
        Some("complete_read_stale") => Some("complete_read_stale"),
        Some("complete_read_expired") => Some("complete_read_expired"),
        Some("invalid_complete_read") => Some("invalid_complete_read"),
        Some("complete_read_profile_exceeded") => Some("complete_read_profile_exceeded"),
        Some("offline") => Some("offline"),
        _ => None,
    };
    Fact {
        event: "zoen_agent_inspect_failure",
        version: 1,
        phase: "state_read",
        kind,
        reported_tag,
        budget_ms,
        elapsed_ms: elapsed.as_millis().to_string(),
    }
}
fn write(error: Option<&CoreError>, budget_ms: u64, elapsed: Duration, sink: &mut impl Write) {
    if let Ok(bytes) = serde_json::to_vec(&fact(error, budget_ms, elapsed)) {
        if bytes.len() <= 512 && sink.write_all(&bytes).is_ok() {
            let _ = sink.write_all(b"\n");
        }
    }
}
pub(super) fn trace(error: Option<&CoreError>, budget_ms: u64, elapsed: Duration) {
    if std::env::var_os("ZOEN_SYNC_WAIT_DIAGNOSTICS").is_some() {
        write(error, budget_ms, elapsed, &mut std::io::stderr().lock());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deadline_and_core_refusal_are_distinct_without_copying_any_private_reason() {
        let mut bytes = Vec::new();
        write(None, 8000, Duration::from_millis(8001), &mut bytes);
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["kind"], "deadline");
        assert_eq!(value["elapsed_ms"], "8001");
        assert_eq!(value["budget_ms"], 8000);
        assert!(value["reported_tag"].is_null());
        let canary = "PRIVATE_REASON_WITH_ID_KEY_CONTENT_AND_URL";
        for error in [
            CoreError::Invalid {
                reason: canary.into(),
            },
            CoreError::Storage {
                message: canary.into(),
            },
            CoreError::Stale {
                reason: canary.into(),
            },
            CoreError::Forbidden {
                reason: canary.into(),
            },
            CoreError::NotFound {
                what: canary.into(),
            },
            CoreError::DraftRejected {
                reason: canary.into(),
            },
        ] {
            bytes.clear();
            write(Some(&error), 8000, Duration::from_millis(2), &mut bytes);
            assert!(bytes.len() < 512);
            assert!(!String::from_utf8_lossy(&bytes).contains(canary));
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_ne!(value["kind"], "deadline");
            assert!(value["reported_tag"].is_null());
        }
    }
    #[test]
    fn only_exact_complete_read_tags_are_observed_and_sink_failure_stays_inert() {
        for tag in [
            "complete_read_busy",
            "complete_read_stale",
            "complete_read_expired",
            "invalid_complete_read",
            "complete_read_profile_exceeded",
            "offline",
        ] {
            let error = CoreError::Invalid { reason: tag.into() };
            assert_eq!(
                fact(Some(&error), 8, Duration::ZERO).reported_tag,
                Some(tag)
            );
        }
        for reason in [
            "complete_read_busy private reason",
            "COMPLETE_READ_BUSY",
            "",
        ] {
            let error = CoreError::Invalid {
                reason: reason.into(),
            };
            assert!(fact(Some(&error), 8, Duration::ZERO).reported_tag.is_none());
        }
        struct Unavailable;
        impl Write for Unavailable {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("sink unavailable"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        write(None, 8, Duration::ZERO, &mut Unavailable);
    }
}
