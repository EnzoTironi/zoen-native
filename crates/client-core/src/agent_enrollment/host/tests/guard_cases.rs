//! guard cases scenarios; existing shared fixture remains in the parent.
use super::*;

#[tokio::test]
pub(super) async fn owner_unknown_is_retained_before_dispatch_and_never_resent_implicitly() {
    let owner = make_owner();
    let vault = DiskVault::new().unwrap();
    let host = vault.client();
    let offer = prepare(&owner, &host, &vault);
    let account = owner.account().unwrap();
    let command = serde_json::json!({"op":"agent_enrollment_preview","code":offer.code,"expected_identity_id":account.identity_id,"expected_device_id":account.device_id});
    let wire: serde_json::Value =
        serde_json::from_str(&owner.client_request(command.to_string()).await.unwrap()).unwrap();
    assert_eq!(wire["offer_digest"], offer.offer_digest);
    assert_eq!(wire["expires_ms"], offer.expires_ms.to_string());
    assert!(owner
        .agent_enrollment_approve(
            &offer.code,
            &offer.offer_digest,
            &account.identity_id,
            &account.device_id,
            "wrong"
        )
        .await
        .is_err());
    assert!(owner
        .agent_enrollment_status(&account.identity_id, &account.device_id)
        .unwrap()
        .is_none());
    let first = owner
        .agent_enrollment_approve(
            &offer.code,
            &offer.offer_digest,
            &account.identity_id,
            &account.device_id,
            &offer.pairing_check,
        )
        .await
        .unwrap();
    assert_eq!(first.outcome, AgentEnrollmentDeliveryOutcome::Unknown);
    assert_eq!(
        owner
            .agent_enrollment_approve(
                &offer.code,
                &offer.offer_digest,
                &account.identity_id,
                &account.device_id,
                &offer.pairing_check
            )
            .await
            .unwrap(),
        first
    );
    let public = owner.lock().store.meta(RECEIPT_META).unwrap().unwrap();
    for secret in ["pairing_secret", "hpke", "owner_proof", "signature", "code"] {
        assert!(!public.contains(secret));
    }
    assert!(!owner
        .agent_enrollment_dismiss(
            "stale",
            &offer.offer_digest,
            &account.identity_id,
            &account.device_id
        )
        .unwrap());
    assert_eq!(
        owner
            .agent_enrollment_status(&account.identity_id, &account.device_id)
            .unwrap()
            .unwrap()
            .delivery,
        first
    );
    assert!(owner
        .agent_enrollment_status(&account.identity_id, &Signer::generate().unwrap().id())
        .is_err());
    assert!(owner
        .agent_enrollment_dismiss(
            &first.receipt_id,
            &offer.offer_digest,
            &account.identity_id,
            &account.device_id
        )
        .unwrap());
    assert!(owner
        .agent_enrollment_status(&account.identity_id, &account.device_id)
        .unwrap()
        .is_none());
}

#[tokio::test]
pub(super) async fn cancel_is_digest_bound_preserves_root_and_never_discards_accepted_consent() {
    let owner = make_owner();
    let vault = DiskVault::new().unwrap();
    let host = vault.client();
    let offer = prepare(&owner, &host, &vault);
    let pending = Pending::load(&vault.vault()).unwrap().unwrap();
    assert!(!host
        .cancel_agent_enrollment(vault.vault(), "0".repeat(64))
        .await
        .unwrap());
    assert!(host
        .cancel_agent_enrollment(vault.vault(), offer.offer_digest)
        .await
        .unwrap());
    assert_eq!(vault.load(ROOT.into()), Some(pending.root.to_vec()));
    let next = prepare(&owner, &host, &vault);
    let second = Pending::load(&vault.vault()).unwrap().unwrap();
    assert_eq!(pending.root, second.root);
    assert_ne!(pending.hpke, second.hpke);
    accept(&owner, &vault);
    assert!(host
        .cancel_agent_enrollment(vault.vault(), next.offer_digest)
        .await
        .is_err());
    assert!(Pending::load(&vault.vault())
        .unwrap()
        .unwrap()
        .accepted
        .is_some());
}

#[test]
pub(super) fn expired_fresh_admission_is_blocked_but_known_registration_recovery_keeps_identity() {
    let owner = make_owner();
    let vault = DiskVault::new().unwrap();
    let host = vault.client();
    prepare(&owner, &host, &vault);
    accept(&owner, &vault);
    host.resume_agent_enrollment(vault.vault()).unwrap();
    let engine = host.lock();
    let value = engine.store.meta(ADMISSION).unwrap().unwrap();
    let mut admission: Admission = serde_json::from_str(&value).unwrap();
    admission.expires_ms = now() - 1;
    engine
        .store
        .set_meta(ADMISSION, &serde_json::to_string(&admission).unwrap())
        .unwrap();
    assert!(engine.agent_enrollment_registration_allowed(false).is_err());
    assert!(engine.agent_enrollment_registration_allowed(true).is_ok());
    admission.device = Signer::generate().unwrap().id();
    engine
        .store
        .set_meta(ADMISSION, &serde_json::to_string(&admission).unwrap())
        .unwrap();
    assert!(engine.agent_enrollment_registration_allowed(true).is_err());
}

