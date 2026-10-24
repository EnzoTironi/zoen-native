use super::*;
use crate::{PrivacyDto, SecretVault};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use types::{EventBody, Role};

#[derive(Default)]
struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.0.lock().unwrap().insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        self.0.lock().unwrap().remove(&key);
    }
}
struct Fixture {
    engine: Arc<Client>,
    vault: Arc<Vault>,
    scope: FileEditDraftScope,
    origin: FileEditDraftCommitResult,
}
fn pdf(id: &str) -> Vec<u8> {
    format!("%PDF-1.7\n% Private edited PDF snapshot {id}\n%%EOF\n").into_bytes()
}
fn fixture(path: &str) -> Fixture {
    let engine = Client::open(path.into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    engine
        .create_account(
            "PDF editor".into(),
            "pdf_editor".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )
        .unwrap();
    let space = engine
        .create_group_with("PDF edits".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let item = engine
        .file_add(
            space.clone(),
            "document.pdf".into(),
            "document.pdf".into(),
            "application/pdf".into(),
            pdf("original"),
            None,
        )
        .unwrap();
    let account = engine.account().unwrap();
    let origin = FileEditDraftCommitResult {
        version: item.version,
        sha256: item.file.unwrap().sha256,
    };
    Fixture {
        engine,
        vault,
        scope: FileEditDraftScope {
            expected_identity_id: account.identity_id,
            expected_device_id: account.device_id,
            space_id: space,
            item_id: item.id,
        },
        origin,
    }
}
fn read(f: &Fixture) -> FileEditDraftDto {
    f.engine.file_edit_draft(f.scope.clone()).unwrap()
}
fn stage(f: &Fixture, expected: &str, editor: &str, capture: &str) -> FileEditDraftSaveResult {
    f.engine
        .stage_file_edit_draft(
            f.scope.clone(),
            expected.into(),
            editor.into(),
            capture.into(),
            f.origin.version,
            f.origin.sha256.clone(),
            pdf(capture),
        )
        .unwrap()
}
fn commit(f: &Fixture, editor: &str, capture: &str) -> FileEditDraftCommitResult {
    f.engine
        .commit_file_edit_draft(
            f.scope.clone(),
            editor.into(),
            capture.into(),
            "Annotations".into(),
        )
        .unwrap()
}
fn export(f: &Fixture, revision: &str, capture: &str) -> R<Vec<u8>> {
    f.engine
        .file_edit_draft_bytes(f.scope.clone(), revision.into(), capture.into())
}
fn counts(e: &Client) -> Vec<i64> {
    let e = e.lock();
    let mut result = ["events", "media", "media_keys", "blob_uploads", "outbox"]
        .map(|table| {
            e.store
                .conn()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap()
        })
        .to_vec();
    result.push(
        e.store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key NOT LIKE 'file.edit.draft.%'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
    );
    result
}
fn rows(f: &Fixture) -> Vec<(String, String)> {
    let e = f.engine.lock();
    let mut statement = e
        .store
        .conn()
        .prepare("SELECT key,value FROM meta WHERE key LIKE 'file.edit.draft.%' ORDER BY key")
        .unwrap();
    statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}
fn temp() -> std::result::Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(types::new_id("file-edit-draft-test")?);
    std::fs::create_dir_all(&path).unwrap();
    Ok(path)
}
fn role(f: &Fixture, role: Role) {
    f.engine
        .lock()
        .append(
            &f.scope.space_id,
            &f.scope.expected_identity_id,
            EventBody::MemberAdded {
                identity: f.scope.expected_identity_id.clone(),
                role,
            },
        )
        .unwrap();
}

#[test]
fn complete_captures_are_private_encrypted_and_survive_a_locked_cold_reopen() {
    private_custody::complete_captures_are_private_encrypted_and_survive_a_locked_cold_reopen()
        .unwrap()
}

#[test]
fn fifo_keeps_all_snapshots_and_advances_only_its_own_base_while_the_origin_stays_pinned() {
    capture_queue::fifo_keeps_all_snapshots_and_advances_only_its_own_base_while_the_origin_stays_pinned()
}

#[test]
fn lost_commit_ack_reopens_and_returns_its_exact_receipt_without_touching_newer_edits_or_versions()
{
    capture_queue::lost_commit_ack_reopens_and_returns_its_exact_receipt_without_touching_newer_edits_or_versions().unwrap()
}

#[test]
fn remote_change_preserves_late_full_capture_but_never_rebases_or_publishes_it() {
    capture_queue::remote_change_preserves_late_full_capture_but_never_rebases_or_publishes_it()
}

#[test]
fn stage_replay_authenticates_all_bytes_and_binding_inputs_and_discard_is_explicit_cas() {
    admission::stage_replay_authenticates_all_bytes_and_binding_inputs_and_discard_is_explicit_cas()
}

#[test]
fn editor_claims_are_exclusive_until_empty_and_preserve_historical_origin_without_rebasing() {
    capture_queue::editor_claims_are_exclusive_until_empty_and_preserve_historical_origin_without_rebasing()
}

#[test]
fn first_capture_after_a_remote_change_is_durable_but_can_never_overwrite_that_remote_version() {
    capture_queue::first_capture_after_a_remote_change_is_durable_but_can_never_overwrite_that_remote_version()
}

#[test]
fn exact_owner_device_item_space_membership_and_writer_checks_guard_every_boundary() {
    admission::exact_owner_device_item_space_membership_and_writer_checks_guard_every_boundary()
}

#[test]
fn origin_hash_and_current_pdf_type_are_canonical_and_cannot_be_supplied_by_the_caller() {
    admission::origin_hash_and_current_pdf_type_are_canonical_and_cannot_be_supplied_by_the_caller()
}

#[test]
fn corrupted_ciphertexts_fail_closed_and_preserve_every_captured_byte_and_canonical_version() {
    failure_recovery::corrupted_ciphertexts_fail_closed_and_preserve_every_captured_byte_and_canonical_version()
}

#[test]
fn failed_private_writes_and_canonical_or_receipt_writes_rollback_and_retry_exactly_once() {
    failure_recovery::failed_private_writes_and_canonical_or_receipt_writes_rollback_and_retry_exactly_once()
}

#[test]
fn validation_queue_bounds_and_same_bytes_commit_preserve_existing_file_semantics() {
    failure_recovery::validation_queue_bounds_and_same_bytes_commit_preserve_existing_file_semantics(
    )
}

#[test]
fn backup_restore_and_device_erase_exclude_private_pdf_journals_even_from_retained_snapshots() {
    private_custody::backup_restore_and_device_erase_exclude_private_pdf_journals_even_from_retained_snapshots().unwrap()
}

mod admission;
mod capture_queue;
mod failure_recovery;
mod private_custody;
