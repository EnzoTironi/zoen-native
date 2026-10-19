//! Voice notes travel as encrypted audio files (the chunked media path), not text markers.
use super::*;
use types::{EventBody, ItemContent, Role};

// An untrusted relay may sequence a correctly signed but unauthorized event.
fn ingest_as(engine: &mut crate::engine::Engine, space: &str, author: &str, body: EventBody) {
    let event = engine.author_for(author).unwrap().sign_event(
        space,
        &types::new_id("voice-event"),
        crate::engine::now_ms(),
        engine.logs[space].head(),
        body,
    );
    let mut relay = engine.logs[space].clone();
    let event = relay.sequence(event);
    assert_eq!(
        engine.ingest(protocol::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: protocol::Envelope::plain(event),
        }),
        crate::sync::Ingest::Applied
    );
}

fn note(e: &Client, space: &str, reply: Option<String>) -> Result<TimelineEntry, CoreError> {
    e.send_voice_note(
        space.into(),
        vec![7; 900],
        "audio/mp4".into(),
        1500,
        vec![0.2],
        "A recording".into(),
        reply,
        true,
    )
}

fn last(e: &Client, space: &str) -> TimelineEntry {
    e.timeline(space.into()).unwrap().pop().unwrap()
}

/// A complete mono PCM WAV, so forwarding tests carry a decodable audio container.
fn pcm_wav(frames: u32) -> Vec<u8> {
    let length = frames * 2;
    let mut wav = Vec::with_capacity(length as usize + 44);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + length).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
    wav.extend_from_slice(&8000u32.to_le_bytes());
    wav.extend_from_slice(&16000u32.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&length.to_le_bytes());
    for index in 0..frames {
        let sample = ((index % 80) as i16 - 40) * 400;
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    assert_eq!(wav.len(), length as usize + 44);
    wav
}

fn wav_note(
    e: &Client,
    space: &str,
    bytes: &[u8],
    reply: Option<String>,
    thread: bool,
) -> TimelineEntry {
    e.send_voice_note(
        space.into(),
        bytes.to_vec(),
        "audio/wav".into(),
        ((bytes.len() - 44) / 16) as u64,
        vec![0.0, 0.5, 1.0],
        "Synthetic audio transcript".into(),
        reply,
        thread,
    )
    .unwrap()
}

fn voice_document(e: &Client, item: &str) -> types::FileDoc {
    let engine = e.lock();
    let ItemContent::File(document) = &engine.state.items[item].current().content else {
        panic!("voice file")
    };
    document.clone()
}

