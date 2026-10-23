//! Real PostgreSQL migrations, current directory authorization and filesystem
//! object_store. Every fixture owns its database, directory and HTTP listeners.
#[path = "site_publications/mod.rs"]
mod fixture;

use event_log::Signer;
use futures_util::FutureExt;
use sqlx::{Connection, PgConnection, PgPool};

#[tokio::test]
async fn selected_publication_replay_rollback_revocation_and_isolated_http() {
    let admin = std::env::var("ZOEN_TEST_PG").expect("set ZOEN_TEST_PG for Site publication tests");
    let name = format!("zoen_sites_{}", &Signer::generate().id()[..16]);
    let directory = std::env::temp_dir().join(&name);
    std::fs::create_dir(&directory).unwrap();
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
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let result = std::panic::AssertUnwindSafe(fixture::exercise(&pool, &directory))
        .catch_unwind()
        .await;
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE {name} WITH (FORCE)"
    )))
    .execute(&mut connection)
    .await
    .unwrap();
    std::fs::remove_dir_all(directory).unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
