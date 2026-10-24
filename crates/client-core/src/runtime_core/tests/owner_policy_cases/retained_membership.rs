//! A retained directory discovered before the worker's own Welcome must still
//! enroll its dedicated sibling through the ordinary DeviceJoining/MLS path.
use super::*;

#[test]
fn early_directory_then_real_welcome_reconciles_dedicated_native_sibling_after_reopen() -> TestResult
{
    let mut early = None;
    let mut f = Fixture::new_observing_welcome(SPACE, |core| {
        early = Some(core.image().unwrap());
    });
    let native_key = Signer::generate();
    let native_author = Author::device(&f.agent_root, native_key.clone());
    let native_cert = native_author.cert.clone().ok_or("native certificate")?;
    let native_credential = || {
        UnlockedDevice::verify(
            f.agent_profile.clone(),
            f.owner_profile.clone(),
            native_cert.clone(),
            native_key.secret(),
        )
    };
    let mut native = DeviceCore::bootstrap(native_credential()?, "http://relay.test")?;
    let (packages, retained) = native.prepare_initial_key_packages()?;
    native = DeviceCore::restore(retained, native_credential()?)?;
    let mut worker = DeviceCore::restore(early.ok_or("before Welcome snapshot")?, f.credential())?;
    // This is the already registered normal worker, not the dedicated capsule.
    worker.engine.set_registered(true)?;
    worker.engine.devices_arrived(vec![
        protocol::DeviceRecord {
            device: f.agent_device.id(),
            revoked: false,
        },
        protocol::DeviceRecord {
            device: native_key.id(),
            revoked: false,
        },
    ])?;
    assert!(worker.engine.store.outbox()?.is_empty());
    assert!(worker.engine.ready_epoch(SPACE).is_none());
    // Exact original Commit/Welcome wire, not a reconstructed private group.
    let history: Vec<_> = f
        .owner
        .logs
        .get(SPACE)
        .ok_or("owner log")?
        .events()
        .iter()
        .map(|event| Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: Envelope::plain(event),
        })
        .collect();
    let after = worker.retained_head(SPACE)?.ok_or("early head")?.seq;
    worker.ingest(history.iter().filter(|e| e.seq > after).cloned().collect())?;
    native.ingest(history)?;
    assert!(worker.engine.ready_epoch(SPACE).is_some());
    assert!(
        native.engine.ready_epoch(SPACE).is_none(),
        "old Welcome is not for the new certified leaf"
    );
    worker = DeviceCore::restore(worker.image()?, f.credential())?;
    let clock =
        KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).ok_or("claim clock")?;
    worker.engine.mls_to_claim(Some(clock))?;
    let joins: Vec<_> = worker.engine.outbox_envelopes().into_iter().filter(|env|
        env.plain_event().is_some_and(|event| matches!(event.body, EventBody::DeviceJoining { ref device } if device == &native_key.id()))
    ).collect();
    assert_eq!(
        joins.len(),
        1,
        "normal maintenance must reconsider the already retained sibling after its own Welcome"
    );
    worker.engine.mls_to_claim(Some(clock))?;
    assert_eq!(
        worker.engine.outbox_len(),
        1,
        "one durable request across repeated maintenance"
    );
    worker = DeviceCore::restore(worker.image()?, f.credential())?;
    worker.engine.mls_to_claim(Some(clock))?;
    assert_eq!(
        worker.engine.outbox_len(),
        1,
        "cold retry reuses original DeviceJoining"
    );
    let joining = ordered(&f.owner, joins[0].clone());
    assert_eq!(f.owner.ingest(joining.clone()), Ingest::Applied);
    worker.ingest(vec![joining.clone()])?;
    native.ingest(vec![joining])?;
    let (_, targets, operation) = f
        .owner
        .mls_to_claim(Some(clock))?
        .ok_or("native sibling claim")?;
    assert_eq!(targets, vec![f.agent_profile.id.clone()]);
    f.owner.mls_claimed(
        SPACE,
        &operation,
        Ok(vec![KeyPackageRecord {
            identity: f.agent_profile.id.clone(),
            device: native_key.id(),
            data: packages.packages()[0].clone(),
        }]),
    );
    for (sha, _) in f.owner.pending_uploads(100) {
        f.owner.upload_done(&sha);
    }
    let mut count = 0;
    while f.owner.outbox_len() > 0 {
        let frames = f.owner.outbox_envelopes();
        assert!(!frames.is_empty());
        for envelope in frames {
            let entry = ordered(&f.owner, envelope);
            assert_eq!(f.owner.ingest(entry.clone()), Ingest::Confirmed);
            worker.ingest(vec![entry.clone()])?;
            native.ingest(vec![entry])?;
            count += 1;
            assert!(count < 12, "ordinary handshake did not settle");
        }
    }
    assert!(native.engine.ready_epoch(SPACE).is_some());
    native = DeviceCore::restore(native.image()?, native_credential()?)?;
    assert!(native.engine.ready_epoch(SPACE).is_some());
    // Only a NEW original sent after the valid Welcome can be read by this leaf.
    f.owner.set_registered(true)?;
    let account = f.owner.account().ok_or("owner account")?;
    let scope = OwnerPolicyScope {
        expected_identity_id: account.identity.clone(),
        expected_device_id: account.device.clone(),
        space_id: SPACE.into(),
        agent_id: f.agent_profile.id.clone(),
    };
    let pending = f.owner.request_account_policy_options(&scope)?;
    f.owner.mls_seal_outbox()?;
    for envelope in f.owner.outbox_envelopes() {
        let entry = ordered(&f.owner, envelope);
        assert_eq!(f.owner.ingest(entry.clone()), Ingest::Confirmed);
        worker.ingest(vec![entry.clone()])?;
        native.ingest(vec![entry])?;
    }
    let query = worker.engine.owner_policy_inquiry(SPACE, &pending.source)?;
    native.owner_policy_account_context(query.signed(), crate::engine::now_ms())?;
    assert!(f.owner.state.grants.is_empty());
    assert!(native.engine.state.requests.is_empty());
    assert!(native.engine.store.outbox()?.is_empty());
    Ok(())
}

