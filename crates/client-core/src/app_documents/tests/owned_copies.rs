//! Whole owned copies cases on the same original parent fixture.

use super::*;

pub(super) fn copy_is_one_atomic_new_item_with_original_untouched_and_exact_lost_ack_receipt() {
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

pub(super) fn failed_copy_cannot_publish_a_blank_or_ungranted_item() {
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

pub(super) fn app_forwarding_owns_a_fresh_manifest_and_preserves_current_full_scene_without_source_refs(
) {
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

pub(super) fn evicted_copy_receipt_cannot_create_a_second_item_for_an_old_intent() {
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
