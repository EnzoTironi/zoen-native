use super::*;
use openmls_traits::{signatures::SignerError, types::SignatureScheme};
use roda_log::{Author, Signer as NativeSigner};
use roda_mls::Device;

#[path = "client_tests_drawings.rs"]
mod drawings;

#[derive(Clone)]
pub(super) struct HostSigner(pub(super) NativeSigner);
impl Signer for HostSigner {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SignerError> {
        hex::decode(self.0.sign(payload)).map_err(|_| SignerError::SigningError)
    }
    fn signature_scheme(&self) -> SignatureScheme {
        roda_mls::SUITE.signature_algorithm()
    }
}

pub(super) fn account(identity: &NativeSigner, device: &NativeSigner) -> LinkedAccount {
    LinkedAccount {
        identity: identity.id(),
        device: device.id(),
        cert: identity.sign(&roda_log::device_cert_message(&device.id())),
        name: "Fixture".into(),
        handle: "fixture".into(),
        relay: "http://127.0.0.1:9000".into(),
        primary: identity.id(),
        primary_hpke: String::new(),
        hpke_secret: String::new(),
        link_secret: String::new(),
        agreement_secret: String::new(),
        profile_key: String::new(),
        profile_key_version: 0,
        profile_seen_version: 0,
        profile_fields: String::new(),
    }
}

pub(super) fn sequence(log: &mut SpaceLog, env: Envelope) -> Vec<u8> {
    let outer = roda_log::event_from_content(
        env.content().to_vec(),
        env.sig.clone(),
        env.cert.clone(),
        0,
        String::new(),
        String::new(),
    )
    .unwrap();
    let event = log.sequence(outer);
    ServerFrame::Event {
        ev: Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env,
        },
    }
    .encode()
}

pub(super) fn clear(log: &mut SpaceLog, author: &Author, body: EventBody, now: i64) -> Vec<u8> {
    let event = author.sign_event(
        log.space(),
        &roda_types::new_ulid(now),
        now,
        log.head(),
        body,
    );
    sequence(log, Envelope::plain(&event))
}

fn sealed(
    log: &mut SpaceLog,
    author: &Author,
    kind: SealedKind,
    data: Vec<u8>,
    now: i64,
) -> Vec<u8> {
    let (content, sig) = author.sign_sealed(
        log.space(),
        &roda_types::new_ulid(now),
        now,
        log.head().as_ref(),
        Sealed::new(kind, SUITE_ID, data),
    );
    sequence(
        log,
        Envelope::new(content, sig, author.cert.clone(), None).unwrap(),
    )
}

