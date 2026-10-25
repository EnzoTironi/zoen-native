//! Original whole test bodies, called by their unchanged registered facade.
use super::*;

pub(super) fn native_voice_replies_versions_and_reverts_stay_on_the_original_post(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use types::{FileDoc, ItemContent, ItemKind, ReplyRef, VoiceMeta};

    let identity = NativeSigner::from_secret(&[31; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[32; 32]));
    let author = Author::root(identity.clone());
    let other = Author::root(NativeSigner::from_secret(&[33; 32]));
    let mut client = Client::new(account(&identity, &host.0), host, "").unwrap();
    let space = "sp_voice";
    let mut log = SpaceLog::new(space);
    let append =
        |log: &mut SpaceLog, author: &Author, body| -> Result<(), Box<dyn std::error::Error>> {
            let now = log.next_seq() as i64 + 1;
            clear(log, author, body, now)?;
            Ok(())
        };
    append(
        &mut log,
        &author,
        EventBody::SpaceCreated {
            title: "Voice fixture".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
    )?;
    append(
        &mut log,
        &author,
        EventBody::MessagePosted {
            message: "msg_root".into(),
            text: "Bring the tent".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    )?;
    let root = log.events().last().unwrap().hash.clone();
    let root_alias = log.events().last().unwrap().client_id.clone();
    let mut original = crate::media::seal_file("voice.webm", "audio/webm", b"original", None)
        .unwrap()
        .doc;
    original.path = "VoiceNotes/native.webm".into();
    original.voice = Some(VoiceMeta {
        ms: 3400,
        levels: "08ff80".into(),
        transcript: "See you there".into(),
        reply: Some(ReplyRef::inline(&root)),
        forwarded: false,
    });
    let post = |item: &str, doc: FileDoc| EventBody::ItemCreated {
        item: item.into(),
        kind: ItemKind::File,
        content: ItemContent::File(doc),
        origin: "Voice message".into(),
    };
    append(&mut log, &author, post("it_voice", original.clone()))?;
    let entry = log.events().last().unwrap().hash.clone();
    let mut threaded = original.clone();
    threaded.voice.as_mut().unwrap().reply = Some(ReplyRef::thread(&root_alias));
    append(&mut log, &author, post("it_thread", threaded))?;
    let thread_entry = log.events().last().unwrap().hash.clone();
    client.logs.insert(space.into(), log.clone());

    let conversation = &client.conversations()[0];
    assert_eq!(conversation.messages.len(), 3);
    let original_ids = conversation
        .messages
        .iter()
        .map(|message| message.id.clone())
        .collect::<Vec<_>>();
    let voice = &conversation.messages[1];
    assert_eq!(voice.file.as_ref(), Some(&original));
    assert_eq!(voice.quote.as_ref().unwrap().id, root);
    assert_eq!(voice.quote.as_ref().unwrap().text, "Bring the tent");
    assert!(voice.thread.is_none());
    assert!(!voice.edited);
    assert_eq!(
        conversation.messages[2].thread.as_deref(),
        Some(root.as_str())
    );
    assert!(conversation.messages[2].quote.is_none());
    assert_eq!(conversation.messages[0].replies, 1);
    assert_eq!(
        client.posted(space, &entry),
        Some(Some(ReplyRef::inline(&root)))
    );
    assert_eq!(
        client.posted(space, &thread_entry),
        Some(Some(ReplyRef::thread(&root_alias)))
    );

    let mut edited = crate::media::seal_file("voice.webm", "audio/webm", b"edited", None)
        .unwrap()
        .doc;
    // Native byte edits clear stale timings/transcript but retain the legacy voice path.
    edited.path = original.path.clone();
    let version = |doc: FileDoc| EventBody::ItemVersioned {
        item: "it_voice".into(),
        content: ItemContent::File(doc),
        note: "Trimmed".into(),
    };
    append(&mut log, &author, version(edited.clone()))?;
    append(&mut log, &other, version(original.clone()))?;
    // A retained caption/forward must resolve the same current manifest as the
    // original card, including ownership-checked versions and reverts.
    let attachment = author.sign_event(
        space,
        "attachment-forward",
        100,
        log.head(),
        EventBody::MessagePosted {
            message: "legacy-forward".into(),
            text: "caption".into(),
            attaches: Some("it_voice".into()),
            reply: None,
            media: None,
            forwarded: true,
        },
    )?;
    let assert_attachment = |client: &Client<HostSigner>, expected: &FileDoc| {
        let view = client
            .message_view(
                &attachment,
                "confirmed",
                None,
                &documents::item_edits(&client.document_items().unwrap()),
            )
            .unwrap();
        assert_eq!(view.text, "caption");
        assert!(view.forwarded);
        assert_eq!(view.file.as_ref().unwrap().chunks, expected.chunks);
        assert_eq!(view.file.as_ref(), Some(expected));
    };
    client.logs.insert(space.into(), log.clone());
    assert_attachment(&client, &edited);
    let conversation = &client.conversations()[0];
    let voice = &conversation.messages[1];
    assert_eq!(conversation.messages.len(), 4);
    assert_eq!(
        conversation.messages[..3]
            .iter()
            .map(|message| &message.id)
            .collect::<Vec<_>>(),
        original_ids.iter().collect::<Vec<_>>()
    );
    let activity = conversation.messages[3].item_edit.as_ref().unwrap();
    assert_eq!(activity.item_id, "it_voice");
    assert_eq!(activity.version, 2);
    assert_eq!(activity.note, "Trimmed");
    assert!(!activity.is_undo);
    assert!(conversation.messages[3].file.is_none());
    assert_eq!(conversation.messages[0].replies, 1);
    assert_eq!(voice.entry.as_deref(), Some(entry.as_str()));
    assert_eq!(voice.file.as_ref(), Some(&edited));
    assert!(voice.edited);
    assert_eq!(voice.quote.as_ref().unwrap().id, root);

    let revert = |to_version| EventBody::ItemReverted {
        item: "it_voice".into(),
        to_version,
        note: "Restored".into(),
    };
    append(&mut log, &other, revert(1))?;
    append(&mut log, &author, revert(0))?;
    append(&mut log, &author, revert(99))?;
    client.logs.insert(space.into(), log.clone());
    assert_eq!(
        client.conversations()[0].messages[1].file.as_ref(),
        Some(&edited)
    );
    assert_attachment(&client, &edited);
    assert_eq!(
        client.conversations()[0].messages.len(),
        4,
        "Foreign-author and invalid restore targets add no activity"
    );
    append(&mut log, &author, revert(1))?; // Version 3 is a copy of version 1.
    client.logs.insert(space.into(), log.clone());
    assert_eq!(
        client.conversations()[0].messages[1].file.as_ref(),
        Some(&original)
    );
    assert_attachment(&client, &original);
    append(&mut log, &author, revert(2))?; // Version 4 is a copy of version 2.
    append(&mut log, &author, revert(3))?; // Reverts can target earlier reverts.
    client.logs.insert(space.into(), log);
    let conversation = &client.conversations()[0];
    assert_eq!(conversation.messages.len(), 7);
    assert_eq!(
        conversation.messages[..3]
            .iter()
            .map(|message| &message.id)
            .collect::<Vec<_>>(),
        original_ids.iter().collect::<Vec<_>>()
    );
    let activities = conversation.messages[3..]
        .iter()
        .map(|message| message.item_edit.as_ref().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        activities
            .iter()
            .map(|edit| (edit.version, edit.is_undo))
            .collect::<Vec<_>>(),
        [(2, false), (3, true), (4, true), (5, true)]
    );
    assert!(activities
        .iter()
        .all(|edit| edit.item_id == "it_voice" && edit.space_id == space));
    assert_eq!(conversation.messages[0].replies, 1);
    assert_eq!(conversation.messages[1].quote.as_ref().unwrap().id, root);
    assert_eq!(
        conversation.messages[2].thread.as_deref(),
        Some(root.as_str())
    );
    assert_eq!(conversation.messages[1].file.as_ref(), Some(&original));
    assert!(conversation.messages[1].edited);
    let native_target = client.logs[space]
        .events()
        .iter()
        .find(|e| e.hash == entry)
        .unwrap()
        .client_id
        .clone();
    let mut log = client.logs[space].clone();
    append(
        &mut log,
        &other,
        EventBody::MessageAnnotated {
            target: native_target.clone(),
            note: types::Annotation::Delete,
        },
    )?;
    client.logs.insert(space.into(), log.clone());
    assert!(!client.conversations()[0].messages[1].deleted);
    append(
        &mut log,
        &author,
        EventBody::MessageAnnotated {
            target: native_target,
            note: types::Annotation::Delete,
        },
    )?;
    client.logs.insert(space.into(), log);
    let conversations = client.conversations();
    assert!(conversations[0].messages[1].deleted);
    assert!(conversations[0].messages[1].file.is_none());
    Ok(())
}

pub(super) fn native_forwarded_voice_is_one_destination_only_message_after_receive_and_saved_reload(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use types::{ItemContent, ItemKind, VoiceMeta};

    for forwarded in [false, true] {
        let identity = NativeSigner::from_secret(&[34; 32]);
        let host = HostSigner(NativeSigner::from_secret(&[35; 32]));
        let author = Author::root(identity.clone());
        let linked = account(&identity, &host.0);
        let mut client = Client::new(linked.clone(), host.clone(), "").unwrap();
        let space = "sp_forward_destination_only";
        let mut log = SpaceLog::new(space);
        let create = clear(
            &mut log,
            &author,
            EventBody::SpaceCreated {
                title: "Destination only".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
            1,
        )?;
        client.receive(&create, 2).unwrap();
        let mut document =
            crate::media::seal_file("forwarded.wav", "audio/wav", b"synthetic audio", None)
                .unwrap()
                .doc;
        document.path = "VoiceNotes/forwarded-destination.wav".into();
        // Old senders omit the flag entirely. New forwarded voices carry it in
        // their one ItemCreated, without a second message or foreign reply IDs.
        let mut wire =
            serde_json::json!({"ms":1500,"levels":"08f","transcript":"Retained transcript"});
        if forwarded {
            wire["forwarded"] = true.into();
        }
        document.voice = Some(serde_json::from_value::<VoiceMeta>(wire).unwrap());
        let posted = clear(
            &mut log,
            &author,
            EventBody::ItemCreated {
                item: "it_fresh_destination_voice".into(),
                kind: ItemKind::File,
                content: ItemContent::File(document.clone()),
                origin: "Voice message".into(),
            },
            3,
        )?;
        let update = client.receive(&posted, 4).unwrap();
        let assert_projection = |client: &Client<HostSigner>| {
            let conversations = client.conversations();
            assert_eq!(conversations.len(), 1);
            let conversation = &conversations[0];
            assert_eq!(conversation.id, space);
            assert_eq!(conversation.messages.len(), 1);
            let message = &conversation.messages[0];
            assert_eq!(message.forwarded, forwarded);
            assert!(message.text.is_empty());
            assert!(message.quote.is_none() && message.thread.is_none());
            assert_eq!(message.replies, 0);
            assert_eq!(
                message.file_item_id.as_deref(),
                Some("it_fresh_destination_voice")
            );
            assert_eq!(message.file.as_ref(), Some(&document));
            assert_eq!(message.file_version, Some(1));
            assert_eq!(client.logs.len(), 1, "no source history is needed");
        };
        assert_projection(&client);
        let restored = Client::new(linked, host, &update.snapshot).unwrap();
        assert_projection(&restored);
    }
    Ok(())
}
