use super::*;

fn connection() -> ConnectionDto {
    ConnectionDto {
        state: "online".into(),
        synced: true,
        pending: 0,
        error: None,
    }
}

#[test]
fn each_original_condition_short_circuits_in_the_original_order() {
    for mask in 0u8..16 {
        let mut called = Vec::new();
        let observed = SyncCompletionObservation::evaluate(&connection(), |check| {
            let n = match check {
                Check::Unknown => 0,
                Check::Uploads => 1,
                Check::Profiles => 2,
                Check::Mls => 3,
            };
            called.push(n);
            mask & (1 << n) != 0
        });
        let first_false = (0..4).find(|n| mask & (1 << n) == 0);
        let evaluated = first_false.map_or(4, |n| n + 1);
        assert_eq!(called, (0..evaluated).collect::<Vec<_>>());
        assert_eq!(observed.satisfied(), mask == 15);
        let flags = [
            observed.unknown_ready,
            observed.uploads_ready,
            observed.profiles_ready,
            observed.mls_ready,
        ];
        for (n, flag) in flags.into_iter().enumerate() {
            assert_eq!(flag, (n < evaluated).then_some(mask & (1 << n) != 0));
        }
    }
}

#[test]
fn connection_preconditions_never_enter_engine_checks() {
    for c in [
        ConnectionDto {
            state: "offline".into(),
            ..connection()
        },
        ConnectionDto {
            state: "connecting".into(),
            ..connection()
        },
        ConnectionDto {
            state: "secret-state-canary".into(),
            ..connection()
        },
        ConnectionDto {
            synced: false,
            ..connection()
        },
        ConnectionDto {
            pending: 7,
            ..connection()
        },
    ] {
        let observed = SyncCompletionObservation::evaluate(&c, |_| panic!("must not lock Engine"));
        assert!(!observed.satisfied());
        assert!(observed.unknown_ready.is_none());
        let text = serde_json::to_string(&observed).unwrap();
        assert!(!text.contains("secret-state-canary"));
    }
}

#[test]
fn actual_engine_unknown_state_is_captured_once_without_retaining_identity_or_error() {
    let client = Client::open(":memory:".into(), "en".into()).unwrap();
    client
        .lock()
        .net
        .unknown
        .insert("secret-identity-canary".into());
    let c = ConnectionDto {
        error: Some("secret-error-canary".into()),
        ..connection()
    };
    let observed = client.sync_completion_observation(&c);
    assert_eq!(observed.unknown_ready, Some(false));
    assert!(observed.uploads_ready.is_none());
    client.lock().net.unknown.clear();
    assert!(client.sync_idle(&c));
    let mut bytes = Vec::new();
    observed.write_deadline(80, Duration::from_millis(80), &mut bytes);
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains("canary"));
    assert!(text.contains("\"unknown_ready\":false"));
    assert!(text.contains("\"uploads_ready\":null"));
}

#[test]
fn actual_scheduled_mls_work_is_not_hidden_by_an_empty_event_outbox() {
    let client = Client::open(":memory:".into(), "en".into()).unwrap();
    client.lock().net.mls.wake_recovery("secret-space-canary");
    let observed = client.sync_completion_observation(&connection());
    assert_eq!(observed.unknown_ready, Some(true));
    assert_eq!(observed.uploads_ready, Some(true));
    assert_eq!(observed.profiles_ready, Some(true));
    assert_eq!(observed.mls_ready, Some(false));
    assert_eq!(observed.pending, 0);
    assert!(!observed.satisfied());
    assert!(!serde_json::to_string(&observed).unwrap().contains("canary"));
}

#[test]
fn success_emits_nothing_and_failed_writer_cannot_change_the_decision() {
    struct Refuses;
    impl Write for Refuses {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let ready = SyncCompletionObservation::evaluate(&connection(), |_| true);
    let mut bytes = Vec::new();
    ready.write_deadline(0, Duration::ZERO, &mut bytes);
    assert!(bytes.is_empty());
    let blocked = SyncCompletionObservation::evaluate(&connection(), |_| false);
    blocked.write_deadline(0, Duration::ZERO, &mut Refuses);
    assert!(!blocked.satisfied());
}

#[test]
fn deadline_schema_is_closed_bounded_and_records_only_evaluated_flags() {
    let c = ConnectionDto {
        pending: u64::MAX,
        error: Some("secret-token-canary".repeat(500)),
        ..connection()
    };
    let observed = SyncCompletionObservation::evaluate(&c, |_| panic!("connection blocked"));
    let mut bytes = Vec::new();
    observed.write_deadline(u64::MAX, Duration::from_millis(u64::MAX), &mut bytes);
    assert!(bytes.len() <= 2_048);
    let record: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(record.as_object().unwrap().len(), 11);
    assert_eq!(record["event"], "zoen_sync_wait_deadline");
    assert_eq!(record["version"], 1);
    assert_eq!(record["pending"], u64::MAX);
    assert_eq!(record["mls_ready"], serde_json::Value::Null);
    assert!(!String::from_utf8(bytes).unwrap().contains("canary"));
}

#[tokio::test(start_paused = true)]
async fn actual_wait_keeps_the_original_deadline_and_does_not_make_offline_ready() {
    let client = Client::open(":memory:".into(), "en".into()).unwrap();
    let start = tokio::time::Instant::now();
    let result = client.wait_until_idle(80).await;
    assert_eq!(start.elapsed(), Duration::from_millis(80));
    assert_eq!(result.state, "offline");
    assert!(!result.synced);
    assert!(client.wait_until_settled(0).await.is_err());
}
