//! Chunked account backups: the ceiling is named before any ciphertext, a confirmed
//! piece is not sent back, and a clear SQLite header is never stored.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use axum::{body::Bytes, extract::State, http::HeaderMap};
use object_store::ObjectStoreExt;
use roda_log::{Author, Signer};
use roda_proto::{backup_message, Envelope, InviteCreated, Sequenced};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, Connection, PgConnection};
use zoen_relay::{
    analytics, fanout, limits,
    log::{InviteInfo, LogStore, Reject, Sequencing, StoreError},
    metrics, AppState,
};

const CHUNK_BYTES: usize = 8 * 1024 * 1024;
const GIB: u64 = 1024 * 1024 * 1024;
const SQLITE_HEADER: &[u8] = b"SQLite format 3\0";

struct QuietLog;

#[async_trait]
impl LogStore for QuietLog {
    async fn append(&self, _: &Envelope, _: bool) -> Result<Sequencing, Reject> {
        Err(Reject::unavailable())
    }
    async fn read(&self, _: &str, _: u64, _: usize) -> Result<Vec<Sequenced>, StoreError> {
        Ok(Vec::new())
    }
    async fn role(&self, _: &str, _: &str) -> Result<Option<roda_types::Role>, StoreError> {
        Ok(None)
    }
    async fn removed_at(&self, _: &str, _: &str) -> Result<Option<u64>, StoreError> {
        Ok(None)
    }
    async fn members(&self, _: &str) -> Result<Vec<(String, roda_types::Role)>, StoreError> {
        Ok(Vec::new())
    }
    async fn spaces_of(&self, _: &str) -> Result<Vec<String>, StoreError> {
        Ok(Vec::new())
    }
    async fn co_members(&self, _: &str) -> Result<Vec<String>, StoreError> {
        Ok(Vec::new())
    }
    async fn create_invite(
        &self,
        _: &str,
        _: &str,
        _: roda_types::Role,
        _: u32,
        _: u64,
    ) -> Result<InviteCreated, String> {
        Err("not used".into())
    }
    async fn preview_invite(&self, _: &str) -> Result<InviteInfo, String> {
        Err("not used".into())
    }
}

struct Harness {
    state: Arc<AppState>,
    author: Author,
    generation: [u8; 32],
    database: String,
    admin: PgConnection,
}

impl Harness {
    async fn open(ceiling: u64) -> Self {
        let admin_url = std::env::var("ZOEN_TEST_PG").expect("set ZOEN_TEST_PG");
        let database = format!(
            "zoen_u31_{}",
            &hex::encode(Signer::generate().secret())[..12]
        );
        let mut admin = PgConnection::connect(&admin_url).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {database}")))
            .execute(&mut admin)
            .await
            .unwrap();
        let url = format!("{}/{database}", admin_url.rsplit_once('/').unwrap().0);
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&url)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let auth = || {
            PgPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(Duration::from_secs(5))
                .connect_lazy(&url)
                .unwrap()
        };
        let settings = zoen_relay::backup::Settings {
            account_ceiling: ceiling,
            ..Default::default()
        };
        let state = Arc::new(AppState {
            analytics: analytics::Analytics::load(&pool).await.unwrap(),
            pool,
            session_auth: auth(),
            delivery_auth: auth(),
            log: Arc::new(QuietLog),
            fanout: fanout::Fanout::local("backup-chunks".into()),
            limits: limits::Limits::from_spec("").unwrap(),
            client_ip_header: None,
            relay_name: "test-relay".into(),
            metrics: metrics::Metrics::default(),
            blobs: Arc::new(object_store::memory::InMemory::new()),
            backup_vault: None,
            backup_settings: settings,
            apple_app_ids: Vec::new(),
        });
        let root = Signer::generate();
        let author = Author::device(&root, Signer::generate());
        let generation = [5u8; 32];
        let generation_hex = hex::encode(generation);
        let handle = format!("u{}", &hex::encode(Signer::generate().secret())[..10]);
        sqlx::query(
            "INSERT INTO identities (id, handle, kind, profile) VALUES ($1, $2, 'Person', $3)",
        )
        .bind(&author.identity)
        .bind(&handle)
        .bind(serde_json::json!({"id": author.identity}))
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO devices (device, identity, cert) VALUES ($1, $2, 'cert')")
            .bind(author.device.as_deref().unwrap())
            .bind(&author.identity)
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO backup_vaults (identity, mode, verifier, wrapped_key, kdf, generation)
             VALUES ($1, 'recovery_key', $2, $3, $4, $5)",
        )
        .bind(&author.identity)
        .bind(&[9u8; 32][..])
        .bind(b"wrapped".as_slice())
        .bind(serde_json::json!({"alg": "hkdf-sha256", "v": 1}))
        .bind(&generation_hex)
        .execute(&state.pool)
        .await
        .unwrap();
        Self {
            state,
            author,
            generation,
            database,
            admin,
        }
    }

    async fn close(mut self) {
        self.state.pool.close().await;
        self.state.session_auth.close().await;
        self.state.delivery_auth.close().await;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE {} WITH (FORCE)",
            self.database
        )))
        .execute(&mut self.admin)
        .await
        .unwrap();
    }

    fn headers(&self, body: &[u8]) -> HeaderMap {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let sha = hex::encode(Sha256::digest(body));
        let sig = self.author.key.sign(&backup_message(
            "test-relay",
            &self.author.identity,
            "blob",
            &sha,
            ts,
        ));
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-zoen-device",
            self.author.device.as_deref().unwrap().parse().unwrap(),
        );
        headers.insert("x-zoen-ts", ts.to_string().parse().unwrap());
        headers.insert("x-zoen-sig", sig.parse().unwrap());
        headers
    }

    async fn put(&self, body: Vec<u8>) -> (axum::http::StatusCode, Vec<u8>) {
        let headers = self.headers(&body);
        let response =
            zoen_relay::backup::put_blob(State(self.state.clone()), headers, Bytes::from(body))
                .await;
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap();
        (status, bytes.to_vec())
    }

    async fn uploads(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM backup_uploads WHERE identity = $1")
            .bind(&self.author.identity)
            .fetch_one(&self.state.pool)
            .await
            .unwrap()
    }

    async fn chunks(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM backup_chunks WHERE identity = $1")
            .bind(&self.author.identity)
            .fetch_one(&self.state.pool)
            .await
            .unwrap()
    }

    async fn blob_bytes(&self) -> Option<i64> {
        sqlx::query_scalar("SELECT blob_bytes FROM backup_vaults WHERE identity = $1")
            .bind(&self.author.identity)
            .fetch_one(&self.state.pool)
            .await
            .unwrap()
    }
}

