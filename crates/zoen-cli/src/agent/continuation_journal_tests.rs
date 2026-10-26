use super::*;
use sha2::{Digest, Sha256};

struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(types::new_id("continuation-journal")?);
        std::fs::create_dir(&path).unwrap();
        Ok(Self(path))
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
fn tools<'a>(fixture: &'a crate::deckcraft::tests::Fixture, journal: &'a Path) -> SpaceTools<'a> {
    SpaceTools {
        selection: None,
        e: &fixture.client,
        journal,
        source: &fixture.source,
        space: fixture
            .client
            .item(fixture.base.item_id.clone())
            .unwrap()
            .space_id,
        serving: &fixture.owner,
        names: vec![],
        deckcraft: None,
        deckcraft_items: vec![],
        page_items: vec![],
        scene_items: vec![],
    }
}

#[test]
fn legacy_steps_remain_readable_and_writes_stay_legacy_until_a_resume_is_reserved(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let directory = Directory::new()?;
    let path = directory.journal();
    let fixture = crate::deckcraft::tests::fixture_at(directory.0.clone(), [72; 32])?;
    let host = tools(&fixture, &path);
    let original = vec![(fixture.source.clone(), vec![step("first")])];
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
    Ok(())
}

#[test]
fn exact_request_budget_survives_reopens_and_step_recording_without_reset(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let directory = Directory::new()?;
    let path = directory.journal();
    let hash = "ab".repeat(32);
    let mut fixture = crate::deckcraft::tests::fixture_at(directory.0.clone(), [72; 32])?;
    for expected in 1..=MAX_TRIES {
        // Reopen both the signed native account/source and journal. No counter
        // or authority is supplied by an anonymous in-memory Client.
        drop(fixture.client);
        fixture.client =
            canonical_tool_approval_tests::reopen(&fixture.path, fixture.vault.clone());
        let host = tools(&fixture, &path);
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
    Ok(())
}

#[test]
fn corrupt_or_unsupported_journal_cannot_reset_the_budget_or_be_overwritten_by_recording(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for corrupt in [
        b"not-json".as_slice(),
        br#"{"version":2,"steps":[],"continuations":{}}"#,
        br#"{"version":1,"steps":[],"continuations":{"synthetic-request":{"content_hash":"short","attempts":1}}}"#,
        br#"{"version":1,"steps":[],"continuations":{},"unknown":true}"#,
        br#"{"version":1,"steps":[],"continuations":{"synthetic-request":{"content_hash":"abababababababababababababababababababababababababababababababab","attempts":0}}}"#,
    ] {
        let directory = Directory::new()?;
        let path = directory.journal();
        std::fs::write(&path, corrupt).unwrap();
        assert!(JournalState::begin_resume(&path, "synthetic-request", &"ab".repeat(32), None).is_err());
        let fixture = crate::deckcraft::tests::fixture_at(directory.0.clone(), [72; 32])?;
        tools(&fixture, &path).record(step("must-not-replace-corrupt-data"));
        assert_eq!(std::fs::read(&path).unwrap(), corrupt);
        assert!(!path.with_extension("tmp").exists());
    }
    Ok(())
}

#[test]
fn failed_atomic_reservation_and_leftover_temporary_file_never_admit_a_resume(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let directory = Directory::new()?;
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
    Ok(())
}

#[test]
fn prepared_and_completed_metadata_survive_reopen_bound_to_the_same_source_and_request_hash(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let directory = Directory::new()?;
    let path = directory.journal();
    let hash = "ab".repeat(32);
    let mut fixture = crate::deckcraft::tests::fixture_at(directory.0.clone(), [72; 32])?;
    let account = fixture.client.account().unwrap();
    let space = fixture
        .client
        .item(fixture.base.item_id.clone())
        .unwrap()
        .space_id;
    let source = fixture
        .client
        .timeline(space.clone())
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == fixture.source)
        .unwrap();
    let EntryKind::Message { text, .. } = source.kind else {
        panic!("signed fixture source");
    };
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
            agent: account.identity_id,
            device: account.device_id,
            owner: fixture.owner.clone(),
            space,
            from: fixture.owner.clone(),
            inbound_sha256: hex::encode(Sha256::digest(text.as_bytes())),
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
    assert!(
        JournalState::begin_resume(&path, "synthetic-request", &hash, Some(&fixture.source))
            .unwrap()
    );
    JournalState::update(&path, |state| {
        let budget = state.continuations.get_mut("synthetic-request").unwrap();
        budget.progress = Some(progress.clone());
        budget.prepared = Some(prepared.clone());
        state.steps = vec![(fixture.source.clone(), vec![completed.clone()])];
        Ok(true)
    })
    .unwrap();
    drop(fixture.client);
    fixture.client = canonical_tool_approval_tests::reopen(&fixture.path, fixture.vault.clone());
    let host = tools(&fixture, &path);
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
        ..tools(&fixture, &path)
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
    Ok(())
}
