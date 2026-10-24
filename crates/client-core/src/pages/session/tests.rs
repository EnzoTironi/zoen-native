use super::*;
use crate::sync::Ingest;
use crate::Client;
use documents::plan::{PlanBlockContent, MAX_PLAN_PACKET_BYTES};
use documents::{Kind, Span};
use event_log::chain_hash;
use protocol::{Envelope, Sequenced};

const RAW: &str = "raw 😀\r\n\0e\u{301}\r\u{85}\u{2029}tail";

struct Database(std::path::PathBuf);
impl Database {
    fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(new_id("zoen-plan-session")?);
        std::fs::create_dir_all(&path).unwrap();
        Ok(Self(path))
    }
    fn path(&self) -> String {
        self.0.join("session.sqlite").to_str().unwrap().into()
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

struct Fixture {
    engine: Engine,
    space: String,
    item: String,
    owner: String,
}

#[test]
fn legacy_binary_bytes_context_and_journal_remain_unchanged(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let public = Client::open(":memory:".into(), "en".into()).unwrap();
    public.seed_demo_if_empty().unwrap();
    let space = public
        .spaces()
        .into_iter()
        .find(|s| s.title == "Paraty with Marina")
        .unwrap()
        .id;
    let id = public
        .page_import_markdown(
            space,
            "legacy.md".into(),
            "# Title\n\nLegacy binary text\n".into(),
        )
        .unwrap()
        .id;
    let read = public.page(id.clone()).unwrap();
    let mut e = public.lock();
    let it = e.item_state(&id).unwrap();
    let ItemContent::Page(content) = &it.versions[0].content else {
        panic!("Page")
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&content.loro)
        .unwrap();
    assert!(content.manifest.is_none());
    let literal = serde_json::to_vec(content).unwrap();
    assert!(!String::from_utf8(literal.clone())
        .unwrap()
        .contains("manifest"));
    assert_eq!(
        serde_json::from_slice::<PageContent>(&literal).unwrap(),
        *content
    );
    let decoded = e.decode_page_edit_context(it, &read.edit_context).unwrap();
    assert!(read.edit_context.starts_with("v1:"));
    let raw_context = serde_json::to_value(drafts::EditContext {
        generation: decoded.generation,
        frontiers: decoded.frontiers.clone(),
    })
    .unwrap();
    assert_eq!(raw_context.as_object().unwrap().len(), 2);
    let page = Page::new();
    page.import(&bytes).unwrap();
    assert_eq!(page.frontiers().encode(), decoded.frontiers);
    // A real encrypted old journal omits accepted_generation, which still defaults.
    let old = serde_json::json!({"generation":0,"minimum_generation":0,"snapshot":null,"has_draft":false,"commit":null,"receipts":[{"fingerprint":"old","mutation_id":"old-receipt","context_hash":"old-context","generation":0,"content_hash":"old-hash","frontiers":decoded.frontiers}]});
    let cipher = write_legacy_fixture(&e, it, &serde_json::to_vec(&old).unwrap()).unwrap();
    assert_eq!(
        e.store.meta(&row_id(&e, it).unwrap()).unwrap(),
        Some(cipher)
    );
    assert_eq!(
        e.load_page_journal(it)
            .unwrap()
            .receipt("old-receipt")
            .unwrap()
            .accepted_generation,
        0
    );
    let mut session = PageSession::open(&e, &id).unwrap();
    let binary = session.read(&e).unwrap();
    assert_eq!(binary.tag, DocumentTag::BinaryPageV1);
    assert_eq!(binary.content_hash, read.content_hash);
    let DocumentView::BinaryPage { blocks } = binary.view else {
        panic!("Binary")
    };
    let mut edit = super::super::block_dto(&blocks[1]);
    edit.text = "private binary driver".into();
    edit.spans.clear();
    let order: Vec<_> = blocks.iter().map(|b| b.id.clone()).collect();
    let accepted = session
        .apply_from(
            &e,
            "binary-driver",
            &binary.edit_context,
            DocumentEdit::BinaryPage {
                order: &order,
                changed: std::slice::from_ref(&edit),
            },
        )
        .unwrap();
    assert!(accepted.applied_edit_context.starts_with("v1:"));
    let space = e.item_state(&id).unwrap().space.clone();
    e.store.set_synced(&space).unwrap();
    e.net.synced.insert(space);
    let reference = queued(session.commit(&mut e, "binary compatibility").unwrap());
    let CommitRef::BinaryPage { client_id } = reference else {
        panic!("binary commit")
    };
    let event = e.store.outbox_get(&client_id).unwrap().unwrap();
    assert!(matches!(
        &event.body,
        EventBody::ItemVersioned {
            content: ItemContent::Page(PageContent { manifest: None, .. }),
            ..
        }
    ));
    let frame = envelope_frame(&e, Envelope::plain(&event))?;
    assert_eq!(e.ingest(frame), Ingest::Confirmed);
    assert!(!session.read(&e).unwrap().unsaved);
    edit.text = "public binary driver".into();
    drop(e);
    public.lock().reload().unwrap();
    let public_read = public.page(id.clone()).unwrap();
    let mut dto = public_read.blocks[1].clone();
    dto.text = edit.text;
    let accepted = public
        .page_apply_from(
            id.clone(),
            "legacy-public".into(),
            public_read.edit_context.clone(),
            public_read.blocks.iter().map(|b| b.id.clone()).collect(),
            vec![dto.clone()],
        )
        .unwrap();
    public.lock().reload().unwrap();
    let replay = public
        .page_apply_from(
            id.clone(),
            "legacy-public".into(),
            public_read.edit_context,
            public_read.blocks.iter().map(|b| b.id.clone()).collect(),
            vec![dto],
        )
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.applied_content_hash, accepted.applied_content_hash);
    assert!(public
        .page(id)
        .unwrap()
        .blocks
        .iter()
        .any(|b| b.text.contains("public binary driver")));
    Ok(())
}

