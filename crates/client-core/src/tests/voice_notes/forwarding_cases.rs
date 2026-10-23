//! Complete forwarding scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn forwarding_in_the_same_chat_keeps_the_original_and_its_reactions() {
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

pub(super) fn cross_space_voice_forwarding_copies_playable_bytes_with_fresh_destination_keys_once()
{
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

pub(super) fn forwarded_voice_uses_current_bytes_and_cleared_metadata_and_is_independent() {
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

pub(super) fn cross_space_voice_forwarding_strips_inline_thread_and_legacy_marker_destinations() {
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

pub(super) fn voice_forwarding_requires_source_access_and_destination_writer_without_leaking_material(
) {
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