#[test]
fn sibling_join_suppression_and_intent_rollback_together_and_revocation_stays_excluded(
) -> TestResult {
    let mut f = Fixture::new();
    let sibling = Signer::generate().id();
    let records = |revoked| {
        vec![protocol::DeviceRecord {
            device: sibling.clone(),
            revoked,
        }]
    };
    let asked = format!("link.asked:{SPACE}:{sibling}");
    assert!(f.core.engine.store.outbox()?.is_empty());
    f.core.engine.store.conn().execute_batch(
        "CREATE TEMP TRIGGER fail_sibling_marker BEFORE INSERT ON meta
         WHEN NEW.key LIKE 'link.asked:%'
         BEGIN SELECT RAISE(ABORT, 'controlled sibling marker failure'); END;",
    )?;
    assert!(f.core.engine.devices_arrived(records(false)).is_err());
    assert!(f.core.engine.store.outbox()?.is_empty());
    assert!(f.core.engine.store.meta(&asked)?.is_none());
    assert!(f.core.engine.store.meta("devices.active")?.is_none());
    assert!(f.core.engine.ready_epoch(SPACE).is_some());
    f.core
        .engine
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_sibling_marker")?;
    f.core.engine.devices_arrived(records(true))?;
    f.core
        .engine
        .mls_to_claim(KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()))?;
    assert!(
        f.core.engine.store.outbox()?.is_empty(),
        "revoked sibling cannot be requested"
    );
    f.core.engine.devices_arrived(records(false))?;
    assert_eq!(f.core.engine.outbox_len(), 1);
    assert_eq!(f.core.engine.store.meta(&asked)?.as_deref(), Some("1"));
    f.core = DeviceCore::restore(f.core.image()?, f.credential())?;
    f.core.engine.mls_siblings()?;
    assert_eq!(
        f.core.engine.outbox_len(),
        1,
        "cold retry retains one exact intent"
    );
    Ok(())
}
