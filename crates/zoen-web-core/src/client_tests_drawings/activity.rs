#[test]
fn signed_drawing_activity_keeps_each_canonical_version_and_survives_browser_reload(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, mut log) = document_fixture()?;
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11)?;
    for index in 0..3 {
        let now = 20 + index * 10;
        let base = read(&mut client, &item, now);
        doc(
            &mut client,
            save_request(
                &item,
                &base,
                &scene(&format!("Drawing {index}")),
                &format!("activity-{index}"),
            ),
            now + 1,
        );
        let pending = client.conversations();
        let activity = pending[0]
            .messages
            .iter()
            .find(|message| message.delivery == "sending" && message.item_edit.is_some())
            .unwrap();
        assert_eq!(
            activity.item_edit.as_ref().unwrap().version,
            index as u32 + 2
        );
        upload_and_confirm(&mut client, &mut log, now + 2)?;
    }
    let human = client
        .queue_message(log.space(), "Updated the drawing", 60)
        .unwrap();
    confirm_documents(&mut client, &mut log, human, 61)?;
    let before = pending_state(&client);
    let views = client.conversations();
    let canonical = log
        .events()
        .iter()
        .filter(|event| matches!(event.body, EventBody::ItemVersioned { .. }))
        .collect::<Vec<_>>();
    let edits = views[0]
        .messages
        .iter()
        .filter(|message| message.item_edit.is_some())
        .collect::<Vec<_>>();
    assert_eq!(edits.len(), 3);
    for (index, (view, event)) in edits.iter().zip(canonical).enumerate() {
        let edit = view.item_edit.as_ref().unwrap();
        assert_eq!(view.id, event.client_id);
        assert_eq!(view.entry.as_deref(), Some(event.hash.as_str()));
        assert_eq!(view.seq, Some(event.seq));
        assert_eq!(edit.space_id, log.space());
        assert_eq!(edit.item_id, item);
        assert_eq!(edit.version, index as u32 + 2);
        assert_eq!(edit.note, view.text);
        assert!(!edit.is_undo);
        assert!(
            view.card.is_none()
                && view.file.is_none()
                && view.quote.is_none()
                && view.thread.is_none()
        );
    }
    let same_words = views[0].messages.last().unwrap();
    assert_eq!(same_words.text, "Updated the drawing");
    assert!(
        same_words.item_edit.is_none(),
        "Human wording must never manufacture activity"
    );
    assert_eq!(
        pending_state(&client),
        before,
        "Projection must not mutate canonical or saved state"
    );
    let restored = Client::new(client.account.clone(), client.signer.clone(), &before).unwrap();
    assert_eq!(
        serde_json::to_value(restored.conversations()).unwrap(),
        serde_json::to_value(views).unwrap()
    );
    Ok(())
}

#[test]
fn activity_uses_accepted_scoped_versions_and_keeps_undo_distinct_from_its_restore_target(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, author, mut log) = document_fixture()?;
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11)?;
    let base = read(&mut client, &item, 12);
    doc(
        &mut client,
        save_request(&item, &base, &scene("Version two"), "activity-save"),
        13,
    );
    upload_and_confirm(&mut client, &mut log, 14)?;
    let invalid = clear(
        &mut log,
        &author,
        EventBody::ItemReverted {
            item: item.clone(),
            to_version: 99,
            note: "Invalid target".into(),
        },
        15,
    )?;
    client.receive(&invalid, 16).unwrap();
    assert_eq!(
        client.conversations()[0]
            .messages
            .iter()
            .filter(|message| message.item_edit.is_some())
            .count(),
        1
    );
    let restore = clear(
        &mut log,
        &author,
        EventBody::ItemReverted {
            item: item.clone(),
            to_version: 1,
            note: "Restored v1".into(),
        },
        17,
    )?;
    client.receive(&restore, 18).unwrap();
    let views = client.conversations();
    let restored = views[0]
        .messages
        .iter()
        .find_map(|message| message.item_edit.as_ref().filter(|edit| edit.is_undo))
        .unwrap();
    assert_eq!(
        restored.version, 3,
        "Undo is a new canonical version, never version 1"
    );
    assert_eq!(restored.note, "Restored v1");
    let mut foreign = SpaceLog::new("sp_other_activity_scope");
    let created = clear(
        &mut foreign,
        &author,
        EventBody::SpaceCreated {
            title: "Other".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
        19,
    )?;
    client.receive(&created, 20).unwrap();
    let forged_scope = clear(
        &mut foreign,
        &author,
        EventBody::ItemReverted {
            item: item.clone(),
            to_version: 1,
            note: "Other space".into(),
        },
        21,
    )?;
    client.receive(&forged_scope, 22).unwrap();
    let views = client.conversations();
    assert!(views
        .iter()
        .find(|chat| chat.id == foreign.space())
        .unwrap()
        .messages
        .is_empty());
    assert_eq!(
        views
            .iter()
            .find(|chat| chat.id == log.space())
            .unwrap()
            .messages
            .iter()
            .filter(|message| message.item_edit.is_some())
            .count(),
        2
    );
    Ok(())
}

