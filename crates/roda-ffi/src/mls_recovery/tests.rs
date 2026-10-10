use super::*;
use crate::sync::Ingest;
use roda_log::{chain_hash, SpaceLog};
use roda_proto::Sequenced;
use roda_types::{EventBody, Privacy, SpaceKind};

fn ordered(engine: &Engine, env: Envelope) -> Sequenced {
    let (seq, prev) = engine
        .logs
        .get(env.space())
        .map(|log| {
            (
                log.next_seq(),
                log.events()
                    .last()
                    .map(|e| e.hash.clone())
                    .unwrap_or_else(|| roda_types::GENESIS_PREV.into()),
            )
        })
        .unwrap_or_else(|| (0, roda_types::GENESIS_PREV.into()));
    Sequenced {
        seq,
        hash: chain_hash(env.space(), seq, &prev, &env.wire_hash()),
        prev,
        env,
    }
}

fn seen(engine: &Engine, space: &str) -> Option<roda_types::Seen> {
    engine
        .logs
        .get(space)
        .and_then(|log| log.events().last())
        .map(|e| roda_types::Seen {
            seq: e.seq,
            hash: e.hash.clone(),
        })
}

struct StagedRecovery {
    engine: Engine,
    peer_engine: Engine,
    root: [u8; 32],
    device: [u8; 32],
    agreement: [u8; 32],
    peer: roda_proto::AgreementKeyRecord,
    baseline: Vec<Sequenced>,
    context_blob: Vec<u8>,
}

