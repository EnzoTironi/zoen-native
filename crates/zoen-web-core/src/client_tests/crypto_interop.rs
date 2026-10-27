//! Original whole test bodies, called by their unchanged registered facade.
use super::*;

pub(super) fn native_sqlite_welcome_messages_saved_reload_and_removal_use_shared_mls(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let identity = NativeSigner::from_secret(&[11; 32]);
    let native_key = NativeSigner::from_secret(&[12; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[13; 32]));
    let account = account(&identity, &host.0);
    let author = Author::device(&identity, native_key.clone());
    let mut sqlite = rusqlite::Connection::open_in_memory().unwrap();
    mls::migrate(&mut sqlite).unwrap();
    let native = Device::new(
        &sqlite,
        [14; 32],
        &identity.id(),
        native_key.secret(),
        author.cert.as_deref().unwrap(),
    )
    .unwrap();
    let mut client = Client::new(account.clone(), host.clone(), "").unwrap();
    let start = client.bootstrap().unwrap();
    let package = start
        .frames
        .iter()
        .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Req {
                op: Op::PublishKeyPackages { packages, .. },
                ..
            } => packages.into_iter().next(),
            _ => None,
        })
        .unwrap();
    let space = "sp_native_web_fixture";
    let mut log = SpaceLog::new(space);
    let create = clear(
        &mut log,
        &author,
        EventBody::SpaceCreated {
            title: "Native fixture".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
        1,
    )?;
    client.receive(&create, 2).unwrap();
    assert!(client.queue_message(space, "not yet", 3).is_err());
    native.create_group(space).unwrap();
    let roster = BTreeSet::from([identity.id()]);
    let commit = native.commit(space, &[package], &BTreeSet::new()).unwrap();
    native
        .open(space, &commit.commit, &roster, native.leaf())
        .unwrap();
    client
        .receive(
            &sealed(&mut log, &author, SealedKind::Commit, commit.commit, 4)?,
            5,
        )
        .unwrap();
    client
        .receive(
            &sealed(
                &mut log,
                &author,
                SealedKind::Welcome,
                commit.welcome.unwrap(),
                6,
            )?,
            7,
        )
        .unwrap();
    client.receive(&ServerFrame::SyncDone.encode(), 8).unwrap();
    assert_eq!(
        client.device().unwrap().checkpoint(space).unwrap(),
        native.checkpoint(space).unwrap()
    );
    assert!(client.conversations()[0].can_send);

    let inner = author.sign_event(
        space,
        &types::new_ulid(9)?,
        9,
        log.head(),
        EventBody::MessagePosted {
            message: "msg_native".into(),
            text: "Native to web".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    )?;
    let plaintext = InnerEvent {
        content: inner.content,
        sig: inner.sig,
    }
    .encode();
    let ciphertext = native.seal(space, &plaintext).unwrap();
    let (content, sig) = author.sign_sealed(
        space,
        &inner.client_id,
        9,
        inner.seen.as_ref(),
        Sealed::new(SealedKind::Application, SUITE_ID, ciphertext),
    );
    client
        .receive(
            &sequence(
                &mut log,
                Envelope::new(content, sig, author.cert.clone(), None).unwrap(),
            )?,
            10,
        )
        .unwrap();
    assert_eq!(client.conversations()[0].messages[0].text, "Native to web");

    // Ordinary files need a companion chat post for native's Entry projection.
    let before_media = client.saved.outbox.clone();
    let file = crate::media::seal_file("report.txt", "text/plain", b"report", None)
        .unwrap()
        .doc;
    client.queue_file(space, file, 11).unwrap();
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
        .unwrap();
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
    // Ordinary files use the same MessagePosted identity as native. Only voice
    // creation is itself a message; otherwise reactions/deletes target the wrong hash.
    let companion = events
        .iter()
        .find(|e| matches!(e.body, EventBody::MessagePosted { .. }))
        .unwrap();
    assert!(client
        .message_view(
            companion,
            "sending",
            None,
            &documents::item_edits(&client.document_items().unwrap())
        )
        .unwrap()
        .file
        .is_some());
    let creation = events
        .iter()
        .find(|event| matches!(event.body, EventBody::ItemCreated { .. }))
        .unwrap();
    assert!(client
        .message_view(
            creation,
            "sending",
            None,
            &documents::item_edits(&client.document_items().unwrap())
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
            &documents::item_edits(&client.document_items().unwrap()),
        )
        .unwrap();
    assert_eq!(shared.file.as_ref().unwrap().name, "report.txt");
    let mut other_author = companion.clone();
    other_author.author = "another-author".into();
    assert!(client
        .message_view(
            &other_author,
            "sending",
            None,
            &documents::item_edits(&client.document_items().unwrap())
        )
        .unwrap()
        .file
        .is_some());
    let mut other_space = companion.clone();
    other_space.space = "another-space".into();
    assert!(client
        .message_view(
            &other_space,
            "sending",
            None,
            &documents::item_edits(&client.document_items().unwrap())
        )
        .unwrap()
        .file
        .is_none());
    for (text, reply, forwarded, attachment) in [
        ("caption", None, false, item.clone()),
        (
            "",
            Some(types::ReplyRef {
                quote: None,
                artifact: None,
                to: "root".into(),
                thread: false,
            }),
            false,
            item.clone(),
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
            item.clone(),
        ),
        ("", None, true, item.clone()),
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
            *r = reply.clone();
            *f = forwarded;
            *attaches = Some(attachment.clone());
        }
        let view = client
            .message_view(
                &post,
                "sending",
                None,
                &documents::item_edits(&client.document_items().unwrap()),
            )
            .expect("meaningful attachment post must survive");
        assert_eq!(view.text, text);
        if attachment == item {
            let doc = view
                .file
                .as_ref()
                .expect("retained attachment must render file payload");
            assert_eq!(doc.name, "report.txt");
            assert!(!doc.chunks.is_empty());
        } else {
            assert!(view.file.is_none());
        }
        assert_eq!(view.forwarded, forwarded);
        assert_eq!(
            view.quote.is_some() || view.thread.is_some(),
            reply.is_some()
        );
    }
    client.saved.outbox = before_media.clone();
    let target = client.conversations()[0].messages[0].entry.clone().unwrap();
    let expected = client
        .resolve_post(space, &target)
        .unwrap()
        .client_id
        .clone();
    let mut voice = crate::media::seal_file("voice.webm", "audio/webm", b"voice", None)
        .unwrap()
        .doc;
    voice.voice = Some(types::VoiceMeta {
        reply: Some(types::ReplyRef {
            quote: None,
            artifact: None,
            to: target,
            thread: false,
        }),
        ..Default::default()
    });
    client.queue_file(space, voice, 11).unwrap();
    assert!(client
        .saved
        .outbox
        .values()
        .filter_map(|p| p.inner.as_ref())
        .any(|e| matches!(&e.body,
        EventBody::ItemCreated { content: types::ItemContent::File(doc), .. }
        if doc.voice.as_ref().and_then(|v| v.reply.as_ref()).is_some_and(|r| r.to == expected))));
    client.saved.outbox = before_media;

    let sealed =
        crate::media::seal_file("voice.webm", "audio/webm", b"retained audio", None).unwrap();
    let chunks = sealed
        .uploads
        .iter()
        .map(|u| (u.blob.clone(), u.sealed.clone()))
        .collect();
    // A recording that failed offline must survive without a synced MLS epoch.
    client.synced = false;
    let failed = client
        .queue_failed_file(space, sealed.doc.clone(), 11, "offline".into(), chunks)
        .unwrap();
    assert!(failed
        .frames
        .iter()
        .all(|f| !matches!(ClientFrame::decode(f).unwrap(), ClientFrame::Publish { .. })));
    let restored_failed = Client::new(account.clone(), host.clone(), &failed.snapshot).unwrap();
    let bytes = restored_failed.local_chunk(&sealed.doc.chunks[0].blob);
    assert_eq!(
        crate::media::open_chunk(&sealed.doc.chunks[0], &bytes).unwrap(),
        b"retained audio"
    );
    assert_eq!(
        restored_failed.conversations()[0]
            .messages
            .last()
            .unwrap()
            .delivery,
        "failed"
    );
    client.synced = true;
    client.saved.outbox.retain(|_, p| p.failure.is_none());
    let sending = client.queue_message(space, "Web to native", 11).unwrap();
    let env = sending
        .frames
        .iter()
        .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Publish { env }
                if env
                    .sealed_data()
                    .is_some_and(|(kind, _)| kind == SealedKind::Application) =>
            {
                Some(env)
            }
            _ => None,
        })
        .unwrap();
    env.verify().unwrap();
    let (_, ciphertext) = env.sealed_data().unwrap();
    let from = Leaf {
        identity: account.identity.clone(),
        device: account.device.clone(),
        cert: account.cert.clone(),
    };
    let Opened::Application { plaintext, .. } =
        native.open(space, ciphertext, &roster, &from).unwrap()
    else {
        panic!("real MLS application expected")
    };
    let opened = InnerEvent::decode(&plaintext).unwrap();
    let event = event_log::event_from_content(
        opened.content,
        opened.sig,
        Some(account.cert.clone()),
        0,
        String::new(),
        String::new(),
    )
    .unwrap();
    event_log::verify_author(&event).unwrap();
    assert!(matches!(event.body, EventBody::MessagePosted { text, .. } if text == "Web to native"));
    let accepted = client.receive(&sequence(&mut log, env)?, 12).unwrap();
    assert_eq!(accepted.conversations[0].messages.len(), 2);
    assert_eq!(accepted.conversations[0].messages[1].delivery, "confirmed");

    // A cold offline reopen can enqueue with its provisioned MLS group. It must
    // persist the ratchet and outbox without publishing until catch-up finishes.
    let mut offline = Client::new(account.clone(), host.clone(), &accepted.snapshot).unwrap();
    assert!(!offline.synced);
    assert!(offline.conversations()[0].can_send);
    let queued = offline
        .queue_message(space, "Offline after reopening", 12)
        .unwrap();
    let queued_id = queued.conversations[0].messages.last().unwrap().id.clone();
    assert!(!queued.frames.iter().any(|frame| matches!(
        ClientFrame::decode(frame).unwrap(),
        ClientFrame::Publish { env } if env.client_id() == queued_id
    )));
    let mut offline_reopened =
        Client::new(account.clone(), host.clone(), &queued.snapshot).unwrap();
    assert_eq!(offline_reopened.conversations()[0].messages.len(), 3);
    assert_eq!(
        offline_reopened.conversations()[0].messages[2].text,
        "Offline after reopening"
    );
    offline_reopened.bootstrap().unwrap();
    let resumed = offline_reopened
        .receive(&ServerFrame::SyncDone.encode(), 13)
        .unwrap();
    assert_eq!(
        resumed
            .frames
            .iter()
            .filter(|frame| matches!(
                ClientFrame::decode(frame).unwrap(),
                ClientFrame::Publish { env } if env.client_id() == queued_id
            ))
            .count(),
        1
    );

    let mut restored = Client::new(account, host, &accepted.snapshot).unwrap();
    restored.bootstrap().unwrap();
    restored
        .receive(&ServerFrame::SyncDone.encode(), 13)
        .unwrap();
    assert!(restored.conversations()[0].can_send);
    assert_eq!(restored.conversations()[0].messages.len(), 2);
    assert_eq!(
        restored.device().unwrap().checkpoint(space).unwrap(),
        native.checkpoint(space).unwrap()
    );
    restored
        .receive(
            &clear(
                &mut log,
                &author,
                EventBody::MemberRemoved {
                    identity: identity.id(),
                },
                14,
            )?,
            15,
        )
        .unwrap();
    assert!(restored.queue_message(space, "removed", 16).is_err());
    assert_eq!(
        restored.device().unwrap().group_state(space).unwrap(),
        GroupState::Missing
    );
    Ok(())
}

pub(super) fn invitation_queues_a_certified_self_add_with_the_original_code_and_no_plain_message_fallback(
) {
    let identity = NativeSigner::from_secret(&[21; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[22; 32]));
    let mut client = Client::new(account(&identity, &host.0), host, "").unwrap();
    assert!(client
        .queue_join("sp_invite", Role::Member, "invalid", 1)
        .is_err());
    let update = client
        .queue_join("sp_invite", Role::Member, "0123456789", 2)
        .unwrap();
    let env = update
        .frames
        .iter()
        .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Publish { env } => Some(env),
            _ => None,
        })
        .unwrap();
    assert_eq!(env.invite.as_deref(), Some("0123456789"));
    env.verify().unwrap();
    assert!(
        matches!(env.plain_event().unwrap().body, EventBody::MemberAdded { identity: ref added, role: Role::Member } if added == &identity.id())
    );
    assert!(client
        .queue_message("sp_invite", "plain text forbidden", 3)
        .is_err());
}
