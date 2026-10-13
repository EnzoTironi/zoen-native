//! A link box stays until the recorded device receipts it.

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use roda_log::{Author, Signer};
use roda_proto::{
    auth_message, ClientFrame, DeviceCertificate, Envelope, InviteCreated, Op, Reply, Sequenced,
    ServerFrame, PROTOCOL_VERSION,
};
use roda_types::{Identity, IdentityKind, Role};
use sqlx::{postgres::PgPoolOptions, Connection, PgConnection, PgPool};
use std::{sync::Arc, time::Duration};
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};
use zoen_relay::{
    analytics, fanout, limits,
    log::{InviteInfo, LogStore, Reject, Sequencing, StoreError},
    metrics, AppState,
};

struct QuietLog;

#[async_trait]
impl LogStore for QuietLog {
    async fn append(&self, _: &Envelope, _: bool) -> Result<Sequencing, Reject> {
        Err(Reject::unavailable())
    }
    async fn read(&self, _: &str, _: u64, _: usize) -> Result<Vec<Sequenced>, StoreError> {
        Ok(Vec::new())
    }
    async fn role(&self, _: &str, _: &str) -> Result<Option<Role>, StoreError> {
        Ok(None)
    }
    async fn removed_at(&self, _: &str, _: &str) -> Result<Option<u64>, StoreError> {
        Ok(None)
    }
    async fn members(&self, _: &str) -> Result<Vec<(String, Role)>, StoreError> {
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
        _: Role,
        _: u32,
        _: u64,
    ) -> Result<InviteCreated, String> {
        Err("not used".into())
    }
    async fn preview_invite(&self, _: &str) -> Result<InviteInfo, String> {
        Err("not used".into())
    }
}

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

struct TestRelay {
    pool: PgPool,
    state: Arc<AppState>,
    ws_url: String,
    server: tokio::task::JoinHandle<()>,
    database: String,
    admin: PgConnection,
}

impl TestRelay {
    async fn new() -> Self {
        let admin_url = std::env::var("ZOEN_TEST_PG").expect("set ZOEN_TEST_PG");
        let database = format!(
            "zoen_t_link_receipt_{}",
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
            analytics: analytics::Analytics::load(&pool).await.unwrap(),
            pool: pool.clone(),
            session_auth: auth_pool(),
            delivery_auth: auth_pool(),
            log: Arc::new(QuietLog),
            fanout: fanout::Fanout::local("link-receipts".into()),
            limits: limits::Limits::from_spec("").unwrap(),
            client_ip_header: None,
            relay_name: "test-relay".into(),
            metrics: metrics::Metrics::default(),
            blobs: Arc::new(object_store::memory::InMemory::new()),
            backup_vault: None,
            backup_settings: Default::default(),
            apple_app_ids: Vec::new(),
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
            pool,
            state,
            ws_url: format!("ws://{address}/v1/sync"),
            server,
            database,
            admin,
        }
    }

    async fn close(mut self) {
        self.server.abort();
        self.pool.close().await;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE {} WITH (FORCE)",
            self.database
        )))
        .execute(&mut self.admin)
        .await
        .unwrap();
    }
}

async fn send(socket: &mut Socket, frame: ClientFrame) {
    socket
        .send(Message::Binary(frame.encode().into()))
        .await
        .unwrap();
}

async fn recv(socket: &mut Socket) -> Option<ServerFrame> {
    loop {
        match tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .expect("socket stalled")
        {
            Some(Ok(Message::Binary(bytes))) => return Some(ServerFrame::decode(&bytes).unwrap()),
            None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return None,
            _ => {}
        }
    }
}

async fn request(socket: &mut Socket, id: u64, op: Op) -> Result<Reply, String> {
    send(socket, ClientFrame::Req { id, op }).await;
    loop {
        match recv(socket).await.expect("caller closed") {
            ServerFrame::Res { id: got, result } if got == id => return result,
            _ => {}
        }
    }
}

async fn connect(url: &str, author: &Author) -> Socket {
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    send(
        &mut socket,
        ClientFrame::Hello {
            protocol: PROTOCOL_VERSION,
            capabilities: Vec::new(),
            identity: author.identity.clone(),
            device: author.device.clone().unwrap(),
            cert: author.cert.clone().unwrap(),
        },
    )
    .await;
    let Some(ServerFrame::Challenge { nonce, relay, .. }) = recv(&mut socket).await else {
        panic!("challenge")
    };
    send(
        &mut socket,
        ClientFrame::Auth {
            sig: author.key.sign(&auth_message(&nonce, &relay)),
        },
    )
    .await;
    assert!(matches!(
        recv(&mut socket).await,
        Some(ServerFrame::Ready { .. })
    ));
    socket
}

fn profile(author: &Author, handle: &str) -> Identity {
    Identity {
        owner_proof: None,
        id: author.identity.clone(),
        kind: IdentityKind::Person,
        name: "Ana".into(),
        handle: handle.into(),
        tint_hex: "#123456".into(),
        glyph: None,
        owner: None,
        bio: String::new(),
    }
}

fn deliver(author: &Author, id: &str) -> Op {
    Op::DeliverLink {
        id: id.into(),
        sealed: vec![7, 7],
        device: Some(DeviceCertificate {
            device: author.device.clone().unwrap(),
            cert: author.cert.clone().unwrap(),
        }),
    }
}