#[test]
fn native_sqlite_welcome_messages_saved_reload_and_removal_use_shared_mls() {
    let identity = NativeSigner::from_secret(&[11; 32]);
    let native_key = NativeSigner::from_secret(&[12; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[13; 32]));
    let account = account(&identity, &host.0);
    let author = Author::device(&identity, native_key.clone());
    let mut sqlite = rusqlite::Connection::open_in_memory().unwrap();
    roda_mls::migrate(&mut sqlite).unwrap();
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
    );
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
            &sealed(&mut log, &author, SealedKind::Commit, commit.commit, 4),
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
            ),
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
        &roda_types::new_ulid(9),
        9,
        log.head(),
        EventBody::MessagePosted {
            message: "msg_native".into(),
            text: "Native to web".into(),
            attaches: None,
            reply: None,
            forwarded: false,
        },
    );
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
            ),
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
        .message_view(companion, "sending", None)
        .unwrap()
        .file
        .is_some());
    let creation = events
        .iter()
        .find(|event| matches!(event.body, EventBody::ItemCreated { .. }))
        .unwrap();
    assert!(client.message_view(creation, "sending", None).is_none());
    assert_eq!(client.conversations()[0].messages.len(), 2);
    // A later empty reshare is not the generated companion, even by its owner.
    let mut reshare = companion.clone();
    if let EventBody::MessagePosted { message, .. } = &mut reshare.body {
        *message = "later-reshare".into();
    }
    reshare.at_ms += 60_000;
    let shared = client.message_view(&reshare, "sending", None).unwrap();
    assert_eq!(shared.file.as_ref().unwrap().name, "report.txt");
    let mut other_author = companion.clone();
    other_author.author = "another-author".into();
    assert!(client
        .message_view(&other_author, "sending", None)
        .unwrap()
        .file
        .is_some());
    let mut other_space = companion.clone();
    other_space.space = "another-space".into();
    assert!(client
        .message_view(&other_space, "sending", None)
        .unwrap()
        .file
        .is_none());
    for (text, reply, forwarded, attachment) in [
        ("caption", None, false, item.clone()),
        (
            "",
            Some(roda_types::ReplyRef {
                to: "root".into(),
                thread: false,
            }),
            false,
            item.clone(),
        ),
        (
            "",
            Some(roda_types::ReplyRef {
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
            .message_view(&post, "sending", None)
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
    voice.voice = Some(roda_types::VoiceMeta {
        reply: Some(roda_types::ReplyRef {
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
        EventBody::ItemCreated { content: roda_types::ItemContent::File(doc), .. }
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
    let event = roda_log::event_from_content(
        opened.content,
        opened.sig,
        Some(account.cert.clone()),
        0,
        String::new(),
        String::new(),
    )
    .unwrap();
    roda_log::verify_author(&event).unwrap();
    assert!(matches!(event.body, EventBody::MessagePosted { text, .. } if text == "Web to native"));
    let accepted = client.receive(&sequence(&mut log, env), 12).unwrap();
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
            ),
            15,
        )
        .unwrap();
    assert!(restored.queue_message(space, "removed", 16).is_err());
    assert_eq!(
        restored.device().unwrap().group_state(space).unwrap(),
        GroupState::Missing
    );
}

#[test]
fn invitation_queues_a_certified_self_add_with_the_original_code_and_no_plain_message_fallback() {
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

#[test]
fn native_voice_replies_versions_and_reverts_stay_on_the_original_post() {
    use roda_types::{FileDoc, ItemContent, ItemKind, ReplyRef, VoiceMeta};

    let identity = NativeSigner::from_secret(&[31; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[32; 32]));
    let author = Author::root(identity.clone());
    let other = Author::root(NativeSigner::from_secret(&[33; 32]));
    let mut client = Client::new(account(&identity, &host.0), host, "").unwrap();
    let space = "sp_voice";
    let mut log = SpaceLog::new(space);
    let append = |log: &mut SpaceLog, author: &Author, body| {
        let now = log.next_seq() as i64 + 1;
        clear(log, author, body, now);
    };
    append(
        &mut log,
        &author,
        EventBody::SpaceCreated {
            title: "Voice fixture".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
    );
    append(
        &mut log,
        &author,
        EventBody::MessagePosted {
            message: "msg_root".into(),
            text: "Bring the tent".into(),
            attaches: None,
            reply: None,
            forwarded: false,
        },
    );
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
    append(&mut log, &author, post("it_voice", original.clone()));
    let entry = log.events().last().unwrap().hash.clone();
    let mut threaded = original.clone();
    threaded.voice.as_mut().unwrap().reply = Some(ReplyRef::thread(&root_alias));
    append(&mut log, &author, post("it_thread", threaded));
    let thread_entry = log.events().last().unwrap().hash.clone();
    client.logs.insert(space.into(), log.clone());

    let conversation = &client.conversations()[0];
    assert_eq!(conversation.messages.len(), 3);
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
    append(&mut log, &author, version(edited.clone()));
    append(&mut log, &other, version(original.clone()));
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
            forwarded: true,
        },
    );
    let assert_attachment = |client: &Client<HostSigner>, expected: &FileDoc| {
        let view = client.message_view(&attachment, "confirmed", None).unwrap();
        assert_eq!(view.text, "caption");
        assert!(view.forwarded);
        assert_eq!(view.file.as_ref().unwrap().chunks, expected.chunks);
        assert_eq!(view.file.as_ref(), Some(expected));
    };
    client.logs.insert(space.into(), log.clone());
    assert_attachment(&client, &edited);
    let conversation = &client.conversations()[0];
    let voice = &conversation.messages[1];
    assert_eq!(conversation.messages.len(), 3);
    assert_eq!(voice.entry.as_deref(), Some(entry.as_str()));
    assert_eq!(voice.file.as_ref(), Some(&edited));
    assert!(voice.edited);
    assert_eq!(voice.quote.as_ref().unwrap().id, root);

    let revert = |to_version| EventBody::ItemReverted {
        item: "it_voice".into(),
        to_version,
        note: "Restored".into(),
    };
    append(&mut log, &other, revert(1));
    append(&mut log, &author, revert(0));
    append(&mut log, &author, revert(99));
    client.logs.insert(space.into(), log.clone());
    assert_eq!(
        client.conversations()[0].messages[1].file.as_ref(),
        Some(&edited)
    );
    assert_attachment(&client, &edited);
    append(&mut log, &author, revert(1)); // Version 3 is a copy of version 1.
    client.logs.insert(space.into(), log.clone());
    assert_eq!(
        client.conversations()[0].messages[1].file.as_ref(),
        Some(&original)
    );
    assert_attachment(&client, &original);
    append(&mut log, &author, revert(2)); // Version 4 is a copy of version 2.
    append(&mut log, &author, revert(3)); // Reverts can target earlier reverts.
    client.logs.insert(space.into(), log);
    let conversation = &client.conversations()[0];
    assert_eq!(conversation.messages.len(), 3);
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
            note: roda_types::Annotation::Delete,
        },
    );
    client.logs.insert(space.into(), log.clone());
    assert!(!client.conversations()[0].messages[1].deleted);
    append(
        &mut log,
        &author,
        EventBody::MessageAnnotated {
            target: native_target,
            note: roda_types::Annotation::Delete,
        },
    );
    client.logs.insert(space.into(), log);
    let conversations = client.conversations();
    assert!(conversations[0].messages[1].deleted);
    assert!(conversations[0].messages[1].file.is_none());
}

#[test]
fn native_forwarded_voice_is_one_destination_only_message_after_receive_and_saved_reload() {
    use roda_types::{ItemContent, ItemKind, VoiceMeta};

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
        );
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
        );
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
}

fn web_fixture() -> (Client<HostSigner>, Author, Author, SpaceLog, String) {
    let identity = NativeSigner::from_secret(&[41; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[42; 32]));
    let author = Author::device(&identity, NativeSigner::from_secret(&[43; 32]));
    let other = Author::device(
        &NativeSigner::from_secret(&[44; 32]),
        NativeSigner::from_secret(&[45; 32]),
    );
    let client = Client::new(account(&identity, &host.0), host, "").unwrap();
    let mut log = SpaceLog::new("sp_web_receipts");
    clear(
        &mut log,
        &author,
        EventBody::SpaceCreated {
            title: "Fixture".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
        1,
    );
    clear(
        &mut log,
        &author,
        EventBody::MessagePosted {
            message: "msg_fixture".into(),
            text: "Oi".into(),
            attaches: None,
            reply: None,
            forwarded: false,
        },
        2,
    );
    let target = log.events().last().unwrap().hash.clone();
    (client, author, other, log, target)
}

fn annotate(log: &mut SpaceLog, author: &Author, target: &str, note: roda_types::Annotation) {
    let now = log.next_seq() as i64 + 10;
    clear(
        log,
        author,
        EventBody::MessageAnnotated {
            target: target.into(),
            note,
        },
        now,
    );
}

#[test]
fn conversation_metadata_comes_from_logs_and_only_my_personal_log_controls_preferences() {
    let (mut client, author, other, mut chat, _) = web_fixture();
    clear(
        &mut chat,
        &author,
        EventBody::MemberAdded {
            identity: other.identity.clone(),
            role: Role::Reader,
        },
        3,
    );
    let prefs = roda_types::ChatPrefs {
        pinned: true,
        archived: true,
        muted_until_ms: Some(i64::MAX),
    };
    let body = || EventBody::ChatPrefsSet {
        space: chat.space().into(),
        prefs: prefs.clone(),
    };
    let mut personal = SpaceLog::new("sp_my_personal");
    clear(
        &mut personal,
        &author,
        EventBody::SpaceCreated {
            title: "Personal".into(),
            kind: SpaceKind::Personal,
            privacy: Privacy::EndToEnd,
        },
        4,
    );
    clear(&mut personal, &author, body(), 5);
    clear(
        &mut personal,
        &other,
        EventBody::ChatPrefsSet {
            space: chat.space().into(),
            prefs: Default::default(),
        },
        6,
    );
    client.logs.insert(personal.space().into(), personal);
    let mut impostor = SpaceLog::new("sp_someone_elses_personal");
    clear(
        &mut impostor,
        &other,
        EventBody::SpaceCreated {
            title: "Other".into(),
            kind: SpaceKind::Personal,
            privacy: Privacy::EndToEnd,
        },
        7,
    );
    clear(
        &mut impostor,
        &author,
        EventBody::ChatPrefsSet {
            space: chat.space().into(),
            prefs: Default::default(),
        },
        8,
    );
    client.logs.insert(impostor.space().into(), impostor);
    let profile = Identity {
        id: other.identity.clone(),
        kind: roda_types::IdentityKind::Agent,
        name: "Garden".into(),
        handle: "garden".into(),
        tint_hex: "#34752A".into(),
        glyph: Some("leaf".into()),
        owner: Some(author.identity.clone()),
        bio: "Agent".into(),
        owner_proof: None,
    };
    client
        .saved
        .profiles
        .insert(profile.id.clone(), profile.clone());
    client.logs.insert(chat.space().into(), chat.clone());
    let conversations = client.conversations();
    assert_eq!(conversations.len(), 1);
    let conversation = &conversations[0];
    assert_eq!(conversation.privacy, Privacy::EndToEnd);
    assert_eq!(conversation.roster[&other.identity], Role::Reader);
    assert_eq!(
        conversation
            .profiles
            .iter()
            .map(|profile| profile.identity.clone())
            .collect::<Vec<_>>(),
        vec![profile]
    );
    assert_eq!(conversation.event_count, chat.next_seq().to_string());
    assert!(conversation.prefs.pinned && conversation.prefs.archived);
    assert_eq!(
        conversation.prefs.muted_until_ms.as_deref(),
        Some("9223372036854775807")
    );

    // The same preference body in the group has no effect.
    let chat_id = chat.space().to_owned();
    clear(
        &mut chat,
        &author,
        EventBody::ChatPrefsSet {
            space: chat_id,
            prefs: Default::default(),
        },
        9,
    );
    client.logs.insert(chat.space().into(), chat);
    assert!(client.conversations()[0].prefs.pinned);
    for privacy in [Privacy::Closed, Privacy::Public] {
        let space = format!("sp_{privacy:?}");
        let mut log = SpaceLog::new(&space);
        clear(
            &mut log,
            &author,
            EventBody::SpaceCreated {
                title: space.clone(),
                kind: SpaceKind::Community,
                privacy,
            },
            10,
        );
        client.logs.insert(space.clone(), log);
        let projected = client
            .conversations()
            .into_iter()
            .find(|chat| chat.id == space)
            .unwrap();
        assert_eq!(projected.privacy, privacy);
        assert!(!projected.prefs.pinned);
    }
}

#[test]
fn web_reaction_participants_replace_remove_and_ignore_invalid_reactions() {
    let (mut client, author, other, mut log, target) = web_fixture();
    let react = |emoji: &str| roda_types::Annotation::React {
        emoji: emoji.into(),
    };
    annotate(&mut log, &author, &target, react("👍"));
    annotate(&mut log, &other, &target, react("👍"));
    client.logs.insert(log.space().into(), log.clone());
    let view = client.conversations();
    let reaction = &view[0].messages[0].reactions[0];
    assert_eq!(reaction.count, 2);
    assert_eq!(reaction.participants.len(), 2);
    assert!(reaction.mine);
    assert!(reaction
        .participants
        .iter()
        .any(|p| p.identity == client.account.identity && p.name == "Fixture"));
    annotate(&mut log, &author, &target, react("❤️"));
    annotate(&mut log, &other, &target, react(" "));
    annotate(&mut log, &other, &target, react(&"😀".repeat(9)));
    client.logs.insert(log.space().into(), log.clone());
    let view = client.conversations();
    assert_eq!(view[0].messages[0].reactions.len(), 2);
    annotate(&mut log, &author, &target, roda_types::Annotation::Unreact);
    client.logs.insert(log.space().into(), log.clone());
    let view = client.conversations();
    let reactions = &view[0].messages[0].reactions;
    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].emoji, "👍");
    assert_eq!(reactions[0].count, 1);
    assert!(!reactions[0].mine);
    annotate(&mut log, &author, &target, roda_types::Annotation::Delete);
    annotate(&mut log, &other, &target, react("❤️"));
    client.logs.insert(log.space().into(), log);
    assert!(client.conversations()[0].messages[0].reactions.is_empty());
    assert!(client
        .queue_annotation("sp_web_receipts", &target, react("👍"), 90)
        .err()
        .unwrap()
        .contains("no longer available"));
}

