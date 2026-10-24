use super::*;
use crate::{PrivacyDto, SecretVault};
use std::sync::{Arc, Mutex};

mod gallery_cases;
mod recovery;

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
    scope: MediaDraftScope,
}

#[test]
fn private_selection_reopens_encrypted_without_a_message_file_or_upload() {
    selection_cases::private_selection_reopens_encrypted_without_a_message_file_or_upload().unwrap()
}

#[test]
fn compare_and_swap_preserves_immutable_assets_and_prunes_only_removed_references() {
    selection_cases::compare_and_swap_preserves_immutable_assets_and_prunes_only_removed_references(
    )
}

#[test]
fn second_file_failure_rolls_back_entire_batch_and_freezes_exact_retry() {
    save_receipts_cases::second_file_failure_rolls_back_entire_batch_and_freezes_exact_retry()
}

#[test]
fn accepted_cold_retry_returns_same_entries_and_preserves_a_newer_unsent_selection() {
    publication_cases::accepted_cold_retry_returns_same_entries_and_preserves_a_newer_unsent_selection().unwrap()
}

#[test]
fn voice_keeps_original_and_edit_recipe_private_then_sends_one_exact_render_once() {
    voice_cases::voice_keeps_original_and_edit_recipe_private_then_sends_one_exact_render_once()
}

#[test]
fn owner_membership_reply_and_thread_admission_precede_attempted_or_publication() {
    selection_cases::owner_membership_reply_and_thread_admission_precede_attempted_or_publication()
}

#[test]
fn ciphertext_corruption_or_asset_substitution_never_publishes_or_overwrites_original_bytes() {
    selection_cases::ciphertext_corruption_or_asset_substitution_never_publishes_or_overwrites_original_bytes()
}

#[test]
fn malformed_content_duplicate_ids_and_invalid_voice_metadata_fail_without_private_or_public_writes(
) {
    selection_cases::malformed_content_duplicate_ids_and_invalid_voice_metadata_fail_without_private_or_public_writes()
}

#[test]
fn private_asset_and_record_storage_failures_rollback_and_can_retry_without_orphans() {
    save_receipts_cases::private_asset_and_record_storage_failures_rollback_and_can_retry_without_orphans()
}

#[test]
fn accepted_retry_precedes_changed_writer_reply_and_clear_retains_receipt() {
    publication_cases::accepted_retry_precedes_changed_writer_reply_and_clear_retains_receipt()
}

#[test]
fn backup_restore_and_device_erase_never_carry_a_private_media_draft_to_another_account() {
    save_receipts_cases::backup_restore_and_device_erase_never_carry_a_private_media_draft_to_another_account().unwrap()
}

#[test]
fn failed_receipt_commit_rolls_back_canonical_send_and_retains_the_attempted_payload() {
    save_receipts_cases::failed_receipt_commit_rolls_back_canonical_send_and_retains_the_attempted_payload()
}

#[test]
fn voice_append_failure_rolls_back_chunks_keys_and_uploads_then_retries_once() {
    voice_cases::voice_append_failure_rolls_back_chunks_keys_and_uploads_then_retries_once()
}

#[test]
fn history_export_remains_only_canonical_events_while_media_draft_is_private() {
    custody_cases::history_export_remains_only_canonical_events_while_media_draft_is_private()
}

#[test]
fn exact_lost_save_ack_replay_returns_newer_current_and_checks_supplied_bytes_metadata_and_order() {
    save_receipts_cases::exact_lost_save_ack_replay_returns_newer_current_and_checks_supplied_bytes_metadata_and_order()
}

#[test]
fn exact_clear_replay_preserves_newer_content_and_save_receipts_survive_send() {
    save_receipts_cases::exact_clear_replay_preserves_newer_content_and_save_receipts_survive_send()
}

