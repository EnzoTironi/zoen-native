//! Real sockets and directory transactions, with blocked reads and queued writes.

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use roda_log::{Author, Signer};
use roda_proto::{
    auth_message, ClientFrame, Envelope, InviteCreated, Op, Reply, Sequenced, ServerFrame,
    PROTOCOL_VERSION,
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
    metrics, ownership, AppState,
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

async fn stalled_operation_revocation(publish: bool) {
    let admin_url = std::env::var("ZOEN_TEST_PG").expect("set ZOEN_TEST_PG");
    let database = format!(
        "zoen_t_read_revoke_{}",
        hex::encode(Signer::generate().secret())[..12].to_string()
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
        backup_vault: None,
        apple_app_ids: Vec::new(),
        owner: ownership::NodeOwner::claim_all("read-revocation"),
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
    let root = Signer::generate();
    let first = Author::device(&root, Signer::generate());
    let lost = Author::device(&root, Signer::generate());
    let ws_url = format!("ws://{address}/v1/sync");
    let mut controller = connect(&ws_url, &first).await;
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
    let mut target = connect(&ws_url, &lost).await;
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
    server.abort();
    state.pool.close().await;
    state.session_auth.close().await;
    state.delivery_auth.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE {database} WITH (FORCE)"
    )))
    .execute(&mut admin)
    .await
    .unwrap();
}

#[tokio::test]
async fn a_stalled_sync_cannot_block_online_device_revocation() {
    stalled_operation_revocation(false).await;
}

#[tokio::test]
async fn an_admitted_queued_publish_can_finish_without_blocking_revocation() {
    stalled_operation_revocation(true).await;
}
