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
    assert!(entry.voice.is_none());
    match &entry.kind {
        EntryKind::Message {
            text,
            card: Some(card),
        } => {
            assert!(text.is_empty());
            card
        }
        other => panic!("expected file attachment, got {other:?}"),
    }
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