#[test]
fn binary_offline_commit_preserves_the_existing_page_contract(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    binary_compatibility_cases::binary_offline_commit_preserves_the_existing_page_contract()
}

#[test]
fn binary_replay_keeps_the_exact_pre_upgrade_payload(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    binary_compatibility_cases::binary_replay_keeps_the_exact_pre_upgrade_payload()
}

#[test]
fn binary_refresh_reuses_verified_payload_and_rechecks_after_reload() {
    binary_compatibility_cases::binary_refresh_reuses_verified_payload_and_rechecks_after_reload()
        .unwrap()
}

#[test]
fn plan_refresh_reuses_verified_payload_and_rechecks_after_reload() {
    authority_cutover_cases::plan_refresh_reuses_verified_payload_and_rechecks_after_reload()
        .unwrap()
}

#[test]
fn binary_failed_publication_preserves_the_existing_visible_error(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    binary_compatibility_cases::binary_failed_publication_preserves_the_existing_visible_error()
}

#[test]
fn typed_plan_draft_and_exact_receipt_survive_file_reopen() {
    context_receipts_cases::typed_plan_draft_and_exact_receipt_survive_file_reopen().unwrap()
}

#[test]
fn contexts_bind_actor_space_item_kind_and_creation() {
    context_receipts_cases::contexts_bind_actor_space_item_kind_and_creation().unwrap()
}

#[test]
fn same_mutation_replays_exact_accepted_context_but_returns_current_view() {
    context_receipts_cases::same_mutation_replays_exact_accepted_context_but_returns_current_view()
        .unwrap()
}

#[test]
fn changed_request_with_same_mutation_refuses_atomically() {
    context_receipts_cases::changed_request_with_same_mutation_refuses_atomically().unwrap()
}

#[test]
fn lost_reply_empty_diff_undo_and_redo_continue_own_branch() {
    continuation_cases::lost_reply_empty_diff_undo_and_redo_continue_own_branch().unwrap()
}

#[test]
fn continuation_does_not_replace_unseen_remote_fields_or_blocks() {
    continuation_cases::continuation_does_not_replace_unseen_remote_fields_or_blocks().unwrap()
}

#[test]
fn continuation_removes_own_new_block_without_reusing_history() {
    continuation_cases::continuation_removes_own_new_block_without_reusing_history().unwrap()
}

#[test]
fn generation_fence_allows_exact_receipt_replay_but_refuses_new_continuation() {
    let f = Fixture::new().unwrap();
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    let edit = text_edit(&read, "accepted before fence");
    let accepted = apply(&f, &mut s, &read, "accepted", std::slice::from_ref(&edit)).unwrap();
    let mut fenced = journal(&s).clone();
    fenced.invalidate_contexts().unwrap();
    super::super::drafts::tagged::save(&f.engine, f.engine.item_state(&f.item).unwrap(), &fenced)
        .unwrap();
    let before = exact(&f, &s);
    let retry = apply(&f, &mut s, &read, "accepted", std::slice::from_ref(&edit)).unwrap();
    assert!(retry.replayed);
    assert_eq!(retry.applied_edit_context, accepted.applied_edit_context);
    assert!(matches!(
        apply(&f, &mut s, &read, "unaccepted", std::slice::from_ref(&edit)),
        Err(CoreError::Stale { .. })
    ));
    assert!(matches!(
        s.apply_from(
            &f.engine,
            "stale-continuation",
            &retry.applied_edit_context,
            DocumentEdit::Plan {
                order: &order_read(&read),
                changed: &[edit]
            }
        ),
        Err(CoreError::Stale { .. })
    ));
    no_change(&f, &s, &before);
}

