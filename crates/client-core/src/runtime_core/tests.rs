use super::publication::{MAX_PUBLISH_BYTES, MAX_REPLY_BYTES};
use super::*;
use event_log::{
    chain_hash,
    content::{InnerEvent, Sealed, SealedKind},
    SpaceLog,
};
use protocol::{ClientFrame, Envelope, KeyPackageClaimClock, KeyPackageRecord};

const SPACE: &str = "certified-owner-agent-direct";

mod canonical_approvals;

struct Fixture {
    space: String,
    owner: Engine,
    core: DeviceCore,
    owner_profile: Identity,
    agent_profile: Identity,
    agent_device: Signer,
    agent_cert: String,
}

fn profile(root: &Signer, kind: IdentityKind) -> Identity {
    Identity {
        id: root.id(),
        kind,
        name: "Certified test principal".into(),
        handle: "certified".into(),
        tint_hex: "#4F7CFF".into(),
        glyph: None,
        owner: None,
        bio: String::new(),
        owner_proof: None,
    }
}

fn ordered(engine: &Engine, envelope: Envelope) -> Sequenced {
    let log = engine.logs.get(envelope.space());
    let seq = log.map(SpaceLog::next_seq).unwrap_or(0);
    let prev = log
        .and_then(|l| l.events().last())
        .map(|e| e.hash.clone())
        .unwrap_or_else(|| GENESIS_PREV.into());
    Sequenced {
        seq,
        hash: chain_hash(envelope.space(), seq, &prev, &envelope.wire_hash()),
        prev,
        env: envelope,
    }
}

impl Fixture {
    fn credential(&self) -> UnlockedDevice {
        UnlockedDevice::verify(
            self.agent_profile.clone(),
            self.owner_profile.clone(),
            self.agent_cert.clone(),
            self.agent_device.secret(),
        )
        .unwrap()
    }

    fn new() -> Self {
        Self::new_in(SPACE)
    }

    fn new_in(space_id: &str) -> Self {
        let owner_root = Signer::generate();
        let owner_author = Author::device(&owner_root, Signer::generate());
        let owner_profile = profile(&owner_root, IdentityKind::Person);
        let agent_root = Signer::generate();
        let agent_device = Signer::generate();
        let agent_author = Author::device(&agent_root, agent_device.clone());
        let mut agent_profile = profile(&agent_root, IdentityKind::Agent);
        agent_profile.owner = Some(owner_profile.id.clone());
        agent_profile.owner_proof = Some(Box::new(
            agent_owner::authorize(&owner_author, &agent_root.id()).unwrap(),
        ));
        let agent_cert = agent_author.cert.unwrap();
        let credential = UnlockedDevice::verify(
            agent_profile.clone(),
            owner_profile.clone(),
            agent_cert.clone(),
            agent_device.secret(),
        )
        .unwrap();
        let mut core = DeviceCore::bootstrap(credential, "http://relay.test").unwrap();
        let mut owner = Engine::open(":memory:").unwrap();
        owner.store.put_identity(&owner_profile, None).unwrap();
        owner.store.put_identity(&agent_profile, None).unwrap();
        owner.store.set_meta("me", &owner_profile.id).unwrap();
        owner
            .save_linked_account(AccountMeta {
                identity: owner_profile.id.clone(),
                device: owner_author.key.id(),
                cert: owner_author.cert.clone().unwrap(),
                relay_url: "http://relay.test".into(),
                registered: false,
            })
            .unwrap();
        owner.reload().unwrap();
        owner.net.author = Some(owner_author);
        owner
            .init_profile(&owner_profile.id, &owner_profile.name)
            .unwrap();
        core.engine
            .init_profile(&agent_profile.id, &agent_profile.name)
            .unwrap();
        let mut fixture = Self {
            space: space_id.into(),
            owner,
            core,
            owner_profile,
            agent_profile,
            agent_device,
            agent_cert,
        };
        fixture.clear(EventBody::SpaceCreated {
            title: "Owner and Agent".into(),
            kind: SpaceKind::Direct,
            privacy: Privacy::EndToEnd,
        });
        fixture.clear(EventBody::MemberAdded {
            identity: fixture.agent_profile.id.clone(),
            role: Role::Member,
        });
        fixture.core.engine.net.profiles.supported = true;
        let (public, signed) = fixture.core.engine.agreement_to_publish().unwrap();
        fixture.owner.agreement_keys_arrived(
            vec![protocol::AgreementKeyRecord {
                identity: fixture.agent_profile.id.clone(),
                public,
                signed,
            }],
            std::slice::from_ref(&fixture.agent_profile.id),
        );
        fixture.owner.create_mls_group(space_id).unwrap();
        let clock = KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap();
        let (space, targets, operation) = fixture.owner.mls_to_claim(Some(clock)).unwrap().unwrap();
        assert_eq!(space, space_id);
        assert_eq!(targets, vec![fixture.agent_profile.id.clone()]);
        let package = fixture
            .core
            .engine
            .device()
            .unwrap()
            .key_packages(1, false)
            .unwrap()
            .remove(0);
        fixture.owner.mls_claimed(
            space_id,
            &operation,
            Ok(vec![KeyPackageRecord {
                identity: fixture.agent_profile.id.clone(),
                device: fixture.agent_device.id(),
                data: package,
            }]),
        );
        for (sha, _) in fixture.owner.pending_uploads(100) {
            fixture.owner.upload_done(&sha);
        }
        let mut handshakes = 0;
        while fixture.owner.outbox_len() > 0 {
            let frames = fixture.owner.outbox_envelopes();
            assert!(!frames.is_empty(), "real protocol outbox must advance");
            for envelope in frames {
                let frame = ordered(&fixture.owner, envelope);
                assert_eq!(fixture.owner.ingest(frame.clone()), Ingest::Confirmed);
                fixture.core.ingest(vec![frame]).unwrap();
                handshakes += 1;
                assert!(handshakes < 12);
            }
        }
        assert!(fixture.core.engine.ready_epoch(space_id).is_some());
        fixture
    }

    fn clear(&mut self, body: EventBody) -> String {
        let author = self.owner.net.author.as_ref().unwrap();
        let seen = self.owner.logs.get(&self.space).and_then(SpaceLog::head);
        let event = author.sign_event(&self.space, &new_id("clear"), 100, seen, body);
        let frame = ordered(&self.owner, Envelope::plain(&event));
        assert_eq!(self.owner.ingest(frame.clone()), Ingest::Applied);
        self.core.ingest(vec![frame.clone()]).unwrap();
        frame.hash
    }

    fn encrypted(&mut self, body: EventBody, at_ms: i64) -> String {
        let author = self.owner.net.author.clone().unwrap();
        self.owner
            .append_synced(&self.space, &author, at_ms, body)
            .unwrap();
        self.owner.mls_seal_outbox().unwrap();
        let frames = self.owner.outbox_envelopes();
        assert_eq!(frames.len(), 1);
        let frame = ordered(&self.owner, frames.into_iter().next().unwrap());
        assert_eq!(self.owner.ingest(frame.clone()), Ingest::Confirmed);
        self.core.ingest(vec![frame.clone()]).unwrap();
        frame.hash
    }

    fn grant(&mut self, id: &str, expiry: Option<i64>) -> String {
        self.encrypted(
            EventBody::GrantIssued {
                grant: Grant {
                    id: id.into(),
                    grantor: self.owner_profile.id.clone(),
                    grantee: Some(self.agent_profile.id.clone()),
                    scope: GrantScope::Space(self.space.clone()),
                    capability: Capability::Trust(TrustLevel::Listen),
                    expires_at_ms: expiry,
                },
            },
            200,
        )
    }

    fn message(&mut self, text: &str) -> String {
        self.encrypted(
            EventBody::MessagePosted {
                message: new_id("message"),
                text: text.into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
            300,
        )
    }
}

#[test]
fn bounded_discovery_cursor_survives_complete_image() {
    let mut fixture = Fixture::new();
    fixture.grant("discovery-owner-trust", Some(1000));
    for index in 0..17 {
        fixture.message(&format!("Retained request {index}"));
    }
    let first = fixture.core.discover_replies(SPACE, 400).unwrap();
    assert_eq!(first.replies.len(), 16);
    assert!(first.changed && !first.complete && first.examined <= 64);
    let first_triggers: Vec<_> = first
        .replies
        .iter()
        .map(|facts| facts.intent().trigger().to_string())
        .collect();
    let image = fixture.core.image().unwrap();
    let mut reopened = DeviceCore::restore(image, fixture.credential()).unwrap();
    let second = reopened.discover_replies(SPACE, 400).unwrap();
    assert_eq!(second.replies.len(), 1);
    assert!(second.changed && second.complete);
    assert!(!first_triggers.contains(&second.replies[0].intent().trigger().to_string()));
    let repeated = reopened.discover_replies(SPACE, 400).unwrap();
    assert!(repeated.replies.is_empty() && repeated.complete && !repeated.changed);
    assert_eq!(repeated.examined, 0);
}

#[test]
fn prepared_reply_preserves_exact_native_outbox_through_cold_restore_and_confirmation() {
    let mut fixture = Fixture::new();
    fixture.grant("publication-owner-trust", Some(1000));
    let trigger = fixture.message("Original reply request");
    let facts = fixture.core.reply(SPACE, &trigger, 400).unwrap();
    let original = facts.intent().clone();
    let frontier = facts.frontier().clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Retained model result\n", 500)
        .unwrap();
    assert_eq!(prepared.facts().intent(), &original);
    assert_eq!(prepared.facts().frontier(), &frontier);
    assert_eq!(prepared.facts().expires_at_ms(), Some(1000));
    let envelope = prepared.envelope().clone();
    envelope.verify().unwrap();
    assert!(envelope.is_sealed() && !envelope.is_pruned());
    assert_eq!(envelope.author(), fixture.agent_profile.id);
    assert_eq!(envelope.device(), Some(fixture.agent_device.id().as_str()));
    assert_eq!(envelope.cert.as_ref(), Some(&fixture.agent_cert));
    let wire = ClientFrame::Publish {
        env: envelope.clone(),
    }
    .encode();
    let mut cold = DeviceCore::restore(
        NativeImage::decode_bounded(prepared.image().as_bytes().to_vec()).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    let pending = cold.engine.store.outbox().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].event.client_id, envelope.client_id());
    assert_eq!(
        event_log::content_hash_of(&pending[0].event),
        prepared.event_digest()
    );
    assert!(matches!(
        &pending[0].event.body,
        EventBody::MessagePosted { message, text, attaches: None, reply: None, media: None, forwarded: false, }
            if message == prepared.message() && text == "Retained model result\n"
    ));
    assert_eq!(cold.engine.outbox_envelopes(), vec![envelope.clone()]);
    assert_eq!(
        ClientFrame::Publish {
            env: cold.engine.outbox_envelopes().remove(0)
        }
        .encode(),
        wire
    );
    let frame = ordered(&fixture.owner, envelope);
    assert_eq!(fixture.owner.ingest(frame.clone()), Ingest::Applied);
    let received = fixture.owner.logs[SPACE].events().last().unwrap();
    assert!(matches!(
        &received.body,
        EventBody::MessagePosted { message, text, attaches: None, reply: None, media: None, forwarded: false, }
            if message == prepared.message() && text == "Retained model result\n"
    ));
    cold.ingest(vec![frame.clone()]).unwrap();
    assert_eq!(cold.engine.store.outbox_len().unwrap(), 0);
    assert_eq!(cold.retained_head(SPACE).unwrap().unwrap().hash, frame.hash);
    assert!(cold.engine.logs[SPACE]
        .events()
        .iter()
        .all(|event| !matches!(event.body, EventBody::UsageRecorded { .. })));
    println!("JOURNEY prepared reply keeps certified sealed wire and own pending effect in complete cold image; actual accepted echo clears outbox without native usage charge");
}

