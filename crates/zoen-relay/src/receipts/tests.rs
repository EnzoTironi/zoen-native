use super::*;
#[test]
fn folds_unsigned_without_clamp() {
    let r = Receipts::default();
    r.raise("sp", "a", 5, 4);
    r.raise("sp", "a", 3, u64::MAX);
    assert_eq!(
        r.pending.lock().unwrap().marks["sp"]["a"].pair(),
        (u64::MAX, u64::MAX)
    );
}
#[test]
fn bounded_local_keyset_includes_pending_without_cross_space_rows() {
    let r = Receipts::default();
    for n in 0..400 {
        r.raise("sp", &format!("{n:064x}"), n, 0);
    }
    r.raise("other", "000", u64::MAX, 0);
    let rows = r.local_page("sp", "", &format!("{:064x}", 399), PAGE_ROWS + 1);
    assert_eq!(rows.len(), PAGE_ROWS + 1);
    assert_eq!(rows.first_key_value().unwrap().0, &format!("{:064x}", 0));
    assert_eq!(
        rows.last_key_value().unwrap().0,
        &format!("{:064x}", PAGE_ROWS)
    );
}
async fn owned_database() -> (PgPool, String, String) {
    use sqlx::{Connection, PgConnection};
    let admin =
        std::env::var("ZOEN_TEST_PG").expect("set ZOEN_TEST_PG for actual receipt race tests");
    let name = format!(
        "zoen_receipt_race_{}",
        &event_log::Signer::generate().id()[..16]
    );
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
    sqlx::query(include_str!("../../migrations/0030_receipts.sql"))
        .execute(&pool)
        .await
        .unwrap();
    (pool, admin, name)
}
async fn dispose_database(pool: PgPool, admin: &str, name: &str) {
    use sqlx::{Connection, PgConnection};
    pool.close().await;
    let mut connection = PgConnection::connect(admin).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE {name} WITH (FORCE)"
    )))
    .execute(&mut connection)
    .await
    .unwrap();
}
#[tokio::test]
async fn local_prequery_capture_survives_actual_sql_snapshot_then_flush_and_clear() {
    let (pool, admin, name) = owned_database().await;
    let receipts = std::sync::Arc::new(Receipts::default());
    let who = "ab".repeat(32);
    receipts.raise("sp", &who, 7, 5);
    let (read_tx, read_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let task = {
        let r = receipts.clone();
        let pool = pool.clone();
        let who = who.clone();
        tokio::spawn(async move {
            r.page_after_query(&pool, "sp", "", &who, async move {
                read_tx.send(()).unwrap();
                release_rx.await.unwrap();
            })
            .await
            .unwrap()
        })
    };
    read_rx.await.unwrap(); // The actual PostgreSQL SELECT already returned no rows.
    receipts.flush(&pool, &Metrics::default()).await.unwrap();
    assert_eq!(receipts.backlog(), (0, 0));
    release_tx.send(()).unwrap();
    let actual = task.await.unwrap();
    dispose_database(pool, &admin, &name).await;
    assert_eq!(actual.rows, vec![(who, 7, 5)]); // Previously lost between SELECT and postquery merge.
}
#[tokio::test]
async fn actual_blocked_upsert_retains_concurrent_higher_mark_for_next_flush() {
    let (pool, admin, name) = owned_database().await;
    let who = "cd".repeat(32);
    crate::db::raise_receipts(&pool, &[(("sp".into(), who.clone()), (0, 0))])
        .await
        .unwrap();
    let mut lock = pool.begin().await.unwrap();
    sqlx::query("SELECT identity FROM space_receipts WHERE space='sp' FOR UPDATE")
        .fetch_all(&mut *lock)
        .await
        .unwrap();
    let receipts = std::sync::Arc::new(Receipts::default());
    receipts.raise("sp", &who, u64::MAX, u64::MAX - 1);
    let task = {
        let r = receipts.clone();
        let p = pool.clone();
        tokio::spawn(async move { r.flush(&p, &Metrics::default()).await.unwrap() })
    };
    tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'INSERT INTO space_receipts%')")
                    .fetch_one(&pool).await.unwrap();
                if waiting { break; }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    assert!(receipts.raise("sp", &who, 0, u64::MAX));
    let actual = receipts.page(&pool, "sp", "", &who).await.unwrap();
    lock.commit().await.unwrap();
    task.await.unwrap();
    assert_eq!(receipts.backlog(), (1, 66));
    // A committed older snapshot cannot clear the concurrent higher mark.
    receipts.flush(&pool, &Metrics::default()).await.unwrap();
    assert_eq!(receipts.backlog(), (0, 0));
    let stored = Receipts::default()
        .page(&pool, "sp", "", &who)
        .await
        .unwrap();
    dispose_database(pool, &admin, &name).await;
    assert_eq!(actual.rows, vec![(who.clone(), u64::MAX, u64::MAX)]);
    assert_eq!(stored.rows, actual.rows);
}