#[test]
fn web_reaction_submission_rejects_blank_and_oversized_utf8_before_encryption() {
    let (mut client, _, _, log, target) = web_fixture();
    client.logs.insert(log.space().into(), log);
    for emoji in [" ".into(), "😀".repeat(9)] {
        assert!(client
            .queue_annotation(
                "sp_web_receipts",
                &target,
                roda_types::Annotation::React { emoji },
                90
            )
            .err()
            .unwrap()
            .contains("32 bytes"));
    }
}

fn receipt_marks(frames: &[Vec<u8>]) -> Vec<(&'static str, u64)> {
    frames
        .iter()
        .filter_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Ephemeral {
                kind: roda_proto::EphemeralKind::Delivered { seq },
                ..
            } => Some(("delivered", seq)),
            ClientFrame::Ephemeral {
                kind: roda_proto::EphemeralKind::Read { seq },
                ..
            } => Some(("read", seq)),
            _ => None,
        })
        .collect()
}

#[test]
fn web_receipts_replay_after_saved_unsent_frames_and_each_reconnect() {
    let (mut client, _, _, log, _) = web_fixture();
    let space = log.space().to_owned();
    let last = log.next_seq() - 1;
    client.logs.insert(space.clone(), log);
    let first = client.mark_read(&space).unwrap(); // Simulate a durable save, then losing the outgoing frame.
    assert_eq!(receipt_marks(&first.frames), vec![("read", last)]);
    assert_eq!(
        receipt_marks(&client.mark_read(&space).unwrap().frames),
        vec![("read", last)]
    );
    let mut restored = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &first.snapshot,
    )
    .unwrap();
    for _ in 0..2 {
        restored.bootstrap().unwrap();
        let update = restored
            .receive(&ServerFrame::SyncDone.encode(), 100)
            .unwrap();
        assert_eq!(
            receipt_marks(&update.frames),
            vec![("delivered", last), ("read", last)]
        );
        assert!(restored.delivered().is_empty());
    }
}