fn staged_recovery(path: &str, space: &str) -> StagedRecovery {
    let mut engine = Engine::open(path).unwrap();
    let (root, _, agreement) = engine
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    let mut peer_engine = Engine::open(":memory:").unwrap();
    peer_engine
        .create_account("Bruno", "bruno", "http://relay.test")
        .unwrap();
    engine.net.profiles.supported = true;
    peer_engine.net.profiles.supported = true;
    let (public, signed) = peer_engine.agreement_to_publish().unwrap();
    let peer = roda_proto::AgreementKeyRecord {
        identity: peer_engine.me.clone().unwrap(),
        public,
        signed,
    };
    engine.agreement_keys_arrived(vec![peer.clone()], std::slice::from_ref(&peer.identity));
    let author = engine.net.author.clone().unwrap();
    let mut baseline = Vec::new();
    for (cid, body) in [
        (
            "genesis",
            EventBody::SpaceCreated {
                title: "Held recovery".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        ),
        (
            "add-peer",
            EventBody::MemberAdded {
                identity: peer.identity.clone(),
                role: Role::Member,
            },
        ),
    ] {
        let env = Envelope::plain(&author.sign_event(space, cid, 1, seen(&engine, space), body));
        let confirmed = ordered(&engine, env);
        assert_eq!(engine.ingest(confirmed.clone()), Ingest::Applied);
        baseline.push(confirmed);
    }
    engine.create_mls_group(space).unwrap();
    let packages = peer_engine
        .device()
        .unwrap()
        .key_packages(1, false)
        .unwrap();
    engine
        .commit_now(space, &packages, &Default::default())
        .unwrap();
    let queued = engine.store.outbox().unwrap();
    let reference =
        RecoveryRef::parse(Envelope::plain(&queued[0].event).recovery().unwrap()).unwrap();
    let context_blob = engine
        .pending_uploads(8)
        .into_iter()
        .find(|(sha, _)| *sha == reference.blob)
        .unwrap()
        .1;
    engine.upload_done(&reference.blob);
    for queued in queued {
        let confirmed = ordered(&engine, Envelope::plain(&queued.event));
        assert_eq!(engine.ingest(confirmed.clone()), Ingest::Confirmed);
        baseline.push(confirmed);
    }
    engine.device().unwrap().forget(space).unwrap();
    let replacement = roda_log::Signer::generate();
    let device = replacement.secret();
    engine
        .install_restored_account(
            &roda_log::Signer::from_secret(&root),
            replacement,
            "http://relay.test",
        )
        .unwrap();
    engine.unlock_profile(Some(agreement.to_vec())).unwrap();
    engine.net.profiles.supported = true;
    let reference = engine.recovery_context(space).unwrap();
    assert!(engine.recovery_context_arrived(&reference, &context_blob));
    assert!(engine.mls_recovery_pass().unwrap());
    assert!(engine.device().unwrap().recovery_pending(space));
    assert!(engine.outbox_envelopes().is_empty());
    StagedRecovery {
        engine,
        peer_engine,
        root,
        device,
        agreement,
        peer,
        baseline,
        context_blob,
    }
}

#[test]
fn a_staged_recovery_checkpoint_does_not_mean_the_peer_has_admitted_the_device() {
    let space = "recovery-admission";
    let fixture = staged_recovery(":memory:", space);
    let mut recovered = fixture.engine;
    let mut peer = fixture.peer_engine;
    for frame in &fixture.baseline {
        assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    }
    assert!(recovered.mls_status(space).is_some());
    assert!(recovered.device().unwrap().recovery_pending(space));
    assert_ne!(recovered.mls_status(space), peer.mls_status(space));
    let external = Envelope::plain(&recovered.store.outbox().unwrap()[0].event);
    let reference = RecoveryRef::parse(external.recovery().unwrap()).unwrap();
    recovered.upload_done(&reference.blob);
    let frame = ordered(&peer, external);
    assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    assert_eq!(recovered.ingest(frame), Ingest::Confirmed);
    assert!(!recovered.device().unwrap().recovery_pending(space));
    assert_eq!(recovered.mls_status(space), peer.mls_status(space));
}

#[test]
fn peer_external_commit_storage_failure_keeps_the_cursor_epoch_and_context_for_retry() {
    let space = "peer-merge-storage-failure";
    let fixture = staged_recovery(":memory:", space);
    let mut recovered = fixture.engine;
    let mut peer = fixture.peer_engine;
    for frame in &fixture.baseline {
        assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    }
    let external = Envelope::plain(&recovered.store.outbox().unwrap()[0].event);
    let reference = RecoveryRef::parse(external.recovery().unwrap()).unwrap();
    recovered.upload_done(&reference.blob);
    let frame = ordered(&peer, external);
    let before = (
        peer.logs[space].next_seq(),
        seen(&peer, space),
        peer.store.events(space).unwrap().len(),
        peer.device().unwrap().checkpoint(space).unwrap(),
        peer.recovery_context(space),
        peer.device().unwrap().recovery_pending(space),
        peer.device().unwrap().needs_reconciliation(space),
        peer.outbox_len(),
        peer.store.cursors().unwrap(),
    );
    assert!(
        !before.5,
        "the existing peer has no own recovery to confirm"
    );
    for (install, remove, reason) in [
        (
            "CREATE TRIGGER fail_peer_merge BEFORE INSERT ON openmls_group_data
         WHEN NEW.data_type = 'group_state'
         BEGIN SELECT RAISE(ABORT, 'injected peer merge failure'); END;",
            "DROP TRIGGER fail_peer_merge",
            "injected peer merge failure",
        ),
        (
            "ALTER TABLE openmls_group_data RENAME TO unavailable_group_data",
            "ALTER TABLE unavailable_group_data RENAME TO openmls_group_data",
            "no such table",
        ),
        (
            "ALTER TABLE openmls_epoch_keys_pairs RENAME TO unavailable_epoch_keys",
            "ALTER TABLE unavailable_epoch_keys RENAME TO openmls_epoch_keys_pairs",
            "MissingDecryptionKey",
        ),
    ] {
        peer.store.conn().execute_batch(install).unwrap();
        let result = peer.ingest(frame.clone());
        assert!(
            matches!(result, Ingest::Invalid(ref error) if error.contains(reason)),
            "{result:?}"
        );
        peer.store.conn().execute_batch(remove).unwrap();
        assert_eq!(
            (
                peer.logs[space].next_seq(),
                seen(&peer, space),
                peer.store.events(space).unwrap().len(),
                peer.device().unwrap().checkpoint(space).unwrap(),
                peer.recovery_context(space),
                peer.device().unwrap().recovery_pending(space),
                peer.device().unwrap().needs_reconciliation(space),
                peer.outbox_len(),
                peer.store.cursors().unwrap(),
            ),
            before
        );
    }
    assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    assert_eq!(recovered.ingest(frame), Ingest::Confirmed);
    assert_eq!(peer.logs[space].next_seq(), before.0 + 1);
    assert_eq!(
        peer.recovery_context(space),
        recovered.recovery_context(space)
    );
    assert_eq!(
        peer.device().unwrap().checkpoint(space).unwrap(),
        recovered.device().unwrap().checkpoint(space).unwrap()
    );
    for (sender, recipient) in [(&recovered, &peer), (&peer, &recovered)] {
        let sender = sender.device().unwrap();
        let data = sender
            .seal(space, b"readable after the identical retry")
            .unwrap();
        assert!(matches!(
            recipient.device().unwrap().open(space, &data, &recipient.roster(space), sender.leaf()).unwrap(),
            roda_mls::Opened::Application { plaintext, .. } if plaintext == b"readable after the identical retry"
        ));
    }
}

#[test]
fn peer_application_processing_failure_keeps_the_same_frame_readable_on_retry() {
    let space = "peer-processing-storage-failure";
    let fixture = staged_recovery(":memory:", space);
    let mut recovered = fixture.engine;
    let mut peer = fixture.peer_engine;
    for frame in &fixture.baseline {
        assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    }
    let external = Envelope::plain(&recovered.store.outbox().unwrap()[0].event);
    let reference = RecoveryRef::parse(external.recovery().unwrap()).unwrap();
    recovered.upload_done(&reference.blob);
    let frame = ordered(&peer, external);
    assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    assert_eq!(recovered.ingest(frame), Ingest::Confirmed);
    recovered.send_message(space, "readable on retry").unwrap();
    recovered.mls_seal_outbox().unwrap();
    let application = recovered
        .outbox_envelopes()
        .into_iter()
        .find(|env| env.sealed_kind() == Some(SealedKind::Application))
        .unwrap();
    let frame = ordered(&peer, application);
    let before = (
        peer.logs[space].next_seq(),
        peer.store.cursors().unwrap(),
        peer.store.events(space).unwrap().len(),
        peer.device().unwrap().checkpoint(space).unwrap(),
        peer.recovery_context(space),
    );
    peer.store
        .conn()
        .execute_batch(
            "CREATE TRIGGER fail_processing BEFORE INSERT ON openmls_group_data
         WHEN NEW.data_type = 'message_secrets'
         BEGIN SELECT RAISE(ABORT, 'injected processing failure'); END;",
        )
        .unwrap();
    let result = peer.ingest(frame.clone());
    assert!(
        matches!(result, Ingest::Invalid(ref reason) if reason.contains("injected processing failure")),
        "{result:?}"
    );
    assert_eq!(
        (
            peer.logs[space].next_seq(),
            peer.store.cursors().unwrap(),
            peer.store.events(space).unwrap().len(),
            peer.device().unwrap().checkpoint(space).unwrap(),
            peer.recovery_context(space),
        ),
        before
    );
    peer.store
        .conn()
        .execute_batch("DROP TRIGGER fail_processing")
        .unwrap();
    assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    assert_eq!(recovered.ingest(frame), Ingest::Confirmed);
    assert!(matches!(
        &peer.state.spaces[space].entries.last().unwrap().body,
        crate::engine::EntryBody::Message { text, .. } if text == "readable on retry"
    ));
}

#[test]
fn welcome_storage_failure_rolls_back_the_key_package_and_cursor_for_retry() {
    let space = "welcome-storage-failure";
    let fixture = staged_recovery(":memory:", space);
    let mut peer = fixture.peer_engine;
    let welcome = fixture.baseline.last().unwrap().clone();
    assert_eq!(welcome.env.sealed_kind(), Some(SealedKind::Welcome));
    for frame in &fixture.baseline[..fixture.baseline.len() - 1] {
        assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    }
    assert!(!peer.device().unwrap().has_group(space));
    let before = (
        peer.logs[space].next_seq(),
        peer.store.cursors().unwrap(),
        peer.store.events(space).unwrap().len(),
        peer.recovery_context(space),
    );
    peer.store
        .conn()
        .execute_batch(
            "CREATE TRIGGER fail_welcome BEFORE INSERT ON openmls_group_data
         WHEN NEW.data_type = 'group_state'
         BEGIN SELECT RAISE(ABORT, 'injected Welcome failure'); END;",
        )
        .unwrap();
    let result = peer.ingest(welcome.clone());
    assert!(
        matches!(result, Ingest::Invalid(ref reason) if reason.contains("injected Welcome failure")),
        "{result:?}"
    );
    assert_eq!(
        (
            peer.logs[space].next_seq(),
            peer.store.cursors().unwrap(),
            peer.store.events(space).unwrap().len(),
            peer.recovery_context(space),
        ),
        before
    );
    assert!(!peer.device().unwrap().has_group(space));
    peer.store
        .conn()
        .execute_batch("DROP TRIGGER fail_welcome")
        .unwrap();
    assert_eq!(peer.ingest(welcome), Ingest::Applied);
    assert_eq!(peer.device().unwrap().epoch(space).unwrap(), 1);
    assert_eq!(peer.logs[space].next_seq(), before.0 + 1);
}

#[test]
fn a_removal_during_held_recovery_survives_restart_and_repairs_before_sealing() {
    let path = std::env::temp_dir().join(format!(
        "zoen-removal-recovery-{}.db",
        roda_types::new_ulid(now_ms())
    ));
    let space = "removed-during-upload";
    let fixture = staged_recovery(path.to_str().unwrap(), space);
    let mut engine = fixture.engine;
    let external = Envelope::plain(&engine.store.outbox().unwrap()[0].event);
    let reference = RecoveryRef::parse(external.recovery().unwrap()).unwrap();
    let author = engine.net.author.clone().unwrap();
    let removal = Envelope::plain(&author.sign_event(
        space,
        "remove-during-upload",
        2,
        seen(&engine, space),
        EventBody::MemberRemoved {
            identity: fixture.peer.identity.clone(),
        },
    ));
    assert_eq!(engine.ingest(ordered(&engine, removal)), Ingest::Applied);
    assert!(engine.device().unwrap().recovery_pending(space));
    assert!(engine.outbox_envelopes().is_empty());
    engine.upload_done(&reference.blob);
    assert_eq!(engine.ingest(ordered(&engine, external)), Ingest::Confirmed);
    assert!(!engine.device().unwrap().recovery_pending(space));
    assert!(engine.device().unwrap().needs_reconciliation(space));
    assert!(engine
        .device()
        .unwrap()
        .seal(space, b"held for removal")
        .is_err());
    assert_eq!(engine.outbox_len(), 0);
    drop(engine);

    let mut engine = Engine::open(path.to_str().unwrap()).unwrap();
    engine.unlock(Some(fixture.device.to_vec())).unwrap();
    engine
        .unlock_profile(Some(fixture.agreement.to_vec()))
        .unwrap();
    engine.net.profiles.supported = true;
    assert!(engine.device().unwrap().needs_reconciliation(space));
    assert!(engine
        .device()
        .unwrap()
        .seal(space, b"held after restart")
        .is_err());
    assert!(engine
        .mls_to_claim(Some(
            roda_proto::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap()
        ))
        .unwrap()
        .is_none());
    let repairs = engine.store.outbox().unwrap();
    assert_eq!(
        repairs.len(),
        1,
        "the reopened engine automatically stages the removal commit"
    );
    let repair = Envelope::plain(&repairs[0].event);
    let reference = RecoveryRef::parse(repair.recovery().unwrap()).unwrap();
    assert!(engine.outbox_envelopes().is_empty());
    engine.upload_done(&reference.blob);
    assert_eq!(engine.ingest(ordered(&engine, repair)), Ingest::Confirmed);
    assert!(!engine.device().unwrap().needs_reconciliation(space));
    assert_eq!(
        engine.device().unwrap().roster(space).unwrap(),
        engine.roster(space)
    );
    assert!(!engine
        .device()
        .unwrap()
        .roster(space)
        .unwrap()
        .contains(&fixture.peer.identity));
    assert!(engine
        .device()
        .unwrap()
        .seal(space, b"ready after the confirmed repair")
        .is_ok());
    assert_eq!(engine.outbox_len(), 0);
    drop(engine);
    let _ = std::fs::remove_file(path);
}

#[test]
fn agreement_lookup_finishes_before_the_engine_requests_one_shot_key_packages() {
    let mut engine = Engine::open(":memory:").unwrap();
    engine
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    let mut peer = Engine::open(":memory:").unwrap();
    peer.create_account("Bruno", "bruno", "http://relay.test")
        .unwrap();
    engine.net.profiles.supported = true;
    peer.net.profiles.supported = true;
    let author = engine.net.author.clone().unwrap();
    let identity = peer.me.clone().unwrap();
    let space = "agreement-before-claim";
    for (cid, body) in [
        (
            "genesis",
            EventBody::SpaceCreated {
                title: "No wasted claims".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        ),
        (
            "member",
            EventBody::MemberAdded {
                identity: identity.clone(),
                role: Role::Member,
            },
        ),
    ] {
        let env = Envelope::plain(&author.sign_event(space, cid, 1, seen(&engine, space), body));
        assert_eq!(engine.ingest(ordered(&engine, env)), Ingest::Applied);
    }
    engine.create_mls_group(space).unwrap();
    assert!(engine
        .mls_to_claim(Some(
            roda_proto::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap()
        ))
        .unwrap()
        .is_none());
    assert!(engine.take_need_agreement().contains(&identity));
    assert_eq!(engine.outbox_len(), 0);
    let (public, signed) = peer.agreement_to_publish().unwrap();
    engine.agreement_keys_arrived(
        vec![roda_proto::AgreementKeyRecord {
            identity: identity.clone(),
            public,
            signed,
        }],
        std::slice::from_ref(&identity),
    );
    let (claimed_space, targets, operation) = engine
        .mls_to_claim(Some(
            roda_proto::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap(),
        ))
        .unwrap()
        .unwrap();
    assert_eq!(claimed_space, space);
    assert_eq!(targets, vec![identity.clone()]);
    let package = peer
        .device()
        .unwrap()
        .key_packages(1, false)
        .unwrap()
        .remove(0);
    engine.mls_claimed(
        space,
        &operation,
        Ok(vec![roda_proto::KeyPackageRecord {
            identity,
            device: peer.net.account.as_ref().unwrap().device.clone(),
            data: package,
        }]),
    );
    assert_eq!(
        engine.outbox_len(),
        2,
        "one claim stages a commit and Welcome"
    );
    assert!(engine.device().unwrap().pending(space));
    assert_eq!(engine.uploads_pending(), 1);
}

#[derive(Default)]
struct Vault(std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>);
impl crate::SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.0.lock().unwrap().insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        self.0.lock().unwrap().remove(&key);
    }
}

