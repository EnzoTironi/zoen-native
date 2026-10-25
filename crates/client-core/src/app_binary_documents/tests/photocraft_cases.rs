//! Actual upstream native raster bytes through the existing encrypted binary Item authority.
use super::*;
use miniapps::photocraft;
const PHOTO: &[u8] = photocraft::EMPTY;
const EDITED: &[u8] =
    include_bytes!("../../../../miniapps/src/photocraft/fixtures/native-rotated.pcraft");
fn photo(f: &Fixture, id: &str) -> R<AppBinaryDocumentDto> {
    f.client.app_binary_create_for(
        photocraft::APP.into(),
        f.space.clone(),
        "Document".into(),
        STANDARD.encode(PHOTO),
        id.into(),
    )
}
#[test]
fn real_pcraft_creation_reuses_receipts_but_keeps_format_mime_scope_and_encrypted_bytes_distinct(
) -> R<()> {
    let f = fixture(":memory:");
    let doc = photo(&f, "shared-intent")?;
    let before = counts(&f);
    assert_eq!(photo(&f, "shared-intent")?, doc);
    assert_eq!(counts(&f), before);
    let deck = create(&f, "shared-intent");
    assert_ne!(deck.item_id, doc.item_id);
    assert!(doc.item_id.starts_with("it_photo_"));
    assert_eq!(doc.kind, photocraft::APP);
    assert_eq!(doc.engine_revision, photocraft::ENGINE_REVISION);
    assert_eq!(STANDARD.decode(&doc.data_base64).expect("base64"), PHOTO);
    let e = f.client.lock();
    let ItemContent::App(app) = &e.item_state(&doc.item_id)?.current().content else {
        panic!("App")
    };
    assert_eq!(app.resource_uri, photocraft::RESOURCE);
    let file = app.document.as_ref().expect("FileDoc");
    assert_eq!(file.mime, photocraft::MIME);
    assert_eq!(file.name, "image.pcraft");
    assert!(!file.chunks.is_empty());
    assert!(file
        .chunks
        .iter()
        .all(|chunk| chunk.blob != chunk.sha256 && !chunk.key.is_empty()));
    assert!(!app.state_json.contains("data_base64"));
    assert!(!app.state_json.contains("unsupported private data"));
    assert_eq!(
        e.file_bytes(&doc.item_id, Some(1))?.expect("owned bytes"),
        PHOTO
    );
    drop(e);
    let before = counts(&f);
    assert!(save(&f, &doc, ORIGINAL, "wrong-format").is_err());
    assert!(save(&f, &deck, PHOTO, "wrong-format").is_err());
    assert_eq!(counts(&f), before);
    assert_eq!(
        miniapps::binary::Format::Photocraft.available(),
        option_env!("ZOEN_PHOTOCRAFT_RESOURCE_MANIFEST_JSON").is_some()
    );
    assert_eq!(
        miniapps::spec(photocraft::APP).is_some(),
        miniapps::binary::Format::Photocraft.available()
    );
    Ok(())
}
#[test]
fn pcraft_lost_ack_cas_cold_history_and_export_keep_exact_original_selected_file() -> R<()> {
    let path =
        std::env::temp_dir().join(format!("zoen-photo-cas-{}.sqlite", types::new_id("test")?));
    let f = fixture(path.to_str().expect("fixture path"));
    let base = photo(&f, "create")?;
    let accepted = save(&f, &base, EDITED, "lost-ack")?;
    let newer = save(&f, &accepted, PHOTO, "later")?;
    // v3 intentionally restores v1's exact bytes; the foreign hash must be v2's.
    assert_eq!(newer.sha256, base.sha256);
    assert_ne!(accepted.sha256, base.sha256);
    let before = counts(&f);
    assert!(matches!(
        save(&f, &base, PHOTO, "stale"),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(counts(&f), before);
    let vault = f.vault.clone();
    drop(f);
    let client = Client::open(path.to_string_lossy().into(), "en".into())?;
    assert!(client.unlock(vault)?);
    let retry = client.app_binary_save(
        base.item_id.clone(),
        base.version,
        base.sha256.clone(),
        STANDARD.encode(EDITED),
        "lost-ack".into(),
        hash(b"test operation"),
    )?;
    assert_eq!(retry.version, 2);
    assert_eq!(retry.current_version, 3);
    assert_eq!(retry.sha256, accepted.sha256);
    assert_eq!(STANDARD.decode(&retry.data_base64).expect("base64"), EDITED);
    let old = client.app_binary_read(base.item_id.clone(), Some(1), Some(base.sha256.clone()))?;
    assert_eq!(STANDARD.decode(old.data_base64).expect("base64"), PHOTO);
    assert_eq!(old.current_version, 3);
    assert!(matches!(
        client.app_binary_read(base.item_id.clone(), Some(1), Some(accepted.sha256.clone())),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(
        client
            .app_binary_read(base.item_id.clone(), None, None)?
            .version,
        3
    );
    assert_eq!(client.lock().item_state(&base.item_id)?.versions.len(), 3);
    drop(client);
    std::fs::remove_file(path).expect("owned fixture cleanup");
    Ok(())
}
#[test]
fn pcraft_receipt_failure_rolls_back_and_revoked_trust_cannot_observe_an_ack_or_write() -> R<()> {
    let f = fixture(":memory:");
    let base = photo(&f, "create")?;
    let before = counts(&f);
    f.client.lock().store.conn().execute_batch("CREATE TRIGGER deny_photo_receipt BEFORE INSERT ON meta WHEN NEW.key LIKE 'app_document_receipts:%' BEGIN SELECT RAISE(ABORT,'synthetic photo failure'); END;").expect("fixture trigger");
    assert!(save(&f, &base, EDITED, "edit").is_err());
    assert_eq!(counts(&f), before);
    assert_eq!(
        f.client.app_binary_read(base.item_id.clone(), None, None)?,
        base
    );
    f.client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER deny_photo_receipt")
        .expect("drop trigger");
    let accepted = save(&f, &base, EDITED, "edit")?;
    {
        let mut e = f.client.lock();
        let me = e.me_id()?;
        let grant = e
            .state
            .grants
            .iter()
            .find(|g| g.grant.grantee.as_deref() == Some(format!("app:{}", base.item_id).as_str()))
            .expect("item grant")
            .grant
            .id
            .clone();
        e.append(&f.space, &me, EventBody::GrantRevoked { grant })?;
    }
    let before = counts(&f);
    assert!(save(&f, &base, EDITED, "edit").is_err());
    assert!(save(&f, &accepted, PHOTO, "new").is_err());
    assert_eq!(counts(&f), before);
    Ok(())
}
#[test]
fn configured_photocraft_person_factory_creates_one_standard_card_and_same_host_file() -> R<()> {
    let f = fixture(":memory:");
    let before = counts(&f);
    let args = json!({"title":"Shared image","mutationId":"person-photo-create"}).to_string();
    if !miniapps::binary::Format::Photocraft.available() {
        assert!(f
            .client
            .install_app(f.space.clone(), photocraft::APP.into(), args)
            .is_err());
        assert_eq!(counts(&f), before);
        return Ok(());
    }
    let first = f
        .client
        .install_app(f.space.clone(), photocraft::APP.into(), args.clone())?;
    let repeated = f
        .client
        .install_app(f.space.clone(), photocraft::APP.into(), args)?;
    assert_eq!(first.id, repeated.id);
    let base = f.client.app_binary_read(first.id.clone(), None, None)?;
    assert_eq!(base.actor_kind, "human");
    assert_eq!(STANDARD.decode(&base.data_base64).expect("base64"), PHOTO);
    let edited = save(&f, &base, EDITED, "person-host-edit")?;
    assert_eq!(edited.item_id, first.id);
    assert_eq!(edited.version, 2);
    assert_eq!(
        STANDARD.decode(&edited.data_base64).expect("base64"),
        EDITED
    );
    let cards=f.client.timeline(f.space.clone())?.into_iter().filter(|entry|matches!(&entry.kind,crate::EntryKind::Message{card:Some(card),..} if card.item_id==first.id)).count();
    assert_eq!(cards, 1);
    assert_eq!(
        f.client
            .app_binary_read(first.id, Some(1), Some(base.sha256))?
            .data_base64,
        base.data_base64
    );
    Ok(())
}
