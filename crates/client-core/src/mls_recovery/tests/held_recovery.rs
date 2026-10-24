//! Preserved held recovery scenarios; stable test entries stay in the parent.
use super::*;

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

pub(super) fn a_removal_during_held_recovery_survives_restart_and_repairs_before_sealing(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(format!(
        "zoen-removal-recovery-{}.db",
        types::new_ulid(now_ms())?
    ));
    let space = "removed-during-upload";
    let fixture = staged_recovery(path.to_str().unwrap(), space)?;
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
    )?);
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
            protocol::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap()
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

    Ok(())
}

pub(super) fn erase_device_clears_held_markers_atomically_and_fresh_restore_can_recover_the_same_space(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::SecretVault;
    let path = std::env::temp_dir().join(format!(
        "zoen-erased-recovery-{}.db",
        types::new_ulid(now_ms())?
    ));
    let space = "recover-after-erasure";
    let fixture = staged_recovery(path.to_str().unwrap(), space)?;
    drop(fixture.engine);
    let api = crate::Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
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
            &event_log::Signer::from_secret(&fixture.root),
            event_log::Signer::generate()?,
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

    Ok(())
}

pub(super) fn legacy_stub_cannot_install_or_invalidate_authenticated_recovery_context() {
    use event_log::content::{Payload, SignedContent};
    use prost::Message;
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../event-log/testdata/legacy-sealed-v3.json"
    ))
    .unwrap();
    let mut wire = protocol::wire::PbSequenced::decode(
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
