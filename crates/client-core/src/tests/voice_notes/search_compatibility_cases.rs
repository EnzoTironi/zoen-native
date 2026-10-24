//! Complete search compatibility scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn core_search_finds_voice_transcripts_as_messages_and_follows_edits() {
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

pub(super) fn typed_voice_transcripts_are_searchable_by_message_id_and_follow_edits() {
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

pub(super) fn old_android_file_and_marker_replay_as_one_voice_message() {
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

pub(super) fn legacy_two_event_voice_replies_still_replay_once() {
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

pub(super) fn voice_reply_syncs_its_metadata_and_encrypted_bytes_and_survives_replay(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
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
        relay.sequence(event.event)?;
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
    Ok(())
}
