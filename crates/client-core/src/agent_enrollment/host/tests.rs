use super::*;
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
};

/// Actual private filesystem IO, atomic replacement and sync. Faults occur around
/// real writes/readbacks, never by replacing crypto/account validation with success.
struct DiskVault {
    dir: PathBuf,
    writes: AtomicUsize,
    crash_after: AtomicUsize,
    false_after: AtomicUsize,
    refuse: Mutex<Option<String>>,
    hide_read: AtomicBool,
}
impl DiskVault {
    fn new() -> Arc<Self> {
        let dir = std::env::temp_dir().join(types::new_id("zoen-enrollment-test"));
        fs::create_dir(&dir).unwrap();
        Arc::new(Self {
            dir,
            writes: AtomicUsize::new(0),
            crash_after: AtomicUsize::new(0),
            false_after: AtomicUsize::new(0),
            refuse: Mutex::new(None),
            hide_read: AtomicBool::new(false),
        })
    }
    fn vault(self: &Arc<Self>) -> Arc<dyn SecretVault> {
        self.clone()
    }
    fn db(&self) -> String {
        self.dir.join("host.sqlite").to_string_lossy().into()
    }
    fn client(&self) -> Arc<Client> {
        Client::open(self.db(), "en".into()).unwrap()
    }
}
impl Drop for DiskVault {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
impl SecretVault for DiskVault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        if self.hide_read.load(Ordering::SeqCst) {
            return None;
        }
        fs::read(self.dir.join(key)).ok()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        if self.refuse.lock().unwrap().as_deref() == Some(key.as_str()) {
            return false;
        }
        let temp = self.dir.join("private-vault-tmp");
        let result = (|| -> std::io::Result<()> {
            let mut file = fs::File::create(&temp)?;
            file.write_all(&value)?;
            file.sync_all()?;
            fs::rename(&temp, self.dir.join(&key))?;
            fs::File::open(&self.dir)?.sync_all()
        })();
        if result.is_err() {
            return false;
        }
        let write = self.writes.fetch_add(1, Ordering::SeqCst) + 1;
        if self.crash_after.load(Ordering::SeqCst) == write {
            panic!("synthetic process cut after durable vault write");
        }
        self.false_after.load(Ordering::SeqCst) != write
    }
    fn delete(&self, key: String) {
        let _ = fs::remove_file(self.dir.join(key));
    }
}
fn make_owner() -> Arc<Client> {
    let client = Client::open(":memory:".into(), "en".into()).unwrap();
    client
        .lock()
        .create_account("Synthetic owner", "fixture_owner", "https://relay.example")
        .unwrap();
    // Offline fixture only: this does not prove relay registration/admission.
    client.lock().set_registered(true).unwrap();
    client
}
fn request(owner: &Client) -> AgentEnrollmentHostRequest {
    AgentEnrollmentHostRequest { expected_owner_id: owner.account().unwrap().identity_id,
        relay_url: "https://relay.example".into(), name: "Local helper".into(), handle: "local_helper".into(),
        definition_json: r#"{"version":1,"kind":"user","persona":"Help with reviewed tasks.","rules":["Ask before effects."],"state":{},"model_tier":"cheap"}"#.into() }
}
fn prepare(owner: &Client, host: &Client, vault: &Arc<DiskVault>) -> AgentEnrollmentHostOffer {
    host.prepare_agent_enrollment(request(owner), vault.vault())
        .unwrap()
}
fn make_envelope(owner: &Client, pending: &Pending) -> OwnerEnvelope {
    let account = owner.account().unwrap();
    let context = owner_context(&owner.lock(), &account.identity_id, &account.device_id).unwrap();
    let digest = pending.code.offer.digest().unwrap();
    let (proof, signature) =
        agent_owner::authorize_enrollment(&context.author, &pending.code.offer.agent_id, &digest)
            .unwrap();
    OwnerEnvelope {
        schema: 1,
        offer_digest: digest,
        owner_id: account.identity_id,
        agent_id: pending.code.offer.agent_id.clone(),
        owner_proof: proof,
        owner_offer_signature: signature,
    }
}
fn sealed(pending: &Pending, envelope: &OwnerEnvelope) -> Vec<u8> {
    crate::link::seal(
        &hex::decode(&pending.code.offer.hpke_public).unwrap(),
        &canonical_secret(&pending.code.pairing_secret).unwrap(),
        &pending.code.box_context().unwrap(),
        &serde_json::to_vec(envelope).unwrap(),
    )
    .unwrap()
}
fn accept(owner: &Client, vault: &Arc<DiskVault>) -> Pending {
    let mut pending = Pending::load(&vault.vault()).unwrap().unwrap();
    let public = make_envelope(owner, &pending);
    pending.accepted = Some(pending.open(&sealed(&pending, &public)).unwrap());
    pending.save(&vault.vault()).unwrap();
    pending
}

#[test]
fn agent_code_roundtrip_pins_owner_relay_profile_definition_and_rejects_device_link() {
    let owner = make_owner();
    let vault = DiskVault::new();
    let host = vault.client();
    let offer = prepare(&owner, &host, &vault);
    let account = owner.account().unwrap();
    let preview = owner
        .agent_enrollment_preview(&offer.code, &account.identity_id, &account.device_id)
        .unwrap();
    assert_eq!(preview.offer_digest, offer.offer_digest);
    assert_eq!(preview.pairing_check, offer.pairing_check);
    assert_eq!(preview.name, "Local helper");
    assert!(host.account().is_none());
    assert!(vault.load(VAULT_DEVICE.into()).is_none());
    assert!(owner
        .agent_enrollment_status(&account.identity_id, &account.device_id)
        .unwrap()
        .is_none());
    assert_eq!(prepare(&owner, &host, &vault).code, offer.code);
    let parsed = Code::parse(&offer.code, now()).unwrap();
    for field in [
        "agent_id",
        "expected_owner_id",
        "relay_origin",
        "hpke_public",
        "challenge_sha256",
        "issued_ms",
        "expires_ms",
        "name",
        "handle",
        "definition_json",
    ] {
        let mut value = serde_json::to_value(&parsed).unwrap();
        let v = &mut value["offer"][field];
        *v = if v.is_number() {
            serde_json::json!(v.as_u64().unwrap() + 1)
        } else {
            serde_json::json!(format!("{}x", v.as_str().unwrap()))
        };
        let changed = format!(
            "{PREFIX}{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value).unwrap())
        );
        assert!(Code::parse(&changed, now()).is_err(), "{field}");
    }
    assert!(Code::parse(&format!("{PREFIX}{}", "A".repeat(CODE_LIMIT)), now()).is_err());
    assert!(Code::parse("zoen-link:1:ordinary-device-code", now()).is_err());
    assert!(owner
        .agent_enrollment_preview(&offer.code, &Signer::generate().id(), &account.device_id)
        .is_err());
    let other = make_owner();
    let other_account = other.account().unwrap();
    assert!(other
        .agent_enrollment_preview(
            &offer.code,
            &other_account.identity_id,
            &other_account.device_id
        )
        .is_err());
}

#[test]
fn mailbox_envelope_requires_pinned_owner_full_signature_and_agent_context() {
    let owner = make_owner();
    let vault = DiskVault::new();
    let host = vault.client();
    prepare(&owner, &host, &vault);
    let pending = Pending::load(&vault.vault()).unwrap().unwrap();
    let envelope = make_envelope(&owner, &pending);
    assert!(pending.open(&sealed(&pending, &envelope)).is_ok());
    let mut changed = envelope.clone();
    changed.offer_digest = "0".repeat(64);
    assert!(pending.open(&sealed(&pending, &changed)).is_err());
    let other = make_owner();
    let changed = make_envelope(&other, &pending);
    assert!(pending.open(&sealed(&pending, &changed)).is_err());
    let secret = canonical_secret(&pending.code.pairing_secret).unwrap();
    let normal_context = crate::link::seal(
        &hex::decode(&pending.code.offer.hpke_public).unwrap(),
        &secret,
        "identity",
        &serde_json::to_vec(&envelope).unwrap(),
    )
    .unwrap();
    assert!(pending.open(&normal_context).is_err());
    let ordinary_identity = serde_json::json!({"root_secret":"not-a-real-secret","device_cert":"synthetic","profile":{}});
    let bytes = crate::link::seal(
        &hex::decode(&pending.code.offer.hpke_public).unwrap(),
        &secret,
        &pending.code.box_context().unwrap(),
        &serde_json::to_vec(&ordinary_identity).unwrap(),
    )
    .unwrap();
    assert!(pending.open(&bytes).is_err());
    assert!(host.account().is_none());
}

#[test]
fn complete_pending_write_and_readback_precede_any_account_or_key_slot() {
    let owner = make_owner();
    for unreadable in [false, true] {
        let vault = DiskVault::new();
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
fn durable_accepted_replays_same_complete_keys_after_every_slot_write_and_commit() {
    for cut in 0..=4 {
        let owner = make_owner();
        let vault = DiskVault::new();
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
fn sqlite_savepoint_failures_leave_no_partial_agent_and_resume_without_new_keys() {
    for (table, when) in [
        ("meta", "NEW.key = 'account'"),
        ("profile_keys", "1"),
        ("meta", "NEW.key = 'agent.enrollment.admission.v1'"),
    ] {
        let owner = make_owner();
        let vault = DiskVault::new();
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
fn conflicts_corruption_and_foreign_person_never_overwrite_slots_or_account() {
    let owner = make_owner();
    let vault = DiskVault::new();
    let host = vault.client();
    prepare(&owner, &host, &vault);
    let pending = accept(&owner, &vault);
    let foreign = Signer::generate().secret();
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

#[tokio::test]
async fn owner_unknown_is_retained_before_dispatch_and_never_resent_implicitly() {
    let owner = make_owner();
    let vault = DiskVault::new();
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
        .agent_enrollment_status(&account.identity_id, &Signer::generate().id())
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
async fn cancel_is_digest_bound_preserves_root_and_never_discards_accepted_consent() {
    let owner = make_owner();
    let vault = DiskVault::new();
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
fn expired_fresh_admission_is_blocked_but_known_registration_recovery_keeps_identity() {
    let owner = make_owner();
    let vault = DiskVault::new();
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
    admission.device = Signer::generate().id();
    engine
        .store
        .set_meta(ADMISSION, &serde_json::to_string(&admission).unwrap())
        .unwrap();
    assert!(engine.agent_enrollment_registration_allowed(true).is_err());
}

#[test]
fn durable_pending_and_accepted_cuts_resume_without_replacing_own_keys() {
    for accepting in [false, true] {
        let owner = make_owner();
        let vault = DiskVault::new();
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
fn key_write_refusals_preserve_complete_accepted_material_for_exact_retry() {
    for key in [ROOT, VAULT_IDENTITY, VAULT_DEVICE, VAULT_AGREEMENT] {
        let owner = make_owner();
        let vault = DiskVault::new();
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
fn unsupported_definitions_low_order_hpke_and_noncanonical_codes_fail_closed() {
    let owner = make_owner();
    let vault = DiskVault::new();
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
async fn public_owner_receipt_reopens_without_code_and_cannot_cross_account_or_device() {
    let owner_disk = DiskVault::new();
    let owner = owner_disk.client();
    let (_, device, _) = owner
        .lock()
        .create_account("Synthetic owner", "fixture_owner", "https://relay.example")
        .unwrap();
    owner.lock().set_registered(true).unwrap();
    let vault = DiskVault::new();
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
    receipt.owner = Signer::generate().id();
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
async fn unreadable_or_mismatching_pending_cannot_reset_guarded_identity() {
    let owner = make_owner();
    let vault = DiskVault::new();
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
    let other_vault = DiskVault::new();
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

#[test]
fn lost_vault_save_ack_retains_pending_and_accepted_identity_for_readback_recovery() {
    for accepting in [false, true] {
        let owner = make_owner();
        let vault = DiskVault::new();
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn production_agent_wait_uses_own_pickup_auth_installs_verified_box_and_never_refetches_accepted(
) {
    use futures_util::{SinkExt, StreamExt};
    use protocol::{auth_message, ClientFrame, ServerFrame, PROTOCOL_VERSION};
    use tokio_tungstenite::tungstenite::Message;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = format!("http://{}", listener.local_addr().unwrap());
    let owner = Client::open(":memory:".into(), "en".into()).unwrap();
    owner
        .lock()
        .create_account("Synthetic owner", "fixture_owner", &relay)
        .unwrap();
    owner.lock().set_registered(true).unwrap(); // offline fixture, not relay proof
    let vault = DiskVault::new();
    let host = vault.client();
    let mut setup = request(&owner);
    setup.relay_url = relay;
    let offer = host.prepare_agent_enrollment(setup, vault.vault()).unwrap();
    let account = owner.account().unwrap();
    assert!(owner
        .agent_enrollment_preview(&offer.code, &account.identity_id, &account.device_id)
        .is_ok());
    let pending = Pending::load(&vault.vault()).unwrap().unwrap();
    let response = sealed(&pending, &make_envelope(&owner, &pending));
    let expected_pickup = Signer::from_secret(&pending.pickup).id();
    let expected_box = crate::link::box_id(
        &canonical_secret(&pending.code.pairing_secret).unwrap(),
        &pending.code.box_context().unwrap(),
    );
    assert_ne!(expected_pickup, pending.code.offer.agent_id);
    let peer = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let Message::Binary(frame) = socket.next().await.unwrap().unwrap() else {
            panic!("Hello");
        };
        let ClientFrame::Hello {
            identity,
            device,
            cert,
            ..
        } = ClientFrame::decode(&frame).unwrap()
        else {
            panic!("Hello");
        };
        assert_eq!(identity, expected_pickup);
        assert_eq!(device, expected_pickup);
        assert!(event_log::verify_sig(
            &device,
            &event_log::device_cert_message(&device),
            &cert
        ));
        let nonce = "synthetic-enrollment-test";
        let relay_name = "synthetic-enrollment-relay";
        socket
            .send(Message::Binary(
                ServerFrame::Challenge {
                    nonce: nonce.into(),
                    relay: relay_name.into(),
                    protocol: PROTOCOL_VERSION,
                    capabilities: Vec::new(),
                    server_time_ms: None,
                }
                .encode()
                .into(),
            ))
            .await
            .unwrap();
        let Message::Binary(frame) = socket.next().await.unwrap().unwrap() else {
            panic!("Auth");
        };
        let ClientFrame::Auth { sig } = ClientFrame::decode(&frame).unwrap() else {
            panic!("Auth");
        };
        assert!(event_log::verify_sig(
            &device,
            &auth_message(nonce, relay_name),
            &sig
        ));
        socket
            .send(Message::Binary(
                ServerFrame::Ready {
                    identity,
                    registered: false,
                }
                .encode()
                .into(),
            ))
            .await
            .unwrap();
        let Message::Binary(frame) = socket.next().await.unwrap().unwrap() else {
            panic!("FetchLink");
        };
        let ClientFrame::Req {
            id,
            op: Op::FetchLink { id: box_id },
        } = ClientFrame::decode(&frame).unwrap()
        else {
            panic!("only FetchLink allowed");
        };
        assert_eq!(box_id, expected_box);
        socket
            .send(Message::Binary(
                ServerFrame::Res {
                    id,
                    result: Ok(Reply::Link(Some(response))),
                }
                .encode()
                .into(),
            ))
            .await
            .unwrap();
        listener
    });
    let progress = tokio::time::timeout(
        Duration::from_secs(3),
        host.wait_agent_enrollment(vault.vault(), 2000),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(progress.agent_id, pending.code.offer.agent_id);
    assert!(!progress.registered);
    assert_eq!(
        host.account().unwrap().device_id,
        Signer::from_secret(&pending.device).id()
    );
    assert!(Pending::load(&vault.vault())
        .unwrap()
        .unwrap()
        .accepted
        .is_some());
    let listener = tokio::time::timeout(Duration::from_secs(3), peer)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        host.wait_agent_enrollment(vault.vault(), 2000)
            .await
            .unwrap(),
        progress
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
