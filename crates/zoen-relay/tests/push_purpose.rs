//! Fresh test-owned database: migrate legacy tokens, then exercise production
//! directory helpers. No provider request or real user registration is made.
use event_log::{Author, Signer};
use futures_util::FutureExt;
use sqlx::{Connection, PgConnection, PgPool};
use types::{Identity, IdentityKind};
use zoen_relay::{db, push};

#[tokio::test]
async fn legacy_migration_and_purpose_refresh_clear_transfer_and_revocation(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let admin = std::env::var("ZOEN_TEST_PG").expect("set ZOEN_TEST_PG for push purpose tests");
    let name = format!("zoen_push_{}", &Signer::generate()?.id()[..16]);
    let mut connection = PgConnection::connect(&admin).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&mut connection)
        .await
        .unwrap();
    let options = admin
        .parse::<sqlx::postgres::PgConnectOptions>()
        .unwrap()
        .database(&name);
    let pool = PgPool::connect_with(options).await.unwrap();
    let result = std::panic::AssertUnwindSafe(exercise(&pool))
        .catch_unwind()
        .await;
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE {name} WITH (FORCE)"
    )))
    .execute(&mut connection)
    .await
    .unwrap();
    match result {
        Ok(outcome) => outcome,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

async fn exercise(pool: &PgPool) -> std::result::Result<(), Box<dyn std::error::Error>> {
    sqlx::raw_sql(include_str!("../migrations/0029_push_tokens.sql"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO push_tokens(identity, device, platform, token) VALUES ('legacy', 'device', 'apns', 'legacy-token')")
        .execute(pool).await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/0034_push_purpose.sql"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/0042_push_registration_id.sql"))
        .execute(pool)
        .await?;
    let legacy: (String, String) = sqlx::query_as("SELECT token, purpose FROM push_tokens")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(legacy, ("legacy-token".into(), "notification".into()));
    sqlx::query("DROP TABLE push_tokens")
        .execute(pool)
        .await
        .unwrap();
    let previous = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            zoen_relay::MIGRATIONS
                .iter()
                .filter(|migration| migration.version <= 41)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    previous.run(pool).await?;
    sqlx::query("INSERT INTO push_tokens(identity, device, platform, token, purpose) VALUES ('upgrade', 'device', 'apns', 'upgrade-token', 'notification')")
        .execute(pool)
        .await?;
    sqlx::raw_sql(
        "INSERT INTO identities (id, kind, profile) VALUES ('upgrade-owner', 'Person', '{}'::jsonb);
         INSERT INTO devices (identity, device, cert, revoked_at) VALUES
           ('upgrade-owner', 'revoked-device', 'fixture', now()),
           ('upgrade-owner', 'active-device', 'fixture', NULL);
         INSERT INTO push_tokens (identity, device, platform, token, purpose) VALUES
           ('upgrade-owner', 'revoked-device', 'apns', 'aabb', 'notification'),
           ('upgrade-owner', 'revoked-device', 'apns-voip', 'ccdd', 'voip'),
           ('upgrade-owner', 'active-device', 'apns', 'eeff', 'notification');",
    )
    .execute(pool)
    .await?;
    zoen_relay::MIGRATIONS.run(pool).await?;
    let upgrade_tokens: Vec<String> = sqlx::query_scalar(
        "SELECT token FROM push_tokens WHERE identity = 'upgrade-owner' ORDER BY token",
    )
    .fetch_all(pool)
    .await?;
    assert_eq!(upgrade_tokens, ["eeff"]);
    let id: i64 = sqlx::query_scalar(
        "DELETE FROM push_tokens WHERE token = 'upgrade-token' RETURNING registration_id",
    )
    .fetch_one(pool)
    .await?;
    assert!(id > 0);

    let root = Signer::generate()?;
    let first = Author::device(&root, Signer::generate()?);
    let second = Author::device(&root, Signer::generate()?);
    let identity = Identity {
        id: root.id(),
        kind: IdentityKind::Person,
        name: "Push test".into(),
        handle: "purpose_test".into(),
        tint_hex: "#123456".into(),
        glyph: None,
        owner: None,
        bio: String::new(),
        owner_proof: None,
    };
    let (device, sibling) = (
        first.device.as_deref().unwrap(),
        second.device.as_deref().unwrap(),
    );
    let mut tx = pool.begin().await.unwrap();
    db::register(
        &mut tx,
        &identity,
        &identity.handle,
        device,
        first.cert.as_deref().unwrap(),
        true,
    )
    .await
    .unwrap();
    db::enroll_device(
        &mut tx,
        &identity.id,
        sibling,
        second.cert.as_deref().unwrap(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let now_ms: i64 =
        sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint")
            .fetch_one(pool)
            .await?;
    let expiry = now_ms + 30_000;
    let (first, second) = tokio::join!(
        db::claim_voip_wake(pool, "direct", 12, expiry),
        db::claim_voip_wake(pool, "direct", 12, expiry),
    );
    assert_ne!(first?, second?);
    assert!(!db::claim_voip_wake(pool, "direct", 12, expiry).await?);
    assert!(!db::claim_voip_wake(pool, "direct", 13, now_ms).await?);
    assert!(db::claim_voip_wake(pool, "direct", 13, expiry).await?);

    let sender = Signer::generate()?.id();
    sqlx::query("INSERT INTO identities (id, kind, profile) VALUES ($1, 'Person', '{}'::jsonb)")
        .bind(&sender)
        .execute(pool)
        .await?;
    let audience = [identity.id.clone()];

    let (first_claim, second_claim) = tokio::join!(
        db::set_push_token(pool, &identity.id, device, push::APNS_VOIP, "ABCD"),
        db::set_push_token(pool, &identity.id, sibling, push::APNS_VOIP, "abcd"),
    );
    first_claim?;
    second_claim?;
    assert!(db::voip_push_targets(pool, &identity.id, &sender, "direct")
        .await?
        .is_empty());
    assert!(!db::get_voip_consent(pool, &identity.id, &sender, "direct").await?);
    db::set_voip_consent(pool, &identity.id, device, &sender, "direct", true).await?;
    assert!(db::get_voip_consent(pool, &identity.id, &sender, "direct").await?);
    assert!(!db::get_voip_consent(pool, &identity.id, &sender, "other").await?);
    assert!(!db::get_voip_consent(pool, &sender, &identity.id, "direct").await?);
    assert!(db::voip_push_targets(pool, &identity.id, &sender, "other")
        .await?
        .is_empty());
    assert!(
        db::voip_push_targets(pool, &identity.id, &identity.id, "direct")
            .await?
            .is_empty()
    );
    let selected = db::voip_push_targets(pool, &identity.id, &sender, "direct").await?;
    let registration = selected[0].registration_id;
    assert_eq!(tokens(selected), ["abcd"]);
    assert!(db::voip_registration_current(pool, &identity.id, &sender, "direct", registration).await?);
    db::set_voip_consent(pool, &identity.id, sibling, &sender, "direct", false).await?;
    assert!(!db::get_voip_consent(pool, &identity.id, &sender, "direct").await?);
    assert!(db::voip_push_targets(pool, &identity.id, &sender, "direct")
        .await?
        .is_empty());
    assert!(!db::voip_registration_current(pool, &identity.id, &sender, "direct", registration).await?);
    db::set_voip_consent(pool, &identity.id, device, &sender, "direct", true).await?;
    db::set_push_token(pool, &identity.id, device, push::APNS_VOIP, "").await?;
    db::set_push_token(pool, &identity.id, sibling, push::APNS_VOIP, "").await?;

    db::set_push_token(pool, &identity.id, device, push::APNS, "aabb")
        .await
        .unwrap();
    db::set_push_token(pool, &identity.id, device, push::APNS_VOIP, "ccdd")
        .await
        .unwrap();
    assert_eq!(
        tokens(db::push_targets(pool, &audience).await.unwrap()),
        ["aabb"]
    );
    assert_eq!(
        tokens(
            db::voip_push_targets(pool, &identity.id, &sender, "direct")
                .await
                .unwrap()
        ),
        ["ccdd"]
    );
    db::set_push_token(pool, &identity.id, device, push::APNS_VOIP_SANDBOX, "eeff")
        .await
        .unwrap();
    db::set_push_token(pool, &identity.id, device, push::APNS_SANDBOX, "0011")
        .await
        .unwrap();
    assert_eq!(
        tokens(db::push_targets(pool, &audience).await.unwrap()),
        ["0011"]
    );
    assert_eq!(
        tokens(
            db::voip_push_targets(pool, &identity.id, &sender, "direct")
                .await
                .unwrap()
        ),
        ["eeff"]
    );

    // Moving a token to a sibling removes only the old purpose registration.
    // Older relays could persist APNs tokens without canonicalizing case.
    sqlx::query("UPDATE push_tokens SET token = 'EEFF' WHERE purpose = 'voip' AND token = 'eeff'")
        .execute(pool)
        .await?;
    let previous_voip = db::voip_push_targets(pool, &identity.id, &sender, "direct")
        .await?
        .remove(0);
    db::set_push_token(pool, &identity.id, sibling, push::APNS_VOIP, "EEFF")
        .await
        .unwrap();
    assert!(
        !db::voip_registration_current(
            pool,
            &identity.id,
            &sender,
            "direct",
            previous_voip.registration_id,
        )
        .await?
    );
    db::drop_push_token(pool, &previous_voip).await?;
    let owner: String = sqlx::query_scalar("SELECT device FROM push_tokens WHERE purpose = 'voip'")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(owner, sibling);
    assert_eq!(
        tokens(db::push_targets(pool, &audience).await.unwrap()),
        ["0011"]
    );
    db::set_push_token(pool, &identity.id, sibling, push::APNS_VOIP, "")
        .await
        .unwrap();
    assert!(db::voip_push_targets(pool, &identity.id, &sender, "direct")
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        tokens(db::push_targets(pool, &audience).await.unwrap()),
        ["0011"]
    );

    // Even matching token bytes cannot couple the two purposes on refresh/death.
    db::set_push_token(pool, &identity.id, device, push::APNS_VOIP, "0011")
        .await
        .unwrap();
    let old = db::push_targets(pool, &audience).await?.remove(0);
    db::set_push_token(pool, &identity.id, device, push::APNS, "0011").await?;
    db::drop_push_token(pool, &old).await?;
    let current = db::push_targets(pool, &audience).await?.remove(0);
    assert_ne!(old.registration_id, current.registration_id);
    assert_eq!(current.token, old.token);
    db::drop_push_token(pool, &current).await?;
    assert!(db::push_targets(pool, &audience).await.unwrap().is_empty());
    assert_eq!(
        tokens(
            db::voip_push_targets(pool, &identity.id, &sender, "direct")
                .await
                .unwrap()
        ),
        ["0011"]
    );
    db::set_push_token(pool, &identity.id, device, push::APNS, "2233")
        .await
        .unwrap();
    db::set_push_token(pool, &identity.id, sibling, push::APNS, "4455").await?;
    let mut tx = pool.begin().await.unwrap();
    assert!(db::revoke_device(&mut tx, &identity.id, device)
        .await
        .unwrap());
    let registration = db::set_push_token(pool, &identity.id, device, push::APNS, "3344");
    tokio::pin!(registration);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut registration)
            .await
            .is_err()
    );
    tx.commit().await.unwrap();
    assert!(registration.await.is_err());
    let retained: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM push_tokens WHERE identity = $1 AND device = $2",
    )
    .bind(&identity.id)
    .bind(device)
    .fetch_one(pool)
    .await?;
    assert_eq!(retained, 0);
    assert!(
        db::set_voip_consent(pool, &identity.id, device, &sender, "direct", true)
            .await
            .is_err()
    );
    assert_eq!(tokens(db::push_targets(pool, &audience).await?), ["4455"]);
    assert!(db::voip_push_targets(pool, &identity.id, &sender, "direct")
        .await
        .unwrap()
        .is_empty());
    assert!(
        db::set_push_token(pool, &identity.id, device, push::APNS, "4455")
            .await
            .is_err()
    );
    assert_eq!(tokens(db::push_targets(pool, &audience).await?), ["4455"]);
    assert!(
        db::set_push_token(pool, &identity.id, sibling, "unknown", "4455")
            .await
            .is_err()
    );
    Ok(())
}

fn tokens(targets: Vec<push::Target>) -> Vec<String> {
    targets.into_iter().map(|target| target.token).collect()
}
