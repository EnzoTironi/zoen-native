//! Real account signatures and channel retirement; relay replies are controlled.
use super::*;
use crate::link_api::{
    enrollment_owner_tests::{code, wire},
    test_transport::{next_call, Fixture},
};

fn agent(f: &Fixture) -> Result<(), Box<dyn std::error::Error>> {
    let owner = Signer::generate()?;
    let engine = f.client().lock();
    let identity = engine.me_id()?;
    let mut profile = engine
        .store
        .identities()?
        .into_iter()
        .map(|(p, _)| p)
        .find(|p| p.id == identity)
        .ok_or("profile")?;
    profile.kind = types::IdentityKind::Agent;
    profile.owner = Some(owner.id());
    profile.owner_proof = Some(Box::new(
        event_log::agent_owner::authorize(
            &event_log::Author::device(&owner, Signer::generate()?),
            &identity,
        )
        .ok_or("owner proof")?,
    ));
    engine.store.put_identity(&profile, None)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runtime_enrollment_replays_public_certificate_after_unknown_ack_and_cold_restart(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new().await?;
    agent(&f)?;
    f.online().await;
    let (code, _) = code()?;
    let before = f.vault.values.lock().map_err(|_| "vault")?.clone();
    let start = |f: &Fixture| {
        let client = f.client().clone();
        let vault = f.vault.clone();
        let code = code.encode();
        tokio::spawn(async move { client.link_runtime_device(code, vault).await })
    };
    let work = start(&f);
    let call = next_call(&mut f.relay.calls).await;
    let original = wire(&call);
    let Op::DeliverLink {
        id,
        sealed,
        device: Some(certificate),
    } = &call.op
    else {
        return Err("certificate delivery".into());
    };
    assert_eq!(id, &link::box_id(&code.secret, "runtime-device/1"));
    let (domain, identity, device, hpke, cert): (String, String, String, String, String) =
        serde_json::from_slice(sealed)?;
    assert_eq!(domain, "zoen-runtime-device/1");
    assert_eq!(device, code.device);
    assert_eq!(hpke, hex::encode(&code.hpke));
    assert_eq!(certificate.cert, cert);
    assert!(event_log::verify_sig(
        &identity,
        &event_log::device_cert_message(&device),
        &cert
    ));
    assert!(link::open(&[7; 32], &code.secret, "identity", sealed).is_none());
    drop(call.result);
    assert!(work.await?.is_err());
    f.reopen();
    f.online().await;
    let work = start(&f);
    let call = next_call(&mut f.relay.calls).await;
    assert_eq!(wire(&call), original);
    call.result.send(Ok(Reply::Done)).map_err(|_| "reply")?;
    assert_eq!(work.await??.device_id, code.device);
    assert_eq!(*f.vault.values.lock().map_err(|_| "vault")?, before);
    assert!(f.client().lock().net.link.peers.is_empty());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runtime_enrollment_rejects_person_and_wrong_signing_key_without_delivery(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new().await?;
    f.online().await;
    let (code, _) = code()?;
    assert!(f
        .client()
        .link_runtime_device(code.encode(), f.vault.clone())
        .await
        .is_err());
    assert!(f.relay.calls.try_recv().is_err());
    agent(&f)?;
    f.vault
        .values
        .lock()
        .map_err(|_| "vault")?
        .insert(VAULT_IDENTITY.into(), vec![9; 32]);
    assert!(f
        .client()
        .link_runtime_device(code.encode(), f.vault.clone())
        .await
        .is_err());
    assert!(f.relay.calls.try_recv().is_err());
    Ok(())
}
