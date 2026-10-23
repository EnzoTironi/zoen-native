//! Signed HTTP admission uses the durable device row, even when the device keeps its key.

use async_trait::async_trait;
use event_log::{Author, Signer};
use protocol::{
    blob_put_message,
    experiments::{report_message, unit_for, ClientReport, Health},
    transfer_message, Envelope, InviteCreated, Sequenced,
};
use reqwest::{Method, Response, StatusCode};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, Connection, PgConnection};
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use types::{Identity, IdentityKind, Role};
use zoen_relay::{
    analytics, db, fanout, limits,
    log::{InviteInfo, LogStore, Reject, Sequencing, StoreError},
    metrics, AppState,
};

// These HTTP endpoints must not touch the event log or need a FoundationDB server.
struct UnusedLog;

#[async_trait]
impl LogStore for UnusedLog {
    async fn append(&self, _: &Envelope, _: bool) -> Result<Sequencing, Reject> {
        unreachable!("HTTP device admission does not use the log")
    }
    async fn read(&self, _: &str, _: u64, _: usize) -> Result<Vec<Sequenced>, StoreError> {
        unreachable!()
    }
    async fn read_visible(
        &self,
        space: &str,
        who: &str,
        from: u64,
        limit: usize,
    ) -> Result<Option<zoen_relay::log::VisiblePage>, StoreError> {
        let _ = (space, who, from, limit);
        unreachable!("HTTP device admission does not read history")
    }
    async fn role(&self, _: &str, _: &str) -> Result<Option<Role>, StoreError> {
        unreachable!()
    }
    async fn removed_at(&self, _: &str, _: &str) -> Result<Option<u64>, StoreError> {
        unreachable!()
    }
    async fn members(&self, _: &str) -> Result<Vec<(String, Role)>, StoreError> {
        unreachable!()
    }
    async fn spaces_of(&self, _: &str) -> Result<Vec<String>, StoreError> {
        unreachable!()
    }
    async fn co_members(&self, _: &str) -> Result<Vec<String>, StoreError> {
        unreachable!()
    }
    async fn create_invite(
        &self,
        _: &str,
        _: &str,
        _: Role,
        _: u32,
        _: u64,
    ) -> Result<InviteCreated, String> {
        unreachable!()
    }
    async fn create_agent_invite(
        &self,
        _: &str,
        _: &str,
        _: u32,
        _: u64,
    ) -> Result<InviteCreated, String> {
        unreachable!()
    }
    async fn preview_invite(&self, _: &str) -> Result<InviteInfo, String> {
        unreachable!()
    }
    async fn redeem_agent_invite(&self, _: &str, _: &str) -> Result<InviteInfo, String> {
        unreachable!()
    }
    async fn revoke_agent_invite(&self, _: &str) -> Result<(), String> {
        unreachable!()
    }
}

struct TestRelay {
    state: Arc<AppState>,
    url: String,
    client: reqwest::Client,
    server: tokio::task::JoinHandle<()>,
    database: String,
    admin: PgConnection,
}

impl TestRelay {
    async fn new() -> Self {
        Self::with_store(Arc::new(object_store::memory::InMemory::new())).await
    }

    async fn with_store(blobs: Arc<dyn object_store::ObjectStore>) -> Self {
        Self::configured(blobs, "").await
    }

