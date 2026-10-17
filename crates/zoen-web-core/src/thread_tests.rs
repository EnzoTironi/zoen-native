use super::*;
use openmls_traits::{signatures::SignerError, types::SignatureScheme};
use roda_log::{Author, Signer as NativeSigner};

#[derive(Clone)]
struct HostSigner(NativeSigner);
impl Signer for HostSigner {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SignerError> {
        hex::decode(self.0.sign(payload)).map_err(|_| SignerError::SigningError)
    }
    fn signature_scheme(&self) -> SignatureScheme {
        roda_mls::SUITE.signature_algorithm()
    }
}

fn append(log: &mut SpaceLog, author: &Author, body: EventBody) -> Event {
    let event = author.sign_event(
        log.space(),
        &roda_types::new_ulid(100),
        100,
        log.head(),
        body,
    );
    log.sequence(event).clone()
}

fn fixture() -> (Client<HostSigner>, Author, Author, String, String) {
    let identity = NativeSigner::from_secret(&[51; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[52; 32]));
    let account = LinkedAccount {
        identity: identity.id(),
        device: host.0.id(),
        cert: identity.sign(&roda_log::device_cert_message(&host.0.id())),
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
    };
    let me = Author::device(&identity, host.0.clone());
    let other = Author::root(NativeSigner::from_secret(&[53; 32]));
    let mut client = Client::new(account, host, "").unwrap();
    let mut chat = SpaceLog::new("chat");
    append(
        &mut chat,
        &me,
        EventBody::SpaceCreated {
            title: "Chat".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
    );
    let root = append(
        &mut chat,
        &other,
        EventBody::MessagePosted {
            message: "root".into(),
            text: "@fixture hi".into(),
            attaches: None,
            reply: None,
            forwarded: false,
        },
    );
    client.logs.insert("chat".into(), chat);
    let mut personal = SpaceLog::new("personal");
    append(
        &mut personal,
        &me,
        EventBody::SpaceCreated {
            title: "Personal".into(),
            kind: SpaceKind::Personal,
            privacy: Privacy::EndToEnd,
        },
    );
    client.logs.insert("personal".into(), personal);
    client.synced = true;
    (client, me, other, root.client_id, root.hash)
}

fn reply(client: &mut Client<HostSigner>, author: &Author, root: &str, also_in_chat: bool) -> u64 {
    append(
        client.logs.get_mut("chat").unwrap(),
        author,
        EventBody::MessagePosted {
            message: roda_types::new_secret_id("msg"),
            text: "reply".into(),
            attaches: None,
            reply: Some(roda_types::ReplyRef {
                to: root.into(),
                thread: true,
                also_in_chat,
            }),
            forwarded: false,
        },
    )
    .seq
}

#[test]
fn synced_follow_read_and_also_in_chat_project_native_roots() {
    let (mut client, me, other, root, hash) = fixture();
    let first = reply(&mut client, &other, &root, false);
    let view = client.conversations().remove(0);
    assert_eq!(view.threads_unread, 1); // automatic follow by whole-handle mention
    assert_eq!(view.unread, 1); // root only, not the thread-only reply
    assert_eq!(view.messages[1].thread.as_deref(), Some(hash.as_str()));
    append(
        client.logs.get_mut("personal").unwrap(),
        &me,
        EventBody::ThreadStateSet {
            space: "chat".into(),
            root: root.clone(),
            follow: Some(false),
            read_up_to: first,
            read_only: false,
        },
    );
    reply(&mut client, &other, &root, true);
    assert_eq!(client.conversations()[0].threads_unread, 0); // explicit opt-out wins
    assert_eq!(client.conversations()[0].unread, 2); // also-in-chat counts here
    append(
        client.logs.get_mut("personal").unwrap(),
        &me,
        EventBody::ThreadStateSet {
            space: "chat".into(),
            root: root.clone(),
            follow: Some(true),
            read_up_to: first,
            read_only: false,
        },
    );
    assert_eq!(client.conversations()[0].threads_unread, 1); // only after watermark
    append(
        client.logs.get_mut("personal").unwrap(),
        &me,
        EventBody::ThreadStateSet {
            space: "chat".into(),
            root: root.clone(),
            follow: None,
            read_up_to: 0,
            read_only: false,
        },
    );
    assert_eq!(client.conversations()[0].threads_unread, 1); // stale device must not regress the read watermark
                                                             // A similarly shaped event in the chat is not a personal preference.
    append(
        client.logs.get_mut("chat").unwrap(),
        &me,
        EventBody::ThreadStateSet {
            space: "chat".into(),
            root: root.clone(),
            follow: Some(false),
            read_up_to: 0,
            read_only: false,
        },
    );
    assert_eq!(client.conversations()[0].threads_unread, 1);
    let snapshot = client.update(Vec::new()).unwrap().snapshot;
    let restored = Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
    assert_eq!(restored.conversations()[0].threads_unread, 1);
}

#[test]
fn personal_preferences_fail_closed_without_encryption_keys() {
    let (mut client, _, other, root, hash) = fixture();
    reply(&mut client, &other, &root, false);
    // Without the Personal MLS group, fail closed rather than publish preferences.
    client.synced = false;
    assert!(client.mark_thread_read("chat", &hash, 199).is_err());
    assert_eq!(client.conversations()[0].threads_unread, 1);
    client.synced = true;
    assert!(client
        .set_thread_follow("chat", &hash, Some(true), 200)
        .is_err());
    assert!(client.mark_thread_read("chat", &hash, 201).is_err());
    assert!(client.saved.outbox.is_empty());
    assert_eq!(client.conversations()[0].threads_unread, 1);
}

fn deliver(client: &mut Client<HostSigner>, log: &mut SpaceLog, env: Envelope) {
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
    client
        .receive(
            &ServerFrame::Event {
                ev: Sequenced {
                    seq: event.seq,
                    prev: event.prev.clone(),
                    hash: event.hash.clone(),
                    env,
                },
            }
            .encode(),
            300,
        )
        .unwrap();
}

#[test]
fn personal_thread_state_is_encrypted_and_native_can_receive_it() {
    let (mut client, _, other, root, hash) = fixture();
    reply(&mut client, &other, &root, false);
    let mut voice = crate::media::seal_file("voice.webm", "audio/webm", b"voice", None)
        .unwrap()
        .doc;
    voice.voice = Some(roda_types::VoiceMeta {
        reply: Some(roda_types::ReplyRef {
            to: root.clone(),
            thread: true,
            also_in_chat: false,
        }),
        ..Default::default()
    });
    let last = append(
        client.logs.get_mut("chat").unwrap(),
        &other,
        EventBody::ItemCreated {
            item: "voice_reply".into(),
            kind: roda_types::ItemKind::File,
            content: roda_types::ItemContent::File(voice),
            origin: String::new(),
        },
    )
    .seq;
    let package = client
        .bootstrap()
        .unwrap()
        .frames
        .into_iter()
        .find_map(|f| match ClientFrame::decode(&f).unwrap() {
            ClientFrame::Req {
                op: Op::PublishKeyPackages { packages, .. },
                ..
            } => packages.into_iter().next(),
            _ => None,
        })
        .unwrap();
    let identity = NativeSigner::from_secret(&[51; 32]);
    let native_key = NativeSigner::from_secret(&[54; 32]);
    let author = Author::device(&identity, native_key.clone());
    let mut sqlite = rusqlite::Connection::open_in_memory().unwrap();
    roda_mls::migrate(&mut sqlite).unwrap();
    let native = roda_mls::Device::new(
        &sqlite,
        [55; 32],
        &identity.id(),
        native_key.secret(),
        author.cert.as_deref().unwrap(),
    )
    .unwrap();
    native.create_group("personal").unwrap();
    let roster = BTreeSet::from([identity.id()]);
    let commit = native
        .commit("personal", &[package], &BTreeSet::new())
        .unwrap();
    native
        .open("personal", &commit.commit, &roster, native.leaf())
        .unwrap();
    let mut log =
        SpaceLog::from_events("personal", client.logs["personal"].events().to_vec()).unwrap();
    for (kind, bytes) in [
        (SealedKind::Commit, commit.commit),
        (SealedKind::Welcome, commit.welcome.unwrap()),
    ] {
        let (content, sig) = author.sign_sealed(
            "personal",
            &roda_types::new_ulid(250),
            250,
            log.head().as_ref(),
            Sealed::new(kind, SUITE_ID, bytes),
        );
        deliver(
            &mut client,
            &mut log,
            Envelope::new(content, sig, author.cert.clone(), None).unwrap(),
        );
    }
    client
        .receive(&ServerFrame::SyncDone.encode(), 301)
        .unwrap();
    // A reply arrives after the UI captured `last`, before its queued read runs.
    reply(&mut client, &other, &root, false);
    let update = client
        .mark_thread_read_up_to("chat", &hash, last, 302)
        .unwrap();
    let count = client.saved.outbox.len();
    client
        .mark_thread_read_up_to("chat", &hash, last, 302)
        .unwrap();
    assert_eq!(client.saved.outbox.len(), count);
    let restored = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &update.snapshot,
    )
    .unwrap();
    assert_eq!(
        restored.conversations()[0].threads_unread,
        1,
        "the later unseen reply stays unread"
    );
    let env = update
        .frames
        .into_iter()
        .find_map(|f| match ClientFrame::decode(&f).unwrap() {
            ClientFrame::Publish { env } if env.space() == "personal" && env.is_sealed() => {
                Some(env)
            }
            _ => None,
        })
        .unwrap();
    assert!(env.plain_event().is_none());
    assert!(!String::from_utf8_lossy(env.content()).contains("ThreadStateSet"));
    let (_, ciphertext) = env.sealed_data().unwrap();
    let Opened::Application { plaintext, from } = native
        .open("personal", ciphertext, &roster, native.leaf())
        .unwrap()
    else {
        panic!("expected application")
    };
    let sequenced = Sequenced {
        seq: log.events().len() as u64 + 1,
        prev: log.head().unwrap().hash,
        hash: String::new(),
        env: env.clone(),
    };
    let event = inner_event(&sequenced, &plaintext, &from).unwrap();
    roda_log::verify_author(&event).unwrap();
    assert!(
        matches!(&event.body, EventBody::ThreadStateSet { root: r, read_up_to, follow: None, read_only: true, .. } if r == &root && *read_up_to == last)
    );
    deliver(&mut client, &mut log, env);
    // The phone explicitly unfollows after reading. Browser must consume the same state.
    let inner = author.sign_event(
        "personal",
        &roda_types::new_ulid(303),
        303,
        log.head(),
        EventBody::ThreadStateSet {
            space: "chat".into(),
            root: root.clone(),
            follow: Some(false),
            read_up_to: last,
            read_only: false,
        },
    );
    let ciphertext = native
        .seal(
            "personal",
            &InnerEvent {
                content: inner.content.clone(),
                sig: inner.sig.clone(),
            }
            .encode(),
        )
        .unwrap();
    let (content, sig) = author.sign_sealed(
        "personal",
        &inner.client_id,
        inner.at_ms,
        inner.seen.as_ref(),
        Sealed::new(SealedKind::Application, SUITE_ID, ciphertext),
    );
    deliver(
        &mut client,
        &mut log,
        Envelope::new(content, sig, author.cert.clone(), None).unwrap(),
    );
    reply(&mut client, &other, &root, false);
    let view = client.conversations().remove(0);
    assert!(!view.messages[0].thread_following);
    assert_eq!(view.threads_unread, 0);
    assert_eq!(view.messages[0].thread_unread, 2);
}

