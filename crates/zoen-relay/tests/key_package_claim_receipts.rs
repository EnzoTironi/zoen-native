//! PostgreSQL transaction/retention boundaries. Uses a fresh database and actual signed
//! MLS publications; socket authorization and message readability are CLI journeys.
use roda_log::{Author, Signer};
use roda_proto::{KeyPackageRecord, Reply, ServerFrame, KEY_PACKAGE_CLAIM_TTL_MS};
use roda_types::{Identity, IdentityKind};
use sqlx::{Connection, PgConnection, PgPool};
use std::time::{SystemTime, UNIX_EPOCH};
use zoen_relay::{
    db,
    key_package_claims::{self, GC_BATCH, MAX_RECEIPTS, MAX_RECEIPT_BYTES},
};

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

struct Database {
    pool: PgPool,
    admin: String,
    name: String,
}
impl Database {
    async fn new() -> Self {
        let admin =
            std::env::var("ZOEN_TEST_PG").expect("set ZOEN_TEST_PG for receipt transaction tests");
        let name = format!("zoen_claim_{}", &Signer::generate().id()[..16]);
        let mut connection = PgConnection::connect(&admin).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
            .execute(&mut connection)
            .await
            .unwrap();
        let base = admin.rsplit_once('/').unwrap().0;
        let pool = PgPool::connect(&format!("{base}/{name}")).await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        Self { pool, admin, name }
    }
    async fn finish(self) {
        self.pool.close().await;
        let mut connection = PgConnection::connect(&self.admin).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE {} WITH (FORCE)",
            self.name
        )))
        .execute(&mut connection)
        .await
        .unwrap();
    }
    async fn person(&self, handle: &str) -> Author {
        let root = Signer::generate();
        let author = Author::device(&root, Signer::generate());
        let profile = Identity {
            owner_proof: None,
            id: root.id(),
            kind: IdentityKind::Person,
            name: handle.into(),
            handle: handle.into(),
            tint_hex: "#123456".into(),
            glyph: None,
            owner: None,
            bio: String::new(),
        };
        let mut tx = self.pool.begin().await.unwrap();
        db::register(
            &mut tx,
            &profile,
            handle,
            author.device.as_deref().unwrap(),
            author.cert.as_deref().unwrap(),
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        author
    }
    async fn publish(&self, author: &Author) {
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        roda_mls::migrate(&mut connection).unwrap();
        let device = roda_mls::Device::new(
            &connection,
            Signer::generate().secret(),
            &author.identity,
            author.key.secret(),
            author.cert.as_deref().unwrap(),
        )
        .unwrap();
        let packages: Vec<_> = device
            .key_packages(4, false)
            .unwrap()
            .into_iter()
            .map(|p| {
                let (_, expires) = roda_mls::key_package_publication(&p).unwrap();
                (p, expires)
            })
            .collect();
        assert!(db::put_key_packages(
            &self.pool,
            &author.identity,
            author.device.as_deref().unwrap(),
            &packages,
            None
        )
        .await
        .unwrap());
    }
    async fn stock(&self, target: &Author) -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM key_packages WHERE identity = $1 AND NOT last_resort",
        )
        .bind(&target.identity)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }
}

