use super::*;
struct Journal(PathBuf);
impl Journal {
    fn new() -> Self {
        let home = std::env::temp_dir().join(types::new_id("image-host-journal"));
        std::fs::create_dir(&home).unwrap();
        Self(home.join("steps.json"))
    }
}
impl Drop for Journal {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
    }
}
fn call(prompt: &str) -> ToolCall {
    ToolCall {
        name: TOOL.into(),
        arguments: json!({"prompt":prompt}),
    }
}
#[test]
fn stage_scheduling_survives_cold_open_and_lost_subprocess_reply_without_new_request() {
    let journal = Journal::new();
    let call = call("A calm sea");
    let initial = retained_or_schedule(&journal.0, "space", "source", &call).unwrap();
    assert_eq!(
        serde_json::from_str::<Pending>(&initial).unwrap().progress,
        Progress::CatchingUp
    );
    // No process response reached the host. Cold replay keeps the same source
    // and call; Runtime reconciliation, never this journal, recovers its review.
    assert_eq!(
        retained_or_schedule(&journal.0, "space", "source", &call).unwrap(),
        initial
    );
    let published = serde_json::to_string(&Pending {
        schema: "runtime-image/1".into(),
        space: "space".into(),
        source: "source".into(),
        progress: Progress::Review {
            request: "original-request".into(),
            content_hash: "a".repeat(64),
        },
    })
    .unwrap();
    replace_step(&journal.0, "source", &call, &initial, &published).unwrap();
    assert_eq!(
        retained_or_schedule(&journal.0, "space", "source", &call).unwrap(),
        published
    );
    assert_eq!(JournalState::load(&journal.0).unwrap().steps.len(), 1);
    assert!(replace_step(&journal.0, "source", &call, &initial, "changed").is_err());
    assert_eq!(
        retained_or_schedule(&journal.0, "space", "source", &call).unwrap(),
        published
    );
}
#[test]
fn ordinary_history_pruning_keeps_unresolved_review_and_recovery_boundedly() {
    let pending = ToolStep {
        call: call("pending"),
        result: serde_json::to_string(&Pending {
            schema: "runtime-image/1".into(),
            space: "space".into(),
            source: "source".into(),
            progress: Progress::Recovery {
                run: "billed-original".into(),
            },
        })
        .unwrap(),
        is_error: false,
    };
    let ordinary = ToolStep {
        call: ToolCall {
            name: "read_page".into(),
            arguments: json!({}),
        },
        result: "ok".into(),
        is_error: false,
    };
    let mut steps = vec![("source".into(), vec![pending.clone()])];
    steps.extend((0..JOURNAL_KEEP + 30).map(|n| (format!("ordinary-{n}"), vec![ordinary.clone()])));
    trim_completed(&mut steps);
    assert_eq!(steps.len(), JOURNAL_KEEP + 1);
    assert_eq!(steps[0].1, vec![pending]);
    assert_eq!(steps[1].0, "ordinary-30");
}
#[test]
fn full_pending_capacity_refuses_before_executor_without_mutating_journal() {
    let journal = Journal::new();
    for n in 0..JOURNAL_KEEP {
        retained_or_schedule(&journal.0, "space", &format!("source-{n}"), &call("one")).unwrap();
    }
    let before = std::fs::read(&journal.0).unwrap();
    assert!(retained_or_schedule(&journal.0, "space", "overflow", &call("one")).is_err());
    assert_eq!(std::fs::read(&journal.0).unwrap(), before);
    assert_eq!(
        JournalState::load(&journal.0).unwrap().steps.len(),
        JOURNAL_KEEP
    );
}
#[test]
fn content_protocol_rejects_authority_fields_and_bounds_utf8_bytes() {
    let authority = json!({"command":"stage","agent":"agent","space":"space","source":"source","prompt":"paint","approved":true,"credential_key":"foreign"});
    assert!(serde_json::from_value::<Command>(authority).is_err());
    assert!(serde_json::from_value::<Input>(json!({"prompt":"paint","model":"foreign"})).is_err());
    assert!(Command::Stage {
        agent: "agent".into(),
        space: "space".into(),
        source: "source".into(),
        prompt: "🌍".repeat(2049)
    }
    .validate()
    .is_err());
}