fn plan(generation: &[u8; 32], total: u64, chunks: u32, sha: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "generation": hex::encode(generation),
        "total_bytes": total,
        "chunks": chunks,
        "sha": sha,
    }))
    .unwrap()
}

fn frame(generation: &[u8; 32], index: u32, count: u32, total: u64, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(56 + payload.len());
    body.extend_from_slice(b"ZOENBC1\0");
    body.extend_from_slice(generation);
    body.extend_from_slice(&index.to_be_bytes());
    body.extend_from_slice(&count.to_be_bytes());
    body.extend_from_slice(&total.to_be_bytes());
    body.extend_from_slice(payload);
    body
}

fn sha_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[tokio::test]
async fn migration_0035_names_the_chunk_tables() {
    let relay = Harness::open(GIB).await;
    let table: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('public.backup_uploads')::text")
            .fetch_one(&relay.state.pool)
            .await
            .unwrap();
    assert_eq!(table.as_deref(), Some("backup_uploads"));
    let chunks: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('public.backup_chunks')::text")
            .fetch_one(&relay.state.pool)
            .await
            .unwrap();
    assert_eq!(chunks.as_deref(), Some("backup_chunks"));
    relay.close().await;
}

#[tokio::test]
async fn ceiling_is_declared_before_any_chunk_is_stored() {
    let relay = Harness::open(GIB).await;
    let total = 200 * 1024 * 1024;
    let (status, body) = relay
        .put(plan(&relay.generation, total, 25, &sha_hex(&[1, 2, 3])))
        .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let answer: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(answer["ceiling"], serde_json::json!(GIB));
    assert_eq!(answer["confirmed"], serde_json::json!([]));
    assert_eq!(answer["missing"].as_array().unwrap().len(), 25);
    assert_eq!(relay.uploads().await, 1);
    assert_eq!(relay.chunks().await, 0);
    assert_eq!(relay.blob_bytes().await, None);
    relay.close().await;

    let small = Harness::open(64 * 1024 * 1024).await;
    let (status, body) = small
        .put(plan(&small.generation, total, 25, &sha_hex(&[1, 2, 3])))
        .await;
    assert_eq!(status, axum::http::StatusCode::PAYLOAD_TOO_LARGE);
    let answer: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(answer["ceiling"], serde_json::json!(64 * 1024 * 1024));
    assert_eq!(small.uploads().await, 0);
    assert_eq!(small.chunks().await, 0);
    assert_eq!(small.blob_bytes().await, None);
    small.close().await;
}

#[tokio::test]
async fn a_backup_above_one_gib_is_refused_before_any_row() {
    let relay = Harness::open(GIB).await;
    let (status, body) = relay
        .put(plan(&relay.generation, GIB + 1, 1, &sha_hex(b"too-big")))
        .await;
    assert_eq!(status, axum::http::StatusCode::PAYLOAD_TOO_LARGE);
    let answer: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(answer["ceiling"], serde_json::json!(GIB));
    assert!(!body
        .windows(SQLITE_HEADER.len())
        .any(|window| window == SQLITE_HEADER));
    assert_eq!(relay.uploads().await, 0);
    assert_eq!(relay.chunks().await, 0);
    assert_eq!(relay.blob_bytes().await, None);
    relay.close().await;
}

