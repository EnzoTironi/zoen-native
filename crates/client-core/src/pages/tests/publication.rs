//! Whole publication cases on the same original parent fixture.

use super::*;

pub(super) fn event_append_failure_preserves_draft_across_reload_and_retry() {
    let (engine, observed) = fixture();
    let mut edited = observed.blocks.clone();
    edited[1].text = "Draft before interrupted save".into();
    apply(&engine, &observed, "draft", &edited).unwrap();
    engine.lock().store.conn().execute_batch("CREATE TEMP TRIGGER refuse_commit BEFORE INSERT ON events BEGIN SELECT RAISE(ABORT, 'injected event failure'); END;").unwrap();
    assert!(engine
        .page_commit(observed.item_id.clone(), "Save".into())
        .is_err());
    let restored = engine.page(observed.item_id.clone()).unwrap();
    assert_eq!(restored.blocks, edited);
    assert!(restored.unsaved);
    engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER refuse_commit")
        .unwrap();
    assert!(engine
        .page_commit(observed.item_id.clone(), "Retry".into())
        .unwrap());
    assert!(!engine.page(observed.item_id.clone()).unwrap().unsaved);
    engine.lock().reload().unwrap();
    assert!(
        apply(&engine, &observed, "draft", &edited)
            .unwrap()
            .replayed
    );
}

pub(super) fn newer_draft_waits_for_ack_and_survives_a_permanent_refusal(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (engine, observed) = fixture();
    {
        let mut e = engine.lock();
        e.store.set_synced(&observed.space_id).unwrap();
        e.net.synced.insert(observed.space_id.clone());
    }
    let mut first = observed.blocks.clone();
    first[1].text = "First queued save".into();
    apply(&engine, &observed, "first", &first).unwrap();
    assert!(engine
        .page_commit(observed.item_id.clone(), "First".into())
        .unwrap());
    let fresh = engine.page(observed.item_id.clone()).unwrap();
    assert!(fresh.pending_sync);
    let mut second = fresh.blocks.clone();
    second[1].text = "Newer draft while offline".into();
    apply(&engine, &fresh, "second", &second).unwrap();
    assert!(!engine
        .page_commit(observed.item_id.clone(), "Deferred".into())
        .unwrap());
    let id = engine.lock().store.outbox().unwrap().into_iter().find(|p| matches!(&p.event.body, EventBody::ItemVersioned { item, .. } if item == &observed.item_id)).unwrap().event.client_id;
    engine
        .lock()
        .reject(&id, "permanent permission refusal", true, None);
    let refused = engine.page(observed.item_id.clone()).unwrap();
    assert_eq!(refused.blocks, second);
    assert!(refused.unsaved);
    assert!(!refused.pending_sync);
    assert!(refused.save_error.is_some());
    assert!(engine
        .page_commit(observed.item_id.clone(), "Retry whole draft".into())
        .unwrap());
    assert_ne!(confirm_pending(&engine, &observed.item_id)?, id);
    let confirmed = engine.page(observed.item_id.clone()).unwrap();
    assert_eq!(confirmed.blocks, second);
    assert!(!confirmed.unsaved);
    assert!(!confirmed.pending_sync);
    assert_eq!(confirmed.save_error, None);
    Ok(())
}

pub(super) fn public_page_reads_reuse_verified_media_and_require_it_again_after_reload(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (engine, original) = fixture();
    let markdown = "# Cached\r\n\r\nVerified raw 😀 e\u{301}\r\ncontent\n";
    let document = Page::from_markdown(markdown).unwrap();
    let id = new_id("cached-public-page")?;
    let blob = {
        let mut core = engine.lock();
        let blob = core
            .page_blob(&original.space_id, &document.snapshot())
            .unwrap();
        let author = core.me_id().unwrap();
        core.append(
            &original.space_id,
            &author,
            EventBody::ItemCreated {
                item: id.clone(),
                kind: ItemKind::Page,
                content: ItemContent::Page(PageContent {
                    title: "Cached".into(),
                    path: "cached.md".into(),
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
    let observed = engine.page(id.clone()).unwrap();
    assert!(observed.ready && observed.can_edit);
    let (mime, bytes) = {
        let core = engine.lock();
        let stored = core.store.media(&blob.sha256).unwrap().unwrap();
        assert_eq!(
            core.store
                .conn()
                .execute("DELETE FROM media WHERE sha256 = ?1", [&blob.sha256])
                .unwrap(),
            1
        );
        assert!(core.store.media(&blob.sha256).unwrap().is_none());
        stored
    };
    let cached = engine.page(id.clone()).unwrap();
    assert!(cached.ready && cached.can_edit);
    assert_eq!(cached.blocks, observed.blocks);
    assert_eq!(
        engine.page_at(id.clone(), 1).unwrap().blocks,
        observed.blocks
    );
    assert_eq!(engine.page_markdown(id.clone()).unwrap(), markdown);
    engine.lock().reload().unwrap();
    let missing = engine.page(id.clone()).unwrap();
    assert!(!missing.ready && !missing.can_edit);
    assert!(engine
        .page_commit(id.clone(), "Refused while missing".into())
        .is_err());
    assert!(engine.page_markdown(id.clone()).is_err());
    assert!(journal_rows(&engine).is_empty());
    engine
        .lock()
        .store
        .put_media(&blob.sha256, &mime, &bytes, crate::engine::now_ms())
        .unwrap();
    let resumed = engine.page(id.clone()).unwrap();
    assert!(resumed.ready && resumed.can_edit);
    assert_eq!(resumed.blocks, observed.blocks);
    assert_eq!(engine.page_markdown(id).unwrap(), markdown);

    Ok(())
}

pub(super) fn publication_journal_failure_rolls_back_the_actual_event_and_retains_the_draft() {
    let (engine, observed) = fixture();
    let mut blocks = observed.blocks.clone();
    blocks[1].text = "Draft survives failed publication marker".into();
    apply(&engine, &observed, "atomic-publication", &blocks).unwrap();
    let before = engine.item(observed.item_id.clone()).unwrap();
    let encrypted = journal_rows(&engine);
    let events = engine.lock().logs[&observed.space_id].events().len();
    engine
        .lock()
        .store
        .conn()
        .execute_batch(
            "CREATE TEMP TRIGGER refuse_publication_marker BEFORE UPDATE ON meta
         WHEN NEW.key LIKE 'page.draft.%'
         BEGIN SELECT RAISE(ABORT, 'injected publication marker failure'); END;",
        )
        .unwrap();
    assert!(engine
        .page_commit(observed.item_id.clone(), "Refused marker".into())
        .is_err());
    assert_eq!(
        engine.item(observed.item_id.clone()).unwrap().version,
        before.version
    );
    assert_eq!(
        engine.lock().logs[&observed.space_id].events().len(),
        events
    );
    assert_eq!(journal_rows(&engine), encrypted);
    assert_eq!(
        engine.page(observed.item_id.clone()).unwrap().blocks,
        blocks
    );
    engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER refuse_publication_marker")
        .unwrap();
    assert!(engine
        .page_commit(observed.item_id.clone(), "Accepted marker".into())
        .unwrap());
    assert_eq!(
        engine.item(observed.item_id.clone()).unwrap().version,
        before.version + 1
    );
    assert_eq!(engine.page(observed.item_id).unwrap().blocks, blocks);
}
