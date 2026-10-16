//! Receipt watermarks written behind (SCALE.md §3): folding, the batched upsert, never
//! lowering a stored mark, and replay seeing what is still pending. Fresh database.
use sqlx::{Connection, PgConnection, PgPool};
use zoen_relay::{db, metrics::Metrics, receipts::Receipts};

async fn database() -> (PgPool, String, String) {
    let admin = std::env::var("ZOEN_TEST_PG").expect("set ZOEN_TEST_PG for receipt tests");
    let name = format!("zoen_rcpt_{}", &zoen_log::Signer::generate().id()[..16]);
    let mut c = PgConnection::connect(&admin).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&mut c)
        .await
        .unwrap();
    let base = admin.rsplit_once('/').unwrap().0;
    let pool = PgPool::connect(&format!("{base}/{name}")).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    (pool, admin, name)
}

async fn drop_db(pool: PgPool, admin: &str, name: &str) {
    pool.close().await;
    let mut c = PgConnection::connect(admin).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE {name} WITH (FORCE)"
    )))
    .execute(&mut c)
    .await
    .unwrap();
}

fn sorted(mut v: Vec<(String, i64, i64)>) -> Vec<(String, i64, i64)> {
    v.sort();
    v
}

#[tokio::test]
async fn watermarks_fold_flush_in_one_batch_and_never_drop() {
    let (pool, admin, name) = database().await;
    let r = Receipts::default();
    let m = Metrics::default();

    // Many receipts, two people: one row each after folding.
    for seq in 1..=500u64 {
        r.raise("sp1", "ana", seq, 0);
        r.raise("sp1", "bia", seq, seq / 2);
    }
    // Replay before the flush already sees them.
    assert_eq!(
        sorted(r.of_space(&pool, "sp1").await.unwrap()),
        vec![("ana".into(), 500, 0), ("bia".into(), 500, 250)]
    );
    assert_eq!(r.flush(&pool, &m).await.unwrap(), 2);
    assert_eq!(r.flush(&pool, &m).await.unwrap(), 0, "nothing left pending");
    assert_eq!(
        sorted(db::receipts(&pool, "sp1").await.unwrap()),
        vec![("ana".into(), 500, 0), ("bia".into(), 500, 250)]
    );

    // A late, lower receipt (another node, a stale device) never lowers the stored mark;
    // read implies delivered.
    r.raise("sp1", "ana", 3, 600);
    r.raise("sp1", "bia", 1, 1);
    r.flush(&pool, &m).await.unwrap();
    assert_eq!(
        sorted(db::receipts(&pool, "sp1").await.unwrap()),
        vec![("ana".into(), 600, 600), ("bia".into(), 500, 250)]
    );

    // A batch larger than one statement's chunk, across many Spaces.
    for i in 0..5_000u64 {
        r.raise(&format!("bulk{i}"), "ana", i + 1, 0);
    }
    assert_eq!(r.flush(&pool, &m).await.unwrap(), 5_000);
    assert_eq!(
        db::receipts(&pool, "bulk4999").await.unwrap(),
        vec![("ana".into(), 5000, 0)]
    );
    assert!(m.receipt_flushes.load(std::sync::atomic::Ordering::Relaxed) >= 4);

    // Spaces don't leak into each other's replay.
    r.raise("sp2", "caio", 9, 9);
    assert_eq!(
        sorted(r.of_space(&pool, "sp1").await.unwrap()),
        vec![("ana".into(), 600, 600), ("bia".into(), 500, 250)]
    );
    // Many Spaces in one query, pending merged, unknown Spaces absent.
    let many = r
        .of_spaces(&pool, &["sp1".into(), "sp2".into(), "nope".into()])
        .await
        .unwrap();
    assert_eq!(
        sorted(many["sp1"].clone()),
        vec![("ana".into(), 600, 600), ("bia".into(), 500, 250)]
    );
    assert_eq!(many["sp2"], vec![("caio".into(), 9, 9)]);
    assert!(!many.contains_key("nope"));
    drop_db(pool, &admin, &name).await;
}
