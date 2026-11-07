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
