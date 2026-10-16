use super::*;
use openmls_traits::{signatures::SignerError, types::SignatureScheme};
use roda_log::{Author, Signer as NativeSigner};
use roda_mls::Device;

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

fn account(identity: &NativeSigner, device: &NativeSigner) -> LinkedAccount {
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

fn sequence(log: &mut SpaceLog, env: Envelope) -> Vec<u8> {
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

fn clear(log: &mut SpaceLog, author: &Author, body: EventBody, now: i64) -> Vec<u8> {
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
    let mut original = crate::media::seal_file("voice.webm", "audio/webm", b"original", None)
        .unwrap()
        .doc;
    original.path = "VoiceNotes/native.webm".into();
    original.voice = Some(VoiceMeta {
        ms: 3400,
        levels: "08ff80".into(),
        transcript: "See you there".into(),
        reply: Some(ReplyRef::inline(&root)),
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
    threaded.voice.as_mut().unwrap().reply = Some(ReplyRef::thread(&root));
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
        Some(Some(ReplyRef::thread(&root)))
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
    client.logs.insert(space.into(), log.clone());
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
    append(&mut log, &author, revert(1)); // Version 3 is a copy of version 1.
    client.logs.insert(space.into(), log.clone());
    assert_eq!(
        client.conversations()[0].messages[1].file.as_ref(),
        Some(&original)
    );
    append(&mut log, &author, revert(2)); // Version 4 is a copy of version 2.
    append(&mut log, &author, revert(3)); // Reverts can target earlier reverts.
    client.logs.insert(space.into(), log);
    let conversation = &client.conversations()[0];
    assert_eq!(conversation.messages.len(), 3);
    assert_eq!(conversation.messages[1].file.as_ref(), Some(&original));
    assert!(conversation.messages[1].edited);
}
