//! Real sockets and directory transactions, with blocked reads and queued writes.

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use roda_log::{Author, Signer};
use roda_proto::{
    auth_message, ClientFrame, DeviceCertificate, Envelope, ErrorCode, InviteCreated, Op, Reply,
    Sequenced, ServerFrame, PROTOCOL_VERSION,
};
use roda_types::{Identity, IdentityKind, Role};
use sqlx::{postgres::PgPoolOptions, Connection, PgConnection};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{oneshot, Notify, Semaphore};
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};
use zoen_relay::{
    analytics, fanout, limits,
    log::{InviteInfo, LogStore, Reject, Sequencing, StoreError},
    metrics, AppState,
};

struct BlockedLog {
    entered: Notify,
    cancelled: Notify,
    reading: AtomicBool,
    release: Arc<Semaphore>,
    saved: Arc<std::sync::Mutex<Vec<Sequenced>>>,
    finished: Arc<Notify>,
}

struct Reading<'a>(&'a AtomicBool, &'a Notify);
impl Drop for Reading<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
        self.1.notify_one();
    }
}

#[async_trait]
impl LogStore for BlockedLog {
    async fn append(&self, env: &Envelope, _: bool) -> Result<Sequencing, Reject> {
        self.reading.store(true, Ordering::Release);
        let _reading = Reading(&self.reading, &self.cancelled);
        // Like the FDB sequencer, a queued append owns its work independently of the
        // response future. Revocation may cancel this reader without undoing admission.
        let (reply, answer) = oneshot::channel();
        let (release, saved, finished, env) = (
            self.release.clone(),
            self.saved.clone(),
            self.finished.clone(),
            env.clone(),
        );
        tokio::spawn(async move {
            release.acquire().await.unwrap().forget();
            let hash =
                roda_log::chain_hash(env.space(), 0, roda_types::GENESIS_PREV, &env.wire_hash());
            let ev = Sequenced {
                seq: 0,
                prev: roda_types::GENESIS_PREV.into(),
                hash,
                env,
            };
            saved.lock().unwrap().push(ev.clone());
            let _ = reply.send(Ok(Sequencing::New {
                audience: vec![ev.env.author().into()],
                ev,
                joined: None,
            }));
            finished.notify_one();
        });
        self.entered.notify_one();
        answer.await.unwrap_or_else(|_| Err(Reject::unavailable()))
    }
    async fn read(&self, _: &str, _: u64, _: usize) -> Result<Vec<Sequenced>, StoreError> {
        unreachable!("spaces_of never returns")
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
        self.reading.store(true, Ordering::Release);
        let _reading = Reading(&self.reading, &self.cancelled);
        self.entered.notify_one();
        std::future::pending().await
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

async fn authenticate(url: &str, author: &Author) -> (Socket, ServerFrame) {
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
    let result = recv(&mut socket).await.expect("authentication result");
    (socket, result)
}

async fn connect(url: &str, author: &Author) -> Socket {
    let (mut socket, result) = authenticate(url, author).await;
    assert!(matches!(result, ServerFrame::Ready { .. }), "{result:?}");
    send(&mut socket, ClientFrame::Ping).await;
    while !matches!(
        recv(&mut socket).await.expect("caller closed"),
        ServerFrame::Pong
    ) {}
    socket
}

struct TestRelay {
    state: Arc<AppState>,
    log: Arc<BlockedLog>,
    ws_url: String,
    server: tokio::task::JoinHandle<()>,
    database: String,
    admin: PgConnection,
}

impl TestRelay {
    async fn new() -> Self {
        let admin_url = std::env::var("ZOEN_TEST_PG").expect("set ZOEN_TEST_PG");
        let database = format!(
            "zoen_t_read_revoke_{}",
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
        let log = Arc::new(BlockedLog {
            entered: Notify::new(),
            cancelled: Notify::new(),
            reading: AtomicBool::new(false),
            release: Arc::new(Semaphore::new(0)),
            saved: Arc::default(),
            finished: Arc::default(),
        });
        let state = Arc::new(AppState {
            analytics: analytics::Analytics::load(&pool).await.unwrap(),
            pool,
            session_auth: auth_pool(),
            delivery_auth: auth_pool(),
            log: log.clone(),
            fanout: fanout::Fanout::local("read-revocation".into()),
            limits: limits::Limits::from_spec("").unwrap(),
            client_ip_header: None,
            relay_name: "test-relay".into(),
            metrics: metrics::Metrics::default(),
            blobs: Arc::new(object_store::memory::InMemory::new()),
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
            state,
            log,
            ws_url: format!("ws://{address}/v1/sync"),
            server,
            database,
            admin,
        }
    }

    async fn close(mut self) {
        self.server.abort();
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

async fn request(socket: &mut Socket, id: u64, op: Op) -> Result<Reply, String> {
    send(socket, ClientFrame::Req { id, op }).await;
    loop {
        match recv(socket).await.expect("caller closed") {
            ServerFrame::Res { id: got, result } if got == id => return result,
            _ => {}
        }
    }
}

fn profile(author: &Author, handle: &str) -> Identity {
    Identity {
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

fn link_op(author: &Author, id: &str) -> Op {
    Op::DeliverLink {
        id: id.into(),
        sealed: vec![1],
        device: Some(DeviceCertificate {
            device: author.device.clone().unwrap(),
            cert: author.cert.clone().unwrap(),
        }),
    }
}

async fn stalled_operation_revocation(publish: bool) {
    let test = TestRelay::new().await;
    let (state, log, ws_url) = (&test.state, &test.log, &test.ws_url);
    let root = Signer::generate();
    let first = Author::device(&root, Signer::generate());
    let lost = Author::device(&root, Signer::generate());
    let mut controller = connect(ws_url, &first).await;
    send(
        &mut controller,
        ClientFrame::Req {
            id: 1,
            op: Op::Register {
                profile: Identity {
                    id: root.id(),
                    kind: IdentityKind::Person,
                    name: "Ana".into(),
                    handle: "ana".into(),
                    tint_hex: "#123456".into(),
                    glyph: None,
                    owner: None,
                    bio: String::new(),
                },
            },
        },
    )
    .await;
    loop {
        if let ServerFrame::Res { id: 1, result } =
            recv(&mut controller).await.expect("controller closed")
        {
            result.unwrap();
            break;
        }
    }
    request(&mut controller, 3, link_op(&lost, &Signer::generate().id()))
        .await
        .unwrap();
    let mut target = connect(ws_url, &lost).await;
    let operation = if publish {
        ClientFrame::Publish {
            env: Envelope::plain(&lost.sign_event(
                "sp_before_revoke",
                "admitted-before-revocation",
                1,
                None,
                roda_types::EventBody::SpaceCreated {
                    title: "Already admitted".into(),
                    kind: roda_types::SpaceKind::Group,
                    privacy: roda_types::Privacy::Closed,
                },
            )),
        }
    } else {
        ClientFrame::Sync {
            cursors: Vec::new(),
            all: true,
        }
    };
    send(&mut target, operation).await;
    tokio::time::timeout(Duration::from_secs(3), log.entered.notified())
        .await
        .unwrap();
    assert!(log.reading.load(Ordering::Acquire));
    send(
        &mut controller,
        ClientFrame::Req {
            id: 2,
            op: Op::Unlink {
                device: lost.device.clone().unwrap(),
            },
        },
    )
    .await;
    loop {
        if let ServerFrame::Res { id: 2, result } =
            recv(&mut controller).await.expect("controller closed")
        {
            assert_eq!(result.unwrap(), Reply::Done);
            break;
        }
    }
    assert!(
        recv(&mut target).await.is_none(),
        "revoked stalled reader did not close"
    );
    tokio::time::timeout(Duration::from_secs(3), log.cancelled.notified())
        .await
        .unwrap();
    assert!(
        zoen_relay::db::device_revoked(&state.pool, lost.device.as_ref().unwrap())
            .await
            .unwrap()
    );
    assert!(
        !log.reading.load(Ordering::Acquire),
        "blocked read future survived closure"
    );
    if publish {
        assert!(log.saved.lock().unwrap().is_empty());
        log.release.add_permits(1);
        tokio::time::timeout(Duration::from_secs(3), log.finished.notified())
            .await
            .unwrap();
        assert_eq!(
            log.saved.lock().unwrap()[0].env.client_id(),
            "admitted-before-revocation"
        );
        // A key-package write admitted earlier could also finish after the deletion.
        // Future claims must never give a revoked device's retained packages out.
        assert!(zoen_relay::db::put_key_packages(
            &state.pool,
            &root.id(),
            lost.device.as_ref().unwrap(),
            &[],
            Some(b"late-key-package")
        )
        .await
        .unwrap());
        assert!(
            zoen_relay::db::claim_key_packages(&state.pool, &[root.id()])
                .await
                .unwrap()
                .is_empty()
        );
    }
    let _ = controller.close(None).await;
    test.close().await;
}

#[tokio::test]
async fn a_stalled_sync_cannot_block_online_device_revocation() {
    stalled_operation_revocation(false).await;
}

#[tokio::test]
async fn an_admitted_queued_publish_can_finish_without_blocking_revocation() {
    stalled_operation_revocation(true).await;
}

#[tokio::test]
async fn a_protocol_three_client_is_refused_before_authentication_or_linking() {
    let test = TestRelay::new().await;
    let primary = Author::device(&Signer::generate(), Signer::generate());
    let mut current = connect(&test.ws_url, &primary).await;
    request(
        &mut current,
        1,
        Op::Register {
            profile: profile(&primary, "ana"),
        },
    )
    .await
    .unwrap();
    let _ = current.close(None).await;

    // An enrolled sponsor with a valid certificate still needs the enrollment-aware
    // client. Refusal must be the first frame, before a challenge or link request.
    let (mut old, _) = tokio_tungstenite::connect_async(&test.ws_url)
        .await
        .unwrap();
    send(
        &mut old,
        ClientFrame::Hello {
            protocol: 3,
            capabilities: Vec::new(),
            identity: primary.identity.clone(),
            device: primary.device.clone().unwrap(),
            cert: primary.cert.clone().unwrap(),
        },
    )
    .await;
    match recv(&mut old).await {
        Some(ServerFrame::Error { code, message }) => {
            assert_eq!(code, ErrorCode::UpgradeRequired);
            assert!(message.contains("needs 4 or newer"), "{message}");
        }
        other => panic!("expected an upgrade error before authentication, got {other:?}"),
    }
    assert!(recv(&mut old).await.is_none());
    test.close().await;
}

#[tokio::test]
async fn an_unlinked_root_holder_cannot_return_with_a_fresh_certificate() {
    let test = TestRelay::new().await;
    let root = Signer::generate();
    let primary = Author::device(&root, Signer::generate());
    let lost = Author::device(&root, Signer::generate());
    let mut controller = connect(&test.ws_url, &primary).await;
    request(
        &mut controller,
        1,
        Op::Register {
            profile: profile(&primary, "ana"),
        },
    )
    .await
    .unwrap();
    request(&mut controller, 2, link_op(&lost, &Signer::generate().id()))
        .await
        .unwrap();
    let mut target = connect(&test.ws_url, &lost).await;
    request(
        &mut controller,
        3,
        Op::Unlink {
            device: lost.device.clone().unwrap(),
        },
    )
    .await
    .unwrap();
    // A linked device retains the root secret. Minting a valid new certificate and
    // proving the new key now succeeds cryptographically, but grants no enrollment.
    let fresh = Author::device(&root, Signer::generate());
    let (mut attempted, result) = authenticate(&test.ws_url, &fresh).await;
    assert!(
        matches!(
            result,
            ServerFrame::Error {
                code: ErrorCode::Unauthorized,
                ..
            }
        ),
        "{result:?}"
    );
    assert!(recv(&mut attempted).await.is_none());
    assert!(
        !zoen_relay::db::device_known(&test.state.pool, fresh.device.as_ref().unwrap())
            .await
            .unwrap()
    );
    while let Some(frame) = recv(&mut target).await {
        assert!(matches!(
            frame,
            ServerFrame::Error {
                code: ErrorCode::Unauthorized,
                ..
            }
        ));
    }
    // Even an active sponsor cannot turn the old device's row back on.
    let box_id = Signer::generate().id();
    assert!(request(&mut controller, 4, link_op(&lost, &box_id))
        .await
        .is_err());
    assert!(
        zoen_relay::db::device_revoked(&test.state.pool, lost.device.as_ref().unwrap())
            .await
            .unwrap()
    );
    assert!(zoen_relay::db::take_link_box(&test.state.pool, &box_id)
        .await
        .unwrap()
        .is_none());
    // The surviving enrolled device can still change its own directory profile.
    request(
        &mut controller,
        5,
        Op::Register {
            profile: profile(&primary, "ana_changed"),
        },
    )
    .await
    .unwrap();
    let handle: String = sqlx::query_scalar("SELECT handle FROM identities WHERE id = $1")
        .bind(root.id())
        .fetch_one(&test.state.pool)
        .await
        .unwrap();
    assert_eq!(handle, "ana_changed");
    let _ = controller.close(None).await;
    test.close().await;
}

#[tokio::test]
async fn a_preopened_unregistered_socket_cannot_enroll_after_registration() {
    let test = TestRelay::new().await;
    let root = Signer::generate();
    let first = Author::device(&root, Signer::generate());
    let late = Author::device(&root, Signer::generate());
    let (mut controller, first_ready) = authenticate(&test.ws_url, &first).await;
    let (mut stale, late_ready) = authenticate(&test.ws_url, &late).await;
    assert!(matches!(
        first_ready,
        ServerFrame::Ready {
            registered: false,
            ..
        }
    ));
    assert!(matches!(
        late_ready,
        ServerFrame::Ready {
            registered: false,
            ..
        }
    ));
    request(
        &mut controller,
        1,
        Op::Register {
            profile: profile(&first, "ana"),
        },
    )
    .await
    .unwrap();
    send(
        &mut stale,
        ClientFrame::Req {
            id: 2,
            op: Op::Register {
                profile: profile(&late, "hijacked"),
            },
        },
    )
    .await;
    while let Some(frame) = recv(&mut stale).await {
        assert!(matches!(
            frame,
            ServerFrame::Error {
                code: ErrorCode::Unauthorized,
                ..
            }
        ));
    }
    // This is also refused at the SQL creation boundary if identity registration wins
    // after the unregistered request's admission check, before its INSERT executes.
    let mut tx = test.state.pool.begin().await.unwrap();
    assert!(zoen_relay::db::register(
        &mut tx,
        &profile(&late, "hijacked"),
        "hijacked",
        late.device.as_ref().unwrap(),
        late.cert.as_ref().unwrap(),
        true,
    )
    .await
    .is_err());
    tx.rollback().await.unwrap();
    let mut tx = test.state.pool.begin().await.unwrap();
    assert!(zoen_relay::db::register(
        &mut tx,
        &profile(&late, "hijacked"),
        "hijacked",
        late.device.as_ref().unwrap(),
        late.cert.as_ref().unwrap(),
        false,
    )
    .await
    .is_err());
    tx.rollback().await.unwrap();
    let handle: String = sqlx::query_scalar("SELECT handle FROM identities WHERE id = $1")
        .bind(root.id())
        .fetch_one(&test.state.pool)
        .await
        .unwrap();
    assert_eq!(handle, "ana");
    assert!(
        !zoen_relay::db::device_known(&test.state.pool, late.device.as_ref().unwrap())
            .await
            .unwrap()
    );
    let _ = controller.close(None).await;
    test.close().await;
}

#[tokio::test]
async fn enrollment_and_link_box_delivery_commit_or_rollback_together() {
    let test = TestRelay::new().await;
    let root = Signer::generate();
    let first = Author::device(&root, Signer::generate());
    let next = Author::device(&root, Signer::generate());
    let mut controller = connect(&test.ws_url, &first).await;
    request(
        &mut controller,
        1,
        Op::Register {
            profile: profile(&first, "ana"),
        },
    )
    .await
    .unwrap();
    let box_id = Signer::generate().id();
    let pending = Author::device(&next.key, next.key.clone());
    let mut waiting = connect(&test.ws_url, &pending).await;
    assert_eq!(
        request(&mut waiting, 1, Op::FetchLink { id: box_id.clone() })
            .await
            .unwrap(),
        Reply::Link(None)
    );
    request(&mut controller, 2, link_op(&next, &box_id))
        .await
        .unwrap();
    // The link waiter authenticates as its own key, even after the sponsor enrolls
    // that key under the account. It must still be able to fetch the identity box.
    assert_eq!(
        request(&mut waiting, 2, Op::FetchLink { id: box_id.clone() })
            .await
            .unwrap(),
        Reply::Link(Some(vec![1]))
    );
    assert!(request(
        &mut waiting,
        3,
        Op::Register {
            profile: profile(&pending, "stolen")
        }
    )
    .await
    .is_err());
    assert!(
        !zoen_relay::db::is_registered(&test.state.pool, &pending.identity)
            .await
            .unwrap()
    );
    let _ = waiting.close(None).await;
    let mut linked = connect(&test.ws_url, &next).await;
    // Retry-safe enrollment keeps an already-active certificate, while duplicate box
    // delivery aborts the whole transaction and cannot leave a third device enrolled.
    let third = Author::device(&root, Signer::generate());
    let occupied_id = Signer::generate().id();
    request(
        &mut controller,
        3,
        Op::DeliverLink {
            id: occupied_id.clone(),
            sealed: vec![1],
            device: None,
        },
    )
    .await
    .unwrap();
    assert!(request(&mut controller, 4, link_op(&third, &occupied_id))
        .await
        .is_err());
    assert!(
        !zoen_relay::db::device_known(&test.state.pool, third.device.as_ref().unwrap())
            .await
            .unwrap()
    );
    assert_eq!(
        zoen_relay::db::take_link_box(&test.state.pool, &occupied_id)
            .await
            .unwrap(),
        Some(vec![1])
    );
    let invalid_id = Signer::generate().id();
    let bad_cert = Author::device(&Signer::generate(), Signer::generate());
    assert!(request(&mut controller, 5, link_op(&bad_cert, &invalid_id))
        .await
        .is_err());
    assert!(zoen_relay::db::take_link_box(&test.state.pool, &invalid_id)
        .await
        .unwrap()
        .is_none());
    assert!(
        !zoen_relay::db::device_known(&test.state.pool, bad_cert.device.as_ref().unwrap())
            .await
            .unwrap()
    );
    // History manifests still work without a certificate and do not enroll any key.
    let history_id = Signer::generate().id();
    request(
        &mut controller,
        6,
        Op::DeliverLink {
            id: history_id.clone(),
            sealed: vec![2],
            device: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        zoen_relay::db::take_link_box(&test.state.pool, &history_id)
            .await
            .unwrap(),
        Some(vec![2])
    );
    let mut tx = test.state.pool.begin().await.unwrap();
    zoen_relay::db::enroll_device(
        &mut tx,
        &root.id(),
        next.device.as_ref().unwrap(),
        next.cert.as_ref().unwrap(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let _ = controller.close(None).await;
    let _ = linked.close(None).await;
    test.close().await;
}

#[tokio::test]
async fn an_enrollment_retry_can_finish_while_a_backup_writer_waits_for_its_vault() {
    let mut test = TestRelay::new().await;
    let root = Signer::generate();
    let device = Author::device(&root, Signer::generate());
    let mut controller = connect(&test.ws_url, &device).await;
    request(
        &mut controller,
        1,
        Op::Register {
            profile: profile(&device, "ana"),
        },
    )
    .await
    .unwrap();
    let _ = controller.close(None).await;
    // PR33 has no backup tables. This row reproduces the authoritative vault lock,
    // while the enrollment helper and both device row fences use the real directory.
    sqlx::query("CREATE TABLE enrollment_test_vault (identity TEXT PRIMARY KEY, revision INTEGER NOT NULL DEFAULT 0)")
        .execute(&test.state.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO enrollment_test_vault (identity) VALUES ($1)")
        .bind(root.id())
        .execute(&test.state.pool)
        .await
        .unwrap();
    let last_seen: String =
        sqlx::query_scalar("SELECT last_seen::text FROM devices WHERE device = $1")
            .bind(device.device.as_ref().unwrap())
            .fetch_one(&test.state.pool)
            .await
            .unwrap();
    // One slot in the application's main pool; the other PostgreSQL connection models
    // a concurrently authorized writer and exposes any nested pool acquisition.
    let admin_url = std::env::var("ZOEN_TEST_PG").unwrap();
    let url = format!(
        "{}/{}",
        admin_url.rsplit_once('/').unwrap().0,
        test.database
    );
    let mut writing = PgConnection::connect(&url).await.unwrap();
    let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut writing)
        .await
        .unwrap();
    let mut restore = test.state.pool.begin().await.unwrap();
    let restore_pid: i32 = sqlx::query_scalar(
        "SELECT pg_backend_pid() FROM enrollment_test_vault WHERE identity = $1 FOR UPDATE",
    )
    .bind(root.id())
    .fetch_one(&mut *restore)
    .await
    .unwrap();
    let (ready, acquired) = oneshot::channel();
    let identity = root.id();
    let device_key = device.device.clone().unwrap();
    let writer = tokio::spawn(async move {
        let mut tx = writing.begin().await.unwrap();
        sqlx::query("SELECT 1 FROM devices WHERE identity = $1 AND device = $2 AND revoked_at IS NULL FOR SHARE")
            .bind(&identity)
            .bind(device_key)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        ready.send(()).unwrap();
        sqlx::query("UPDATE enrollment_test_vault SET revision = revision + 1 WHERE identity = $1")
            .bind(identity)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    });
    acquired.await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let blocked_by: Vec<i32> = sqlx::query_scalar("SELECT pg_blocking_pids($1)")
                .bind(writer_pid)
                .fetch_one(&mut test.admin)
                .await
                .unwrap();
            if blocked_by.contains(&restore_pid) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("writer did not wait for the held vault row");
    tokio::time::timeout(
        Duration::from_secs(2),
        zoen_relay::db::enroll_device(
            &mut restore,
            &root.id(),
            device.device.as_ref().unwrap(),
            device.cert.as_ref().unwrap(),
        ),
    )
    .await
    .expect("enrollment retry waited for the writer's compatible device fence")
    .unwrap();
    restore.commit().await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), writer)
        .await
        .unwrap()
        .unwrap();
    let revision: i32 =
        sqlx::query_scalar("SELECT revision FROM enrollment_test_vault WHERE identity = $1")
            .bind(root.id())
            .fetch_one(&test.state.pool)
            .await
            .unwrap();
    assert_eq!(revision, 1);
    let after: String = sqlx::query_scalar("SELECT last_seen::text FROM devices WHERE device = $1")
        .bind(device.device.as_ref().unwrap())
        .fetch_one(&test.state.pool)
        .await
        .unwrap();
    assert_eq!(
        after, last_seen,
        "an enrollment retry mutated the device row"
    );
    test.close().await;
}
