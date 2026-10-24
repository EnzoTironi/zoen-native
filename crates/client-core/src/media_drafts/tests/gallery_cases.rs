use super::*;
use serde_json::{json, Value};

fn selection() -> Value {
    json!({"kind":"attachments","reply_to":null,
      "items":[{"asset_id":"base","caption":"Original","mutation_id":"send-base"},{"asset_id":"photo","caption":"Cachoeira 🌊","mutation_id":"send-photo"}],
      "gallery":{"entries":[{"asset_id":"photo","photo_id":"ph://private-library-id"}],"forced_caption":false}})
}
fn photo() -> MediaDraftAssetInput {
    let mut asset = input("photo");
    asset.name = "waterfall.jpg".into();
    asset.mime = "image/jpeg".into();
    asset
}

#[test]
fn gallery_selection_and_caption_cold_reopen_encrypted_without_publishing_and_cancel_keeps_baseline(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = temp()?;
    let path = dir.join("gallery.sqlite");
    let f = fixture(path.to_str().ok_or("temporary path is not UTF-8")?);
    let baseline = save(&f, "0", &attachments(&["base"], None), vec![input("base")]);
    let counts_before = counts(&f.engine);
    let value = selection();
    let draft = save(&f, &baseline.revision, &value.to_string(), vec![photo()]);
    assert_eq!(counts(&f.engine), counts_before);
    assert!(!draft.attempted);
    assert!(rows(&f).iter().all(|(_, bytes)| bytes.starts_with("v1:")
        && !bytes.contains("private-library-id")
        && !bytes.contains("Cachoeira")));
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let engine = Client::open(
        path.to_str().ok_or("temporary path is not UTF-8")?.into(),
        "pt-BR".into(),
    )?;
    assert!(engine.media_draft(scope.clone()).is_err());
    assert!(engine.unlock(vault.clone())?);
    assert_eq!(engine.media_draft(scope.clone())?, draft);
    assert_eq!(
        engine.media_draft_asset(scope.clone(), draft.revision.clone(), "photo".into())?,
        photo().bytes
    );
    let mut foreign = scope.clone();
    foreign.expected_device_id = "another-device".into();
    assert!(engine.media_draft(foreign).is_err());
    let cancelled = engine
        .save_media_draft(
            scope.clone(),
            draft.revision,
            baseline.content_json.clone(),
            vec![],
        )?
        .draft;
    assert_eq!(cancelled.content_json, baseline.content_json);
    assert_eq!(cancelled.assets, baseline.assets);
    assert_eq!(counts(&engine), counts_before);
    drop(engine);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn gallery_references_are_bounded_unique_suffixes_and_reject_paths_unknown_fields_or_nonmedia_without_writes(
) {
    let f = fixture(":memory:");
    let baseline = save(&f, "0", &attachments(&["base"], None), vec![input("base")]);
    let before = rows(&f);
    let public = counts(&f.engine);
    let mut cases = Vec::new();
    let mut wrong = selection();
    wrong["gallery"]["entries"][0]["asset_id"] = json!("base");
    cases.push(wrong);
    let mut path = selection();
    path["gallery"]["entries"][0]["photo_id"] = json!("file:///private-copy");
    cases.push(path);
    let mut long = selection();
    long["gallery"]["entries"][0]["photo_id"] = json!(format!("ph://{}", "a".repeat(2048)));
    cases.push(long);
    let mut control = selection();
    control["gallery"]["entries"][0]["photo_id"] = json!("ph://bad\u{0000}");
    cases.push(control);
    let mut unknown = selection();
    unknown["gallery"]["entries"][0]["path"] = json!("secret");
    cases.push(unknown);
    let mut empty = selection();
    empty["gallery"]["entries"] = json!([]);
    cases.push(empty);
    let mut duplicate = selection();
    duplicate["gallery"]["entries"] = json!([{"asset_id":"base","photo_id":"ph://same"},{"asset_id":"photo","photo_id":"ph://same"}]);
    cases.push(duplicate);
    for value in cases {
        assert!(f
            .engine
            .save_media_draft(
                f.scope.clone(),
                baseline.revision.clone(),
                value.to_string(),
                vec![photo()]
            )
            .is_err());
        assert_eq!(rows(&f), before);
        assert_eq!(counts(&f.engine), public);
    }
    let mut document = photo();
    document.mime = "application/pdf".into();
    assert!(f
        .engine
        .save_media_draft(
            f.scope.clone(),
            baseline.revision,
            selection().to_string(),
            vec![document]
        )
        .is_err());
    assert_eq!(rows(&f), before);
    assert_eq!(counts(&f.engine), public);
}
