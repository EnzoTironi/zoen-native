//! durability cases scenarios; existing shared fixture remains in the parent.
use super::*;

#[test]
pub(super) fn complete_pending_write_and_readback_precede_any_account_or_key_slot() {
    let owner = make_owner();
    for unreadable in [false, true] {
        let vault = DiskVault::new().unwrap();
        let host = vault.client();
        if unreadable {
            vault.hide_read.store(true, Ordering::SeqCst);
        } else {
            *vault.refuse.lock().unwrap() = Some(PENDING.into());
        }
        assert!(host
            .prepare_agent_enrollment(request(&owner), vault.vault())
            .is_err());
        assert!(host.account().is_none());
        assert!(host.lock().store.meta(PREPARING).unwrap().is_some());
        vault.hide_read.store(false, Ordering::SeqCst);
        assert!(vault.load(VAULT_IDENTITY.into()).is_none());
        assert!(vault.load(VAULT_DEVICE.into()).is_none());
        drop(host);
        let reopened = vault.client();
        if unreadable {
            let held = Pending::load(&vault.vault()).unwrap().unwrap();
            assert_eq!(
                reopened
                    .prepare_agent_enrollment(request(&owner), vault.vault())
                    .unwrap()
                    .offer_digest,
                held.code.offer.digest().unwrap()
            );
        } else {
            // Even after the backend recovers, an unavailable complete record
            // cannot be mistaken for an empty host or silently replaced.
            *vault.refuse.lock().unwrap() = None;
            assert!(reopened
                .prepare_agent_enrollment(request(&owner), vault.vault())
                .is_err());
            assert!(reopened.resume_agent_enrollment(vault.vault()).is_err());
            assert!(vault.load(PENDING.into()).is_none());
        }
    }
}

#[test]
pub(super) fn durable_accepted_replays_same_complete_keys_after_every_slot_write_and_commit() {
    for cut in 0..=4 {
        let owner = make_owner();
        let vault = DiskVault::new().unwrap();
        let host = vault.client();
        prepare(&owner, &host, &vault);
        let pending = accept(&owner, &vault);
        if cut > 0 {
            vault
                .crash_after
                .store(vault.writes.load(Ordering::SeqCst) + cut, Ordering::SeqCst);
            assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || host.resume_agent_enrollment(vault.vault())
            ))
            .is_err());
        } else {
            // The canonical commit can succeed with no subsequent vault cleanup;
            // restart retains consumption and exactly the same key material.
            host.resume_agent_enrollment(vault.vault())
                .unwrap()
                .unwrap();
        }
        drop(host);
        vault.crash_after.store(0, Ordering::SeqCst);
        let reopened = vault.client();
        let progress = reopened
            .resume_agent_enrollment(vault.vault())
            .unwrap()
            .unwrap();
        let account = reopened.account().unwrap();
        assert_eq!(progress.agent_id, pending.code.offer.agent_id);
        assert!(!progress.registered);
        assert_eq!(account.device_id, Signer::from_secret(&pending.device).id());
        for (key, material) in [
            (VAULT_IDENTITY, pending.root),
            (VAULT_DEVICE, pending.device),
            (VAULT_AGREEMENT, pending.agreement),
        ] {
            assert_eq!(vault.load(key.into()), Some(material.to_vec()));
        }
        let key = reopened
            .lock()
            .store
            .profile_key(&progress.agent_id)
            .unwrap()
            .unwrap()
            .key;
        reopened.lock().set_registered(true).unwrap();
        drop(reopened);
        let reopened = vault.client();
        assert!(
            reopened
                .resume_agent_enrollment(vault.vault())
                .unwrap()
                .unwrap()
                .registered
        );
        assert_eq!(
            reopened
                .lock()
                .store
                .profile_key(&progress.agent_id)
                .unwrap()
                .unwrap()
                .key,
            key
        );
        assert_eq!(reopened.lock().identities.len(), 1);
        assert!(Pending::load(&vault.vault())
            .unwrap()
            .unwrap()
            .accepted
            .is_some());
    }
}

#[test]
pub(super) fn sqlite_savepoint_failures_leave_no_partial_agent_and_resume_without_new_keys() {
    for (table, when) in [
        ("meta", "NEW.key = 'account'"),
        ("profile_keys", "1"),
        ("meta", "NEW.key = 'agent.enrollment.admission.v1'"),
    ] {
        let owner = make_owner();
        let vault = DiskVault::new().unwrap();
        let host = vault.client();
        prepare(&owner, &host, &vault);
        let pending = accept(&owner, &vault);
        host.lock().store.conn().execute_batch(&format!("CREATE TRIGGER enrollment_fail BEFORE INSERT ON {table} WHEN {when} BEGIN SELECT RAISE(ABORT, 'synthetic account cut'); END;")).unwrap();
        assert!(host.resume_agent_enrollment(vault.vault()).is_err());
        assert!(host.account().is_none());
        assert!(host.lock().identities.is_empty());
        assert!(host.lock().store.profile_keys().unwrap().is_empty());
        assert!(Pending::load(&vault.vault())
            .unwrap()
            .unwrap()
            .accepted
            .is_some());
        host.lock()
            .store
            .conn()
            .execute_batch("DROP TRIGGER enrollment_fail")
            .unwrap();
        drop(host);
        let reopened = vault.client();
        assert_eq!(
            reopened
                .resume_agent_enrollment(vault.vault())
                .unwrap()
                .unwrap()
                .agent_id,
            pending.code.offer.agent_id
        );
        assert_eq!(
            reopened.account().unwrap().device_id,
            Signer::from_secret(&pending.device).id()
        );
    }
}

