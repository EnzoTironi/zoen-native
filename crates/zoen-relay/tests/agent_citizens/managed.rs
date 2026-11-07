//! Queue acceptance must not become ownership, registration or spending consent.
use super::*;
use protocol::managed_agents::{Request, Service};
use zoen_relay::agents::provision::{self, host, Error};
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn new_request(service: &Service, operation: u64) -> Request {
    Request {
        operation: format!("{operation:064x}"),
        service: Some(service.clone()),
        name: "My assistant".into(),
        handle: "my_assistant".into(),
        recipient_public_key: "b1".repeat(32),
    }
}
fn service() -> Service {
    Service {
        id: "a1".repeat(32),
        template_sha256: "c1".repeat(32),
    }
}

#[tokio::test]
async fn owner_bound_replay_retains_request_even_when_service_is_down() -> Result {
    let d = Database::new().await?;
    let (owner, device) = d.person("managed_owner").await?;
    let (other, other_device) = d.person("managed_other").await?;
    let service = service();
    assert!(provision::availability(&d.pool).await?.service.is_none());
    let request = new_request(&service, 1);
    assert!(matches!(
        provision::prepare(&d.pool, &owner.id, dev(&device), &request).await,
        Err(Error::Unavailable)
    ));
    provision::heartbeat(&d.pool, &service).await?;
    assert_eq!(
        provision::availability(&d.pool).await?.service,
        Some(service.clone())
    );
    assert!(
        provision::prepare(&d.pool, &owner.id, dev(&other_device), &request)
            .await
            .is_err()
    );
    let (a, b) = tokio::join!(
        provision::prepare(&d.pool, &owner.id, dev(&device), &request),
        provision::prepare(&d.pool, &owner.id, dev(&device), &request)
    );
    let retained = a?;
    assert_eq!(retained, b?);
    assert_eq!(retained.state, "queued");
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM managed_agent_requests")
        .fetch_one(&d.pool)
        .await?;
    assert_eq!(count, 1);
    assert!(provision::status(
        &d.pool,
        &other.id,
        dev(&other_device),
        &service.id,
        &request.operation
    )
    .await
    .is_err());
    for field in ["name", "handle", "recipient", "template", "service"] {
        let mut changed = request.clone();
        match field {
            "name" => changed.name = "Changed".into(),
            "handle" => changed.handle = "other_assistant".into(),
            "recipient" => changed.recipient_public_key = "c2".repeat(32),
            "template" => {
                changed.service.as_mut().ok_or("service")?.template_sha256 = "d2".repeat(32)
            }
            _ => changed.service.as_mut().ok_or("service")?.id = "e2".repeat(32),
        }
        assert!(
            matches!(
                provision::prepare(&d.pool, &owner.id, dev(&device), &changed).await,
                Err(Error::Changed)
            ),
            "{field}"
        );
    }
    // Only scheduling telemetry changes. No productive clock/permission is changed.
    sqlx::query(
        "UPDATE managed_agent_services SET heartbeat_at=clock_timestamp()-interval '1 minute'",
    )
    .execute(&d.pool)
    .await?;
    assert!(provision::availability(&d.pool).await?.service.is_none());
    assert_eq!(
        provision::prepare(&d.pool, &owner.id, dev(&device), &request).await?,
        retained
    );
    assert_eq!(
        provision::status(
            &d.pool,
            &owner.id,
            dev(&device),
            &service.id,
            &request.operation
        )
        .await?,
        retained
    );
    assert!(
        provision::prepare(&d.pool, &owner.id, dev(&device), &new_request(&service, 2))
            .await
            .is_err()
    );
    d.finish().await;
    Ok(())
}

