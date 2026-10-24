//! Ordinary attachments must be real chat messages, without changing library imports.
use super::*;
use types::{EventBody, Role};

#[test]
fn ordinary_files_are_actual_messages_and_library_imports_stay_quiet() {
    sending_cases::ordinary_files_are_actual_messages_and_library_imports_stay_quiet()
}

#[tokio::test]
async fn file_replies_preserve_inline_and_normalized_thread_destinations() {
    sending_cases::file_replies_preserve_inline_and_normalized_thread_destinations().await
}

#[test]
fn invalid_or_foreign_targets_and_nonwriters_leave_no_file_or_key_material() {
    sending_cases::invalid_or_foreign_targets_and_nonwriters_leave_no_file_or_key_material()
}

#[test]
fn failed_message_or_receipt_write_rolls_back_files_keys_and_projection() {
    sending_cases::failed_message_or_receipt_write_rolls_back_files_keys_and_projection()
}

#[test]
fn exact_file_send_retries_survive_cold_reopen_and_reject_intent_drift() {
    sending_cases::exact_file_send_retries_survive_cold_reopen_and_reject_intent_drift().unwrap()
}

#[test]
fn file_caption_is_signed_with_its_attachment_and_thread_before_receiver_replay() {
    captions_cases::file_caption_is_signed_with_its_attachment_and_thread_before_receiver_replay()
}

#[test]
fn clearing_file_caption_through_public_edit_reaches_signed_recipient_without_changing_file() {
    captions_cases::clearing_file_caption_through_public_edit_reaches_signed_recipient_without_changing_file()
}

#[test]
fn blank_caption_edit_is_scoped_to_live_own_ordinary_files_and_enforces_caption_bounds() {
    captions_cases::blank_caption_edit_is_scoped_to_live_own_ordinary_files_and_enforces_caption_bounds()
}

#[test]
fn captioned_file_retries_survive_reopen_and_reject_caption_drift() {
    captions_cases::captioned_file_retries_survive_reopen_and_reject_caption_drift().unwrap()
}

#[test]
fn file_caption_bounds_and_failed_receipt_leave_no_partial_file_or_message() {
    captions_cases::file_caption_bounds_and_failed_receipt_leave_no_partial_file_or_message()
}

#[test]
fn file_message_sync_delivers_the_original_attachment_and_encrypted_chunks() {
    sending_cases::file_message_sync_delivers_the_original_attachment_and_encrypted_chunks()
}

#[test]
fn cross_chat_media_forwarding_copies_current_files_without_source_bindings() {
    forwarding_cases::cross_chat_media_forwarding_copies_current_files_without_source_bindings()
}

#[test]
fn destination_only_receiver_decrypts_forwarded_media_and_thumbnail() {
    forwarding_cases::destination_only_receiver_decrypts_forwarded_media_and_thumbnail()
}

#[test]
fn unavailable_or_corrupt_forwarded_media_does_not_write_a_partial_copy() {
    forwarding_cases::unavailable_or_corrupt_forwarded_media_does_not_write_a_partial_copy()
}

#[test]
fn failed_forwarded_file_or_message_write_rolls_back_every_copy_and_can_retry() {
    forwarding_cases::failed_forwarded_file_or_message_write_rolls_back_every_copy_and_can_retry()
}

#[test]
fn same_chat_file_forwarding_keeps_the_existing_attachment_and_foreign_items_fail_cleanly() {
    forwarding_cases::same_chat_file_forwarding_keeps_the_existing_attachment_and_foreign_items_fail_cleanly()
}

#[test]
fn forwarding_an_ordinary_audio_filename_does_not_create_a_second_voice_message() {
    forwarding_cases::forwarding_an_ordinary_audio_filename_does_not_create_a_second_voice_message()
}

// Complete cases keep their original annotated entrypoints above.
mod captions_cases;
mod forwarding_cases;
mod sending_cases;
mod support;
use support::*;
