use super::*;

pub(super) fn assert_file_cards(
    client: &mut Client<HostSigner>,
    space: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Ordinary files need a companion chat post for native's Entry projection.
    let before_media = client.saved.outbox.clone();
    let file = crate::media::seal_file("report.txt", "text/plain", b"report", None)?.doc;
    client.queue_file(space, file, 11)?;
    let events: Vec<_> = client
        .saved
        .outbox
        .values()
        .filter_map(|p| p.inner.clone())
        .collect();
    let item = events
        .iter()
        .find_map(|e| match &e.body {
            EventBody::ItemCreated { item, .. } => Some(item.clone()),
            _ => None,
        })
        .ok_or("missing authenticated test find_map value")?;
    assert!(events.iter().any(|e| matches!(&e.body,
        EventBody::MessagePosted { attaches: Some(id), .. } if id == &item)));
    assert_eq!(
        client.conversations()[0]
            .messages
            .iter()
            .filter(|m| m.file.is_some())
            .count(),
        1
    );
    let companion = assert_file_aliases(client, &events)?;
    assert_file_post_variants(client, &companion, &item)?;
    client.saved.outbox = before_media;
    Ok(())
}

fn assert_file_aliases(
    client: &Client<HostSigner>,
    events: &[Event],
) -> std::result::Result<Event, Box<dyn std::error::Error>> {
    // Ordinary files use the same MessagePosted identity as native. Only voice
    // creation is itself a message; otherwise reactions/deletes target the wrong hash.
    let companion = events
        .iter()
        .find(|e| matches!(e.body, EventBody::MessagePosted { .. }))
        .ok_or("missing authenticated test find value")?;
    assert!(client
        .message_view(
            companion,
            "sending",
            None,
            &documents::item_edits(&client.document_items()?)
        )
        .ok_or("missing authenticated test message_view value")?
        .file
        .is_some());
    let creation = events
        .iter()
        .find(|event| matches!(event.body, EventBody::ItemCreated { .. }))
        .ok_or("missing authenticated test find value")?;
    assert!(client
        .message_view(
            creation,
            "sending",
            None,
            &documents::item_edits(&client.document_items()?)
        )
        .is_none());
    assert_eq!(client.conversations()[0].messages.len(), 2);
    // A later empty reshare is not the generated companion, even by its owner.
    let mut reshare = companion.clone();
    if let EventBody::MessagePosted { message, .. } = &mut reshare.body {
        *message = "later-reshare".into();
    }
    reshare.at_ms += 60_000;
    let shared = client
        .message_view(
            &reshare,
            "sending",
            None,
            &documents::item_edits(&client.document_items()?),
        )
        .ok_or("missing authenticated test message_view value")?;
    assert_eq!(
        shared
            .file
            .as_ref()
            .ok_or("missing authenticated test as_ref value")?
            .name,
        "report.txt"
    );
    let mut other_author = companion.clone();
    other_author.author = "another-author".into();
    assert!(client
        .message_view(
            &other_author,
            "sending",
            None,
            &documents::item_edits(&client.document_items()?)
        )
        .ok_or("missing authenticated test message_view value")?
        .file
        .is_some());
    let mut other_space = companion.clone();
    other_space.space = "another-space".into();
    assert!(client
        .message_view(
            &other_space,
            "sending",
            None,
            &documents::item_edits(&client.document_items()?)
        )
        .ok_or("missing authenticated test message_view value")?
        .file
        .is_none());
    Ok(companion.clone())
}

