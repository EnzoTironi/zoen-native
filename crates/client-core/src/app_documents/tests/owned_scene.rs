//! Whole owned scene cases on the same original parent fixture.

use super::*;

pub(super) fn install_and_save_have_one_app_item_compact_cards_and_owned_encrypted_scene() {
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

pub(super) fn disk_reopen_reverifies_scene_receipts_and_preserves_exact_unknown_fields_and_images()
{
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
    let engine = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
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

pub(super) fn invalid_scenes_and_stale_bases_never_write_or_silently_drop_embedded_assets() {
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

pub(super) fn additive_owned_document_field_does_not_change_ordinary_app_signed_json() {
    let text = r#"{"app":"pet","resource_uri":"ui://builtin/pet","title":"Pet","state_json":"{}"}"#;
    let app: AppDoc = serde_json::from_str(text).unwrap();
    assert!(app.document.is_none());
    assert_eq!(serde_json::to_string(&app).unwrap(), text);
}

pub(super) async fn only_document_write_commands_accept_large_scene_envelopes_and_preserve_exact_bytes(
) {
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
