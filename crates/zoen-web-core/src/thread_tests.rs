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
        },
    );
    assert_eq!(client.conversations()[0].threads_unread, 1);
    let snapshot = client.update(Vec::new()).unwrap().snapshot;
    let restored = Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
    assert_eq!(restored.conversations()[0].threads_unread, 1);
}

#[test]
fn browser_thread_state_is_encrypted_durable_and_read_is_idempotent() {
    let (mut client, _, other, root, hash) = fixture();
    let last = reply(&mut client, &other, &root, false);
    // A failed Personal Space write must not hide unread state or prevent retry.
    assert!(client.mark_thread_read("chat", &hash, 199).is_err());
    assert_eq!(client.conversations()[0].threads_unread, 1);
    client.device().unwrap().create_group("personal").unwrap();
    client
        .set_thread_follow("chat", &hash, Some(true), 200)
        .unwrap();
    client.mark_thread_read("chat", &hash, 201).unwrap();
    let count = client.saved.outbox.len();
    assert_eq!(client.conversations()[0].threads_unread, 0);
    let update = client.mark_thread_read("chat", &hash, 202).unwrap();
    assert_eq!(client.saved.outbox.len(), count);
    let pending = client.saved.outbox.values().find(|p| matches!(&p.inner.as_ref().unwrap().body, EventBody::ThreadStateSet { read_up_to, .. } if *read_up_to == last)).unwrap();
    assert_eq!(pending.envelope().unwrap().space(), "personal");
    assert!(pending.envelope().unwrap().plain_event().is_none());
    assert!(
        matches!(&pending.inner.as_ref().unwrap().body, EventBody::ThreadStateSet { root: r, follow: Some(true), .. } if r == &root)
    );
    let restored = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &update.snapshot,
    )
    .unwrap();
    assert_eq!(restored.conversations()[0].threads_unread, 0);
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
fn native_and_browser_exchange_encrypted_personal_thread_state() {
    let (mut client, _, other, root, hash) = fixture();
    let last = reply(&mut client, &other, &root, false);
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
    let update = client.mark_thread_read("chat", &hash, 302).unwrap();
    let env = update
        .frames
        .into_iter()
        .find_map(|f| match ClientFrame::decode(&f).unwrap() {
            ClientFrame::Publish { env }
                if env.space() == "personal"
                    && env
                        .sealed_data()
                        .is_some_and(|(k, _)| k == SealedKind::Application) =>
            {
                Some(env)
            }
            _ => None,
        })
        .unwrap();
    let from = Leaf {
        identity: client.account.identity.clone(),
        device: client.account.device.clone(),
        cert: client.account.cert.clone(),
    };
    let Opened::Application { plaintext, .. } = native
        .open("personal", env.sealed_data().unwrap().1, &roster, &from)
        .unwrap()
    else {
        panic!("MLS application expected")
    };
    let opened = InnerEvent::decode(&plaintext).unwrap();
    let event = roda_log::event_from_content(
        opened.content,
        opened.sig,
        Some(client.account.cert.clone()),
        0,
        String::new(),
        String::new(),
    )
    .unwrap();
    roda_log::verify_author(&event).unwrap();
    assert!(
        matches!(&event.body, EventBody::ThreadStateSet { root: r, read_up_to, follow: None, .. } if r == &root && *read_up_to == last)
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
    assert_eq!(view.messages[0].thread_unread, 1);
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
fn mentions_require_a_whole_handle() {
    assert!(mentions("Hi @Fixture!", "fixture"));
    assert!(!mentions("@fixture2", "fixture"));
    assert!(!mentions("mail@fixture.com", "fixture"));
}
