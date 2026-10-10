use super::*;
use crate::RodaEngine;
use std::sync::Arc;

fn fixture() -> (Arc<RodaEngine>, PageDto) {
    fixture_at(":memory:")
}

fn fixture_at(path: &str) -> (Arc<RodaEngine>, PageDto) {
    let engine = RodaEngine::open(path.into(), "en".into()).unwrap();
    engine.seed_demo_if_empty().unwrap();
    let space = engine
        .spaces()
        .into_iter()
        .find(|s| s.title == "Paraty with Marina")
        .unwrap();
    let id = engine
        .page_import_markdown(
            space.id,
            "review.md".into(),
            "# Title\n\nAlpha\n\nBeta\n\nGamma\n".into(),
        )
        .unwrap();
    let page = engine.page(id.id).unwrap();
    (engine, page)
}

#[test]
fn closing_and_reopening_the_database_recovers_the_exact_edit_receipt_and_draft() {
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
    let engine = RodaEngine::open(path.into(), "en".into()).unwrap();
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

#[test]
fn an_editor_context_cannot_be_reused_on_another_page() {
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

#[test]
fn restoration_fences_old_drafts_even_when_native_storage_cleanup_fails() {
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

fn apply(
    engine: &RodaEngine,
    observed: &PageDto,
    mutation: &str,
    blocks: &[PageBlockDto],
) -> R<PageEditResult> {
    let changed = blocks
        .iter()
        .filter(|b| !observed.blocks.contains(b))
        .cloned()
        .collect();
    engine.page_apply_from(
        observed.item_id.clone(),
        mutation.into(),
        observed.edit_context.clone(),
        blocks.iter().map(|b| b.id.clone()).collect(),
        changed,
    )
}

fn journal_rows(engine: &RodaEngine) -> Vec<String> {
    let e = engine.lock();
    let mut q = e
        .store
        .conn()
        .prepare("SELECT value FROM meta WHERE key LIKE 'page.draft.%' ORDER BY key")
        .unwrap();
    q.query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn two_editors_preserve_unseen_text_and_insertions_while_deleting_and_moving_blocks() {
    let (engine, first) = fixture();
    let second = engine.page(first.item_id.clone()).unwrap();
    assert_ne!(first.edit_context, second.edit_context);
    let mut foreign = first.blocks.clone();
    foreign[3].text = "Gamma from the other editor".into();
    let mut inserted = foreign[2].clone();
    inserted.id = "foreign-block".into();
    inserted.text = "New remote block".into();
    foreign.insert(2, inserted);
    apply(&engine, &first, "first-edit", &foreign).unwrap();
    let mut local = second.blocks.clone();
    local.remove(2); // Delete Beta, which this editor observed.
    let moved = local.pop().unwrap();
    local.insert(0, moved);
    local.iter_mut().find(|b| b.text == "Alpha").unwrap().text = "Alpha from this editor".into();
    let result = apply(&engine, &second, "second-edit", &local).unwrap();
    assert!(result
        .page
        .blocks
        .iter()
        .any(|b| b.text == "New remote block"));
    assert!(result
        .page
        .blocks
        .iter()
        .any(|b| b.text == "Gamma from the other editor"));
    assert!(result
        .page
        .blocks
        .iter()
        .any(|b| b.text == "Alpha from this editor"));
    assert!(!result
        .page
        .blocks
        .iter()
        .any(|b| b.id == second.blocks[2].id));
    assert_eq!(result.page.blocks[0].id, second.blocks[3].id);
    assert!(result.page.unsaved);
}

#[test]
fn lost_reply_continuation_can_undo_and_redo_without_duplicate_text() {
    let (engine, observed) = fixture();
    let mut edited = observed.blocks.clone();
    edited[1].text.push_str(" typed once");
    let first = apply(&engine, &observed, "typing", &edited).unwrap();
    engine.lock().reload().unwrap();
    let undone = apply(&engine, &observed, "undo", &observed.blocks).unwrap();
    assert_eq!(undone.page.blocks, observed.blocks);
    assert!(!undone.page.unsaved);
    let redone = apply(&engine, &observed, "redo", &edited).unwrap();
    assert_eq!(redone.page.blocks, edited);
    assert_eq!(redone.applied_content_hash, first.applied_content_hash);
    let retry = apply(&engine, &observed, "typing", &edited).unwrap();
    assert!(retry.replayed);
    assert_eq!(retry.applied_content_hash, first.applied_content_hash);
    assert_eq!(retry.page.blocks, edited);
    assert_eq!(retry.mutation_id, "typing");
    let mut different = edited.clone();
    different[1].text.push('!');
    assert!(matches!(
        apply(&engine, &observed, "typing", &different),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(
        engine.page(observed.item_id.clone()).unwrap().blocks,
        edited
    );
}

#[test]
fn lost_reply_continuation_can_remove_its_own_new_block() {
    let (engine, observed) = fixture();
    let mut edited = observed.blocks.clone();
    let mut new = edited[1].clone();
    new.id = "temporary-block".into();
    new.text = "A block the user then deletes".into();
    edited.push(new);
    apply(&engine, &observed, "insert", &edited).unwrap();
    engine.lock().reload().unwrap();
    let removed = apply(&engine, &observed, "delete", &observed.blocks).unwrap();
    assert_eq!(removed.page.blocks, observed.blocks);
}

#[test]
fn conflicting_external_ids_are_refused_before_persisting_merged_content() {
    let (engine, first) = fixture();
    let second = engine.page(first.item_id.clone()).unwrap();
    let mut a = first.blocks.clone();
    let mut block = a[1].clone();
    block.id = "same-external-id".into();
    block.text = "First".into();
    a.push(block.clone());
    apply(&engine, &first, "first-insert", &a).unwrap();
    let rows = journal_rows(&engine);
    let mut b = second.blocks.clone();
    block.text = "Second".into();
    b.push(block);
    assert!(apply(&engine, &second, "second-insert", &b).is_err());
    assert_eq!(journal_rows(&engine), rows);
    let page = engine.page(first.item_id.clone()).unwrap();
    assert!(page.can_edit);
    assert_eq!(page.blocks, a);
}

#[test]
fn invalid_formatting_is_atomic_even_with_a_delete_and_reorder() {
    let (engine, observed) = fixture();
    let mut edited = observed.blocks.clone();
    edited.remove(2);
    edited.reverse();
    edited[0].spans.push(TextSpanDto {
        start: 1,
        end: 999,
        key: "b".into(),
        value: String::new(),
    });
    assert!(apply(&engine, &observed, "invalid", &edited).is_err());
    assert!(journal_rows(&engine).is_empty());
    assert_eq!(
        engine.page(observed.item_id.clone()).unwrap().blocks,
        observed.blocks
    );
}

#[test]
fn journal_failure_does_not_publish_an_edit_and_retry_is_safe() {
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

#[test]
fn event_append_failure_preserves_draft_across_reload_and_retry() {
    let (engine, observed) = fixture();
    let mut edited = observed.blocks.clone();
    edited[1].text = "Draft before interrupted save".into();
    apply(&engine, &observed, "draft", &edited).unwrap();
    engine.lock().store.conn().execute_batch("CREATE TEMP TRIGGER refuse_commit BEFORE INSERT ON events BEGIN SELECT RAISE(ABORT, 'injected event failure'); END;").unwrap();
    assert!(engine
        .page_commit(observed.item_id.clone(), "Save".into())
        .is_err());
    let restored = engine.page(observed.item_id.clone()).unwrap();
    assert_eq!(restored.blocks, edited);
    assert!(restored.unsaved);
    engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER refuse_commit")
        .unwrap();
    assert!(engine
        .page_commit(observed.item_id.clone(), "Retry".into())
        .unwrap());
    assert!(!engine.page(observed.item_id.clone()).unwrap().unsaved);
    engine.lock().reload().unwrap();
    assert!(
        apply(&engine, &observed, "draft", &edited)
            .unwrap()
            .replayed
    );
}

#[test]
fn readers_and_removed_members_cannot_change_cached_pages_or_journals() {
    let (engine, observed) = fixture();
    let me = engine.me().unwrap().id;
    let mut edited = observed.blocks.clone();
    edited[1].text = "Refused write".into();
    for role in [Some(Role::Reader), None] {
        {
            let mut e = engine.lock();
            let members = &mut e.state.spaces.get_mut(&observed.space_id).unwrap().members;
            members.retain(|(id, _)| id != &me);
            if let Some(role) = role {
                members.push((me.clone(), role));
            }
        }
        assert!(matches!(
            apply(&engine, &observed, "reader", &edited),
            Err(CoreError::Forbidden { .. })
        ));
        assert!(matches!(
            engine.page_commit(observed.item_id.clone(), "Save".into()),
            Err(CoreError::Forbidden { .. })
        ));
        let page = engine.page(observed.item_id.clone()).unwrap();
        assert!(!page.can_edit);
        assert!(page.edit_context.is_empty());
        assert_eq!(page.blocks, observed.blocks);
        assert!(journal_rows(&engine).is_empty());
    }
}

fn confirm_pending(engine: &RodaEngine, item: &str) -> String {
    let mut e = engine.lock();
    let pending = e
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .find(|p| {
            !p.failed
                && matches!(&p.event.body, EventBody::ItemVersioned { item: id, .. } if id == item)
        })
        .unwrap();
    let space = pending.event.space.clone();
    let head = e.logs[&space].head().unwrap();
    let seq = head.seq + 1;
    let env = roda_proto::Envelope::plain(&pending.event);
    let hash = roda_log::chain_hash(&space, seq, &head.hash, &env.wire_hash());
    let result = e.ingest(roda_proto::Sequenced {
        seq,
        prev: head.hash,
        hash,
        env,
    });
    assert_eq!(result, crate::sync::Ingest::Confirmed);
    pending.event.client_id
}

#[test]
fn newer_draft_waits_for_ack_and_survives_a_permanent_refusal() {
    let (engine, observed) = fixture();
    {
        let mut e = engine.lock();
        e.store.set_synced(&observed.space_id).unwrap();
        e.net.synced.insert(observed.space_id.clone());
    }
    let mut first = observed.blocks.clone();
    first[1].text = "First queued save".into();
    apply(&engine, &observed, "first", &first).unwrap();
    assert!(engine
        .page_commit(observed.item_id.clone(), "First".into())
        .unwrap());
    let fresh = engine.page(observed.item_id.clone()).unwrap();
    assert!(fresh.pending_sync);
    let mut second = fresh.blocks.clone();
    second[1].text = "Newer draft while offline".into();
    apply(&engine, &fresh, "second", &second).unwrap();
    assert!(!engine
        .page_commit(observed.item_id.clone(), "Deferred".into())
        .unwrap());
    let id = engine.lock().store.outbox().unwrap().into_iter().find(|p| matches!(&p.event.body, EventBody::ItemVersioned { item, .. } if item == &observed.item_id)).unwrap().event.client_id;
    engine
        .lock()
        .reject(&id, "permanent permission refusal", true, None);
    let refused = engine.page(observed.item_id.clone()).unwrap();
    assert_eq!(refused.blocks, second);
    assert!(refused.unsaved);
    assert!(!refused.pending_sync);
    assert!(refused.save_error.is_some());
    assert!(engine
        .page_commit(observed.item_id.clone(), "Retry whole draft".into())
        .unwrap());
    assert_ne!(confirm_pending(&engine, &observed.item_id), id);
    let confirmed = engine.page(observed.item_id.clone()).unwrap();
    assert_eq!(confirmed.blocks, second);
    assert!(!confirmed.unsaved);
    assert!(!confirmed.pending_sync);
    assert_eq!(confirmed.save_error, None);
}

#[test]
fn forgotten_retry_fails_before_mutation_but_a_recent_retry_survives_reload() {
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

#[test]
fn corrupt_journal_preserves_saved_read_and_refuses_overwriting_the_draft() {
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
