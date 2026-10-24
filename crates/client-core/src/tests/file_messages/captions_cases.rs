//! Complete captions scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn file_caption_is_signed_with_its_attachment_and_thread_before_receiver_replay() {
    use crate::sync::Ingest;
    use protocol::{Envelope, Sequenced};
    let sender = seeded();
    let space = find_space(&sender, "Zoen").id;
    let root = sender.send_message(space.clone(), "Root".into()).unwrap();
    let child = sender
        .send_reply(space.clone(), "Child".into(), root.id.clone(), true)
        .unwrap();
    let caption = "  Página 2 🗂️\nTotal: 161.  ";
    let before = sender.timeline(space.clone()).unwrap().len();
    let sent = send_caption(&sender, &space, Some(child.id), true, "caption", caption).unwrap();
    let item = caption_card(&sent, caption).item_id.clone();
    assert_eq!(sent.in_thread.as_deref(), Some(root.id.as_str()));
    assert_eq!(sender.timeline(space.clone()).unwrap().len(), before + 1);
    let receiver = Client::open(":memory:".into(), "en".into()).unwrap();
    for event in sender.lock().logs[&space].events() {
        assert!(matches!(
            receiver.lock().ingest(Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: Envelope::plain(event)
            }),
            Ingest::Applied
        ));
    }
    let received = receiver.thread(space.clone(), root.id).unwrap();
    let received = received.last().unwrap();
    assert_eq!(received.id, sent.id);
    assert_eq!(caption_card(received, caption).item_id, item);
    assert!(!received.edited);
    let engine = sender.lock();
    let events = engine.logs[&space].events();
    assert!(
        matches!(&events[events.len()-2].body, EventBody::ItemCreated { item: id, .. } if id == &item)
    );
    assert!(matches!(&events.last().unwrap().body,
        EventBody::MessagePosted { text, attaches: Some(id), .. } if text == caption && id == &item));
    assert!(receiver.verify_log(space).valid);
}

