//! Actual authenticated inquiry and real MLS state; observations never admit work.
use super::*;
use crate::owner_policy::{
    OwnerPolicyContextRejection as Rejection, WorkerConnectionState as C,
    WorkerDirectoryState as D, WorkerGroupState as G,
};

fn options_fixture(
    query: &crate::owner_policy::OwnerPolicyInquiryControl,
) -> Result<types::owner_policy_context::OwnerPolicyOptions, Box<dyn std::error::Error>> {
    use chrono::Datelike;
    let now = crate::engine::now_ms();
    let date = chrono::DateTime::from_timestamp_millis(now).ok_or("fixture UTC clock")?;
    Ok(types::owner_policy_context::OwnerPolicyOptions {
        inquiry_digest: query.digest().into(),
        currency: "USD".into(),
        scale: 6,
        reservation_units: 100,
        image_profile_digest: "22".repeat(32),
        observed_at_ms: now,
        year: date.year(),
        month: u8::try_from(date.month())?,
        current_policy: None,
        current_policy_digest: None,
        image_spec: Some(types::owner_policy_context::OwnerPolicyImageSpec {
            size: "1024x1024".into(),
            quality: "low".into(),
        }),
    })
}

#[test]
fn authenticated_worker_snapshot_is_bounded_payload_free_and_read_only() -> TestResult {
    let (mut f, scope, _) = setup()?;
    let pending = f.owner.request_account_policy_options(&scope)?;
    deliver_owner(&mut f)?;
    let query = f
        .core
        .engine
        .owner_policy_inquiry(&scope.space_id, &pending.source)?;
    let sibling = Signer::generate().id();
    f.core.engine.devices_arrived(vec![
        protocol::DeviceRecord {
            device: f.agent_device.id(),
            revoked: false,
        },
        protocol::DeviceRecord {
            device: sibling.clone(),
            revoked: false,
        },
    ])?;
    let before = f.core.image()?;
    let observed = f.core.engine.owner_policy_worker_observation(&query)?;
    assert_eq!(observed.group, G::Ready);
    assert_eq!(observed.directory, D::Complete);
    assert_eq!(observed.self_active, Some(true));
    assert_eq!(observed.active_devices, Some(2));
    assert_eq!(observed.revoked_devices, Some(0));
    assert_eq!(observed.other_active_devices, Some(1));
    assert_eq!(observed.other_active_leaves, Some(0));
    assert_eq!(observed.asked_other_devices, Some(1));
    assert_eq!(observed.own_leaf, Some(true));
    assert!(observed.space_synced && observed.roster_lists_self && observed.dirty);
    assert_eq!(
        observed.device_fingerprint,
        event_log::sha256_hex(f.agent_device.id().as_bytes())
    );
    let wire = serde_json::to_string(&observed)?;
    assert!(wire.len() < 1024);
    for private in [
        &scope.space_id,
        &scope.agent_id,
        &scope.expected_identity_id,
        &pending.source,
        &sibling,
        &f.agent_device.id(),
    ] {
        assert!(!wire.contains(private));
    }
    assert_eq!(f.core.image()?.as_bytes(), before.as_bytes());
    assert_eq!(
        f.core.engine.owner_policy_worker_observation(&query)?,
        observed
    );
    assert_eq!(f.core.image()?.as_bytes(), before.as_bytes());
    assert_eq!(observed.socket, C::Unobserved);
    assert_eq!(observed.socket_synced, None);
    let client = crate::Client::open(":memory:".into(), "en".into())?;
    *client.inner.lock().map_err(|_| "fixture client mutex")? = f.core.engine;
    let offline = client.owner_policy_worker_observation(&query)?;
    assert!(offline.space_synced);
    assert_eq!(offline.socket, C::Offline);
    assert_eq!(offline.socket_synced, Some(false));
    let before = client.lock().store.memory_image()?;
    let poison = client.clone();
    assert!(std::thread::spawn(move || {
        let _guard = poison
            .net
            .lock()
            .expect("controlled healthy fixture socket mutex");
        panic!("controlled socket observation poisoning");
    })
    .join()
    .is_err());
    assert!(matches!(
        client.owner_policy_worker_observation(&query),
        Err(CoreError::Storage { .. })
    ));
    assert_eq!(client.lock().store.memory_image()?, before);
    Ok(())
}

