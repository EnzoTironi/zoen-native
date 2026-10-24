//! Complete forwarding recovery scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn missing_corrupt_or_oversized_source_audio_cannot_create_destination_items_or_keys() {
    for invalid in [
        "missing",
        "corrupt",
        "oversized_cache",
        "oversized_metadata",
    ] {
        let e = seeded();
        let source = find_space(&e, "Paraty com a Marina").id;
        let destination = find_space(&e, "Zoen").id;
        let sent = wav_note(&e, &source, &pcm_wav(8000), None, false);
        let item = sent.voice.as_ref().unwrap().item_id.clone();
        let mut document = voice_document(&e, &item);
        {
            let mut engine = e.lock();
            let sha = &document.chunks[0].sha256;
            match invalid {
                "missing" => {
                    engine
                        .store
                        .conn()
                        .execute("DELETE FROM media WHERE sha256 = ?1", [sha])
                        .unwrap();
                }
                "corrupt" => {
                    engine
                        .store
                        .conn()
                        .execute(
                            "UPDATE media SET bytes = zeroblob(?1) WHERE sha256 = ?2",
                            rusqlite::params![document.chunks[0].bytes, sha],
                        )
                        .unwrap();
                }
                "oversized_cache" => {
                    engine
                        .store
                        .conn()
                        .execute(
                            "UPDATE media SET bytes = zeroblob(1048576) WHERE sha256 = ?1",
                            [sha],
                        )
                        .unwrap();
                }
                _ => {
                    document.bytes = u64::MAX;
                    let me = engine.me_id().unwrap();
                    engine
                        .append(
                            &source,
                            &me,
                            EventBody::ItemVersioned {
                                item: item.clone(),
                                content: ItemContent::File(document),
                                note: "Authenticated invalid metadata".into(),
                            },
                        )
                        .unwrap();
                }
            }
        }
        let before = voice_forward_storage(&e);
        let timeline = e.timeline(destination.clone()).unwrap();
        let items = e.items().len();
        assert!(
            e.forward_message(source, sent.id, destination.clone())
                .is_err(),
            "{invalid}"
        );
        assert_eq!(voice_forward_storage(&e), before, "{invalid}");
        assert_eq!(e.timeline(destination).unwrap(), timeline, "{invalid}");
        assert_eq!(e.items().len(), items, "{invalid}");
    }
}

pub(super) fn failed_voice_forward_append_rolls_back_destination_file_keys_chunks_and_projection() {
    for synced in [false, true] {
        let e = seeded();
        let source = find_space(&e, "Paraty com a Marina").id;
        let destination = find_space(&e, "Zoen").id;
        let bytes = pcm_wav(8000);
        let sent = wav_note(&e, &source, &bytes, None, false);
        {
            let mut engine = e.lock();
            if synced {
                engine.store.set_synced(&destination).unwrap();
                engine.net.synced.insert(destination.clone());
            }
            let table = if synced { "outbox" } else { "events" };
            engine.store.conn().execute_batch(&format!("CREATE TRIGGER fail_voice_forward BEFORE INSERT ON {table} WHEN NEW.json LIKE '%ItemCreated%' BEGIN SELECT RAISE(ABORT,'voice forward failed'); END;")).unwrap();
        }
        let before = voice_forward_storage(&e);
        let timeline = e.timeline(destination.clone()).unwrap();
        let items = e.items().len();
        assert!(e
            .forward_message(source.clone(), sent.id.clone(), destination.clone())
            .is_err());
        assert_eq!(voice_forward_storage(&e), before, "synced={synced}");
        assert_eq!(e.timeline(destination.clone()).unwrap(), timeline);
        assert_eq!(e.items().len(), items);
        assert!(e.lock().net.pending.is_empty());
        assert_eq!(
            e.file_bytes(sent.voice.as_ref().unwrap().item_id.clone(), None)
                .unwrap(),
            Some(bytes.clone())
        );
        e.lock()
            .store
            .conn()
            .execute_batch("DROP TRIGGER fail_voice_forward")
            .unwrap();
        let forwarded = e.forward_message(source, sent.id, destination).unwrap();
        assert!(forwarded.forwarded);
        assert_eq!(
            e.file_bytes(forwarded.voice.unwrap().item_id, None)
                .unwrap(),
            Some(bytes)
        );
    }
}

