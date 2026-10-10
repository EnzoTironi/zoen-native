use super::*;
use roda_log::{
    chain_hash,
    content::{Sealed, SealedKind},
    SpaceLog,
};
use roda_proto::{Envelope, KeyPackageClaimClock, KeyPackageRecord};

const SPACE: &str = "certified-owner-agent-direct";

struct Fixture {
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
            vec![roda_proto::AgreementKeyRecord {
                identity: fixture.agent_profile.id.clone(),
                public,
                signed,
            }],
            std::slice::from_ref(&fixture.agent_profile.id),
        );
        fixture.owner.create_mls_group(SPACE).unwrap();
        let clock = KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap();
        let (space, targets, operation) = fixture.owner.mls_to_claim(Some(clock)).unwrap().unwrap();
        assert_eq!(space, SPACE);
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
            SPACE,
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
        assert!(fixture.core.engine.ready_epoch(SPACE).is_some());
        fixture
    }

    fn clear(&mut self, body: EventBody) -> String {
        let author = self.owner.net.author.as_ref().unwrap();
        let seen = self.owner.logs.get(SPACE).and_then(SpaceLog::head);
        let event = author.sign_event(SPACE, &new_id("clear"), 100, seen, body);
        let frame = ordered(&self.owner, Envelope::plain(&event));
        assert_eq!(self.owner.ingest(frame.clone()), Ingest::Applied);
        self.core.ingest(vec![frame.clone()]).unwrap();
        frame.hash
    }

    fn encrypted(&mut self, body: EventBody, at_ms: i64) -> String {
        let author = self.owner.net.author.clone().unwrap();
        self.owner
            .append_synced(SPACE, &author, at_ms, body)
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
                    scope: GrantScope::Space(SPACE.into()),
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
    let author = fixture.core.engine.net.author.clone().unwrap();
    fixture
        .core
        .engine
        .append_synced(
            SPACE,
            &author,
            450,
            EventBody::MessagePosted {
                message: new_id("message"),
                text: "Agent's retained own reply".into(),
                attaches: None,
                reply: None,
            },
        )
        .unwrap();
    fixture.core.engine.mls_seal_outbox().unwrap();
    let mut own = None;
    for envelope in fixture.core.engine.outbox_envelopes() {
        let frame = ordered(&fixture.owner, envelope);
        assert_eq!(fixture.owner.ingest(frame.clone()), Ingest::Applied);
        fixture.core.ingest(vec![frame.clone()]).unwrap();
        if frame.env.sealed_kind() == Some(SealedKind::Application) {
            own = Some(frame.hash);
        }
    }
    let own = own.unwrap();
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
        },
    );
    let plaintext = roda_log::content::InnerEvent {
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
        Sealed::new(SealedKind::Application, roda_mls::SUITE_ID, ciphertext),
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
                    Sealed::new(SealedKind::Application, roda_mls::SUITE_ID, vec![1, 2, 3]),
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
    let self_certificate = agent_root.sign(&roda_log::device_cert_message(&agent_root.id()));
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