fn document_fixture() -> (Client<HostSigner>, Author, SpaceLog) {
    let identity = NativeSigner::from_secret(&[71; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[72; 32]));
    let author = Author::device(&identity, host.0.clone());
    let mut client = Client::new(account(&identity, &host.0), host, "").unwrap();
    let mut log = SpaceLog::new("sp_web_documents");
    let created = clear(
        &mut log,
        &author,
        EventBody::SpaceCreated {
            title: "Documentos compartilhados".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
        1,
    );
    client.receive(&created, 2).unwrap();
    client.receive(&ServerFrame::SyncDone.encode(), 3).unwrap();
    (client, author, log)
}

fn doc(client: &mut Client<HostSigner>, command: serde_json::Value, now: i64) -> DocumentResponse {
    client.document_request(&command.to_string(), now).unwrap()
}

fn confirm_documents(
    client: &mut Client<HostSigner>,
    log: &mut SpaceLog,
    update: Update,
    now: i64,
) {
    for frame in update.frames {
        if let ClientFrame::Publish { env } = ClientFrame::decode(&frame).unwrap() {
            let frame = sequence(log, env);
            client.receive(&frame, now).unwrap();
        }
    }
}

#[test]
fn browser_new_document_notes_follow_language_without_rewriting_received_history() {
    for (language, created, imported, untitled, page_label, restore_note) in [
        (
            roda_i18n::Lang::En,
            "Created",
            "Imported",
            "Untitled",
            "Page",
            "Restored v1",
        ),
        (
            roda_i18n::Lang::Pt,
            "Criado",
            "Importado",
            "Sem título",
            "Página",
            "Restaurou a v1",
        ),
    ] {
        let _language = roda_i18n::enter(language);
        let (mut client, author, mut log) = document_fixture();
        let made = doc(
            &mut client,
            serde_json::json!({"op":"page_create","space_id":log.space(),"title":""}),
            10,
        );
        assert_eq!(made.result["origin"], created);
        assert_eq!(made.result["title"], untitled);
        assert_eq!(made.result["kind_id"], "page");
        assert_eq!(made.result["kind_label"], page_label);
        confirm_documents(&mut client, &mut log, made.update.unwrap(), 11);
        let made = doc(
            &mut client,
            serde_json::json!({"op":"page_import_markdown","space_id":log.space(),"path":"notas/original.md","markdown":"# Título em português 🪴\n"}),
            12,
        );
        assert_eq!(made.result["origin"], imported);
        assert_eq!(made.result["title"], "Título em português 🪴");
        assert_eq!(made.result["path"], "notas/original.md");
        confirm_documents(&mut client, &mut log, made.update.unwrap(), 13);
        let received = clear(
            &mut log,
            &author,
            EventBody::ItemCreated {
                item: "it_literal_history".into(),
                kind: roda_types::ItemKind::Note,
                content: roda_types::ItemContent::Text {
                    text: "Texto original em português".into(),
                },
                origin: "Criado no iOS — manter literal".into(),
            },
            14,
        );
        client.receive(&received, 15).unwrap();
        let received = clear(
            &mut log,
            &author,
            EventBody::ItemVersioned {
                item: "it_literal_history".into(),
                content: roda_types::ItemContent::Text {
                    text: "Outra versão em português".into(),
                },
                note: "Editou no iOS — manter literal".into(),
            },
            16,
        );
        client.receive(&received, 17).unwrap();
        let restored = doc(
            &mut client,
            serde_json::json!({"op":"restore_version","item_id":"it_literal_history","version":1}),
            18,
        );
        assert_eq!(restored.result["versions"][0]["note"], restore_note);
        assert_eq!(
            restored.result["versions"][1]["note"],
            "Editou no iOS — manter literal"
        );
        assert_eq!(
            restored.result["versions"][2]["note"],
            "Criado no iOS — manter literal"
        );
        assert_eq!(restored.result["text"], "Texto original em português");
    }
}

#[test]
fn browser_pages_share_native_codec_persist_drafts_and_replay_receipts() {
    let (mut client, _, mut log) = document_fixture();
    let made = doc(
        &mut client,
        serde_json::json!({"op":"page_import_markdown","space_id":log.space(),"path":"notes/viagem.md","markdown":"# Viagem 🪴\n\nTexto **marcado**.\n"}),
        10,
    );
    let id = made.result["id"].as_str().unwrap().to_owned();
    confirm_documents(&mut client, &mut log, made.update.unwrap(), 11);
    let read = doc(
        &mut client,
        serde_json::json!({"op":"page","item_id":id}),
        12,
    )
    .result;
    assert_eq!(read["path"], "notes/viagem.md");
    let mut changed = read["blocks"][1].clone();
    changed["text"] = "Texto **preservado** 🪴".into();
    changed["spans"] = serde_json::json!([]);
    let request = serde_json::json!({"op":"page_apply","item_id":id,"mutation_id":"first","edit_context":read["edit_context"],"order":read["blocks"].as_array().unwrap().iter().map(|b|b["id"].clone()).collect::<Vec<_>>(),"changed":[changed]});
    let applied = doc(&mut client, request.clone(), 13);
    assert_eq!(applied.result["replayed"], false);
    assert_eq!(applied.result["page"]["unsaved"], true);
    let snapshot = applied.update.unwrap().snapshot;
    let mut reopened =
        Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
    let replay = doc(&mut reopened, request.clone(), 14);
    assert_eq!(replay.result["replayed"], true);
    assert_eq!(
        replay.result["applied_content_hash"],
        applied.result["applied_content_hash"]
    );
    let exported = doc(
        &mut reopened,
        serde_json::json!({"op":"page_markdown","item_id":id}),
        15,
    );
    assert!(exported.result.as_str().unwrap().contains("preservado"));
    let mut wrong = request;
    wrong["changed"][0]["text"] = "Outra edição".into();
    assert!(reopened.document_request(&wrong.to_string(), 16).is_err());
    let committed = doc(
        &mut reopened,
        serde_json::json!({"op":"page_commit","item_id":id,"note":"Editou no navegador"}),
        17,
    );
    assert_eq!(committed.result, true);
    let queued_page = doc(
        &mut reopened,
        serde_json::json!({"op":"page","item_id":id}),
        18,
    )
    .result;
    assert_eq!(queued_page["unsaved"], false);
    assert_eq!(queued_page["pending_sync"], true);
}

#[test]
fn browser_stale_page_editor_merges_unseen_blocks_and_restores_as_a_new_change() {
    use base64::Engine as _;
    let (mut client, author, mut log) = document_fixture();
    let page = roda_docs::Page::from_markdown("# Original\n\nTexto antigo\n").unwrap();
    let content = roda_types::PageContent {
        title: "Original".into(),
        path: String::new(),
        loro: base64::engine::general_purpose::STANDARD.encode(page.snapshot()),
        blob: None,
        manifest: None,
    };
    let create = clear(
        &mut log,
        &author,
        EventBody::ItemCreated {
            item: "it_merge".into(),
            kind: roda_types::ItemKind::Page,
            content: roda_types::ItemContent::Page(content),
            origin: "Criado no nativo".into(),
        },
        10,
    );
    client.receive(&create, 11).unwrap();
    let read = doc(
        &mut client,
        serde_json::json!({"op":"page","item_id":"it_merge"}),
        12,
    )
    .result;
    let before = page.version();
    let mut order = page
        .blocks()
        .iter()
        .map(|b| b.id.clone())
        .collect::<Vec<_>>();
    order.push("remote-block".into());
    page.apply(
        &order,
        &[roda_docs::BlockEdit {
            id: "remote-block".into(),
            kind: roda_docs::Kind::Paragraph,
            text: "Mudança remota".into(),
            spans: vec![],
        }],
    )
    .unwrap();
    let version = clear(
        &mut log,
        &author,
        EventBody::ItemVersioned {
            item: "it_merge".into(),
            content: roda_types::ItemContent::Page(roda_types::PageContent {
                title: "Original".into(),
                path: String::new(),
                loro: base64::engine::general_purpose::STANDARD.encode(page.updates_since(&before)),
                blob: None,
                manifest: None,
            }),
            note: "Mudança concorrente".into(),
        },
        13,
    );
    client.receive(&version, 14).unwrap();
    let mut heading = read["blocks"][0].clone();
    heading["text"] = "Editado no navegador".into();
    let request = serde_json::json!({"op":"page_apply","item_id":"it_merge","mutation_id":"merge","edit_context":read["edit_context"],"order":read["blocks"].as_array().unwrap().iter().map(|b|b["id"].clone()).collect::<Vec<_>>(),"changed":[heading]});
    let merged = doc(&mut client, request, 15).result;
    assert_eq!(merged["page"]["blocks"].as_array().unwrap().len(), 3);
    assert!(merged["page"]["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b["text"] == "Mudança remota"));
    let committed = doc(
        &mut client,
        serde_json::json!({"op":"page_commit","item_id":"it_merge","note":"Mesclou"}),
        16,
    );
    confirm_documents(&mut client, &mut log, committed.update.unwrap(), 17);
    let past = doc(
        &mut client,
        serde_json::json!({"op":"page_at","item_id":"it_merge","version":1}),
        18,
    )
    .result;
    assert_eq!(past["blocks"].as_array().unwrap().len(), 2);
    assert_eq!(past["can_edit"], false);
    let restored = doc(
        &mut client,
        serde_json::json!({"op":"restore_version","item_id":"it_merge","version":1}),
        19,
    );
    assert_eq!(restored.result["version"], 4);
    let current = doc(
        &mut client,
        serde_json::json!({"op":"page","item_id":"it_merge"}),
        20,
    )
    .result;
    assert_eq!(current["blocks"].as_array().unwrap().len(), 2);
    assert_eq!(current["blocks"][0]["text"], "Original");
}

#[test]
fn browser_large_page_upload_barrier_survives_reload_and_missing_download_waits() {
    use base64::Engine as _;
    let (mut client, author, mut log) = document_fixture();
    let original_snapshot = client.update(Vec::new()).unwrap().snapshot;
    let markdown = (0..8000)
        .map(|i| format!("Parágrafo {i}: conteúdo de uma página compartilhada 🪴.\n\n"))
        .collect::<String>();
    let made = doc(
        &mut client,
        serde_json::json!({"op":"page_import_markdown","space_id":log.space(),"path":"large.md","markdown":markdown}),
        10,
    );
    assert!(!made.uploads.is_empty());
    let update = made.update.unwrap();
    assert!(!update
        .frames
        .iter()
        .any(|frame| matches!(ClientFrame::decode(frame), Ok(ClientFrame::Publish { .. }))));
    let mut reopened = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &update.snapshot,
    )
    .unwrap();
    let resumed: Vec<String> = serde_json::from_str(&reopened.document_uploads().unwrap()).unwrap();
    assert_eq!(resumed, made.uploads);
    let queued = reopened
        .saved
        .outbox
        .values()
        .find_map(|p| p.inner.clone().or_else(|| p.envelope().ok()?.plain_event()))
        .unwrap();
    let roda_types::EventBody::ItemCreated {
        item,
        content: roda_types::ItemContent::Page(content),
        ..
    } = queued.body
    else {
        panic!("page creation")
    };
    let reference = content.blob.unwrap();
    let sealed = reopened.local_chunk(&reference.blob);
    let bytes = crate::media::open_chunk(&reference, &sealed).unwrap();
    let page = roda_docs::Page::new();
    page.import(&bytes).unwrap();
    assert!(page.to_markdown().contains("Parágrafo 7999"));
    reopened.synced = true;
    let released = reopened.document_uploaded(&resumed).unwrap();
    assert!(released
        .frames
        .iter()
        .any(|frame| matches!(ClientFrame::decode(frame), Ok(ClientFrame::Publish { .. }))));
    let received = clear(
        &mut log,
        &author,
        EventBody::ItemCreated {
            item: item.clone(),
            kind: roda_types::ItemKind::Page,
            content: roda_types::ItemContent::Page(roda_types::PageContent {
                title: "Grande".into(),
                path: "large.md".into(),
                loro: String::new(),
                blob: Some(reference.clone()),
                manifest: None,
            }),
            origin: "Outro cliente".into(),
        },
        20,
    );
    let mut remote = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &original_snapshot,
    )
    .unwrap();
    remote.receive(&received, 21).unwrap();
    let waiting = doc(
        &mut remote,
        serde_json::json!({"op":"page","item_id":item}),
        22,
    );
    assert_eq!(waiting.result["ready"], false);
    assert_eq!(waiting.result["can_edit"], false);
    assert_eq!(waiting.downloads.len(), 1);
    assert!(remote
        .document_cache_blob(reference.clone(), b"wrong".to_vec())
        .is_err());
    remote.document_cache_blob(reference, bytes).unwrap();
    let ready = doc(
        &mut remote,
        serde_json::json!({"op":"page_markdown","item_id":item}),
        23,
    );
    assert!(ready.result.as_str().unwrap().contains("Parágrafo 7999"));
    // The codec itself is the same binary snapshot native imports.
    assert!(
        base64::engine::general_purpose::STANDARD
            .encode(page.snapshot())
            .len()
            > 48 << 10
    );
}

#[test]
fn browser_plans_preserve_large_cents_and_refuse_stale_writes_and_undo() {
    let (mut client, author, mut log) = document_fixture();
    let plan = roda_types::PlanDoc {
        title: "Plano exato".into(),
        summary: "Centavos sem float".into(),
        budget_cents: Some(i64::MAX),
        sections: vec![roda_types::PlanSection {
            title: "Custos".into(),
            lines: vec![roda_types::PlanLine {
                id: "ln_exact".into(),
                text: "Custo grande".into(),
                cost_cents: 9_007_199_254_740_993,
                done: false,
            }],
        }],
    };
    let made = clear(
        &mut log,
        &author,
        EventBody::ItemCreated {
            item: "it_plan".into(),
            kind: roda_types::ItemKind::Plan,
            content: roda_types::ItemContent::Plan(plan),
            origin: "Criado no nativo".into(),
        },
        10,
    );
    client.receive(&made, 11).unwrap();
    let read = doc(
        &mut client,
        serde_json::json!({"op":"plan_page","item_id":"it_plan"}),
        12,
    )
    .result;
    assert_eq!(read["total_cents"], "9007199254740993");
    assert_eq!(read["legacy_edit_admitted"], true);
    let detail = doc(
        &mut client,
        serde_json::json!({"op":"item","item_id":"it_plan"}),
        13,
    )
    .result;
    assert_eq!(
        detail["plan"]["sections"][0]["lines"][0]["cost_cents"],
        "9007199254740993"
    );
    let toggle = doc(
        &mut client,
        serde_json::json!({"op":"toggle_plan_line","item_id":"it_plan","line_id":"ln_exact","observed_hash":read["content_hash"]}),
        14,
    );
    assert_eq!(
        toggle.result["item"]["plan"]["sections"][0]["lines"][0]["done"],
        true
    );
    assert!(client.document_request(&serde_json::json!({"op":"toggle_plan_line","item_id":"it_plan","line_id":"ln_exact","observed_hash":read["content_hash"]}).to_string(),15).is_err());
    let undo = doc(
        &mut client,
        serde_json::json!({"op":"undo","token":toggle.result["undo"]}),
        16,
    );
    assert_eq!(
        undo.result["plan"]["sections"][0]["lines"][0]["done"],
        false
    );
    assert!(client
        .document_request(
            &serde_json::json!({"op":"undo","token":toggle.result["undo"]}).to_string(),
            17
        )
        .is_err());
}

#[test]
fn browser_miniapps_use_shared_rules_item_scoped_grants_and_confirmations() {
    let (mut client, _, mut log) = document_fixture();
    let made=client.app_request(&serde_json::json!({"op":"install_app","space_id":log.space(),"app_id":"pet","args_json":"{\"name\":\"Jumento\"}"}).to_string(),10,"pt-BR").unwrap();
    let item = made.result["id"].as_str().unwrap().to_owned();
    assert_eq!(made.result["app"]["trust"], "Act");
    let update = made.update.unwrap();
    let published = update
        .frames
        .iter()
        .filter_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Publish { env } => env.plain_event(),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(published.len(), 3);
    assert!(matches!(published[0].body, EventBody::ItemCreated { .. }));
    assert!(matches!(published[1].body, EventBody::GrantIssued { .. }));
    assert!(matches!(published[2].body, EventBody::MessagePosted { .. }));
    let timeline = client.conversations();
    let card = timeline[0]
        .messages
        .iter()
        .find_map(|message| message.card.as_ref())
        .unwrap();
    assert_eq!(card["item_id"], item);
    assert_eq!(card["app"]["app_id"], "pet");
    confirm_documents(&mut client, &mut log, update, 11);
    let read=client.app_request(&serde_json::json!({"op":"app_call_tool","item_id":item,"tool":"pet_get_state","args_json":"{}","confirmed":false}).to_string(),12,"pt-BR").unwrap();
    assert_eq!(read.result["status"], "Done");
    assert!(read.update.is_none());
    let fed=client.app_request(&serde_json::json!({"op":"app_call_tool","item_id":item,"tool":"pet_feed","args_json":"{}","confirmed":false}).to_string(),13,"pt-BR").unwrap();
    assert_eq!(fed.result["status"], "Done");
    assert_eq!(fed.result["item"]["version"], 2);
    let count = client.saved.outbox.len();
    let needs=client.app_request(&serde_json::json!({"op":"app_call_tool","item_id":item,"tool":"pet_release","args_json":"{}","confirmed":false}).to_string(),14,"pt-BR").unwrap();
    assert_eq!(needs.result["status"], "NeedsConfirmation");
    assert!(needs.update.is_none());
    assert_eq!(client.saved.outbox.len(), count);
    let released=client.app_request(&serde_json::json!({"op":"app_call_tool","item_id":item,"tool":"pet_release","args_json":"{}","confirmed":true}).to_string(),15,"pt-BR").unwrap();
    assert_eq!(released.result["status"], "Done");
    assert_eq!(released.result["item"]["version"], 3);
    let resource = client
        .app_request(
            &serde_json::json!({"op":"app_resource","uri":"ui://roda/pet"}).to_string(),
            16,
            "pt-BR",
        )
        .unwrap();
    assert_eq!(
        resource.result["text"],
        roda_apps::spec("pet").unwrap().html.unwrap()
    );
    let specs = client
        .app_request(r#"{"op":"app_specs"}"#, 17, "pt-BR")
        .unwrap();
    assert_eq!(
        specs.result.as_array().unwrap().len(),
        roda_apps::specs().len()
    );
}

#[test]
fn browser_closed_chat_posts_queue_offline_and_never_publish_plaintext_after_encryption() {
    let (mut client, author, mut log) = document_fixture();
    client.synced = false;
    let queued = client
        .queue_message(log.space(), "Uma mensagem offline", 10)
        .unwrap();
    assert_eq!(
        client.conversations()[0].messages.last().unwrap().text,
        "Uma mensagem offline"
    );
    assert!(!queued
        .frames
        .iter()
        .any(|frame| matches!(ClientFrame::decode(frame), Ok(ClientFrame::Publish { .. }))));
    let upgraded = clear(&mut log, &author, EventBody::SpaceEncrypted, 11);
    client.receive(&upgraded, 12).unwrap();
    let resumed = client.receive(&ServerFrame::SyncDone.encode(), 13).unwrap();
    assert!(!resumed.frames.iter().any(|frame|matches!(ClientFrame::decode(frame),Ok(ClientFrame::Publish{env}) if env.plain_event().is_some_and(|event|matches!(event.body,EventBody::MessagePosted{..})))));
}

#[test]
fn browser_file_cards_keep_the_native_message_identity_for_deletion_and_replies() {
    let (mut client, _, mut log) = document_fixture();
    let file = crate::media::seal_file("report.txt", "text/plain", b"report", None)
        .unwrap()
        .doc;
    let (update, item, message) = client.queue_file_record(log.space(), file, 10).unwrap();
    let before = client.conversations();
    assert_eq!(before[0].messages.len(), 1);
    assert_eq!(before[0].messages[0].id, message);
    assert_eq!(
        before[0].messages[0].file_item_id.as_deref(),
        Some(item.as_str())
    );
    confirm_documents(&mut client, &mut log, update, 11);
    let posted = client.conversations()[0].messages[0].entry.clone().unwrap();
    let deleted = client
        .queue_annotation(log.space(), &posted, roda_types::Annotation::Delete, 12)
        .unwrap();
    confirm_documents(&mut client, &mut log, deleted, 13);
    let after = client.conversations();
    assert!(after[0].messages[0].deleted);
    assert!(after[0].messages[0].file.is_none());
}

#[test]
fn browser_directory_never_trusts_person_fields_or_reassigns_a_proven_agent_owner() {
    let (mut client, owner, _) = document_fixture();
    let person = NativeSigner::from_secret(&[81; 32]);
    let mut record = Identity {
        id: person.id(),
        kind: roda_types::IdentityKind::Person,
        name: "Directory must not reveal this".into(),
        handle: "person".into(),
        tint_hex: "#123456".into(),
        glyph: None,
        owner: None,
        bio: "Private directory text".into(),
        owner_proof: None,
    };
    let cached = client
        .directory_profiles(&serde_json::to_string(&vec![record.clone()]).unwrap())
        .unwrap();
    assert_eq!(cached.result[0]["name"], "@person");
    assert_eq!(cached.result[0]["bio"], "");
    let agent = NativeSigner::from_secret(&[82; 32]);
    record.id = agent.id();
    record.kind = roda_types::IdentityKind::Agent;
    record.name = "Verified agent".into();
    record.owner = Some(owner.identity.clone());
    // Public ownership metadata without the certified device proof has no authority.
    client
        .directory_profiles(&serde_json::to_string(&vec![record.clone()]).unwrap())
        .unwrap();
    assert!(!client.saved.profiles.contains_key(&agent.id()));
    record.owner_proof = Some(Box::new(
        roda_log::agent_owner::authorize(&owner, &agent.id()).unwrap(),
    ));
    client
        .directory_profiles(&serde_json::to_string(&vec![record.clone()]).unwrap())
        .unwrap();
    assert_eq!(
        client.saved.profiles[&agent.id()].owner.as_deref(),
        Some(owner.identity.as_str())
    );
    let other = Author::device(
        &NativeSigner::from_secret(&[83; 32]),
        NativeSigner::from_secret(&[84; 32]),
    );
    record.owner = Some(other.identity.clone());
    record.owner_proof = Some(Box::new(
        roda_log::agent_owner::authorize(&other, &agent.id()).unwrap(),
    ));
    client
        .directory_profiles(&serde_json::to_string(&vec![record]).unwrap())
        .unwrap();
    assert_eq!(
        client.saved.profiles[&agent.id()].owner.as_deref(),
        Some(owner.identity.as_str())
    );
}

#[test]
fn browser_typing_is_connection_local_and_requires_a_current_writing_member() {
    let (mut client, author, mut log) = document_fixture();
    let before = serde_json::to_string(&client.saved).unwrap();
    assert!(matches!(
        ClientFrame::decode(&client.typing_frame(log.space(), true).unwrap()).unwrap(),
        ClientFrame::Ephemeral {
            kind: roda_proto::EphemeralKind::Typing,
            ..
        }
    ));
    assert!(matches!(
        ClientFrame::decode(&client.typing_frame(log.space(), false).unwrap()).unwrap(),
        ClientFrame::Ephemeral {
            kind: roda_proto::EphemeralKind::StoppedTyping,
            ..
        }
    ));
    assert_eq!(serde_json::to_string(&client.saved).unwrap(), before);
    client.synced = false;
    assert!(client.typing_frame(log.space(), true).unwrap().is_empty());
    client.synced = true;
    let reader = clear(
        &mut log,
        &author,
        EventBody::MemberAdded {
            identity: author.identity.clone(),
            role: Role::Reader,
        },
        10,
    );
    client.receive(&reader, 11).unwrap();
    assert!(client.typing_frame(log.space(), true).unwrap().is_empty());
}

#[test]
fn browser_task_checkbox_draft_survives_reread_and_cold_journal_restore() {
    let (mut client, _, mut log) = document_fixture();
    let made = doc(
        &mut client,
        serde_json::json!({"op":"page_import_markdown","space_id":log.space(),"path":"checkbox.md","markdown":"# Checkbox\n\nOriginal café\n\n- [ ] Revisar documento\n"}),
        10,
    );
    let id = made.result["id"].as_str().unwrap().to_owned();
    confirm_documents(&mut client, &mut log, made.update.unwrap(), 11);
    let read = doc(
        &mut client,
        serde_json::json!({"op":"page","item_id":id}),
        12,
    )
    .result;
    assert_eq!(read["unsaved"], false);
    let published_markdown = doc(
        &mut client,
        serde_json::json!({"op":"page_markdown","item_id":id}),
        12,
    )
    .result;
    let mut task = read["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|block| block["kind"] == "task")
        .unwrap()
        .clone();
    assert_eq!(task["checked"], false);
    task["checked"] = true.into();
    let order = read["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| block["id"].clone())
        .collect::<Vec<_>>();
    let applied = doc(
        &mut client,
        serde_json::json!({"op":"page_apply","item_id":id,"mutation_id":"check-task","edit_context":read["edit_context"],"order":order,"changed":[task]}),
        13,
    );
    let assert_checked_draft = |value: &serde_json::Value| {
        assert_eq!(
            value["unsaved"], true,
            "accepted toggle must create a visible local draft"
        );
        let task = value["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|block| block["kind"] == "task")
            .unwrap();
        assert_eq!(task["checked"], true, "accepted toggle must remain checked");
    };
    assert_checked_draft(&applied.result["page"]);
    let snapshot = applied.update.unwrap().snapshot;
    let read_again = doc(
        &mut client,
        serde_json::json!({"op":"page","item_id":id}),
        14,
    )
    .result;
    assert_checked_draft(&read_again);
    let exported = doc(
        &mut client,
        serde_json::json!({"op":"page_markdown","item_id":id}),
        15,
    )
    .result;
    assert!(exported.as_str().unwrap().contains("[x] Revisar documento"));
    let mut reopened =
        Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
    let restored = doc(
        &mut reopened,
        serde_json::json!({"op":"page","item_id":id}),
        16,
    )
    .result;
    assert_checked_draft(&restored);
    let exported = doc(
        &mut reopened,
        serde_json::json!({"op":"page_markdown","item_id":id}),
        17,
    )
    .result;
    assert!(exported.as_str().unwrap().contains("[x] Revisar documento"));
    let mut unchecked = restored["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|block| block["kind"] == "task")
        .unwrap()
        .clone();
    unchecked["checked"] = false.into();
    let restored_order = restored["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| block["id"].clone())
        .collect::<Vec<_>>();
    let reverted = doc(
        &mut reopened,
        serde_json::json!({"op":"page_apply","item_id":id,"mutation_id":"uncheck-task","edit_context":restored["edit_context"],"order":restored_order,"changed":[unchecked]}),
        18,
    );
    let assert_clean = |value: &serde_json::Value| {
        assert_eq!(
            value["unsaved"], false,
            "returning to published content clears the draft difference"
        );
        assert_eq!(
            value["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|block| block["kind"] == "task")
                .unwrap()["checked"],
            false
        );
    };
    assert_clean(&reverted.result["page"]);
    let clean_snapshot = reverted.update.unwrap().snapshot;
    assert_clean(
        &doc(
            &mut reopened,
            serde_json::json!({"op":"page","item_id":id}),
            19,
        )
        .result,
    );
    assert_eq!(
        doc(
            &mut reopened,
            serde_json::json!({"op":"page_markdown","item_id":id}),
            20
        )
        .result,
        published_markdown
    );
    let mut clean_reopened = Client::new(
        reopened.account.clone(),
        reopened.signer.clone(),
        &clean_snapshot,
    )
    .unwrap();
    assert_clean(
        &doc(
            &mut clean_reopened,
            serde_json::json!({"op":"page","item_id":id}),
            21,
        )
        .result,
    );
    assert_eq!(
        doc(
            &mut clean_reopened,
            serde_json::json!({"op":"page_markdown","item_id":id}),
            22
        )
        .result,
        published_markdown
    );
}

#[test]
fn received_app_owned_documents_keep_manifest_history_without_becoming_file_cards() {
    let (mut client, author, mut log) = document_fixture();
    let bytes = br#"{"type":"excalidraw","version":2,"elements":[],"appState":{},"files":{}}"#;
    let sealed = crate::media::seal_file(
        "drawing.excalidraw",
        "application/vnd.excalidraw+json",
        bytes,
        None,
    )
    .unwrap();
    let document = roda_types::AppDoc {
        app: "excalidraw".into(),
        resource_uri: "ui://zoen/excalidraw.html".into(),
        title: "Drawing".into(),
        state_json: r#"{"title":"Drawing","elementCount":0,"textCount":0}"#.into(),
        document: Some(sealed.doc.clone()),
    };
    let created = clear(
        &mut log,
        &author,
        EventBody::ItemCreated {
            item: "it_drawing".into(),
            kind: roda_types::ItemKind::App,
            content: roda_types::ItemContent::App(document),
            origin: "Created".into(),
        },
        10,
    );
    client.receive(&created, 11).unwrap();
    let posted = clear(
        &mut log,
        &author,
        EventBody::MessagePosted {
            message: "msg_drawing".into(),
            text: "Drawing".into(),
            attaches: Some("it_drawing".into()),
            reply: None,
            forwarded: false,
        },
        12,
    );
    client.receive(&posted, 13).unwrap();
    let response = doc(
        &mut client,
        serde_json::json!({"op":"file_manifest","item_id":"it_drawing","version":1}),
        14,
    );
    assert_eq!(
        response.result["doc"],
        serde_json::to_value(&sealed.doc).unwrap()
    );
    let conversation = client.conversations();
    let message = conversation[0].messages.last().unwrap();
    assert!(message.file.is_none());
    assert_eq!(
        message.card.as_ref().unwrap()["app"]["app_id"],
        "excalidraw"
    );
    let snapshot = client.update(Vec::new()).unwrap().snapshot;
    let mut restored =
        Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
    let history = doc(
        &mut restored,
        serde_json::json!({"op":"file_manifest","item_id":"it_drawing","version":1}),
        15,
    );
    assert_eq!(history.result["doc"], response.result["doc"]);
    let installed=restored.app_request(&serde_json::json!({"op":"install_app","space_id":log.space(),"app_id":"excalidraw","args_json":"{}"}).to_string(),16,"en").unwrap();
    assert_eq!(installed.result["kind_id"], "app");
    let before = restored.saved.outbox.len();
    assert!(restored.app_request(&serde_json::json!({"op":"app_call_tool","item_id":"it_drawing","tool":"app_document_read","args_json":"{}","confirmed":false}).to_string(),17,"en").is_err());
    assert_eq!(restored.saved.outbox.len(), before);
}