#[test]
fn signed_second_device_thread_on_activity_survives_annotations_and_exports_boundary_fixture(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, mut log) = document_fixture()?;
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11)?;
    let base = read(&mut client, &item, 12);
    doc(
        &mut client,
        save_request(&item, &base, &scene("Keep this drawing"), "activity-thread"),
        13,
    );
    upload_and_confirm(&mut client, &mut log, 14)?;
    let activity_event = log
        .events()
        .iter()
        .find(|event| matches!(event.body, EventBody::ItemVersioned { .. }))
        .unwrap()
        .clone();
    // Public clients reject a new reply to a non-message. An independently signed
    // device can still publish this protocol-valid body; readers must stay usable.
    assert!(client
        .queue_post(
            log.space(),
            "public API refuses",
            Some((activity_event.hash.clone(), true)),
            false,
            15
        )
        .is_err());
    let second = Author::device(
        &NativeSigner::from_secret(&[71; 32]),
        NativeSigner::from_secret(&[74; 32]),
    );
    assert_ne!(second.key.id(), client.account.device);
    for (index, note) in [
        types::Annotation::React {
            emoji: "❤️".into()
        },
        types::Annotation::Edit {
            text: "Changed the signed activity".into(),
        },
        types::Annotation::Delete,
    ]
    .into_iter()
    .enumerate()
    {
        let frame = clear(
            &mut log,
            &second,
            EventBody::MessageAnnotated {
                target: activity_event.hash.clone(),
                note,
            },
            20 + index as i64,
        )?;
        client.receive(&frame, 30 + index as i64).unwrap();
    }
    let reply = clear(
        &mut log,
        &second,
        EventBody::MessagePosted {
            message: "msg_signed_activity_reply".into(),
            text: "Please keep the blue arrow.".into(),
            attaches: None,
            reply: Some(types::ReplyRef::thread(&activity_event.hash)),
            media: None,
            forwarded: false,
        },
        40,
    )?;
    client.receive(&reply, 41).unwrap();
    let reply_event = log.events().last().unwrap();
    let before = pending_state(&client);
    let conversations = client.conversations();
    let activity = conversations[0]
        .messages
        .iter()
        .find(|message| message.id == activity_event.client_id)
        .unwrap();
    let edit = activity.item_edit.as_ref().unwrap();
    assert_eq!(
        activity.entry.as_deref(),
        Some(activity_event.hash.as_str())
    );
    assert_eq!(
        activity.replies, 1,
        "Never erase the signed human thread to satisfy a DTO parser"
    );
    assert!(!activity.deleted && !activity.edited && activity.reactions.is_empty());
    assert_eq!(activity.text, edit.note);
    let human = conversations[0]
        .messages
        .iter()
        .find(|message| message.id == reply_event.client_id)
        .unwrap();
    assert_eq!(human.thread.as_deref(), Some(activity_event.hash.as_str()));
    assert_eq!(human.text, "Please keep the blue arrow.");
    assert!(human.item_edit.is_none());
    assert_eq!(pending_state(&client), before);
    if let Some(path) = std::env::var_os("ZOEN_WEB_ACTIVITY_FIXTURE") {
        let detail = client
            .item_value(&client.document_item(&item).unwrap(), None, &mut Vec::new())
            .unwrap();
        let fixture = json!({"conversations":conversations,"space_id":log.space(),"item":detail,"activity_id":activity_event.client_id,"activity_hash":activity_event.hash,
            "reply_id":reply_event.client_id,"reply_hash":reply_event.hash});
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        std::io::Write::write_all(&mut output, &serde_json::to_vec_pretty(&fixture).unwrap())
            .unwrap();
    }
    Ok(())
}

