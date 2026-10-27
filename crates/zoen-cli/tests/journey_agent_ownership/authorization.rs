#[tokio::test]
async fn owner_authorization_survives_profile_updates_but_cannot_transfer_ownership(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let w = World::new("agent_owner_valid").await;
    let owner = RawClient::connect(&w.relay_url(), "owner").await?;
    let other = RawClient::connect(&w.relay_url(), "other").await?;
    let author = Author::device(&Signer::generate()?, Signer::generate()?);
    let mut agent = RawClient::reconnect(&w.relay_url(), author).await;
    let original = authorized_profile(&owner.author, &agent.author, "approved_agent");
    assert_eq!(
        agent
            .request(Op::Register {
                profile: original.clone()
            })
            .await,
        Ok(Reply::Registered(original.clone()))
    );
    assert_eq!(
        lookup(&mut agent, "approved_agent").await,
        vec![original.clone()]
    );
    let mut updated = original.clone();
    updated.name = "Updated name".into();
    updated.bio = "Updated bio".into();
    updated.handle = "updated_agent".into();
    agent
        .request(Op::Register {
            profile: updated.clone(),
        })
        .await
        .unwrap();
    let transferred = authorized_profile(&other.author, &agent.author, "transferred_agent");
    assert!(agent
        .request(Op::Register {
            profile: transferred
        })
        .await
        .is_err());
    assert_eq!(lookup(&mut agent, "updated_agent").await, vec![updated]);
    assert!(lookup(&mut agent, "transferred_agent").await.is_empty());
    assert_eq!(w.count("SELECT count(*) FROM identities").await, 3);
    assert_eq!(w.count("SELECT count(*) FROM devices").await, 3);
    Ok(())
}

#[tokio::test]
async fn ownership_proof_cannot_be_replayed_for_another_agent_or_owner(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let w = World::new("agent_owner_binding").await;
    let owner = RawClient::connect(&w.relay_url(), "owner").await?;
    let other = RawClient::connect(&w.relay_url(), "other").await?;
    let author = Author::device(&Signer::generate()?, Signer::generate()?);
    let original = authorized_profile(&owner.author, &author, "first_agent");
    let mut first = RawClient::reconnect(&w.relay_url(), author).await;
    first
        .request(Op::Register {
            profile: original.clone(),
        })
        .await
        .unwrap();
    let second_author = Author::device(&Signer::generate()?, Signer::generate()?);
    let mut second = RawClient::reconnect(&w.relay_url(), second_author).await;
    let valid = authorized_profile(&owner.author, &second.author, "second_agent");
    let mut wrong_agent = valid.clone();
    wrong_agent.owner_proof = original.owner_proof;
    let mut wrong_owner = valid.clone();
    wrong_owner.owner = Some(other.identity());
    let mut wrong_signature = valid.clone();
    wrong_signature.owner_proof.as_mut().unwrap().signature = "00".repeat(64);
    let mut wrong_certificate = valid.clone();
    wrong_certificate.owner_proof.as_mut().unwrap().cert = "00".repeat(64);
    let mut wrong_device = valid.clone();
    wrong_device.owner_proof.as_mut().unwrap().device = other.author.device.clone().unwrap();
    for invalid in [
        wrong_agent,
        wrong_owner,
        wrong_signature,
        wrong_certificate,
        wrong_device,
    ] {
        assert!(second
            .request(Op::Register { profile: invalid })
            .await
            .is_err());
        assert!(lookup(&mut first, "second_agent").await.is_empty());
        assert_eq!(w.count("SELECT count(*) FROM identities").await, 3);
        assert_eq!(w.count("SELECT count(*) FROM devices").await, 3);
    }
    second
        .request(Op::Register {
            profile: valid.clone(),
        })
        .await
        .unwrap();
    assert_eq!(lookup(&mut second, "second_agent").await, vec![valid]);
    Ok(())
}