    async fn configured(blobs: Arc<dyn object_store::ObjectStore>, limit_spec: &str) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let admin_url = std::env::var("ZOEN_TEST_PG").expect("set ZOEN_TEST_PG");
        let database = format!(
            "zoen_t_http_revoke_{}",
            &hex::encode(Signer::generate().secret())[..12]
        );
        let mut admin = PgConnection::connect(&admin_url).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {database}")))
            .execute(&mut admin)
            .await
            .unwrap();
        let url = format!("{}/{database}", admin_url.rsplit_once('/').unwrap().0);
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(2))
            .connect(&url)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let auth_pool = || {
            PgPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(Duration::from_secs(2))
                .connect_lazy(&url)
                .unwrap()
        };
        let state = Arc::new(AppState {
            auth: zoen_relay::auth::Service::disabled(),
            turn: zoen_relay::turn::Service::disabled(),
            analytics: analytics::Analytics::load(&pool).await.unwrap(),
            pool,
            session_auth: auth_pool(),
            delivery_auth: auth_pool(),
            log: Arc::new(UnusedLog),
            fanout: fanout::Fanout::local("http-revocation".into()),
            limits: limits::Limits::from_spec(limit_spec).unwrap(),
            client_ip_header: None,
            relay_name: "test-relay".into(),
            metrics: metrics::Metrics::default(),
            blobs,
            backup_vault: None,
            backup_settings: Default::default(),
            apple_app_ids: Vec::new(),
            android_packages: Vec::new(),
            android_store_url: None,
            push: Arc::new(zoen_relay::push::Pusher::disabled()),
            receipts: Default::default(),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = zoen_relay::router(state.clone());
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });
        Self {
            state,
            url: format!("http://{address}"),
            client: reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap(),
            server,
            database,
            admin,
        }
    }

    async fn register(&self) -> Author {
        let root = Signer::generate();
        let author = Author::device(&root, Signer::generate());
        let profile = Identity {
            owner_proof: None,
            id: root.id(),
            kind: IdentityKind::Person,
            name: "Ana".into(),
            handle: "ana".into(),
            tint_hex: "#123456".into(),
            glyph: None,
            owner: None,
            bio: String::new(),
        };
        let mut tx = self.state.pool.begin().await.unwrap();
        db::register(
            &mut tx,
            &profile,
            &profile.handle,
            author.device.as_deref().unwrap(),
            author.cert.as_deref().unwrap(),
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        author
    }

    async fn signed(
        &self,
        author: &Author,
        method: Method,
        path: &str,
        body: &[u8],
        message: impl FnOnce(i64) -> Vec<u8>,
    ) -> Response {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        self.client
            .request(method, format!("{}{path}", self.url))
            .header("origin", "https://app.tryzoen.com")
            .header("x-zoen-device", author.device.as_deref().unwrap())
            .header("x-zoen-ts", ts.to_string())
            .header("x-zoen-sig", author.key.sign(&message(ts)))
            .body(body.to_vec())
            .send()
            .await
            .unwrap()
    }

    async fn close(mut self) {
        self.server.abort();
        let _ = self.server.await;
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
}

#[path = "http_device_revocation/media_fixture.rs"]
mod media_fixture;
#[path = "http_device_revocation/transfer_fixture.rs"]
mod transfer_fixture;

#[tokio::test]
async fn provider_auth_failures_keep_wire_contract_and_retry_policy() {
    let test = TestRelay::new().await;
    // No provider is configured in this fixture. The HTTP boundary still applies
    // privacy headers and admission limits before any provider or database work.
    for attempt in 0..11 {
        let response = test
            .client
            .post(format!("{}/v1/auth/challenge", test.url))
            .header("content-type", "application/json")
            .body(serde_json::json!({"provider":"google","client":"ios"}).to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.headers()["pragma"], "no-cache");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        let expected = if attempt < 10 {
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            assert!(response.headers().get("retry-after").is_none());
            "provider_not_configured"
        } else {
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
            assert!(
                response.headers()["retry-after"]
                    .to_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap()
                    >= 1
            );
            "rate_limited"
        };
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&response.bytes().await.unwrap()).unwrap(),
            serde_json::json!({"error":expected})
        );
    }
    let malformed = test.client
        .post(format!("{}/v1/auth/verify", test.url))
        .header("content-type", "application/json")
        .body(serde_json::json!({"challenge_id":"bad","state":"bad","id_token":"must-not-be-reflected"}).to_string())
        .send().await.unwrap();
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&malformed.bytes().await.unwrap()).unwrap(),
        serde_json::json!({"error":"invalid_authentication"})
    );
    test.close().await;
}