#[test]
fn erase_device_clears_held_markers_atomically_and_fresh_restore_can_recover_the_same_space() {
    use crate::SecretVault;
    let path = std::env::temp_dir().join(format!(
        "zoen-erased-recovery-{}.db",
        roda_types::new_ulid(now_ms())
    ));
    let space = "recover-after-erasure";
    let fixture = staged_recovery(path.to_str().unwrap(), space);
    drop(fixture.engine);
    let api = crate::RodaEngine::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    let vault = std::sync::Arc::new(Vault::default());
    for (key, value) in [
        (crate::sync::VAULT_IDENTITY, fixture.root),
        (crate::sync::VAULT_DEVICE, fixture.device),
        (crate::profile::VAULT_AGREEMENT, fixture.agreement),
    ] {
        vault.save(key.into(), value.to_vec());
    }
    api.lock().store.conn().execute_batch("CREATE TRIGGER fail_erase BEFORE DELETE ON mls_recovery_pending BEGIN SELECT RAISE(ABORT, 'injected erasure write failure'); END;").unwrap();
    assert!(api.erase_device(vault.clone()).is_err());
    assert_eq!(api.lock().outbox_len(), 1);
    assert_eq!(
        vault.0.lock().unwrap().len(),
        3,
        "failed erasure preserves account secrets"
    );
    {
        let mut engine = api.lock();
        engine.unlock(Some(fixture.device.to_vec())).unwrap();
        assert!(engine.device().unwrap().recovery_pending(space));
        engine
            .store
            .conn()
            .execute_batch("DROP TRIGGER fail_erase")
            .unwrap();
        // Also clear the durable post-confirmation hold, when present.
        engine
            .store
            .conn()
            .execute(
                "INSERT INTO mls_recovery_reconcile (space) VALUES (?1)",
                [space],
            )
            .unwrap();
    }
    api.erase_device(vault.clone()).unwrap();
    assert!(vault.0.lock().unwrap().is_empty());
    {
        let engine = api.lock();
        assert_eq!(engine.outbox_len(), 0);
        let markers: i64 = engine.store.conn().query_row("SELECT (SELECT COUNT(*) FROM mls_recovery_pending) + (SELECT COUNT(*) FROM mls_recovery_reconcile)", [], |r| r.get(0)).unwrap();
        assert_eq!(markers, 0);
    }
    drop(api);
    let mut engine = Engine::open(path.to_str().unwrap()).unwrap();
    engine
        .install_restored_account(
            &roda_log::Signer::from_secret(&fixture.root),
            roda_log::Signer::generate(),
            "http://relay.test",
        )
        .unwrap();
    engine
        .unlock_profile(Some(fixture.agreement.to_vec()))
        .unwrap();
    engine.net.profiles.supported = true;
    engine.agreement_keys_arrived(
        vec![fixture.peer.clone()],
        std::slice::from_ref(&fixture.peer.identity),
    );
    for frame in fixture.baseline {
        assert_eq!(engine.ingest(frame), Ingest::Applied);
    }
    assert!(!engine.device().unwrap().has_group(space));
    assert!(!engine.device().unwrap().pending(space));
    let reference = engine.recovery_context(space).unwrap();
    assert!(engine.recovery_context_arrived(&reference, &fixture.context_blob));
    assert!(engine.mls_recovery_pass().unwrap());
    assert!(engine.device().unwrap().recovery_pending(space));
    assert_eq!(engine.outbox_len(), 1);
    drop(engine);
    let _ = std::fs::remove_file(path);
}

