//! Real sockets and directory transactions, with blocked reads and queued writes.

use async_trait::async_trait;
use event_log::{
    profile::{
        seal_profile, sign_agreement, sign_upload, verify_agreement, verify_upload, AgreementKey,
        ProfileFields, ProfileKey,
    },
    Author, Signer,
};
use futures_util::{FutureExt, SinkExt, StreamExt};
use protocol::{
    auth_message, AgreementKeyRecord, ClientFrame, DeviceCertificate, Envelope, ErrorCode,
    InviteCreated, Op, Reply, SealedProfile, Sequenced, ServerFrame, PROTOCOL_VERSION,
};
use sqlx::{postgres::PgPoolOptions, Connection, PgConnection};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{oneshot, Notify, Semaphore};
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};
use types::{Identity, IdentityKind, Role};
use zoen_relay::{
    analytics, fanout, limits,
    log::{InviteInfo, LogStore, Reject, Sequencing, StoreError},
    metrics, AppState,
};

struct BlockedLog {
    // Every live Session retains this log through AppState. Keep the scheduling
    // lease through background socket teardown even if its fixture panics.
    _snapshot_test_lease: Arc<snapshot_test_lease::Lease>,
    managed_registry: std::sync::OnceLock<zoen_relay::managed_devices::ManagedDevices>,
    sync_spaces: std::sync::Mutex<Option<Vec<String>>>,
    sync_history: std::sync::Mutex<Option<Arc<sync_membership::History>>>,
    sync_members: std::sync::Mutex<std::collections::BTreeSet<(String, String)>>,
    block_presence: AtomicBool,
    presence_entered: Notify,
    presence_lookup: std::sync::Mutex<Option<PendingPresenceLookup>>,
    entered: Notify,
    cancelled: Notify,
    reading: AtomicBool,
    release: Arc<Semaphore>,
    saved: Arc<std::sync::Mutex<Vec<Sequenced>>>,
    finished: Arc<Notify>,
}

/// Test-owned fetch result; its producer survives a canceled Session future.
struct PendingPresenceLookup {
    identity: String,
    result: oneshot::Receiver<Vec<String>>,
    entered: oneshot::Sender<()>,
    retired: oneshot::Sender<(bool, Duration)>,
    drops: Arc<AtomicUsize>,
    continuations: Arc<AtomicUsize>,
    hub: Arc<zoen_relay::hub::Hub>,
}