#[test]
fn receipt_eviction_fences_forgotten_retry_across_reopen() {
    journal_recovery_cases::receipt_eviction_fences_forgotten_retry_across_reopen().unwrap()
}

#[test]
fn journal_insert_or_update_failure_keeps_previous_draft() {
    journal_recovery_cases::journal_insert_or_update_failure_keeps_previous_draft().unwrap()
}

#[test]
fn oversized_context_or_journal_fails_before_memory_replacement() {
    journal_recovery_cases::oversized_context_or_journal_fails_before_memory_replacement().unwrap()
}

#[test]
fn lost_response_after_durable_accept_recovers_the_same_receipt() {
    journal_recovery_cases::lost_response_after_durable_accept_recovers_the_same_receipt().unwrap()
}

#[test]
fn outbox_insert_failure_keeps_draft_and_receipts() {
    publication_cases::outbox_insert_failure_keeps_draft_and_receipts().unwrap()
}

#[test]
fn journal_failure_after_outbox_insert_rolls_back_event_and_projection() {
    publication_cases::journal_failure_after_outbox_insert_rolls_back_event_and_projection()
        .unwrap()
}

#[test]
fn lost_commit_response_reopens_one_exact_pending_source() {
    publication_cases::lost_commit_response_reopens_one_exact_pending_source().unwrap()
}

#[test]
fn only_complete_canonical_exact_source_acks() {
    publication_cases::only_complete_canonical_exact_source_acks().unwrap()
}

fn expect_source(f: &Fixture, s: &mut PageSession, event: &Event) {
    let mut journal = journal(s).clone();
    journal.commit = Some(PlanCommitRef {
        client_id: event.client_id.clone(),
        event_id: plan::source(&f.space, &f.item, event, false).event_id,
        published_revision_base64: encode_bytes(
            &plan_doc(&s.live)
                .unwrap()
                .revision()
                .unwrap()
                .encode()
                .unwrap(),
        ),
        published_content_hash: plan_doc(&s.live).unwrap().semantic_hash().unwrap(),
    });
    super::super::drafts::tagged::save(&f.engine, f.engine.item_state(&f.item).unwrap(), &journal)
        .unwrap();
    s.refresh(&f.engine).unwrap();
}

#[test]
fn semantically_refused_source_preserves_draft_and_visible_history() {
    publication_cases::semantically_refused_source_preserves_draft_and_visible_history().unwrap()
}

#[test]
fn waiting_blob_blocks_edit_ack_and_later_versions_until_ready() {
    publication_cases::waiting_blob_blocks_edit_ack_and_later_versions_until_ready().unwrap()
}

#[test]
fn pending_legacy_and_remote_cutover_never_expand_seed() {
    authority_cutover_cases::pending_legacy_and_remote_cutover_never_expand_seed().unwrap()
}

#[test]
fn late_legacy_versions_never_ack_or_change_canonical_shadow() {
    authority_cutover_cases::late_legacy_versions_never_ack_or_change_canonical_shadow().unwrap()
}

#[test]
fn reader_removed_actor_and_key_loss_refuse_mutation_but_keep_saved_read() {
    authority_cutover_cases::reader_removed_actor_and_key_loss_refuse_mutation_but_keep_saved_read()
        .unwrap()
}

#[test]
fn newer_draft_survives_older_ack_and_permanent_transport_refusal() {
    publication_cases::newer_draft_survives_older_ack_and_permanent_transport_refusal().unwrap()
}

#[test]
fn blob_publication_and_two_first_writers_converge_without_cas() {
    convergence_backup_cases::blob_publication_and_two_first_writers_converge_without_cas().unwrap()
}

// Both document tags use the existing device-key-sealed page.draft namespace.
// Exercise actual journals and a newly certified device, not fabricated ciphertext.

#[test]
fn backup_export_excludes_actual_page_and_plan_private_journals_without_mutating_source() {
    convergence_backup_cases::backup_export_excludes_actual_page_and_plan_private_journals_without_mutating_source().unwrap()
}

#[test]
fn backup_restore_strips_authentic_old_device_page_and_plan_journals_before_edit_admission() {
    convergence_backup_cases::backup_restore_strips_authentic_old_device_page_and_plan_journals_before_edit_admission().unwrap()
}

#[path = "input_tests.rs"]
mod input_tests;

// Complete cases keep their original annotated entrypoints above.
mod authority_cutover_cases;
mod binary_compatibility_cases;
mod context_receipts_cases;
mod continuation_cases;
mod convergence_backup_cases;
mod fixture;
mod journal_recovery_cases;
mod publication_cases;
mod support;
use support::*;
