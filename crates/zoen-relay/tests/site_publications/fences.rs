use super::*;

async fn waiting(pool: &PgPool, query: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database()
                 AND query=$1 AND wait_event_type='Lock')",
            )
            .bind(query)
            .fetch_one(pool)
            .await
            .unwrap();
            if blocked {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("owned operation must reach its real SQL lock wait");
}

pub async fn exercise(pool: &PgPool, service: &Service, author: &Author) {
    let site = types::new_secret_id("site");
    let signed = request(author, &site, None, FIRST, 1);
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&site)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let publisher = service.clone();
    let publication = tokio::spawn(async move { publisher.apply(&signed, Some(FIRST), NOW).await });
    // Publication has already acquired the device FOR SHARE fence and is now
    // blocked at its site lock. No timing guess substitutes for this observation.
    waiting(
        pool,
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
    )
    .await;
    let revoker_pool = pool.clone();
    let owner = author.identity.clone();
    let device = author.key.id();
    let revocation = tokio::spawn(async move {
        let mut tx = revoker_pool.begin().await.unwrap();
        // Use an explicit equivalent update so the observer has a stable query.
        sqlx::query("UPDATE devices SET revoked_at=now() WHERE identity=$1 AND device=$2")
            .bind(owner)
            .bind(device)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    });
    waiting(
        pool,
        "UPDATE devices SET revoked_at=now() WHERE identity=$1 AND device=$2",
    )
    .await;
    assert!(!revocation.is_finished());
    blocker.commit().await.unwrap();
    assert!(publication.await.unwrap().is_ok());
    revocation.await.unwrap();
}
