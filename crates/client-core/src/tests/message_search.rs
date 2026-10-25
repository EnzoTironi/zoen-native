use super::*;
use types::{Annotation, EventBody};

#[test]
fn mutations_search_and_references_survive_reopen() {
    historical_references_cases::mutations_search_and_references_survive_reopen().unwrap()
}

#[test]
fn scoped_pages_are_bounded_complete_and_never_leak_other_conversations() {
    scoped_paging_cases::scoped_pages_are_bounded_complete_and_never_leak_other_conversations()
}

#[test]
fn scoped_search_rejects_invalid_stale_cross_query_and_cross_space_cursors() {
    scoped_paging_cases::scoped_search_rejects_invalid_stale_cross_query_and_cross_space_cursors()
}

#[test]
fn scoped_search_propagates_database_errors_and_checks_membership() {
    scoped_paging_cases::scoped_search_propagates_database_errors_and_checks_membership()
}

#[test]
fn historical_recipe_activity_is_searchable_by_its_exact_entry_after_reopen() {
    historical_references_cases::historical_recipe_activity_is_searchable_by_its_exact_entry_after_reopen().unwrap()
}

#[test]
fn activity_and_message_pages_preserve_threads_deletion_scope_and_unique_ids() {
    historical_references_cases::activity_and_message_pages_preserve_threads_deletion_scope_and_unique_ids()
}

#[test]
fn activity_search_uses_historical_note_not_current_mutable_file_title() {
    historical_references_cases::activity_search_uses_historical_note_not_current_mutable_file_title(
    )
}

// Complete cases keep their original annotated entrypoints above.
mod historical_references_cases;
mod scoped_paging_cases;
mod support;
use support::*;