fn voice_forward_storage(e: &Client) -> Vec<i64> {
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
fn a_voice_note_is_an_encrypted_audio_file_with_its_waveform() {
    let e = seeded();
    let space = find_space(&e, "Paraty com a Marina").id;
    let audio: Vec<u8> = (0..40_000u32).map(|i| (i % 251) as u8).collect();
    let sent = e
        .send_voice_note(
            space.clone(),
            audio.clone(),
            "audio/mp4".into(),
            7012,
            vec![0.0, 0.5, 1.0],
            "levo o fogareiro".into(),
            None,
            false,
        )
        .unwrap();
    let voice = sent.voice.clone().expect("a voice note");
    assert_eq!(voice.ms, 7012);
    assert_eq!(voice.mime, "audio/mp4");
    assert_eq!(voice.transcript, "levo o fogareiro");
    assert_eq!(voice.levels.len(), 3);
    assert!((voice.levels[1] - 0.533).abs() < 0.01 && voice.levels[2] == 1.0);
    assert!(matches!(&sent.kind, EntryKind::Message { text, card: None } if text.is_empty()));
    assert_eq!(last(&e, &space).id, sent.id);
    // The bytes are a real file: sealed in chunks, readable back by item id.
    assert_eq!(
        e.file_bytes(voice.item_id.clone(), None).unwrap(),
        Some(audio)
    );
    // No marker on the wire.
    assert!(!e
        .lock()
        .state
        .spaces[&space]
        .entries
        .iter()
        .any(|x| matches!(&x.body, crate::engine::EntryBody::Message { text, .. } if text.starts_with("⟦voice:"))));
    let s = find_space(&e, "Paraty com a Marina");
    assert!(
        s.last_preview.contains("Voice message") || s.last_preview.contains("Mensagem de voz"),
        "{}",
        s.last_preview
    );
    // People react to it and the author deletes it like any message.
    e.react(space.clone(), sent.id.clone(), Some("👍".into()))
        .unwrap();
    assert_eq!(last(&e, &space).reactions.len(), 1);
    e.delete_message(space.clone(), sent.id.clone()).unwrap();
    let gone = last(&e, &space);
    assert!(gone.deleted && gone.voice.is_none());
}

#[test]
fn a_voice_reply_shows_once_with_its_quote_and_threads() {
    let e = seeded();
    let space = find_space(&e, "Paraty com a Marina").id;
    let root = e
        .send_message(space.clone(), "Quem leva o fogareiro?".into())
        .unwrap();
    let before = e.timeline(space.clone()).unwrap().len();
    let reply = e
        .send_voice_note(
            space.clone(),
            vec![7; 900],
            "audio/mp4".into(),
            1500,
            vec![0.2],
            String::new(),
            Some(root.id.clone()),
            false,
        )
        .unwrap();
    let tl = e.timeline(space.clone()).unwrap();
    assert_eq!(
        tl.len(),
        before + 1,
        "one entry, not the file plus the reply"
    );
    assert_eq!(
        reply.reply_to.as_ref().map(|q| q.id.as_str()),
        Some(root.id.as_str())
    );
    assert!(reply.voice.is_some());
    let threaded = e
        .send_voice_note(
            space.clone(),
            vec![8; 900],
            "audio/mp4".into(),
            900,
            vec![],
            String::new(),
            Some(root.id.clone()),
            true,
        )
        .unwrap();
    assert_eq!(threaded.in_thread.as_deref(), Some(root.id.as_str()));
    let thread = e.thread(space.clone(), root.id.clone()).unwrap();
    assert!(thread
        .iter()
        .any(|t| t.id == threaded.id && t.voice.is_some()));
}

#[test]
fn web_recordings_play_and_other_audio_files_stay_files() {
    let e = seeded();
    let space = find_space(&e, "Paraty com a Marina").id;
    let before = e.timeline(space.clone()).unwrap().len();
    // A plain audio file added to the Space is not a voice note.
    e.file_add(
        space.clone(),
        "musica/trilha.mp3".into(),
        "trilha.mp3".into(),
        "audio/mpeg".into(),
        vec![1; 64],
        None,
    )
    .unwrap();
    assert_eq!(e.timeline(space.clone()).unwrap().len(), before);
    // Web builds before the tag: "Mensagem de voz.webm", no length or waveform.
    let web = e
        .file_add(
            space.clone(),
            String::new(),
            "Mensagem de voz.webm".into(),
            "audio/webm".into(),
            vec![2; 64],
            None,
        )
        .unwrap();
    let entry = last(&e, &space);
    let voice = entry.voice.expect("shown as a voice note");
    assert_eq!(voice.item_id, web.id);
    assert_eq!((voice.ms, voice.levels.len()), (0, 0));
}

#[test]
fn only_audio_is_a_voice_note() {
    let e = seeded();
    let space = find_space(&e, "Paraty com a Marina").id;
    assert!(e
        .send_voice_note(
            space,
            vec![1; 10],
            "image/png".into(),
            1,
            vec![],
            String::new(),
            None,
            false
        )
        .is_err());
}

#[test]
fn forwarding_in_the_same_chat_keeps_the_original_and_its_reactions() {
    let e = seeded();
    let space = find_space(&e, "Paraty com a Marina").id;
    let original = note(&e, &space, None).unwrap();
    e.react(space.clone(), original.id.clone(), Some("👍".into()))
        .unwrap();
    for _ in 0..2 {
        let forwarded = e.lock().forward(&space, &original.id, &space).unwrap();
        assert!(forwarded.forwarded && forwarded.voice == original.voice);
    }
    // Both live projection and replay must retain all three entries.
    e.lock().reload().unwrap();
    let timeline = e.timeline(space).unwrap();
    let kept = timeline.iter().find(|x| x.id == original.id).unwrap();
    assert_eq!(kept.reactions.len(), 1);
    assert_eq!(
        timeline
            .iter()
            .filter(|x| x.voice == original.voice)
            .count(),
        3
    );
}

#[test]
fn cross_space_voice_forwarding_copies_playable_bytes_with_fresh_destination_keys_once() {
    let e = seeded();
    let source = find_space(&e, "Paraty com a Marina").id;
    let destination = find_space(&e, "Zoen").id;
    let bytes = pcm_wav(8000);
    let sent = wav_note(&e, &source, &bytes, None, false);
    let original = sent.voice.as_ref().unwrap();
    let original_doc = voice_document(&e, &original.item_id);
    let before = e.timeline(destination.clone()).unwrap().len();
    let forwarded = e
        .forward_message(source.clone(), sent.id.clone(), destination.clone())
        .unwrap();
    let copied = forwarded.voice.as_ref().expect("a playable voice message");
    assert!(forwarded.forwarded);
    assert!(!forwarded.deleted && !forwarded.edited);
    assert!(forwarded.reactions.is_empty());
    assert!(forwarded.reply_to.is_none() && forwarded.in_thread.is_none());
    assert_ne!(forwarded.id, sent.id);
    assert_ne!(copied.item_id, original.item_id);
    assert_eq!(copied.sha256, original.sha256);
    assert_eq!(copied.ms, original.ms);
    assert_eq!(copied.levels, original.levels);
    assert_eq!(copied.transcript, original.transcript);
    assert_eq!(copied.mime, "audio/wav");
    assert_eq!(
        e.file_bytes(copied.item_id.clone(), None).unwrap(),
        Some(bytes)
    );
    assert!(matches!(&forwarded.kind, EntryKind::Message { text, card: None } if text.is_empty()));
    assert_eq!(e.timeline(destination.clone()).unwrap().len(), before + 1);
    let copied_doc = voice_document(&e, &copied.item_id);
    assert_eq!(copied_doc.sha256, original_doc.sha256);
    assert_eq!(copied_doc.chunks.len(), original_doc.chunks.len());
    for (original, copied) in original_doc.chunks.iter().zip(&copied_doc.chunks) {
        assert_eq!(copied.sha256, original.sha256);
        assert_eq!(copied.bytes, original.bytes);
        assert_ne!(
            copied.key, original.key,
            "destination uses a fresh file key"
        );
        assert_ne!(
            copied.blob, original.blob,
            "source ciphertext is not reused"
        );
    }
    let engine = e.lock();
    assert_eq!(engine.state.items[&copied.item_id].space, destination);
    assert_ne!(
        engine
            .store
            .meta(&format!("file_key:{}", original.item_id))
            .unwrap(),
        engine
            .store
            .meta(&format!("file_key:{}", copied.item_id))
            .unwrap(),
    );
    let event = engine.logs[&destination].events().last().unwrap().clone();
    assert!(matches!(&event.body, EventBody::ItemCreated { item, .. } if item == &copied.item_id));
    let wire = serde_json::to_string(&event.body).unwrap();
    assert!(!wire.contains(&original.item_id));
    assert!(serde_json::to_value(copied_doc.voice.unwrap()).unwrap()["forwarded"] == true);
    drop(engine);
    assert_eq!(last(&e, &source).id, sent.id);
    e.lock().reload().unwrap();
    let timeline = e.timeline(destination).unwrap();
    assert_eq!(timeline.len(), before + 1);
    assert_eq!(timeline.last().unwrap().id, forwarded.id);
    assert!(timeline.last().unwrap().forwarded);
}

#[test]
fn forwarded_voice_uses_current_bytes_and_cleared_metadata_and_is_independent() {
    let e = seeded();
    let source = find_space(&e, "Paraty com a Marina").id;
    let destination = find_space(&e, "Zoen").id;
    let original_bytes = pcm_wav(8000);
    let sent = wav_note(&e, &source, &original_bytes, None, false);
    let source_item = sent.voice.as_ref().unwrap().item_id.clone();
    let edited_bytes = pcm_wav(4000);
    e.file_new_version_typed(
        source_item.clone(),
        edited_bytes.clone(),
        None,
        "Source trim".into(),
        "private-source-name.wav".into(),
        "audio/wav".into(),
    )
    .unwrap();
    assert_eq!(e.item(source_item.clone()).unwrap().version, 2);
    let current = e
        .timeline(source.clone())
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == sent.id)
        .unwrap()
        .voice
        .unwrap();
    assert_eq!(current.ms, 0);
    assert!(current.levels.is_empty() && current.transcript.is_empty());
    let forwarded = e
        .forward_message(source.clone(), sent.id.clone(), destination.clone())
        .unwrap();
    let copied = forwarded.voice.as_ref().unwrap();
    assert_eq!(copied.sha256, current.sha256);
    assert_eq!(copied.ms, 0);
    assert!(copied.levels.is_empty() && copied.transcript.is_empty());
    assert_eq!(
        e.file_bytes(copied.item_id.clone(), None).unwrap(),
        Some(edited_bytes.clone())
    );
    assert_ne!(
        voice_document(&e, &copied.item_id).name,
        "private-source-name.wav"
    );
    // Later changes in either Space must not edit the other audience's copy.
    e.file_new_version(
        source_item.clone(),
        pcm_wav(2000),
        None,
        "Source edit again".into(),
    )
    .unwrap();
    assert_eq!(
        e.file_bytes(copied.item_id.clone(), None).unwrap(),
        Some(edited_bytes)
    );
    let destination_bytes = pcm_wav(1000);
    e.file_new_version(
        copied.item_id.clone(),
        destination_bytes.clone(),
        None,
        "Destination trim".into(),
    )
    .unwrap();
    assert_eq!(
        e.file_bytes(source_item.clone(), None).unwrap(),
        Some(pcm_wav(2000))
    );
    e.delete_message(source.clone(), sent.id.clone()).unwrap();
    e.lock().reload().unwrap();
    let retained = e
        .timeline(destination.clone())
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == forwarded.id)
        .unwrap();
    assert_eq!(retained.id, forwarded.id);
    assert!(retained.forwarded && !retained.deleted);
    assert_eq!(
        e.file_bytes(copied.item_id.clone(), None).unwrap(),
        Some(destination_bytes)
    );
    assert!(
        e.timeline(source)
            .unwrap()
            .into_iter()
            .find(|entry| entry.id == sent.id)
            .unwrap()
            .deleted
    );
}

