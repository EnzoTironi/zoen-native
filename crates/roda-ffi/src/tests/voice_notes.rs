//! Voice notes travel as encrypted audio files (the chunked media path), not text markers.
use super::*;
use roda_types::{EventBody, ItemContent, Role};

// An untrusted relay may sequence a correctly signed but unauthorized event.
fn ingest_as(engine: &mut crate::engine::Engine, space: &str, author: &str, body: EventBody) {
    let event = engine.author_for(author).unwrap().sign_event(
        space,
        &roda_types::new_id("voice-event"),
        crate::engine::now_ms(),
        engine.logs[space].head(),
        body,
    );
    let mut relay = engine.logs[space].clone();
    let event = relay.sequence(event);
    assert_eq!(
        engine.ingest(roda_proto::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: roda_proto::Envelope::plain(event),
        }),
        crate::sync::Ingest::Applied
    );
}

fn note(e: &RodaEngine, space: &str, reply: Option<String>) -> Result<TimelineEntry, CoreError> {
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

fn last(e: &RodaEngine, space: &str) -> TimelineEntry {
    e.timeline(space.into()).unwrap().pop().unwrap()
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
fn cross_space_voice_forwarding_does_not_post_a_blank_unplayable_message() {
    let e = seeded();
    let source = find_space(&e, "Paraty com a Marina").id;
    let destination = find_space(&e, "Zoen").id;
    let sent = note(&e, &source, None).unwrap();
    let before = e.timeline(destination.clone()).unwrap();
    assert!(matches!(
        e.forward_message(source.clone(), sent.id.clone(), destination.clone()),
        Err(CoreError::Invalid { .. })
    ));
    assert_eq!(e.timeline(destination).unwrap(), before);
    assert_eq!(last(&e, &source).id, sent.id);
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
                    message: roda_types::new_id("msg"),
                    text: String::new(),
                    attaches: Some(item),
                    reply: Some(roda_types::ReplyRef::thread(root.id)),
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
    doc.voice = Some(roda_types::VoiceMeta::default());
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
    let dir = std::env::temp_dir().join(roda_types::new_id("voice-refusal"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("core.sqlite").to_string_lossy().to_string();
    let (space, root, sent) = {
        let e = RodaEngine::open(path.clone(), "en".into()).unwrap();
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
        assert!(engine.reject(&client_id, "permission denied", true, None));
        assert_eq!(
            engine.net.failed.get(&client_id).map(String::as_str),
            Some("permission denied")
        );
        assert!(engine.outbox_envelopes().is_empty());
        (space, root, sent)
    };
    let e = RodaEngine::open(path, "en".into()).unwrap();
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
    use roda_proto::{Envelope, Sequenced};

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
    let receiver = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
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
