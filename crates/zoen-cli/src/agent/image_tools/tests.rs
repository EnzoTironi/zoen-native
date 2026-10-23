use super::*;
use std::path::PathBuf;
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
        id: "image-call".into(),
        requires_review: false,
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
        review: None,
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
            review: None,
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
            id: "read-call".into(),
            requires_review: false,
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
fn request(status: client_core::RequestStatus, expires_ms: i64) -> client_core::AgentRequestDto {
    serde_json::from_value(json!({
        "tool_approval":{"content_hash":"a".repeat(64),"artifact_id":"space","artifact_title":"Image",
            "version":1,"operation":"generate_image","expires_ms":expires_ms,
            "image":{"prompt":"sea","size":"1024x1024","quality":"low","background":"opaque","definition_version":"v1","price_digest":"b".repeat(64),"max_image_bytes":1000,"source_id":"source","source_sha256":"c".repeat(64)}},
        "id":"original-request","agent":{"id":"agent","kind":"Agent","name":"Agent","handle":"agent",
            "initials":"A","tint_hex":"#000000","glyph":null,"bio":"","owner_id":"owner",
            "owner_name":null,"owner_tint_hex":null,"photo_sha256":null,"photo_ready":false,
            "photo_mime":null,"is_me":false,"is_mine":false},
        "space_id":"space","space_title":"Chat","title":"Image","detail":"sea","audience":"owner",
        "action_label":"Approve","cost_cents":null,"reason":"review","status":status,
        "opened_ms":1,"resolved_ms":null,"item_id":null,"line_id":null,"action_key":"image",
        "can_always_approve":false,"by_standing":false
    })).unwrap()
}
fn reviewed() -> Pending {
    let mut pending = Pending {
        schema: "runtime-image/1".into(),
        space: "space".into(),
        source: "source".into(),
        progress: Progress::CatchingUp,
        review: None,
    };
    pending.observe(Progress::Review {
        request: "original-request".into(),
        content_hash: "a".repeat(64),
    });
    pending
}
#[test]
fn cold_financial_recovery_rechecks_the_same_request_and_cannot_restage() {
    let mut pending = reviewed();
    pending.observe(Progress::Recovery {
        run: "original-billed-run".into(),
    });
    let bytes = serde_json::to_vec(&pending).unwrap();
    let pending: Pending = serde_json::from_slice(&bytes).unwrap();
    let requests = vec![request(client_core::RequestStatus::Approved, 500)];
    assert!(canonical_request(&pending, "agent", &requests).is_some());
    match pending.advance_command("agent").unwrap() {
        Command::Advance { request, space, .. } => {
            assert_eq!(request, "original-request");
            assert_eq!(space, "space");
        }
        Command::Stage { .. } | Command::PolicyImport { .. } | Command::PolicyContext { .. } => {
            panic!("financial recovery must never create another review/provider attempt")
        }
    }
    for case in 0..4 {
        let mut wrong = requests.clone();
        match case {
            0 => wrong[0].id.push('x'),
            1 => wrong[0].agent.id.push('x'),
            2 => wrong[0]
                .tool_approval
                .as_mut()
                .unwrap()
                .content_hash
                .push('x'),
            3 => wrong[0]
                .tool_approval
                .as_mut()
                .unwrap()
                .image
                .as_mut()
                .unwrap()
                .source_id
                .push('x'),
            _ => unreachable!(),
        }
        assert!(canonical_request(&pending, "agent", &wrong).is_none());
    }
}
#[test]
fn matching_canonical_terminal_releases_capacity_without_claiming_delivery_or_erasing_finance() {
    let journal = Journal::new();
    for n in 0..JOURNAL_KEEP {
        retained_or_schedule(&journal.0, "space", &format!("source-{n}"), &call("one")).unwrap();
    }
    for (status, expiry, expected) in [
        (client_core::RequestStatus::Denied, 500, Terminal::Denied),
        (
            client_core::RequestStatus::Cancelled,
            500,
            Terminal::Cancelled,
        ),
        (client_core::RequestStatus::Stale, 500, Terminal::Stale),
        (client_core::RequestStatus::Approved, 99, Terminal::Expired),
    ] {
        let canonical = request(status, expiry);
        let terminal = canonical_terminal(&canonical, Some(100)).unwrap();
        assert_eq!(terminal, expected);
        let mut pending = reviewed();
        pending.progress = Progress::Terminal {
            request: "original-request".into(),
            reason: terminal,
        };
        let step = ToolStep {
            call: call("one"),
            result: serde_json::to_string(&pending).unwrap(),
            is_error: false,
        };
        assert!(!unresolved(&step));
        assert!(!matches!(pending.progress, Progress::Delivered { .. }));
        assert!(pending.advance_command("agent").is_none());
    }
    assert!(canonical_terminal(
        &request(client_core::RequestStatus::Pending, 500),
        Some(100)
    )
    .is_none());
    assert!(canonical_terminal(
        &request(client_core::RequestStatus::Approved, 500),
        Some(100)
    )
    .is_none());
    JournalState::update(&journal.0, |state| {
        let old = &mut state.steps[0].1[0];
        let mut pending: Pending = serde_json::from_str(&old.result).unwrap();
        pending.progress = Progress::Terminal {
            request: "original-request".into(),
            reason: Terminal::Denied,
        };
        old.result = serde_json::to_string(&pending).unwrap();
        Ok(true)
    })
    .unwrap();
    retained_or_schedule(&journal.0, "space", "new-source", &call("one")).unwrap();
    assert_eq!(
        unresolved_count(&JournalState::load(&journal.0).unwrap().steps),
        JOURNAL_KEEP
    );
}

#[test]
fn a_second_prompt_for_the_same_source_cannot_create_orphan_scheduling() {
    let journal = Journal::new();
    retained_or_schedule(&journal.0, "space", "source", &call("original")).unwrap();
    let before = std::fs::read(&journal.0).unwrap();
    assert!(retained_or_schedule(&journal.0, "space", "source", &call("different")).is_err());
    assert_eq!(std::fs::read(&journal.0).unwrap(), before);
    retained_or_schedule(&journal.0, "space", "new-source", &call("different")).unwrap();
}

#[test]
fn unrepresentable_content_cannot_create_or_change_pending_scheduling() {
    let journal = Journal::new();
    for prompt in [
        String::new(),
        " ".repeat(8),
        "x".repeat(8193),
        "\0".repeat(8192),
    ] {
        assert!(schedule_command(&journal.0, "agent", "space", "invalid", &call(&prompt)).is_err());
        assert!(
            !journal.0.exists(),
            "invalid content created pending scheduling"
        );
    }
    let valid = call("A calm sea");
    schedule_command(&journal.0, "agent", "space", "valid", &valid).unwrap();
    let before = std::fs::read(&journal.0).unwrap();
    assert!(schedule_command(
        &journal.0,
        "agent",
        "space",
        "escaped",
        &call(&"\0".repeat(8192))
    )
    .is_err());
    assert_eq!(std::fs::read(&journal.0).unwrap(), before);
    assert_eq!(
        unresolved_count(&JournalState::load(&journal.0).unwrap().steps),
        1
    );
}