#[tokio::test]
async fn a_confirmed_chunk_is_not_returned_and_carries_no_sqlite_header() {
    let relay = Harness::open(GIB).await;
    let marker = b"piece-marker-u31";
    let mut first = vec![0x7Au8; CHUNK_BYTES];
    first[..marker.len()].copy_from_slice(marker);
    let second = vec![0x6Bu8; 64];
    let mut sealed = first.clone();
    sealed.extend_from_slice(&second);
    assert!(!first
        .windows(SQLITE_HEADER.len())
        .any(|window| window == SQLITE_HEADER));
    let total = sealed.len() as u64;
    let digest = sha_hex(&sealed);
    let (status, body) = relay.put(plan(&relay.generation, total, 2, &digest)).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let answer: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(answer["confirmed"], serde_json::json!([]));
    assert_eq!(answer["missing"], serde_json::json!([0, 1]));

    let (status, _) = relay
        .put(frame(&relay.generation, 0, 2, total, &first))
        .await;
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let key: String =
        sqlx::query_scalar("SELECT object_key FROM backup_chunks WHERE identity = $1 AND idx = 0")
            .bind(&relay.author.identity)
            .fetch_one(&relay.state.pool)
            .await
            .unwrap();
    let stored = relay
        .state
        .blobs
        .get(&object_store::path::Path::from(key.as_str()))
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert!(stored.windows(marker.len()).any(|window| window == marker));
    assert!(!stored
        .windows(SQLITE_HEADER.len())
        .any(|window| window == SQLITE_HEADER));

    let (status, body) = relay.put(plan(&relay.generation, total, 2, &digest)).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let answer: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(answer["confirmed"], serde_json::json!([0]));
    assert_eq!(answer["missing"], serde_json::json!([1]));
    assert!(!body.windows(marker.len()).any(|window| window == marker));

    let (status, body) = relay
        .put(frame(&relay.generation, 0, 2, total, &first))
        .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(body.is_empty());
    let again: String =
        sqlx::query_scalar("SELECT object_key FROM backup_chunks WHERE identity = $1 AND idx = 0")
            .bind(&relay.author.identity)
            .fetch_one(&relay.state.pool)
            .await
            .unwrap();
    assert_eq!(again, key);
    assert_eq!(relay.chunks().await, 1);
    assert_eq!(relay.blob_bytes().await, None);
    relay.close().await;
}

#[tokio::test]
async fn a_clear_sqlite_header_is_refused_and_not_stored() {
    let relay = Harness::open(GIB).await;
    let (status, body) = relay
        .put(frame(
            &relay.generation,
            0,
            1,
            SQLITE_HEADER.len() as u64,
            SQLITE_HEADER,
        ))
        .await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert!(String::from_utf8_lossy(&body).contains("not sealed"));
    assert_eq!(relay.uploads().await, 0);
    assert_eq!(relay.chunks().await, 0);
    assert_eq!(relay.blob_bytes().await, None);
    relay.close().await;

    let relay = Harness::open(GIB).await;
    let total = (CHUNK_BYTES + 8) as u64;
    let mut sealed = vec![0x11u8; total as usize];
    sealed[CHUNK_BYTES - 8..CHUNK_BYTES + 8].copy_from_slice(SQLITE_HEADER);
    let first = &sealed[..CHUNK_BYTES];
    let second = &sealed[CHUNK_BYTES..];
    assert!(!first
        .windows(SQLITE_HEADER.len())
        .any(|window| window == SQLITE_HEADER));
    assert!(second.len() < SQLITE_HEADER.len());
    let digest = sha_hex(&sealed);
    let (status, _) = relay.put(plan(&relay.generation, total, 2, &digest)).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let (status, _) = relay
        .put(frame(&relay.generation, 0, 2, total, first))
        .await;
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let key: String =
        sqlx::query_scalar("SELECT object_key FROM backup_chunks WHERE identity = $1 AND idx = 0")
            .bind(&relay.author.identity)
            .fetch_one(&relay.state.pool)
            .await
            .unwrap();
    let (status, body) = relay
        .put(frame(&relay.generation, 1, 2, total, second))
        .await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert!(String::from_utf8_lossy(&body).contains("not sealed"));
    assert_eq!(relay.uploads().await, 0);
    assert_eq!(relay.chunks().await, 0);
    assert_eq!(relay.blob_bytes().await, None);
    assert!(relay
        .state
        .blobs
        .get(&object_store::path::Path::from(key.as_str()))
        .await
        .is_err());
    relay.close().await;
}

#[tokio::test]
async fn a_single_zoenbg1_body_is_refused_and_not_published() {
    let relay = Harness::open(GIB).await;
    let mut body = b"ZOENBG1\0".to_vec();
    body.extend_from_slice(&relay.generation);
    body.extend_from_slice(b"sealed-but-not-chunked");
    let (status, response) = relay.put(body).await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    let text = String::from_utf8_lossy(&response);
    assert!(text.contains("chunks"), "{text}");
    assert!(!text.contains("SQLite format"));
    assert_eq!(relay.uploads().await, 0);
    assert_eq!(relay.chunks().await, 0);
    assert_eq!(relay.blob_bytes().await, None);
    relay.close().await;
}