async fn claim(
    pool: &PgPool,
    actor: &Author,
    target: &Author,
    operation: &str,
) -> Vec<KeyPackageRecord> {
    key_package_claims::claim(
        pool,
        &actor.identity,
        actor.device.as_deref().unwrap(),
        operation,
        std::slice::from_ref(&target.identity),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn concurrent_retries_use_one_transactional_receipt() {
    let d = Database::new().await;
    let actor = d.person("ana").await;
    let target = d.person("bruno").await;
    d.publish(&target).await;
    let operation = roda_types::new_ulid(now());
    let (first, second) = tokio::join!(
        claim(&d.pool, &actor, &target, &operation),
        claim(&d.pool, &actor, &target, &operation)
    );
    assert_eq!(first, second);
    assert_eq!(first.len(), 1);
    assert_eq!(d.stock(&target).await, 3);
    d.finish().await;
}

#[tokio::test]
async fn receipt_capacity_preserves_replay_and_rolls_back_new_consumption() {
    let d = Database::new().await;
    let actor = d.person("ana").await;
    let target = d.person("bruno").await;
    d.publish(&target).await;
    let operation = roda_types::new_ulid(now());
    let original = claim(&d.pool, &actor, &target, &operation).await;
    let empty = ServerFrame::Res {
        id: 0,
        result: Ok(Reply::KeyPackages(Vec::new())),
    }
    .encode();
    // Synthetic receipt rows model resource saturation only. Every consumed package
    // above/below is a real publication; a failed capacity check must restore it.
    sqlx::query("INSERT INTO key_package_claim_receipts(source_identity,source_device,operation_id,targets,response,expires_at_ms)
        SELECT $1,$2,lpad(n::text,26,'0'),$3,$4,$5 FROM generate_series(1,$6) AS n")
        .bind(&actor.identity).bind(actor.device.as_deref().unwrap()).bind(vec![target.identity.clone()])
        .bind(&empty).bind(now() + KEY_PACKAGE_CLAIM_TTL_MS).bind(MAX_RECEIPTS - 1).execute(&d.pool).await.unwrap();
    assert_eq!(claim(&d.pool, &actor, &target, &operation).await, original);
    let next = roda_types::new_ulid(now());
    let error = key_package_claims::claim(
        &d.pool,
        &actor.identity,
        actor.device.as_deref().unwrap(),
        &next,
        std::slice::from_ref(&target.identity),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("capacity"));
    assert_eq!(d.stock(&target).await, 3);
    sqlx::query("DELETE FROM key_package_claim_receipts WHERE operation_id <> $1")
        .bind(&operation)
        .execute(&d.pool)
        .await
        .unwrap();
    let original_bytes: i64 = sqlx::query_scalar("SELECT octet_length(response)::bigint FROM key_package_claim_receipts WHERE operation_id=$1").bind(&operation).fetch_one(&d.pool).await.unwrap();
    let padding = vec![0u8; (MAX_RECEIPT_BYTES - original_bytes - 1) as usize];
    sqlx::query("INSERT INTO key_package_claim_receipts(source_identity,source_device,operation_id,targets,response,expires_at_ms) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(&actor.identity).bind(actor.device.as_deref().unwrap()).bind("00000000000000000000000001")
        .bind(vec![target.identity.clone()]).bind(&padding).bind(now() + KEY_PACKAGE_CLAIM_TTL_MS).execute(&d.pool).await.unwrap();
    assert_eq!(claim(&d.pool, &actor, &target, &operation).await, original);
    let error = key_package_claims::claim(
        &d.pool,
        &actor.identity,
        actor.device.as_deref().unwrap(),
        &next,
        std::slice::from_ref(&target.identity),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("capacity"));
    assert_eq!(
        d.stock(&target).await,
        3,
        "byte-cap rejection after DELETE must roll back consumption"
    );
    d.finish().await;
}

#[tokio::test]
async fn indexed_cleanup_deletes_at_most_a_batch_and_retains_live_receipts() {
    let d = Database::new().await;
    let actor = d.person("ana").await;
    let target = d.person("bruno").await;
    d.publish(&target).await;
    let operation = roda_types::new_ulid(now());
    let original = claim(&d.pool, &actor, &target, &operation).await;
    let empty = ServerFrame::Res {
        id: 0,
        result: Ok(Reply::KeyPackages(Vec::new())),
    }
    .encode();
    sqlx::query("INSERT INTO key_package_claim_receipts(source_identity,source_device,operation_id,targets,response,expires_at_ms)
        SELECT $1,$2,lpad(n::text,26,'0'),$3,$4,$5 FROM generate_series(1,$6) AS n")
        .bind(&actor.identity).bind(actor.device.as_deref().unwrap()).bind(vec![target.identity.clone()])
        .bind(&empty).bind(now() - 1000).bind(GC_BATCH + 7).execute(&d.pool).await.unwrap();
    assert_eq!(
        key_package_claims::prune(&d.pool).await.unwrap(),
        GC_BATCH as u64
    );
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM key_package_claim_receipts")
        .fetch_one(&d.pool)
        .await
        .unwrap();
    assert_eq!(left, 8);
    assert_eq!(key_package_claims::prune(&d.pool).await.unwrap(), 7);
    assert_eq!(key_package_claims::prune(&d.pool).await.unwrap(), 0);
    assert_eq!(claim(&d.pool, &actor, &target, &operation).await, original);
    assert_eq!(d.stock(&target).await, 3);
    d.finish().await;
}