#[test]
pub(super) fn conflicts_corruption_and_foreign_person_never_overwrite_slots_or_account() {
    let owner = make_owner();
    let vault = DiskVault::new().unwrap();
    let host = vault.client();
    prepare(&owner, &host, &vault);
    let pending = accept(&owner, &vault);
    let foreign = Signer::generate().unwrap().secret();
    vault.save(VAULT_DEVICE.into(), foreign.to_vec());
    assert!(host.resume_agent_enrollment(vault.vault()).is_err());
    assert_eq!(vault.load(VAULT_DEVICE.into()), Some(foreign.to_vec()));
    assert!(host.account().is_none());
    vault.delete(VAULT_DEVICE.into());
    host.lock()
        .create_account(
            "Existing person",
            "existing_person",
            "https://relay.example",
        )
        .unwrap();
    let person = host.account().unwrap();
    assert!(host.resume_agent_enrollment(vault.vault()).is_err());
    assert_eq!(host.account().unwrap(), person);
    assert!(vault.load(VAULT_IDENTITY.into()).is_none());
    assert_eq!(
        Pending::load(&vault.vault()).unwrap().unwrap().root,
        pending.root
    );
    vault.save(PENDING.into(), b"invalid protected record".to_vec());
    assert!(host.resume_agent_enrollment(vault.vault()).is_err());
    assert_eq!(host.account().unwrap(), person);
}

#[test]
pub(super) fn durable_pending_and_accepted_cuts_resume_without_replacing_own_keys() {
    for accepting in [false, true] {
        let owner = make_owner();
        let vault = DiskVault::new().unwrap();
        let host = vault.client();
        if accepting {
            prepare(&owner, &host, &vault);
        }
        vault
            .crash_after
            .store(vault.writes.load(Ordering::SeqCst) + 1, Ordering::SeqCst);
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if accepting {
                let _ = accept(&owner, &vault);
            } else {
                let _ = host.prepare_agent_enrollment(request(&owner), vault.vault());
            }
        }))
        .is_err());
        assert!(host.account().is_none());
        assert!(vault.load(VAULT_DEVICE.into()).is_none());
        let before = Pending::load(&vault.vault()).unwrap().unwrap();
        drop(host);
        vault.crash_after.store(0, Ordering::SeqCst);
        let reopened = vault.client();
        if accepting {
            let result = reopened
                .resume_agent_enrollment(vault.vault())
                .unwrap()
                .unwrap();
            assert_eq!(result.agent_id, before.code.offer.agent_id);
            assert_eq!(
                reopened.account().unwrap().device_id,
                Signer::from_secret(&before.device).id()
            );
        } else {
            let result = reopened
                .prepare_agent_enrollment(request(&owner), vault.vault())
                .unwrap();
            assert_eq!(result.code, before.code.encode().unwrap());
        }
        let after = Pending::load(&vault.vault()).unwrap().unwrap();
        assert_eq!(before.root, after.root);
        assert_eq!(before.device, after.device);
        assert_eq!(before.agreement, after.agreement);
    }
}

#[test]
pub(super) fn key_write_refusals_preserve_complete_accepted_material_for_exact_retry() {
    for key in [ROOT, VAULT_IDENTITY, VAULT_DEVICE, VAULT_AGREEMENT] {
        let owner = make_owner();
        let vault = DiskVault::new().unwrap();
        let host = vault.client();
        prepare(&owner, &host, &vault);
        let pending = accept(&owner, &vault);
        *vault.refuse.lock().unwrap() = Some(key.into());
        assert!(host.resume_agent_enrollment(vault.vault()).is_err());
        assert!(host.account().is_none());
        let retained = Pending::load(&vault.vault()).unwrap().unwrap();
        assert_eq!(retained.root, pending.root);
        assert_eq!(retained.device, pending.device);
        *vault.refuse.lock().unwrap() = None;
        assert_eq!(
            host.resume_agent_enrollment(vault.vault())
                .unwrap()
                .unwrap()
                .agent_id,
            pending.code.offer.agent_id
        );
    }
}

#[test]
pub(super) fn lost_vault_save_ack_retains_pending_and_accepted_identity_for_readback_recovery() {
    for accepting in [false, true] {
        let owner = make_owner();
        let vault = DiskVault::new().unwrap();
        let host = vault.client();
        if accepting {
            prepare(&owner, &host, &vault);
        }
        vault
            .false_after
            .store(vault.writes.load(Ordering::SeqCst) + 1, Ordering::SeqCst);
        if accepting {
            let mut pending = Pending::load(&vault.vault()).unwrap().unwrap();
            pending.accepted = Some(make_envelope(&owner, &pending));
            assert!(pending.save(&vault.vault()).is_err());
        } else {
            assert!(host
                .prepare_agent_enrollment(request(&owner), vault.vault())
                .is_err());
        }
        let retained = Pending::load(&vault.vault()).unwrap().unwrap();
        assert!(host.account().is_none());
        vault.false_after.store(0, Ordering::SeqCst);
        drop(host);
        let reopened = vault.client();
        if accepting {
            assert_eq!(
                reopened
                    .resume_agent_enrollment(vault.vault())
                    .unwrap()
                    .unwrap()
                    .agent_id,
                retained.code.offer.agent_id
            );
        } else {
            assert_eq!(
                reopened
                    .prepare_agent_enrollment(request(&owner), vault.vault())
                    .unwrap()
                    .code,
                retained.code.encode().unwrap()
            );
        }
    }
}