#[test]
pub(super) fn unsupported_definitions_low_order_hpke_and_noncanonical_codes_fail_closed() {
    let owner = make_owner();
    let vault = DiskVault::new().unwrap();
    let host = vault.client();
    for (key, value) in [
        ("kind", serde_json::json!("creator")),
        ("model_tier", serde_json::json!("premium")),
        ("state", serde_json::json!({"authority":true})),
    ] {
        let mut setup = request(&owner);
        let mut definition: serde_json::Value =
            serde_json::from_str(&setup.definition_json).unwrap();
        definition[key] = value;
        setup.definition_json = definition.to_string();
        assert!(host.prepare_agent_enrollment(setup, vault.vault()).is_err());
        assert!(vault.load(PENDING.into()).is_none());
    }
    let result = prepare(&owner, &host, &vault);
    let pending = Pending::load(&vault.vault()).unwrap().unwrap();
    let mut code = pending.code.clone();
    code.offer.hpke_public = "0".repeat(64);
    code.agent_signature = Signer::from_secret(&pending.root).sign(
        &agent_owner::enrollment_offer_message(&code.offer.agent_id, &code.offer.digest().unwrap())
            .unwrap(),
    );
    assert!(Code::parse(&code.encode().unwrap(), now()).is_err());
    let bytes = URL_SAFE_NO_PAD
        .decode(result.code.strip_prefix(PREFIX).unwrap())
        .unwrap();
    let text = String::from_utf8(bytes).unwrap();
    for changed in [
        format!(" {text}"),
        text.replacen("{", "{\"unknown\":1,", 1),
        text.replacen("{", "{\"pairing_secret\":\"duplicate\",", 1),
    ] {
        assert!(Code::parse(
            &format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(changed)),
            now()
        )
        .is_err());
    }
}

#[tokio::test]
pub(super) async fn public_owner_receipt_reopens_without_code_and_cannot_cross_account_or_device() {
    let owner_disk = DiskVault::new().unwrap();
    let owner = owner_disk.client();
    let (_, device, _) = owner
        .lock()
        .create_account("Synthetic owner", "fixture_owner", "https://relay.example")
        .unwrap();
    owner.lock().set_registered(true).unwrap();
    let vault = DiskVault::new().unwrap();
    let host = vault.client();
    let offer = prepare(&owner, &host, &vault);
    let account = owner.account().unwrap();
    let first = owner
        .agent_enrollment_approve(
            &offer.code,
            &offer.offer_digest,
            &account.identity_id,
            &account.device_id,
            &offer.pairing_check,
        )
        .await
        .unwrap();
    drop(owner);
    let reopened = owner_disk.client();
    reopened.lock().unlock(Some(device.to_vec())).unwrap();
    let restored = reopened
        .agent_enrollment_status(&account.identity_id, &account.device_id)
        .unwrap()
        .unwrap();
    assert_eq!(restored.delivery, first);
    assert_eq!(restored.preview.name, "Local helper");
    let context =
        owner_context(&reopened.lock(), &account.identity_id, &account.device_id).unwrap();
    let mut receipt: OwnerReceipt =
        serde_json::from_str(&reopened.lock().store.meta(RECEIPT_META).unwrap().unwrap()).unwrap();
    receipt.owner = Signer::generate().unwrap().id();
    assert!(!receipt.matches(&context));
    reopened
        .lock()
        .store
        .set_meta(RECEIPT_META, &serde_json::to_string(&receipt).unwrap())
        .unwrap();
    assert!(reopened
        .agent_enrollment_status(&account.identity_id, &account.device_id)
        .unwrap()
        .is_none());
    assert!(reopened
        .agent_enrollment_dismiss(
            &first.receipt_id,
            &offer.offer_digest,
            &account.identity_id,
            &account.device_id
        )
        .is_err());
}

#[tokio::test]
pub(super) async fn unreadable_or_mismatching_pending_cannot_reset_guarded_identity() {
    let owner = make_owner();
    let vault = DiskVault::new().unwrap();
    let host = vault.client();
    let offer = prepare(&owner, &host, &vault);
    vault.hide_read.store(true, Ordering::SeqCst);
    assert!(host
        .prepare_agent_enrollment(request(&owner), vault.vault())
        .is_err());
    assert!(host.resume_agent_enrollment(vault.vault()).is_err());
    assert!(host
        .cancel_agent_enrollment(vault.vault(), offer.offer_digest.clone())
        .await
        .is_err());
    vault.hide_read.store(false, Ordering::SeqCst);
    assert_eq!(prepare(&owner, &host, &vault).code, offer.code);
    let other_vault = DiskVault::new().unwrap();
    let other_host = other_vault.client();
    prepare(&owner, &other_host, &other_vault);
    let foreign = Zeroizing::new(other_vault.load(PENDING.into()).unwrap());
    let original = Zeroizing::new(vault.load(PENDING.into()).unwrap());
    vault.save(PENDING.into(), foreign.to_vec());
    assert!(host
        .prepare_agent_enrollment(request(&owner), vault.vault())
        .is_err());
    assert!(host.resume_agent_enrollment(vault.vault()).is_err());
    assert!(host
        .cancel_agent_enrollment(vault.vault(), offer.offer_digest.clone())
        .await
        .is_err());
    vault.save(PENDING.into(), original.to_vec());
    assert!(host
        .cancel_agent_enrollment(vault.vault(), offer.offer_digest)
        .await
        .unwrap());
    vault.hide_read.store(true, Ordering::SeqCst);
    assert!(host
        .prepare_agent_enrollment(request(&owner), vault.vault())
        .is_err());
    vault.hide_read.store(false, Ordering::SeqCst);
    prepare(&owner, &host, &vault);
}
