use super::*;
use miniapps::gridcraft;
const GRID: &[u8] = include_bytes!("../fixtures/managed-grid-base.xlsx");
const GRID_CHANGED: &[u8] = include_bytes!("../fixtures/managed-grid-changed.xlsx");

fn grid(f: &Fixture, mutation: &str) -> AppBinaryDocumentDto {
    f.client
        .app_binary_create_for(
            gridcraft::APP.into(),
            f.space.clone(),
            "Budget".into(),
            STANDARD.encode(GRID),
            mutation.into(),
        )
        .unwrap()
}
#[test]
fn grid_and_deck_share_the_transaction_but_never_format_identity_or_creation_receipts() {
    let f = fixture(":memory:");
    let spreadsheet = grid(&f, "same_intent");
    let before = counts(&f);
    assert_eq!(grid(&f, "same_intent"), spreadsheet);
    assert_eq!(counts(&f), before);
    let presentation = create(&f, "same_intent");
    assert_ne!(spreadsheet.item_id, presentation.item_id);
    assert!(spreadsheet.item_id.starts_with("it_grid_"));
    assert!(presentation.item_id.starts_with("it_deck_"));
    assert_eq!(spreadsheet.kind, gridcraft::APP);
    assert_eq!(spreadsheet.engine_revision, gridcraft::ENGINE_REVISION);
    assert_eq!(spreadsheet.data_base64, STANDARD.encode(GRID));
    let engine = f.client.lock();
    let ItemContent::App(doc) = &engine
        .item_state(&spreadsheet.item_id)
        .unwrap()
        .current()
        .content
    else {
        panic!("App")
    };
    assert_eq!(doc.resource_uri, gridcraft::RESOURCE);
    assert_eq!(doc.document.as_ref().unwrap().mime, gridcraft::MIME);
    assert!(
        doc.document
            .as_ref()
            .unwrap()
            .chunks
            .iter()
            .all(|c| c.blob != c.sha256 && !c.key.is_empty())
    );
    assert_eq!(
        serde_json::from_str::<Value>(&doc.state_json).unwrap()["cells"],
        2
    );
}
#[test]
fn wrong_format_or_unknown_editor_cannot_write_or_change_any_receipt() {
    let f = fixture(":memory:");
    let sheet = grid(&f, "create_grid");
    let deck = create(&f, "create_deck");
    let before = counts(&f);
    assert!(save(&f, &sheet, ORIGINAL, "wrong").is_err());
    assert!(save(&f, &deck, GRID, "wrong").is_err());
    assert!(
        f.client
            .app_binary_create_for(
                "unknown".into(),
                f.space.clone(),
                "No".into(),
                STANDARD.encode(GRID),
                "unknown".into()
            )
            .is_err()
    );
    assert!(
        f.client
            .app_binary_create_for(
                gridcraft::APP.into(),
                f.space.clone(),
                "No".into(),
                STANDARD.encode(ORIGINAL),
                "wrong".into()
            )
            .is_err()
    );
    assert_eq!(counts(&f), before);
}
#[test]
fn grid_cas_exact_replay_and_cold_history_use_the_same_encrypted_filedoc_authority() {
    let path = std::env::temp_dir().join(format!("zoen-grid-cas-{}.sqlite", types::new_id("test")));
    let f = fixture(path.to_str().unwrap());
    let base = grid(&f, "create");
    let accepted = save(&f, &base, GRID_CHANGED, "accepted").unwrap();
    let before = counts(&f);
    assert_eq!(save(&f, &base, GRID_CHANGED, "accepted").unwrap(), accepted);
    assert!(matches!(
        save(&f, &base, GRID, "stale"),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(counts(&f), before);
    assert_eq!(accepted.version, 2);
    assert_eq!(accepted.data_base64, STANDARD.encode(GRID_CHANGED));
    let vault = f.vault.clone();
    drop(f);
    let client = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
    assert!(client.unlock(vault).unwrap());
    assert_eq!(
        client
            .app_binary_read(base.item_id.clone(), None, None)
            .unwrap(),
        accepted
    );
    let original = client
        .app_binary_read(base.item_id, Some(1), Some(base.sha256))
        .unwrap();
    assert_eq!(original.data_base64, STANDARD.encode(GRID));
    assert_eq!(original.current_version, 2);
    drop(client);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn grid_failed_transaction_and_revoked_grant_never_acknowledge_a_new_version() {
    let f = fixture(":memory:");
    let base = grid(&f, "create");
    let before = counts(&f);
    f.client.lock().store.conn().execute_batch("CREATE TRIGGER deny_grid_receipt BEFORE INSERT ON meta WHEN NEW.key LIKE 'app_document_receipts:%' BEGIN SELECT RAISE(ABORT,'synthetic grid failure'); END;").unwrap();
    assert!(save(&f, &base, GRID_CHANGED, "change").is_err());
    assert_eq!(counts(&f), before);
    assert_eq!(
        f.client
            .app_binary_read(base.item_id.clone(), None, None)
            .unwrap(),
        base
    );
    f.client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER deny_grid_receipt")
        .unwrap();
    let accepted = save(&f, &base, GRID_CHANGED, "change").unwrap();
    {
        let mut e = f.client.lock();
        let me = e.me_id().unwrap();
        let grant = e
            .state
            .grants
            .iter()
            .find(|g| g.grant.grantee.as_deref() == Some(format!("app:{}", base.item_id).as_str()))
            .unwrap()
            .grant
            .id
            .clone();
        e.append(&f.space, &me, EventBody::GrantRevoked { grant })
            .unwrap();
    }
    let before = counts(&f);
    assert!(save(&f, &base, GRID_CHANGED, "change").is_err());
    assert!(save(&f, &accepted, GRID, "next").is_err());
    assert_eq!(counts(&f), before);
}