#[test]
fn cross_space_voice_forwarding_strips_inline_thread_and_legacy_marker_destinations() {
    for kind in ["inline", "thread", "android", "web"] {
        let e = seeded();
        let source = find_space(&e, "Paraty com a Marina").id;
        let destination = find_space(&e, "Zoen").id;
        let root = e
            .send_message(source.clone(), "Private source reply root".into())
            .unwrap();
        let bytes = pcm_wav(8000);
        let sent = match kind {
            "inline" | "thread" => {
                wav_note(&e, &source, &bytes, Some(root.id.clone()), kind == "thread")
            }
            "android" => {
                let file = e
                    .file_add(
                        source.clone(),
                        "VoiceNotes/old.wav".into(),
                        "old.wav".into(),
                        "audio/wav".into(),
                        bytes.clone(),
                        None,
                    )
                    .unwrap();
                e.send_message(
                    source.clone(),
                    format!("⟦voice:{}:1000:08f⟧\nOld transcript", file.id),
                )
                .unwrap()
            }
            _ => {
                e.file_add(
                    source.clone(),
                    String::new(),
                    "Mensagem de voz.wav".into(),
                    "audio/wav".into(),
                    bytes.clone(),
                    None,
                )
                .unwrap();
                last(&e, &source)
            }
        };
        let original = sent
            .voice
            .as_ref()
            .expect("canonical legacy voice detection");
        let before = e.timeline(destination.clone()).unwrap().len();
        let forwarded = e
            .forward_message(source.clone(), sent.id, destination.clone())
            .unwrap();
        let copied = forwarded.voice.as_ref().unwrap();
        assert!(forwarded.forwarded, "{kind}");
        assert!(
            forwarded.reply_to.is_none() && forwarded.in_thread.is_none(),
            "{kind}"
        );
        assert_ne!(copied.item_id, original.item_id);
        assert_eq!(copied.ms, original.ms, "{kind}");
        assert_eq!(copied.levels, original.levels, "{kind}");
        assert_eq!(copied.transcript, original.transcript, "{kind}");
        assert_eq!(
            e.file_bytes(copied.item_id.clone(), None).unwrap(),
            Some(bytes)
        );
        assert!(voice_document(&e, &copied.item_id)
            .voice
            .unwrap()
            .reply
            .is_none());
        assert!(
            matches!(&forwarded.kind, EntryKind::Message { text, card: None } if text.is_empty())
        );
        let engine = e.lock();
        let wire = serde_json::to_string(&engine.logs[&destination].events().last().unwrap().body)
            .unwrap();
        assert!(
            !wire.contains(&root.id) && !wire.contains(&original.item_id),
            "{kind}"
        );
        drop(engine);
        e.lock().reload().unwrap();
        let timeline = e.timeline(destination).unwrap();
        assert_eq!(timeline.len(), before + 1, "{kind}");
        assert_eq!(timeline.last().unwrap().id, forwarded.id);
        assert!(timeline.last().unwrap().forwarded);
    }
}

