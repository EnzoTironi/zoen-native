//! Ordinary attachments must be real chat messages, without changing library imports.
use super::*;
use roda_types::{EventBody, Role};

fn send(
    e: &RodaEngine,
    space: &str,
    mime: &str,
    reply: Option<String>,
    thread: bool,
    id: &str,
) -> Result<TimelineEntry, CoreError> {
    e.send_file_message(
        space.into(),
        "picked.bin".into(),
        mime.into(),
        vec![7; 1024],
        Some(vec![9; 64]),
        reply,
        thread,
        id.into(),
    )
}

fn card(entry: &TimelineEntry) -> &ItemCard {
    caption_card(entry, "")
}

fn caption_card<'a>(entry: &'a TimelineEntry, caption: &str) -> &'a ItemCard {
    assert!(entry.voice.is_none());
    match &entry.kind {
        EntryKind::Message {
            text,
            card: Some(card),
        } => {
            assert_eq!(text, caption);
            card
        }
        other => panic!("expected file attachment, got {other:?}"),
    }
}

fn send_caption(
    e: &RodaEngine,
    space: &str,
    reply: Option<String>,
    thread: bool,
    id: &str,
    caption: &str,
) -> Result<TimelineEntry, CoreError> {
    e.send_file_message_with_caption(
        space.into(),
        "picked.bin".into(),
        "application/pdf".into(),
        vec![7; 1024],
        Some(vec![9; 64]),
        reply,
        thread,
        id.into(),
        caption.into(),
    )
}

fn storage(e: &RodaEngine) -> Vec<i64> {
    let engine = e.lock();
    [
        "events",
        "meta",
        "media",
        "media_keys",
        "blob_uploads",
        "outbox",
    ]
    .map(|table| {
        engine
            .store
            .conn()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    })
    .to_vec()
}

#[test]
fn ordinary_files_are_actual_messages_and_library_imports_stay_quiet() {
    let e = seeded();
    let space = find_space(&e, "Zoen").id;
    let before = e.timeline(space.clone()).unwrap().len();
    e.file_add(
        space.clone(),
        "library.pdf".into(),
        "library.pdf".into(),
        "application/pdf".into(),
        vec![1; 50],
        None,
    )
    .unwrap();
    assert_eq!(e.timeline(space.clone()).unwrap().len(), before);
    for (i, mime) in ["image/jpeg", "video/mp4", "application/pdf", "audio/mp4"]
        .iter()
        .enumerate()
    {
        let sent = send(&e, &space, mime, None, false, &format!("send-{i}")).unwrap();
        let file = card(&sent);
        assert_eq!(file.kind_id, "file");
        assert_eq!(
            e.file_bytes(file.item_id.clone(), None).unwrap(),
            Some(vec![7; 1024])
        );
        assert_eq!(
            e.file_info_at(file.item_id.clone(), None).unwrap().mime,
            *mime
        );
        assert_eq!(
            e.timeline(space.clone()).unwrap().last().unwrap().id,
            sent.id
        );
        let engine = e.lock();
        let events = engine.logs[&space].events();
        assert_eq!(events.last().unwrap().client_id, sent.id);
        assert!(
            matches!(&events[events.len()-2].body, EventBody::ItemCreated { item, .. } if item == &file.item_id)
        );
        assert!(
            matches!(&events.last().unwrap().body, EventBody::MessagePosted { attaches: Some(item), .. } if item == &file.item_id)
        );
    }
    assert_eq!(e.timeline(space.clone()).unwrap().len(), before + 4);
    assert!(e.verify_log(space).valid);
}

