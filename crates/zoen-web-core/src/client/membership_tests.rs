use super::super::tests::{account, sequence, HostSigner};
use super::*;
use event_log::{
    profile::{AgreementKey, ProfileFields, ProfileKey},
    Signer as NativeSigner,
};
use mls::Device;

fn published(update: &Update) -> Vec<Envelope> {
    update
        .frames
        .iter()
        .filter_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Publish { env } => Some(env),
            _ => None,
        })
        .collect()
}

fn claim(update: &Update) -> (String, Vec<String>) {
    update
        .frames
        .iter()
        .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Req {
                id: CLAIM_REQUEST,
                op:
                    Op::ClaimKeyPackages {
                        ids,
                        operation_id: Some(operation),
                    },
            } => Some((operation, ids)),
            _ => None,
        })
        .expect("an authenticated member-key claim must be emitted")
}

fn handshake(update: &Update, kind: SealedKind) -> Envelope {
    published(update)
        .into_iter()
        .find(|env| env.sealed_kind() == Some(kind))
        .expect("the expected ordered handshake must be emitted")
}

struct Fixture {
    client: Client<HostSigner>,
    account: LinkedAccount,
    host: HostSigner,
    bob: NativeSigner,
    bob_device: NativeSigner,
    bob_agreement: AgreementKey,
    sqlite: rusqlite::Connection,
    space: String,
    log: SpaceLog,
    staged: Update,
    operation: String,
    now: i64,
}

fn fixture() -> Fixture {
    let alice = NativeSigner::from_secret(&[101; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[102; 32]));
    let bob = NativeSigner::from_secret(&[103; 32]);
    let bob_device = NativeSigner::from_secret(&[104; 32]);
    let alice_agreement = AgreementKey::from_secret([105; 32]);
    let bob_agreement = AgreementKey::from_secret([106; 32]);
    let mut account = account(&alice, &host.0);
    account.agreement_secret = hex::encode(alice_agreement.secret());
    account.profile_key = ProfileKey::generate().to_hex();
    account.profile_key_version = 1;
    account.profile_seen_version = 1;
    account.profile_fields = hex::encode(
        ProfileFields {
            name: "Alice privado".into(),
            bio: String::new(),
            photo: None,
        }
        .to_bytes(),
    );
    let mut sqlite = rusqlite::Connection::open_in_memory().unwrap();
    mls::migrate(&mut sqlite).unwrap();
    let package = {
        let native = Device::new(
            &sqlite,
            [107; 32],
            &bob.id(),
            bob_device.secret(),
            &bob.sign(&event_log::device_cert_message(&bob_device.id())),
        )
        .unwrap();
        native.key_packages(1, false).unwrap().pop().unwrap()
    };
    let mut client = Client::new(account.clone(), host.clone(), "").unwrap();
    client.saved.profiles.insert(
        bob.id(),
        Identity {
            id: bob.id(),
            kind: types::IdentityKind::Person,
            name: String::new(),
            handle: "bob".into(),
            tint_hex: "#4F7CFF".into(),
            glyph: None,
            owner: None,
            owner_proof: None,
            bio: String::new(),
        },
    );
    client
        .saved
        .profile
        .agreement
        .insert(bob.id(), bob_agreement.public_hex());
    let now = 1_800_000_000_000;
    client.claim_clock(&now.to_string()).unwrap();
    client
        .receive(&ServerFrame::SyncDone.encode(), now)
        .unwrap();
    let created = client
        .membership_request(
            &json!({
                "op": "create_group", "title": "Browser → iOS", "member_ids": [bob.id()],
            })
            .to_string(),
            now + 1,
        )
        .unwrap();
    let space = created.result.as_str().unwrap().to_owned();
    let mut log = SpaceLog::new(&space);
    let outgoing = published(created.update.as_ref().unwrap());
    assert_eq!(
        outgoing.len(),
        1,
        "MemberAdded waits for the actual SpaceCreated receipt"
    );
    assert!(matches!(
        outgoing[0].body(),
        Some(EventBody::SpaceCreated {
            privacy: Privacy::EndToEnd,
            ..
        })
    ));
    assert!(outgoing[0].seen().is_none());
    let member = client
        .receive(&sequence(&mut log, outgoing[0].clone()), now + 2)
        .unwrap();
    let addition = published(&member).into_iter().find(|env| matches!(env.body(),
        Some(EventBody::MemberAdded { identity, role: Role::Member }) if identity == bob.id()
    )).unwrap();
    assert_eq!(
        addition.seen(),
        log.head(),
        "the member event observes the ordered creation"
    );
    let claiming = client
        .receive(&sequence(&mut log, addition), now + 3)
        .unwrap();
    let (operation, targets) = claim(&claiming);
    assert_eq!(targets, vec![bob.id()]);
    assert_eq!(client.saved.membership.claims[&space].operation, operation);

    // A lost key-package response retries the same durable claim, not a new
    // consumptive operation. The native package is still held in real SQLite.
    let mut client = Client::new(account.clone(), host.clone(), &claiming.snapshot).unwrap();
    client.claim_clock(&now.to_string()).unwrap();
    client.bootstrap().unwrap();
    let resumed = client
        .receive(&ServerFrame::SyncDone.encode(), now + 4)
        .unwrap();
    assert_eq!(claim(&resumed), (operation.clone(), vec![bob.id()]));
    let staged = client
        .receive(
            &ServerFrame::Res {
                id: CLAIM_REQUEST,
                result: Ok(Reply::KeyPackages(vec![KeyPackageRecord {
                    identity: bob.id(),
                    device: bob_device.id(),
                    data: package,
                }])),
            }
            .encode(),
            now + 5,
        )
        .unwrap();
    assert!(!client.saved.membership.claims.contains_key(&space));
    assert!(!published(&staged).iter().any(|env| matches!(
        env.sealed_kind(),
        Some(SealedKind::Commit | SealedKind::Welcome)
    )));
    Fixture {
        client,
        account,
        host,
        bob,
        bob_device,
        bob_agreement,
        sqlite,
        space,
        log,
        staged,
        operation,
        now: now + 5,
    }
}