#[test]
fn evicted_save_ack_is_stale_without_guessing_or_overwriting_current_payload() {
    save_receipts_cases::evicted_save_ack_is_stale_without_guessing_or_overwriting_current_payload()
}

#[test]
fn voice_save_ack_survives_realistic_float_normalization_and_identifies_newer_current() {
    voice_cases::voice_save_ack_survives_realistic_float_normalization_and_identifies_newer_current(
    )
}

#[test]
fn legacy_receipt_without_output_revision_preserves_cold_draft_but_cannot_guess_ack() {
    save_receipts_cases::legacy_receipt_without_output_revision_preserves_cold_draft_but_cannot_guess_ack().unwrap()
}

#[test]
fn media_contract_defaults_preserve_legacy_save_receipts_and_signed_bytes() {
    save_receipts_cases::media_contract_defaults_preserve_legacy_save_receipts_and_signed_bytes()
}

#[test]
fn media_contract_file_album_preserves_real_mime_order_signature_and_retry() {
    album_publication_cases::media_contract_file_album_preserves_real_mime_order_signature_and_retry(
    ).unwrap()
}

#[test]
fn media_contract_transient_first_album_refusal_preserves_all_confirmed_members_after_reopen() {
    album_publication_cases::media_contract_transient_first_album_refusal_preserves_all_confirmed_members_after_reopen().unwrap()
}

#[test]
fn media_contract_single_media_transient_file_refusal_survives_cold_receiver() {
    album_publication_cases::media_contract_single_media_transient_file_refusal_survives_cold_receiver().unwrap()
}

#[test]
fn media_contract_group_dependencies_wait_for_confirmed_files_and_do_not_block_other_spaces() {
    group_admission_cases::media_contract_group_dependencies_wait_for_confirmed_files_and_do_not_block_other_spaces()
}

#[test]
fn media_contract_grouped_preexisting_confirmed_files_need_no_new_creation() {
    group_admission_cases::media_contract_grouped_preexisting_confirmed_files_need_no_new_creation()
        .unwrap()
}

#[test]
fn media_contract_permanent_file_refusal_fails_dependents_and_releases_later_heads() {
    refusals_cases::media_contract_permanent_file_refusal_fails_dependents_and_releases_later_heads(
    )
    .unwrap()
}

#[test]
fn media_contract_refused_duplicate_creation_cannot_poison_confirmed_file_reference() {
    refusals_cases::media_contract_refused_duplicate_creation_cannot_poison_confirmed_file_reference(
    ).unwrap()
}

#[test]
fn media_contract_terminal_group_refusal_is_atomic_and_exactly_scoped() {
    refusals_cases::media_contract_terminal_group_refusal_is_atomic_and_exactly_scoped().unwrap()
}

#[test]
fn media_contract_group_failure_rolls_back_and_retains_frozen_options() {
    save_receipts_cases::media_contract_group_failure_rolls_back_and_retains_frozen_options()
}

#[test]
fn media_contract_rejects_invalid_options_and_foreign_authority_without_mutation() {
    group_admission_cases::media_contract_rejects_invalid_options_and_foreign_authority_without_mutation().unwrap()
}

#[test]
fn media_contract_rejects_invalid_group_index_and_duplicate_membership_atomically() {
    group_admission_cases::media_contract_rejects_invalid_group_index_and_duplicate_membership_atomically()
}

#[test]
fn media_contract_forwarded_group_is_rejected_and_single_forward_preserves_file_presentation() {
    group_admission_cases::media_contract_forwarded_group_is_rejected_and_single_forward_preserves_file_presentation().unwrap()
}

#[test]
fn media_contract_failed_local_metadata_remains_visible_after_revocation_and_cold_recovery() {
    custody_cases::media_contract_failed_local_metadata_remains_visible_after_revocation_and_cold_recovery().unwrap()
}

