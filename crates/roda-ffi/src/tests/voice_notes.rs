//! Voice notes travel as encrypted audio files (the chunked media path), not text markers.
use super::*;

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