#[test]
fn browser_created_membership_claim_recovery_barrier_and_welcome_join_native_sqlite() {
    let mut f = fixture();
    let commit = f
        .client
        .saved
        .outbox
        .values()
        .find(|pending| pending.envelope().unwrap().sealed_kind() == Some(SealedKind::Commit))
        .unwrap()
        .envelope()
        .unwrap();
    let recovery = event_log::recovery::RecoveryRef::parse(commit.recovery().unwrap()).unwrap();
    let blob = f.client.saved.local_chunks.get(&recovery.blob).unwrap();
    assert_eq!(hex::encode(Sha256::digest(blob)), recovery.blob);
    assert_eq!(
        f.client.pending_document_uploads(),
        vec![recovery.blob.clone()]
    );
    let group_info = event_log::recovery::open(
        &f.space,
        recovery.epoch,
        &f.account.device,
        &f.bob.id(),
        &f.bob_agreement,
        blob,
    )
    .unwrap();
    let summary =
        mls::recovery::verify_group_info(&f.space, &group_info, Some(&f.account.device)).unwrap();
    assert_eq!(
        summary.identities,
        BTreeSet::from([f.account.identity.clone(), f.bob.id()])
    );

    // Neither a page reload nor SyncDone may release the commit before durable
    // upload acknowledgment; the welcome additionally waits for its exact commit.
    let mut client = Client::new(f.account.clone(), f.host.clone(), &f.staged.snapshot).unwrap();
    let startup = client.bootstrap().unwrap();
    assert!(!published(&startup).iter().any(|env| matches!(
        env.sealed_kind(),
        Some(SealedKind::Commit | SealedKind::Welcome)
    )));
    let synced = client
        .receive(&ServerFrame::SyncDone.encode(), f.now + 1)
        .unwrap();
    assert!(!published(&synced).iter().any(|env| matches!(
        env.sealed_kind(),
        Some(SealedKind::Commit | SealedKind::Welcome)
    )));
    let uploaded = client.document_uploaded(&[recovery.blob]).unwrap();
    let commit = handshake(&uploaded, SealedKind::Commit);
    assert!(!published(&uploaded)
        .iter()
        .any(|env| env.sealed_kind() == Some(SealedKind::Welcome)));
    let accepted = client
        .receive(&sequence(&mut f.log, commit), f.now + 2)
        .unwrap();
    let welcome = handshake(&accepted, SealedKind::Welcome);
    let roster = BTreeSet::from([f.account.identity.clone(), f.bob.id()]);
    {
        let native = Device::new(
            &f.sqlite,
            [107; 32],
            &f.bob.id(),
            f.bob_device.secret(),
            &f.bob
                .sign(&event_log::device_cert_message(&f.bob_device.id())),
        )
        .unwrap();
        assert_eq!(native.group_state(&f.space).unwrap(), GroupState::Missing);
        assert!(native
            .join(&f.space, welcome.sealed_data().unwrap().1, &roster)
            .unwrap());
        assert_eq!(
            native.checkpoint(&f.space).unwrap(),
            client.device().unwrap().checkpoint(&f.space).unwrap()
        );
    }
    client
        .receive(&sequence(&mut f.log, welcome), f.now + 3)
        .unwrap();
    let sent = client
        .queue_message(&f.space, "Criado na Web, aberto no iOS 🌱", f.now + 4)
        .unwrap();
    let application = handshake(&sent, SealedKind::Application);
    // Reopen the native adapter after joining: interoperability includes durable
    // native group state, not just matching in-memory cryptographic objects.
    let native = Device::new(
        &f.sqlite,
        [107; 32],
        &f.bob.id(),
        f.bob_device.secret(),
        &f.bob
            .sign(&event_log::device_cert_message(&f.bob_device.id())),
    )
    .unwrap();
    let sender = Leaf {
        identity: f.account.identity.clone(),
        device: f.account.device.clone(),
        cert: f.account.cert.clone(),
    };
    let Opened::Application { plaintext, from } = native
        .open(
            &f.space,
            application.sealed_data().unwrap().1,
            &roster,
            &sender,
        )
        .unwrap()
    else {
        panic!("the native member must decrypt the browser application");
    };
    assert_eq!(from, sender);
    let inner = InnerEvent::decode(&plaintext).unwrap();
    let event = event_log::event_from_content(
        inner.content,
        inner.sig,
        Some(f.account.cert.clone()),
        0,
        String::new(),
        String::new(),
    )
    .unwrap();
    event_log::verify_author(&event).unwrap();
    assert!(
        matches!(event.body, EventBody::MessagePosted { text, .. } if text == "Criado na Web, aberto no iOS 🌱")
    );
}