#[test]
fn voice_forwarding_requires_source_access_and_destination_writer_without_leaking_material() {
    for denied in ["source_removed", "target_reader", "target_removed"] {
        let e = seeded();
        let source = find_space(&e, "Paraty com a Marina").id;
        let destination = find_space(&e, "Zoen").id;
        let sent = wav_note(&e, &source, &pcm_wav(8000), None, false);
        let me = e.me().unwrap().id;
        let (space, body) = match denied {
            "source_removed" => (
                &source,
                EventBody::MemberRemoved {
                    identity: me.clone(),
                },
            ),
            "target_reader" => (
                &destination,
                EventBody::MemberAdded {
                    identity: me.clone(),
                    role: Role::Reader,
                },
            ),
            _ => (
                &destination,
                EventBody::MemberRemoved {
                    identity: me.clone(),
                },
            ),
        };
        e.lock().append(space, &me, body).unwrap();
        let before = voice_forward_storage(&e);
        let items = e.items().len();
        assert!(
            matches!(
                e.forward_message(source, sent.id, destination),
                Err(CoreError::Forbidden { .. })
            ),
            "{denied}"
        );
        assert_eq!(voice_forward_storage(&e), before, "{denied}");
        assert_eq!(e.items().len(), items, "{denied}");
    }
    // Reading a source is sufficient; forwarding does not mutate its Item.
    let e = seeded();
    let source = find_space(&e, "Paraty com a Marina").id;
    let destination = find_space(&e, "Zoen").id;
    let sent = wav_note(&e, &source, &pcm_wav(8000), None, false);
    let me = e.me().unwrap().id;
    e.lock()
        .append(
            &source,
            &me,
            EventBody::MemberAdded {
                identity: me.clone(),
                role: Role::Reader,
            },
        )
        .unwrap();
    assert!(e
        .forward_message(source, sent.id, destination)
        .unwrap()
        .voice
        .is_some());
}