#[test]
fn signed_duplicate_item_origins_surface_projection_error_without_dropping_human_history(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, author, mut log) = document_fixture()?;
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11)?;
    let base = read(&mut client, &item, 12);
    doc(
        &mut client,
        save_request(&item, &base, &scene("Original scope"), "collision-save"),
        13,
    );
    upload_and_confirm(&mut client, &mut log, 14)?;
    let original = client
        .document_item(&item)
        .unwrap()
        .current()
        .content
        .clone();
    let human = client
        .queue_message(log.space(), "Human history remains available", 15)
        .unwrap();
    confirm_documents(&mut client, &mut log, human, 16)?;
    let human_id = log.events().last().unwrap().client_id.clone();
    let mut foreign = SpaceLog::new("sp_item_collision");
    let created = clear(
        &mut foreign,
        &author,
        EventBody::SpaceCreated {
            title: "Other legitimate scope".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
        17,
    )?;
    client.receive(&created, 18).unwrap();
    let duplicate = clear(
        &mut foreign,
        &author,
        EventBody::ItemCreated {
            item: item.clone(),
            kind: ItemKind::App,
            content: original,
            origin: "Conflicting origin".into(),
        },
        19,
    )?;
    client.receive(&duplicate, 20).unwrap();
    let other_human = clear(
        &mut foreign,
        &author,
        EventBody::MessagePosted {
            message: "msg_collision_human".into(),
            text: "Other human history remains available".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
        21,
    )?;
    client.receive(&other_human, 22).unwrap();
    let reason = client
        .document_items()
        .err()
        .expect("The existing fold must refuse ambiguous ownership");
    let before = pending_state(&client);
    let conversations = client.conversations();
    assert_eq!(conversations.len(), 2);
    for view in &conversations {
        assert_eq!(view.integrity_error.as_deref(), Some(reason.as_str()));
        assert!(view
            .messages
            .iter()
            .any(|message| message.text.contains("human history")
                || message.text.contains("Human history")));
        assert!(
            view.messages
                .iter()
                .all(|message| message.item_edit.is_none()),
            "No ambiguous Item projection is invented"
        );
        let report = client
            .integrity_request(&json!({"op":"verify_log","space_id":view.id}).to_string())
            .unwrap();
        assert_eq!(report["valid"], false);
        assert_eq!(report["error"], reason);
        assert_eq!(report["head_hash"], "");
    }
    assert!(conversations
        .iter()
        .find(|view| view.id == log.space())
        .unwrap()
        .messages
        .iter()
        .any(|message| message.id == human_id));
    assert_eq!(
        pending_state(&client),
        before,
        "Read-only diagnostics never mutate canonical history"
    );
    client
        .saved
        .integrity_errors
        .insert(log.space().into(), "Existing saved fork".into());
    let views = client.conversations();
    assert_eq!(
        views
            .iter()
            .find(|view| view.id == log.space())
            .unwrap()
            .integrity_error
            .as_deref(),
        Some("Existing saved fork")
    );
    assert_eq!(
        client
            .integrity_request(r#"{"op":"integrity_warnings"}"#)
            .unwrap()[log.space()],
        "Existing saved fork"
    );
    if let Some(path) = std::env::var_os("ZOEN_WEB_ACTIVITY_CONFLICT_FIXTURE") {
        let fixture = json!({"conversations":views,"reason":reason,"saved_warnings":client.integrity_request(r#"{"op":"integrity_warnings"}"#).unwrap(),"human_id":human_id});
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        std::io::Write::write_all(&mut output, &serde_json::to_vec_pretty(&fixture).unwrap())
            .unwrap();
    }
    Ok(())
}