pub(super) fn unresolved_legacy_voice_markers_cannot_escape_their_source_chat_as_text() {
    let e = seeded();
    let source = find_space(&e, "Paraty com a Marina").id;
    let destination = find_space(&e, "Zoen").id;
    for marker in [
        "⟦voice:missing:1000:08f⟧\nPrivate source transcript",
        "⟦voice:file:///private/recording.m4a:1000:08f⟧",
        "⟦voice:malformed⟧",
    ] {
        let sent = e.send_message(source.clone(), marker.into()).unwrap();
        assert!(sent.voice.is_none());
        let before = voice_forward_storage(&e);
        let timeline = e.timeline(destination.clone()).unwrap();
        assert!(matches!(
            e.forward_message(source.clone(), sent.id, destination.clone()),
            Err(CoreError::Invalid { .. })
        ));
        assert_eq!(voice_forward_storage(&e), before);
        assert_eq!(e.timeline(destination.clone()).unwrap(), timeline);
    }
}

pub(super) fn forwarded_voice_survives_cold_reopen_and_a_destination_only_peer_decrypts_new_chunks(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::sync::Ingest;
    use protocol::{Envelope, Sequenced};

    let dir = std::env::temp_dir().join(types::new_id("voice-forward-cold")?);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("core.sqlite").to_string_lossy().to_string();
    let bytes = pcm_wav(8000);
    let (source_item, destination, forwarded, mut relay, before) = {
        let sender = Client::open(path.clone(), "pt-BR".into()).unwrap();
        sender.seed_demo_if_empty().unwrap();
        let source = find_space(&sender, "Paraty com a Marina").id;
        let destination = find_space(&sender, "Zoen").id;
        let original = wav_note(&sender, &source, &bytes, None, false);
        let source_item = original.voice.unwrap().item_id;
        let relay = sender.lock().logs[&destination].clone();
        let before = sender.timeline(destination.clone()).unwrap().len();
        {
            let mut engine = sender.lock();
            engine.store.set_synced(&destination).unwrap();
            engine.net.synced.insert(destination.clone());
        }
        let forwarded = sender
            .forward_message(source, original.id, destination.clone())
            .unwrap();
        assert_eq!(sender.lock().store.outbox().unwrap().len(), 1);
        (source_item, destination, forwarded, relay, before)
    };
    // A real disk reopen restores the pending voice, metadata and fresh upload.
    let sender = Client::open(path, "pt-BR".into()).unwrap();
    let copied = forwarded.voice.as_ref().unwrap();
    let restored = sender.timeline(destination.clone()).unwrap();
    assert_eq!(restored.len(), before + 1);
    assert_eq!(restored.last().unwrap().id, forwarded.id);
    assert!(restored.last().unwrap().forwarded);
    assert_eq!(restored.last().unwrap().voice.as_ref(), Some(copied));
    assert_eq!(
        sender.file_bytes(copied.item_id.clone(), None).unwrap(),
        Some(bytes.clone())
    );
    let pending = sender.lock().store.outbox().unwrap();
    assert_eq!(pending.len(), 1);
    for event in pending {
        relay.sequence(event.event);
    }
    let receiver = Client::open(":memory:".into(), "en".into()).unwrap();
    for event in relay.events() {
        let envelope = Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: Envelope::plain(event),
        };
        assert!(matches!(
            receiver.lock().ingest(envelope.clone()),
            Ingest::Applied
        ));
        if event.client_id == forwarded.id {
            assert!(matches!(sender.lock().ingest(envelope), Ingest::Confirmed));
        }
    }
    assert!(!receiver.lock().state.items.contains_key(&source_item));
    let delivered = last(&receiver, &destination);
    assert_eq!(delivered.id, forwarded.id);
    assert!(delivered.forwarded);
    assert_eq!(delivered.voice.as_ref(), Some(copied));
    assert_eq!(
        receiver.file_bytes(copied.item_id.clone(), None).unwrap(),
        None
    );
    let uploads = sender.lock().pending_uploads(100);
    let wanted = receiver.lock().wanted_media();
    let document = voice_document(&receiver, &copied.item_id);
    assert_eq!(wanted.len(), document.chunks.len());
    for piece in wanted {
        assert!(document.chunks.iter().any(|chunk| chunk.blob == piece.blob));
        let encrypted = &uploads
            .iter()
            .find(|(blob, _)| blob == &piece.blob)
            .unwrap()
            .1;
        assert_ne!(encrypted, &bytes);
        assert!(receiver.lock().media_arrived(&piece, encrypted));
    }
    receiver.lock().reload().unwrap();
    sender.lock().reload().unwrap();
    assert_eq!(
        receiver.file_bytes(copied.item_id.clone(), None).unwrap(),
        Some(bytes)
    );
    assert_eq!(
        receiver.timeline(destination.clone()).unwrap().len(),
        before + 1
    );
    assert!(last(&receiver, &destination).forwarded);
    assert_eq!(
        sender.timeline(destination.clone()).unwrap().len(),
        before + 1
    );
    assert!(receiver.verify_log(destination).valid);
    drop(sender);
    std::fs::remove_dir_all(dir).unwrap();

    Ok(())
}
