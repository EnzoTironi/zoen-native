//! Actual authenticated inquiry and real MLS state; observations never admit work.
use super::*;
use crate::owner_policy::{
    OwnerPolicyContextRejection as Rejection, WorkerDirectoryState as D, WorkerGroupState as G,
};

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
    assert!(observed.synced && observed.roster_lists_self && observed.dirty);
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
        packages.packages(),
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