#[test]
fn missing_personal_group_requests_device_join_like_a_chat() {
    let (mut client, _, _, _, _) = fixture();
    let update = client
        .receive(&ServerFrame::SyncDone.encode(), 400)
        .unwrap();
    assert!(update.frames.iter().any(|f| matches!(ClientFrame::decode(f).unwrap(), ClientFrame::Publish { env } if env.space() == "personal" && matches!(env.plain_event().map(|e| e.body), Some(EventBody::DeviceJoining { .. })))));
}

#[test]
fn many_threads_project_hash_and_client_aliases_without_cross_counting() {
    let (mut client, me, other, _, _) = fixture();
    for i in 0..128 {
        let root = append(
            client.logs.get_mut("chat").unwrap(),
            &me,
            EventBody::MessagePosted {
                message: format!("root-{i}"),
                text: format!("root {i}"),
                attaches: None,
                reply: None,
                forwarded: false,
            },
        );
        reply(&mut client, &other, &root.hash, false);
        reply(&mut client, &other, &root.client_id, false);
    }
    let view = client.conversations().remove(0);
    assert_eq!(view.threads_unread, 256);
    for root in view.messages.iter().filter(|m| m.text.starts_with("root ")) {
        assert!(root.thread_following);
        assert_eq!(root.thread_unread, 2);
    }
}