#[test]
fn missing_corrupt_or_oversized_source_audio_cannot_create_destination_items_or_keys() {
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

#[test]
fn failed_voice_forward_append_rolls_back_destination_file_keys_chunks_and_projection() {
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

#[test]
fn historical_voice_metadata_keeps_its_serialized_shape_and_defaults_to_not_forwarded() {
    let old = r#"{"ms":1500,"levels":"08f","transcript":"Original"}"#;
    let metadata: types::VoiceMeta = serde_json::from_str(old).unwrap();
    assert_eq!(serde_json::to_string(&metadata).unwrap(), old);
    let e = seeded();
    let source = find_space(&e, "Paraty com a Marina").id;
    let original = note(&e, &source, None).unwrap();
    assert!(!original.forwarded);
    e.lock().reload().unwrap();
    assert!(!last(&e, &source).forwarded);
}

#[test]
fn unresolved_legacy_voice_markers_cannot_escape_their_source_chat_as_text() {
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

#[test]
fn forwarded_voice_survives_cold_reopen_and_a_destination_only_peer_decrypts_new_chunks() {
    use crate::sync::Ingest;
    use protocol::{Envelope, Sequenced};

    let dir = std::env::temp_dir().join(types::new_id("voice-forward-cold"));
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
}

#[test]
fn another_author_or_a_removed_author_cannot_deduplicate_an_earlier_note() {
    let e = seeded();
    let space = find_space(&e, "Paraty com a Marina");
    let root = e.send_message(space.id.clone(), "Root".into()).unwrap();
    let original = note(&e, &space.id, None).unwrap();
    let me = e.me().unwrap().id;
    let other = space.members.iter().find(|p| !p.is_me).unwrap().id.clone();
    let item = original.voice.as_ref().unwrap().item_id.clone();
    {
        let mut engine = e.lock();
        engine
            .reply_with(&space.id, &other, "", Some(item.clone()), &root.id, true)
            .unwrap();
        engine
            .append(
                &space.id,
                &me,
                EventBody::MemberRemoved {
                    identity: me.clone(),
                },
            )
            .unwrap();
        // Even a signed message placed in the log by an untrusted relay cannot remove
        // the former member's old note. Projection must check membership itself.
        engine
            .append(
                &space.id,
                &me,
                EventBody::MessagePosted {
                    message: types::new_id("msg"),
                    text: String::new(),
                    attaches: Some(item),
                    reply: Some(types::ReplyRef::thread(root.id)),
                    forwarded: false,
                },
            )
            .unwrap();
        engine.reload().unwrap();
    }
    assert!(e
        .timeline(space.id)
        .unwrap()
        .iter()
        .any(|x| x.id == original.id));
}

#[test]
fn invalid_voice_reply_targets_do_not_publish_audio_or_create_items() {
    let e = seeded();
    let space = find_space(&e, "Paraty com a Marina").id;
    let elsewhere = find_space(&e, "Zoen").id;
    let foreign_root = e.send_message(elsewhere, "Elsewhere".into()).unwrap();
    let non_message = e
        .timeline(space.clone())
        .unwrap()
        .into_iter()
        .find(|x| !matches!(x.kind, EntryKind::Message { .. }))
        .unwrap();
    let before = e.stats().events;
    let items = e.items().len();
    for target in ["missing".to_string(), foreign_root.id, non_message.id] {
        assert!(note(&e, &space, Some(target)).is_err());
        assert_eq!(e.stats().events, before);
        assert_eq!(e.items().len(), items);
    }
}

#[test]
fn only_writers_can_send_voice_notes() {
    for role in [Some(Role::Reader), None] {
        let e = seeded();
        let space = find_space(&e, "Paraty com a Marina").id;
        let me = e.me().unwrap().id;
        let body = match role {
            Some(role) => EventBody::MemberAdded {
                identity: me.clone(),
                role,
            },
            None => EventBody::MemberRemoved {
                identity: me.clone(),
            },
        };
        e.lock().append(&space, &me, body).unwrap();
        let before = e.stats().events;
        let items = e.items().len();
        assert!(matches!(
            note(&e, &space, None),
            Err(CoreError::Forbidden { .. })
        ));
        assert_eq!(e.stats().events, before);
        assert_eq!(e.items().len(), items);
    }
}

#[test]
fn renaming_voice_audio_keeps_metadata_but_replacing_bytes_invalidates_it() {
    let e = seeded();
    let space = find_space(&e, "Zoen").id;
    let sent = note(&e, &space, None).unwrap();
    let voice = sent.voice.as_ref().unwrap();
    e.file_new_version_typed(
        voice.item_id.clone(),
        vec![7; 900],
        None,
        "Rename".into(),
        "renamed.m4a".into(),
        "audio/mp4".into(),
    )
    .unwrap();
    assert_eq!(
        e.timeline(space.clone())
            .unwrap()
            .iter()
            .find(|x| x.id == sent.id)
            .unwrap()
            .voice
            .as_ref(),
        Some(voice)
    );
    e.file_new_version_typed(
        voice.item_id.clone(),
        vec![8; 500],
        None,
        "Cut".into(),
        "short.m4a".into(),
        "audio/mp4".into(),
    )
    .unwrap();
    let current = e
        .timeline(space)
        .unwrap()
        .into_iter()
        .find(|x| x.id == sent.id)
        .unwrap()
        .voice
        .unwrap();
    assert_eq!(current.item_id, voice.item_id);
    assert_ne!(current.sha256, voice.sha256);
    assert_eq!(current.ms, 0);
    assert!(current.levels.is_empty() && current.transcript.is_empty());
}

#[test]
fn voice_versions_and_reverts_are_author_only_locally_and_on_replay() {
    for legacy in [false, true] {
        for hidden in [false, true] {
            let e = seeded();
            let space = find_space(&e, "Paraty com a Marina");
            let me = e.me().unwrap().id;
            let other = space.members.iter().find(|p| !p.is_me).unwrap().id.clone();
            let item = if legacy {
                e.file_add(
                    space.id.clone(),
                    String::new(),
                    "Mensagem de voz.webm".into(),
                    "audio/webm".into(),
                    vec![7; 900],
                    None,
                )
                .unwrap()
                .id
            } else {
                note(&e, &space.id, None).unwrap().voice.unwrap().item_id
            };
            // Even an owner version that no longer looks like voice keeps ownership.
            e.file_new_version_typed(
                item.clone(),
                vec![8; 500],
                None,
                "Edit".into(),
                "renamed.m4a".into(),
                if hidden {
                    "application/octet-stream"
                } else {
                    "audio/mp4"
                }
                .into(),
            )
            .unwrap();
            let mut engine = e.lock();
            let current = engine.state.items[&item].current().content.clone();
            let original = engine.state.items[&item].versions[0].content.clone();
            assert!(engine.state.writer(&space.id, &other));
            engine.me = Some(other.clone());
            let events = engine.store.event_count().unwrap();
            assert!(engine
                .file_new_version(&item, &[9; 500], None, "Replace")
                .is_err());
            assert!(engine
                .file_new_version_typed(
                    &item,
                    &[9; 500],
                    None,
                    "Replace",
                    "other.mp3",
                    "audio/mpeg"
                )
                .is_err());
            assert!(engine.restore_version(&item, 1, "Revert").is_err());
            assert_eq!(engine.store.event_count().unwrap(), events);
            for body in [
                EventBody::ItemVersioned {
                    item: item.clone(),
                    content: original,
                    note: "Replace".into(),
                },
                EventBody::ItemVersioned {
                    item: item.clone(),
                    content: current.clone(),
                    note: "Replace without voice metadata".into(),
                },
                EventBody::ItemReverted {
                    item: item.clone(),
                    to_version: 1,
                    note: "Revert".into(),
                },
            ] {
                assert!(engine.append(&space.id, &other, body.clone()).is_err());
                ingest_as(&mut engine, &space.id, &other, body);
                assert_eq!(engine.state.items[&item].versions.len(), 2);
                assert_eq!(engine.state.items[&item].current().content, current);
            }
            engine.reload().unwrap();
            assert_eq!(engine.state.items[&item].versions.len(), 2);
            assert_eq!(engine.file_bytes(&item, None).unwrap(), Some(vec![8; 500]));
            engine.me = Some(me);
            engine.restore_version(&item, 1, "Owner revert").unwrap();
            assert_eq!(engine.file_bytes(&item, None).unwrap(), Some(vec![7; 900]));
        }
    }
}

#[test]
fn ordinary_files_stay_collaborative_but_only_the_author_can_make_them_voice() {
    let e = seeded();
    let space = find_space(&e, "Paraty com a Marina");
    let item = e
        .file_add(
            space.id.clone(),
            String::new(),
            "music.mp3".into(),
            "audio/mpeg".into(),
            vec![1; 64],
            None,
        )
        .unwrap()
        .id;
    let other = space.members.iter().find(|p| !p.is_me).unwrap().id.clone();
    let mut engine = e.lock();
    engine.me = Some(other.clone());
    engine
        .file_new_version(&item, &[2; 64], None, "Collaborative edit")
        .unwrap();
    engine
        .restore_version(&item, 1, "Collaborative revert")
        .unwrap();
    let ItemContent::File(mut doc) = engine.state.items[&item].current().content.clone() else {
        panic!("file")
    };
    doc.voice = Some(types::VoiceMeta::default());
    let body = EventBody::ItemVersioned {
        item: item.clone(),
        content: ItemContent::File(doc),
        note: "Impersonate voice".into(),
    };
    assert!(engine.append(&space.id, &other, body.clone()).is_err());
    ingest_as(&mut engine, &space.id, &other, body);
    engine.reload().unwrap();
    assert_eq!(engine.state.items[&item].versions.len(), 3);
    assert_eq!(engine.file_bytes(&item, None).unwrap(), Some(vec![1; 64]));
}

#[test]
fn rejected_voice_sends_keep_their_audio_reply_and_delivery_error_after_reopen() {
    let dir = std::env::temp_dir().join(types::new_id("voice-refusal"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("core.sqlite").to_string_lossy().to_string();
    let (space, root, sent) = {
        let e = Client::open(path.clone(), "en".into()).unwrap();
        e.seed_demo_if_empty().unwrap();
        let space = find_space(&e, "Zoen").id;
        let root = e.send_message(space.clone(), "Root".into()).unwrap();
        let me = e.me().unwrap().id;
        {
            let mut engine = e.lock();
            engine.store.set_synced(&space).unwrap();
            engine.net.synced.insert(space.clone());
        }
        let sent = note(&e, &space, Some(root.id.clone())).unwrap();
        let mut engine = e.lock();
        // Permission loss is one reason a queued send can be refused permanently.
        ingest_as(
            &mut engine,
            &space,
            &me,
            EventBody::MemberRemoved {
                identity: me.clone(),
            },
        );
        let client_id = engine.store.outbox().unwrap()[0].event.client_id.clone();
        assert!(engine
            .reject(&client_id, "permission denied", true, None)
            .user_failed());
        assert_eq!(
            engine.net.failed.get(&client_id).map(String::as_str),
            Some("permission denied")
        );
        assert!(engine.outbox_envelopes().is_empty());
        (space, root, sent)
    };
    let e = Client::open(path, "en".into()).unwrap();
    let thread = e.thread(space.clone(), root.id).unwrap();
    let failed = thread.iter().find(|x| x.id == sent.id).unwrap();
    assert_eq!(failed.delivery, Delivery::Failed);
    assert_eq!(failed.voice, sent.voice);
    assert_eq!(failed.in_thread, sent.in_thread);
    assert_eq!(
        e.file_bytes(failed.voice.as_ref().unwrap().item_id.clone(), None)
            .unwrap(),
        Some(vec![7; 900])
    );
    let engine = e.lock();
    let pending = engine.store.outbox().unwrap();
    assert_eq!(
        engine
            .net
            .failed
            .get(&pending[0].event.client_id)
            .map(String::as_str),
        Some("permission denied")
    );
    assert!(engine.outbox_envelopes().is_empty());
    drop(engine);
    drop(e);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn core_search_finds_voice_transcripts_as_messages_and_follows_edits() {
    let e = seeded();
    let space = find_space(&e, "Zoen").id;
    let sent = note(&e, &space, None).unwrap();
    let hits = || e.search("  ReCoRdInG  ".into());
    let assert_hit = || {
        let hits = hits();
        assert_eq!(hits.len(), 1);
        let hit = &hits[0];
        assert_eq!(hit.space_id, space);
        assert!(hit.item_id.is_none(), "transcript belongs to the message");
        assert_eq!(hit.snippet, "A recording");
        assert_eq!(hit.author, sent.author);
        assert_eq!(hit.at_ms, sent.at_ms);
    };
    assert!(matches!(&sent.kind, EntryKind::Message { text, .. } if text.is_empty()));
    assert_hit();
    e.lock().reload().unwrap();
    assert_hit();
    let item = sent.voice.as_ref().unwrap().item_id.clone();
    e.file_new_version(item.clone(), vec![8; 500], None, "Cut".into())
        .unwrap();
    assert!(hits().is_empty());
    e.restore_version(item, 1).unwrap();
    assert_hit();
    e.delete_message(space.clone(), sent.id.clone()).unwrap();
    assert!(hits().is_empty());
}

#[test]
fn typed_voice_transcripts_are_searchable_by_message_id_and_follow_edits() {
    let e = seeded();
    let space = find_space(&e, "Zoen").id;
    let sent = note(&e, &space, None).unwrap();
    let hits = || {
        e.universal_search("recording".into(), vec!["message".into()], 20)
            .unwrap()
            .hits
    };
    assert_eq!(
        hits().iter().map(|h| h.ref_id.as_str()).collect::<Vec<_>>(),
        vec![sent.id.as_str()]
    );
    e.lock().reload().unwrap();
    assert_eq!(hits()[0].ref_id, sent.id);
    let item = sent.voice.as_ref().unwrap().item_id.clone();
    e.file_new_version(item.clone(), vec![8; 500], None, "Cut".into())
        .unwrap();
    assert!(hits().is_empty(), "removed transcript must leave the index");
    e.restore_version(item, 1).unwrap();
    assert_eq!(hits()[0].ref_id, sent.id);
    e.delete_message(space, sent.id).unwrap();
    assert!(hits().is_empty());
}

#[test]
fn old_android_file_and_marker_replay_as_one_voice_message() {
    let e = seeded();
    let space = find_space(&e, "Paraty com a Marina").id;
    let before = e.timeline(space.clone()).unwrap().len();
    let file = e
        .file_add(
            space.clone(),
            "VoiceNotes/old.m4a".into(),
            "old.m4a".into(),
            "audio/mp4".into(),
            vec![1; 64],
            None,
        )
        .unwrap();
    let marker = format!("⟦voice:{}:1000:08f⟧\nOld transcript", file.id);
    let sent = e.send_message(space.clone(), marker.clone()).unwrap();
    e.lock().reload().unwrap();
    let timeline = e.timeline(space.clone()).unwrap();
    assert_eq!(timeline.len(), before + 1);
    assert_eq!(timeline.last().unwrap().id, sent.id);
    assert!(
        matches!(&timeline.last().unwrap().kind, EntryKind::Message { text, .. } if text == &marker)
    );
    let voice = timeline.last().unwrap().voice.as_ref().unwrap();
    assert_eq!(voice.item_id, file.id);
    assert_eq!(voice.ms, 1000);
    assert_eq!(voice.transcript, "Old transcript");
    assert_eq!(
        e.file_bytes(file.id.clone(), None).unwrap(),
        Some(vec![1; 64])
    );
    e.file_new_version(file.id.clone(), vec![2; 64], None, "Edit".into())
        .unwrap();
    e.lock().reload().unwrap();
    let edited = e
        .timeline(space.clone())
        .unwrap()
        .into_iter()
        .find(|x| x.id == sent.id)
        .unwrap()
        .voice
        .unwrap();
    assert_eq!(edited.item_id, voice.item_id);
    assert_ne!(edited.sha256, voice.sha256);
    assert_eq!(edited.ms, 0);
    assert!(edited.levels.is_empty() && edited.transcript.is_empty());
    e.restore_version(file.id, 1).unwrap();
    assert_eq!(
        e.timeline(space)
            .unwrap()
            .into_iter()
            .find(|x| x.id == sent.id)
            .unwrap()
            .voice
            .as_ref(),
        Some(voice)
    );
    assert!(e.verify_all().iter().all(|r| r.valid));
}

#[test]
fn legacy_two_event_voice_replies_still_replay_once() {
    let e = seeded();
    let space = find_space(&e, "Zoen").id;
    let root = e.send_message(space.clone(), "Root".into()).unwrap();
    let before = e.timeline(space.clone()).unwrap().len();
    let implicit = note(&e, &space, None).unwrap();
    let me = e.me().unwrap().id;
    let item = implicit.voice.as_ref().unwrap().item_id.clone();
    let explicit = e
        .lock()
        .reply_with(&space, &me, "", Some(item), &root.id, true)
        .unwrap();
    e.lock().reload().unwrap();
    let timeline = e.timeline(space.clone()).unwrap();
    assert_eq!(timeline.len(), before + 1);
    assert!(timeline.iter().all(|entry| entry.id != implicit.id));
    let thread = e.thread(space, root.id).unwrap();
    assert_eq!(thread.len(), 2);
    assert_eq!(thread[1].id, explicit.id);
    assert!(thread[1].voice.is_some());
}

#[test]
fn voice_reply_syncs_its_metadata_and_encrypted_bytes_and_survives_replay() {
    use crate::sync::Ingest;
    use protocol::{Envelope, Sequenced};

    let sender = seeded();
    let space = find_space(&sender, "Zoen").id;
    let root = sender.send_message(space.clone(), "Root".into()).unwrap();
    let mut relay = sender.lock().logs[&space].clone();
    sender.lock().net.synced.insert(space.clone());
    let sent = note(&sender, &space, Some(root.id.clone())).unwrap();
    let voice = sent.voice.as_ref().unwrap();
    let pending = sender.lock().store.outbox().unwrap();
    assert_eq!(
        pending.len(),
        1,
        "the voice and its thread destination must be atomic"
    );
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
        if event.seq >= sender.lock().logs[&space].len() as u64 {
            assert!(matches!(sender.lock().ingest(envelope), Ingest::Confirmed));
        }
    }
    assert_eq!(last(&receiver, &space).voice.as_ref(), Some(voice));
    assert_eq!(
        receiver.file_bytes(voice.item_id.clone(), None).unwrap(),
        None
    );
    let uploads = sender.lock().pending_uploads(10);
    let wanted = receiver.lock().wanted_media();
    assert_eq!(wanted.len(), 1);
    for piece in wanted {
        let encrypted = &uploads
            .iter()
            .find(|(blob, _)| blob == &piece.blob)
            .unwrap()
            .1;
        assert_ne!(encrypted, &vec![7; 900]);
        assert!(receiver.lock().media_arrived(&piece, encrypted));
    }
    receiver.lock().reload().unwrap();
    assert_eq!(last(&receiver, &space).voice.as_ref(), Some(voice));
    assert_eq!(
        receiver.file_bytes(voice.item_id.clone(), None).unwrap(),
        Some(vec![7; 900])
    );
    let thread = receiver.thread(space.clone(), root.id).unwrap();
    assert_eq!(thread.len(), 2);
    assert_eq!(thread[1].id, sent.id);
    assert!(receiver.verify_log(space).valid);
}