async fn count(pool: &PgPool, id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM link_boxes WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_link_box_is_read_twice_then_receipted_and_refused_to_anyone_else() {
    let test = TestRelay::new().await;
    let root = Signer::generate();
    let sponsor = Author::device(&root, Signer::generate());
    let next = Author::device(&root, Signer::generate());
    let mut sponsor_socket = connect(&test.ws_url, &sponsor).await;
    request(
        &mut sponsor_socket,
        1,
        Op::Register {
            profile: profile(&sponsor, "ana"),
        },
    )
    .await
    .unwrap();
    let box_id = Signer::generate().id();
    request(&mut sponsor_socket, 2, deliver(&next, &box_id))
        .await
        .unwrap();
    assert_eq!(count(&test.pool, &box_id).await, 1);

    assert_eq!(
        request(&mut sponsor_socket, 3, Op::FetchLink { id: box_id.clone() })
            .await
            .unwrap_err(),
        "not your link box"
    );
    assert_eq!(count(&test.pool, &box_id).await, 1);

    let (mut early, _) = tokio_tungstenite::connect_async(&test.ws_url)
        .await
        .unwrap();
    send(
        &mut early,
        ClientFrame::Hello {
            protocol: PROTOCOL_VERSION,
            capabilities: Vec::new(),
            identity: next.identity.clone(),
            device: next.device.clone().unwrap(),
            cert: next.cert.clone().unwrap(),
        },
    )
    .await;
    assert!(matches!(
        recv(&mut early).await,
        Some(ServerFrame::Challenge { .. })
    ));
    send(
        &mut early,
        ClientFrame::Req {
            id: 1,
            op: Op::AckLink { id: box_id.clone() },
        },
    )
    .await;
    match recv(&mut early).await {
        Some(ServerFrame::Error { message, .. }) => {
            assert!(message.contains("expected auth"), "{message}")
        }
        other => panic!("receipt before authentication was accepted: {other:?}"),
    }
    assert_eq!(count(&test.pool, &box_id).await, 1);

    let mut reader = connect(&test.ws_url, &next).await;
    assert_eq!(
        request(&mut reader, 1, Op::FetchLink { id: box_id.clone() })
            .await
            .unwrap(),
        Reply::Link(Some(vec![7, 7]))
    );
    assert_eq!(
        request(&mut reader, 2, Op::FetchLink { id: box_id.clone() })
            .await
            .unwrap(),
        Reply::Link(Some(vec![7, 7]))
    );
    assert_eq!(count(&test.pool, &box_id).await, 1);
    assert_eq!(
        request(&mut reader, 3, Op::FetchLink { id: box_id.clone() })
            .await
            .unwrap(),
        Reply::Link(Some(vec![7, 7]))
    );

    assert_eq!(
        request(&mut sponsor_socket, 4, Op::AckLink { id: box_id.clone() })
            .await
            .unwrap_err(),
        "not your link box"
    );
    assert_eq!(
        request(&mut reader, 4, Op::AckLink { id: box_id.clone() })
            .await
            .unwrap(),
        Reply::Done
    );
    assert_eq!(count(&test.pool, &box_id).await, 0);
    assert_eq!(
        request(&mut reader, 5, Op::AckLink { id: box_id.clone() })
            .await
            .unwrap(),
        Reply::Done
    );
    assert_eq!(
        request(&mut reader, 6, Op::FetchLink { id: box_id.clone() })
            .await
            .unwrap(),
        Reply::Link(None)
    );

    let expired = Signer::generate().id();
    request(&mut sponsor_socket, 5, deliver(&next, &expired))
        .await
        .unwrap();
    sqlx::query("UPDATE link_boxes SET created_at = now() - interval '16 minutes' WHERE id = $1")
        .bind(&expired)
        .execute(&test.pool)
        .await
        .unwrap();
    assert_eq!(
        request(
            &mut reader,
            7,
            Op::FetchLink {
                id: expired.clone()
            }
        )
        .await
        .unwrap_err(),
        "link expired"
    );
    assert_eq!(count(&test.pool, &expired).await, 0);

    let revoked = Signer::generate().id();
    request(&mut sponsor_socket, 6, deliver(&next, &revoked))
        .await
        .unwrap();
    assert_eq!(count(&test.pool, &revoked).await, 1);
    request(
        &mut sponsor_socket,
        7,
        Op::Unlink {
            device: next.device.clone().unwrap(),
        },
    )
    .await
    .unwrap();
    assert_eq!(count(&test.pool, &revoked).await, 0);

    let _ = reader.close(None).await;
    let _ = sponsor_socket.close(None).await;
    test.close().await;
}

#[tokio::test]
async fn an_abandoned_link_box_is_removed_by_the_periodic_sweep_alone() {
    let test = TestRelay::new().await;
    let abandoned = Signer::generate().id();
    let fresh = Signer::generate().id();
    sqlx::query("INSERT INTO link_boxes (id, sealed, device) VALUES ($1, $2, $3)")
        .bind(&abandoned)
        .bind([9u8].as_slice())
        .bind("abandoned-device")
        .execute(&test.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO link_boxes (id, sealed, device) VALUES ($1, $2, $3)")
        .bind(&fresh)
        .bind([8u8].as_slice())
        .bind("fresh-device")
        .execute(&test.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE link_boxes SET created_at = now() - interval '16 minutes' WHERE id = $1")
        .bind(&abandoned)
        .execute(&test.pool)
        .await
        .unwrap();
    assert_eq!(count(&test.pool, &abandoned).await, 1);
    let sweep = zoen_relay::db::spawn_link_box_sweep_every(&test.state, Duration::from_millis(50));
    let removed = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if count(&test.pool, &abandoned).await == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(removed.is_ok(), "periodic sweep left the abandoned box");
    assert_eq!(count(&test.pool, &fresh).await, 1);
    sweep.abort();
    test.close().await;
}
