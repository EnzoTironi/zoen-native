use super::*;
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn service() -> Service {
    Service {
        id: "01".repeat(32),
        template_sha256: "02".repeat(32),
    }
}

#[test]
fn managed_intent_cold_recovery_preserves_operation_and_refuses_replacement() -> TestResult {
    let dir = std::env::temp_dir().join(types::new_id("zoen-managed-intent")?);
    std::fs::create_dir(&dir)?;
    let path = dir.join("owner.sqlite").to_string_lossy().into_owned();
    let client = Client::open(path.clone(), "en".into())?;
    let (_, key, _) =
        client
            .lock()
            .create_account("Owner", "managed_owner", "https://relay.example")?;
    client.lock().set_registered(true)?;
    let account = client.account().ok_or("account")?;
    let agreement = Zeroizing::new(
        client
            .lock()
            .net
            .profiles
            .agreement
            .as_ref()
            .ok_or("agreement")?
            .secret(),
    );
    let request = Intent::prepare(
        &client.lock(),
        &service(),
        "Helper",
        "managed_helper",
        &account.identity_id,
        &account.device_id,
    )?;
    let stored = client.lock().store.meta(META)?.ok_or("intent")?;
    assert_eq!(
        client.pending_managed_enrollment(&account.identity_id, &account.device_id)?,
        Some(request.clone())
    );
    drop(client);
    let cold = Client::open(path, "en".into())?;
    assert!(cold.lock().unlock(Some(key.to_vec()))?);
    cold.lock().unlock_profile(Some(agreement.to_vec()))?;
    assert_eq!(
        cold.pending_managed_enrollment(&account.identity_id, &account.device_id)?,
        Some(request.clone())
    );
    assert_eq!(
        Intent::prepare(
            &cold.lock(),
            &service(),
            "Helper",
            "managed_helper",
            &account.identity_id,
            &account.device_id
        )?,
        request
    );
    assert!(Intent::prepare(
        &cold.lock(),
        &service(),
        "Changed",
        "managed_helper",
        &account.identity_id,
        &account.device_id
    )
    .is_err());
    let mut other = service();
    other.id = "03".repeat(32);
    assert!(Intent::prepare(
        &cold.lock(),
        &other,
        "Helper",
        "managed_helper",
        &account.identity_id,
        &account.device_id
    )
    .is_err());
    assert!(cold
        .pending_managed_enrollment(&account.identity_id, &"04".repeat(32))
        .is_err());
    assert_eq!(
        cold.lock().store.meta(META)?.as_deref(),
        Some(stored.as_str())
    );
    drop(cold);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn managed_intent_corruption_is_not_an_empty_slot_and_receipts_match_exactly() -> TestResult {
    let client = Client::open(":memory:".into(), "en".into())?;
    client
        .lock()
        .create_account("Owner", "managed_owner", "https://relay.example")?;
    client.lock().set_registered(true)?;
    let account = client.account().ok_or("account")?;
    let request = Intent::prepare(
        &client.lock(),
        &service(),
        "Helper",
        "managed_helper",
        &account.identity_id,
        &account.device_id,
    )?;
    let record = Record {
        request: Some(request.clone()),
        owner: account.identity_id.clone(),
        state: "queued".into(),
        agent: String::new(),
        offer_digest: String::new(),
        sealed_offer: vec![],
        expires_ms: 0,
    };
    assert_eq!(
        managed_record(
            &client.lock(),
            &request,
            Reply::ManagedAgent(record.clone()),
            &account.identity_id,
            &account.device_id
        )?,
        record
    );
    let mut altered = record.clone();
    altered.owner = "05".repeat(32);
    assert!(managed_record(
        &client.lock(),
        &request,
        Reply::ManagedAgent(altered),
        &account.identity_id,
        &account.device_id
    )
    .is_err());
    let mut altered = record;
    altered.request.as_mut().ok_or("request")?.operation = "06".repeat(32);
    assert!(managed_record(
        &client.lock(),
        &request,
        Reply::ManagedAgent(altered),
        &account.identity_id,
        &account.device_id
    )
    .is_err());
    client.lock().store.set_meta(META, "{broken")?;
    assert!(Intent::prepare(
        &client.lock(),
        &service(),
        "Helper",
        "managed_helper",
        &account.identity_id,
        &account.device_id
    )
    .is_err());
    assert_eq!(client.lock().store.meta(META)?.as_deref(), Some("{broken"));
    Ok(())
}

#[test]
fn managed_cancellation_disposition_fences_signing_before_any_network_ack() -> TestResult {
    let client = Client::open(":memory:".into(), "en".into())?;
    client
        .lock()
        .create_account("Owner", "cancel_owner", "https://relay.example")?;
    client.lock().set_registered(true)?;
    let account = client.account().ok_or("account")?;
    let engine = client.lock();
    Intent::prepare(
        &engine,
        &service(),
        "Helper",
        "managed_helper",
        &account.identity_id,
        &account.device_id,
    )?;
    assert!(approval_allowed(&engine, &account.identity_id, &account.device_id).is_ok());
    let mut intent =
        Intent::load(&engine, &account.identity_id, &account.device_id)?.ok_or("intent")?;
    intent.cancel_requested = true;
    engine
        .store
        .set_meta(META, &serde_json::to_string(&intent)?)?;
    assert!(approval_allowed(&engine, &account.identity_id, &account.device_id).is_err());
    assert_eq!(
        Intent::load(&engine, &account.identity_id, &account.device_id)?
            .ok_or("retained")?
            .request,
        intent.request
    );
    Ok(())
}

#[test]
fn managed_host_confirmed_unused_offer_retires_only_its_matching_unknown_receipt() -> TestResult {
    let client = Client::open(":memory:".into(), "en".into())?;
    client
        .lock()
        .create_account("Owner", "cancel_receipt", "https://relay.example")?;
    client.lock().set_registered(true)?;
    let account = client.account().ok_or("account")?;
    let engine = client.lock();
    let request = Intent::prepare(
        &engine,
        &service(),
        "Helper",
        "managed_helper",
        &account.identity_id,
        &account.device_id,
    )?;
    let context = owner_context(&engine, &account.identity_id, &account.device_id)?;
    let receipt = OwnerReceipt {
        owner: account.identity_id.clone(),
        device: account.device_id.clone(),
        certificate_sha256: context.certificate_sha256,
        relay: context.relay,
        status: AgentEnrollmentStatusDto {
            preview: AgentEnrollmentPreviewDto {
                agent_id: "11".repeat(32),
                offer_digest: "12".repeat(32),
                name: "Helper".into(),
                handle: "managed_helper".into(),
                persona: "Help".into(),
                rules: vec![],
                pairing_check: "123456".into(),
                expires_ms: 1,
            },
            delivery: AgentEnrollmentDeliveryDto {
                receipt_id: "receipt".into(),
                agent_id: "11".repeat(32),
                offer_digest: "12".repeat(32),
                outcome: AgentEnrollmentDeliveryOutcome::Unknown,
            },
        },
    };
    engine
        .store
        .set_meta(RECEIPT_META, &serde_json::to_string(&receipt)?)?;
    let mut record = Record {
        request: Some(request),
        owner: account.identity_id.clone(),
        state: "expired".into(),
        agent: "11".repeat(32),
        offer_digest: "12".repeat(32),
        sealed_offer: vec![1; 40],
        expires_ms: 1,
    };
    assert!(
        super::discard::retire(&engine, &record, &account.identity_id, &account.device_id).is_err()
    );
    record.state = "cancelled".into();
    record.offer_digest = "13".repeat(32);
    assert!(
        super::discard::retire(&engine, &record, &account.identity_id, &account.device_id).is_err()
    );
    assert!(engine.store.meta(RECEIPT_META)?.is_some());
    record.offer_digest = "12".repeat(32);
    assert!(super::discard::retire(
        &engine,
        &record,
        &account.identity_id,
        &account.device_id
    )?);
    assert!(engine.store.meta(META)?.is_none());
    assert!(engine.store.meta(RECEIPT_META)?.is_none());
    Ok(())
}
