//! Voice notes travel as encrypted audio files (the chunked media path), not text markers.
use super::*;
use types::{EventBody, ItemContent, Role};

// An untrusted relay may sequence a correctly signed but unauthorized event.

#[test]
fn a_voice_note_is_an_encrypted_audio_file_with_its_waveform() {
    presentation_cases::a_voice_note_is_an_encrypted_audio_file_with_its_waveform()
}

#[test]
fn a_voice_reply_shows_once_with_its_quote_and_threads() {
    presentation_cases::a_voice_reply_shows_once_with_its_quote_and_threads()
}

#[test]
fn web_recordings_play_and_other_audio_files_stay_files() {
    presentation_cases::web_recordings_play_and_other_audio_files_stay_files()
}

#[test]
fn only_audio_is_a_voice_note() {
    presentation_cases::only_audio_is_a_voice_note()
}

#[test]
fn forwarding_in_the_same_chat_keeps_the_original_and_its_reactions() {
    forwarding_cases::forwarding_in_the_same_chat_keeps_the_original_and_its_reactions()
}

#[test]
fn cross_space_voice_forwarding_copies_playable_bytes_with_fresh_destination_keys_once() {
    forwarding_cases::cross_space_voice_forwarding_copies_playable_bytes_with_fresh_destination_keys_once()
}

#[test]
fn forwarded_voice_uses_current_bytes_and_cleared_metadata_and_is_independent() {
    forwarding_cases::forwarded_voice_uses_current_bytes_and_cleared_metadata_and_is_independent()
}

#[test]
fn cross_space_voice_forwarding_strips_inline_thread_and_legacy_marker_destinations() {
    forwarding_cases::cross_space_voice_forwarding_strips_inline_thread_and_legacy_marker_destinations()
}

#[test]
fn voice_forwarding_requires_source_access_and_destination_writer_without_leaking_material() {
    forwarding_cases::voice_forwarding_requires_source_access_and_destination_writer_without_leaking_material()
}

#[test]
fn missing_corrupt_or_oversized_source_audio_cannot_create_destination_items_or_keys() {
    forwarding_recovery_cases::missing_corrupt_or_oversized_source_audio_cannot_create_destination_items_or_keys()
}

#[test]
fn failed_voice_forward_append_rolls_back_destination_file_keys_chunks_and_projection() {
    forwarding_recovery_cases::failed_voice_forward_append_rolls_back_destination_file_keys_chunks_and_projection()
}

#[test]
fn historical_voice_metadata_keeps_its_serialized_shape_and_defaults_to_not_forwarded() {
    presentation_cases::historical_voice_metadata_keeps_its_serialized_shape_and_defaults_to_not_forwarded()
}

#[test]
fn unresolved_legacy_voice_markers_cannot_escape_their_source_chat_as_text() {
    forwarding_recovery_cases::unresolved_legacy_voice_markers_cannot_escape_their_source_chat_as_text()
}

#[test]
fn forwarded_voice_survives_cold_reopen_and_a_destination_only_peer_decrypts_new_chunks() {
    forwarding_recovery_cases::forwarded_voice_survives_cold_reopen_and_a_destination_only_peer_decrypts_new_chunks().unwrap()
}

#[test]
fn another_author_or_a_removed_author_cannot_deduplicate_an_earlier_note() {
    author_admission_cases::another_author_or_a_removed_author_cannot_deduplicate_an_earlier_note()
        .unwrap()
}

#[test]
fn invalid_voice_reply_targets_do_not_publish_audio_or_create_items() {
    author_admission_cases::invalid_voice_reply_targets_do_not_publish_audio_or_create_items()
}

#[test]
fn only_writers_can_send_voice_notes() {
    author_admission_cases::only_writers_can_send_voice_notes()
}

#[test]
fn renaming_voice_audio_keeps_metadata_but_replacing_bytes_invalidates_it() {
    presentation_cases::renaming_voice_audio_keeps_metadata_but_replacing_bytes_invalidates_it()
}

#[test]
fn voice_versions_and_reverts_are_author_only_locally_and_on_replay() {
    author_admission_cases::voice_versions_and_reverts_are_author_only_locally_and_on_replay()
        .unwrap()
}

#[test]
fn ordinary_files_stay_collaborative_but_only_the_author_can_make_them_voice() {
    presentation_cases::ordinary_files_stay_collaborative_but_only_the_author_can_make_them_voice()
        .unwrap()
}

#[test]
fn rejected_voice_sends_keep_their_audio_reply_and_delivery_error_after_reopen() {
    presentation_cases::rejected_voice_sends_keep_their_audio_reply_and_delivery_error_after_reopen(
    )
    .unwrap()
}

#[test]
fn core_search_finds_voice_transcripts_as_messages_and_follows_edits() {
    search_compatibility_cases::core_search_finds_voice_transcripts_as_messages_and_follows_edits()
}

#[test]
fn typed_voice_transcripts_are_searchable_by_message_id_and_follow_edits() {
    search_compatibility_cases::typed_voice_transcripts_are_searchable_by_message_id_and_follow_edits()
}

#[test]
fn old_android_file_and_marker_replay_as_one_voice_message() {
    search_compatibility_cases::old_android_file_and_marker_replay_as_one_voice_message()
}

#[test]
fn legacy_two_event_voice_replies_still_replay_once() {
    search_compatibility_cases::legacy_two_event_voice_replies_still_replay_once()
}

#[test]
fn voice_reply_syncs_its_metadata_and_encrypted_bytes_and_survives_replay(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    search_compatibility_cases::voice_reply_syncs_its_metadata_and_encrypted_bytes_and_survives_replay()
}

// Complete cases keep their original annotated entrypoints above.
mod author_admission_cases;
mod forwarding_cases;
mod forwarding_recovery_cases;
mod presentation_cases;
mod search_compatibility_cases;
mod support;
use support::*;
