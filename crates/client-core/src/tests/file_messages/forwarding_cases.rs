//! Complete forwarding scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn cross_chat_media_forwarding_copies_current_files_without_source_bindings() {
    for mime in ["image/jpeg", "video/mp4", "application/pdf", "audio/mp4"] {
        let e = seeded();
        let source = find_space(&e, "Paraty com a Marina").id;
        let destination = find_space(&e, "Zoen").id;
        let root = e
            .send_message(source.clone(), "Source thread".into())
            .unwrap();
        let sent = send(&e, &source, mime, Some(root.id), true, "source").unwrap();
        let source_item = card(&sent).item_id.clone();
        let bytes = vec![6; 2100];
        let thumbnail = vec![5; 125];
        e.file_new_version_typed(
            source_item.clone(),
            bytes.clone(),
            Some(thumbnail.clone()),
            "Edit".into(),
            "current.bin".into(),
            mime.into(),
        )
        .unwrap();
        e.edit_message(source.clone(), sent.id.clone(), "Current caption".into())
            .unwrap();
        let before = e.timeline(destination.clone()).unwrap().len();
        let forwarded = e
            .forward_message(source.clone(), sent.id.clone(), destination.clone())
            .unwrap();
        assert!(forwarded.forwarded && forwarded.voice.is_none());
        assert!(forwarded.reply_to.is_none() && forwarded.in_thread.is_none());
        let EntryKind::Message {
            text,
            card: Some(copied),
        } = &forwarded.kind
        else {
            panic!("forwarded media must render as an attachment");
        };
        assert_eq!(text, "Current caption");
        assert_ne!(copied.item_id, source_item);
        assert_eq!(copied.version, 1);
        let original = file_document(&e, &source_item);
        let duplicate = file_document(&e, &copied.item_id);
        assert_eq!(duplicate.name, "current.bin");
        assert_eq!(duplicate.mime, mime);
        assert_eq!(duplicate.sha256, original.sha256);
        assert_ne!(duplicate.chunks[0].key, original.chunks[0].key);
        assert_ne!(duplicate.chunks[0].blob, original.chunks[0].blob);
        assert_ne!(
            duplicate.thumb.as_ref().unwrap().key,
            original.thumb.as_ref().unwrap().key
        );
        assert_eq!(
            e.file_bytes(copied.item_id.clone(), None).unwrap(),
            Some(bytes.clone())
        );
        assert_eq!(
            e.file_thumbnail(copied.item_id.clone()).unwrap(),
            Some(thumbnail.clone())
        );
        {
            let engine = e.lock();
            assert_ne!(
                engine
                    .store
                    .meta(&format!("file_key:{source_item}"))
                    .unwrap(),
                engine
                    .store
                    .meta(&format!("file_key:{}", copied.item_id))
                    .unwrap()
            );
            let events = engine.logs[&destination].events();
            assert!(matches!(&events[events.len()-2].body,
                EventBody::ItemCreated { item, .. } if item == &copied.item_id));
            assert!(matches!(&events.last().unwrap().body,
                EventBody::MessagePosted { attaches: Some(item), reply: None, forwarded: true, .. }
                    if item == &copied.item_id));
            assert!(!serde_json::to_string(&events[events.len() - 2..])
                .unwrap()
                .contains(&source_item));
        }
        assert_eq!(e.timeline(destination.clone()).unwrap().len(), before + 1);
        // A later edit or removal in the first audience cannot change the copy.
        e.file_new_version(source_item, vec![4; 64], None, "Later edit".into())
            .unwrap();
        e.delete_message(source, sent.id).unwrap();
        e.lock().reload().unwrap();
        let retained = e.timeline(destination.clone()).unwrap().pop().unwrap();
        assert_eq!(retained.id, forwarded.id);
        assert!(retained.forwarded && !retained.deleted);
        assert_eq!(
            e.file_bytes(copied.item_id.clone(), None).unwrap(),
            Some(bytes)
        );
        assert_eq!(
            e.file_thumbnail(copied.item_id.clone()).unwrap(),
            Some(thumbnail)
        );
        assert!(e.verify_log(destination).valid);
    }
}

