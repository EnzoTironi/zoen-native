//! Bounded expiry maintenance must not affect live or one-time link boxes.
use sqlx::{Connection, PgConnection, PgPool};
use zoen_relay::db;

#[tokio::test]
async fn publication_bounds_cleanup_and_reuses_expired_id_beyond_batch() {
    let admin = std::env::var("ZOEN_TEST_PG").expect("set ZOEN_TEST_PG for link-box tests");
    let name = format!("zoen_link_{}", &zoen_log::Signer::generate().id()[..16]);
    let mut c = PgConnection::connect(&admin).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&mut c)
        .await
        .unwrap();
    let base = admin.rsplit_once('/').unwrap().0;
    let pool = PgPool::connect(&format!("{base}/{name}")).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO link_boxes (id, sealed, created_at)
        SELECT 'old' || n, '\\x01'::bytea, now() - interval '1 hour'
        FROM generate_series(1, 1000) n",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO link_boxes (id, sealed, created_at)
        VALUES ('reuse', '\\x01', now() - interval '16 minutes'),
               ('live', '\\x02', now())",
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(db::put_link_box(&mut tx, "reuse", &[3]).await.unwrap());
    tx.commit().await.unwrap();
    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM link_boxes
        WHERE created_at < now() - interval '15 minutes'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        remaining, 872,
        "only 128 old boxes and the requested expired id removed"
    );
    assert_eq!(
        db::take_link_box(&pool, "reuse").await.unwrap(),
        Some(vec![3])
    );
    assert_eq!(db::take_link_box(&pool, "reuse").await.unwrap(), None);
    let mut tx = pool.begin().await.unwrap();
    assert!(!db::put_link_box(&mut tx, "live", &[9]).await.unwrap());
    tx.commit().await.unwrap();
    assert_eq!(
        db::take_link_box(&pool, "live").await.unwrap(),
        Some(vec![2])
    );
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE {name} WITH (FORCE)"
    )))
    .execute(&mut c)
    .await
    .unwrap();
}
