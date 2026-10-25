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
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    Ok(())
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
    let legacy: (String, String) = sqlx::query_as("SELECT token, purpose FROM push_tokens")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(legacy, ("legacy-token".into(), "notification".into()));
    sqlx::query("DROP TABLE push_tokens")
        .execute(pool)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(pool).await.unwrap();

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
    let audience = [identity.id.clone()];

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
        tokens(db::voip_push_targets(pool, &audience).await.unwrap()),
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
        tokens(db::voip_push_targets(pool, &audience).await.unwrap()),
        ["eeff"]
    );

    // Moving a token to a sibling removes only the old purpose registration.
    db::set_push_token(pool, &identity.id, sibling, push::APNS_VOIP, "eeff")
        .await
        .unwrap();
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
    assert!(db::voip_push_targets(pool, &audience)
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
    db::drop_push_token(pool, "0011").await.unwrap();
    assert!(db::push_targets(pool, &audience).await.unwrap().is_empty());
    assert_eq!(
        tokens(db::voip_push_targets(pool, &audience).await.unwrap()),
        ["0011"]
    );
    db::set_push_token(pool, &identity.id, device, push::APNS, "2233")
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(db::revoke_device(&mut tx, &identity.id, device)
        .await
        .unwrap());
    tx.commit().await.unwrap();
    assert!(db::push_targets(pool, &audience).await.unwrap().is_empty());
    assert!(db::voip_push_targets(pool, &audience)
        .await
        .unwrap()
        .is_empty());
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
