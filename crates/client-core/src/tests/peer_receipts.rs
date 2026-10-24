//! Real-device receipt history must not depend on the demo or a live relay replay.
use super::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use types::{EventBody, Identity, IdentityKind, Privacy, Role, SpaceKind};

struct Database {
    directory: PathBuf,
    path: String,
}

impl Database {
    fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let directory = std::env::temp_dir().join(types::new_id("peer-receipts")?);
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("core.sqlite").to_string_lossy().into_owned();
        Ok(Self { directory, path })
    }

    fn open(&self) -> Arc<Client> {
        Client::open(self.path.clone(), "en".into()).unwrap()
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.directory).unwrap();
    }
}

struct Conversation {
    space: String,
    me: String,
    peer: String,
    outsider: String,
    text: TimelineEntry,
    voice: TimelineEntry,
}

// Apply an actual signed, relay-sequenced membership change to canonical history.

type ReceiptRow = (String, String, String, String);
type MemoryReceipts = BTreeMap<String, BTreeMap<String, (u64, u64)>>;

#[derive(Debug, PartialEq)]
struct ReceiptSnapshot {
    rows: Vec<ReceiptRow>,
    memory: MemoryReceipts,
    meta: Vec<(String, String)>,
}

#[test]
fn own_text_and_voice_keep_delivered_then_read_after_offline_cold_reopens() {
    watermarks_cases::own_text_and_voice_keep_delivered_then_read_after_offline_cold_reopens()
        .unwrap()
}

#[test]
fn independent_watermark_maxima_are_exact_u64_and_stale_replay_cannot_lower_them() {
    watermarks_cases::independent_watermark_maxima_are_exact_u64_and_stale_replay_cannot_lower_them(
    )
    .unwrap()
}

#[test]
fn unknown_foreign_removed_and_inaccessible_receipts_never_write() {
    membership_cases::unknown_foreign_removed_and_inaccessible_receipts_never_write().unwrap()
}

#[test]
fn delivery_aggregates_only_current_members_and_read_implies_delivered() {
    watermarks_cases::delivery_aggregates_only_current_members_and_read_implies_delivered().unwrap()
}

#[test]
fn receipt_history_does_not_override_pending_or_permanently_failed_sends() {
    watermarks_cases::receipt_history_does_not_override_pending_or_permanently_failed_sends()
        .unwrap()
}

#[test]
fn sql_insert_and_update_failures_leave_disk_and_published_ticks_unchanged_then_retry() {
    storage_cases::sql_insert_and_update_failures_leave_disk_and_published_ticks_unchanged_then_retry().unwrap()
}

#[test]
fn reload_preserves_watermarks_and_wipe_or_erase_device_clears_only_its_database() {
    storage_cases::reload_preserves_watermarks_and_wipe_or_erase_device_clears_only_its_database()
        .unwrap()
}

#[test]
fn device_peer_receipts_are_excluded_from_backup_without_changing_the_live_store() {
    storage_cases::device_peer_receipts_are_excluded_from_backup_without_changing_the_live_store()
        .unwrap()
}

#[test]
fn early_receipts_survive_sync_boundaries_until_membership_and_foreign_pairs_stay_bounded() {
    membership_cases::early_receipts_survive_sync_boundaries_until_membership_and_foreign_pairs_stay_bounded().unwrap()
}

#[test]
fn legacy_sample_receipts_migrate_only_canonical_peers_and_failure_keeps_the_blob() {
    storage_cases::legacy_sample_receipts_migrate_only_canonical_peers_and_failure_keeps_the_blob()
        .unwrap()
}

#[test]
fn own_identity_read_storage_failure_propagates_through_receive_and_retry_is_durable() {
    storage_cases::own_identity_read_storage_failure_propagates_through_receive_and_retry_is_durable(
    ).unwrap()
}

#[test]
fn pending_membership_cannot_admit_a_new_peer_or_suppress_a_confirmed_peers_receipt() {
    membership_cases::pending_membership_cannot_admit_a_new_peer_or_suppress_a_confirmed_peers_receipt().unwrap()
}

#[test]
fn confirmed_removal_discards_new_and_deferred_marks_so_rejoin_cannot_authorize_them() {
    membership_cases::confirmed_removal_discards_new_and_deferred_marks_so_rejoin_cannot_authorize_them().unwrap()
}

// Complete cases keep their original annotated entrypoints above.
mod membership_cases;
mod storage_cases;
mod support;
mod watermarks_cases;
use support::*;