#[test]
fn hash_references_beat_later_client_aliases_in_thread_projection() {
    let (mut client, _, other, root_id, root_hash) = fixture();
    let shadow = {
        let log = client.logs.get_mut("chat").unwrap();
        let event = other.sign_event(
            "chat",
            &root_hash,
            101,
            log.head(),
            EventBody::MessagePosted {
                message: "shadow".into(),
                text: "A different post".into(),
                attaches: None,
                reply: None,
                forwarded: false,
            },
        );
        log.sequence(event).clone()
    };
    reply(&mut client, &other, &root_hash, false);

    assert_eq!(client.thread_root("chat", &root_hash), root_id);
    let view = client.conversations().remove(0);
    let root = view
        .messages
        .iter()
        .find(|message| message.entry.as_deref() == Some(root_hash.as_str()))
        .unwrap();
    let shadow = view
        .messages
        .iter()
        .find(|message| message.entry.as_deref() == Some(shadow.hash.as_str()))
        .unwrap();
    assert_eq!(root.replies, 1);
    assert_eq!(shadow.replies, 0);
    assert!(view.messages.iter().any(|message| {
        message.text == "reply" && message.thread.as_deref() == Some(root_hash.as_str())
    }));
}

#[test]
fn colliding_thread_aliases_keep_follow_and_read_state_on_the_selected_root() {
    let (mut client, me, other, _, first_hash) = fixture();
    let shadow = {
        let log = client.logs.get_mut("chat").unwrap();
        let event = other.sign_event(
            "chat",
            &first_hash,
            101,
            log.head(),
            EventBody::MessagePosted {
                message: "shadow".into(),
                text: "Second root".into(),
                attaches: None,
                reply: None,
                forwarded: false,
            },
        );
        log.sequence(event).clone()
    };
    let canonical = client.thread_root("chat", &shadow.hash);
    assert_eq!(
        canonical, shadow.hash,
        "an alias that resolves to another hash is not safe to serialize"
    );
    let last = reply(&mut client, &other, &shadow.hash, false);
    append(
        client.logs.get_mut("personal").unwrap(),
        &me,
        EventBody::ThreadStateSet {
            space: "chat".into(),
            root: canonical,
            follow: Some(true),
            read_up_to: last,
            read_only: false,
        },
    );
    let view = client.conversations().remove(0);
    let shadow = view
        .messages
        .iter()
        .find(|m| m.entry.as_deref() == Some(shadow.hash.as_str()))
        .unwrap();
    assert_eq!(shadow.id, shadow.entry.as_deref().unwrap());
    assert!(shadow.thread_following);
    assert_eq!(shadow.thread_unread, 0);
    assert_eq!(view.threads_unread, 0);
    assert_eq!(client.thread_state("chat", &first_hash), (None, 0));
}

