//! Receipt watermarks written behind (SCALE.md §3): folding, the batched upsert, never
//! lowering a stored mark, and replay seeing what is still pending. Fresh database.
use sqlx::{Connection, PgConnection, PgPool};
use zoen_relay::{db, metrics::Metrics, receipts::Receipts};

async fn database() -> (PgPool, String, String) {
    let admin = std::env::var("ZOEN_TEST_PG").expect("set ZOEN_TEST_PG for receipt tests");
    let name = format!("zoen_rcpt_{}", &event_log::Signer::generate().id()[..16]);
    let mut c = PgConnection::connect(&admin).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&mut c)
        .await
        .unwrap();
    let options = admin
        .parse::<sqlx::postgres::PgConnectOptions>()
        .unwrap()
        .database(&name);
    let pool = PgPool::connect_with(options).await.unwrap();
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

fn who(n: u64) -> String {
    format!("{n:064x}")
}
async fn all(r: &Receipts, pool: &PgPool, space: &str, ceiling: &str) -> Vec<(String, u64, u64)> {
    let mut after = String::new();
    let mut rows = vec![];
    loop {
        let page = r.page(pool, space, &after, ceiling).await.unwrap();
        rows.extend(page.rows);
        let Some(next) = page.next else { break };
        assert!(next > after);
        after = next;
    }
    rows
}
#[tokio::test]
async fn full_unsigned_marks_fold_flush_restart_and_never_regress() {
    let (pool, admin, name) = database().await;
    let r = Receipts::default();
    let metrics = Metrics::default();
    for seq in 1..=500 {
        r.raise("sp1", &who(1), seq, 0);
        r.raise("sp1", &who(2), seq, seq / 2);
    }
    let initial = all(&r, &pool, "sp1", &who(2)).await;
    assert_eq!(initial, vec![(who(1), 500, 0), (who(2), 500, 250)]);
    assert_eq!(r.flush(&pool, &metrics).await.unwrap(), 2);
    r.raise("sp1", &who(1), 3, u64::MAX);
    r.raise("sp1", &who(2), 1, 1);
    r.raise("sp1", &who(3), 0, 0); // First zero remains an actual identity row.
    r.flush(&pool, &metrics).await.unwrap();
    let actual = all(&Receipts::default(), &pool, "sp1", &who(3)).await;
    drop_db(pool, &admin, &name).await;
    assert_eq!(
        actual,
        vec![
            (who(1), u64::MAX, u64::MAX),
            (who(2), 500, 250),
            (who(3), 0, 0)
        ]
    );
}
#[tokio::test]
async fn keyset_crosses_pages_without_table_snapshot_and_preserves_initial_ceiling() {
    let (pool, admin, name) = database().await;
    let r = Receipts::default();
    let metrics = Metrics::default();
    for n in 1..=600 {
        r.raise("sp", &who(n * 2), n, n / 2);
    }
    r.flush(&pool, &metrics).await.unwrap();
    // Initial max captures persisted and local pending. No receipt rows copied.
    r.raise("sp", &who(1202), u64::MAX, 0);
    let ceiling = r
        .ceilings(&pool, &["sp".into(), "unknown".into()])
        .await
        .unwrap();
    assert_eq!(ceiling.len(), 1);
    assert_eq!(ceiling["sp"], who(1202));
    let first = r.page(&pool, "sp", "", &ceiling["sp"]).await.unwrap();
    assert_eq!(first.rows.len(), 128);
    assert_eq!(first.next, Some(who(256)));
    // New keys/updates beyond the page cut may appear, keys above initial ceiling
    // cannot extend this traversal. Every original row remains represented.
    r.raise("sp", &who(259), 9_007_199_254_740_993, 0);
    r.raise("sp", &who(2000), u64::MAX, u64::MAX);
    r.raise("sp", &who(400), 0, u64::MAX);
    r.flush(&pool, &metrics).await.unwrap();
    let mut actual = first.rows;
    let mut after = first.next.unwrap();
    loop {
        let page = r.page(&pool, "sp", &after, &ceiling["sp"]).await.unwrap();
        actual.extend(page.rows);
        let Some(next) = page.next else { break };
        after = next;
    }
    let keys: Vec<_> = actual.iter().map(|row| row.0.clone()).collect();
    assert!(keys.windows(2).all(|pair| pair[0] < pair[1]));
    for n in 1..=600 {
        assert!(actual
            .iter()
            .any(|row| row.0 == who(n * 2) && row.1 >= n && row.2 >= n / 2));
    }
    assert!(actual.iter().any(|row| row.0 == who(259)));
    assert!(!keys.contains(&who(2000)));
    assert_eq!(
        actual.iter().find(|row| row.0 == who(400)).unwrap().2,
        u64::MAX
    );
    drop_db(pool, &admin, &name).await;
}
#[tokio::test]
async fn numeric_storage_accepts_unsigned_boundaries_and_rejects_out_of_range_or_read_ahead() {
    let (pool, admin, name) = database().await;
    for (n, value) in [0, 9_007_199_254_740_993, i64::MAX as u64 + 1, u64::MAX]
        .into_iter()
        .enumerate()
    {
        db::raise_receipts(&pool, &[(("sp".into(), who(n as u64)), (value, value))])
            .await
            .unwrap();
    }
    let actual = all(&Receipts::default(), &pool, "sp", &who(3)).await;
    let mut refused = vec![];
    for (d, r) in [("-1", "0"), ("18446744073709551616", "0"), ("1", "2")] {
        refused.push(sqlx::query("INSERT INTO space_receipts(space,identity,delivered,read) VALUES('bad',$1,$2::NUMERIC,$3::NUMERIC)")
            .bind(who(10)).bind(d).bind(r).execute(&pool).await.is_err());
    }
    drop_db(pool, &admin, &name).await;
    assert_eq!(
        actual.iter().map(|row| row.1).collect::<Vec<_>>(),
        vec![0, 9_007_199_254_740_993, i64::MAX as u64 + 1, u64::MAX]
    );
    assert!(refused.into_iter().all(|r| r));
}
#[tokio::test]
async fn failed_later_flush_chunk_keeps_unwritten_marks_visible_and_retryable() {
    let (pool, admin, name) = database().await;
    let r = Receipts::default();
    let metrics = Metrics::default();
    // Actual PostgreSQL trigger fails the second real 2000-row chunk.
    sqlx::query("CREATE FUNCTION reject_receipt() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN IF NEW.identity=lpad(to_hex(2200),64,'0') THEN RAISE EXCEPTION 'owned receipt chunk fault'; END IF; RETURN NEW; END $$")
        .execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER receipt_fault BEFORE INSERT ON space_receipts FOR EACH ROW EXECUTE FUNCTION reject_receipt()")
        .execute(&pool).await.unwrap();
    for n in 0..4500 {
        r.raise("sp", &who(n), n + 1, 0);
    }
    assert!(r.flush(&pool, &metrics).await.is_err());
    assert_eq!(all(&r, &pool, "sp", &who(4499)).await.len(), 4500);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM space_receipts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2000);
    sqlx::query("DROP TRIGGER receipt_fault ON space_receipts")
        .execute(&pool)
        .await
        .unwrap();
    r.flush(&pool, &metrics).await.unwrap();
    let actual = all(&Receipts::default(), &pool, "sp", &who(4499)).await;
    drop_db(pool, &admin, &name).await;
    assert_eq!(actual.len(), 4500);
    assert!(actual.into_iter().all(|(_, d, r)| d > 0 && r == 0));
}
