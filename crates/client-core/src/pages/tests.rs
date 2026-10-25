use super::*;
use crate::Client;
use std::sync::Arc;

fn fixture() -> (Arc<Client>, PageDto) {
    fixture_at(":memory:")
}

fn fixture_at(path: &str) -> (Arc<Client>, PageDto) {
    let engine = Client::open(path.into(), "en".into()).unwrap();
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
    journal_recovery::closing_and_reopening_the_database_recovers_the_exact_edit_receipt_and_draft()
        .unwrap()
}

#[test]
fn an_editor_context_cannot_be_reused_on_another_page() {
    journal_recovery::an_editor_context_cannot_be_reused_on_another_page()
}

#[test]
fn restoration_fences_old_drafts_even_when_native_storage_cleanup_fails() {
    journal_recovery::restoration_fences_old_drafts_even_when_native_storage_cleanup_fails()
}

fn apply(
    engine: &Client,
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

fn journal_rows(engine: &Client) -> Vec<String> {
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
    collaborative_edits::two_editors_preserve_unseen_text_and_insertions_while_deleting_and_moving_blocks()
}

#[test]
fn lost_reply_continuation_can_undo_and_redo_without_duplicate_text() {
    collaborative_edits::lost_reply_continuation_can_undo_and_redo_without_duplicate_text()
}

#[test]
fn lost_reply_continuation_can_remove_its_own_new_block() {
    collaborative_edits::lost_reply_continuation_can_remove_its_own_new_block()
}

#[test]
fn conflicting_external_ids_are_refused_before_persisting_merged_content() {
    collaborative_edits::conflicting_external_ids_are_refused_before_persisting_merged_content()
}

#[test]
fn invalid_formatting_is_atomic_even_with_a_delete_and_reorder() {
    collaborative_edits::invalid_formatting_is_atomic_even_with_a_delete_and_reorder()
}

#[test]
fn journal_failure_does_not_publish_an_edit_and_retry_is_safe() {
    journal_recovery::journal_failure_does_not_publish_an_edit_and_retry_is_safe()
}

#[test]
fn event_append_failure_preserves_draft_across_reload_and_retry() {
    publication::event_append_failure_preserves_draft_across_reload_and_retry()
}

#[test]
fn readers_and_removed_members_cannot_change_cached_pages_or_journals() {
    admission::readers_and_removed_members_cannot_change_cached_pages_or_journals()
}

fn confirm_pending(
    engine: &Client,
    item: &str,
) -> std::result::Result<String, Box<dyn std::error::Error>> {
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
    let env = protocol::Envelope::plain(&pending.event)?;
    let hash = event_log::chain_hash(&space, seq, &head.hash, &env.wire_hash())?;
    let result = e.ingest(protocol::Sequenced {
        seq,
        prev: head.hash,
        hash,
        env,
    });
    assert_eq!(result, crate::sync::Ingest::Confirmed);
    Ok(pending.event.client_id)
}

#[test]
fn newer_draft_waits_for_ack_and_survives_a_permanent_refusal(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    publication::newer_draft_waits_for_ack_and_survives_a_permanent_refusal()
}

#[test]
fn forgotten_retry_fails_before_mutation_but_a_recent_retry_survives_reload() {
    journal_recovery::forgotten_retry_fails_before_mutation_but_a_recent_retry_survives_reload()
}

#[test]
fn corrupt_journal_preserves_saved_read_and_refuses_overwriting_the_draft() {
    journal_recovery::corrupt_journal_preserves_saved_read_and_refuses_overwriting_the_draft()
}

#[test]
fn public_page_reads_reuse_verified_media_and_require_it_again_after_reload() {
    publication::public_page_reads_reuse_verified_media_and_require_it_again_after_reload().unwrap()
}

#[test]
fn publication_journal_failure_rolls_back_the_actual_event_and_retains_the_draft() {
    publication::publication_journal_failure_rolls_back_the_actual_event_and_retains_the_draft()
}

#[test]
fn shared_session_does_not_activate_public_plan_page_writers() {
    admission::shared_session_does_not_activate_public_plan_page_writers()
}

mod admission;
mod collaborative_edits;
mod journal_recovery;
mod lossless_duplication;
mod publication;

mod version_exports;