pub(super) fn clearing_file_caption_through_public_edit_reaches_signed_recipient_without_changing_file(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::sync::Ingest;
    use protocol::{Envelope, Sequenced};
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

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
    let account = |name: &str| {
        let engine = Client::open(":memory:".into(), "en".into()).unwrap();
        engine
            .create_account(
                name.into(),
                name.into(),
                "http://127.0.0.1:1".into(),
                Arc::new(Vault::default()),
            )
            .unwrap();
        engine
    };
    let sender = account("caption_author");
    let receiver = account("caption_reader");
    let space = sender
        .create_group_with(
            "Caption edit QA".into(),
            vec![receiver.account().unwrap().identity_id],
            PrivacyDto::Closed,
        )
        .unwrap();
    let root = sender.send_message(space.clone(), "Thread".into()).unwrap();
    let sent = send_caption(
        &sender,
        &space,
        Some(root.id.clone()),
        true,
        "clear",
        "Caption",
    )
    .unwrap();
    let item = caption_card(&sent, "Caption").item_id.clone();
    let revised_bytes = b"%PDF-1.4 immutable second version 53 161".to_vec();
    sender
        .file_new_version(
            item.clone(),
            revised_bytes.clone(),
            None,
            "Version 2".into(),
        )
        .unwrap();
    let before = sender.item(item.clone()).unwrap();
    let timeline_count = sender.timeline(space.clone()).unwrap().len();
    let mut relay = event_log::SpaceLog::new(space.clone());
    let mut sequenced = HashSet::new();
    let mut catch_up = || -> std::result::Result<(), Box<dyn std::error::Error>> {
        for pending in sender.lock().store.outbox().unwrap() {
            if pending.event.space == space && sequenced.insert(pending.event.client_id.clone()) {
                let event = relay.sequence(pending.event)?;
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
        }
        Ok(())
    };
    catch_up()?;
    let uploads = sender.lock().pending_uploads(100);
    let wanted = receiver.lock().wanted_media();
    for wanted in wanted {
        let encrypted = &uploads
            .iter()
            .find(|(blob, _)| blob == &wanted.blob)
            .unwrap()
            .1;
        assert!(receiver.lock().media_arrived(&wanted, encrypted));
    }
    for blank in ["", " \n\t"] {
        sender
            .edit_message(space.clone(), sent.id.clone(), blank.into())
            .expect("the public FFI must allow clearing an ordinary file caption");
        catch_up()?;
        let thread = receiver.thread(space.clone(), root.id.clone()).unwrap();
        let received = thread.iter().find(|entry| entry.id == sent.id).unwrap();
        assert_eq!(caption_card(received, "").item_id, item);
        assert!(received.edited && !received.author.is_me && received.voice.is_none());
        assert_eq!(received.author.id, sender.account().unwrap().identity_id);
        assert_eq!(received.in_thread.as_deref(), Some(root.id.as_str()));
        let received_storage = storage(&receiver);
        assert!(matches!(
            receiver.edit_message(space.clone(), sent.id.clone(), "".into()),
            Err(CoreError::Forbidden { .. })
        ));
        assert_eq!(storage(&receiver), received_storage);
        assert_eq!(
            sender.timeline(space.clone()).unwrap().len(),
            timeline_count
        );
        assert_eq!(sender.item(item.clone()).unwrap(), before);
        assert_eq!(
            sender.file_bytes(item.clone(), Some(1)).unwrap(),
            Some(vec![7; 1024])
        );
        assert_eq!(
            sender.file_bytes(item.clone(), Some(2)).unwrap(),
            Some(revised_bytes.clone())
        );
        assert_eq!(
            receiver.file_bytes(item.clone(), None).unwrap(),
            Some(revised_bytes.clone())
        );
        receiver.lock().reload().unwrap();
        assert_eq!(
            caption_card(
                receiver
                    .thread(space.clone(), root.id.clone())
                    .unwrap()
                    .last()
                    .unwrap(),
                ""
            )
            .item_id,
            item
        );
        assert!(receiver.verify_log(space.clone()).valid);
    }
    Ok(())
}

pub(super) fn blank_caption_edit_is_scoped_to_live_own_ordinary_files_and_enforces_caption_bounds()
{
    let e = seeded();
    let space = find_space(&e, "Zoen").id;
    let other_space = find_space(&e, "Paraty com a Marina").id;
    let me = e.me().unwrap().id;
    let plain = e.send_message(space.clone(), "Text body".into()).unwrap();
    let page = e.page_create(space.clone(), "Miniapp".into()).unwrap();
    let miniapp = e
        .lock()
        .post_as(&space, &me, "Miniapp", Some(page.id))
        .unwrap();
    let voice = e
        .send_voice_note(
            space.clone(),
            vec![7; 500],
            "audio/mp4".into(),
            1000,
            vec![0.5],
            "Transcript".into(),
            None,
            false,
        )
        .unwrap();
    assert!(voice.voice.is_some());
    let file = send_caption(&e, &space, None, false, "editable", "Caption").unwrap();
    caption_card(&file, "Caption");
    for invalid in [
        "bad\0caption".into(),
        "é".repeat(crate::files::MAX_FILE_CAPTION_BYTES / 2 + 1),
        " ".repeat(crate::files::MAX_FILE_CAPTION_BYTES + 1),
    ] {
        let before = storage(&e);
        assert!(matches!(
            e.edit_message(space.clone(), file.id.clone(), invalid),
            Err(CoreError::Invalid { .. })
        ));
        assert_eq!(storage(&e), before);
    }
    let maximum = "é".repeat(crate::files::MAX_FILE_CAPTION_BYTES / 2);
    e.edit_message(space.clone(), file.id.clone(), maximum.clone())
        .unwrap();
    let edited = e
        .timeline(space.clone())
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == file.id)
        .unwrap();
    caption_card(&edited, &maximum);
    let foreign = send_caption(&e, &other_space, None, false, "foreign", "Other chat").unwrap();
    e.delete_message(space.clone(), file.id.clone()).unwrap();
    for entry in [plain, miniapp, voice, foreign, file] {
        for blank in ["", " \n\t"] {
            let before = storage(&e);
            assert!(
                e.edit_message(space.clone(), entry.id.clone(), blank.into())
                    .is_err(),
                "blank edit must remain unavailable for {}",
                entry.id
            );
            assert_eq!(storage(&e), before);
        }
    }
}