#[test]
fn row_and_key_byte_budgets_do_not_evict_or_block_existing_marks() {
    let rows = Receipts::with_capacity(2, 100);
    assert!(rows.raise("sp", "a", 1, 0));
    assert!(rows.raise("sp", "b", 0, 0));
    assert!(!rows.raise("sp", "c", 9, 9));
    assert!(rows.raise("sp", "a", 0, u64::MAX));
    assert_eq!(rows.backlog(), (2, 6));
    assert_eq!(
        rows.local_page("sp", "", "z", 3)["a"].pair(),
        (u64::MAX, u64::MAX)
    );
    let bytes = Receipts::with_capacity(10, 6);
    assert!(bytes.raise("sp", "a", 1, 0));
    assert!(bytes.raise("sp", "b", 2, 0)); // Exact byte boundary accepted.
    assert!(!bytes.raise("sp", "c", 3, 0));
    assert!(!bytes.raise("oversized", "key", 4, 0));
    assert_eq!(bytes.backlog(), (2, 6));
    assert!(bytes.raise("sp", "b", 3, 2));
    assert_eq!(bytes.backlog(), (2, 6));
    assert!(!Receipts::with_capacity(0, 0).raise("sp", "a", 0, 0));
}

#[test]
fn default_admission_never_exceeds_row_budget() {
    let receipts = Receipts::default();
    for n in 0..=65_536 {
        receipts.raise("sp", &format!("{n:064x}"), 1, 0);
    }
    assert_eq!(
        receipts.local_page("sp", "", &"f".repeat(64), 65_537).len(),
        65_536
    );
}

#[tokio::test]
async fn concurrent_flushes_serialize_actual_sql_and_keep_latest_watermark() {
    let (pool, admin, name) = owned_database().await;
    let who = "ef".repeat(32);
    crate::db::raise_receipts(&pool, &[(("sp".into(), who.clone()), (0, 0))])
        .await
        .unwrap();
    let mut lock = pool.begin().await.unwrap();
    sqlx::query("SELECT identity FROM space_receipts WHERE space='sp' FOR UPDATE")
        .fetch_all(&mut *lock)
        .await
        .unwrap();
    let receipts = std::sync::Arc::new(Receipts::with_capacity(1, 66));
    let metrics = std::sync::Arc::new(Metrics::default());
    assert!(receipts.raise("sp", &who, 10, 5));
    let first = {
        let (r, p, m) = (receipts.clone(), pool.clone(), metrics.clone());
        tokio::spawn(async move { r.flush(&p, &m).await.unwrap() })
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'INSERT INTO space_receipts%')").fetch_one(&pool).await.unwrap();
            if waiting { break; }
            tokio::task::yield_now().await;
        }
    }).await.unwrap();
    assert!(receipts.raise("sp", &who, 100, 99));
    assert!(!receipts.raise("sp", "new", 1, 0));
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let second = {
        let (r, p, m) = (receipts.clone(), pool.clone(), metrics.clone());
        tokio::spawn(async move {
            entered.send(()).unwrap();
            r.flush(&p, &m).await.unwrap()
        })
    };
    waiting.await.unwrap();
    assert!(receipts.flushing.try_lock().is_err());
    lock.commit().await.unwrap();
    assert_eq!(first.await.unwrap(), 1);
    assert_eq!(second.await.unwrap(), 1);
    assert_eq!(receipts.backlog(), (0, 0));
    assert_eq!(
        metrics
            .receipts_flushed
            .load(std::sync::atomic::Ordering::Relaxed),
        2
    );
    let stored = Receipts::default()
        .page(&pool, "sp", "", &who)
        .await
        .unwrap();
    dispose_database(pool, &admin, &name).await;
    assert_eq!(stored.rows, vec![(who, 100, 99)]);
}
