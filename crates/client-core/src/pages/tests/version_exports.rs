//! Version export uses the real encrypted journal, codec and application command.
use super::*;

#[tokio::test]
async fn historical_page_export_keeps_the_observed_version_and_private_draft_after_reopen() {
    let directory = std::env::temp_dir().join(new_id("page-export-test"));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory
        .join("client.sqlite")
        .to_string_lossy()
        .into_owned();
    let (client, fixture) = fixture_at(&path);
    let original = "# Original 🪴\r\n\r\nRaw **text** e\u{301}\r\n";
    let item = client
        .page_import_markdown(
            fixture.space_id.clone(),
            "original.md".into(),
            original.into(),
        )
        .unwrap();
    let first = client.page(item.id.clone()).unwrap();
    let mut shared = first.blocks.clone();
    shared[0].text = "Shared revision".into();
    apply(&client, &first, "shared-revision", &shared).unwrap();
    client
        .page_commit(item.id.clone(), "Shared revision".into())
        .unwrap();
    let second = client.page(item.id.clone()).unwrap();
    let shared_markdown = client.page_markdown(item.id.clone()).unwrap();
    let mut private = second.blocks.clone();
    private[1].text = "PRIVATE DRAFT must never be exported here".into();
    private[1].spans.clear();
    apply(&client, &second, "private-revision", &private).unwrap();
    let before = journal_rows(&client);
    let count = client.lock().logs[&fixture.space_id].events().len();
    let command = |version: u32, hash: &str| {
        serde_json::json!({"op":"page_export_version", "space_id":fixture.space_id,
        "item_id":item.id, "version":version, "observed_hash":hash})
        .to_string()
    };
    let export: PageVersionExport = serde_json::from_str(
        &client
            .client_request(command(1, &first.content_hash))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(export.markdown, original);
    assert_eq!(export.title, first.title);
    assert_eq!(export.version, 1);
    let export: PageVersionExport = serde_json::from_str(
        &client
            .client_request(command(2, &second.content_hash))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(export.markdown, shared_markdown);
    assert_eq!(export.title, second.title);
    assert!(!export.markdown.contains("PRIVATE"));
    assert_eq!(journal_rows(&client), before);
    assert_eq!(client.lock().logs[&fixture.space_id].events().len(), count);
    drop(client);
    let reopened = Client::open(path, "en".into()).unwrap();
    let export: PageVersionExport = serde_json::from_str(
        &reopened
            .client_request(command(1, &first.content_hash))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(export.markdown, original);
    assert_eq!(journal_rows(&reopened), before);
    assert_eq!(reopened.page(item.id.clone()).unwrap().blocks, private);
    assert!(reopened
        .page_markdown(item.id)
        .unwrap()
        .contains("PRIVATE DRAFT"));
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn historical_export_refuses_substituted_scope_hash_version_and_revoked_authority() {
    let (client, page) = fixture();
    let other = client
        .spaces()
        .into_iter()
        .find(|s| s.id != page.space_id)
        .unwrap()
        .id;
    let mut engine = client.lock();
    let count = engine.logs[&page.space_id].events().len();
    for version in [0, 2, u32::MAX] {
        assert!(engine
            .page_export_version(&page.space_id, &page.item_id, version, &page.content_hash)
            .is_err());
    }
    assert!(engine
        .page_export_version(&other, &page.item_id, 1, &page.content_hash)
        .is_err());
    assert!(engine
        .page_export_version(&page.space_id, &page.item_id, 1, "stale-hash")
        .is_err());
    engine
        .state
        .spaces
        .get_mut(&page.space_id)
        .unwrap()
        .integrity_error = Some("injected integrity failure".into());
    assert!(engine
        .page_export_version(&page.space_id, &page.item_id, 1, &page.content_hash)
        .is_err());
    engine
        .state
        .spaces
        .get_mut(&page.space_id)
        .unwrap()
        .integrity_error = None;
    let me = engine.me_id().unwrap();
    engine
        .state
        .spaces
        .get_mut(&page.space_id)
        .unwrap()
        .members
        .retain(|(id, _)| id != &me);
    assert!(matches!(
        engine.page_export_version(&page.space_id, &page.item_id, 1, &page.content_hash),
        Err(CoreError::Forbidden { .. })
    ));
    assert_eq!(engine.logs[&page.space_id].events().len(), count);
}

#[test]
fn historical_export_never_substitutes_an_empty_document_for_missing_verified_media() {
    let (client, seed) = fixture();
    let markdown = "# Verified media\r\n\r\nKeep these exact bytes 🪴\n";
    let document = Page::from_markdown(markdown).unwrap();
    let id = new_id("export-blob");
    let blob = {
        let mut engine = client.lock();
        let blob = engine
            .page_blob(&seed.space_id, &document.snapshot())
            .unwrap();
        let author = engine.me_id().unwrap();
        engine
            .append(
                &seed.space_id,
                &author,
                EventBody::ItemCreated {
                    item: id.clone(),
                    kind: ItemKind::Page,
                    content: ItemContent::Page(PageContent {
                        title: "Verified media".into(),
                        path: "verified.md".into(),
                        loro: String::new(),
                        blob: Some(blob.clone()),
                        manifest: None,
                    }),
                    origin: "Imported".into(),
                },
            )
            .unwrap();
        blob
    };
    let page = client.page(id.clone()).unwrap();
    let mut engine = client.lock();
    let (mime, bytes) = engine.store.media(&blob.sha256).unwrap().unwrap();
    engine
        .store
        .conn()
        .execute("DELETE FROM media WHERE sha256 = ?1", [&blob.sha256])
        .unwrap();
    engine.reload().unwrap();
    assert!(engine
        .page_export_version(&seed.space_id, &id, 1, &page.content_hash)
        .is_err());
    engine
        .store
        .put_media(&blob.sha256, &mime, &bytes, crate::engine::now_ms())
        .unwrap();
    let exported = engine
        .page_export_version(&seed.space_id, &id, 1, &page.content_hash)
        .unwrap();
    assert_eq!(exported.markdown, markdown);
}

#[test]
fn historical_preview_and_export_stay_available_while_a_newer_version_downloads() {
    let (client, first) = fixture();
    let original = client.page_markdown(first.item_id.clone()).unwrap();
    {
        let mut engine = client.lock();
        let document = Page::from_markdown("# Later version\n\nStill downloading\n").unwrap();
        let blob = engine
            .page_blob(&first.space_id, &document.snapshot())
            .unwrap();
        engine
            .store
            .conn()
            .execute("DELETE FROM media WHERE sha256 = ?1", [&blob.sha256])
            .unwrap();
        let me = engine.me_id().unwrap();
        engine
            .append(
                &first.space_id,
                &me,
                EventBody::ItemVersioned {
                    item: first.item_id.clone(),
                    content: ItemContent::Page(PageContent {
                        title: "Later version".into(),
                        path: "review.md".into(),
                        loro: String::new(),
                        blob: Some(blob),
                        manifest: None,
                    }),
                    note: "Waiting for media".into(),
                },
            )
            .unwrap();
        engine.reload().unwrap();
    }
    assert!(!client.page(first.item_id.clone()).unwrap().ready);
    let previous = client.page_at(first.item_id.clone(), 1).unwrap();
    assert!(previous.ready);
    assert!(!previous.input_pending && !previous.unsaved && !previous.can_edit);
    assert_eq!(previous.blocks, first.blocks);
    let engine = client.lock();
    assert_eq!(
        engine
            .page_export_version(&first.space_id, &first.item_id, 1, &first.content_hash)
            .unwrap()
            .markdown,
        original
    );
    assert!(engine
        .page_export_version(&first.space_id, &first.item_id, 2, &first.content_hash)
        .is_err());
}