pub(super) fn destination_only_receiver_decrypts_forwarded_media_and_thumbnail() {
    use crate::sync::Ingest;
    use protocol::{Envelope, Sequenced};
    let sender = seeded();
    let source = find_space(&sender, "Paraty com a Marina").id;
    let destination = find_space(&sender, "Zoen").id;
    let sent = send(&sender, &source, "video/mp4", None, false, "original").unwrap();
    let source_item = card(&sent).item_id.clone();
    let mut relay = sender.lock().logs[&destination].clone();
    sender.lock().net.synced.insert(destination.clone());
    let forwarded = sender
        .forward_message(source.clone(), sent.id, destination.clone())
        .unwrap();
    let item = card(&forwarded).item_id.clone();
    let pending = sender.lock().store.outbox().unwrap();
    assert_eq!(pending.len(), 2);
    for event in pending {
        relay.sequence(event.event);
    }
    let receiver = Client::open(":memory:".into(), "en".into()).unwrap();
    for event in relay.events() {
        assert!(matches!(
            receiver.lock().ingest(Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: Envelope::plain(event),
            }),
            Ingest::Applied
        ));
    }
    assert!(!receiver.lock().logs.contains_key(&source));
    assert!(receiver.item(source_item).is_err());
    let received = receiver
        .timeline(destination.clone())
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(received.id, forwarded.id);
    assert!(received.forwarded && received.reply_to.is_none() && received.in_thread.is_none());
    assert_eq!(card(&received).item_id, item);
    assert_eq!(receiver.file_bytes(item.clone(), None).unwrap(), None);
    let uploads = sender.lock().pending_uploads(10);
    let wanted = receiver.lock().wanted_media();
    assert_eq!(
        wanted.len(),
        2,
        "both original bytes and thumbnail travel independently"
    );
    for piece in wanted {
        let encrypted = &uploads
            .iter()
            .find(|(blob, _)| blob == &piece.blob)
            .unwrap()
            .1;
        assert_ne!(encrypted, &vec![7; 1024]);
        assert!(receiver.lock().media_arrived(&piece, encrypted));
    }
    receiver.lock().reload().unwrap();
    assert_eq!(
        receiver.file_bytes(item.clone(), None).unwrap(),
        Some(vec![7; 1024])
    );
    assert_eq!(receiver.file_thumbnail(item).unwrap(), Some(vec![9; 64]));
    assert!(receiver.verify_log(destination).valid);
}

pub(super) fn unavailable_or_corrupt_forwarded_media_does_not_write_a_partial_copy() {
    for damage in [
        "missing_file",
        "corrupt_file",
        "missing_thumbnail",
        "corrupt_thumbnail",
    ] {
        let e = seeded();
        let source = find_space(&e, "Paraty com a Marina").id;
        let destination = find_space(&e, "Zoen").id;
        let sent = send(&e, &source, "image/jpeg", None, false, "source").unwrap();
        let doc = file_document(&e, &card(&sent).item_id);
        let sha = if damage.ends_with("thumbnail") {
            &doc.thumb.unwrap().sha256
        } else {
            &doc.chunks[0].sha256
        };
        let sql = if damage.starts_with("missing") {
            "DELETE FROM media WHERE sha256 = ?1"
        } else {
            "UPDATE media SET bytes = zeroblob(length(bytes)) WHERE sha256 = ?1"
        };
        e.lock().store.conn().execute(sql, [sha]).unwrap();
        let before = storage(&e);
        let timeline = e.timeline(destination.clone()).unwrap();
        let items = e.items().len();
        assert!(
            e.forward_message(source, sent.id, destination.clone())
                .is_err(),
            "{damage}"
        );
        assert_eq!(storage(&e), before, "{damage}");
        assert_eq!(e.timeline(destination).unwrap(), timeline, "{damage}");
        assert_eq!(e.items().len(), items, "{damage}");
    }
}