#[test]
fn refused_browser_commit_abandons_pending_epoch_and_never_releases_its_welcome() {
    let mut f = fixture();
    assert!(f
        .client
        .device()
        .unwrap()
        .committed_leaves(&f.space)
        .unwrap()
        .is_none());
    let committed = f.client.device().unwrap().leaves(&f.space).unwrap();
    let commit = f
        .client
        .saved
        .outbox
        .values()
        .find(|pending| pending.envelope().unwrap().sealed_kind() == Some(SealedKind::Commit))
        .unwrap()
        .envelope()
        .unwrap();
    let welcome = f
        .client
        .saved
        .outbox
        .values()
        .find(|pending| pending.envelope().unwrap().sealed_kind() == Some(SealedKind::Welcome))
        .unwrap()
        .envelope()
        .unwrap();
    let refused = f
        .client
        .receive(
            &ServerFrame::Rejected {
                space: f.space.clone(),
                client_id: commit.client_id().into(),
                reason: "stale_commit".into(),
                permanent: true,
            }
            .encode(),
            f.now + 1,
        )
        .unwrap();
    assert!(!f.client.saved.outbox.contains_key(commit.client_id()));
    assert!(!f.client.saved.outbox.contains_key(welcome.client_id()));
    assert_eq!(
        f.client
            .device()
            .unwrap()
            .committed_leaves(&f.space)
            .unwrap()
            .unwrap(),
        committed
    );
    assert!(!published(&refused).iter().any(|env| matches!(
        env.sealed_kind(),
        Some(SealedKind::Commit | SealedKind::Welcome)
    )));
    let mut reloaded = Client::new(f.account, f.host, &refused.snapshot).unwrap();
    reloaded.claim_clock(&f.now.to_string()).unwrap();
    let resumed = reloaded
        .receive(&ServerFrame::SyncDone.encode(), f.now + 501)
        .unwrap();
    assert_ne!(
        claim(&resumed).0,
        f.operation,
        "a refused epoch requires a fresh claim operation"
    );
    assert!(!published(&resumed)
        .iter()
        .any(|env| env.sealed_kind() == Some(SealedKind::Welcome)));
}