fn colliding_native_reply(fixture: &Fixture, prepared: &PreparedReply) -> Envelope {
    let fork = DeviceCore::restore(
        NativeImage::decode_bounded(prepared.image().as_bytes().to_vec()).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    let original = prepared.envelope();
    let author = fork.engine.net.author.as_ref().unwrap();
    let event = author.sign_event(
        SPACE,
        original.client_id(),
        original.at_ms(),
        original.seen(),
        EventBody::MessagePosted {
            message: new_id("different-message"),
            text: "Different native result".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    );
    let bytes = fork
        .engine
        .device()
        .unwrap()
        .seal(
            SPACE,
            &InnerEvent {
                content: event.content,
                sig: event.sig,
            }
            .encode(),
        )
        .unwrap();
    Envelope::sealed(
        author,
        SPACE,
        original.client_id(),
        original.at_ms(),
        original.seen().as_ref(),
        Sealed::new(SealedKind::Application, ::mls::SUITE_ID, bytes),
    )
}

#[test]
fn generic_ingest_refuses_colliding_ciphertext_plaintext_pruning_and_changed_header() {
    for case in ["ciphertext", "plaintext", "pruned", "timestamp"] {
        let mut fixture = Fixture::new();
        fixture.grant("echo-original-trust", None);
        let trigger = fixture.message("Original source request");
        let original = fixture
            .core
            .reply(SPACE, &trigger, 400)
            .unwrap()
            .intent()
            .clone();
        let prepared = fixture
            .core
            .prepare_reply(&original, "Original native result", 500)
            .unwrap();
        let committed = prepared.image().as_bytes().to_vec();
        let expected = prepared.envelope().clone();
        let author = fixture.core.engine.net.author.as_ref().unwrap();
        let envelope = match case {
            "ciphertext" => colliding_native_reply(&fixture, &prepared),
            "plaintext" => Envelope::plain(
                &fixture
                    .core
                    .engine
                    .store
                    .outbox_get(expected.client_id())
                    .unwrap()
                    .unwrap(),
            ),
            "pruned" => expected.pruned().unwrap(),
            "timestamp" => Envelope::sealed(
                author,
                SPACE,
                expected.client_id(),
                expected.at_ms() + 1,
                expected.seen().as_ref(),
                Sealed::new(
                    SealedKind::Application,
                    ::mls::SUITE_ID,
                    expected.sealed_data().unwrap().1.to_vec(),
                ),
            ),
            _ => unreachable!(),
        };
        envelope.verify().unwrap();
        assert_eq!(envelope.client_id(), expected.client_id());
        assert_eq!(envelope.author(), expected.author());
        assert_eq!(envelope.device(), expected.device());
        assert_ne!(envelope, expected);
        if case == "pruned" {
            assert_eq!(envelope.wire_hash(), expected.wire_hash());
        }
        let frame = ordered(&fixture.owner, envelope);
        if case == "ciphertext" {
            assert_eq!(fixture.owner.ingest(frame.clone()), Ingest::Applied);
            let actual = fixture.owner.logs[SPACE].events().last().unwrap();
            assert!(
                matches!(&actual.body, EventBody::MessagePosted { text, .. } if text == "Different native result")
            );
            assert!(crate::mls_opening::opened_leaf(&fixture.owner.store, actual).is_some());
        }
        let before = fixture.core.engine.logs[SPACE].head();
        assert!(
            matches!(fixture.core.ingest(vec![frame]), Err(NativeError::History)),
            "{case}"
        );
        assert!(fixture.core.image().is_err());
        assert_eq!(fixture.core.engine.logs[SPACE].head(), before);
        assert_eq!(fixture.core.engine.store.outbox_len().unwrap(), 1);
        let cold = DeviceCore::restore(
            NativeImage::decode_bounded(committed).unwrap(),
            fixture.credential(),
        )
        .unwrap();
        assert_eq!(cold.engine.outbox_envelopes(), vec![expected]);
    }
    println!("JOURNEY generic facade refuses a different genuinely opened MLS result with valid same CID/device, signed plaintext, same-hash pruned frame and signed changed timestamp before own confirmation");
}

#[test]
fn own_echo_requires_valid_original_inner_and_historical_sealing_copy() {
    for case in [
        "missing-copy",
        "bad-copy",
        "changed-inner",
        "signed-inner",
        "missing-reference",
        "missing-native-pending",
    ] {
        let mut fixture = Fixture::new();
        fixture.grant("copy-original-trust", None);
        let trigger = fixture.message("Retained source request");
        let original = fixture
            .core
            .reply(SPACE, &trigger, 400)
            .unwrap()
            .intent()
            .clone();
        let prepared = fixture
            .core
            .prepare_reply(&original, "Retained native output", 500)
            .unwrap();
        let committed = prepared.image().as_bytes().to_vec();
        let envelope = prepared.envelope().clone();
        let key = format!("mls.sealed:{}", envelope.client_id());
        match case {
            "missing-copy" => fixture.core.engine.store.meta_delete(&key).unwrap(),
            "bad-copy" => fixture
                .core
                .engine
                .store
                .set_meta(&key, "wrong:copy:signature")
                .unwrap(),
            "changed-inner" => {
                let mut pending = fixture
                    .core
                    .engine
                    .store
                    .outbox_get(envelope.client_id())
                    .unwrap()
                    .unwrap();
                if let EventBody::MessagePosted { text, .. } = &mut pending.body {
                    *text = "Altered retained inner view".into();
                }
                fixture
                    .core
                    .engine
                    .store
                    .outbox_remove(envelope.client_id())
                    .unwrap();
                fixture.core.engine.store.outbox_put(&pending).unwrap();
            }
            "signed-inner" => {
                let pending = fixture.core.engine.net.author.as_ref().unwrap().sign_event(
                    SPACE,
                    envelope.client_id(),
                    envelope.at_ms(),
                    envelope.seen(),
                    EventBody::MessagePosted {
                        message: prepared.message().into(),
                        text: "Another genuinely signed native inner".into(),
                        attaches: None,
                        reply: None,
                        media: None,
                        forwarded: false,
                    },
                );
                event_log::verify_author(&pending).unwrap();
                assert_ne!(
                    event_log::content_hash_of(&pending),
                    prepared.event_digest()
                );
                fixture
                    .core
                    .engine
                    .store
                    .outbox_remove(envelope.client_id())
                    .unwrap();
                fixture.core.engine.store.outbox_put(&pending).unwrap();
                fixture.core =
                    DeviceCore::restore(fixture.core.image().unwrap(), fixture.credential())
                        .unwrap();
                let roundtrip = fixture
                    .core
                    .engine
                    .store
                    .outbox_get(envelope.client_id())
                    .unwrap()
                    .unwrap();
                assert_eq!(roundtrip, pending);
                assert_eq!(
                    fixture.core.engine.sealed_copy(&roundtrip).unwrap().1,
                    envelope
                );
            }
            "missing-reference" => fixture
                .core
                .engine
                .store
                .meta_delete(&super::reference::reference_key(envelope.client_id()))
                .unwrap(),
            "missing-native-pending" => {
                fixture.core.engine.net.pending.remove(envelope.client_id());
            }
            _ => unreachable!(),
        }
        let frame = ordered(&fixture.owner, envelope.clone());
        let before = fixture.core.engine.logs[SPACE].head();
        assert!(
            matches!(fixture.core.ingest(vec![frame]), Err(NativeError::History)),
            "{case}"
        );
        assert!(fixture.core.image().is_err());
        assert_eq!(fixture.core.engine.logs[SPACE].head(), before);
        let cold = DeviceCore::restore(
            NativeImage::decode_bounded(committed).unwrap(),
            fixture.credential(),
        )
        .unwrap();
        assert_eq!(cold.engine.outbox_envelopes(), vec![envelope]);
    }
    println!("JOURNEY missing/malformed copy, altered view, another genuinely signed inner after real store roundtrip, missing original reference and inconsistent pending map refuse before confirmation");
}

#[test]
fn lost_sync_marker_or_scope_cannot_downgrade_a_retained_sealed_reply() {
    for case in ["sync-marker", "scope", "both"] {
        let mut fixture = Fixture::new();
        fixture.grant("marker-original-trust", None);
        let trigger = fixture.message("Original marker request");
        let original = fixture
            .core
            .reply(SPACE, &trigger, 400)
            .unwrap()
            .intent()
            .clone();
        let prepared = fixture
            .core
            .prepare_reply(&original, "Exact sealed output", 500)
            .unwrap();
        let key = super::reference::reference_key(prepared.envelope().client_id());
        let reference = fixture.core.engine.store.meta(&key).unwrap().unwrap();
        if case != "scope" {
            fixture
                .core
                .engine
                .store
                .conn()
                .execute("DELETE FROM synced_spaces WHERE space = ?1", [SPACE])
                .unwrap();
            fixture.core =
                DeviceCore::restore(fixture.core.image().unwrap(), fixture.credential()).unwrap();
            assert!(!fixture.core.engine.net.synced.contains(SPACE));
        }
        if case != "sync-marker" {
            fixture.core.engine.state.spaces.remove(SPACE);
        }
        assert!(!fixture.core.engine.is_e2e(SPACE));
        let pending = fixture
            .core
            .engine
            .store
            .outbox_get(prepared.envelope().client_id())
            .unwrap()
            .unwrap();
        let plain = Envelope::plain(&pending);
        plain.verify().unwrap();
        let before = fixture.core.engine.logs[SPACE].head();
        assert!(
            matches!(
                fixture.core.ingest(vec![ordered(&fixture.owner, plain)]),
                Err(NativeError::History)
            ),
            "{case}"
        );
        assert!(fixture.core.image().is_err());
        assert_eq!(fixture.core.engine.logs[SPACE].head(), before);
        assert_eq!(fixture.core.engine.store.outbox_len().unwrap(), 1);
        assert_eq!(
            fixture.core.engine.store.meta(&key).unwrap().as_deref(),
            Some(reference.as_str())
        );
    }
    println!("JOURNEY actual persisted sync-marker loss and missing current scope projection cannot turn the original sealed Application into an accepted signed plaintext echo; original reference survives");
}

#[test]
fn exact_old_epoch_echo_survives_real_commit_cold_restore_and_replay() {
    let mut fixture = Fixture::new();
    fixture.grant("historical-trust", None);
    let trigger = fixture.message("Original history");
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Historical native result", 500)
        .unwrap();
    let envelope = prepared.envelope().clone();
    let prior_epoch = fixture.core.engine.ready_epoch(SPACE).unwrap();
    let commit = fixture
        .owner
        .device()
        .unwrap()
        .commit(SPACE, &[], &Default::default())
        .unwrap();
    let commit_envelope = Envelope::sealed(
        fixture.owner.net.author.as_ref().unwrap(),
        SPACE,
        "actual-epoch-advance",
        600,
        fixture.owner.logs[SPACE].head().as_ref(),
        Sealed::new(SealedKind::Commit, ::mls::SUITE_ID, commit.commit),
    );
    let commit_frame = ordered(&fixture.owner, commit_envelope);
    assert_eq!(fixture.owner.ingest(commit_frame.clone()), Ingest::Applied);
    fixture.core.ingest(vec![commit_frame]).unwrap();
    assert_eq!(
        fixture.core.engine.ready_epoch(SPACE),
        Some(prior_epoch + 1)
    );
    assert!(fixture.core.engine.outbox_envelopes().is_empty());
    let pending = fixture
        .core
        .engine
        .store
        .outbox_get(envelope.client_id())
        .unwrap()
        .unwrap();
    assert_eq!(
        fixture.core.engine.sealed_copy(&pending).unwrap(),
        (prior_epoch, envelope.clone())
    );
    let mut cold =
        DeviceCore::restore(fixture.core.image().unwrap(), fixture.credential()).unwrap();
    let client_id = envelope.client_id().to_string();
    let reference_key = super::reference::reference_key(&client_id);
    let reference = cold.engine.store.meta(&reference_key).unwrap().unwrap();
    let accepted = ordered(&fixture.owner, envelope);
    assert_eq!(fixture.owner.ingest(accepted.clone()), Ingest::Applied);
    assert!(
        matches!(&fixture.owner.logs[SPACE].events().last().unwrap().body, EventBody::MessagePosted { text, .. } if text == "Historical native result")
    );
    let receipt = cold
        .receive_reply(&original, &client_id, vec![accepted.clone()])
        .unwrap();
    assert_eq!(receipt.original(), &original);
    assert_eq!(receipt.message(), prepared.message());
    assert_eq!(receipt.event_digest(), prepared.event_digest());
    assert_eq!(receipt.reference_digest(), prepared.reference_digest());
    assert_eq!(receipt.publish_digest(), prepared.publish_digest());
    assert_eq!(receipt.accepted().hash, accepted.hash);
    assert_eq!(cold.engine.store.outbox_len().unwrap(), 0);
    assert_eq!(
        cold.engine.store.meta(&reference_key).unwrap().as_deref(),
        Some(reference.as_str())
    );
    let event = cold.engine.logs[SPACE].events().last().unwrap();
    assert!(crate::mls_opening::own_echo(
        &cold.engine.store,
        event,
        cold.engine.net.account.as_ref().unwrap()
    ));
    assert_eq!(event_log::content_hash_of(event), prepared.event_digest());
    let mut replay = DeviceCore::restore(
        NativeImage::decode_bounded(receipt.image().as_bytes().to_vec()).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    let repeated = replay
        .receive_reply(&original, &client_id, vec![accepted.clone()])
        .unwrap();
    assert_eq!(repeated.accepted(), receipt.accepted());
    assert_eq!(repeated.reference_digest(), prepared.reference_digest());
    assert_eq!(repeated.publish_digest(), prepared.publish_digest());
    assert_eq!(replay.engine.store.outbox_len().unwrap(), 0);
    assert_eq!(
        replay.retained_head(SPACE).unwrap().unwrap().hash,
        accepted.hash
    );
    println!("JOURNEY real native commit advances epoch without resealing the pending output; historical exact signed copy confirms after cold restore and unchanged replay creates no new outbox");
}

#[test]
fn native_receipt_survives_revocation_expiry_and_cold_replay_without_new_effects() {
    let mut fixture = Fixture::new();
    fixture.grant("receipt-original-trust", Some(1000));
    let trigger = fixture.message("Original receipt source");
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Original accepted native result", 500)
        .unwrap();
    let client_id = prepared.envelope().client_id().to_string();
    let key = super::reference::reference_key(&client_id);
    let reference = fixture.core.engine.store.meta(&key).unwrap().unwrap();
    let accepted = ordered(&fixture.owner, prepared.envelope().clone());
    assert_eq!(fixture.owner.ingest(accepted.clone()), Ingest::Applied);
    let first = fixture
        .core
        .receive_reply(&original, &client_id, vec![accepted.clone()])
        .unwrap();
    assert_eq!(first.accepted().hash, accepted.hash);
    assert_eq!(fixture.core.engine.store.outbox_len().unwrap(), 0);
    assert!(fixture
        .core
        .engine
        .store
        .meta(&format!("mls.sealed:{client_id}"))
        .unwrap()
        .is_none());
    fixture.encrypted(
        EventBody::GrantRevoked {
            grant: "receipt-original-trust".into(),
        },
        1100,
    );
    assert!(matches!(
        fixture.core.refresh(&original, 1200),
        Err(NativeError::Permission)
    ));
    let before = fixture.core.image().unwrap();
    let mut cold = DeviceCore::restore(
        NativeImage::decode_bounded(before.as_bytes().to_vec()).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    let receipt = cold
        .receive_reply(&original, &client_id, vec![accepted.clone()])
        .unwrap();
    assert_eq!(receipt.original(), &original);
    assert_eq!(receipt.message(), prepared.message());
    assert_eq!(receipt.event_digest(), prepared.event_digest());
    assert_eq!(receipt.accepted(), first.accepted());
    assert_eq!(receipt.image().as_bytes(), before.as_bytes());
    assert_eq!(
        cold.engine.store.meta(&key).unwrap().as_deref(),
        Some(reference.as_str())
    );
    assert_eq!(cold.engine.store.outbox_len().unwrap(), 0);
    assert!(cold.engine.logs[SPACE]
        .events()
        .iter()
        .all(|e| !matches!(e.body, EventBody::UsageRecorded { .. })));
    let mut changed = original.clone();
    changed.grant_digest = "00".repeat(32);
    assert!(matches!(
        cold.receive_reply(&changed, &client_id, vec![]),
        Err(NativeError::History)
    ));
    assert!(cold.image().is_err());
    println!("JOURNEY exact retained receipt survives accepted native grant revoke/expiry and cold replay without provider/image changes, new outbox or usage; a different original refuses");
}

#[test]
fn receipt_refuses_different_duplicate_opening_and_missing_original_reference() {
    for case in [
        "duplicate-ciphertext",
        "pruned-duplicate",
        "changed-opening",
        "missing-reference",
        "changed-message",
        "changed-digest",
        "future-reference",
    ] {
        let mut fixture = Fixture::new();
        fixture.grant("duplicate-original-trust", None);
        let trigger = fixture.message("Original duplicate source");
        let original = fixture
            .core
            .reply(SPACE, &trigger, 400)
            .unwrap()
            .intent()
            .clone();
        let prepared = fixture
            .core
            .prepare_reply(&original, "Exact native original output", 500)
            .unwrap();
        let client_id = prepared.envelope().client_id().to_string();
        let key = super::reference::reference_key(&client_id);
        let different = colliding_native_reply(&fixture, &prepared);
        let accepted = ordered(&fixture.owner, prepared.envelope().clone());
        assert_eq!(fixture.owner.ingest(accepted.clone()), Ingest::Applied);
        fixture
            .core
            .receive_reply(&original, &client_id, vec![accepted.clone()])
            .unwrap();
        let committed = fixture.core.image().unwrap();
        let mut replay = accepted.clone();
        match case {
            "duplicate-ciphertext" => {
                replay.env = different;
            }
            "pruned-duplicate" => {
                replay.env = replay.env.pruned().unwrap();
            }
            "changed-opening" => {
                let opening_key = format!("runtime.mls-opening.v1/{}", accepted.hash);
                let opening = fixture
                    .core
                    .engine
                    .store
                    .meta(&opening_key)
                    .unwrap()
                    .unwrap();
                fixture
                    .core
                    .engine
                    .store
                    .set_meta(&opening_key, &opening.replace("OwnRetainedEcho", "Opaque"))
                    .unwrap();
            }
            "missing-reference" => fixture.core.engine.store.meta_delete(&key).unwrap(),
            "changed-message" | "changed-digest" | "future-reference" => {
                let json = fixture.core.engine.store.meta(&key).unwrap().unwrap();
                let changed = match case {
                    "changed-message" => json.replace(prepared.message(), "different-message"),
                    "changed-digest" => json.replace(prepared.event_digest(), &"00".repeat(32)),
                    "future-reference" => json.replace("\"version\":1", "\"version\":2"),
                    _ => unreachable!(),
                };
                assert_ne!(json, changed);
                fixture.core.engine.store.set_meta(&key, &changed).unwrap();
            }
            _ => unreachable!(),
        }
        fixture.core =
            DeviceCore::restore(fixture.core.image().unwrap(), fixture.credential()).unwrap();
        assert!(
            matches!(
                fixture
                    .core
                    .receive_reply(&original, &client_id, vec![replay]),
                Err(NativeError::History)
            ),
            "{case}"
        );
        assert!(fixture.core.image().is_err());
        let mut good = DeviceCore::restore(committed, fixture.credential()).unwrap();
        assert_eq!(
            good.receive_reply(&original, &client_id, vec![accepted])
                .unwrap()
                .event_digest(),
            prepared.event_digest()
        );
    }
    println!("JOURNEY receipt replay refuses another signed ciphertext or pruned duplicate, opaque opening, missing/future reference and changed original message/digest after actual cold store roundtrip");
}

#[test]
fn actual_maximum_text_reference_is_bounded_and_oversized_fields_refuse() {
    let extreme_space = "\0".repeat(256);
    let mut fixture = Fixture::new_in(&extreme_space);
    fixture.grant(&"\0".repeat(256), None);
    let trigger = fixture.message("Bounded original source");
    let original = fixture
        .core
        .reply(&extreme_space, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let text = "🧬".repeat(super::publication::MAX_REPLY_BYTES / 4);
    let prepared = fixture.core.prepare_reply(&original, &text, 500).unwrap();
    let client_id = prepared.envelope().client_id().to_string();
    let key = super::reference::reference_key(&client_id);
    let json = fixture.core.engine.store.meta(&key).unwrap().unwrap();
    let wire = ClientFrame::Publish {
        env: prepared.envelope().clone(),
    }
    .encode();
    assert!(json.len() <= super::reference::MAX_REFERENCE_BYTES);
    assert!(wire.len() <= super::publication::MAX_PUBLISH_BYTES);
    let mut cold = DeviceCore::restore(
        NativeImage::decode_bounded(prepared.image().as_bytes().to_vec()).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    let accepted = ordered(&fixture.owner, prepared.envelope().clone());
    assert_eq!(fixture.owner.ingest(accepted.clone()), Ingest::Applied);
    let receipt = cold
        .receive_reply(&original, &client_id, vec![accepted])
        .unwrap();
    assert_eq!(receipt.event_digest(), prepared.event_digest());
    assert!(
        matches!(&cold.engine.logs[&extreme_space].events().last().unwrap().body, EventBody::MessagePosted { text: actual, .. } if actual == &text)
    );
    cold.engine
        .store
        .set_meta(&key, &" ".repeat(super::reference::MAX_REFERENCE_BYTES + 1))
        .unwrap();
    assert!(matches!(
        cold.receive_reply(&original, &client_id, vec![]),
        Err(NativeError::Bounds)
    ));
    assert!(cold.image().is_err());
    let mut oversized = Fixture::new();
    oversized.grant(&"g".repeat(257), None);
    let trigger = oversized.message("Oversized original field");
    let original = oversized
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let before = oversized.core.image().unwrap();
    assert!(matches!(
        oversized
            .core
            .prepare_reply(&original, "Unpublished output", 500),
        Err(NativeError::Bounds)
    ));
    assert!(oversized.core.image().is_err());
    assert_eq!(
        DeviceCore::restore(before, oversized.credential())
            .unwrap()
            .engine
            .store
            .outbox_len()
            .unwrap(),
        0
    );
    println!("JOURNEY actual 32768-byte UTF-8 reply with 256-byte JSON-escaped Space/grant fields produces reference_json={} publish={} image={} bytes; 135169-byte metadata and 257-byte field refuse without adopting or truncating", json.len(), wire.len(), prepared.image().as_bytes().len());
}

#[test]
fn receipt_delta_preserves_other_pending_and_refuses_real_new_join_or_row_changes() {
    let mut fixture = Fixture::new();
    fixture.grant("delta-original-trust", None);
    let trigger = fixture.message("Original delta source");
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Original delta output", 500)
        .unwrap();
    let client_id = prepared.envelope().client_id().to_string();
    let before = fixture.core.snapshot_outbox().unwrap();
    let author = fixture.core.engine.net.author.clone().unwrap();
    let joining = fixture
        .core
        .engine
        .append_synced(
            SPACE,
            &author,
            600,
            EventBody::DeviceJoining {
                device: fixture.agent_device.id(),
            },
        )
        .unwrap();
    event_log::verify_author(&joining).unwrap();
    assert!(matches!(
        fixture.core.verify_receipt_delta(&before, &client_id),
        Err(NativeError::History)
    ));
    let surviving = fixture
        .core
        .engine
        .store
        .outbox_get(&joining.client_id)
        .unwrap()
        .unwrap();
    let accepted = ordered(&fixture.owner, prepared.envelope().clone());
    assert_eq!(fixture.owner.ingest(accepted.clone()), Ingest::Applied);
    fixture
        .core
        .receive_reply(&original, &client_id, vec![accepted])
        .unwrap();
    assert_eq!(
        fixture
            .core
            .engine
            .store
            .outbox_get(&joining.client_id)
            .unwrap(),
        Some(surviving.clone())
    );
    let before = fixture.core.snapshot_outbox().unwrap();
    fixture
        .core
        .engine
        .store
        .outbox_note(&joining.client_id, "changed retry state", false)
        .unwrap();
    assert!(matches!(
        fixture.core.verify_receipt_delta(&before, &client_id),
        Err(NativeError::History)
    ));
    fixture
        .core
        .engine
        .store
        .outbox_remove(&joining.client_id)
        .unwrap();
    fixture.core.engine.net.pending.remove(&joining.client_id);
    assert!(matches!(
        fixture.core.verify_receipt_delta(&before, &client_id),
        Err(NativeError::History)
    ));
    let before = fixture.core.snapshot_outbox().unwrap();
    fixture
        .core
        .engine
        .store
        .set_meta("mls.sealed:orphan-metadata", "1:00:00")
        .unwrap();
    assert!(matches!(
        fixture.core.verify_receipt_delta(&before, &client_id),
        Err(NativeError::History)
    ));
    println!("JOURNEY explicit delta-check refuses a separately generated genuinely signed joining event, retry-row change, unrelated removal and new orphan sealing metadata; actual target receipt preserves an existing other pending exactly");
}

#[test]
fn later_colliding_echo_invalidates_an_entire_partially_ingested_batch() {
    let mut fixture = Fixture::new();
    fixture.grant("batch-original-trust", None);
    let trigger = fixture.message("Original batch source");
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Exact batch output", 500)
        .unwrap();
    let committed = prepared.image().as_bytes().to_vec();
    let different = colliding_native_reply(&fixture, &prepared);
    let author = fixture.owner.net.author.clone().unwrap();
    let (epoch, digest) = fixture.owner.device().unwrap().checkpoint(SPACE).unwrap();
    let preceding = author.sign_event(
        SPACE,
        "preceding-native-checkpoint",
        600,
        fixture.owner.logs[SPACE].head(),
        EventBody::Checkpoint {
            upto: fixture.owner.logs[SPACE].head().unwrap(),
            epoch,
            digest,
        },
    );
    let first = ordered(&fixture.owner, Envelope::plain(&preceding));
    assert_eq!(fixture.owner.ingest(first.clone()), Ingest::Applied);
    let second = ordered(&fixture.owner, different);
    assert!(matches!(
        fixture.core.ingest(vec![first.clone(), second]),
        Err(NativeError::History)
    ));
    assert!(fixture.core.image().is_err());
    assert_eq!(
        fixture.core.engine.logs[SPACE].head().unwrap().hash,
        first.hash
    );
    let cold = DeviceCore::restore(
        NativeImage::decode_bounded(committed).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    assert_eq!(
        cold.engine.outbox_envelopes(),
        vec![prepared.envelope().clone()]
    );
    assert_ne!(cold.engine.logs[SPACE].head().unwrap().hash, first.hash);
    println!("JOURNEY a colliding own echo after an applied entry poisons the whole scratch batch; cold prior image keeps its earlier frontier and exact pending output");
}

#[test]
fn preparation_refuses_expired_replaced_revoked_and_reader_authority() {
    for case in ["expired", "replaced", "revoked", "reader"] {
        let mut fixture = Fixture::new();
        fixture.grant("publication-original-trust", Some(1000));
        let trigger = fixture.message("Original retained request");
        let original = fixture
            .core
            .reply(SPACE, &trigger, 400)
            .unwrap()
            .intent()
            .clone();
        match case {
            "expired" => {}
            "replaced" => {
                fixture.grant("publication-replacement-trust", None);
            }
            "revoked" => {
                fixture.encrypted(
                    EventBody::GrantRevoked {
                        grant: "publication-original-trust".into(),
                    },
                    450,
                );
            }
            "reader" => {
                fixture.clear(EventBody::MemberAdded {
                    identity: fixture.agent_profile.id.clone(),
                    role: Role::Reader,
                });
            }
            _ => unreachable!(),
        }
        let before = fixture.core.image().unwrap();
        let at_ms = if case == "expired" { 1000 } else { 500 };
        assert!(matches!(
            fixture
                .core
                .prepare_reply(&original, "Retained output", at_ms),
            Err(NativeError::Permission)
        ));
        assert_eq!(fixture.core.engine.store.outbox_len().unwrap(), 0);
        assert!(matches!(fixture.core.image(), Err(NativeError::History)));
        let restored = DeviceCore::restore(before, fixture.credential()).unwrap();
        assert_eq!(restored.engine.store.outbox_len().unwrap(), 0);
    }
    println!("JOURNEY native reply preparation refuses actual expiry, replacement, revocation and reader role; failed scratch images cannot activate");
}

fn receipt_source_message(fixture: &mut Fixture, index: usize) -> Sequenced {
    let author = fixture.owner.net.author.clone().unwrap();
    fixture
        .owner
        .append_synced(
            &fixture.space,
            &author,
            600 + index as i64,
            EventBody::MessagePosted {
                message: new_id("prefix"),
                text: format!("Historical position {index}"),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )
        .unwrap();
    fixture.owner.mls_seal_outbox().unwrap();
    let frame = ordered(&fixture.owner, fixture.owner.outbox_envelopes().remove(0));
    assert_eq!(fixture.owner.ingest(frame.clone()), Ingest::Confirmed);
    frame
}

#[test]
fn concurrent_receipt_prefix_restores_only_the_matching_certified_device() {
    let mut fixture = Fixture::new();
    fixture.grant("concurrent-prefix-trust", None);
    let trigger = fixture.message("Concurrent receipt input");
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Retained concurrent output", 500)
        .unwrap();
    let cid = prepared.envelope().client_id().to_string();
    let pending = fixture.core.snapshot_outbox().unwrap();
    let next = receipt_source_message(&mut fixture, 0);
    assert_eq!(next.env.seen(), prepared.envelope().seen());
    assert_ne!(next.env.author(), prepared.envelope().author());
    next.env.verify().unwrap();
    let ReplyReceipt::Prefix(prefix) = fixture
        .core
        .receive_reply_progress(&original, &cid, vec![next])
        .unwrap()
    else {
        panic!("the concurrent owner message is not the reply receipt");
    };
    assert!(fixture.core.snapshot_outbox().unwrap() == pending);
    let other = Fixture::new_in("other-certified-device");
    assert!(matches!(
        DeviceCore::restore(
            NativeImage::decode_bounded(prefix.image().as_bytes().to_vec()).unwrap(),
            other.credential(),
        ),
        Err(NativeError::Credential)
    ));
    let mut cold = DeviceCore::restore(
        NativeImage::decode_bounded(prefix.image().as_bytes().to_vec()).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    assert!(cold.snapshot_outbox().unwrap() == pending);
    let accepted = ordered(&fixture.owner, prepared.envelope().clone());
    assert_eq!(fixture.owner.ingest(accepted.clone()), Ingest::Applied);
    let ReplyReceipt::Accepted(receipt) = cold
        .receive_reply_progress(&original, &cid, vec![accepted.clone()])
        .unwrap()
    else {
        panic!("the original pending reply must produce acceptance");
    };
    assert_eq!(receipt.accepted().hash, accepted.hash);
    assert_eq!(receipt.reference_digest(), prepared.reference_digest());
    assert_eq!(receipt.publish_digest(), prepared.publish_digest());
    assert_eq!(cold.engine.store.outbox_len().unwrap(), 0);
    println!("JOURNEY concurrent same-prefix owner message preserves the original pending reply through cold restore; a different real certified agent is refused and only the original reply produces acceptance");
}

#[test]
fn conflicting_sealed_seen_blocks_cold_restore_with_valid_pending_credentials() {
    let mut fixture = Fixture::new();
    fixture.grant("conflicting-prefix-trust", None);
    let trigger = fixture.message("Original retained input");
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Original retained output", 500)
        .unwrap();
    let before = fixture.core.retained_head(SPACE).unwrap().unwrap();
    let pending = fixture.core.snapshot_outbox().unwrap();
    let source = receipt_source_message(&mut fixture, 0);
    assert_eq!(source.env.seen(), prepared.envelope().seen());
    let diverged = Seen {
        seq: before.seq,
        hash: "00".repeat(32),
    };
    assert_ne!(diverged.hash, before.hash);
    let (kind, bytes) = source.env.sealed_data().unwrap();
    let author = fixture.owner.net.author.as_ref().unwrap();
    let conflicting = Envelope::sealed(
        author,
        SPACE,
        source.env.client_id(),
        source.env.at_ms(),
        Some(&diverged),
        Sealed::new(kind, ::mls::SUITE_ID, bytes.to_vec()),
    );
    conflicting.verify().unwrap();
    let frame = ordered(&fixture.core.engine, conflicting);
    let got = fixture.core.engine.ingest(frame);
    assert!(matches!(got, Ingest::Invalid(_)), "{got:?}");
    assert_eq!(fixture.core.retained_head(SPACE).unwrap(), Some(before));
    assert!(fixture.core.snapshot_outbox().unwrap() == pending);
    assert!(fixture.core.engine.fork_recorded(SPACE));
    assert!(fixture.core.engine.outbox_envelopes().is_empty());
    assert!(matches!(
        DeviceCore::restore(fixture.core.image().unwrap(), fixture.credential()),
        Err(NativeError::Credential)
    ));
    println!("JOURNEY a member's real signature over a conflicting held-prefix hash preserves the pending reply but parks publication and rejects cold restore with the matching certified device");
}

#[test]
fn receipt_prefix_keeps_exact_pending_bytes_and_resumes_beyond_64_real_positions() {
    use sha2::{Digest, Sha256};
    let mut fixture = Fixture::new();
    fixture.grant("prefix-trust", Some(1000));
    let trigger = fixture.message("Original prefix input");
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Paid original output", 500)
        .unwrap();
    let cid = prepared.envelope().client_id().to_string();
    let key = super::reference::reference_key(&cid);
    let json = fixture.core.engine.store.meta(&key).unwrap().unwrap();
    assert_eq!(
        prepared.reference_digest(),
        hex::encode(Sha256::digest(json.as_bytes()))
    );
    assert_eq!(
        prepared.publish_digest(),
        hex::encode(Sha256::digest(
            ClientFrame::Publish {
                env: prepared.envelope().clone()
            }
            .encode()
        ))
    );
    fixture
        .core
        .engine
        .store
        .set_meta("mls.sealed:preserved-orphan", "original opaque metadata")
        .unwrap();
    let author = fixture.core.engine.net.author.clone().unwrap();
    fixture
        .core
        .engine
        .append_synced(
            SPACE,
            &author,
            550,
            EventBody::DeviceJoining {
                device: fixture.agent_device.id(),
            },
        )
        .unwrap();
    let pending = fixture.core.snapshot_outbox().unwrap();
    let before = fixture.core.retained_head(SPACE).unwrap().unwrap();
    let mut batch = Vec::new();
    for index in 0..65 {
        batch.push(receipt_source_message(&mut fixture, index));
    }
    let accepted = ordered(&fixture.owner, prepared.envelope().clone());
    assert_eq!(fixture.owner.ingest(accepted.clone()), Ingest::Applied);
    let first = batch.drain(..64).collect::<Vec<_>>();
    let frontier = first.last().unwrap().clone();
    let ReplyReceipt::Prefix(prefix) = fixture
        .core
        .receive_reply_progress(&original, &cid, first)
        .unwrap()
    else {
        panic!("target was beyond the real first batch");
    };
    assert_eq!(prefix.before(), &before);
    assert_eq!(
        prefix.frontier(),
        &Seen {
            seq: frontier.seq,
            hash: frontier.hash
        }
    );
    assert_eq!(prefix.reference_digest(), prepared.reference_digest());
    assert_eq!(prefix.publish_digest(), prepared.publish_digest());
    assert!(fixture.core.snapshot_outbox().unwrap() == pending);
    assert_eq!(fixture.core.engine.store.meta(&key).unwrap().unwrap(), json);
    let mut cold = DeviceCore::restore(
        NativeImage::decode_bounded(prefix.image().as_bytes().to_vec()).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    batch.push(accepted.clone());
    let ReplyReceipt::Accepted(receipt) =
        cold.receive_reply_progress(&original, &cid, batch).unwrap()
    else {
        panic!("actual native acceptance must produce AcceptedReply");
    };
    assert_eq!(
        receipt.accepted(),
        &Seen {
            seq: accepted.seq,
            hash: accepted.hash
        }
    );
    assert_eq!(receipt.reference_digest(), prepared.reference_digest());
    assert_eq!(receipt.publish_digest(), prepared.publish_digest());
    assert_eq!(cold.engine.store.outbox_len().unwrap(), 1);
    assert_eq!(
        cold.engine
            .store
            .meta("mls.sealed:preserved-orphan")
            .unwrap()
            .as_deref(),
        Some("original opaque metadata")
    );
    println!("JOURNEY 65 real certified encrypted source positions resume across cold 64-position prefix; pending retry rows, other command, orphan sealing and original reference remain exact; only the actual target produces acceptance");
}

#[test]
fn receipt_prefix_validates_original_pending_before_reading_unrelated_history() {
    for case in ["pending-inner", "missing-sealed-copy", "pending-map"] {
        let mut fixture = Fixture::new();
        fixture.grant("prefix-pending-trust", None);
        let trigger = fixture.message("Original pending input");
        let original = fixture
            .core
            .reply(SPACE, &trigger, 400)
            .unwrap()
            .intent()
            .clone();
        let prepared = fixture
            .core
            .prepare_reply(&original, "Original pending output", 500)
            .unwrap();
        let cid = prepared.envelope().client_id().to_string();
        let next = receipt_source_message(&mut fixture, 0);
        let initial = fixture.core.retained_head(SPACE).unwrap().unwrap();
        match case {
            "pending-inner" => {
                let author = fixture.core.engine.net.author.clone().unwrap();
                let event = author.sign_event(
                    SPACE,
                    &cid,
                    500,
                    prepared.envelope().seen(),
                    EventBody::MessagePosted {
                        message: prepared.message().into(),
                        text: "Different certified output".into(),
                        attaches: None,
                        reply: None,
                        media: None,
                        forwarded: false,
                    },
                );
                fixture.core.engine.store.outbox_remove(&cid).unwrap();
                fixture.core.engine.store.outbox_put(&event).unwrap();
            }
            "missing-sealed-copy" => fixture
                .core
                .engine
                .store
                .conn()
                .execute(
                    "DELETE FROM meta WHERE key=?1",
                    [format!("mls.sealed:{cid}")],
                )
                .map(|_| ())
                .unwrap(),
            "pending-map" => {
                fixture.core.engine.net.pending.remove(&cid);
            }
            _ => unreachable!(),
        }
        assert!(
            matches!(
                fixture
                    .core
                    .receive_reply_progress(&original, &cid, vec![next]),
                Err(NativeError::History)
            ),
            "{case}"
        );
        assert!(fixture.core.image().is_err());
        assert_eq!(fixture.core.engine.logs[SPACE].head().unwrap(), initial);
    }
    println!("JOURNEY tampered certified inner, missing original sealed copy and pending-map mismatch refuse before applying unrelated prefix history");
}

#[test]
fn receipt_prefix_refuses_skipped_duplicate_oversized_and_partially_invalid_batches() {
    for case in [
        "skip",
        "duplicate",
        "oversized",
        "late-invalid",
        "other-pending-acceptance",
    ] {
        let mut fixture = Fixture::new();
        fixture.grant("prefix-batch-trust", None);
        let trigger = fixture.message("Original batch input");
        let original = fixture
            .core
            .reply(SPACE, &trigger, 400)
            .unwrap()
            .intent()
            .clone();
        let prepared = fixture
            .core
            .prepare_reply(&original, "Original batch output", 500)
            .unwrap();
        let cid = prepared.envelope().client_id().to_string();
        let prior = fixture.core.image().unwrap().as_bytes().to_vec();
        let first = receipt_source_message(&mut fixture, 0);
        let mut batch = vec![first.clone()];
        match case {
            "skip" => {
                batch = vec![receipt_source_message(&mut fixture, 1)];
            }
            "duplicate" => {
                fixture.core.ingest(vec![first.clone()]).unwrap();
            }
            "oversized" => {
                for index in 1..65 {
                    batch.push(receipt_source_message(&mut fixture, index));
                }
            }
            "late-invalid" => {
                let mut last = receipt_source_message(&mut fixture, 1);
                last.hash = "00".repeat(32);
                batch.push(last);
            }
            "other-pending-acceptance" => {
                let author = fixture.core.engine.net.author.clone().unwrap();
                let event = fixture
                    .core
                    .engine
                    .append_synced(
                        SPACE,
                        &author,
                        550,
                        EventBody::DeviceJoining {
                            device: fixture.agent_device.id(),
                        },
                    )
                    .unwrap();
                let last = ordered(&fixture.owner, Envelope::plain(&event));
                assert_eq!(fixture.owner.ingest(last.clone()), Ingest::Applied);
                batch.push(last);
            }
            _ => unreachable!(),
        }
        assert!(
            fixture
                .core
                .receive_reply_progress(&original, &cid, batch)
                .is_err(),
            "{case}"
        );
        assert!(fixture.core.image().is_err());
        let cold = DeviceCore::restore(
            NativeImage::decode_bounded(prior).unwrap(),
            fixture.credential(),
        )
        .unwrap();
        assert_eq!(
            cold.engine.outbox_envelopes(),
            vec![prepared.envelope().clone()]
        );
    }
    println!("JOURNEY source gap, replay-only prefix, 65-position input, invalid later chain and another pending acceptance refuse whole scratch; original committed publication stays recoverable");
}

#[test]
fn preparation_bounds_text_and_never_reseals_an_existing_pending_reply() {
    let mut fixture = Fixture::new();
    fixture.grant("publication-bounded-trust", None);
    let trigger = fixture.message("Original bounded request");
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let before = fixture.core.image().unwrap().as_bytes().to_vec();
    for text in [
        String::new(),
        " \n\t".into(),
        "x".repeat(MAX_REPLY_BYTES + 1),
    ] {
        let mut scratch = DeviceCore::restore(
            NativeImage::decode_bounded(before.clone()).unwrap(),
            fixture.credential(),
        )
        .unwrap();
        assert!(matches!(
            scratch.prepare_reply(&original, &text, 500),
            Err(NativeError::Bounds)
        ));
        assert_eq!(scratch.engine.store.outbox_len().unwrap(), 0);
        assert!(matches!(scratch.image(), Err(NativeError::History)));
    }
    let text = "é".repeat(MAX_REPLY_BYTES / 2);
    let prepared = fixture.core.prepare_reply(&original, &text, 500).unwrap();
    let wire = ClientFrame::Publish {
        env: prepared.envelope().clone(),
    }
    .encode();
    assert!(wire.len() <= MAX_PUBLISH_BYTES);
    assert!(prepared.envelope().content().len() <= MAX_EVENT_BYTES);
    let image = prepared.image().as_bytes().to_vec();
    let mut cold = DeviceCore::restore(
        NativeImage::decode_bounded(image.clone()).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    assert!(matches!(
        cold.prepare_reply(&original, "Second output", 501),
        Err(NativeError::NotReady)
    ));
    assert!(matches!(cold.image(), Err(NativeError::History)));
    let restored = DeviceCore::restore(
        NativeImage::decode_bounded(image).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    assert_eq!(restored.engine.store.outbox_len().unwrap(), 1);
    assert_eq!(
        ClientFrame::Publish {
            env: restored.engine.outbox_envelopes().remove(0)
        }
        .encode(),
        wire
    );
    println!("JOURNEY real 32 KiB UTF-8 reply fits native wire/image bounds; empty, oversized and repeated pending preparation refuse without resealing");
}

#[test]
fn failed_reply_sealing_discards_scratch_and_preserves_committed_image() {
    let mut fixture = Fixture::new();
    fixture.grant("publication-sealing-trust", None);
    let trigger = fixture.message("Original sealing request");
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let before = fixture.core.image().unwrap();
    fixture.core.engine.store.conn().execute_batch(
        "CREATE TRIGGER fail_reply_seal BEFORE INSERT ON meta WHEN NEW.key LIKE 'mls.sealed:%' BEGIN SELECT RAISE(ABORT,'injected sealed outbox failure'); END;"
    ).unwrap();
    assert!(matches!(
        fixture
            .core
            .prepare_reply(&original, "Output requiring durable sealing", 500),
        Err(NativeError::Storage)
    ));
    assert!(matches!(fixture.core.image(), Err(NativeError::History)));
    let mut restored = DeviceCore::restore(before, fixture.credential()).unwrap();
    assert_eq!(restored.engine.store.outbox_len().unwrap(), 0);
    let prepared = restored
        .prepare_reply(&original, "Output requiring durable sealing", 500)
        .unwrap();
    prepared.envelope().verify().unwrap();
    println!("JOURNEY real sealed outbox write failure poisons scratch after native append; prior committed image remains usable with no loose native effect");
}

#[test]
fn discovery_refuses_cursor_outside_verified_history() {
    let mut fixture = Fixture::new();
    fixture.grant("discovery-owner-trust", Some(1000));
    fixture.message("Actual verified owner request");
    fixture
        .core
        .engine
        .store
        .set_meta(
            &format!("runtime.reply-scan/1/{SPACE}"),
            &serde_json::to_string(&Seen {
                seq: u64::MAX,
                hash: "fabricated".into(),
            })
            .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        fixture.core.discover_replies(SPACE, 400),
        Err(NativeError::History)
    ));
}

#[test]
fn actual_owner_opening_survives_complete_image_and_pins_original_input() {
    let mut fixture = Fixture::new();
    fixture.grant("explicit-owner-trust", Some(1000));
    let trigger = fixture.message("Private retained owner request");
    let peer_event = fixture.core.engine.logs[SPACE].events().last().unwrap();
    assert_eq!(
        crate::mls_opening::opened_leaf(&fixture.core.engine.store, peer_event)
            .unwrap()
            .identity,
        fixture.owner_profile.id
    );
    let own_event = fixture.owner.logs[SPACE].events().last().unwrap();
    assert!(crate::mls_opening::opened_leaf(&fixture.owner.store, own_event).is_none());
    assert!(crate::mls_opening::own_echo(
        &fixture.owner.store,
        own_event,
        fixture.owner.account().unwrap()
    ));
    let live = fixture.core.reply(SPACE, &trigger, 400).unwrap();
    assert_eq!(live.text(), "Private retained owner request\n");
    let original = live.intent().clone();
    assert_eq!(live.expires_at_ms(), Some(1000));
    let image = fixture.core.image().unwrap();
    assert!(image.as_bytes().len() < MAX_MEMORY_IMAGE_BYTES);
    let cold = DeviceCore::restore(image, fixture.credential()).unwrap();
    let restored = cold.refresh(&original, 400).unwrap();
    assert_eq!(restored.frontier(), live.frontier());
    assert_eq!(restored.text(), live.text());
    fixture.message("A later request cannot rewrite a held attempt");
    let refreshed = fixture.core.refresh(&original, 500).unwrap();
    assert_ne!(refreshed.frontier().hash(), live.frontier().hash());
    assert_eq!(refreshed.intent(), &original);
    assert_eq!(refreshed.text(), live.text());
    assert!(matches!(
        fixture.core.refresh(&original, 1000),
        Err(NativeError::Permission)
    ));
    println!("JOURNEY certified real Welcome/Commit/Application; live/cold equivalent; original input pinned; expiry denied");
}

#[test]
fn failed_opening_record_rolls_back_log_and_ratchet_and_discards_workspace() {
    let mut fixture = Fixture::new();
    fixture.grant("retained-trust", None);
    let trigger = fixture.message("Known original request");
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let before = fixture.core.image().unwrap();
    let count = fixture.core.engine.store.event_count().unwrap();
    fixture.core.engine.store.conn().execute_batch(
        "CREATE TRIGGER fail_opening BEFORE INSERT ON meta WHEN NEW.key LIKE 'runtime.mls-opening.v1/%' BEGIN SELECT RAISE(ABORT,'injected opening write failure'); END;"
    ).unwrap();
    let author = fixture.owner.net.author.clone().unwrap();
    fixture
        .owner
        .append_synced(
            SPACE,
            &author,
            450,
            EventBody::MessagePosted {
                message: new_id("message"),
                text: "Open atomically".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )
        .unwrap();
    fixture.owner.mls_seal_outbox().unwrap();
    let frame = ordered(&fixture.owner, fixture.owner.outbox_envelopes().remove(0));
    assert!(matches!(
        fixture.core.ingest(vec![frame.clone()]),
        Err(NativeError::History)
    ));
    assert_eq!(fixture.core.engine.store.event_count().unwrap(), count);
    assert!(fixture
        .core
        .engine
        .store
        .meta(&format!("runtime.mls-opening.v1/{}", frame.hash))
        .unwrap()
        .is_none());
    assert!(matches!(fixture.core.image(), Err(NativeError::History)));
    assert!(matches!(
        fixture.core.refresh(&original, 500),
        Err(NativeError::History)
    ));
    let mut restored = DeviceCore::restore(before, fixture.credential()).unwrap();
    restored.ingest(vec![frame.clone()]).unwrap();
    assert!(restored.reply(SPACE, &frame.hash, 500).is_ok());
    println!("JOURNEY real opening/log/ratchet rollback on receipt failure; workspace discarded; same message opens from retained image");
}

#[test]
fn whole_image_keeps_non_openmls_provider_tables_and_real_pending_outbox() {
    let mut fixture = Fixture::new();
    fixture.core.engine.store.conn().execute_batch(
        "INSERT INTO vc_emulation_bindings VALUES (1,x'01',x'0203'); INSERT INTO registered_vc_emulation_epochs VALUES (1,x'01',x'0405');"
    ).unwrap();
    let author = fixture.core.engine.net.author.clone().unwrap();
    let pending = fixture
        .core
        .engine
        .append_synced(
            SPACE,
            &author,
            400,
            EventBody::MessagePosted {
                message: new_id("message"),
                text: "Retain protocol queue".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )
        .unwrap();
    let cold = DeviceCore::restore(fixture.core.image().unwrap(), fixture.credential()).unwrap();
    for (table, column, expected) in [
        ("vc_emulation_bindings", "bindings", vec![2u8, 3]),
        (
            "registered_vc_emulation_epochs",
            "registration",
            vec![4u8, 5],
        ),
    ] {
        let bytes: Vec<u8> = cold
            .engine
            .store
            .conn()
            .query_row(
                &format!("SELECT {column} FROM {table} WHERE group_id=x'01'"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bytes, expected);
    }
    assert_eq!(
        cold.engine.store.outbox_get(&pending.client_id).unwrap(),
        Some(pending)
    );
    assert_eq!(
        cold.engine.device().unwrap().epoch(SPACE).unwrap(),
        fixture.core.engine.device().unwrap().epoch(SPACE).unwrap()
    );
    println!("JOURNEY complete image retains actual joined MLS group, pending signed outbox and non-openmls provider table bytes");
}

#[test]
fn own_echo_is_context_only_and_mismatching_mls_sender_cannot_refresh() {
    let mut fixture = Fixture::new();
    fixture.grant("explicit-trust", None);
    let trigger = fixture.message("Owner's original request");
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Agent's retained own reply", 450)
        .unwrap();
    let frame = ordered(&fixture.owner, prepared.envelope().clone());
    assert_eq!(fixture.owner.ingest(frame.clone()), Ingest::Applied);
    fixture.core.ingest(vec![frame.clone()]).unwrap();
    let own = frame.hash;
    assert!(matches!(
        fixture.core.reply(SPACE, &own, 500),
        Err(NativeError::Permission)
    ));
    assert_eq!(
        fixture.core.refresh(&original, 500).unwrap().text(),
        "Owner's original request\n"
    );
    let next = fixture.message("Owner's next request");
    assert!(fixture
        .core
        .reply(SPACE, &next, 500)
        .unwrap()
        .text()
        .contains("Agent's retained own reply"));

    let agent = fixture.core.engine.net.author.as_ref().unwrap();
    let seen = fixture.owner.logs[SPACE].head();
    let inner = agent.sign_event(
        SPACE,
        "wrong-mls-leaf",
        550,
        seen.clone(),
        EventBody::MessagePosted {
            message: new_id("message"),
            text: "Outer agent differs from actual owner MLS leaf".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    );
    let plaintext = event_log::content::InnerEvent {
        content: inner.content,
        sig: inner.sig,
    }
    .encode();
    let ciphertext = fixture
        .owner
        .device()
        .unwrap()
        .seal(SPACE, &plaintext)
        .unwrap();
    let envelope = Envelope::sealed(
        agent,
        SPACE,
        "wrong-mls-leaf",
        550,
        seen.as_ref(),
        Sealed::new(SealedKind::Application, ::mls::SUITE_ID, ciphertext),
    );
    envelope.verify().unwrap();
    let frame = ordered(&fixture.owner, envelope);
    fixture.core.ingest(vec![frame]).unwrap();
    assert!(matches!(
        fixture.core.refresh(&original, 600),
        Err(NativeError::Permission)
    ));
    println!("JOURNEY own retained echo is context, never owner trigger; valid signed outer with different actual MLS sender denies refresh");
}

#[test]
fn conflicting_actual_member_checkpoint_parks_live_and_cold_core() {
    let mut fixture = Fixture::new();
    fixture.grant("explicit-trust", None);
    let trigger = fixture.message("Original request");
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    fixture.clear(EventBody::Checkpoint {
        upto: fixture.owner.logs[SPACE].head().unwrap(),
        epoch: fixture.core.engine.device().unwrap().epoch(SPACE).unwrap(),
        digest: "00".repeat(32),
    });
    assert!(matches!(
        fixture.core.refresh(&original, 500),
        Err(NativeError::NotReady)
    ));
    assert!(matches!(
        DeviceCore::restore(fixture.core.image().unwrap(), fixture.credential()),
        Err(NativeError::Credential)
    ));
    println!("JOURNEY signed member checkpoint with conflicting group digest parks live core and refuses cold restore");
}

#[test]
fn diverged_member_checkpoint_parks_live_and_cold_core() {
    let mut fixture = Fixture::new();
    fixture.grant("explicit-trust", None);
    let trigger = fixture.message("Original request");
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let author = fixture.owner.net.author.clone().unwrap();
    let shared = fixture.owner.logs[SPACE].head().unwrap();
    assert_eq!(fixture.core.engine.logs[SPACE].head(), Some(shared.clone()));
    let epoch = fixture.core.engine.device().unwrap().epoch(SPACE).unwrap();
    let digest = fixture
        .core
        .engine
        .store
        .meta(&format!("mls.digest:{SPACE}:{epoch}"))
        .unwrap()
        .unwrap_or_else(|| "aa".repeat(32));
    let on_owner = author.sign_event(
        SPACE,
        "diverge-owner",
        460,
        Some(shared.clone()),
        EventBody::Checkpoint {
            upto: shared.clone(),
            epoch,
            digest: digest.clone(),
        },
    );
    let on_core = author.sign_event(
        SPACE,
        "diverge-core",
        461,
        Some(shared.clone()),
        EventBody::Checkpoint {
            upto: shared,
            epoch,
            digest,
        },
    );
    assert_eq!(
        fixture
            .owner
            .ingest(ordered(&fixture.owner, Envelope::plain(&on_owner))),
        Ingest::Applied
    );
    assert_eq!(
        fixture
            .core
            .engine
            .ingest(ordered(&fixture.core.engine, Envelope::plain(&on_core),)),
        Ingest::Applied
    );
    assert!(fixture.core.engine.state.spaces[SPACE]
        .integrity_error
        .is_none());
    let owner_head = fixture.owner.logs[SPACE].head().unwrap();
    let core_at_owner_seq = fixture.core.engine.logs[SPACE].events()[owner_head.seq as usize]
        .hash
        .clone();
    assert_ne!(owner_head.hash, core_at_owner_seq);
    let checkpoint = author.sign_event(
        SPACE,
        "fork-checkpoint",
        470,
        Some(owner_head),
        EventBody::Checkpoint {
            upto: fixture.owner.logs[SPACE].head().unwrap(),
            epoch: 1,
            digest: "ab".repeat(32),
        },
    );
    let got = fixture
        .core
        .engine
        .ingest(ordered(&fixture.core.engine, Envelope::plain(&checkpoint)));
    assert!(matches!(got, Ingest::Invalid(_)));
    assert!(fixture.core.engine.logs[SPACE]
        .events()
        .iter()
        .all(|event| event.client_id != "fork-checkpoint"));
    assert_eq!(
        fixture.core.engine.state.spaces[SPACE]
            .integrity_error
            .as_deref(),
        Some("Members see different groups in this chat (the server may have forked it).")
    );
    assert!(matches!(
        fixture.core.refresh(&original, 500),
        Err(NativeError::NotReady)
    ));
    assert!(matches!(
        DeviceCore::restore(fixture.core.image().unwrap(), fixture.credential()),
        Err(NativeError::Credential)
    ));
    println!(
        "JOURNEY honest checkpoint from the other fork parks live send and refuses cold restore"
    );
}

fn sealed_frame(author: &Author, client_id: &str, seen: Option<&Seen>) -> Envelope {
    Envelope::sealed(
        author,
        SPACE,
        client_id,
        1_700_000_000_500,
        seen,
        Sealed::new(SealedKind::Application, ::mls::SUITE_ID, vec![9, 9, 9]),
    )
}

#[test]
fn sealed_member_seen_ahead_marks_the_fork_and_parks_send() {
    let mut fixture = Fixture::new();
    fixture.grant("sealed-fork-trust", None);
    let trigger = fixture.message("Original request");
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let author = fixture.owner.net.author.clone().unwrap();
    let ahead = Seen {
        seq: 9_000,
        hash: "12".repeat(32),
    };
    let next = fixture.core.engine.next_seq(SPACE);
    let mut late = ordered(
        &fixture.core.engine,
        sealed_frame(&author, "sealed-gap", Some(&ahead)),
    );
    late.seq = next + 3;
    let gapped = fixture.core.engine.ingest(late);
    assert!(
        matches!(gapped, Ingest::Gap { next: n } if n == next),
        "{gapped:?}"
    );
    assert!(fixture.core.engine.state.spaces[SPACE]
        .integrity_error
        .is_none());

    let stranger = event_log::Author::root(event_log::Signer::generate());
    let refused = fixture.core.engine.ingest(ordered(
        &fixture.core.engine,
        sealed_frame(&stranger, "sealed-stranger", Some(&ahead)),
    ));
    assert!(matches!(refused, Ingest::Invalid(_)), "{refused:?}");
    let mut forged = sealed_frame(&author, "sealed-forged", Some(&ahead));
    forged.sig = "00".repeat(64);
    let forged = fixture
        .core
        .engine
        .ingest(ordered(&fixture.core.engine, forged));
    assert!(matches!(forged, Ingest::Invalid(_)), "{forged:?}");
    assert!(fixture.core.engine.state.spaces[SPACE]
        .integrity_error
        .is_none());
    assert_eq!(
        fixture
            .core
            .engine
            .store
            .meta(&format!("mls.fork:{SPACE}"))
            .unwrap(),
        None
    );
    assert_eq!(fixture.core.engine.next_seq(SPACE), next);

    let got = fixture.core.engine.ingest(ordered(
        &fixture.core.engine,
        sealed_frame(&author, "sealed-ahead", Some(&ahead)),
    ));
    assert!(matches!(got, Ingest::Invalid(_)), "{got:?}");
    assert!(fixture.core.engine.logs[SPACE]
        .events()
        .iter()
        .all(|event| event.client_id != "sealed-ahead"));
    assert_eq!(fixture.core.engine.next_seq(SPACE), next);
    assert_eq!(
        fixture.core.engine.state.spaces[SPACE]
            .integrity_error
            .as_deref(),
        Some("Members see different groups in this chat (the server may have forked it)."),
        "sealed member seen ahead did not mark the fork"
    );
    assert_eq!(
        fixture
            .core
            .engine
            .store
            .meta(&format!("mls.fork:{SPACE}"))
            .unwrap()
            .as_deref(),
        Some("Members see different groups in this chat (the server may have forked it).")
    );
    assert!(matches!(
        fixture.core.refresh(&original, 500),
        Err(NativeError::NotReady)
    ));
}

#[test]
fn no_default_or_optimistic_grant_can_start_a_reply() {
    let mut fixture = Fixture::new();
    let trigger = fixture.message("No explicit grant yet");
    assert!(matches!(
        fixture.core.reply(SPACE, &trigger, 400),
        Err(NativeError::Permission)
    ));
    let pending = fixture.owner.net.author.clone().unwrap().sign_event(
        SPACE,
        "pending-trust",
        200,
        fixture.owner.logs.get(SPACE).and_then(SpaceLog::head),
        EventBody::GrantIssued {
            grant: Grant {
                id: "optimistic-owner-trust".into(),
                grantor: fixture.owner_profile.id.clone(),
                grantee: Some(fixture.agent_profile.id.clone()),
                scope: GrantScope::Space(SPACE.into()),
                capability: Capability::Trust(TrustLevel::Autonomous),
                expires_at_ms: None,
            },
        },
    );
    fixture.core.engine.store.outbox_put(&pending).unwrap();
    fixture.core.engine.reload().unwrap();
    assert!(fixture
        .core
        .engine
        .state
        .grants
        .iter()
        .any(|g| g.grant.id == "optimistic-owner-trust"));
    assert!(matches!(
        fixture.core.reply(SPACE, &trigger, 400),
        Err(NativeError::Permission)
    ));
    fixture.grant("real-owner-trust", None);
    let real = fixture.message("Explicit opened owner trust");
    assert!(fixture.core.reply(SPACE, &real, 400).is_ok());
    println!("JOURNEY native optimistic overlay and default trust excluded from ordered reply permission");
}

#[test]
fn revoke_replacement_reader_and_opaque_history_stop_the_original_reply() {
    for case in ["revoke", "replace", "reader", "opaque", "unsupported"] {
        let mut fixture = Fixture::new();
        fixture.grant("initial-trust", None);
        let trigger = fixture.message("Initial retained request");
        let original = fixture
            .core
            .reply(SPACE, &trigger, 400)
            .unwrap()
            .intent()
            .clone();
        match case {
            "revoke" => {
                fixture.encrypted(
                    EventBody::GrantRevoked {
                        grant: "initial-trust".into(),
                    },
                    450,
                );
            }
            "replace" => {
                fixture.grant("replacement-trust", None);
            }
            "reader" => {
                fixture.clear(EventBody::MemberAdded {
                    identity: fixture.agent_profile.id.clone(),
                    role: Role::Reader,
                });
            }
            "unsupported" => {
                fixture.encrypted(
                    EventBody::BackgroundSet {
                        background: BackgroundSpec {
                            style: "none".into(),
                            media: None,
                            zoom_pm: 1000,
                            offset_x_pm: 0,
                            offset_y_pm: 0,
                            dim_pm: None,
                            blur_pm: 0,
                            appearance: "auto".into(),
                        },
                    },
                    450,
                );
            }
            "opaque" => {
                let author = fixture.owner.net.author.as_ref().unwrap();
                let envelope = Envelope::sealed(
                    author,
                    SPACE,
                    "unreadable",
                    450,
                    fixture.owner.logs[SPACE].head().as_ref(),
                    Sealed::new(SealedKind::Application, ::mls::SUITE_ID, vec![1, 2, 3]),
                );
                let frame = ordered(&fixture.owner, envelope);
                fixture.core.ingest(vec![frame]).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            matches!(
                fixture.core.refresh(&original, 500),
                Err(NativeError::Permission)
            ),
            "{case}"
        );
    }
    println!("JOURNEY exact grant revoke/replacement, Reader role, opaque and unsupported history deny refresh");
}

#[test]
fn certified_device_and_owner_proof_are_required_without_root_fallback() {
    let fixture = Fixture::new();
    let mut agent = fixture.agent_profile.clone();
    agent.owner_proof = None;
    assert!(matches!(
        UnlockedDevice::verify(
            agent,
            fixture.owner_profile.clone(),
            fixture.agent_cert.clone(),
            fixture.agent_device.secret()
        ),
        Err(NativeError::Credential)
    ));
    assert!(matches!(
        UnlockedDevice::verify(
            fixture.agent_profile.clone(),
            fixture.owner_profile.clone(),
            "00".repeat(64),
            fixture.agent_device.secret()
        ),
        Err(NativeError::Credential)
    ));
    assert!(matches!(
        UnlockedDevice::verify(
            fixture.agent_profile.clone(),
            fixture.owner_profile.clone(),
            fixture.agent_cert.clone(),
            Signer::generate().secret()
        ),
        Err(NativeError::Credential)
    ));
    let mut owner = fixture.owner_profile.clone();
    owner.kind = IdentityKind::Agent;
    assert!(matches!(
        UnlockedDevice::verify(
            fixture.agent_profile.clone(),
            owner,
            fixture.agent_cert.clone(),
            fixture.agent_device.secret()
        ),
        Err(NativeError::Credential)
    ));
    let agent_root = Signer::generate();
    let owner_author = fixture.owner.net.author.as_ref().unwrap();
    let mut agent = profile(&agent_root, IdentityKind::Agent);
    agent.owner = Some(fixture.owner_profile.id.clone());
    agent.owner_proof = Some(Box::new(
        agent_owner::authorize(owner_author, &agent.id).unwrap(),
    ));
    let self_certificate = agent_root.sign(&event_log::device_cert_message(&agent_root.id()));
    assert!(matches!(
        UnlockedDevice::verify(
            agent,
            fixture.owner_profile.clone(),
            self_certificate,
            agent_root.secret()
        ),
        Err(NativeError::Credential)
    ));
    println!("JOURNEY unsigned owner, false certificate, wrong device, non-Person owner and Agent root fallback refused");
}

#[test]
fn image_rejects_legacy_schema_provider_and_secrets_before_migrations() {
    let fixture = Fixture::new();
    for statement in [
        "PRAGMA user_version=1",
        "UPDATE meta SET value='2' WHERE key='event_format'",
        "CREATE TABLE injected (secret BLOB)",
        "DELETE FROM openmls_sqlite_storage_migrations WHERE version=6",
        "UPDATE identities SET secret=zeroblob(32)",
        "INSERT INTO meta VALUES ('zoen.identity.v1','root-secret')",
        "INSERT INTO vc_emulation_group_secrets VALUES (99, x'01', 'pprf', x'02')",
    ] {
        let image = fixture.core.image().unwrap();
        let store = Store::from_memory_image(image.as_bytes()).unwrap();
        store.conn().execute_batch(statement).unwrap();
        let bytes = store.memory_image().unwrap();
        let before = store.event_count().unwrap();
        let rejected = DeviceCore::restore(
            NativeImage::decode_bounded(bytes).unwrap(),
            fixture.credential(),
        );
        assert!(matches!(rejected, Err(NativeError::Image)), "{statement}");
        assert_eq!(
            store.event_count().unwrap(),
            before,
            "refusal must precede destructive migration"
        );
    }
    assert!(NativeImage::decode_bounded(vec![0; MAX_MEMORY_IMAGE_BYTES + 1]).is_err());
    assert!(NativeImage::decode_bounded(b"SQLite format 3\0".to_vec()).is_err());
    assert!(!format!("{:?}", fixture.core.image().unwrap()).contains("SQLite"));
    println!("JOURNEY strict image bounds, exact schema/event/provider versions and root/demo secret rejection");
}
