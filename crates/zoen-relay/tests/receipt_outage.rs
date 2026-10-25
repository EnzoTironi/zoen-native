//! Opt-in service outage: an external owner stops/restarts only its disposable PG.
//! No Docker/service command or production endpoint is selected by this test.
use event_log::Signer;
use sqlx::{postgres::PgPoolOptions, Connection, PgConnection};
use std::{path::Path, time::Duration};
use zoen_relay::{metrics::Metrics, receipts::Receipts};

async fn phase(directory: &Path, request: &str, completed: &str) {
    tokio::fs::write(directory.join(request), b"owned receipt test\n")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(45), async {
        while !directory.join(completed).is_file() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("owned PostgreSQL controller did not complete phase");
}

#[tokio::test]
#[ignore = "requires an explicit disposable-PG outage controller and private phase directory"]
async fn bounded_receipts_survive_actual_postgresql_stop_and_restart(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let admin_url = std::env::var("ZOEN_TEST_PG").expect("owned PG URL required");
    let directory =
        std::env::var_os("ZOEN_TEST_PG_OUTAGE_DIR").expect("owned phase directory required");
    let directory = Path::new(&directory);
    assert!(directory.is_dir());
    for marker in ["request-stop", "stopped", "request-start", "restarted"] {
        assert!(
            !directory.join(marker).exists(),
            "use a fresh phase directory"
        );
    }
    let name = format!("zoen_outage_{}", &Signer::generate()?.id()[..16]);
    let mut admin = PgConnection::connect(&admin_url).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&mut admin)
        .await
        .unwrap();
    drop(admin);
    let options = admin_url
        .parse::<sqlx::postgres::PgConnectOptions>()
        .unwrap()
        .database(&name);
    let pool = PgPoolOptions::new()
        .max_connections(3)
        .acquire_timeout(Duration::from_millis(500))
        .connect_with(options.clone())
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/0030_receipts.sql"))
        .execute(&pool)
        .await
        .unwrap();
    let receipts = Receipts::with_capacity(3, 198);
    let who = |n: usize| format!("{n:064x}");
    for n in 0..3 {
        assert!(receipts.raise("sp", &who(n), n as u64, 0));
    }
    phase(directory, "request-stop", "stopped").await;
    for _ in 0..3 {
        assert!(receipts.flush(&pool, &Metrics::default()).await.is_err());
        for n in 3..10_003 {
            assert!(!receipts.raise("sp", &who(n), u64::MAX, 0));
        }
        assert!(receipts.raise("sp", &who(0), 0, u64::MAX));
        assert!(receipts.raise("sp", &who(1), 100, 99));
        assert_eq!(receipts.backlog(), (3, 198));
    }
    phase(directory, "request-start", "restarted").await;
    // Container-local pg_isready does not prove Docker's host TCP mapping is up.
    // Establish actual SQL readiness on this test database before testing recovery;
    // this is a fixture barrier, not a change to the relay's retry/timeout policy.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(mut probe) = PgConnection::connect_with(&options).await {
                if sqlx::query("SELECT 1").execute(&mut probe).await.is_ok() {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("owned host-mapped PostgreSQL not ready");
    assert_eq!(receipts.flush(&pool, &Metrics::default()).await.unwrap(), 3);
    assert_eq!(receipts.backlog(), (0, 0));
    let actual = Receipts::default()
        .page(&pool, "sp", "", &who(2))
        .await
        .unwrap();
    assert_eq!(
        actual.rows,
        vec![
            (who(0), u64::MAX, u64::MAX),
            (who(1), 100, 99),
            (who(2), 2, 0)
        ]
    );
    // Capacity is reclaimed only after persistence; new receipts remain usable.
    assert!(receipts.raise("sp", &who(3), 0, 0));
    assert_eq!(receipts.flush(&pool, &Metrics::default()).await.unwrap(), 1);
    pool.close().await;
    let mut admin = PgConnection::connect(&admin_url).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE {name} WITH (FORCE)"
    )))
    .execute(&mut admin)
    .await
    .unwrap();
    Ok(())
}