#[test]
fn missing_and_unreadable_worker_groups_are_distinct_but_both_stay_denied() -> TestResult {
    for storage_fault in [false, true] {
        let (mut f, scope, _) = setup()?;
        let pending = f.owner.request_account_policy_options(&scope)?;
        deliver_owner(&mut f)?;
        let query = f
            .core
            .engine
            .owner_policy_inquiry(&scope.space_id, &pending.source)?;
        let options = options_fixture(&query)?;
        // Prove this quote fixture is accepted while the real group is ready.
        // This is Core reporting validation, not a Runtime financial receipt.
        let mut ready = DeviceCore::restore(f.core.image()?, f.credential())?;
        ready
            .engine
            .report_owner_policy_options(&query, options.clone())?;
        if storage_fault {
            f.core
                .engine
                .store
                .conn()
                .execute_batch("ALTER TABLE openmls_group_data RENAME TO unavailable_group_data")?;
        } else {
            f.core.engine.device()?.forget(&scope.space_id)?;
        }
        let before = f.core.image()?;
        let observed = f.core.engine.owner_policy_worker_observation(&query)?;
        assert_eq!(
            observed.group,
            if storage_fault {
                G::Storage
            } else {
                G::Missing
            }
        );
        assert_eq!(observed.own_leaf, None);
        assert_eq!(observed.other_active_leaves, None);
        assert_eq!(
            f.core
                .owner_policy_account_admission(query.signed(), crate::engine::now_ms()),
            Err(if storage_fault {
                Rejection::MlsStorage
            } else {
                Rejection::MlsGroup
            })
        );
        assert!(f
            .core
            .engine
            .owner_policy_inquiry(&scope.space_id, &pending.source)
            .is_err());
        assert!(f
            .core
            .owner_policy_account_context(query.signed(), crate::engine::now_ms())
            .is_err());
        assert!(f.core.engine.refuse_owner_policy_options(&query).is_err());
        assert!(f
            .core
            .engine
            .report_owner_policy_options(&query, options)
            .is_err());
        assert_eq!(f.core.image()?.as_bytes(), before.as_bytes());
        if storage_fault {
            f.core
                .engine
                .store
                .conn()
                .execute_batch("ALTER TABLE unavailable_group_data RENAME TO openmls_group_data")?;
        }
    }
    Ok(())
}

#[test]
fn real_unconfirmed_mls_commit_is_observed_pending_and_never_admits_account_context() -> TestResult
{
    let (mut f, scope, _) = setup()?;
    let pending = f.owner.request_account_policy_options(&scope)?;
    deliver_owner(&mut f)?;
    let query = f
        .core
        .engine
        .owner_policy_inquiry(&scope.space_id, &pending.source)?;
    let key = Signer::generate();
    let author = Author::device(&f.agent_root, key.clone());
    let cert = author.cert.ok_or("sibling certificate")?;
    let credential = UnlockedDevice::verify(
        f.agent_profile.clone(),
        f.owner_profile.clone(),
        cert,
        key.secret(),
    )?;
    let mut sibling = DeviceCore::bootstrap(credential, "http://relay.test")?;
    let (packages, _) = sibling.prepare_initial_key_packages()?;
    let commit = f.core.engine.device()?.commit(
        &scope.space_id,
        packages
            .packages()
            .get(..1)
            .ok_or("one actual package for one device")?,
        &std::collections::BTreeSet::new(),
    )?;
    assert!(!commit.commit.is_empty());
    let before = f.core.image()?;
    let observed = f.core.engine.owner_policy_worker_observation(&query)?;
    assert_eq!(observed.group, G::Pending);
    assert_eq!(
        f.core
            .owner_policy_account_admission(query.signed(), crate::engine::now_ms()),
        Err(Rejection::MlsPending)
    );
    assert!(f
        .core
        .engine
        .owner_policy_inquiry(&scope.space_id, &pending.source)
        .is_err());
    assert_eq!(f.core.image()?.as_bytes(), before.as_bytes());
    assert!(
        f.core.engine.store.outbox()?.is_empty(),
        "diagnostic publishes no handshake or receipt"
    );
    Ok(())
}

#[test]
fn diagnostic_rechecks_expiry_foreign_account_and_directory_corruption_without_defaults(
) -> TestResult {
    let (mut f, scope, _) = setup()?;
    let pending = f.owner.request_account_policy_options(&scope)?;
    deliver_owner(&mut f)?;
    let query = f
        .core
        .engine
        .owner_policy_inquiry(&scope.space_id, &pending.source)?;
    let before = f.core.image()?;
    assert!(f
        .core
        .engine
        .owner_policy_worker_observation_at(
            &query,
            query.signed().inquiry.at_ms + types::owner_policy_context::CONTEXT_TTL_MS + 1
        )
        .is_err());
    assert!(f.owner.owner_policy_worker_observation(&query).is_err());
    let (foreign, _, _) = setup()?;
    assert!(foreign
        .core
        .engine
        .owner_policy_worker_observation(&query)
        .is_err());
    assert_eq!(f.core.image()?.as_bytes(), before.as_bytes());
    // The retained cache is not substituted with an empty successful directory.
    f.core.engine.store.set_meta("devices.active", "not-json")?;
    f.core.engine.store.set_meta("devices.revoked", "[]")?;
    let before = f.core.image()?;
    let observed = f.core.engine.owner_policy_worker_observation(&query)?;
    assert_eq!(observed.directory, D::Invalid);
    assert_eq!(observed.active_devices, None);
    assert_eq!(observed.self_active, None);
    assert_eq!(f.core.image()?.as_bytes(), before.as_bytes());
    Ok(())
}