#[tokio::test]
async fn file_replies_preserve_inline_and_normalized_thread_destinations() {
    let e = seeded();
    let space = find_space(&e, "Zoen").id;
    let root = e.send_message(space.clone(), "Root".into()).unwrap();
    let inline = send(
        &e,
        &space,
        "image/jpeg",
        Some(root.id.clone()),
        false,
        "inline",
    )
    .unwrap();
    assert_eq!(inline.reply_to.unwrap().id, root.id);
    let child = e
        .send_reply(space.clone(), "Child".into(), root.id.clone(), true)
        .unwrap();
    let sent = send(&e, &space, "video/mp4", Some(child.id), true, "thread").unwrap();
    assert_eq!(sent.in_thread.as_deref(), Some(root.id.as_str()));
    // The legacy full timeline includes thread children. Exercise the actual Expo
    // client boundary, which intentionally excludes them from the main page.
    let page: serde_json::Value = serde_json::from_str(
        &e.client_request(
            serde_json::json!({"op":"timeline", "space_id":space, "limit":200}).to_string(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert!(page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["id"] != sent.id));
    assert_eq!(
        e.thread(space.clone(), root.id.clone())
            .unwrap()
            .last()
            .unwrap()
            .id,
        sent.id
    );
    e.lock().reload().unwrap();
    assert_eq!(
        e.thread(space, root.id).unwrap().last().unwrap().id,
        sent.id
    );
}

#[test]
fn invalid_or_foreign_targets_and_nonwriters_leave_no_file_or_key_material() {
    let e = seeded();
    let space = find_space(&e, "Zoen").id;
    let other = find_space(&e, "Paraty com a Marina").id;
    let foreign = e.send_message(other, "Not in destination".into()).unwrap();
    for (reply, thread) in [
        (None, true),
        (Some("missing".into()), false),
        (Some(foreign.id), true),
    ] {
        let before = storage(&e);
        assert!(send(&e, &space, "image/jpeg", reply, thread, "invalid").is_err());
        assert_eq!(storage(&e), before);
    }
    let me = e.me().unwrap().id;
    e.lock()
        .append(
            &space,
            &me,
            EventBody::MemberAdded {
                identity: me.clone(),
                role: Role::Reader,
            },
        )
        .unwrap();
    let before = storage(&e);
    assert!(matches!(
        send(&e, &space, "image/jpeg", None, false, "reader"),
        Err(CoreError::Forbidden { .. })
    ));
    assert_eq!(storage(&e), before);
}

#[test]
fn failed_message_or_receipt_write_rolls_back_files_keys_and_projection() {
    for synced in [false, true] {
        for receipt in [false, true] {
            let e = seeded();
            let space = find_space(&e, "Zoen").id;
            {
                let mut engine = e.lock();
                if synced {
                    engine.store.set_synced(&space).unwrap();
                    engine.net.synced.insert(space.clone());
                }
                let sql = if receipt {
                    "CREATE TRIGGER fail_send BEFORE INSERT ON meta WHEN NEW.key LIKE 'file_send:%' BEGIN SELECT RAISE(ABORT,'receipt failed'); END;"
                } else if synced {
                    "CREATE TRIGGER fail_send BEFORE INSERT ON outbox WHEN NEW.json LIKE '%MessagePosted%' BEGIN SELECT RAISE(ABORT,'message failed'); END;"
                } else {
                    "CREATE TRIGGER fail_send BEFORE INSERT ON events WHEN NEW.json LIKE '%MessagePosted%' BEGIN SELECT RAISE(ABORT,'message failed'); END;"
                };
                engine.store.conn().execute_batch(sql).unwrap();
            }
            let before = storage(&e);
            let items = e.items().len();
            let timeline = e.timeline(space.clone()).unwrap();
            assert!(send(&e, &space, "image/jpeg", None, false, "retry").is_err());
            assert_eq!(storage(&e), before, "synced={synced}, receipt={receipt}");
            assert_eq!(e.items().len(), items);
            assert_eq!(e.timeline(space.clone()).unwrap(), timeline);
            assert!(e.lock().net.pending.is_empty());
            e.lock()
                .store
                .conn()
                .execute_batch("DROP TRIGGER fail_send")
                .unwrap();
            let sent = send(&e, &space, "image/jpeg", None, false, "retry").unwrap();
            assert_eq!(
                e.file_bytes(card(&sent).item_id.clone(), None).unwrap(),
                Some(vec![7; 1024])
            );
        }
    }
}

#[test]
fn exact_file_send_retries_survive_cold_reopen_and_reject_intent_drift() {
    let dir = std::env::temp_dir().join(roda_types::new_id("file-send"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("core.sqlite").to_string_lossy().to_string();
    let (space, id, item, counts) = {
        let e = RodaEngine::open(path.clone(), "en".into()).unwrap();
        e.seed_demo_if_empty().unwrap();
        let space = find_space(&e, "Zoen").id;
        let sent = send(&e, &space, "image/jpeg", None, false, "durable").unwrap();
        let counts = storage(&e);
        assert_eq!(
            send(&e, &space, "image/jpeg", None, false, "durable")
                .unwrap()
                .id,
            sent.id
        );
        assert_eq!(storage(&e), counts);
        (space, sent.id.clone(), card(&sent).item_id.clone(), counts)
    };
    let e = RodaEngine::open(path, "en".into()).unwrap();
    let sent = send(&e, &space, "image/jpeg", None, false, "durable").unwrap();
    assert_eq!(sent.id, id);
    assert_eq!(card(&sent).item_id, item);
    assert_eq!(storage(&e), counts);
    for drift in 0..6 {
        let mut name = "picked.bin".to_string();
        let mut mime = "image/jpeg".to_string();
        let mut bytes = vec![7; 1024];
        let mut thumb = Some(vec![9; 64]);
        let mut reply = None;
        let mut thread = false;
        match drift {
            0 => name = "other.jpg".into(),
            1 => mime = "video/mp4".into(),
            2 => bytes[0] = 8,
            3 => thumb = None,
            4 => reply = Some(id.clone()),
            _ => {
                reply = Some(id.clone());
                thread = true;
            }
        }
        assert!(e
            .send_file_message(
                space.clone(),
                name,
                mime,
                bytes,
                thumb,
                reply,
                thread,
                "durable".into()
            )
            .is_err());
        assert_eq!(storage(&e), counts);
    }
    drop(e);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn file_caption_is_signed_with_its_attachment_and_thread_before_receiver_replay() {
    use crate::sync::Ingest;
    use roda_proto::{Envelope, Sequenced};
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
    let receiver = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
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

#[test]
fn clearing_file_caption_through_public_edit_reaches_signed_recipient_without_changing_file() {
    use crate::sync::Ingest;
    use roda_proto::{Envelope, Sequenced};
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
        let engine = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
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
    let mut relay = roda_log::SpaceLog::new(space.clone());
    let mut sequenced = HashSet::new();
    let mut catch_up = || {
        for pending in sender.lock().store.outbox().unwrap() {
            if pending.event.space == space && sequenced.insert(pending.event.client_id.clone()) {
                let event = relay.sequence(pending.event);
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
    };
    catch_up();
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
        catch_up();
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
}

#[test]
fn blank_caption_edit_is_scoped_to_live_own_ordinary_files_and_enforces_caption_bounds() {
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

#[test]
fn captioned_file_retries_survive_reopen_and_reject_caption_drift() {
    use sha2::{Digest, Sha256};
    let dir = std::env::temp_dir().join(roda_types::new_id("caption-send"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("core.sqlite").to_string_lossy().to_string();
    let caption = "First caption 🐝";
    let (space, accepted, counts) = {
        let e = RodaEngine::open(path.clone(), "en".into()).unwrap();
        e.seed_demo_if_empty().unwrap();
        let space = find_space(&e, "Zoen").id;
        let accepted = send_caption(&e, &space, None, false, "durable-caption", caption).unwrap();
        (space, accepted, storage(&e))
    };
    let e = RodaEngine::open(path.clone(), "en".into()).unwrap();
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
    let e = RodaEngine::open(path, "en".into()).unwrap();
    assert_eq!(
        send_caption(&e, &space, None, false, "old-blank", " \n\t").unwrap(),
        blank
    );
    assert!(send_caption(&e, &space, None, false, "old-blank", caption).is_err());
    assert_eq!(storage(&e), blank_counts);
    drop(e);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn file_caption_bounds_and_failed_receipt_leave_no_partial_file_or_message() {
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

#[test]
fn file_message_sync_delivers_the_original_attachment_and_encrypted_chunks() {
    use crate::sync::Ingest;
    use roda_proto::{Envelope, Sequenced};
    let sender = seeded();
    let space = find_space(&sender, "Zoen").id;
    let root = sender.send_message(space.clone(), "Root".into()).unwrap();
    let mut relay = sender.lock().logs[&space].clone();
    sender.lock().net.synced.insert(space.clone());
    let sent = send(
        &sender,
        &space,
        "video/mp4",
        Some(root.id.clone()),
        true,
        "encrypted",
    )
    .unwrap();
    let item = card(&sent).item_id.clone();
    let pending = sender.lock().store.outbox().unwrap();
    assert_eq!(pending.len(), 2);
    for event in pending {
        relay.sequence(event.event);
    }
    let receiver = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
    for event in relay.events() {
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
    assert_eq!(received.last().unwrap().id, sent.id);
    assert_eq!(card(received.last().unwrap()).item_id, item);
    assert_eq!(receiver.file_bytes(item.clone(), None).unwrap(), None);
    let uploads = sender.lock().pending_uploads(10);
    let wanted = receiver.lock().wanted_media();
    assert!(!wanted.is_empty());
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
        receiver.file_bytes(item, None).unwrap(),
        Some(vec![7; 1024])
    );
    assert!(receiver.verify_log(space).valid);
}

fn file_document(e: &RodaEngine, item: &str) -> roda_types::FileDoc {
    let engine = e.lock();
    let roda_types::ItemContent::File(doc) = &engine.state.items[item].current().content else {
        panic!("expected a file document");
    };
    doc.clone()
}

#[test]
fn cross_chat_media_forwarding_copies_current_files_without_source_bindings() {
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

#[test]
fn destination_only_receiver_decrypts_forwarded_media_and_thumbnail() {
    use crate::sync::Ingest;
    use roda_proto::{Envelope, Sequenced};
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
    let receiver = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
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

#[test]
fn unavailable_or_corrupt_forwarded_media_does_not_write_a_partial_copy() {
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

#[test]
fn failed_forwarded_file_or_message_write_rolls_back_every_copy_and_can_retry() {
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

#[test]
fn same_chat_file_forwarding_keeps_the_existing_attachment_and_foreign_items_fail_cleanly() {
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

#[test]
fn forwarding_an_ordinary_audio_filename_does_not_create_a_second_voice_message() {
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