fn assert_file_post_variants(
    client: &Client<HostSigner>,
    companion: &Event,
    item: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for (text, reply, forwarded, attachment) in [
        ("caption", None, false, item.to_owned()),
        (
            "",
            Some(types::ReplyRef {
                quote: None,
                artifact: None,
                to: "root".into(),
                thread: false,
            }),
            false,
            item.to_owned(),
        ),
        (
            "",
            Some(types::ReplyRef {
                quote: None,
                artifact: None,
                to: "root".into(),
                thread: true,
            }),
            false,
            item.to_owned(),
        ),
        ("", None, true, item.to_owned()),
        ("", None, false, "non-file-item".into()),
    ] {
        let mut post = companion.clone();
        if let EventBody::MessagePosted {
            text: t,
            reply: r,
            forwarded: f,
            attaches,
            ..
        } = &mut post.body
        {
            *t = text.into();
            r.clone_from(&reply);
            *f = forwarded;
            *attaches = Some(attachment.clone());
        }
        let view = client
            .message_view(
                &post,
                "sending",
                None,
                &documents::item_edits(&client.document_items()?),
            )
            .ok_or("meaningful attachment post must survive")?;
        assert_eq!(view.text, text);
        if attachment == item {
            let doc = view
                .file
                .as_ref()
                .ok_or("retained attachment must render file payload")?;
            assert_eq!(doc.name, "report.txt");
            assert_ne!(doc.chunks, []);
        } else {
            assert!(view.file.is_none());
        }
        assert_eq!(view.forwarded, forwarded);
        assert_eq!(
            view.quote.is_some() || view.thread.is_some(),
            reply.is_some()
        );
    }
    Ok(())
}

pub(super) fn assert_voice_reply_identity(
    client: &mut Client<HostSigner>,
    space: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let before_media = client.saved.outbox.clone();
    let target = client.conversations()[0].messages[0]
        .entry
        .clone()
        .ok_or("missing authenticated test cloned value")?;
    let expected = client
        .resolve_post(space, &target)
        .ok_or("missing authenticated test resolve_post value")?
        .client_id
        .clone();
    let mut voice = crate::media::seal_file("voice.webm", "audio/webm", b"voice", None)?.doc;
    voice.voice = Some(types::VoiceMeta {
        reply: Some(types::ReplyRef {
            quote: None,
            artifact: None,
            to: target,
            thread: false,
        }),
        ..Default::default()
    });
    client.queue_file(space, voice, 11)?;
    assert!(client
        .saved
        .outbox
        .values()
        .filter_map(|p| p.inner.as_ref())
        .any(|e| matches!(&e.body,
        EventBody::ItemCreated { content: types::ItemContent::File(doc), .. }
        if doc.voice.as_ref().and_then(|v| v.reply.as_ref()).is_some_and(|r| r.to == expected))));
    client.saved.outbox = before_media;

    Ok(())
}

pub(super) fn assert_failed_media_recovery(
    client: &mut Client<HostSigner>,
    account: &LinkedAccount,
    host: &HostSigner,
    space: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let sealed = crate::media::seal_file("voice.webm", "audio/webm", b"retained audio", None)?;
    let chunks = sealed
        .uploads
        .iter()
        .map(|u| (u.blob.clone(), u.sealed.clone()))
        .collect();
    // A recording that failed offline must survive without a synced MLS epoch.
    client.synced = false;
    let failed =
        client.queue_failed_file(space, sealed.doc.clone(), 11, "offline".into(), chunks)?;
    assert!(failed
        .frames
        .iter()
        .map(|frame| ClientFrame::decode(frame).map_err(|error| error.to_string()))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .all(|frame| !matches!(frame, ClientFrame::Publish { .. })));
    let restored_failed = Client::new(account.clone(), host.clone(), &failed.snapshot)?;
    let bytes = restored_failed.local_chunk(&sealed.doc.chunks[0].blob);
    assert_eq!(
        crate::media::open_chunk(&sealed.doc.chunks[0], &bytes)?,
        b"retained audio"
    );
    assert_eq!(
        restored_failed.conversations()[0]
            .messages
            .last()
            .ok_or("missing authenticated test last value")?
            .delivery,
        "failed"
    );
    client.synced = true;
    client.saved.outbox.retain(|_, p| p.failure.is_none());
    Ok(())
}