pub(super) fn failed_forwarded_file_or_message_write_rolls_back_every_copy_and_can_retry() {
    for synced in [false, true] {
        for event in ["ItemCreated", "MessagePosted"] {
            let e = seeded();
            let source = find_space(&e, "Paraty com a Marina").id;
            let destination = find_space(&e, "Zoen").id;
            let sent = send(&e, &source, "image/jpeg", None, false, "source").unwrap();
            {
                let mut engine = e.lock();
                if synced {
                    engine.store.set_synced(&destination).unwrap();
                    engine.net.synced.insert(destination.clone());
                }
                let table = if synced { "outbox" } else { "events" };
                engine.store.conn().execute_batch(&format!(
                    "CREATE TRIGGER fail_forward BEFORE INSERT ON {table} WHEN NEW.json LIKE '%{event}%' BEGIN SELECT RAISE(ABORT,'forward failed'); END;"
                )).unwrap();
            }
            let before = storage(&e);
            let timeline = e.timeline(destination.clone()).unwrap();
            let items = e.items().len();
            assert!(e
                .forward_message(source.clone(), sent.id.clone(), destination.clone())
                .is_err());
            assert_eq!(storage(&e), before, "{synced}, {event}");
            assert_eq!(e.timeline(destination.clone()).unwrap(), timeline);
            assert_eq!(e.items().len(), items);
            assert!(e.lock().net.pending.is_empty());
            e.lock()
                .store
                .conn()
                .execute_batch("DROP TRIGGER fail_forward")
                .unwrap();
            let forwarded = e
                .forward_message(source, sent.id, destination.clone())
                .unwrap();
            assert!(forwarded.forwarded);
            assert_eq!(e.timeline(destination).unwrap().len(), timeline.len() + 1);
            assert_eq!(
                e.file_bytes(card(&forwarded).item_id.clone(), None)
                    .unwrap(),
                Some(vec![7; 1024])
            );
        }
    }
}

pub(super) fn same_chat_file_forwarding_keeps_the_existing_attachment_and_foreign_items_fail_cleanly(
) {
    let e = seeded();
    let source = find_space(&e, "Paraty com a Marina").id;
    let destination = find_space(&e, "Zoen").id;
    let sent = send(&e, &source, "image/jpeg", None, false, "source").unwrap();
    let items = e.items().len();
    let forwarded = e
        .forward_message(source.clone(), sent.id.clone(), source.clone())
        .unwrap();
    assert!(forwarded.forwarded);
    assert_eq!(card(&forwarded).item_id, card(&sent).item_id);
    assert_eq!(e.items().len(), items);
    let plan = paraty_plan(&e);
    let post = {
        let mut engine = e.lock();
        let me = engine.me_id().unwrap();
        engine
            .post_as(&source, &me, "Shared plan", Some(plan.id))
            .unwrap()
    };
    let before = storage(&e);
    let timeline = e.timeline(destination.clone()).unwrap();
    assert!(matches!(
        e.forward_message(source, post.id, destination.clone()),
        Err(CoreError::Invalid { .. })
    ));
    assert_eq!(storage(&e), before);
    assert_eq!(e.timeline(destination).unwrap(), timeline);
}

pub(super) fn forwarding_an_ordinary_audio_filename_does_not_create_a_second_voice_message() {
    let e = seeded();
    let source = find_space(&e, "Paraty com a Marina").id;
    let destination = find_space(&e, "Zoen").id;
    // Imported legacy files can have a slash in a display name. Only the actual
    // signed path, not that name, determines the legacy voice classification.
    let file = e
        .file_add(
            source.clone(),
            "Imports/audio.m4a".into(),
            "VoiceNotes/audio.m4a".into(),
            "audio/mp4".into(),
            vec![3; 128],
            None,
        )
        .unwrap();
    let sent = {
        let mut engine = e.lock();
        let me = engine.me_id().unwrap();
        engine
            .post_as(&source, &me, "Imported recording", Some(file.id))
            .unwrap()
    };
    assert!(sent.voice.is_none());
    let before = e.timeline(destination.clone()).unwrap().len();
    let forwarded = e
        .forward_message(source, sent.id, destination.clone())
        .unwrap();
    assert!(forwarded.forwarded && forwarded.voice.is_none());
    let EntryKind::Message {
        card: Some(card), ..
    } = forwarded.kind
    else {
        panic!("missing file");
    };
    assert!(!file_document(&e, &card.item_id).is_voice());
    assert_eq!(e.timeline(destination.clone()).unwrap().len(), before + 1);
    e.lock().reload().unwrap();
    assert_eq!(e.timeline(destination).unwrap().len(), before + 1);
}