#[test]
fn thread_mentions_only_count_unseen_incoming_whole_handles() {
    let (mut client, me, other, root, _) = fixture();
    let old = append(
        client.logs.get_mut("chat").unwrap(),
        &other,
        EventBody::MessagePosted {
            message: "old-mention".into(),
            text: "@fixture old".into(),
            attaches: None,
            reply: Some(roda_types::ReplyRef {
                to: root.clone(),
                thread: true,
                also_in_chat: false,
            }),
            forwarded: false,
        },
    )
    .seq;
    append(
        client.logs.get_mut("personal").unwrap(),
        &me,
        EventBody::ThreadStateSet {
            space: "chat".into(),
            root: root.clone(),
            follow: Some(true),
            read_up_to: old,
            read_only: false,
        },
    );
    for (author, text) in [
        (&other, "@Fixture new"),
        (&other, "@fixture2 not a mention"),
        (&me, "@fixture own"),
    ] {
        append(
            client.logs.get_mut("chat").unwrap(),
            author,
            EventBody::MessagePosted {
                message: roda_types::new_secret_id("msg"),
                text: text.into(),
                attaches: None,
                reply: Some(roda_types::ReplyRef {
                    to: root.clone(),
                    thread: true,
                    also_in_chat: false,
                }),
                forwarded: false,
            },
        );
    }
    let view = client.conversations().remove(0);
    assert_eq!(view.messages[0].thread_unread, 2);
    assert_eq!(view.messages[0].thread_mentions, 1);
    assert_eq!(
        serde_json::to_value(&view.messages[0]).unwrap()["thread_mentions"],
        1
    );
    let last = client.logs["chat"].events().last().unwrap().seq;
    append(
        client.logs.get_mut("personal").unwrap(),
        &me,
        EventBody::ThreadStateSet {
            space: "chat".into(),
            root,
            follow: Some(true),
            read_up_to: last,
            read_only: false,
        },
    );
    let view = client.conversations().remove(0);
    assert!(view.messages[0].thread_following); // still discoverable after reading
    assert_eq!(view.messages[0].thread_unread, 0);
    assert_eq!(view.messages[0].thread_mentions, 0);
}

