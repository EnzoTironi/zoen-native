use super::*;
use protocol::managed_agents::{Request, Service};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn captured(owner: &Client) -> Result<Request, Box<dyn std::error::Error>> {
    let config = request(owner);
    let recipient_public_key = owner
        .lock()
        .net
        .profiles
        .agreement
        .as_ref()
        .ok_or("missing owner agreement")?
        .public_hex();
    Ok(Request {
        operation: "01".repeat(32),
        service: Some(Service {
            id: "02".repeat(32),
            template_sha256: sha(config.definition_json.as_bytes()),
        }),
        name: config.name,
        handle: config.handle,
        recipient_public_key,
    })
}

#[tokio::test]
async fn managed_offer_keeps_pairing_private_and_reuses_owner_receipt() -> TestResult {
    let owner = make_owner();
    let vault = DiskVault::new()?;
    let host = vault.client();
    let captured = captured(&owner)?;
    let account = owner.account().ok_or("missing account")?;
    let offer = prepare(&owner, &host, &vault);
    let pairing = Zeroizing::new(offer.code.clone());
    let record = offer.seal_managed(&account.identity_id, &captured, "https://relay.example")?;
    assert!(!record
        .sealed_offer
        .windows(pairing.len())
        .any(|part| part == pairing.as_bytes()));
    let review = owner.managed_enrollment_preview(
        &captured,
        &record,
        &account.identity_id,
        &account.device_id,
    )?;
    assert_eq!(review.name, captured.name);
    assert_eq!(review.offer_digest, record.offer_digest);
    assert!(owner
        .approve_managed_enrollment(
            &captured,
            &record,
            &"00".repeat(32),
            &account.identity_id,
            &account.device_id,
        )
        .await
        .is_err());
    assert!(owner
        .agent_enrollment_status(&account.identity_id, &account.device_id)?
        .is_none());
    // No relay is attached: Unknown is retained before dispatch, never forged Sent.
    let delivered = owner
        .approve_managed_enrollment(
            &captured,
            &record,
            &review.offer_digest,
            &account.identity_id,
            &account.device_id,
        )
        .await?;
    assert_eq!(delivered.outcome, AgentEnrollmentDeliveryOutcome::Unknown);
    let replay = owner
        .approve_managed_enrollment(
            &captured,
            &record,
            &review.offer_digest,
            &account.identity_id,
            &account.device_id,
        )
        .await?;
    assert_eq!(replay, delivered);
    let public = owner
        .lock()
        .store
        .meta(RECEIPT_META)?
        .ok_or("missing receipt")?;
    assert!(!public.contains(pairing.as_str()));
    assert!(!public.contains("pairing_secret"));
    Ok(())
}

#[test]
fn managed_offer_rejects_substitution_and_tampering() -> TestResult {
    let owner = make_owner();
    let vault = DiskVault::new()?;
    let host = vault.client();
    let captured = captured(&owner)?;
    let account = owner.account().ok_or("missing account")?;
    let record = prepare(&owner, &host, &vault).seal_managed(
        &account.identity_id,
        &captured,
        "https://relay.example",
    )?;
    let refused = |request: &Request, record: &protocol::managed_agents::Record| {
        assert!(owner
            .managed_enrollment_preview(request, record, &account.identity_id, &account.device_id,)
            .is_err());
    };
    for field in 0..6 {
        let mut altered = captured.clone();
        match field {
            0 => altered.operation = "03".repeat(32),
            1 => altered.name = "Another helper".into(),
            2 => altered.handle = "other_helper".into(),
            3 => altered.recipient_public_key = "04".repeat(32),
            4 => altered.service.as_mut().ok_or("service")?.id = "05".repeat(32),
            _ => altered.service.as_mut().ok_or("service")?.template_sha256 = "06".repeat(32),
        }
        let mut rebound = record.clone();
        rebound.request = Some(altered.clone());
        refused(&altered, &record);
        refused(&altered, &rebound);
    }
    for field in 0..6 {
        let mut altered = record.clone();
        match field {
            0 => altered.agent = "07".repeat(32),
            1 => altered.offer_digest = "08".repeat(32),
            2 => altered.expires_ms += 1,
            3 => altered.state = "expired".into(),
            4 => altered.owner = "09".repeat(32),
            _ => *altered.sealed_offer.last_mut().ok_or("ciphertext")? ^= 1,
        }
        refused(&captured, &altered);
    }
    let other = make_owner();
    let other_account = other.account().ok_or("missing other account")?;
    assert!(other
        .managed_enrollment_preview(
            &captured,
            &record,
            &other_account.identity_id,
            &other_account.device_id,
        )
        .is_err());
    let mut wrong_template = captured.clone();
    wrong_template
        .service
        .as_mut()
        .ok_or("service")?
        .template_sha256 = "0a".repeat(32);
    assert!(prepare(&owner, &host, &vault)
        .seal_managed(
            &account.identity_id,
            &wrong_template,
            "https://relay.example"
        )
        .is_err());
    assert!(prepare(&owner, &host, &vault)
        .seal_managed(&account.identity_id, &captured, "https://another.example")
        .is_err());
    Ok(())
}

