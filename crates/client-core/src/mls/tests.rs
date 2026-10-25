//! Stable original MLS test entrypoints; each executes the complete case body.
mod durability_tests;

use super::*;
use std::collections::BTreeMap;

use event_log::{chain_hash, Author, Signer};

const FORK: &str = "Members see different groups in this chat (the server may have forked it).";

mod support;
use support::*;
mod app_request_readers;
mod authorization_cases;
mod catchup_projection;
mod claim_retry_faults;
mod cold_recovery;
mod existing_app_member;
mod native_member_addition;
mod outbox_cases;
mod prefix_admission_cases;
mod prefix_settlement_cases;
mod publish_window_cases;
mod readiness_cases;
mod recovery_cases;

#[test]
fn direct_seed_waits_for_missing_package_then_reaches_the_same_recipient() {
    readiness_cases::direct_seed_waits_for_missing_package_then_reaches_the_same_recipient();
}

#[test]
fn group_existing_reader_can_receive_while_another_identity_has_no_package() {
    readiness_cases::group_existing_reader_can_receive_while_another_identity_has_no_package();
}

#[test]
fn fifty_one_members_reconcile_in_two_durable_claim_batches() {
    readiness_cases::fifty_one_members_reconcile_in_two_durable_claim_batches();
}

#[test]
fn unsupported_claims_in_one_chat_do_not_block_another_chats_removal() {
    readiness_cases::unsupported_claims_in_one_chat_do_not_block_another_chats_removal();
}

#[test]
fn an_obsolete_stale_rejection_cannot_release_a_current_epoch_block() {
    outbox_cases::an_obsolete_stale_rejection_cannot_release_a_current_epoch_block();
}

#[test]
fn rejected_outer_signature_keeps_the_own_event_pending() {
    outbox_cases::rejected_outer_signature_keeps_the_own_event_pending();
}

#[test]
fn rejected_pruned_history_preserves_the_mls_group() {
    outbox_cases::rejected_pruned_history_preserves_the_mls_group();
}

/// Twelve sealed pages, delivered in the worst order the publish window allows.
/// A seventh generation opened before the first is permanent loss; the outbox
/// never offers that sixth in-flight ciphertext.
#[test]
fn publish_window_keeps_a_reordered_burst_decryptable() {
    publish_window_cases::publish_window_keeps_a_reordered_burst_decryptable();
}

/// Space A already has five application generations in flight. Its sixth
/// application, and anything newer in that same Space, stays queued. Space B
/// still publishes.
#[test]
fn publish_window_lets_the_next_space_through() {
    publish_window_cases::publish_window_lets_the_next_space_through();
}

#[test]
fn divergent_prefix_on_the_local_chain_marks_the_space() {
    prefix_settlement_cases::divergent_prefix_on_the_local_chain_marks_the_space();
}

#[test]
fn matching_prefix_does_not_fork() {
    prefix_settlement_cases::matching_prefix_does_not_fork();
}

#[test]
fn matching_pending_prefix_is_cleared_without_a_fork() {
    prefix_settlement_cases::matching_pending_prefix_is_cleared_without_a_fork();
}

#[test]
fn pending_prefix_forks_when_the_chain_reaches_it() {
    prefix_settlement_cases::pending_prefix_forks_when_the_chain_reaches_it();
}

#[test]
fn pending_prefix_survives_reopen_and_forks_when_reached() {
    prefix_settlement_cases::pending_prefix_survives_reopen_and_forks_when_reached();
}

#[test]
fn divergent_digest_still_marks_the_space() {
    prefix_settlement_cases::divergent_digest_still_marks_the_space();
}

#[test]
fn colluding_near_claims_do_not_evict_an_honest_far_claim() {
    prefix_admission_cases::colluding_near_claims_do_not_evict_an_honest_far_claim();
}

#[test]
fn same_device_higher_claim_replaces_the_lower_and_catches_an_earlier_fork() {
    prefix_admission_cases::same_device_higher_claim_replaces_the_lower_and_catches_an_earlier_fork(
    );
}

#[test]
fn two_devices_of_one_member_keep_separate_claims() {
    prefix_admission_cases::two_devices_of_one_member_keep_separate_claims();
}

#[test]
fn a_non_member_checkpoint_is_ignored() {
    prefix_admission_cases::a_non_member_checkpoint_is_ignored();
}

#[test]
fn an_honest_conversation_with_many_members_ends_with_no_pending_prefix() {
    prefix_admission_cases::an_honest_conversation_with_many_members_ends_with_no_pending_prefix();
}

#[test]
fn a_removed_member_pending_claim_is_kept_until_the_chain_reaches_it() {
    prefix_admission_cases::a_removed_member_pending_claim_is_kept_until_the_chain_reaches_it();
}

#[test]
fn pending_prefix_ceiling_refuses_a_new_device_without_evicting() {
    prefix_admission_cases::pending_prefix_ceiling_refuses_a_new_device_without_evicting();
}

#[test]
fn a_member_seen_at_or_beyond_the_event_seq_is_a_fork() {
    authorization_cases::a_member_seen_at_or_beyond_the_event_seq_is_a_fork();
}

#[test]
fn an_event_ahead_of_the_local_head_stays_a_gap() {
    authorization_cases::an_event_ahead_of_the_local_head_stays_a_gap();
}

#[test]
fn a_non_member_seen_beyond_the_event_is_refused_without_a_fork() {
    authorization_cases::a_non_member_seen_beyond_the_event_is_refused_without_a_fork();
}

#[test]
fn a_forged_signature_is_refused_without_a_fork() {
    authorization_cases::a_forged_signature_is_refused_without_a_fork();
}

#[test]
fn a_non_member_diverged_seen_is_refused_without_a_fork() {
    authorization_cases::a_non_member_diverged_seen_is_refused_without_a_fork();
}

#[test]
fn a_crash_after_fork_meta_still_reports_the_fork_after_reopen() {
    recovery_cases::a_crash_after_fork_meta_still_reports_the_fork_after_reopen();
}

#[test]
fn a_pending_claim_settles_through_a_pruned_stub() {
    recovery_cases::a_pending_claim_settles_through_a_pruned_stub();
}

#[test]
fn concurrent_sealed_siblings_send_until_a_member_proves_a_fork() {
    recovery_cases::concurrent_sealed_siblings_send_until_a_member_proves_a_fork();
}