#[test]
fn mentions_require_a_whole_handle() {
    assert!(mentions("Hi @Fixture!", "fixture"));
    assert!(!mentions("@fixture2", "fixture"));
    assert!(!mentions("mail@fixture.com", "fixture"));
}

#[test]
fn personal_follow_uses_timestamp_not_log_order_and_read_never_regresses() {
    for newer_id in ["aaa", "zzz"] {
        let (mut client, me, _, root, _) = fixture();
        let genesis = client.logs["personal"].events()[0].body.clone();
        for (id, at, follow, read) in [
            ("personal", 200, Some(true), 9),
            (newer_id, 300, Some(false), 2),
        ] {
            let mut log = SpaceLog::new(id);
            append(&mut log, &me, genesis.clone());
            let event = me.sign_event(
                id,
                &roda_types::new_ulid(at),
                at,
                log.head(),
                EventBody::ThreadStateSet {
                    space: "chat".into(),
                    root: root.clone(),
                    follow,
                    read_up_to: read,
                    read_only: false,
                },
            );
            log.sequence(event);
            client.logs.insert(id.into(), log);
        }
        assert_eq!(
            client.thread_state("chat", &root),
            (Some(false), 9),
            "newer log {newer_id}"
        );
    }
}

#[test]
fn legacy_snapshot_never_republishes_plaintext_preferences() {
    let (mut client, _, _, root, _) = fixture();
    let pending = client
        .clear_event(
            "personal",
            EventBody::ThreadStateSet {
                space: "chat".into(),
                root,
                follow: Some(false),
                read_up_to: 4,
                read_only: false,
            },
            400,
        )
        .unwrap();
    let id = pending.envelope().unwrap().client_id().to_string();
    client.saved.outbox.insert(id, pending);
    let update = client.update(Vec::new()).unwrap();
    assert!(!update.frames.iter().any(|frame| matches!(ClientFrame::decode(frame).unwrap(), ClientFrame::Publish { env } if env.plain_event().is_some_and(|event| matches!(event.body, EventBody::ThreadStateSet { .. })))));
    let mut restored = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &update.snapshot,
    )
    .unwrap();
    let update = restored.update(Vec::new()).unwrap();
    assert!(!update.frames.iter().any(|frame| matches!(ClientFrame::decode(frame).unwrap(), ClientFrame::Publish { env } if env.plain_event().is_some_and(|event| matches!(event.body, EventBody::ThreadStateSet { .. })))));
}

fn root_with_alias(client: &mut Client<HostSigner>, author: &Author, alias: &str) -> Event {
    let log = client.logs.get_mut("chat").unwrap();
    let event = author.sign_event(
        "chat",
        alias,
        500,
        log.head(),
        EventBody::MessagePosted {
            message: "alias-root".into(),
            text: "Alias root".into(),
            attaches: None,
            reply: None,
            forwarded: false,
        },
    );
    Envelope::plain(&event).verify().unwrap();
    log.sequence(event).clone()
}