#[tokio::test]
async fn managed_host_cancel_is_durable_before_ack_and_preserves_the_root() -> TestResult {
    let owner = make_owner();
    let vault = DiskVault::new()?;
    let host = vault.client();
    let captured = captured(&owner)?;
    let account = owner.account().ok_or("owner")?;
    let mut record = prepare(&owner, &host, &vault).seal_managed(
        &account.identity_id,
        &captured,
        "https://relay.example",
    )?;
    record.state = "cancel_requested".into();
    let original = vault.load(PENDING.into()).ok_or("pending")?;
    assert!(
        host.cancel_managed_agent_offer(&record, "https://relay.example", vault.vault())
            .await?
    );
    assert!(vault.load(PENDING.into()).is_none());
    assert!(host.account().is_none());
    let root = vault.load(ROOT.into()).ok_or("retained root")?;
    assert_eq!(
        Signer::from_secret(root.as_slice().try_into()?).id(),
        record.agent
    );
    // Process cut after the cancelled guard, before deleting the old pending file.
    assert!(vault.save(PENDING.into(), original));
    drop(host);
    let cold = vault.client();
    let writes = vault.writes.load(Ordering::SeqCst);
    assert!(
        cold.cancel_managed_agent_offer(&record, "https://relay.example", vault.vault())
            .await?
    );
    assert!(vault.load(PENDING.into()).is_none());
    assert_eq!(vault.writes.load(Ordering::SeqCst), writes);
    assert_eq!(vault.load(ROOT.into()), Some(root));
    let mut changed = record.clone();
    changed.request.as_mut().ok_or("request")?.name = "Replacement".into();
    assert!(cold
        .cancel_managed_agent_offer(&changed, "https://relay.example", vault.vault())
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
async fn managed_host_cancel_does_not_mint_keys_and_never_erases_consumed_consent() -> TestResult {
    let owner = make_owner();
    let vault = DiskVault::new()?;
    let host = vault.client();
    let captured = captured(&owner)?;
    let account = owner.account().ok_or("owner")?;
    let mut record = protocol::managed_agents::Record {
        request: Some(captured.clone()),
        owner: account.identity_id.clone(),
        state: "cancel_requested".into(),
        agent: String::new(),
        offer_digest: String::new(),
        sealed_offer: vec![],
        expires_ms: 0,
        onboarding_completed: false,
    };
    assert!(
        host.cancel_managed_agent_offer(&record, "https://relay.example", vault.vault())
            .await?
    );
    assert_eq!(vault.writes.load(Ordering::SeqCst), 0);
    assert!(host.account().is_none());
    let another = DiskVault::new()?;
    let accepted_host = another.client();
    record = prepare(&owner, &accepted_host, &another).seal_managed(
        &account.identity_id,
        &captured,
        "https://relay.example",
    )?;
    record.state = "cancel_requested".into();
    let accepted = accept(&owner, &another);
    assert!(
        !accepted_host
            .cancel_managed_agent_offer(&record, "https://relay.example", another.vault())
            .await?
    );
    let held = Pending::load(&another.vault())?.ok_or("held consent")?;
    assert_eq!(held.code.offer.digest()?, accepted.code.offer.digest()?);
    assert!(held.accepted.is_some());
    assert!(accepted_host
        .resume_agent_enrollment(another.vault())?
        .is_some());
    assert!(
        !accepted_host
            .cancel_managed_agent_offer(&record, "https://relay.example", another.vault())
            .await?
    );
    Ok(())
}
