//! Complete binary compatibility scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn binary_offline_commit_preserves_the_existing_page_contract(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (legacy, legacy_id, _) = edited_binary_fixture()?;
    let mut legacy_engine = legacy.lock();
    let space = legacy_engine.item_state(&legacy_id).unwrap().space.clone();
    legacy_engine.net.synced.remove(&space);
    assert!(legacy_engine
        .page_commit(&legacy_id, "offline baseline")
        .unwrap());
    let legacy_read = legacy_engine.page(&legacy_id).unwrap();
    assert!(!legacy_read.pending_sync);

    let (public, id, mut session) = edited_binary_fixture()?;
    let mut engine = public.lock();
    let space = engine.item_state(&id).unwrap().space.clone();
    engine.net.synced.remove(&space);
    assert!(!engine.net.synced.contains(&space));
    let CommitRef::BinaryPage { client_id } =
        queued(session.commit(&mut engine, "offline save").unwrap())
    else {
        panic!("BinaryPage commit")
    };
    assert!(engine.store.outbox_get(&client_id).unwrap().is_none());
    let event = engine.logs[&space]
        .events()
        .iter()
        .find(|event| event.client_id == client_id)
        .unwrap();
    assert!(
        matches!(&event.body, EventBody::ItemVersioned { item, content: ItemContent::Page(PageContent { manifest: None, .. }), .. } if item == &id)
    );
    let read = session.read(&engine).unwrap();
    assert_eq!(read.pending_sync, legacy_read.pending_sync);
    assert_eq!(read.unsaved, legacy_read.unsaved);
    engine.reload().unwrap();
    let mut reopened = PageSession::open(&engine, &id).unwrap();
    assert_eq!(
        reopened.read(&engine).unwrap().content_hash,
        read.content_hash
    );
    Ok(())
}

