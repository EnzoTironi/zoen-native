use super::*;
use crate::pages::input::{PageInputDto, PageInputMode, PageInputTarget, MAX_CAPTURE};
use crate::FileEditDraftScope;

const WRITER_A: &str = "00000000-0000-4000-8000-000000000001";
const WRITER_B: &str = "00000000-0000-4000-8000-000000000002";

#[test]
fn lossless_input_cold_recovery_preserves_kind_marks_and_exact_retry() {
    capture_acceptance_cases::lossless_input_cold_recovery_preserves_kind_marks_and_exact_retry()
}

#[test]
fn lossless_input_rejects_malformed_ranges_without_replacing_recovery() {
    capture_acceptance_cases::lossless_input_rejects_malformed_ranges_without_replacing_recovery()
}

#[test]
fn page_and_plan_input_cold_reopen_preserve_exact_capture_and_private_journal() {
    capture_acceptance_cases::page_and_plan_input_cold_reopen_preserve_exact_capture_and_private_journal()
}

#[test]
fn input_cas_sequence_and_reacquire_replay_exact_acceptance_without_replacing_newer_input() {
    capture_acceptance_cases::input_cas_sequence_and_reacquire_replay_exact_acceptance_without_replacing_newer_input()
}

#[test]
fn input_apply_sql_failure_and_lost_ack_cold_reopen_have_one_atomic_acceptance() {
    capture_acceptance_cases::input_apply_sql_failure_and_lost_ack_cold_reopen_have_one_atomic_acceptance()
}

#[test]
fn input_discard_guards_and_publication_ack_preserve_private_document_and_future_capture() {
    discard_cases::input_discard_guards_and_publication_ack_preserve_private_document_and_future_capture()
}

#[test]
fn input_limits_foreign_scope_and_authenticated_journal_migration_fail_closed() {
    bounds_cases::input_limits_foreign_scope_and_authenticated_journal_migration_fail_closed()
}

#[test]
fn release_empty_binding_is_atomic_durable_and_replays_without_dropping_newer_input() {
    release_cases::release_empty_binding_is_atomic_durable_and_replays_without_dropping_newer_input(
    )
}

#[test]
fn release_empty_requires_the_current_scope_key_and_writing_membership() {
    release_cases::release_empty_requires_the_current_scope_key_and_writing_membership()
}

#[test]
fn plan_input_metadata_survives_pending_legacy_without_admitting_empty_release() {
    discard_cases::plan_input_metadata_survives_pending_legacy_without_admitting_empty_release()
}

// Complete cases keep their original annotated entrypoints above.
#[path = "input_tests/bounds_cases.rs"]
mod bounds_cases;
#[path = "input_tests/capture_acceptance_cases.rs"]
mod capture_acceptance_cases;
#[path = "input_tests/discard_cases.rs"]
mod discard_cases;
#[path = "input_tests/release_cases.rs"]
mod release_cases;
#[path = "input_tests/support.rs"]
mod support;
use support::*;
#[path = "input_tests/whole_document_cases.rs"]
mod whole_document_cases;