#[tokio::test]
async fn unregistered_and_unenrolled_owner_devices_cannot_authorize_enrollment(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let w = World::new("agent_owner_enrollment").await;
    let root = Signer::generate()?;
    let owner_author = Author::device(&root, Signer::generate()?);
    let author = Author::device(&Signer::generate()?, Signer::generate()?);
    let mut agent = RawClient::reconnect(&w.relay_url(), author).await;
    let valid = authorized_profile(&owner_author, &agent.author, "approved_agent");
    assert!(agent
        .request(Op::Register {
            profile: valid.clone()
        })
        .await
        .is_err());
    assert_eq!(w.count("SELECT count(*) FROM identities").await, 0);
    assert_eq!(w.count("SELECT count(*) FROM devices").await, 0);
    let mut owner = RawClient::reconnect(&w.relay_url(), owner_author).await;
    owner
        .request(Op::Register {
            profile: profile(&owner.author, "owner", None),
        })
        .await
        .unwrap();
    let not_enrolled = Author::device(&root, Signer::generate()?);
    let invalid = authorized_profile(&not_enrolled, &agent.author, "approved_agent");
    assert!(agent
        .request(Op::Register { profile: invalid })
        .await
        .is_err());
    assert_eq!(w.count("SELECT count(*) FROM identities").await, 1);
    assert_eq!(w.count("SELECT count(*) FROM devices").await, 1);
    agent
        .request(Op::Register { profile: valid })
        .await
        .unwrap();
    Ok(())
}

#[tokio::test]
async fn a_revoked_owner_device_cannot_enroll_but_existing_ownership_remains_stable(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let w = World::new("agent_owner_revoked").await;
    let (root, mut owner) = owner_with_root(&w, "owner").await?;
    let first_author = Author::device(&Signer::generate()?, Signer::generate()?);
    let mut first = RawClient::reconnect(&w.relay_url(), first_author).await;
    let original = authorized_profile(&owner.author, &first.author, "first_agent");
    first
        .request(Op::Register {
            profile: original.clone(),
        })
        .await
        .unwrap();
    let controller_author = Author::device(&root, Signer::generate()?);
    owner
        .request(Op::DeliverLink {
            id: controller_author.device.clone().unwrap(),
            sealed: vec![1],
            device: Some(DeviceCertificate {
                device: controller_author.device.clone().unwrap(),
                cert: controller_author.cert.clone().unwrap(),
            }),
        })
        .await
        .unwrap();
    let mut controller = RawClient::reconnect(&w.relay_url(), controller_author).await;
    controller
        .request(Op::Unlink {
            device: owner.author.device.clone().unwrap(),
        })
        .await
        .unwrap();
    let mut changed = original.clone();
    changed.name = "Still owned".into();
    first
        .request(Op::Register {
            profile: changed.clone(),
        })
        .await
        .unwrap();
    assert_eq!(
        lookup(&mut first, "first_agent").await,
        vec![changed.clone()]
    );
    let mut replaced_proof = changed.clone();
    replaced_proof.owner_proof = Some(Box::new(
        agent_owner::authorize(&controller.author, &first.identity()).unwrap(),
    ));
    assert!(first
        .request(Op::Register {
            profile: replaced_proof
        })
        .await
        .is_err());
    assert_eq!(lookup(&mut first, "first_agent").await, vec![changed]);
    let new_author = Author::device(&Signer::generate()?, Signer::generate()?);
    let mut new_agent = RawClient::reconnect(&w.relay_url(), new_author).await;
    let revoked = authorized_profile(&owner.author, &new_agent.author, "new_agent");
    assert!(new_agent
        .request(Op::Register { profile: revoked })
        .await
        .is_err());
    assert!(lookup(&mut first, "new_agent").await.is_empty());
    let live = authorized_profile(&controller.author, &new_agent.author, "new_agent");
    new_agent
        .request(Op::Register { profile: live })
        .await
        .unwrap();
    assert_eq!(w.count("SELECT count(*) FROM identities").await, 3);
    Ok(())
}

#[tokio::test]
async fn agents_cannot_sponsor_other_agents() -> std::result::Result<(), Box<dyn std::error::Error>>
{
    let w = World::new("agent_owner_person").await;
    let owner = RawClient::connect(&w.relay_url(), "owner").await?;
    let first_author = Author::device(&Signer::generate()?, Signer::generate()?);
    let mut first = RawClient::reconnect(&w.relay_url(), first_author).await;
    first
        .request(Op::Register {
            profile: authorized_profile(&owner.author, &first.author, "first_agent"),
        })
        .await
        .unwrap();
    let second_author = Author::device(&Signer::generate()?, Signer::generate()?);
    let mut second = RawClient::reconnect(&w.relay_url(), second_author).await;
    assert!(second
        .request(Op::Register {
            profile: authorized_profile(&first.author, &second.author, "second_agent"),
        })
        .await
        .is_err());
    assert!(lookup(&mut first, "second_agent").await.is_empty());
    assert_eq!(w.count("SELECT count(*) FROM identities").await, 2);
    assert_eq!(w.count("SELECT count(*) FROM devices").await, 2);
    Ok(())
}

