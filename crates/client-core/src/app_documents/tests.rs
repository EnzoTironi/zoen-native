use super::*;
use crate::{EntryKind, PrivacyDto, SecretVault};
use base64::Engine as _;
use std::sync::{Arc, Mutex};
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
    space: String,
    item: String,
}
fn fixture(path: &str) -> Fixture {
    let engine = Client::open(path.into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    engine
        .create_account(
            "Drawing QA".into(),
            "drawing_qa".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )
        .unwrap();
    let space = engine
        .create_group_with("Drawing".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let item = engine
        .install_app(
            space.clone(),
            "excalidraw".into(),
            r#"{"title":"Canvas"}"#.into(),
        )
        .unwrap()
        .id;
    Fixture {
        engine,
        vault,
        space,
        item,
    }
}
fn scene(label: &str) -> String {
    json!({"type":"excalidraw","version":2,"elements":[{"id":"text_1","type":"text","x":10,"y":20,"width":100,"height":24,"text":label,"isDeleted":false}],"appState":{"viewBackgroundColor":"#ffffff"},"files":{},"futurePassive":{"preserved":[1,"value"]}}).to_string()
}
fn save(f: &Fixture, base: &AppDocumentDto, text: &str, id: &str) -> R<AppDocumentDto> {
    f.engine.app_document_save(
        f.item.clone(),
        base.version,
        base.sha256.clone(),
        text.into(),
        id.into(),
    )
}
fn counts(engine: &Client) -> Vec<i64> {
    let e = engine.lock();
    [
        "events",
        "media",
        "media_keys",
        "blob_uploads",
        "outbox",
        "meta",
    ]
    .iter()
    .map(|t| {
        e.store
            .conn()
            .query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
            .unwrap()
    })
    .collect()
}
fn role(f: &Fixture, role: Role) {
    let mut e = f.engine.lock();
    let me = e.me_id().unwrap();
    e.append(
        &f.space,
        &me,
        EventBody::MemberAdded {
            identity: me.clone(),
            role,
        },
    )
    .unwrap();
}

#[test]
fn install_and_save_have_one_app_item_compact_cards_and_owned_encrypted_scene() {
    owned_scene::install_and_save_have_one_app_item_compact_cards_and_owned_encrypted_scene()
}

#[test]
fn saves_replay_exact_accepted_version_after_newer_edits_and_reject_reused_intents() {
    save_receipts::saves_replay_exact_accepted_version_after_newer_edits_and_reject_reused_intents()
}

#[test]
fn unchanged_scene_ack_adds_no_version_and_receipt_eviction_never_rebases_stale_work() {
    save_receipts::unchanged_scene_ack_adds_no_version_and_receipt_eviction_never_rebases_stale_work(
    )
}

#[test]
fn disk_reopen_reverifies_scene_receipts_and_preserves_exact_unknown_fields_and_images() {
    owned_scene::disk_reopen_reverifies_scene_receipts_and_preserves_exact_unknown_fields_and_images(
    )
}

#[test]
fn invalid_scenes_and_stale_bases_never_write_or_silently_drop_embedded_assets() {
    owned_scene::invalid_scenes_and_stale_bases_never_write_or_silently_drop_embedded_assets()
}

#[test]
fn receipt_storage_failure_rolls_back_chunks_event_key_outbox_and_retry_is_one_version() {
    save_receipts::receipt_storage_failure_rolls_back_chunks_event_key_outbox_and_retry_is_one_version()
}

#[test]
fn copy_is_one_atomic_new_item_with_original_untouched_and_exact_lost_ack_receipt() {
    owned_copies::copy_is_one_atomic_new_item_with_original_untouched_and_exact_lost_ack_receipt()
}

#[test]
fn failed_copy_cannot_publish_a_blank_or_ungranted_item() {
    owned_copies::failed_copy_cannot_publish_a_blank_or_ungranted_item()
}

#[test]
fn app_forwarding_owns_a_fresh_manifest_and_preserves_current_full_scene_without_source_refs() {
    owned_copies::app_forwarding_owns_a_fresh_manifest_and_preserves_current_full_scene_without_source_refs()
}

#[test]
fn corrupt_cached_bytes_fail_closed_and_revoked_device_cannot_read() {
    admission::corrupt_cached_bytes_fail_closed_and_revoked_device_cannot_read()
}

#[test]
fn revoked_item_grant_prevents_new_save_without_preventing_read_or_accepted_retry() {
    admission::revoked_item_grant_prevents_new_save_without_preventing_read_or_accepted_retry()
}

#[test]
fn sync_discovers_owned_app_chunks_and_explicit_history_retains_original_versions() {
    admission::sync_discovers_owned_app_chunks_and_explicit_history_retains_original_versions()
}

#[test]
fn additive_owned_document_field_does_not_change_ordinary_app_signed_json() {
    owned_scene::additive_owned_document_field_does_not_change_ordinary_app_signed_json()
}

#[tokio::test]
async fn only_document_write_commands_accept_large_scene_envelopes_and_preserve_exact_bytes() {
    owned_scene::only_document_write_commands_accept_large_scene_envelopes_and_preserve_exact_bytes(
    )
    .await
}

#[test]
fn lost_membership_denies_read_copy_and_save_even_when_all_scene_chunks_are_cached() {
    admission::lost_membership_denies_read_copy_and_save_even_when_all_scene_chunks_are_cached()
}

#[test]
fn evicted_copy_receipt_cannot_create_a_second_item_for_an_old_intent() {
    owned_copies::evicted_copy_receipt_cannot_create_a_second_item_for_an_old_intent()
}

mod admission;
mod owned_copies;
mod owned_scene;
mod save_receipts;
