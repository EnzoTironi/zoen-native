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
    assert_eq!(
        must_err(restored.queue_message(space, "removed", 16)).to_string(),
        "You are no longer a member of this conversation."
    );
    assert_eq!(
        restored.device().unwrap().group_state(space).unwrap(),
        GroupState::Missing
    );
    let held = &restored.conversations()[0];
    assert!(held.removed);
    assert!(!held.can_send);
    assert_eq!(held.messages[0].text, "Native to web");
    assert_eq!(held.messages[1].text, "Web to native");
    let inner = author.sign_event(
        space,
        &roda_types::new_ulid(17),
        17,
        log.head(),
        EventBody::MessagePosted {
            message: "msg_after_removal".into(),
            text: "After removal".into(),
            attaches: None,
            reply: None,
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
        17,
        inner.seen.as_ref(),
        Sealed::new(SealedKind::Application, SUITE_ID, ciphertext),
    );
    restored
        .receive(
            &sequence(
                &mut log,
                Envelope::new(content, sig, author.cert.clone(), None).unwrap(),
            ),
            18,
        )
        .unwrap();
    restored
        .receive(
            &clear(
                &mut log,
                &author,
                EventBody::MemberAdded {
                    identity: identity.id(),
                    role: Role::Member,
                },
                19,
            ),
            19,
        )
        .unwrap();
    let live = &restored.conversations()[0];
    assert!(!live.removed);
    assert_eq!(live.messages.len(), 2);
    assert_eq!(live.messages[0].text, "Native to web");
    assert_eq!(live.messages[1].text, "Web to native");
    assert_eq!(live.unreadable, 1);
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

fn publishes(update: &Update) -> Vec<Envelope> {
    update
        .frames
        .iter()
        .filter_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Publish { env } => Some(env),
            _ => None,
        })
        .collect()
}

fn application(update: &Update) -> Option<Envelope> {
    publishes(update).into_iter().find(|env| {
        env.sealed_data()
            .is_some_and(|(kind, _)| kind == SealedKind::Application)
    })
}

fn must_err<T>(result: Result<T, ClientError>) -> ClientError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("expected an error"),
    }
}

fn waiting(client: &Client<HostSigner>) -> Option<String> {
    client.conversations()[0].waiting.clone()
}

#[test]
fn saved_session_version_1_is_refused_and_version_2_reloads() {
    let identity = NativeSigner::from_secret(&[31; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[32; 32]));
    let linked = account(&identity, &host.0);
    let err = must_err(Client::new(
        linked.clone(),
        host.clone(),
        r#"{"version":1,"storage":[],"logs":{},"outbox":{}}"#,
    ));
    assert_eq!(err, ClientError::OutOfDate);
    assert_eq!(
        err.to_string(),
        "This browser session is out of date. Pair this browser again."
    );
    let mut client = Client::new(linked.clone(), host.clone(), "").unwrap();
    let snapshot = client.bootstrap().unwrap().snapshot;
    let value: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(value["version"], 2);
    Client::new(linked, host, &snapshot).unwrap();
}

