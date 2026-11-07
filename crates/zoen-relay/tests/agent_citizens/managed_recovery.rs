use super::*;
use protocol::managed_agents::{Request, Service};
use zoen_relay::agents::provision::{self, host, recovery};
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn managed_recovery_pages_linked_devices_and_monotonic_completion_without_changing_citizens(
) -> TestResult {
    let d = Database::new().await?;
    let root = Signer::generate()?;
    let first = Author::device(&root, Signer::generate()?);
    let second = Author::device(&root, Signer::generate()?);
    let person = Identity {
        owner_proof: None,
        id: root.id(),
        kind: IdentityKind::Person,
        name: "Recovery owner".into(),
        handle: "recovery_owner".into(),
        tint_hex: "#123456".into(),
        glyph: None,
        owner: None,
        bio: String::new(),
    };
    d.register(&person, &first).await;
    let mut admission = db::authorize_device(&d.pool, &person.id, dev(&first), true)
        .await?
        .ok_or("sponsor admission")?;
    db::enroll_device(
        &mut admission,
        &person.id,
        dev(&second),
        second.cert.as_deref().ok_or("certificate")?,
    )
    .await?;
    admission.commit().await?;
    let (other, foreign) = d.person("recovery_foreign").await?;
    let service = Service {
        id: "d1".repeat(32),
        template_sha256: "d2".repeat(32),
    };
    provision::heartbeat(&d.pool, &service).await?;
    let mut held = Vec::new();
    for n in 1..=34 {
        let request = Request {
            operation: format!("{n:064x}"),
            service: Some(service.clone()),
            name: format!("Agent {n}"),
            handle: format!("recovered_agent_{n}"),
            recipient_public_key: "b1".repeat(32),
        };
        provision::prepare(&d.pool, &person.id, dev(&first), &request).await?;
        assert!(recovery::complete(
            &d.pool,
            &person.id,
            dev(&second),
            &request,
            &"a1".repeat(32)
        )
        .await
        .is_err());
        let root = Signer::generate()?;
        let agent = Author::device(&root, Signer::generate()?);
        let id = root.id();
        let (now,): (i64,) =
            sqlx::query_as("SELECT (extract(epoch FROM clock_timestamp())*1000)::bigint")
                .fetch_one(&d.pool)
                .await?;
        host::retain_offer(
            &d.pool,
            &person.id,
            &request,
            host::Offer {
                agent: &id,
                digest: &"a2".repeat(32),
                sealed: &[7; 128],
                expires_ms: u64::try_from(now)? + 60000,
            },
        )
        .await?;
        d.register(
            &Identity {
                owner_proof: Some(Box::new(
                    agent_owner::authorize(&first, &id).ok_or("proof")?,
                )),
                id: id.clone(),
                kind: IdentityKind::Agent,
                name: request.name.clone(),
                handle: request.handle.clone(),
                tint_hex: "#123456".into(),
                glyph: None,
                owner: Some(person.id.clone()),
                bio: String::new(),
            },
            &agent,
        )
        .await;
        held.push(host::registered(&d.pool, &person.id, &request, &id, &"a2".repeat(32)).await?);
    }
    // Discovery needs no current service heartbeat and never crosses owners.
    sqlx::query(
        "UPDATE managed_agent_services SET heartbeat_at=clock_timestamp()-interval '1 minute'",
    )
    .execute(&d.pool)
    .await?;
    assert!(recovery::discover(&d.pool, &person.id, dev(&foreign), "")
        .await
        .is_err());
    assert!(recovery::discover(&d.pool, &other.id, dev(&foreign), "")
        .await?
        .records
        .is_empty());
    let page = recovery::discover(&d.pool, &person.id, dev(&second), "").await?;
    assert!(page.valid());
    assert_eq!(page.records, held[..32]);
    let tail = recovery::discover(&d.pool, &person.id, dev(&second), &page.next_operation).await?;
    assert_eq!(tail.records, held[32..]);
    assert!(tail.next_operation.is_empty());
    let original = &held[0];
    let request = original.request.as_ref().ok_or("request")?;
    assert_eq!(
        recovery::recover(&d.pool, &person.id, dev(&second), &request.operation).await?,
        *original
    );
    assert!(
        recovery::recover(&d.pool, &other.id, dev(&foreign), &request.operation)
            .await
            .is_err()
    );
    let mut changed = request.clone();
    changed.name.push('!');
    assert!(
        recovery::complete(&d.pool, &person.id, dev(&second), &changed, &original.agent)
            .await
            .is_err()
    );
    assert!(
        recovery::complete(&d.pool, &person.id, dev(&second), request, &held[1].agent)
            .await
            .is_err()
    );
    let mut completed = original.clone();
    completed.onboarding_completed = true;
    for device in [&second, &first, &second] {
        assert_eq!(
            recovery::complete(&d.pool, &person.id, dev(device), request, &original.agent).await?,
            completed
        );
    }
    assert!(
        recovery::recover(&d.pool, &person.id, dev(&second), &request.operation)
            .await
            .is_err()
    );
    let remaining = recovery::discover(&d.pool, &person.id, dev(&first), "").await?;
    assert!(remaining.valid());
    assert_eq!(remaining.records.len(), 32);
    assert!(remaining.records.iter().all(|record| record
        .request
        .as_ref()
        .is_some_and(|held| held.operation != request.operation)));
    assert!(sqlx::query("UPDATE managed_agent_requests SET onboarding_completed=false WHERE owner=$1 AND operation=$2").bind(&person.id).bind(&request.operation).execute(&d.pool).await.is_err());
    assert_eq!(host::page(&d.pool, &service, None).await?.len(), 32);
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM agent_citizens WHERE owner=$1 AND revoked_at IS NULL")
            .bind(&person.id)
            .fetch_one(&d.pool)
            .await?;
    assert_eq!(count, 34);
    sqlx::query("UPDATE devices SET revoked_at=now() WHERE device=$1")
        .bind(dev(&second))
        .execute(&d.pool)
        .await?;
    assert!(recovery::discover(&d.pool, &person.id, dev(&second), "")
        .await
        .is_err());
    assert!(
        recovery::complete(&d.pool, &person.id, dev(&second), request, &original.agent)
            .await
            .is_err()
    );
    d.finish().await;
    Ok(())
}
