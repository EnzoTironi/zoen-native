//! Whole journal recovery cases on the same original parent fixture.

use super::*;

pub(super) fn closing_and_reopening_the_database_recovers_the_exact_edit_receipt_and_draft() {
    let directory = std::env::temp_dir().join(new_id("zoen-page-reopen"));
    std::fs::create_dir_all(&directory).unwrap();
    let database = directory.join("page.sqlite");
    let path = database.to_str().unwrap();
    let (engine, observed) = fixture_at(path);
    let mut edited = observed.blocks.clone();
    edited[1].text = "Draft surviving full engine destruction".into();
    let accepted = apply(&engine, &observed, "durable-edit", &edited).unwrap();
    let encrypted = journal_rows(&engine);
    drop(engine);
    let engine = Client::open(path.into(), "en".into()).unwrap();
    let recovered = engine.page(observed.item_id.clone()).unwrap();
    assert_eq!(recovered.blocks, edited);
    assert!(recovered.unsaved);
    let replay = apply(&engine, &observed, "durable-edit", &edited).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.applied_content_hash, accepted.applied_content_hash);
    assert_eq!(journal_rows(&engine), encrypted);
    drop(engine);
    std::fs::remove_dir_all(directory).unwrap();
}

pub(super) fn an_editor_context_cannot_be_reused_on_another_page() {
    let (engine, observed) = fixture();
    let other = engine
        .page_create(observed.space_id.clone(), "Another page".into())
        .unwrap();
    let page = engine.page(other.id.clone()).unwrap();
    let mut block = page.blocks[0].clone();
    block.text = "Refused cross-page edit".into();
    assert!(engine
        .page_apply_from(
            other.id.clone(),
            "cross-page".into(),
            observed.edit_context,
            page.blocks.iter().map(|b| b.id.clone()).collect(),
            vec![block]
        )
        .is_err());
    assert_eq!(engine.page(other.id).unwrap().blocks, page.blocks);
    assert!(journal_rows(&engine).is_empty());
}

pub(super) fn restoration_fences_old_drafts_even_when_native_storage_cleanup_fails() {
    let (engine, original) = fixture();
    let mut changed = original.blocks.clone();
    changed[1].text = "Saved second version".into();
    let accepted = apply(&engine, &original, "second-version", &changed).unwrap();
    assert!(engine
        .page_commit(original.item_id.clone(), "Second".into())
        .unwrap());
    let old_editor = engine.page(original.item_id.clone()).unwrap();
    engine.restore_version(original.item_id.clone(), 1).unwrap();
    let restored = engine.page(original.item_id.clone()).unwrap();
    assert_eq!(restored.blocks, original.blocks);
    let encrypted = journal_rows(&engine);
    let mut stale_draft = old_editor.blocks.clone();
    stale_draft[1].text = "Unaccepted text left in the native vault".into();
    assert!(matches!(
        apply(&engine, &old_editor, "old-unaccepted", &stale_draft),
        Err(CoreError::Stale { .. })
    ));
    let replay = apply(&engine, &original, "second-version", &changed).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.applied_content_hash, accepted.applied_content_hash);
    assert_eq!(replay.page.blocks, original.blocks);
    let mut continued = old_editor.clone();
    continued.edit_context = replay.applied_edit_context;
    assert!(matches!(
        apply(&engine, &continued, "old-continuation", &stale_draft),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(
        engine.page(original.item_id.clone()).unwrap().blocks,
        original.blocks
    );
    assert_eq!(journal_rows(&engine), encrypted);
    engine.lock().reload().unwrap();
    assert!(matches!(
        apply(&engine, &old_editor, "old-unaccepted", &stale_draft),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(
        engine.page(original.item_id.clone()).unwrap().blocks,
        original.blocks
    );
}

pub(super) fn journal_failure_does_not_publish_an_edit_and_retry_is_safe() {
    let (engine, observed) = fixture();
    let mut edited = observed.blocks.clone();
    edited[1].text = "Private draft text".into();
    engine.lock().store.conn().execute_batch("CREATE TEMP TRIGGER refuse_journal BEFORE INSERT ON meta WHEN NEW.key LIKE 'page.draft.%' BEGIN SELECT RAISE(ABORT, 'injected journal failure'); END;").unwrap();
    assert!(apply(&engine, &observed, "draft", &edited).is_err());
    assert_eq!(
        engine.page(observed.item_id.clone()).unwrap().blocks,
        observed.blocks
    );
    engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER refuse_journal")
        .unwrap();
    apply(&engine, &observed, "draft", &edited).unwrap();
    assert!(journal_rows(&engine)
        .iter()
        .all(|s| s.starts_with("v1:") && !s.contains("Private draft text")));
    engine.lock().reload().unwrap();
    assert!(
        apply(&engine, &observed, "draft", &edited)
            .unwrap()
            .replayed
    );
}

pub(super) fn forgotten_retry_fails_before_mutation_but_a_recent_retry_survives_reload() {
    let (engine, oldest) = fixture();
    let mut last = None;
    for index in 0..=512 {
        let observed = engine.page(oldest.item_id.clone()).unwrap();
        let mut blocks = observed.blocks.clone();
        blocks[1].text = format!("Edit {index}");
        let mutation = format!("edit-{index}");
        apply(&engine, &observed, &mutation, &blocks).unwrap();
        last = Some((observed, mutation, blocks));
    }
    let before = engine.page(oldest.item_id.clone()).unwrap();
    let rows = journal_rows(&engine);
    let mut original_edit = oldest.blocks.clone();
    original_edit[1].text = "Edit 0".into();
    assert!(matches!(
        apply(&engine, &oldest, "edit-0", &original_edit),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(journal_rows(&engine), rows);
    assert_eq!(
        engine.page(oldest.item_id.clone()).unwrap().blocks,
        before.blocks
    );
    engine.lock().reload().unwrap();
    let (observed, mutation, blocks) = last.unwrap();
    assert!(
        apply(&engine, &observed, &mutation, &blocks)
            .unwrap()
            .replayed
    );
}

pub(super) fn corrupt_journal_preserves_saved_read_and_refuses_overwriting_the_draft() {
    let (engine, observed) = fixture();
    let mut blocks = observed.blocks.clone();
    blocks[1].text = "Encrypted local draft".into();
    apply(&engine, &observed, "draft", &blocks).unwrap();
    engine
        .lock()
        .store
        .conn()
        .execute(
            "UPDATE meta SET value = 'locked ciphertext' WHERE key LIKE 'page.draft.%'",
            [],
        )
        .unwrap();
    engine.lock().reload().unwrap();
    let page = engine.page(observed.item_id.clone()).unwrap();
    assert_eq!(page.blocks, observed.blocks);
    assert!(page.ready);
    assert!(!page.can_edit);
    assert!(page.save_error.is_some());
    assert!(apply(&engine, &observed, "retry", &blocks).is_err());
    assert_eq!(journal_rows(&engine), vec!["locked ciphertext"]);
}