#[tokio::test]
async fn revoked_device_cannot_upload_or_delete_or_report_with_valid_signatures() {
    let test = TestRelay::new().await;
    let author = test.register().await;
    let transfer = Signer::generate().id();

    for (path, method) in [
        (format!("/v1/transfer/{transfer}/0"), "PUT"),
        (format!("/v1/transfer/{transfer}"), "DELETE"),
    ] {
        let preflight = test
            .client
            .request(Method::OPTIONS, format!("{}{path}", test.url))
            .header("origin", "https://app.tryzoen.com")
            .header("access-control-request-method", method)
            .header(
                "access-control-request-headers",
                "content-type,x-zoen-device,x-zoen-ts,x-zoen-sig",
            )
            .send()
            .await
            .unwrap();
        assert_eq!(preflight.status(), StatusCode::OK);
        assert_eq!(preflight.headers()["access-control-allow-origin"], "*");
        assert!(preflight.headers()["access-control-allow-methods"]
            .to_str()
            .unwrap()
            .contains(method));
        assert!(preflight
            .headers()
            .get("access-control-allow-credentials")
            .is_none());
    }

    for revoked in [false, true] {
        if revoked {
            // The production Unlink operation calls this same transactional path.
            let mut tx = test.state.pool.begin().await.unwrap();
            assert!(db::revoke_device(
                &mut tx,
                &author.identity,
                author.device.as_deref().unwrap(),
            )
            .await
            .unwrap());
            tx.commit().await.unwrap();
        }
        // Different bytes and chunk numbers ensure this is fresh admission, not a
        // replay of an already stored object. Every request gets a fresh signature.
        let body = if revoked {
            b"ciphertext after revocation".as_slice()
        } else {
            b"ciphertext before revocation".as_slice()
        };
        let sha = hex::encode(Sha256::digest(body));
        let blob = test
            .signed(
                &author,
                Method::PUT,
                &format!("/v1/blobs/{sha}"),
                body,
                |ts| blob_put_message(&sha, ts, &test.state.relay_name),
            )
            .await;
        let n = if revoked { "1" } else { "0" };
        let chunk = test
            .signed(
                &author,
                Method::PUT,
                &format!("/v1/transfer/{transfer}/{n}"),
                body,
                |ts| transfer_message("put", &transfer, n, &sha, ts),
            )
            .await;
        let report_body = serde_json::to_vec(&ClientReport {
            unit: unit_for(&author.identity),
            health: Some(Health {
                sessions: if revoked { 2 } else { 1 },
                crashes: 0,
            }),
            ..Default::default()
        })
        .unwrap();
        let report_sha = hex::encode(Sha256::digest(&report_body));
        let report = test
            .signed(&author, Method::POST, "/v1/report", &report_body, |ts| {
                report_message(&report_sha, ts, &test.state.relay_name)
            })
            .await;
        // Delete another transfer while active, leaving chunk 0 to check below.
        let delete_id = if revoked {
            transfer.clone()
        } else {
            Signer::generate().id()
        };
        let delete = test
            .signed(
                &author,
                Method::DELETE,
                &format!("/v1/transfer/{delete_id}"),
                &[],
                |ts| transfer_message("delete", &delete_id, "", "", ts),
            )
            .await;
        for (endpoint, response, active_status) in [
            ("blob PUT", blob, StatusCode::CREATED),
            ("transfer PUT", chunk, StatusCode::CREATED),
            ("report POST", report, StatusCode::OK),
            ("transfer DELETE", delete, StatusCode::NO_CONTENT),
        ] {
            if endpoint.starts_with("transfer") {
                assert_eq!(response.headers()["access-control-allow-origin"], "*");
            }
            assert_eq!(
                response.status(),
                if revoked {
                    StatusCode::FORBIDDEN
                } else {
                    active_status
                },
                "{endpoint}, revoked={revoked}"
            );
            if revoked {
                assert_eq!(response.text().await.unwrap(), "unlinked device");
            }
        }
    }
    let retained = test
        .client
        .get(format!("{}/v1/transfer/{transfer}/0", test.url))
        .header("origin", "https://app.tryzoen.com")
        .send()
        .await
        .unwrap();
    assert_eq!(retained.status(), StatusCode::OK);
    assert_eq!(retained.headers()["access-control-allow-origin"], "*");
    assert_eq!(
        retained.bytes().await.unwrap().as_ref(),
        b"ciphertext before revocation"
    );
    test.close().await;
}