struct PresenceLookupDrop {
    identity: String,
    retired: Option<oneshot::Sender<(bool, Duration)>>,
    started: tokio::time::Instant,
    drops: Arc<AtomicUsize>,
    hub: Arc<zoen_relay::hub::Hub>,
}
impl Drop for PresenceLookupDrop {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
        if let Some(retired) = self.retired.take() {
            // Include the complete lookup lifetime: a detached lookup may time out
            // after Hub removal, so offline-at-Drop alone cannot prove cancellation.
            let _ = retired.send((!self.hub.is_online(&self.identity), self.started.elapsed()));
        }
    }
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
    fn managed_device(
        &self,
        device: &str,
    ) -> Option<Arc<zoen_relay::managed_devices::ManagedDevice>> {
        self.managed_registry
            .get()
            .and_then(|registry| registry.get(device))
    }
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
            let hash = event_log::chain_hash(env.space(), 0, types::GENESIS_PREV, &env.wire_hash());
            let ev = Sequenced {
                seq: 0,
                prev: types::GENESIS_PREV.into(),
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
    async fn read(&self, _: &str, from: u64, limit: usize) -> Result<Vec<Sequenced>, StoreError> {
        let history = self.sync_history.lock().unwrap().clone();
        if let Some(history) = history {
            return Ok(history.raw(from, limit).await);
        }
        if self.sync_spaces.lock().unwrap().is_some() {
            return Ok(Vec::new());
        }
        unreachable!("spaces_of never returns")
    }
    async fn read_visible(
        &self,
        space: &str,
        who: &str,
        from: u64,
        limit: usize,
    ) -> Result<Option<zoen_relay::log::VisiblePage>, StoreError> {
        let history = self.sync_history.lock().unwrap().clone();
        if let Some(history) = history {
            return Ok(history.visible(who, from, limit).await);
        }
        let _ = (space, who);
        Ok(Some(zoen_relay::log::VisiblePage {
            events: self.read(space, from, limit).await?,
            through: None,
        }))
    }
    async fn role(&self, space: &str, identity: &str) -> Result<Option<Role>, StoreError> {
        Ok(self
            .sync_members
            .lock()
            .unwrap()
            .contains(&(space.into(), identity.into()))
            .then_some(Role::Member))
    }
    async fn removed_at(&self, _: &str, _: &str) -> Result<Option<u64>, StoreError> {
        Ok(None)
    }
    async fn members(&self, _: &str) -> Result<Vec<(String, Role)>, StoreError> {
        Ok(Vec::new())
    }
    async fn spaces_of(&self, _: &str) -> Result<Vec<String>, StoreError> {
        if let Some(spaces) = self.sync_spaces.lock().unwrap().as_ref() {
            return Ok(spaces.clone());
        }
        self.reading.store(true, Ordering::Release);
        let _reading = Reading(&self.reading, &self.cancelled);
        self.entered.notify_one();
        std::future::pending().await
    }
    async fn co_members(&self, identity: &str) -> Result<Vec<String>, StoreError> {
        let started = tokio::time::Instant::now();
        let lookup = {
            let mut pending = self.presence_lookup.lock().unwrap();
            if pending
                .as_ref()
                .is_some_and(|lookup| lookup.identity == identity)
            {
                pending.take()
            } else {
                None
            }
        };
        if let Some(lookup) = lookup {
            let _drop = PresenceLookupDrop {
                identity: lookup.identity,
                retired: Some(lookup.retired),
                started,
                drops: lookup.drops,
                hub: lookup.hub,
            };
            let _ = lookup.entered.send(());
            let result = lookup.result.await.unwrap_or_default();
            lookup.continuations.fetch_add(1, Ordering::SeqCst);
            return Ok(result);
        }
        if self.block_presence.load(Ordering::Acquire) {
            self.presence_entered.notify_one();
            std::future::pending::<()>().await;
        }
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
    async fn create_agent_invite(
        &self,
        _: &str,
        _: &str,
        _: u32,
        _: u64,
    ) -> Result<InviteCreated, String> {
        Err("not used".into())
    }
    async fn preview_invite(&self, _: &str) -> Result<InviteInfo, String> {
        Err("not used".into())
    }
    async fn redeem_agent_invite(&self, _: &str, _: &str) -> Result<InviteInfo, String> {
        Err("not used".into())
    }
    async fn revoke_agent_invite(&self, _: &str) -> Result<(), String> {
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
    recv_with_timeout(socket, Duration::from_secs(3)).await
}

async fn recv_with_timeout(socket: &mut Socket, timeout: Duration) -> Option<ServerFrame> {
    loop {
        match tokio::time::timeout(timeout, socket.next())
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
        ServerFrame::Pong { .. }
    ) {}
    socket
}

#[path = "session_revocation/snapshot_test_lease.rs"]
mod snapshot_test_lease;

struct TestRelay {
    _snapshot_test_lease: Arc<snapshot_test_lease::Lease>,
    state: Arc<AppState>,
    log: Arc<BlockedLog>,
    ws_url: String,
    server: tokio::task::JoinHandle<()>,
    database: String,
    database_url: String,
    admin: PgConnection,
}

impl TestRelay {
    async fn new() -> Self {
        Self::with_snapshot_test_lease(snapshot_test_lease::shared().await).await
    }

    async fn exclusive_snapshots() -> Self {
        Self::with_snapshot_test_lease(snapshot_test_lease::exclusive().await).await
    }

    async fn with_snapshot_test_lease(lease: snapshot_test_lease::Lease) -> Self {
        let lease = Arc::new(lease);
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
            _snapshot_test_lease: lease.clone(),
            managed_registry: Default::default(),
            sync_spaces: Default::default(),
            sync_history: Default::default(),
            sync_members: Default::default(),
            block_presence: AtomicBool::new(false),
            presence_entered: Notify::new(),
            presence_lookup: Default::default(),
            entered: Notify::new(),
            cancelled: Notify::new(),
            reading: AtomicBool::new(false),
            release: Arc::new(Semaphore::new(0)),
            saved: Arc::default(),
            finished: Arc::default(),
        });
        let state = Arc::new(AppState {
            read_snapshots: zoen_relay::read_snapshots::Resources::new(&pool),
            auth: zoen_relay::auth::Service::disabled(),
            turn: zoen_relay::turn::Service::disabled(),
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
            _snapshot_test_lease: lease,
            state,
            log,
            ws_url: format!("ws://{address}/v1/sync"),
            server,
            database,
            database_url: url,
            admin,
        }
    }

    async fn close(mut self) {
        self.server.abort();
        self.state.read_snapshots.close().await;
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

const RECEIPT_SPACE: &str = "sp_receipt_replay";
const EMPTY_RECEIPT_SPACE: &str = "sp_receipt_empty";

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

async fn assert_device_absent(test: &TestRelay, author: &Author) {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE device = $1)")
        .bind(author.device.as_deref().unwrap())
        .fetch_one(&test.state.pool)
        .await
        .unwrap();
    assert!(!exists, "device row survived refused enrollment");
}

#[path = "session_revocation/admission.rs"]
mod admission;
#[path = "session_revocation/concurrent_revocation.rs"]
mod concurrent_revocation;
#[path = "session_revocation/enrollment.rs"]
mod enrollment;
#[path = "session_revocation/mailbox.rs"]
mod mailbox;
#[path = "session_revocation/managed_admission.rs"]
mod managed_admission;
#[path = "session_revocation/presence.rs"]
mod presence;
#[path = "session_revocation/profiles.rs"]
mod profiles;
#[path = "session_revocation/receipts.rs"]
mod receipts;
#[path = "session_revocation/reconciliation.rs"]
mod reconciliation;
#[path = "session_revocation/response_loss.rs"]
mod response_loss;
#[path = "session_revocation/sync_membership.rs"]
mod sync_membership;

#[path = "session_revocation/complete_reads.rs"]
mod complete_reads;