#[test]
fn reply_aliases_empty_ids_retain_the_authoritative_hash() {
    let (mut client, _, other, _, _) = fixture();
    let root = root_with_alias(&mut client, &other, "");
    assert_eq!(client.thread_root("chat", &root.hash), root.hash);
    for thread in [false, true] {
        assert_eq!(
            client.reply_ref("chat", root.hash.clone(), thread, true).to,
            root.hash
        );
    }
}

#[test]
fn reply_aliases_control_client_ids_share_native_follow_and_read_state() {
    let (mut client, me, other, _, _) = fixture();
    let alias = client.logs["chat"].events()[0].client_id.clone();
    let root = root_with_alias(&mut client, &other, &alias);
    assert_eq!(client.thread_root("chat", &root.hash), alias);
    assert_eq!(client.resolve_post("chat", &alias).unwrap().hash, root.hash);
    let last = reply(&mut client, &other, &root.hash, false);
    append(
        client.logs.get_mut("personal").unwrap(),
        &me,
        EventBody::ThreadStateSet {
            space: "chat".into(),
            root: alias,
            follow: Some(true),
            read_up_to: last,
            read_only: false,
        },
    );
    let view = client.conversations().remove(0);
    let root = view
        .messages
        .iter()
        .find(|m| m.entry.as_deref() == Some(root.hash.as_str()))
        .unwrap();
    assert!(root.thread_following);
    assert_eq!(root.thread_unread, 0);
    assert_eq!(view.threads_unread, 0);
}

#[test]
fn chat_mentions_use_whole_handles_including_broadcast_but_not_hidden_replies() {
    let (mut client, me, other, root, _) = fixture();
    client.account.name = "Ana Silva".into();
    client.account.handle = "ana".into();
    for (author, text, reply) in [
        (&other, "Oi @ana!", None),
        (&other, "@ana.silva is a different handle", None),
        (&other, "mail@ana is not a mention", None),
        (&me, "@ana own", None),
        (&other, "@ANA new", None),
        (
            &other,
            "@ana hidden",
            Some(roda_types::ReplyRef::thread(&root)),
        ),
        (
            &other,
            "@ana broadcast",
            Some(roda_types::ReplyRef {
                to: root,
                thread: true,
                also_in_chat: true,
            }),
        ),
    ] {
        append(
            client.logs.get_mut("chat").unwrap(),
            author,
            EventBody::MessagePosted {
                message: roda_types::new_secret_id("msg"),
                text: text.into(),
                attaches: None,
                reply,
                forwarded: false,
            },
        );
    }
    assert_eq!(client.conversations()[0].mentions, 3);
}

#[test]
fn pending_thread_preferences_do_not_lose_to_a_same_millisecond_confirmation() {
    let (mut client, me, _, root, _) = fixture();
    let body = |follow| EventBody::ThreadStateSet {
        space: "chat".into(),
        root: root.clone(),
        follow: Some(follow),
        read_up_to: 0,
        read_only: false,
    };
    let log = client.logs.get_mut("personal").unwrap();
    let confirmed = me.sign_event(
        "personal",
        &roda_types::new_ulid(200),
        200,
        log.head(),
        body(true),
    );
    log.sequence(confirmed);
    let mut pending = client.clear_event("personal", body(false), 200).unwrap();
    let inner = pending.envelope().unwrap().plain_event().unwrap();
    pending.inner = Some(inner.clone());
    client.saved.outbox.insert(inner.client_id.clone(), pending);
    assert_eq!(client.thread_state("chat", &root), (Some(false), 0));
    client
        .logs
        .get_mut("personal")
        .unwrap()
        .sequence(inner.clone());
    client.saved.outbox.remove(&inner.client_id);
    assert_eq!(client.thread_state("chat", &root), (Some(false), 0));
}

#[test]
fn read_only_thread_events_do_not_restore_following() {
    let (mut client, me, _, root, _) = fixture();
    for (follow, read_only, read_up_to) in [(Some(false), false, 1), (Some(true), true, 99)] {
        append(
            client.logs.get_mut("personal").unwrap(),
            &me,
            EventBody::ThreadStateSet {
                space: "chat".into(),
                root: root.clone(),
                follow,
                read_up_to,
                read_only,
            },
        );
    }
    assert_eq!(client.thread_state("chat", &root), (Some(false), 99));
}