#[test]
fn a_duplicate_pending_invite_is_refused_and_invite_is_not_defaulted() {
    let identity = NativeSigner::from_secret(&[33; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[34; 32]));
    let linked = account(&identity, &host.0);
    let mut client = Client::new(linked.clone(), host.clone(), "").unwrap();
    let first = client
        .queue_join("sp_invite_dup", Role::Member, "0123456789", 2)
        .unwrap();
    let err = must_err(client.queue_join("sp_invite_dup", Role::Member, "0123456789", 3));
    assert_eq!(err, ClientError::DuplicateInvite);
    assert_eq!(err.to_string(), "This invitation is already pending.");
    assert_eq!(publishes(&first).len(), 1);
    let mut restored = Client::new(linked.clone(), host.clone(), &first.snapshot).unwrap();
    assert_eq!(
        must_err(restored.queue_join("sp_invite_dup", Role::Member, "0123456789", 4)),
        ClientError::DuplicateInvite
    );
    let mut value: serde_json::Value = serde_json::from_str(&first.snapshot).unwrap();
    let pending = value["outbox"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap();
    let removed = pending.as_object_mut().unwrap().remove("invite");
    assert_eq!(
        removed,
        Some(serde_json::Value::String("0123456789".into()))
    );
    let err = must_err(Client::new(linked, host, &value.to_string()));
    assert_ne!(err, ClientError::OutOfDate);
    assert_ne!(err, ClientError::DuplicateInvite);
}

struct WebGroup {
    sqlite: rusqlite::Connection,
    phone: NativeSigner,
    phone_key: NativeSigner,
    browser: NativeSigner,
    host: HostSigner,
    account: LinkedAccount,
    state_key: [u8; 32],
    space: String,
    log: SpaceLog,
    client: Client<HostSigner>,
}

impl WebGroup {
    fn open(tag: u8) -> Self {
        let phone = NativeSigner::from_secret(&[tag; 32]);
        let phone_key = NativeSigner::from_secret(&[tag.wrapping_add(1); 32]);
        let browser = NativeSigner::from_secret(&[tag.wrapping_add(2); 32]);
        let host = HostSigner(NativeSigner::from_secret(&[tag.wrapping_add(3); 32]));
        let account = account(&browser, &host.0);
        let author = Author::device(&phone, phone_key.clone());
        let mut sqlite = rusqlite::Connection::open_in_memory().unwrap();
        roda_mls::migrate(&mut sqlite).unwrap();
        let state_key = [tag.wrapping_add(4); 32];
        let native = Device::new(
            &sqlite,
            state_key,
            &phone.id(),
            phone_key.secret(),
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
        let space = format!("sp_u2_{tag}");
        let mut log = SpaceLog::new(&space);
        client
            .receive(
                &clear(
                    &mut log,
                    &author,
                    EventBody::SpaceCreated {
                        title: "U2".into(),
                        kind: SpaceKind::Group,
                        privacy: Privacy::EndToEnd,
                    },
                    1,
                ),
                1,
            )
            .unwrap();
        client
            .receive(
                &clear(
                    &mut log,
                    &author,
                    EventBody::MemberAdded {
                        identity: browser.id(),
                        role: Role::Member,
                    },
                    2,
                ),
                2,
            )
            .unwrap();
        native.create_group(&space).unwrap();
        let roster = BTreeSet::from([phone.id(), browser.id()]);
        let commit = native.commit(&space, &[package], &BTreeSet::new()).unwrap();
        native
            .open(&space, &commit.commit, &roster, native.leaf())
            .unwrap();
        client
            .receive(
                &sealed(&mut log, &author, SealedKind::Commit, commit.commit, 3),
                3,
            )
            .unwrap();
        client
            .receive(
                &sealed(
                    &mut log,
                    &author,
                    SealedKind::Welcome,
                    commit.welcome.unwrap(),
                    4,
                ),
                4,
            )
            .unwrap();
        client.receive(&ServerFrame::SyncDone.encode(), 5).unwrap();
        assert!(client.conversations()[0].can_send);
        Self {
            sqlite,
            phone,
            phone_key,
            browser,
            host,
            account,
            state_key,
            space,
            log,
            client,
        }
    }

    fn author(&self) -> Author {
        Author::device(&self.phone, self.phone_key.clone())
    }

    fn native(&self) -> Device<'_> {
        let author = self.author();
        Device::new(
            &self.sqlite,
            self.state_key,
            &self.phone.id(),
            self.phone_key.secret(),
            author.cert.as_deref().unwrap(),
        )
        .unwrap()
    }
}

fn plain_members(tag: u8) -> (Client<HostSigner>, SpaceLog, Author, String) {
    let phone = NativeSigner::from_secret(&[tag; 32]);
    let phone_key = NativeSigner::from_secret(&[tag.wrapping_add(1); 32]);
    let browser = NativeSigner::from_secret(&[tag.wrapping_add(2); 32]);
    let host = HostSigner(NativeSigner::from_secret(&[tag.wrapping_add(3); 32]));
    let author = Author::device(&phone, phone_key);
    let mut client = Client::new(account(&browser, &host.0), host, "").unwrap();
    let space = format!("sp_plain_{tag}");
    let mut log = SpaceLog::new(&space);
    client
        .receive(
            &clear(
                &mut log,
                &author,
                EventBody::SpaceCreated {
                    title: "Plain".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::EndToEnd,
                },
                1,
            ),
            1,
        )
        .unwrap();
    client
        .receive(
            &clear(
                &mut log,
                &author,
                EventBody::MemberAdded {
                    identity: browser.id(),
                    role: Role::Member,
                },
                2,
            ),
            2,
        )
        .unwrap();
    client.receive(&ServerFrame::SyncDone.encode(), 3).unwrap();
    (client, log, author, space)
}

#[test]
fn a_checkpoint_with_the_wrong_prefix_marks_the_space() {
    let (mut client, mut log, author, space) = plain_members(41);
    let head = log.head().unwrap();
    client
        .receive(
            &clear(
                &mut log,
                &author,
                EventBody::Checkpoint {
                    upto: Seen {
                        seq: head.seq,
                        hash: "not-the-local-hash".into(),
                    },
                    epoch: 9,
                    digest: "ab".repeat(32),
                },
                4,
            ),
            4,
        )
        .unwrap();
    assert_eq!(waiting(&client).as_deref(), Some(INTEGRITY_ERROR));
    assert_eq!(
        must_err(client.queue_message(&space, "stopped", 5)).to_string(),
        INTEGRITY_ERROR
    );
}

#[test]
fn a_checkpoint_ahead_of_the_chain_is_checked_when_the_chain_arrives() {
    let (mut client, mut log, author, space) = plain_members(42);
    let future = log.next_seq() + 3;
    client
        .receive(
            &clear(
                &mut log,
                &author,
                EventBody::Checkpoint {
                    upto: Seen {
                        seq: future,
                        hash: "claimed-hash".into(),
                    },
                    epoch: 9,
                    digest: "cd".repeat(32),
                },
                4,
            ),
            4,
        )
        .unwrap();
    assert_ne!(waiting(&client).as_deref(), Some(INTEGRITY_ERROR));
    let mut now = 5;
    while log.next_seq() <= future {
        client
            .receive(
                &clear(
                    &mut log,
                    &author,
                    EventBody::MessagePosted {
                        message: format!("msg_{now}"),
                        text: "later".into(),
                        attaches: None,
                        reply: None,
                    },
                    now,
                ),
                now,
            )
            .unwrap();
        now += 1;
    }
    assert_eq!(waiting(&client).as_deref(), Some(INTEGRITY_ERROR));
    assert_eq!(
        must_err(client.queue_message(&space, "stopped", now)).to_string(),
        INTEGRITY_ERROR
    );
}

#[test]
fn an_old_epoch_digest_still_conflicts_after_reload() {
    let mut group = WebGroup::open(51);
    let first = group
        .client
        .device()
        .unwrap()
        .checkpoint(&group.space)
        .unwrap();
    let commit = {
        let native = group.native();
        let roster = BTreeSet::from([group.phone.id(), group.browser.id()]);
        let advance = native.commit(&group.space, &[], &BTreeSet::new()).unwrap();
        native
            .open(&group.space, &advance.commit, &roster, native.leaf())
            .unwrap();
        advance.commit
    };
    let author = group.author();
    group
        .client
        .receive(
            &sealed(&mut group.log, &author, SealedKind::Commit, commit, 6),
            6,
        )
        .unwrap();
    let second = group
        .client
        .device()
        .unwrap()
        .checkpoint(&group.space)
        .unwrap();
    assert_ne!(first.0, second.0);
    let snapshot = group.client.update(Vec::new()).unwrap().snapshot;
    let mut restored = Client::new(group.account.clone(), group.host.clone(), &snapshot).unwrap();
    let upto = group.log.head().unwrap();
    restored
        .receive(
            &clear(
                &mut group.log,
                &author,
                EventBody::Checkpoint {
                    upto,
                    epoch: first.0,
                    digest: "ab".repeat(32),
                },
                7,
            ),
            7,
        )
        .unwrap();
    assert_eq!(waiting(&restored).as_deref(), Some(INTEGRITY_ERROR));
    assert_eq!(
        must_err(restored.queue_message(&group.space, "stopped", 8)).to_string(),
        INTEGRITY_ERROR
    );
}

#[test]
fn temporary_refusal_backs_off_durably_up_to_five_minutes_and_a_stale_seal_blocks_the_epoch() {
    let mut group = WebGroup::open(61);
    let first = group
        .client
        .queue_message(&group.space, "held", 10_000)
        .unwrap();
    let held = application(&first).unwrap();
    let held_id = held.client_id().to_string();
    let space = group.space.clone();
    let reject = |permanent: bool, reason: &str| {
        ServerFrame::Rejected {
            space: space.clone(),
            client_id: held_id.clone(),
            reason: reason.into(),
            permanent,
        }
        .encode()
    };
    let refused = group
        .client
        .receive(&reject(false, "slow down"), 10_000)
        .unwrap();
    assert_eq!(refused.retry_at_ms, Some(11_000));
    assert_eq!(
        group.client.conversations()[0]
            .messages
            .iter()
            .find(|message| message.text == "held")
            .unwrap()
            .delivery,
        "sending"
    );
    let second = group
        .client
        .queue_message(&group.space, "other", 10_500)
        .unwrap();
    let second_id = application(&second).unwrap().client_id().to_string();
    let ids = publishes(&second)
        .iter()
        .map(|env| env.client_id().to_string())
        .collect::<Vec<_>>();
    assert!(ids.contains(&second_id));
    assert!(!ids.contains(&held_id));
    let early = group.client.advance(10_999).unwrap();
    assert!(!publishes(&early)
        .iter()
        .any(|env| env.client_id() == held_id));
    let due = group.client.advance(11_000).unwrap();
    assert!(publishes(&due).iter().any(|env| env.client_id() == held_id));
    group
        .client
        .receive(&reject(false, "slow down"), 11_000)
        .unwrap();
    let early = group
        .client
        .receive(
            &ServerFrame::Pong {
                server_time_ms: None,
            }
            .encode(),
            12_999,
        )
        .unwrap();
    assert!(!publishes(&early)
        .iter()
        .any(|env| env.client_id() == held_id));
    let due = group
        .client
        .receive(
            &ServerFrame::Pong {
                server_time_ms: None,
            }
            .encode(),
            13_000,
        )
        .unwrap();
    assert!(publishes(&due).iter().any(|env| env.client_id() == held_id));
    let mut now = 13_000;
    for gap in [
        4_000_i64, 8_000, 16_000, 32_000, 64_000, 128_000, 256_000, 300_000, 300_000,
    ] {
        group
            .client
            .receive(&reject(false, "slow down"), now)
            .unwrap();
        let early = group
            .client
            .receive(
                &ServerFrame::Pong {
                    server_time_ms: None,
                }
                .encode(),
                now + gap - 1,
            )
            .unwrap();
        assert!(
            !publishes(&early)
                .iter()
                .any(|env| env.client_id() == held_id),
            "gap {gap}"
        );
        let due = group
            .client
            .receive(
                &ServerFrame::Pong {
                    server_time_ms: None,
                }
                .encode(),
                now + gap,
            )
            .unwrap();
        assert!(
            publishes(&due).iter().any(|env| env.client_id() == held_id),
            "gap {gap}"
        );
        now += gap;
    }
    let held_update = group
        .client
        .receive(&reject(false, "slow down"), now)
        .unwrap();
    assert!(!publishes(&held_update)
        .iter()
        .any(|env| env.client_id() == held_id));
    let mut restored = Client::new(
        group.account.clone(),
        group.host.clone(),
        &held_update.snapshot,
    )
    .unwrap();
    restored.bootstrap().unwrap();
    let early = restored
        .receive(&ServerFrame::SyncDone.encode(), now + 299_999)
        .unwrap();
    assert!(!publishes(&early)
        .iter()
        .any(|env| env.client_id() == held_id));
    let due = restored
        .receive(
            &ServerFrame::Pong {
                server_time_ms: None,
            }
            .encode(),
            now + 300_000,
        )
        .unwrap();
    assert!(publishes(&due).iter().any(|env| env.client_id() == held_id));
    group
        .client
        .receive(&reject(true, roda_proto::STALE_SEAL), now)
        .unwrap();
    assert_eq!(
        group.client.conversations()[0]
            .messages
            .iter()
            .find(|message| message.text == "held")
            .unwrap()
            .delivery,
        "sending"
    );
    assert!(
        must_err(group.client.queue_message(&group.space, "blocked", now + 1))
            .to_string()
            .contains("Waiting to catch up")
    );
    group
        .client
        .receive(&reject(true, "you were removed"), now + 1)
        .unwrap();
    let conversations = group.client.conversations();
    let failed = conversations[0]
        .messages
        .iter()
        .find(|message| message.text == "held")
        .unwrap();
    assert_eq!(failed.delivery, "failed");
    assert_eq!(failed.failure.as_deref(), Some("you were removed"));
}

#[test]
fn a_send_with_no_ack_reloads_the_same_ciphertext_and_confirms_once() {
    let mut group = WebGroup::open(71);
    let sending = group
        .client
        .queue_message(&group.space, "once", 11)
        .unwrap();
    let env = application(&sending).unwrap();
    let id = env.client_id().to_string();
    let sig = env.sig.clone();
    let ciphertext = env.sealed_data().unwrap().1.to_vec();
    let mut restored =
        Client::new(group.account.clone(), group.host.clone(), &sending.snapshot).unwrap();
    assert!(application(&restored.bootstrap().unwrap()).is_none());
    let again = restored
        .receive(&ServerFrame::SyncDone.encode(), 12)
        .unwrap();
    let env = application(&again).unwrap();
    assert_eq!(env.client_id(), id);
    assert_eq!(env.sig, sig);
    assert_eq!(env.sealed_data().unwrap().1, ciphertext.as_slice());
    let wire = sequence(&mut group.log, env);
    let confirmed = restored.receive(&wire, 13).unwrap();
    let copies = confirmed.conversations[0]
        .messages
        .iter()
        .filter(|message| message.id == id)
        .collect::<Vec<_>>();
    assert_eq!(copies.len(), 1);
    assert_eq!(copies[0].delivery, "confirmed");
    assert_eq!(copies[0].text, "once");
    let duplicate = restored.receive(&wire, 14).unwrap();
    assert_eq!(
        duplicate.conversations[0]
            .messages
            .iter()
            .filter(|message| message.id == id)
            .count(),
        1
    );
}

fn person(n: u16) -> Author {
    let mut identity = [0xA5; 32];
    let mut device = [0x5A; 32];
    identity[..2].copy_from_slice(&n.to_le_bytes());
    device[..2].copy_from_slice(&n.to_le_bytes());
    Author::device(
        &NativeSigner::from_secret(&identity),
        NativeSigner::from_secret(&device),
    )
}

fn enroll(
    client: &mut Client<HostSigner>,
    log: &mut SpaceLog,
    owner: &Author,
    member: &Author,
    now: i64,
) {
    client
        .receive(
            &clear(
                log,
                owner,
                EventBody::MemberAdded {
                    identity: member.identity.clone(),
                    role: Role::Member,
                },
                now,
            ),
            now,
        )
        .unwrap();
}

fn claim(
    client: &mut Client<HostSigner>,
    log: &mut SpaceLog,
    author: &Author,
    seq: u64,
    hash: &str,
    now: i64,
) {
    client
        .receive(
            &clear(
                log,
                author,
                EventBody::Checkpoint {
                    upto: Seen {
                        seq,
                        hash: hash.into(),
                    },
                    epoch: 4,
                    digest: "ee".repeat(32),
                },
                now,
            ),
            now,
        )
        .unwrap();
}

fn fill_until(
    client: &mut Client<HostSigner>,
    log: &mut SpaceLog,
    author: &Author,
    seq: u64,
    now: &mut i64,
) {
    while log.next_seq() < seq {
        client
            .receive(
                &clear(
                    log,
                    author,
                    EventBody::MessagePosted {
                        message: format!("msg_{now}"),
                        text: "later".into(),
                        attaches: None,
                        reply: None,
                    },
                    *now,
                ),
                *now,
            )
            .unwrap();
        *now += 1;
    }
}

#[test]
fn forty_near_claims_do_not_evict_an_honest_far_fork() {
    let (mut client, mut log, owner, space) = plain_members(81);
    let mut now = 20_i64;
    let mut colluders = Vec::new();
    for n in 0..40 {
        let author = person(n);
        enroll(&mut client, &mut log, &owner, &author, now);
        now += 1;
        colluders.push(author);
    }
    let honest = person(900);
    enroll(&mut client, &mut log, &owner, &honest, now);
    now += 1;
    let cursor = log.next_seq();
    let near = cursor + 100;
    let far = cursor + 180;
    claim(&mut client, &mut log, &honest, far, "far-fork", now);
    now += 1;
    for author in &colluders {
        claim(&mut client, &mut log, author, near, "near-fork", now);
        now += 1;
    }
    assert_ne!(waiting(&client).as_deref(), Some(INTEGRITY_ERROR));
    for (index, author) in colluders.iter().enumerate() {
        claim(
            &mut client,
            &mut log,
            author,
            far + 1 + index as u64,
            "later-fork",
            now,
        );
        now += 1;
    }
    assert_ne!(waiting(&client).as_deref(), Some(INTEGRITY_ERROR));
    fill_until(&mut client, &mut log, &owner, far, &mut now);
    assert_ne!(waiting(&client).as_deref(), Some(INTEGRITY_ERROR));
    client
        .receive(
            &clear(
                &mut log,
                &owner,
                EventBody::MessagePosted {
                    message: format!("msg_{now}"),
                    text: "fork".into(),
                    attaches: None,
                    reply: None,
                },
                now,
            ),
            now,
        )
        .unwrap();
    assert_eq!(waiting(&client).as_deref(), Some(INTEGRITY_ERROR));
    assert_eq!(
        must_err(client.queue_message(&space, "stopped", now + 1)).to_string(),
        INTEGRITY_ERROR
    );
}

#[test]
fn a_higher_claim_from_the_same_device_replaces_the_lower_and_detects_the_earlier_fork() {
    let (mut client, mut log, author, space) = plain_members(82);
    let mut now = 30_i64;
    let cursor = log.next_seq();
    let lower = cursor + 8;
    let higher = cursor + 16;
    claim(&mut client, &mut log, &author, lower, "lower-fork", now);
    now += 1;
    claim(&mut client, &mut log, &author, higher, "higher-fork", now);
    now += 1;
    fill_until(&mut client, &mut log, &author, lower + 1, &mut now);
    assert_ne!(waiting(&client).as_deref(), Some(INTEGRITY_ERROR));
    fill_until(&mut client, &mut log, &author, higher + 1, &mut now);
    assert_eq!(waiting(&client).as_deref(), Some(INTEGRITY_ERROR));
    assert_eq!(
        must_err(client.queue_message(&space, "stopped", now)).to_string(),
        INTEGRITY_ERROR
    );
}

#[test]
fn a_checkpoint_from_a_non_member_is_ignored() {
    let (mut client, mut log, owner, space) = plain_members(83);
    let stranger = person(77);
    let head = log.head().unwrap();
    claim(
        &mut client,
        &mut log,
        &stranger,
        head.seq,
        "not-the-local-hash",
        40,
    );
    assert_ne!(waiting(&client).as_deref(), Some(INTEGRITY_ERROR));
    let head = log.head().unwrap();
    claim(
        &mut client,
        &mut log,
        &owner,
        head.seq,
        "not-the-local-hash",
        41,
    );
    assert_eq!(waiting(&client).as_deref(), Some(INTEGRITY_ERROR));
    assert_eq!(
        must_err(client.queue_message(&space, "stopped", 42)).to_string(),
        INTEGRITY_ERROR
    );
}

#[test]
fn many_honest_checkpoints_out_of_order_raise_no_error() {
    let (mut client, mut log, owner, _space) = plain_members(84);
    let mut now = 50_i64;
    let mut authors = Vec::new();
    for n in 0..40 {
        let author = person(2_000 + n);
        enroll(&mut client, &mut log, &owner, &author, now);
        now += 1;
        authors.push(author);
    }
    for _ in 0..40 {
        client
            .receive(
                &clear(
                    &mut log,
                    &owner,
                    EventBody::MessagePosted {
                        message: format!("msg_{now}"),
                        text: "history".into(),
                        attaches: None,
                        reply: None,
                    },
                    now,
                ),
                now,
            )
            .unwrap();
        now += 1;
    }
    let history = log
        .events()
        .iter()
        .filter(|event| matches!(event.body, EventBody::MessagePosted { .. }))
        .map(|event| (event.seq, event.hash.clone()))
        .collect::<Vec<_>>();
    assert_eq!(history.len(), 40);
    for (author, (seq, hash)) in authors.iter().rev().zip(history.iter().rev()) {
        claim(&mut client, &mut log, author, *seq, hash, now);
        now += 1;
        assert_ne!(waiting(&client).as_deref(), Some(INTEGRITY_ERROR));
    }
    let head = log.head().unwrap();
    claim(
        &mut client,
        &mut log,
        &owner,
        head.seq,
        "not-the-local-hash",
        now,
    );
    assert_eq!(waiting(&client).as_deref(), Some(INTEGRITY_ERROR));
}

#[test]
fn a_removed_member_keeps_history_until_delete_and_returns_live_when_added() {
    let phone = NativeSigner::from_secret(&[71; 32]);
    let phone_key = NativeSigner::from_secret(&[72; 32]);
    let browser = NativeSigner::from_secret(&[73; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[74; 32]));
    let linked = account(&browser, &host.0);
    let author = Author::device(&phone, phone_key);
    let mut client = Client::new(linked.clone(), host.clone(), "").unwrap();
    let space = "sp_removed";
    let mut log = SpaceLog::new(space);
    let mut now = 1_i64;
    client
        .receive(
            &clear(
                &mut log,
                &author,
                EventBody::SpaceCreated {
                    title: "Kept".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::EndToEnd,
                },
                now,
            ),
            now,
        )
        .unwrap();
    now += 1;
    client
        .receive(
            &clear(
                &mut log,
                &author,
                EventBody::MemberAdded {
                    identity: linked.identity.clone(),
                    role: Role::Member,
                },
                now,
            ),
            now,
        )
        .unwrap();
    now += 1;
    client
        .receive(&ServerFrame::SyncDone.encode(), now)
        .unwrap();
    now += 1;
    client
        .receive(
            &clear(
                &mut log,
                &author,
                EventBody::MessagePosted {
                    message: "msg_before".into(),
                    text: "Before removal".into(),
                    attaches: None,
                    reply: None,
                },
                now,
            ),
            now,
        )
        .unwrap();
    now += 1;
    client
        .receive(
            &clear(
                &mut log,
                &author,
                EventBody::MemberRemoved {
                    identity: linked.identity.clone(),
                },
                now,
            ),
            now,
        )
        .unwrap();
    now += 1;
    client
        .receive(
            &clear(
                &mut log,
                &author,
                EventBody::MessagePosted {
                    message: "msg_after".into(),
                    text: "After removal".into(),
                    attaches: None,
                    reply: None,
                },
                now,
            ),
            now,
        )
        .unwrap();
    now += 1;
    client
        .receive(
            &sealed(
                &mut log,
                &author,
                SealedKind::Application,
                vec![9, 9, 9],
                now,
            ),
            now,
        )
        .unwrap();
    now += 1;

    let view = client
        .conversations()
        .into_iter()
        .find(|conversation| conversation.id == space)
        .unwrap();
    assert!(view.removed);
    assert!(!view.can_send);
    assert_eq!(view.messages.len(), 1);
    assert_eq!(view.messages[0].text, "Before removal");
    assert_eq!(
        must_err(client.queue_message(space, "no seal", now)).to_string(),
        "You are no longer a member of this conversation."
    );
    assert_eq!(
        client
            .conversations()
            .into_iter()
            .find(|conversation| conversation.id == space)
            .unwrap()
            .messages
            .len(),
        1
    );

    let other = "sp_other";
    let mut other_log = SpaceLog::new(other);
    client
        .receive(
            &clear(
                &mut other_log,
                &author,
                EventBody::SpaceCreated {
                    title: "Other".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::EndToEnd,
                },
                now,
            ),
            now,
        )
        .unwrap();
    now += 1;
    client
        .receive(
            &clear(
                &mut other_log,
                &author,
                EventBody::MemberAdded {
                    identity: linked.identity.clone(),
                    role: Role::Member,
                },
                now,
            ),
            now,
        )
        .unwrap();
    now += 1;
    client
        .receive(
            &clear(
                &mut other_log,
                &author,
                EventBody::MessagePosted {
                    message: "msg_other".into(),
                    text: "Other stays".into(),
                    attaches: None,
                    reply: None,
                },
                now,
            ),
            now,
        )
        .unwrap();
    now += 1;
    let both = client.conversations();
    assert_eq!(both.len(), 2);
    assert_eq!(
        both.iter()
            .find(|conversation| conversation.id == other)
            .unwrap()
            .messages[0]
            .text,
        "Other stays"
    );

    let deleted = client.delete_conversation(space).unwrap();
    assert_eq!(deleted.conversations.len(), 1);
    assert_eq!(deleted.conversations[0].id, other);
    assert_eq!(deleted.conversations[0].messages[0].text, "Other stays");
    let mut restored = Client::new(linked.clone(), host, &deleted.snapshot).unwrap();
    assert_eq!(restored.conversations().len(), 1);
    assert_eq!(restored.conversations()[0].id, other);
    assert_eq!(restored.conversations()[0].messages[0].text, "Other stays");

    restored
        .receive(
            &clear(
                &mut log,
                &author,
                EventBody::MemberAdded {
                    identity: linked.identity.clone(),
                    role: Role::Member,
                },
                now,
            ),
            now,
        )
        .unwrap();
    let live = restored
        .conversations()
        .into_iter()
        .find(|conversation| conversation.id == space)
        .unwrap();
    assert!(!live.removed);
    assert_eq!(
        live.messages
            .iter()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>(),
        vec!["Before removal", "After removal"]
    );
    assert_eq!(
        restored
            .conversations()
            .into_iter()
            .find(|conversation| conversation.id == other)
            .unwrap()
            .messages[0]
            .text,
        "Other stays"
    );
}

#[test]
fn a_departed_author_keeps_a_pending_fork_until_the_chain_reaches_it() {
    let (mut client, mut log, owner, space) = plain_members(85);
    let mut now = 80_i64;
    let author = person(850);
    enroll(&mut client, &mut log, &owner, &author, now);
    now += 1;
    let far = log.next_seq() + 6;
    claim(&mut client, &mut log, &author, far, "left-fork", now);
    now += 1;
    client
        .receive(
            &clear(
                &mut log,
                &owner,
                EventBody::MemberRemoved {
                    identity: author.identity.clone(),
                },
                now,
            ),
            now,
        )
        .unwrap();
    now += 1;
    assert_ne!(waiting(&client).as_deref(), Some(INTEGRITY_ERROR));
    fill_until(&mut client, &mut log, &owner, far + 1, &mut now);
    assert_eq!(waiting(&client).as_deref(), Some(INTEGRITY_ERROR));
    assert_eq!(
        must_err(client.queue_message(&space, "stopped", now)).to_string(),
        INTEGRITY_ERROR
    );
}