#[test]
fn legacy_stub_cannot_install_or_invalidate_authenticated_recovery_context() {
    use prost::Message;
    use roda_log::content::{Payload, SignedContent};
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../roda-log/testdata/legacy-sealed-v3.json"
    ))
    .unwrap();
    let mut wire = roda_proto::wire::PbSequenced::decode(
        hex::decode(fixture["legacy_stub"].as_str().unwrap())
            .unwrap()
            .as_slice(),
    )
    .unwrap();
    let env = wire.env.as_mut().unwrap();
    let mut content = SignedContent::decode(env.content.as_slice()).unwrap();
    let Some(Payload::Sealed(sealed)) = &mut content.payload else {
        unreachable!()
    };
    sealed.kind = SealedKind::Commit as i32;
    sealed.recovery = RecoveryRef {
        version: 1,
        epoch: 7,
        blob: "aa".repeat(32),
    }
    .encode();
    env.content = content.encode();
    let grafted = Sequenced::decode(&wire.encode_to_vec()).unwrap();
    grafted.env.verify().unwrap();
    assert!(grafted.env.recovery().is_none());
    let engine = Engine::open(":memory:").unwrap();
    let space = grafted.env.space();
    engine.remember_recovery_context(&grafted.env).unwrap();
    assert!(engine.recovery_context(space).is_none());
    let reference = ContextRef {
        space: space.into(),
        epoch: 8,
        blob: "bb".repeat(32),
        device: "authenticated-publisher".into(),
    };
    engine
        .store
        .set_meta(
            &context_meta(space),
            &serde_json::to_string(&reference).unwrap(),
        )
        .unwrap();
    engine.remember_recovery_context(&grafted.env).unwrap();
    assert_eq!(engine.recovery_context(space), Some(reference));
}

