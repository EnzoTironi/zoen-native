use super::*;

#[test]
fn failed_media_recovery_default_ws_file_rejection_is_durable_and_explicit() {
    default_refusal_cases::failed_media_recovery_default_ws_file_rejection_is_durable_and_explicit()
        .unwrap()
}

#[test]
fn failed_media_recovery_default_ws_refusal_is_atomic_and_keeps_other_pending_files() {
    default_refusal_cases::failed_media_recovery_default_ws_refusal_is_atomic_and_keeps_other_pending_files()
}

#[test]
fn failed_media_recovery_copies_single_or_whole_signed_album_without_sending_or_reusing_intents() {
    copy_recovery_cases::failed_media_recovery_copies_single_or_whole_signed_album_without_sending_or_reusing_intents()
}

#[test]
fn failed_media_recovery_cas_keeps_normal_and_media_drafts_and_rejects_stale_revisions() {
    copy_recovery_cases::failed_media_recovery_cas_keeps_normal_and_media_drafts_and_rejects_stale_revisions()
}

#[test]
fn failed_media_recovery_rejects_pending_confirmed_foreign_scope_and_revoked_access() {
    admission_cases::failed_media_recovery_rejects_pending_confirmed_foreign_scope_and_revoked_access()
}

#[test]
fn failed_media_recovery_revalidates_actual_signed_outbox_device_and_complete_group() {
    admission_cases::failed_media_recovery_revalidates_actual_signed_outbox_device_and_complete_group().unwrap()
}

#[test]
fn failed_media_recovery_missing_corrupt_chunks_and_private_storage_failure_rollback_every_copy() {
    admission_cases::failed_media_recovery_missing_corrupt_chunks_and_private_storage_failure_rollback_every_copy()
}

#[test]
fn failed_voice_recovery_imports_only_final_render_as_a_new_source_without_invented_editor() {
    admission_cases::failed_voice_recovery_imports_only_final_render_as_a_new_source_without_invented_editor()
}

#[test]
fn failed_media_recovery_preserves_the_signed_inline_or_thread_destination() {
    copy_recovery_cases::failed_media_recovery_preserves_the_signed_inline_or_thread_destination()
}

#[test]
fn failed_media_recovery_reopens_the_same_encrypted_copy_and_original_failed_history() {
    copy_recovery_cases::failed_media_recovery_reopens_the_same_encrypted_copy_and_original_failed_history().unwrap()
}

#[tokio::test]
async fn failed_media_recovery_json_api_keeps_integer_wire_contract_and_rejects_extra_fields() {
    copy_recovery_cases::failed_media_recovery_json_api_keeps_integer_wire_contract_and_rejects_extra_fields().await
}

// Complete cases keep their original annotated entrypoints above.
mod admission_cases;
mod copy_recovery_cases;
mod default_refusal_cases;
mod support;
use support::*;