pub(super) fn binary_replay_keeps_the_exact_pre_upgrade_payload(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (public, id, _) = edited_binary_fixture()?;
    let read = public.page(id.clone()).unwrap();
    let order: Vec<_> = read.blocks.iter().map(|b| b.id.clone()).collect();
    let mut dto = read.blocks[1].clone();
    assert_eq!(dto.kind, "paragraph");
    dto.level = 5;
    dto.text = "Exact legacy payload on retry".into();
    dto.spans.clear();
    let accepted = public
        .page_apply_from(
            id.clone(),
            "binary-raw-upgrade".into(),
            read.edit_context.clone(),
            order.clone(),
            vec![dto.clone()],
        )
        .unwrap();
    let legacy_replay = public
        .page_apply_from(
            id.clone(),
            "binary-raw-upgrade".into(),
            read.edit_context.clone(),
            order.clone(),
            vec![dto.clone()],
        )
        .unwrap();
    assert!(legacy_replay.replayed);
    assert_eq!(legacy_replay.mutation_hash, accepted.mutation_hash);
    let engine = public.lock();
    let it = engine.item_state(&id).unwrap();
    let before = serde_json::to_vec(&engine.load_page_journal(it).unwrap()).unwrap();
    let mut session = PageSession::open(&engine, &id).unwrap();
    let replay = session
        .apply_from(
            &engine,
            "binary-raw-upgrade",
            &read.edit_context,
            DocumentEdit::BinaryPage {
                order: &order,
                changed: std::slice::from_ref(&dto),
            },
        )
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.request_hash, accepted.mutation_hash);
    assert_eq!(replay.applied_content_hash, accepted.applied_content_hash);
    let original_context = engine
        .decode_page_edit_context(it, &accepted.applied_edit_context)
        .unwrap();
    for context in [
        &legacy_replay.applied_edit_context,
        &replay.applied_edit_context,
    ] {
        let decoded = engine.decode_page_edit_context(it, context).unwrap();
        assert_eq!(decoded.generation, original_context.generation);
        assert_eq!(decoded.frontiers, original_context.frontiers);
    }
    let mut changed_payload = dto.clone();
    changed_payload.level = 6;
    assert!(matches!(
        session.apply_from(
            &engine,
            "binary-raw-upgrade",
            &read.edit_context,
            DocumentEdit::BinaryPage {
                order: &order,
                changed: std::slice::from_ref(&changed_payload),
            },
        ),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(
        serde_json::to_vec(&engine.load_page_journal(it).unwrap()).unwrap(),
        before
    );
    Ok(())
}

pub(super) fn binary_refresh_reuses_verified_payload_and_rechecks_after_reload(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    let id = new_id("binary-cache-page")?;
    let page = Page::from_markdown("# Cached\n\nVerified raw 😀\r\ncontent\n").unwrap();
    let blob = f.engine.page_blob(&f.space, &page.snapshot()).unwrap();
    f.ordered(EventBody::ItemCreated {
        item: id.clone(),
        kind: ItemKind::Page,
        content: ItemContent::Page(PageContent {
            title: "Cached".into(),
            path: "cached.md".into(),
            loro: String::new(),
            blob: Some(blob.clone()),
            manifest: None,
        }),
        origin: "imported binary Page".into(),
    })?;
    let legacy = f.engine.page(&id).unwrap();
    let mut session = PageSession::open(&f.engine, &id).unwrap();
    let before = session.read(&f.engine).unwrap();
    assert!(before.can_edit && before.save_error.is_none());
    let (mime, bytes) = f.engine.store.media(&blob.sha256).unwrap().unwrap();
    assert_eq!(
        f.engine
            .store
            .conn()
            .execute("DELETE FROM media WHERE sha256 = ?1", [&blob.sha256])
            .unwrap(),
        1
    );
    assert!(f.engine.store.media(&blob.sha256).unwrap().is_none());
    // The existing public Page session retains the bytes it already verified.
    let legacy_cached = f.engine.page(&id).unwrap();
    assert!(legacy_cached.can_edit && legacy_cached.save_error.is_none());
    assert_eq!(legacy_cached.content_hash, legacy.content_hash);
    let cached = session.read(&f.engine).unwrap();
    assert!(
        cached.can_edit && cached.save_error.is_none(),
        "already verified binary history must not be downloaded again"
    );
    assert_eq!(cached.view, before.view);
    assert_eq!(cached.content_hash, before.content_hash);

    // Reprojection discards that trust; a fresh session still needs the payload.
    f.engine.reload().unwrap();
    let reloaded = session.read(&f.engine).unwrap();
    assert!(!reloaded.can_edit);
    assert!(matches!(session.blocked, Some(SessionBlock::Waiting(_))));
    let mut reopened = PageSession::open(&f.engine, &id).unwrap();
    assert_eq!(
        reloaded.content_hash,
        reopened.read(&f.engine).unwrap().content_hash
    );
    f.engine
        .store
        .put_media(&blob.sha256, &mime, &bytes, crate::engine::now_ms())
        .unwrap();
    let resumed = session.read(&f.engine).unwrap();
    assert!(resumed.can_edit && resumed.save_error.is_none());
    assert_eq!(resumed.view, before.view);
    assert_eq!(resumed.content_hash, before.content_hash);

    Ok(())
}

pub(super) fn binary_failed_publication_preserves_the_existing_visible_error(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (public, id, mut session) = edited_binary_fixture()?;
    let mut engine = public.lock();
    let space = engine.item_state(&id).unwrap().space.clone();
    engine.store.set_synced(&space).unwrap();
    engine.net.synced.insert(space);
    let CommitRef::BinaryPage { client_id } =
        queued(session.commit(&mut engine, "save then refuse").unwrap())
    else {
        panic!("BinaryPage commit")
    };
    engine
        .store
        .outbox_note(&client_id, "the server refused this exact save", true)
        .unwrap();
    let public_read = engine.page(&id).unwrap();
    let legacy_error = public_read.save_error.clone();
    assert_eq!(
        legacy_error.as_deref(),
        Some("the server refused this exact save")
    );
    let read = session.read(&engine).unwrap();
    assert_eq!(read.save_error, legacy_error);
    assert!(read.unsaved);
    assert!(!read.pending_sync);
    assert!(public_read.unsaved);
    assert!(!public_read.pending_sync);
    assert_eq!(read.content_hash, public_read.content_hash);
    engine.reload().unwrap();
    let reopened = engine.page(&id).unwrap();
    assert_eq!(reopened.content_hash, public_read.content_hash);
    assert_eq!(reopened.save_error, legacy_error);
    assert!(reopened.unsaved);
    assert!(!reopened.pending_sync);
    Ok(())
}