#[test]
fn context_and_commit_roll_back_together_and_wait_for_upload_after_restart() {
    let path = std::env::temp_dir().join(format!(
        "zoen-recovery-{}.db",
        roda_types::new_ulid(now_ms())
    ));
    let space = "recovery-transaction";
    let (root_secret, secret, agreement, commit, reference, genesis, uploaded_blob);
    {
        let mut engine = Engine::open(path.to_str().unwrap()).unwrap();
        let (root, device, key) = engine
            .create_account("Ana", "ana", "http://relay.test")
            .unwrap();
        secret = device;
        root_secret = root;
        agreement = key;
        let author = engine.net.author.clone().unwrap();
        let mut log = SpaceLog::new(space);
        let created = log
            .sequence(author.sign_event(
                space,
                "genesis",
                1,
                None,
                EventBody::SpaceCreated {
                    title: "Recovery".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::EndToEnd,
                },
            ))
            .clone();
        genesis = created.hash.clone();
        assert_eq!(
            engine.ingest(Sequenced {
                seq: 0,
                prev: created.prev.clone(),
                hash: created.hash.clone(),
                env: Envelope::plain(&created)
            }),
            Ingest::Applied
        );
        engine.create_mls_group(space).unwrap();
        engine.net.profiles.supported = true;
        engine.store.conn().execute_batch("CREATE TRIGGER fail_context BEFORE INSERT ON mls_recovery_uploads BEGIN SELECT RAISE(ABORT, 'injected write failure'); END;").unwrap();
        assert!(engine.commit_now(space, &[], &Default::default()).is_err());
        assert!(!engine.device().unwrap().pending(space));
        assert_eq!(engine.outbox_len(), 0);
        assert_eq!(engine.uploads_pending(), 0);
        engine
            .store
            .conn()
            .execute_batch("DROP TRIGGER fail_context")
            .unwrap();
        engine.commit_now(space, &[], &Default::default()).unwrap();
        let queued = engine.store.outbox().unwrap();
        assert_eq!(queued.len(), 1);
        commit = Envelope::plain(&queued[0].event);
        reference = RecoveryRef::parse(commit.recovery().unwrap()).unwrap();
        uploaded_blob = engine.pending_uploads(1).remove(0).1;
        assert_eq!(engine.uploads_pending(), 1);
        assert!(engine.outbox_envelopes().is_empty());
    }
    let (replacement_secret, external, external_reference, previous);
    {
        let mut engine = Engine::open(path.to_str().unwrap()).unwrap();
        assert!(engine.unlock(Some(secret.to_vec())).unwrap());
        engine.unlock_profile(Some(agreement.to_vec())).unwrap();
        assert!(engine.device().unwrap().pending(space));
        assert!(engine.outbox_envelopes().is_empty());
        engine.upload_refused(&reference.blob);
        assert_eq!(engine.uploads_pending(), 1);
        assert!(
            engine.outbox_envelopes().is_empty(),
            "HTTP refusal must not release the commit"
        );
        engine.upload_done(&reference.blob);
        let ready = engine.outbox_envelopes();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].content(), commit.content());
        assert_eq!(ready[0].client_id(), commit.client_id());
        previous = chain_hash(space, 1, &genesis, &commit.wire_hash());
        assert_eq!(
            engine.ingest(Sequenced {
                seq: 1,
                prev: genesis.clone(),
                hash: previous.clone(),
                env: commit
            }),
            Ingest::Confirmed
        );
        assert!(!engine.device().unwrap().pending(space));
        assert_eq!(engine.outbox_len(), 0);
        assert_eq!(engine.recovery_context(space).unwrap().blob, reference.blob);
        engine.device().unwrap().forget(space).unwrap();
        let replacement = roda_log::Signer::generate();
        replacement_secret = replacement.secret();
        engine
            .install_restored_account(
                &roda_log::Signer::from_secret(&root_secret),
                replacement,
                "http://relay.test",
            )
            .unwrap();
        engine.net.profiles.supported = true;
        let context = engine.recovery_context(space).unwrap();
        assert!(engine.recovery_context_arrived(&context, &uploaded_blob));
        assert!(engine.mls_recovery_pass().unwrap());
        assert!(engine.device().unwrap().recovery_pending(space));
        assert!(engine
            .device()
            .unwrap()
            .seal(space, b"held external join")
            .is_err());
        let old = engine.store.outbox().unwrap()[0].event.client_id.clone();
        engine.store.conn().execute_batch("CREATE TRIGGER fail_abandon BEFORE DELETE ON outbox BEGIN SELECT RAISE(ABORT, 'injected refusal write failure'); END;").unwrap();
        assert!(!engine.reject(&old, roda_proto::STALE_COMMIT, true, None));
        assert!(engine.device().unwrap().recovery_pending(space));
        assert_eq!(engine.outbox_len(), 1);
        assert_eq!(engine.uploads_pending(), 1);
        engine
            .store
            .conn()
            .execute_batch("DROP TRIGGER fail_abandon")
            .unwrap();
        assert!(!engine.reject(&old, roda_proto::STALE_COMMIT, true, None));
        assert!(!engine.device().unwrap().has_group(space));
        assert!(!engine.device().unwrap().pending(space));
        assert_eq!(engine.outbox_len(), 0);
        assert_eq!(engine.uploads_pending(), 0);
        assert!(engine.mls_recovery_pass().unwrap());
        external = Envelope::plain(&engine.store.outbox().unwrap()[0].event);
        external_reference = RecoveryRef::parse(external.recovery().unwrap()).unwrap();
        assert!(engine.outbox_envelopes().is_empty());
    }
    {
        let mut engine = Engine::open(path.to_str().unwrap()).unwrap();
        engine.unlock(Some(replacement_secret.to_vec())).unwrap();
        engine.unlock_profile(Some(agreement.to_vec())).unwrap();
        assert!(engine.device().unwrap().recovery_pending(space));
        assert!(engine.outbox_envelopes().is_empty());
        engine.upload_done(&external_reference.blob);
        assert_eq!(engine.outbox_envelopes()[0].content(), external.content());
        let confirmation = Sequenced {
            seq: 2,
            prev: previous.clone(),
            hash: chain_hash(space, 2, &previous, &external.wire_hash()),
            env: external,
        };
        for table in ["mls_recovery_pending", "outbox"] {
            engine.store.conn().execute_batch(&format!("CREATE TRIGGER fail_confirmation BEFORE DELETE ON {table} BEGIN SELECT RAISE(ABORT, 'injected confirmation write failure'); END;")).unwrap();
            assert!(matches!(
                engine.ingest(confirmation.clone()),
                Ingest::Invalid(_)
            ));
            assert!(engine.device().unwrap().recovery_pending(space));
            assert_eq!(engine.logs[space].next_seq(), 2);
            assert_eq!(engine.outbox_len(), 1);
            assert!(engine
                .store
                .recovery_upload_confirmed(&external_reference.blob)
                .unwrap());
            assert!(engine
                .device()
                .unwrap()
                .seal(space, b"still held after failed confirmation")
                .is_err());
            engine
                .store
                .conn()
                .execute_batch("DROP TRIGGER fail_confirmation")
                .unwrap();
        }
        assert_eq!(engine.ingest(confirmation), Ingest::Confirmed);
        assert!(!engine.device().unwrap().pending(space));
        assert_eq!(engine.outbox_len(), 0);
        assert!(!engine
            .store
            .recovery_upload_confirmed(&external_reference.blob)
            .unwrap());
        assert!(engine
            .device()
            .unwrap()
            .seal(space, b"confirmed after external restart")
            .is_ok());
    }
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}