#[tokio::test]
async fn a_legacy_agent_must_authorize_its_existing_owner_before_directory_discovery(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use sqlx::Connection;
    let w = World::new("agent_owner_legacy").await;
    let owner = RawClient::connect(&w.relay_url(), "owner").await?;
    let other = RawClient::connect(&w.relay_url(), "other").await?;
    let author = Author::device(&Signer::generate()?, Signer::generate()?);
    let legacy = profile(&author, "legacy_agent", Some(owner.identity()));
    let mut db = sqlx::PgConnection::connect(&w.db_url).await.unwrap();
    sqlx::query(
        "INSERT INTO identities (id, handle, kind, owner, profile) VALUES ($1,$2,'Agent',$3,$4::jsonb)",
    )
    .bind(&legacy.id)
    .bind(&legacy.handle)
    .bind(&legacy.owner)
    .bind(serde_json::to_string(&legacy).unwrap())
    .execute(&mut db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO devices (device,identity,cert) VALUES ($1,$2,$3)")
        .bind(author.device.as_ref().unwrap())
        .bind(&author.identity)
        .bind(author.cert.as_ref().unwrap())
        .execute(&mut db)
        .await
        .unwrap();
    let mut agent = RawClient::reconnect(&w.relay_url(), author).await;
    assert!(lookup(&mut agent, "legacy_agent").await.is_empty());
    let wrong = authorized_profile(&other.author, &agent.author, "legacy_agent");
    assert!(agent
        .request(Op::Register { profile: wrong })
        .await
        .is_err());
    let upgraded = authorized_profile(&owner.author, &agent.author, "legacy_agent");
    agent
        .request(Op::Register {
            profile: upgraded.clone(),
        })
        .await
        .unwrap();
    assert_eq!(
        lookup(&mut agent, "legacy_agent").await,
        vec![upgraded.clone()]
    );
    // A directory blob cannot override the immutable SQL owner, even if it carries
    // a valid signature from a different account.
    let forged = authorized_profile(&other.author, &agent.author, "legacy_agent");
    sqlx::query("UPDATE identities SET profile = $2::jsonb WHERE id = $1")
        .bind(&legacy.id)
        .bind(serde_json::to_string(&forged).unwrap())
        .execute(&mut db)
        .await
        .unwrap();
    assert!(lookup(&mut agent, "legacy_agent").await.is_empty());
    db.close().await.unwrap();
    Ok(())
}

#[tokio::test]
async fn owner_revocation_committing_before_registration_is_observed_under_the_device_lock(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use sqlx::Connection;
    use std::time::Duration;
    let w = World::new("agent_owner_fence").await;
    let owner = RawClient::connect(&w.relay_url(), "owner").await?;
    let author = Author::device(&Signer::generate()?, Signer::generate()?);
    let mut agent = RawClient::reconnect(&w.relay_url(), author).await;
    let proposed = authorized_profile(&owner.author, &agent.author, "fenced_agent");
    let mut db = sqlx::PgConnection::connect(&w.db_url).await.unwrap();
    let mut tx = db.begin().await.unwrap();
    let mut observer = sqlx::PgConnection::connect(&w.db_url).await.unwrap();
    sqlx::query("SELECT device FROM devices WHERE device = $1 FOR UPDATE")
        .bind(owner.author.device.as_ref().unwrap())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let pending = tokio::spawn(async move {
        let result = agent.request(Op::Register { profile: proposed }).await;
        (agent, result)
    });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let blocked: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname = current_database()
             AND wait_event_type = 'Lock' AND query LIKE '%FROM devices WHERE device = $2 FOR SHARE%')",
        ).fetch_one(&mut observer).await.unwrap();
        if blocked {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "registration never acquired its owner fence"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!pending.is_finished());
    sqlx::query("UPDATE devices SET revoked_at = now() WHERE device = $1")
        .bind(owner.author.device.as_ref().unwrap())
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let (_agent, result) = pending.await.unwrap();
    assert!(
        result.is_err(),
        "registration ignored committed revocation: {result:?}"
    );
    assert_eq!(w.count("SELECT count(*) FROM identities").await, 1);
    assert_eq!(w.count("SELECT count(*) FROM devices").await, 1);
    Ok(())
}