pub(super) fn captioned_file_retries_survive_reopen_and_reject_caption_drift(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};
    let dir = std::env::temp_dir().join(types::new_id("caption-send")?);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("core.sqlite").to_string_lossy().to_string();
    let caption = "First caption 🐝";
    let (space, accepted, counts) = {
        let e = Client::open(path.clone(), "en".into()).unwrap();
        e.seed_demo_if_empty().unwrap();
        let space = find_space(&e, "Zoen").id;
        let accepted = send_caption(&e, &space, None, false, "durable-caption", caption).unwrap();
        (space, accepted, storage(&e))
    };
    let e = Client::open(path.clone(), "en".into()).unwrap();
    assert_eq!(
        send_caption(&e, &space, None, false, "durable-caption", caption).unwrap(),
        accepted
    );
    for changed in ["Second caption", "", "First caption 🐝 "] {
        assert!(send_caption(&e, &space, None, false, "durable-caption", changed).is_err());
        assert_eq!(storage(&e), counts);
    }
    let blank = send(&e, &space, "application/pdf", None, false, "old-blank").unwrap();
    // Seed the persisted receipt using the exact pre-caption schema, independently
    // from the new wrapper. A cold new client must recognize this old intent.
    let legacy_intent = serde_json::to_vec(&(
        &space,
        "picked.bin",
        "application/pdf",
        hex::encode(Sha256::digest(vec![7; 1024])),
        Some(hex::encode(Sha256::digest(vec![9; 64]))),
        Option::<String>::None,
        false,
    ))
    .unwrap();
    let legacy_receipt =
        serde_json::to_string(&(hex::encode(Sha256::digest(legacy_intent)), &blank.id)).unwrap();
    let legacy_key = format!("file_send:{}:old-blank", e.me().unwrap().id);
    e.lock()
        .store
        .set_meta(&legacy_key, &legacy_receipt)
        .unwrap();
    let blank_counts = storage(&e);
    drop(e);
    let e = Client::open(path, "en".into()).unwrap();
    assert_eq!(
        send_caption(&e, &space, None, false, "old-blank", " \n\t").unwrap(),
        blank
    );
    assert!(send_caption(&e, &space, None, false, "old-blank", caption).is_err());
    assert_eq!(storage(&e), blank_counts);
    drop(e);
    std::fs::remove_dir_all(dir).unwrap();

    Ok(())
}

pub(super) fn file_caption_bounds_and_failed_receipt_leave_no_partial_file_or_message() {
    let e = seeded();
    let space = find_space(&e, "Zoen").id;
    let before = storage(&e);
    let timeline = e.timeline(space.clone()).unwrap();
    for caption in [
        "é".repeat(crate::files::MAX_FILE_CAPTION_BYTES / 2 + 1),
        "bad\0caption".into(),
    ] {
        assert!(send_caption(&e, &space, None, false, "invalid-caption", &caption).is_err());
        assert_eq!(storage(&e), before);
    }
    e.lock().store.conn().execute_batch(
        "CREATE TRIGGER fail_caption BEFORE INSERT ON meta WHEN NEW.key LIKE 'file_send:%' BEGIN SELECT RAISE(ABORT,'caption receipt failed'); END;",
    ).unwrap();
    assert!(send_caption(
        &e,
        &space,
        None,
        false,
        "retry-caption",
        "Keep this caption"
    )
    .is_err());
    assert_eq!(storage(&e), before);
    assert_eq!(e.timeline(space.clone()).unwrap(), timeline);
    e.lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_caption")
        .unwrap();
    let caption = "é".repeat(crate::files::MAX_FILE_CAPTION_BYTES / 2);
    let sent = send_caption(&e, &space, None, false, "retry-caption", &caption).unwrap();
    caption_card(&sent, &caption);
    assert_eq!(e.timeline(space).unwrap().len(), timeline.len() + 1);
}
