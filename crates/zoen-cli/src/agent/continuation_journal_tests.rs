use super::*;

struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(types::new_id("continuation-journal"));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn journal(&self) -> std::path::PathBuf {
        self.0.join("agent-tool-steps.json")
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn step(id: &str) -> ToolStep {
    ToolStep {
        call: ToolCall {
            requires_review: false,
            id: id.into(),
            name: "synthetic-read".into(),
            arguments: json!({"artifactId":"synthetic-artifact"}),
        },
        result: "synthetic retained result".into(),
        is_error: false,
    }
}
fn tools<'a>(client: &'a Arc<Client>, journal: &'a Path) -> SpaceTools<'a> {
    SpaceTools {
        selection: None,
        e: client,
        journal,
        source: "synthetic-source",
        space: "synthetic-space".into(),
        serving: "synthetic-owner",
        names: vec![],
        deckcraft: None,
        deckcraft_items: vec![],
        page_items: vec![],
    }
}

#[test]
fn legacy_steps_remain_readable_and_writes_stay_legacy_until_a_resume_is_reserved() {
    let directory = Directory::new();
    let path = directory.journal();
    let client = Client::open(":memory:".into(), "en".into()).unwrap();
    let host = tools(&client, &path);
    let original = vec![("synthetic-source".to_string(), vec![step("first")])];
    std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
    assert_eq!(host.load(), original);
    assert_eq!(host.done(), vec![step("first")]);
    host.record(step("second"));
    let legacy: Vec<(String, Vec<ToolStep>)> =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(legacy[0].1, vec![step("first"), step("second")]);
    assert!(host
        .begin_resume("synthetic-request", &"ab".repeat(32))
        .unwrap());
    let versioned: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(versioned["version"], 1);
    assert_eq!(
        versioned["continuations"]["synthetic-request"]["attempts"],
        1
    );
    assert_eq!(host.done(), vec![step("first"), step("second")]);
}

#[test]
fn exact_request_budget_survives_reopens_and_step_recording_without_reset() {
    let directory = Directory::new();
    let path = directory.journal();
    let hash = "ab".repeat(32);
    for expected in 1..=MAX_TRIES {
        // Every operation loads from disk. Dropping these clients/hosts simulates the
        // operational state of a fresh process; no counter is held by SpaceTools.
        let client = Client::open(":memory:".into(), "en".into()).unwrap();
        let host = tools(&client, &path);
        assert!(host.begin_resume("synthetic-request", &hash).unwrap());
        host.record(step(&format!("step-{expected}")));
        let recovered = JournalState::load(&path).unwrap();
        assert_eq!(
            recovered.continuations["synthetic-request"].attempts,
            expected
        );
        assert_eq!(
            recovered.continuations["synthetic-request"].content_hash,
            hash
        );
        assert_eq!(recovered.steps[0].1.len(), expected as usize);
    }
    let before = std::fs::read(&path).unwrap();
    assert!(!JournalState::begin_resume(&path, "synthetic-request", &hash, None).unwrap());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(
        JournalState::begin_resume(&path, "synthetic-request", &"cd".repeat(32), None).is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(JournalState::begin_resume(&path, "different-reviewed-request", &hash, None).unwrap());
    assert_eq!(
        JournalState::load(&path).unwrap().continuations["synthetic-request"].attempts,
        MAX_TRIES
    );
}

#[test]
fn corrupt_or_unsupported_journal_cannot_reset_the_budget_or_be_overwritten_by_recording() {
    for corrupt in [
        b"not-json".as_slice(),
        br#"{"version":2,"steps":[],"continuations":{}}"#,
        br#"{"version":1,"steps":[],"continuations":{"synthetic-request":{"content_hash":"short","attempts":1}}}"#,
        br#"{"version":1,"steps":[],"continuations":{},"unknown":true}"#,
        br#"{"version":1,"steps":[],"continuations":{"synthetic-request":{"content_hash":"abababababababababababababababababababababababababababababababab","attempts":0}}}"#,
    ] {
        let directory = Directory::new();
        let path = directory.journal();
        std::fs::write(&path, corrupt).unwrap();
        assert!(JournalState::begin_resume(&path, "synthetic-request", &"ab".repeat(32), None).is_err());
        let client = Client::open(":memory:".into(), "en".into()).unwrap();
        tools(&client, &path).record(step("must-not-replace-corrupt-data"));
        assert_eq!(std::fs::read(&path).unwrap(), corrupt);
        assert!(!path.with_extension("tmp").exists());
    }
}

#[test]
fn failed_atomic_reservation_and_leftover_temporary_file_never_admit_a_resume() {
    let directory = Directory::new();
    let path = directory.journal();
    let hash = "ab".repeat(32);
    assert!(JournalState::begin_resume(&path, "synthetic-request", &hash, None).unwrap());
    let before = std::fs::read(&path).unwrap();
    // An interrupted writer or a competing reservation owns the temporary path.
    std::fs::write(path.with_extension("tmp"), b"interrupted-reservation").unwrap();
    assert!(JournalState::begin_resume(&path, "synthetic-request", &hash, None).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(
        std::fs::read(path.with_extension("tmp")).unwrap(),
        b"interrupted-reservation"
    );
    std::fs::remove_file(path.with_extension("tmp")).unwrap();
    // A destination that cannot be replaced prevents commit, so no attempt is admitted.
    let destination = directory.0.join("unwritable-target.json");
    std::fs::create_dir(&destination).unwrap();
    assert!(JournalState::begin_resume(&destination, "synthetic-request", &hash, None).is_err());
    assert!(destination.is_dir());
    assert!(!destination.with_extension("tmp").exists());
    assert_eq!(
        JournalState::load(&path).unwrap().continuations["synthetic-request"].attempts,
        1
    );
}

#[test]
fn prepared_and_completed_metadata_survive_reopen_bound_to_the_same_source_and_request_hash() {
    let directory = Directory::new();
    let path = directory.journal();
    let hash = "ab".repeat(32);
    let completed = step("completed-read");
    let progress = ToolProgress {
        round: 0,
        steps: vec![completed.clone()],
        remaining: vec![],
    };
    let prepared = ToolAsk {
        version: 1,
        policy_sha256: "cd".repeat(32),
        envelope: zoen_agentd::ToolAskEnvelope {
            agent: "synthetic-agent".into(),
            device: "synthetic-device".into(),
            owner: "synthetic-owner".into(),
            space: "synthetic-space".into(),
            from: "synthetic-owner".into(),
            inbound_sha256: "ef".repeat(32),
        },
        call: step("prepared-write").call,
        definition: ToolDef {
            name: "synthetic-read".into(),
            description: "Synthetic tool".into(),
            parameters: json!({"type":"object"}),
        },
        steps: progress.steps.clone(),
        remaining: vec![],
        round: 1,
        rule: "model:low-risk".into(),
    };
    assert!(JournalState::begin_resume(
        &path,
        "synthetic-request",
        &hash,
        Some("synthetic-source")
    )
    .unwrap());
    JournalState::update(&path, |state| {
        let budget = state.continuations.get_mut("synthetic-request").unwrap();
        budget.progress = Some(progress.clone());
        budget.prepared = Some(prepared.clone());
        state.steps = vec![("synthetic-source".into(), vec![completed.clone()])];
        Ok(true)
    })
    .unwrap();
    let client = Client::open(":memory:".into(), "en".into()).unwrap();
    let host = tools(&client, &path);
    assert_eq!(
        host.recovery_checkpoint("synthetic-request", &hash)
            .unwrap(),
        Some((progress.clone(), Some(prepared.clone())))
    );
    assert!(host
        .recovery_checkpoint("synthetic-request", &"00".repeat(32))
        .is_err());
    let mismatched = SpaceTools {
        source: "different-source",
        ..tools(&client, &path)
    };
    assert!(mismatched
        .recovery_checkpoint("synthetic-request", &hash)
        .is_err());
    host.record(step("separate-cached-step"));
    let recovered = JournalState::load(&path).unwrap();
    assert_eq!(recovered.continuations["synthetic-request"].attempts, 1);
    assert_eq!(
        recovered.continuations["synthetic-request"].progress,
        Some(progress)
    );
    assert_eq!(
        recovered.continuations["synthetic-request"].prepared,
        Some(prepared)
    );
    assert_eq!(host.done(), vec![completed, step("separate-cached-step")]);
}