#[tokio::test]
async fn retained_offer_is_immutable_and_requires_actual_registered_identity() -> Result {
    let d = Database::new().await?;
    let (owner, device) = d.person("offer_owner").await?;
    let service = service();
    provision::heartbeat(&d.pool, &service).await?;
    let request = new_request(&service, 1);
    provision::prepare(&d.pool, &owner.id, dev(&device), &request).await?;
    let root = Signer::generate()?;
    let agent = Author::device(&root, Signer::generate()?);
    let id = root.id();
    let hash = "d1".repeat(32);
    let (now,): (i64,) =
        sqlx::query_as("SELECT (extract(epoch FROM clock_timestamp())*1000)::bigint")
            .fetch_one(&d.pool)
            .await?;
    let expires = u64::try_from(now)? + 60_000;
    let sealed = vec![71; 256];
    let make = |bytes| host::Offer {
        agent: &id,
        digest: &hash,
        sealed: bytes,
        expires_ms: expires,
    };
    let retained = host::retain_offer(&d.pool, &owner.id, &request, make(&sealed)).await?;
    assert!(retained.valid());
    let different_ciphertext = vec![72; 256];
    assert_eq!(
        host::retain_offer(&d.pool, &owner.id, &request, make(&different_ciphertext)).await?,
        retained
    );
    assert!(host::registered(&d.pool, &owner.id, &request, &id, &hash)
        .await
        .is_err());
    assert!(
        sqlx::query("UPDATE managed_agent_requests SET name='replacement' WHERE owner=$1")
            .bind(&owner.id)
            .execute(&d.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE managed_agent_requests SET sealed_offer=$2 WHERE owner=$1")
            .bind(&owner.id)
            .bind(&different_ciphertext)
            .execute(&d.pool)
            .await
            .is_err()
    );
    let profile = Identity {
        owner_proof: Some(Box::new(
            agent_owner::authorize(&device, &id).ok_or("proof")?,
        )),
        id: id.clone(),
        kind: IdentityKind::Agent,
        name: request.name.clone(),
        handle: request.handle.clone(),
        tint_hex: "#123456".into(),
        glyph: None,
        owner: Some(owner.id.clone()),
        bio: String::new(),
    };
    d.register(&profile, &agent).await;
    let cancellation = provision::cancel(&d.pool, &owner.id, dev(&device), &request).await?;
    assert_eq!(cancellation.state, "cancel_requested");
    assert!(provision::cancel::confirmed(&d.pool, &owner.id, &request)
        .await
        .is_err());
    let registered = host::registered(&d.pool, &owner.id, &request, &id, &hash).await?;
    assert_eq!(registered.state, "registered");
    assert_eq!(registered.sealed_offer, sealed);
    assert_eq!(
        host::registered(&d.pool, &owner.id, &request, &id, &hash).await?,
        registered
    );
    assert!(
        sqlx::query("UPDATE managed_agent_requests SET state='offered' WHERE owner=$1")
            .bind(&owner.id)
            .execute(&d.pool)
            .await
            .is_err()
    );
    let page = host::page(&d.pool, &service, None).await?;
    assert_eq!(page, vec![registered]);
    assert!(
        host::page(&d.pool, &service, Some((&owner.id, &request.operation)))
            .await?
            .is_empty()
    );
    d.finish().await;
    Ok(())
}

#[tokio::test]
async fn capacity_never_replaces_pending_work_or_blocks_exact_recovery() -> Result {
    let d = Database::new().await?;
    let (owner, device) = d.person("capacity_owner").await?;
    let service = service();
    provision::heartbeat(&d.pool, &service).await?;
    for i in 0..16 {
        provision::prepare(&d.pool, &owner.id, dev(&device), &new_request(&service, i)).await?;
    }
    assert!(matches!(
        provision::prepare(&d.pool, &owner.id, dev(&device), &new_request(&service, 16)).await,
        Err(Error::Capacity)
    ));
    assert_eq!(
        provision::prepare(&d.pool, &owner.id, dev(&device), &new_request(&service, 0))
            .await?
            .state,
        "queued"
    );
    let mut changed = service.clone();
    changed.template_sha256 = "e1".repeat(32);
    assert!(matches!(
        provision::heartbeat(&d.pool, &changed).await,
        Err(Error::Changed)
    ));
    d.finish().await;
    Ok(())
}

#[tokio::test]
async fn managed_cancel_requires_owner_device_and_host_ack_before_releasing_capacity() -> Result {
    let d = Database::new().await?;
    let (owner, device) = d.person("cancel_owner").await?;
    let (_, other_device) = d.person("cancel_other").await?;
    let service = service();
    provision::heartbeat(&d.pool, &service).await?;
    for i in 0..16 {
        provision::prepare(&d.pool, &owner.id, dev(&device), &new_request(&service, i)).await?;
    }
    let request = new_request(&service, 0);
    assert!(
        provision::cancel(&d.pool, &owner.id, dev(&other_device), &request)
            .await
            .is_err()
    );
    let mut changed = request.clone();
    changed.name = "Changed".into();
    assert!(
        provision::cancel(&d.pool, &owner.id, dev(&device), &changed)
            .await
            .is_err()
    );
    let pending = provision::cancel(&d.pool, &owner.id, dev(&device), &request).await?;
    assert_eq!(pending.state, "cancel_requested");
    assert!(pending.agent.is_empty());
    assert_eq!(
        provision::prepare(&d.pool, &owner.id, dev(&device), &request).await?,
        pending
    );
    assert!(matches!(
        provision::prepare(&d.pool, &owner.id, dev(&device), &new_request(&service, 17)).await,
        Err(Error::Capacity)
    ));
    let cancelled = provision::cancel::confirmed(&d.pool, &owner.id, &request).await?;
    assert_eq!(cancelled.state, "cancelled");
    assert_eq!(
        provision::cancel(&d.pool, &owner.id, dev(&device), &request).await?,
        cancelled
    );
    assert_eq!(
        provision::cancel::confirmed(&d.pool, &owner.id, &request).await?,
        cancelled
    );
    assert!(sqlx::query(
        "UPDATE managed_agent_requests SET state='queued' WHERE owner=$1 AND operation=$2"
    )
    .bind(&owner.id)
    .bind(&request.operation)
    .execute(&d.pool)
    .await
    .is_err());
    provision::prepare(&d.pool, &owner.id, dev(&device), &new_request(&service, 17)).await?;
    d.finish().await;
    Ok(())
}

#[tokio::test]
async fn managed_cancel_serializes_publication_without_substituting_the_original_offer() -> Result {
    let d = Database::new().await?;
    let (owner, device) = d.person("cancel_publish").await?;
    let service = service();
    provision::heartbeat(&d.pool, &service).await?;
    for i in 0..8 {
        let request = new_request(&service, i);
        provision::prepare(&d.pool, &owner.id, dev(&device), &request).await?;
        let (now,): (i64,) =
            sqlx::query_as("SELECT (extract(epoch FROM clock_timestamp())*1000)::bigint")
                .fetch_one(&d.pool)
                .await?;
        let agent = "d2".repeat(32);
        let digest = "d3".repeat(32);
        let sealed = vec![71; 80];
        let (cancel, offer) = tokio::join!(
            provision::cancel(&d.pool, &owner.id, dev(&device), &request),
            host::retain_offer(
                &d.pool,
                &owner.id,
                &request,
                host::Offer {
                    agent: &agent,
                    digest: &digest,
                    sealed: &sealed,
                    expires_ms: u64::try_from(now)? + 60000
                }
            )
        );
        let cancel = cancel?;
        assert_eq!(cancel.state, "cancel_requested");
        if offer.is_ok() {
            assert_eq!(cancel.agent, agent);
            assert_eq!(cancel.sealed_offer, sealed);
        } else {
            assert!(cancel.agent.is_empty());
        }
        assert!(
            host::retain_offer(
                &d.pool,
                &owner.id,
                &request,
                host::Offer {
                    agent: &agent,
                    digest: &digest,
                    sealed: &sealed,
                    expires_ms: u64::try_from(now)? + 60000
                }
            )
            .await
            .is_err()
                || !cancel.agent.is_empty()
        );
        provision::cancel::confirmed(&d.pool, &owner.id, &request).await?;
    }
    d.finish().await;
    Ok(())
}
