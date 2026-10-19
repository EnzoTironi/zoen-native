use super::*;
use crate::{EntryKind, PrivacyDto, SecretVault};
use base64::Engine as _;
use roda_types::{EventBody, Role};
use std::sync::{Arc, Mutex};

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
    engine: Arc<RodaEngine>,
    vault: Arc<Vault>,
    space: String,
    item: String,
}
fn fixture(path: &str) -> Fixture {
    let engine = RodaEngine::open(path.into(), "en".into()).unwrap();
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
fn counts(engine: &RodaEngine) -> Vec<i64> {
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
    let f = fixture(":memory:");
    let initial = f.engine.app_document_read(f.item.clone()).unwrap();
    assert_eq!(initial.version, 1);
    assert_eq!(initial.scene_json, EMPTY);
    let text = scene("Shared drawing");
    let saved = save(&f, &initial, &text, "save1").unwrap();
    assert_eq!(saved.version, 2);
    assert_eq!(saved.scene_json, text);
    assert_eq!(saved.sha256, hash(text.as_bytes()));
    assert_eq!(
        f.engine
            .items()
            .iter()
            .filter(|i| i.space_id == f.space)
            .count(),
        1
    );
    let item = f.engine.item(f.item.clone()).unwrap();
    assert!(item.file.is_none());
    let app = item.app.unwrap();
    assert!(!app.view_json.contains("Shared drawing"));
    assert!(!app.view_json.contains("chunks"));
    let snapshot: Value = serde_json::from_str(&app.snapshot_json).unwrap();
    assert_eq!(snapshot["title"], "Canvas");
    assert!(snapshot["detail"].as_str().unwrap().contains('1'));
    let e = f.engine.lock();
    let state = e.item_state(&f.item).unwrap();
    assert_eq!(state.versions.len(), 2);
    let file = state.current().content.owned_file().unwrap();
    assert_eq!(file.bytes, text.len() as u64);
    assert!(!file.chunks.is_empty());
    let chunk = &file.chunks[0];
    assert_ne!(chunk.blob, chunk.sha256);
    assert_eq!(
        e.owned_file_bytes(
            state.versions[0].content.owned_file().unwrap(),
            MAX_SCENE_BYTES as u64
        )
        .unwrap()
        .unwrap(),
        EMPTY.as_bytes()
    );
    assert!(e.verify_log(&f.space).valid);
}

#[test]
fn saves_replay_exact_accepted_version_after_newer_edits_and_reject_reused_intents() {
    let f = fixture(":memory:");
    let initial = f.engine.app_document_read(f.item.clone()).unwrap();
    let one = scene("First");
    let two = scene("Second");
    let accepted = save(&f, &initial, &one, "save1").unwrap();
    let newer = save(&f, &accepted, &two, "save2").unwrap();
    role(&f, Role::Reader);
    let before = counts(&f.engine);
    let replay = save(&f, &initial, &one, "save1").unwrap();
    assert_eq!(replay.version, accepted.version);
    assert_eq!(replay.current_version, newer.version);
    assert_eq!(replay.scene_json, one);
    assert!(save(&f, &initial, &two, "save1").is_err());
    assert!(save(&f, &newer, &scene("Denied"), "save3").is_err());
    assert_eq!(counts(&f.engine), before);
    assert_eq!(f.engine.app_document_read(f.item.clone()).unwrap(), newer);
}

#[test]
fn unchanged_scene_ack_adds_no_version_and_receipt_eviction_never_rebases_stale_work() {
    let f = fixture(":memory:");
    let initial = f.engine.app_document_read(f.item.clone()).unwrap();
    let first = save(&f, &initial, &scene("Changed"), "first").unwrap();
    for n in 0..MAX_RECEIPTS {
        let accepted = save(&f, &first, &first.scene_json, &format!("noop{n}")).unwrap();
        assert_eq!(accepted.version, 2);
    }
    assert_eq!(
        f.engine.lock().item_state(&f.item).unwrap().versions.len(),
        2
    );
    assert!(matches!(
        save(&f, &initial, &first.scene_json, "first"),
        Err(CoreError::Stale { .. })
    ));
}

#[test]
fn disk_reopen_reverifies_scene_receipts_and_preserves_exact_unknown_fields_and_images() {
    let dir = std::env::temp_dir().join(new_id("app-document-test"));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("device.sqlite");
    let f = fixture(path.to_str().unwrap());
    let initial = f.engine.app_document_read(f.item.clone()).unwrap();
    let mut value: Value = serde_json::from_str(&scene("Image")).unwrap();
    value["elements"].as_array_mut().unwrap().push(
        json!({"id":"image_1","type":"image","x":0,"y":0,"width":2,"height":2,"fileId":"img"}),
    );
    let png = base64::engine::general_purpose::STANDARD
        .encode(b"\x89PNG\r\n\x1a\nsynthetic signed bytes");
    value["files"] = json!({"img":{"id":"img","mimeType":"image/png","dataURL":format!("data:image/png;base64,{png}"),"created":123}});
    let text = format!(" {} \n", value);
    let saved = save(&f, &initial, &text, "persisted").unwrap();
    let item = f.item.clone();
    let vault = f.vault.clone();
    drop(f);
    let engine = RodaEngine::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(engine.unlock(vault).unwrap());
    assert_eq!(engine.app_document_read(item.clone()).unwrap(), saved);
    assert_eq!(
        engine
            .app_document_save(
                item,
                initial.version,
                initial.sha256,
                text,
                "persisted".into()
            )
            .unwrap(),
        saved
    );
    drop(engine);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn invalid_scenes_and_stale_bases_never_write_or_silently_drop_embedded_assets() {
    let f = fixture(":memory:");
    let base = f.engine.app_document_read(f.item.clone()).unwrap();
    let before = counts(&f.engine);
    let mut bad: Value = serde_json::from_str(&scene("invalid")).unwrap();
    for kind in ["iframe", "embeddable", "unknown"] {
        bad["elements"][0]["type"] = json!(kind);
        assert!(save(&f, &base, &bad.to_string(), "bad").is_err());
    }
    bad = serde_json::from_str(&scene("missing image")).unwrap();
    bad["elements"][0]["type"] = json!("image");
    bad["elements"][0]["fileId"] = json!("remote");
    assert!(save(&f, &base, &bad.to_string(), "bad").is_err());
    bad = serde_json::from_str(&scene("bad binding")).unwrap();
    bad["elements"][0]["containerId"] = json!("foreign");
    assert!(save(&f, &base, &bad.to_string(), "bad").is_err());
    bad = serde_json::from_str(&scene("remote file")).unwrap();
    bad["files"] = json!({"remote":{"id":"remote","mimeType":"image/png","dataURL":"https://example.com/private.png"}});
    assert!(save(&f, &base, &bad.to_string(), "bad").is_err());
    assert_eq!(counts(&f.engine), before);
    save(&f, &base, &scene("Accepted"), "valid").unwrap();
    let before = counts(&f.engine);
    assert!(matches!(
        save(&f, &base, &scene("Late"), "late"),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(counts(&f.engine), before);
}

#[test]
fn receipt_storage_failure_rolls_back_chunks_event_key_outbox_and_retry_is_one_version() {
    let f = fixture(":memory:");
    let base = f.engine.app_document_read(f.item.clone()).unwrap();
    let before = counts(&f.engine);
    f.engine.lock().store.conn().execute_batch("CREATE TRIGGER refuse_drawing_receipt BEFORE INSERT ON meta WHEN NEW.key LIKE 'app_document_receipts:%' BEGIN SELECT RAISE(ABORT,'synthetic receipt failure'); END;").unwrap();
    assert!(save(&f, &base, &scene("Retry"), "retry").is_err());
    assert_eq!(counts(&f.engine), before);
    assert_eq!(f.engine.app_document_read(f.item.clone()).unwrap(), base);
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER refuse_drawing_receipt")
        .unwrap();
    assert_eq!(
        save(&f, &base, &scene("Retry"), "retry").unwrap().version,
        2
    );
    assert_eq!(
        save(&f, &base, &scene("Retry"), "retry").unwrap().version,
        2
    );
}

#[test]
fn copy_is_one_atomic_new_item_with_original_untouched_and_exact_lost_ack_receipt() {
    let f = fixture(":memory:");
    let base = f.engine.app_document_read(f.item.clone()).unwrap();
    let text = scene("Recovered conflict");
    let copied = f
        .engine
        .app_document_copy(f.item.clone(), text.clone(), "copy1".into())
        .unwrap();
    assert_ne!(copied.item_id, f.item);
    assert_eq!(copied.version, 1);
    let newer = f
        .engine
        .app_document_save(
            copied.item_id.clone(),
            1,
            copied.sha256.clone(),
            scene("Later"),
            "later".into(),
        )
        .unwrap();
    let before = counts(&f.engine);
    let replay = f
        .engine
        .app_document_copy(f.item.clone(), text, "copy1".into())
        .unwrap();
    assert_eq!(replay.item_id, copied.item_id);
    assert_eq!(replay.version, 1);
    assert_eq!(replay.current_version, newer.version);
    assert!(f
        .engine
        .app_document_copy(f.item.clone(), scene("Changed"), "copy1".into())
        .is_err());
    assert_eq!(counts(&f.engine), before);
    assert_eq!(f.engine.app_document_read(f.item.clone()).unwrap(), base);
    assert_eq!(
        f.engine
            .items()
            .iter()
            .filter(|i| i.space_id == f.space)
            .count(),
        2
    );
    let e = f.engine.lock();
    assert_ne!(
        e.store.meta(&format!("file_key:{}", f.item)).unwrap(),
        e.store
            .meta(&format!("file_key:{}", copied.item_id))
            .unwrap()
    );
}

#[test]
fn failed_copy_cannot_publish_a_blank_or_ungranted_item() {
    let f = fixture(":memory:");
    let before = counts(&f.engine);
    f.engine.lock().store.conn().execute_batch("CREATE TRIGGER refuse_drawing_copy BEFORE INSERT ON meta WHEN NEW.key LIKE 'app_document_copy_receipts:%' BEGIN SELECT RAISE(ABORT,'synthetic copy failure'); END;").unwrap();
    assert!(f
        .engine
        .app_document_copy(f.item.clone(), scene("Copy"), "copy".into())
        .is_err());
    assert_eq!(counts(&f.engine), before);
    assert_eq!(
        f.engine
            .items()
            .iter()
            .filter(|i| i.space_id == f.space)
            .count(),
        1
    );
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER refuse_drawing_copy")
        .unwrap();
    assert_eq!(
        f.engine
            .app_document_copy(f.item.clone(), scene("Copy"), "copy".into())
            .unwrap()
            .version,
        1
    );
}

#[test]
fn app_forwarding_owns_a_fresh_manifest_and_preserves_current_full_scene_without_source_refs() {
    let f = fixture(":memory:");
    let base = f.engine.app_document_read(f.item.clone()).unwrap();
    let saved = save(&f, &base, &scene("Forwarded"), "save").unwrap();
    let target = f
        .engine
        .create_group_with("Target".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let source = f
        .engine
        .timeline(f.space.clone())
        .unwrap()
        .into_iter()
        .find(|m| matches!(&m.kind,EntryKind::Message{card:Some(card),..} if card.item_id==f.item))
        .unwrap();
    let forwarded = f
        .engine
        .forward_message(f.space.clone(), source.id, target.clone())
        .unwrap();
    assert!(forwarded.forwarded);
    assert!(forwarded.reply_to.is_none());
    assert!(forwarded.in_thread.is_none());
    let EntryKind::Message {
        card: Some(card), ..
    } = forwarded.kind
    else {
        panic!("forwarded app card");
    };
    let item = card.item_id;
    assert_ne!(item, f.item);
    let read = f.engine.app_document_read(item.clone()).unwrap();
    assert_eq!(read.scene_json, saved.scene_json);
    assert_eq!(read.version, 1);
    let e = f.engine.lock();
    assert_eq!(e.item_state(&item).unwrap().space, target);
    assert_ne!(
        e.item_state(&item)
            .unwrap()
            .current()
            .content
            .owned_file()
            .unwrap()
            .chunks,
        e.item_state(&f.item)
            .unwrap()
            .current()
            .content
            .owned_file()
            .unwrap()
            .chunks
    );
}

#[test]
fn corrupt_cached_bytes_fail_closed_and_revoked_device_cannot_read() {
    let f = fixture(":memory:");
    let initial = f.engine.app_document_read(f.item.clone()).unwrap();
    let sha = f
        .engine
        .lock()
        .item_state(&f.item)
        .unwrap()
        .current()
        .content
        .owned_file()
        .unwrap()
        .chunks[0]
        .sha256
        .clone();
    f.engine
        .lock()
        .store
        .conn()
        .execute(
            "UPDATE media SET bytes=?1 WHERE sha256=?2",
            rusqlite::params![b"corrupt".as_slice(), sha],
        )
        .unwrap();
    assert!(f.engine.app_document_read(f.item.clone()).is_err());
    let device = f.engine.account().unwrap().device_id;
    f.engine
        .lock()
        .store
        .set_meta("devices.revoked", &json!([device]).to_string())
        .unwrap();
    assert!(f.engine.app_document_read(f.item.clone()).is_err());
    assert!(save(&f, &initial, &scene("Denied"), "deny").is_err());
}

#[test]
fn revoked_item_grant_prevents_new_save_without_preventing_read_or_accepted_retry() {
    let f = fixture(":memory:");
    let base = f.engine.app_document_read(f.item.clone()).unwrap();
    let accepted = save(&f, &base, &scene("Accepted"), "accepted").unwrap();
    {
        let mut e = f.engine.lock();
        let me = e.me_id().unwrap();
        let grant = e
            .state
            .grants
            .iter()
            .find(|g| g.grant.grantee.as_deref() == Some(format!("app:{}", f.item).as_str()))
            .unwrap()
            .grant
            .id
            .clone();
        e.append(&f.space, &me, EventBody::GrantRevoked { grant })
            .unwrap();
    }
    let before = counts(&f.engine);
    assert!(save(&f, &accepted, &scene("Denied"), "denied").is_err());
    assert_eq!(
        save(&f, &base, &accepted.scene_json, "accepted").unwrap(),
        accepted
    );
    assert_eq!(
        f.engine.app_document_read(f.item.clone()).unwrap(),
        accepted
    );
    assert_eq!(counts(&f.engine), before);
}

#[test]
fn sync_discovers_owned_app_chunks_and_explicit_history_retains_original_versions() {
    let f = fixture(":memory:");
    let base = f.engine.app_document_read(f.item.clone()).unwrap();
    let accepted = save(&f, &base, &scene("Current"), "current").unwrap();
    let mut e = f.engine.lock();
    e.net.synced.insert(f.space.clone());
    e.store.conn().execute_batch("DELETE FROM media").unwrap();
    assert!(e.app_document_read(&f.item).is_err());
    let wanted = e.wanted_item_blobs();
    let current = e
        .item_state(&f.item)
        .unwrap()
        .current()
        .content
        .owned_file()
        .unwrap();
    assert!(current
        .chunks
        .iter()
        .all(|c| wanted.iter().any(|w| w.blob == c.blob
            && w.space == f.space
            && w.maximum_sealed_bytes == Some(c.bytes + 40))));
    let key = e
        .file_demand_key(&f.item, 1, MAX_SCENE_BYTES as u64)
        .unwrap();
    let _lease = e
        .net
        .file_demands
        .acquire(
            key,
            MAX_SCENE_BYTES as u64,
            std::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .unwrap();
    let wanted = e.wanted_item_blobs();
    let original = e.item_state(&f.item).unwrap().versions[0]
        .content
        .owned_file()
        .unwrap();
    assert!(original
        .chunks
        .iter()
        .all(|c| wanted.iter().any(|w| w.blob == c.blob)));
    assert_eq!(
        e.item_state(&f.item).unwrap().current().number,
        accepted.version
    );
}

#[test]
fn additive_owned_document_field_does_not_change_ordinary_app_signed_json() {
    let text = r#"{"app":"pet","resource_uri":"ui://roda/pet","title":"Pet","state_json":"{}"}"#;
    let app: AppDoc = serde_json::from_str(text).unwrap();
    assert!(app.document.is_none());
    assert_eq!(serde_json::to_string(&app).unwrap(), text);
}

#[tokio::test]
async fn only_document_write_commands_accept_large_scene_envelopes_and_preserve_exact_bytes() {
    let f = fixture(":memory:");
    let base = f.engine.app_document_read(f.item.clone()).unwrap();
    let mut value: Value = serde_json::from_str(&scene("Large scene")).unwrap();
    value["futurePassive"]["padding"] =
        json!("x".repeat(crate::client_api::MAX_REQUEST_BYTES + 16 * 1024));
    let text = value.to_string();
    let request=json!({"op":"app_document_save","item_id":f.item,"expected_version":base.version,"expected_sha256":base.sha256,"scene_json":text,"mutation_id":"large"}).to_string();
    let saved: AppDocumentDto =
        serde_json::from_str(&f.engine.client_request(request).await.unwrap()).unwrap();
    assert_eq!(saved.scene_json, text);
    assert_eq!(saved.version, 2);
    let before = counts(&f.engine);
    assert!(f.engine.client_request(json!({"op":"send_message","space_id":f.space,"text":"x".repeat(crate::client_api::MAX_REQUEST_BYTES+16*1024)}).to_string()).await.is_err());
    assert_eq!(counts(&f.engine), before);
    assert!(save(&f, &saved, &" ".repeat(MAX_SCENE_BYTES + 1), "oversized").is_err());
    assert_eq!(counts(&f.engine), before);
}

#[test]
fn lost_membership_denies_read_copy_and_save_even_when_all_scene_chunks_are_cached() {
    let f = fixture(":memory:");
    let base = f.engine.app_document_read(f.item.clone()).unwrap();
    {
        let mut e = f.engine.lock();
        let me = e.me_id().unwrap();
        e.append(
            &f.space,
            &me,
            EventBody::MemberRemoved {
                identity: me.clone(),
            },
        )
        .unwrap();
    }
    {
        let mut e = f.engine.lock();
        e.net.synced.insert(f.space.clone());
        e.store.conn().execute_batch("DELETE FROM media").unwrap();
        assert!(e
            .wanted_item_blobs()
            .iter()
            .all(|wanted| wanted.space != f.space));
    }
    let before = counts(&f.engine);
    assert!(f.engine.file_bytes(f.item.clone(), Some(1)).is_err());
    assert!(f.engine.file_info_at(f.item.clone(), Some(1)).is_err());
    assert!(f.engine.app_document_read(f.item.clone()).is_err());
    assert!(save(&f, &base, &scene("No access"), "denied").is_err());
    assert!(f
        .engine
        .app_document_copy(f.item.clone(), scene("No access"), "copy".into())
        .is_err());
    assert_eq!(counts(&f.engine), before);
}

#[test]
fn evicted_copy_receipt_cannot_create_a_second_item_for_an_old_intent() {
    let f = fixture(":memory:");
    let text = scene("Copy once");
    let first = f
        .engine
        .app_document_copy(f.item.clone(), text.clone(), "first".into())
        .unwrap();
    for n in 0..MAX_RECEIPTS {
        f.engine
            .app_document_copy(f.item.clone(), text.clone(), format!("copy{n}"))
            .unwrap();
    }
    let before = counts(&f.engine);
    assert!(matches!(
        f.engine
            .app_document_copy(f.item.clone(), text, "first".into()),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(
        f.engine.app_document_read(first.item_id).unwrap().version,
        1
    );
    assert_eq!(counts(&f.engine), before);
}