#[test]
fn media_contract_received_ordered_fixtures_use_the_same_cursor_as_browser() {
    let fixtures: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../types/src/media-received-fixtures.json"
    ))
    .unwrap();
    for scenario in fixtures {
        let f = fixture(":memory:");
        let members = scenario["members"].as_array().unwrap();
        for index in 0..members.len() {
            f.engine
                .lock()
                .file_add(
                    &f.scope.space_id,
                    "",
                    &format!("{index}.pdf"),
                    "application/pdf",
                    &[index as u8, 7],
                    None,
                )
                .unwrap();
        }
        media_contract_confirm_scope(&f);
        let items = f
            .engine
            .lock()
            .state
            .items
            .values()
            .filter(|item| item.space == f.scope.space_id)
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        for (index, member) in members.iter().enumerate() {
            let media = types::MessageMedia {
                presentation: if member["presentation"] == "file" {
                    types::AttachmentPresentation::File
                } else {
                    types::AttachmentPresentation::Media
                },
                group: Some(types::MediaGroup {
                    id: format!("album_{}", "a".repeat(64)),
                    index: member["index"].as_u64().unwrap() as u32,
                    count: member["count"].as_u64().unwrap_or(2) as u32,
                }),
            };
            media_contract_receive(
                &f,
                types::EventBody::MessagePosted {
                    message: types::new_id("msg").unwrap(),
                    text: format!("fixture-{index}"),
                    attaches: Some(items[index].clone()),
                    media: Some(media),
                    reply: member["reply"].as_str().map(|to| types::ReplyRef {
                        to: to.into(),
                        thread: false,
                    }),
                    forwarded: member["forwarded"].as_bool().unwrap_or(false),
                },
                false,
            )
            .unwrap();
        }
        let accepted = f
            .engine
            .timeline(f.scope.space_id.clone())
            .unwrap()
            .iter()
            .filter_map(|entry| match &entry.kind {
                crate::EntryKind::Message { text, .. } => text
                    .strip_prefix("fixture-")
                    .and_then(|index| index.parse::<u64>().ok()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            accepted,
            scenario["accepted"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_u64().unwrap())
                .collect::<Vec<_>>(),
            "{}",
            scenario["name"]
        );
    }
}

#[test]
fn media_contract_recovery_only_file_refuses_legacy_attachment_command() {
    recovery_authority_cases::media_contract_recovery_only_file_refuses_legacy_attachment_command()
        .unwrap()
}

#[test]
fn media_contract_recovery_only_file_refuses_new_version_before_writes() {
    recovery_authority_cases::media_contract_recovery_only_file_refuses_new_version_before_writes()
        .unwrap()
}

#[test]
fn media_contract_recovery_only_file_refuses_revert_command() {
    recovery_authority_cases::media_contract_recovery_only_file_refuses_revert_command().unwrap()
}

#[test]
fn media_contract_recovery_only_file_refuses_signed_projection_and_ingest() {
    recovery_authority_cases::media_contract_recovery_only_file_refuses_signed_projection_and_ingest(
    ).unwrap()
}

#[test]
fn media_contract_recovery_bytes_can_be_explicitly_copied_to_a_fresh_file() {
    recovery_authority_cases::media_contract_recovery_bytes_can_be_explicitly_copied_to_a_fresh_file(
    ).unwrap()
}

#[test]
fn media_contract_recovery_reservation_yields_to_a_validated_file_creation() {
    recovery_authority_cases::media_contract_recovery_reservation_yields_to_a_validated_file_creation().unwrap()
}

// These projection/WS tests start AFTER the upload prerequisite succeeds. Actual
// HTTP terminal/transient and cold recovery coverage is in net/tests/media_uploads.

// Complete cases keep their original annotated entrypoints above.
mod album_publication_cases;
mod custody_cases;
mod group_admission_cases;
mod publication_cases;
mod recovery_authority_cases;
mod refusals_cases;
mod save_receipts_cases;
mod selection_cases;
mod support;
mod voice_cases;
use support::*;
