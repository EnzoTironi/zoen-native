//! Complete voice scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn voice_keeps_original_and_edit_recipe_private_then_sends_one_exact_render_once() {
    let f = fixture(":memory:");
    let before = counts(&f.engine);
    let original = save(&f, "0", &voice(false), vec![audio("original")]);
    assert!(matches!(
        f.engine
            .send_media_draft(f.scope.clone(), original.revision.clone()),
        Err(CoreError::DraftRejected { .. })
    ));
    assert!(!read(&f).attempted);
    assert_eq!(counts(&f.engine), before);
    let draft = save(&f, &original.revision, &voice(true), vec![audio("edited")]);
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .unwrap();
    assert_eq!(sent.entries.len(), 1);
    let note = sent.entries[0].voice.as_ref().unwrap();
    assert_eq!(note.ms, 901);
    assert_eq!(note.transcript, "spoken result");
    assert_eq!(
        f.engine
            .file_bytes(note.item_id.clone(), None)
            .unwrap()
            .unwrap(),
        audio("edited").bytes
    );
    let accepted_counts = counts(&f.engine);
    assert_eq!(
        f.engine
            .send_media_draft(f.scope.clone(), draft.revision)
            .unwrap()
            .entries,
        sent.entries
    );
    assert_eq!(counts(&f.engine), accepted_counts);
    assert!(!serde_json::to_string(&f.engine.lock().history_bundle())
        .unwrap()
        .contains("Private transcript"));
}

pub(super) fn voice_append_failure_rolls_back_chunks_keys_and_uploads_then_retries_once() {
    let f = fixture(":memory:");
    let draft = save(
        &f,
        "0",
        &voice(true),
        vec![audio("original"), audio("edited")],
    );
    let before = counts(&f.engine);
    f.engine.lock().store.conn().execute_batch("CREATE TRIGGER fail_voice BEFORE INSERT ON outbox WHEN NEW.json LIKE '%ItemCreated%' AND NEW.json LIKE '%VoiceNotes/%' BEGIN SELECT RAISE(ABORT,'voice failed'); END;").unwrap();
    assert!(f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .is_err());
    assert_eq!(counts(&f.engine), before);
    assert!(read(&f).attempted);
    assert_eq!(asset(&f, &draft, "original"), audio("original").bytes);
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_voice")
        .unwrap();
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .unwrap();
    assert_eq!(sent.entries.len(), 1);
    let accepted_counts = counts(&f.engine);
    let retried = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    assert_eq!(retried.entries, sent.entries);
    assert_eq!(retried.draft, sent.draft);
    assert_eq!(counts(&f.engine), accepted_counts);
}

pub(super) fn voice_save_ack_survives_realistic_float_normalization_and_identifies_newer_current() {
    let f = fixture(":memory:");
    let before = counts(&f.engine);
    // Native waveform peaks are Float32 values promoted to Double before JSON encoding.
    let levels: Vec<f64> = (0..160)
        .map(|index| f64::from((index % 81) as f32 / 100.0))
        .collect();
    let waveform = serde_json::to_string(&levels).unwrap();
    let original_json = r#"{"kind":"voice","reply_to":null,"source_asset_id":"original","editor":{"version":1,"draft":{"original":{"id":"original","sampleRate":48000,"frameCount":48000,"levels":WAVEFORM}},"pendingTrim":null},"render":null}"#
        .replace("WAVEFORM", &waveform);
    let first = save_ack(&f, "0", &original_json, vec![audio("original")]);
    assert_eq!(first.accepted_revision, first.draft.revision);
    let original: serde_json::Value = serde_json::from_str(&first.draft.content_json).unwrap();
    assert_eq!(
        original["editor"]["draft"]["original"]["levels"]
            .as_array()
            .unwrap()
            .len(),
        160
    );
    assert_eq!(asset(&f, &first.draft, "original"), audio("original").bytes);

    // Private render metadata is intentionally normalized through the canonical f32 type.
    let rendered_json = original_json.replace(
        r#""render":null"#,
        r#""render":{"asset_id":"edited","mime":"audio/mp4","duration_ms":900.5,"levels":[0.4000000059604645],"transcript":"spoken result"}"#,
    );
    let rendered = save_ack(
        &f,
        &first.accepted_revision,
        &rendered_json,
        vec![audio("edited")],
    );
    assert_eq!(rendered.accepted_revision, rendered.draft.revision);
    assert!(rendered.draft.content_json.contains(r#""levels":[0.4]"#));
    assert!(!rendered
        .draft
        .content_json
        .contains(r#""levels":[0.4000000059604645]"#));
    assert_eq!(asset(&f, &rendered.draft, "edited"), audio("edited").bytes);
    assert_eq!(
        counts(&f.engine),
        before,
        "preparing audio must not publish media"
    );

    let newer = save(
        &f,
        &rendered.accepted_revision,
        &attachments(&["a"], None),
        vec![input("a")],
    );
    let retained = rows(&f);
    let replay = save_ack(
        &f,
        &first.accepted_revision,
        &rendered_json,
        vec![audio("edited")],
    );
    assert_eq!(replay.accepted_revision, rendered.accepted_revision);
    assert_ne!(replay.accepted_revision, replay.draft.revision);
    assert_eq!(replay.draft, newer);
    assert_eq!(
        rows(&f),
        retained,
        "old ACK must not resurrect normalized audio"
    );
}
