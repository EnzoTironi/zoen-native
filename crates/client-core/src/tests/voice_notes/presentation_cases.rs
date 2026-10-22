//! Complete presentation scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn a_voice_note_is_an_encrypted_audio_file_with_its_waveform() {
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

pub(super) fn a_voice_reply_shows_once_with_its_quote_and_threads() {
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

pub(super) fn web_recordings_play_and_other_audio_files_stay_files() {
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

pub(super) fn only_audio_is_a_voice_note() {
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

pub(super) fn historical_voice_metadata_keeps_its_serialized_shape_and_defaults_to_not_forwarded() {
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

pub(super) fn renaming_voice_audio_keeps_metadata_but_replacing_bytes_invalidates_it() {
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

pub(super) fn ordinary_files_stay_collaborative_but_only_the_author_can_make_them_voice() {
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

pub(super) fn rejected_voice_sends_keep_their_audio_reply_and_delivery_error_after_reopen() {
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
