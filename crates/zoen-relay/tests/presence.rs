//! Presence privacy and multi-device behavior through real authenticated sockets.

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use roda_log::{Author, Signer};
use roda_proto::{
    auth_message, ClientFrame, Envelope, InviteCreated, Op, Reply, Sequenced, ServerFrame,
    PROTOCOL_VERSION,
};
use roda_types::{Identity, IdentityKind, Role};
use sqlx::{postgres::PgPoolOptions, Connection, PgConnection};
use std::{sync::Arc, time::Duration};
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};
use zoen_relay::{
    analytics, fanout, limits,
    log::{InviteInfo, LogStore, Reject, Sequencing, StoreError},
    metrics, AppState,
};

#[derive(Default)]
struct PresenceLog {
    members: std::sync::Mutex<Vec<String>>,
}
#[async_trait]
impl LogStore for PresenceLog {
    async fn append(&self, _: &Envelope, _: bool) -> Result<Sequencing, Reject> {
        Err(Reject::unavailable())
    }
    async fn read(&self, _: &str, _: u64, _: usize) -> Result<Vec<Sequenced>, StoreError> {
        Ok(vec![])
    }
    async fn role(&self, _: &str, who: &str) -> Result<Option<Role>, StoreError> {
        Ok(self
            .members
            .lock()
            .unwrap()
            .contains(&who.to_string())
            .then_some(Role::Member))
    }
    async fn removed_at(&self, _: &str, _: &str) -> Result<Option<u64>, StoreError> {
        Ok(None)
    }
    async fn members(&self, _: &str) -> Result<Vec<(String, Role)>, StoreError> {
        Ok(self
            .members
            .lock()
            .unwrap()
            .iter()
            .map(|id| (id.clone(), Role::Member))
            .collect())
    }
    async fn spaces_of(&self, _: &str) -> Result<Vec<String>, StoreError> {
        Ok(vec![])
    }
    async fn co_members(&self, who: &str) -> Result<Vec<String>, StoreError> {
        {
            let members = self.members.lock().unwrap();
            Ok(if members.iter().any(|id| id == who) {
                members
                    .iter()
                    .filter(|id| id.as_str() != who)
                    .cloned()
                    .collect()
            } else {
                vec![]
            })
        }
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
        ServerFrame::Pong { .. }
    ) {}
    socket
}

struct TestRelay {
    state: Arc<AppState>,
    log: Arc<PresenceLog>,
    ws_url: String,
    server: tokio::task::JoinHandle<()>,
    database: String,
    admin: PgConnection,
}

impl TestRelay {
    async fn new() -> Self {
        let admin_url = std::env::var("ZOEN_TEST_PG").expect("set ZOEN_TEST_PG");
        let database = format!(
            "zoen_t_presence_{}",
            &hex::encode(Signer::generate().secret())[..12]
        );
        let mut admin = PgConnection::connect(&admin_url).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {database}")))
            .execute(&mut admin)
            .await
            .unwrap();
        let url = format!("{}/{database}", admin_url.rsplit_once('/').unwrap().0);
        let pool = PgPoolOptions::new()
            .max_connections(4)
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
        let log = Arc::new(PresenceLog::default());
        let state = Arc::new(AppState {
            analytics: analytics::Analytics::load(&pool).await.unwrap(),
            pool,
            session_auth: auth_pool(),
            delivery_auth: auth_pool(),
            log: log.clone(),
            fanout: fanout::Fanout::local("presence".into()),
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

async fn presence(
    socket: &mut Socket,
    identity: &str,
    online: bool,
) -> (Option<u64>, Option<bool>) {
    loop {
        if let ServerFrame::Presence {
            identity: id,
            online: state,
            last_seen_ms,
            hide_last_seen,
            ..
        } = recv(socket).await.expect("presence stream closed")
        {
            if id == identity && state == online {
                return (last_seen_ms, hide_last_seen);
            }
        }
    }
}

#[tokio::test]
async fn privacy_is_enforced_on_disconnect_reconnect_and_sibling_devices() {
    let test = TestRelay::new().await;
    let root = Signer::generate();
    let a = Author::device(&root, Signer::generate());
    let sibling = Author::device(&root, Signer::generate());
    let b = Author::device(&Signer::generate(), Signer::generate());
    let mut first = connect(&test.ws_url, &a).await;
    assert!(request(
        &mut first,
        1,
        Op::Register {
            profile: profile(&a, "ana")
        }
    )
    .await
    .is_ok());
    assert!(request(
        &mut first,
        2,
        Op::DeliverLink {
            id: "a".repeat(64),
            sealed: vec![1],
            device: Some(roda_proto::DeviceCertificate {
                device: sibling.device.clone().unwrap(),
                cert: sibling.cert.clone().unwrap()
            })
        }
    )
    .await
    .is_ok());
    let mut peer = connect(&test.ws_url, &b).await;
    assert!(request(
        &mut peer,
        1,
        Op::Register {
            profile: profile(&b, "bia")
        }
    )
    .await
    .is_ok());
    *test.log.members.lock().unwrap() = vec![a.identity.clone(), b.identity.clone()];
    let mut second = connect(&test.ws_url, &sibling).await;
    // Existing sockets learn a newly shared relationship without reconnecting.
    let audience = vec![a.identity.clone(), b.identity.clone()];
    zoen_relay::presence::joined(&test.state, &b.identity, &audience).await;
    assert_eq!(presence(&mut peer, &a.identity, true).await, (None, None));
    assert_eq!(presence(&mut first, &b.identity, true).await, (None, None));
    // Changing policy on either device synchronizes siblings and strips peer data.
    send(
        &mut second,
        ClientFrame::SetPresencePrivacy {
            hide_last_seen: true,
        },
    )
    .await;
    assert_eq!(
        presence(&mut first, &a.identity, true).await,
        (None, Some(true))
    );
    assert_eq!(
        presence(&mut second, &a.identity, true).await,
        (None, Some(true))
    );
    assert_eq!(presence(&mut peer, &a.identity, true).await, (None, None));
    first.close(None).await.unwrap();
    // A remaining device keeps the whole account online. Ping is a frame barrier;
    // additionally wait for the closed socket's mailbox to be detached.
    tokio::time::timeout(Duration::from_secs(3), async {
        while test.state.fanout.hub.online_sessions() != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(test.state.fanout.is_online(&a.identity).await);
    send(&mut peer, ClientFrame::Ping).await;
    loop {
        match recv(&mut peer).await.unwrap() {
            ServerFrame::Presence {
                identity,
                online: false,
                ..
            } if identity == a.identity => panic!("one device disconnected an online account"),
            ServerFrame::Pong { .. } => break,
            _ => {}
        }
    }
    second.close(None).await.unwrap();
    assert_eq!(presence(&mut peer, &a.identity, false).await, (None, None));
    let stored = zoen_relay::presence::get(&test.state.pool, &a.identity)
        .await
        .unwrap();
    assert!(stored.hide_last_seen && stored.last_seen_ms.is_some());
    // Peer reconnect snapshot must not reveal the stored hidden timestamp.
    peer.close(None).await.unwrap();
    let (mut peer, ready) = authenticate(&test.ws_url, &b).await;
    assert!(matches!(ready, ServerFrame::Ready { .. }));
    assert_eq!(presence(&mut peer, &a.identity, false).await, (None, None));
    let (mut first, ready) = authenticate(&test.ws_url, &a).await;
    assert!(matches!(ready, ServerFrame::Ready { .. }));
    assert_eq!(
        presence(&mut first, &a.identity, true).await,
        (None, Some(true))
    );
    send(
        &mut first,
        ClientFrame::SetPresencePrivacy {
            hide_last_seen: false,
        },
    )
    .await;
    assert_eq!(
        presence(&mut first, &a.identity, true).await,
        (None, Some(false))
    );
    first.close(None).await.unwrap();
    let (timestamp, policy) = presence(&mut peer, &a.identity, false).await;
    assert!(timestamp.is_some());
    assert_eq!(policy, None);
    peer.close(None).await.unwrap();
    // Knowing an identity is not sufficient to query presence: no shared Space,
    // no presence snapshot (the only frame before the barrier is the self policy).
    let outsider = Author::device(&Signer::generate(), Signer::generate());
    let mut stranger = connect(&test.ws_url, &outsider).await;
    assert!(request(
        &mut stranger,
        1,
        Op::Register {
            profile: profile(&outsider, "caio")
        }
    )
    .await
    .is_ok());
    stranger.close(None).await.unwrap();
    let (mut stranger, ready) = authenticate(&test.ws_url, &outsider).await;
    assert!(matches!(ready, ServerFrame::Ready { .. }));
    send(&mut stranger, ClientFrame::Ping).await;
    loop {
        match recv(&mut stranger).await.unwrap() {
            ServerFrame::Presence { identity, .. } => assert_eq!(identity, outsider.identity),
            ServerFrame::Pong { .. } => break,
            _ => {}
        }
    }
    stranger.close(None).await.unwrap();
    test.close().await;
}

async fn publication_waits(
    pool: &sqlx::PgPool,
    task: &mut tokio::task::JoinHandle<Result<(), sqlx::Error>>,
) {
    let waiting = async {
        loop {
            let blocked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype = 'advisory' AND NOT granted AND database = (SELECT oid FROM pg_database WHERE datname = current_database()))")
                .fetch_one(pool).await.unwrap();
            if blocked {
                break;
            }
            tokio::task::yield_now().await;
        }
    };
    tokio::select! {
        result = task => panic!("publication bypassed the policy lock: {result:?}"),
        result = tokio::time::timeout(Duration::from_secs(3), waiting) => result.unwrap(),
    }
}

#[tokio::test]
async fn delayed_snapshot_and_disconnect_use_the_latest_locked_policy() {
    let test = TestRelay::new().await;
    let a = Author::device(&Signer::generate(), Signer::generate());
    let b = Author::device(&Signer::generate(), Signer::generate());
    let mut first = connect(&test.ws_url, &a).await;
    assert!(request(
        &mut first,
        1,
        Op::Register {
            profile: profile(&a, "ana")
        }
    )
    .await
    .is_ok());
    first.close(None).await.unwrap();
    // Wait for the old connection to leave before seeding the observed timestamp.
    tokio::time::timeout(Duration::from_secs(3), async {
        while test.state.fanout.is_online(&a.identity).await {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let mut peer = connect(&test.ws_url, &b).await;
    assert!(request(
        &mut peer,
        1,
        Op::Register {
            profile: profile(&b, "bia")
        }
    )
    .await
    .is_ok());
    sqlx::query("INSERT INTO presence_preferences(identity, last_seen_ms) VALUES ($1, 42) ON CONFLICT (identity) DO UPDATE SET last_seen_ms=42, hide_last_seen=false")
        .bind(&a.identity).execute(&test.state.pool).await.unwrap();
    // A publication stalled behind another policy operation must not cache a
    // public timestamp, then disclose it after a completed privacy change.
    let mut gate = test.state.pool.acquire().await.unwrap();
    sqlx::query("SELECT pg_advisory_lock(hashtextextended($1, 782651))")
        .bind(&a.identity)
        .execute(&mut *gate)
        .await
        .unwrap();
    let (state, identity, recipient) = (test.state.clone(), a.identity.clone(), b.identity.clone());
    let mut snapshot = tokio::spawn(async move {
        zoen_relay::presence::snapshot(&state, &recipient, &[identity]).await
    });
    publication_waits(&test.state.pool, &mut snapshot).await;
    sqlx::query("UPDATE presence_preferences SET hide_last_seen = true WHERE identity = $1")
        .bind(&a.identity)
        .execute(&mut *gate)
        .await
        .unwrap();
    sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1, 782651))")
        .bind(&a.identity)
        .execute(&mut *gate)
        .await
        .unwrap();
    snapshot.await.unwrap().unwrap();
    assert_eq!(presence(&mut peer, &a.identity, false).await, (None, None));

    sqlx::query("SELECT pg_advisory_lock(hashtextextended($1, 782651))")
        .bind(&a.identity)
        .execute(&mut *gate)
        .await
        .unwrap();
    let (state, identity, recipient) = (test.state.clone(), a.identity.clone(), b.identity.clone());
    let mut offline = tokio::spawn(async move {
        zoen_relay::presence::offline(&state, &identity, &[recipient]).await
    });
    publication_waits(&test.state.pool, &mut offline).await;
    sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1, 782651))")
        .bind(&a.identity)
        .execute(&mut *gate)
        .await
        .unwrap();
    offline.await.unwrap().unwrap();
    assert_eq!(presence(&mut peer, &a.identity, false).await, (None, None));
    // Cancellation after acquiring only part of a sorted snapshot audience must
    // release every session lock, never return a locked connection to the pool.
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM unnest($1::text[]) ids(id) ORDER BY hashtextextended(id, 782651)",
    )
    .bind(vec![a.identity.clone(), b.identity.clone()])
    .fetch_all(&mut *gate)
    .await
    .unwrap();
    sqlx::query("SELECT pg_advisory_lock(hashtextextended($1, 782651))")
        .bind(&ids[1])
        .execute(&mut *gate)
        .await
        .unwrap();
    let (state, recipient, audience) = (test.state.clone(), b.identity.clone(), ids.clone());
    let mut cancelled =
        tokio::spawn(
            async move { zoen_relay::presence::snapshot(&state, &recipient, &audience).await },
        );
    publication_waits(&test.state.pool, &mut cancelled).await;
    let acquired: bool =
        sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtextextended($1, 782651))")
            .bind(&ids[0])
            .fetch_one(&mut *gate)
            .await
            .unwrap();
    assert!(!acquired, "snapshot did not acquire its sorted first lock");
    cancelled.abort();
    let _ = cancelled.await;
    // Let the server finish its blocked statement, then prove the cancelled
    // connection is closed instead of being returned with partial session locks.
    sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1, 782651))")
        .bind(&ids[1])
        .execute(&mut *gate)
        .await
        .unwrap();
    tokio::time::timeout(
        Duration::from_secs(3),
        sqlx::query("SELECT pg_advisory_lock(hashtextextended($1, 782651))")
            .bind(&ids[0])
            .execute(&mut *gate),
    )
    .await
    .unwrap()
    .unwrap();
    sqlx::query("SELECT pg_advisory_unlock_all()")
        .execute(&mut *gate)
        .await
        .unwrap();
    drop(gate);
    peer.close(None).await.unwrap();
    test.close().await;
}

#[tokio::test]
async fn database_failure_still_announces_offline_without_a_timestamp() {
    let test = TestRelay::new().await;
    let a = Author::device(&Signer::generate(), Signer::generate());
    let b = Author::device(&Signer::generate(), Signer::generate());
    let mut first = connect(&test.ws_url, &a).await;
    assert!(request(
        &mut first,
        1,
        Op::Register {
            profile: profile(&a, "ana")
        }
    )
    .await
    .is_ok());
    let mut peer = connect(&test.ws_url, &b).await;
    assert!(request(
        &mut peer,
        1,
        Op::Register {
            profile: profile(&b, "bia")
        }
    )
    .await
    .is_ok());
    *test.log.members.lock().unwrap() = vec![a.identity.clone(), b.identity.clone()];
    sqlx::query("ALTER TABLE presence_preferences RENAME TO presence_unavailable")
        .execute(&test.state.pool)
        .await
        .unwrap();
    first.close(None).await.unwrap();
    assert_eq!(presence(&mut peer, &a.identity, false).await, (None, None));
    sqlx::query("ALTER TABLE presence_unavailable RENAME TO presence_preferences")
        .execute(&test.state.pool)
        .await
        .unwrap();
    peer.close(None).await.unwrap();
    test.close().await;
}

#[tokio::test]
async fn delayed_remote_frames_cannot_leak_timestamps_or_revert_self_policy() {
    let test = TestRelay::new().await;
    let a = Author::device(&Signer::generate(), Signer::generate());
    let b = Author::device(&Signer::generate(), Signer::generate());
    let mut first = connect(&test.ws_url, &a).await;
    assert!(request(
        &mut first,
        1,
        Op::Register {
            profile: profile(&a, "ana")
        }
    )
    .await
    .is_ok());
    let mut peer = connect(&test.ws_url, &b).await;
    assert!(request(
        &mut peer,
        1,
        Op::Register {
            profile: profile(&b, "bia")
        }
    )
    .await
    .is_ok());
    *test.log.members.lock().unwrap() = vec![a.identity.clone(), b.identity.clone()];
    sqlx::query("UPDATE presence_preferences SET last_seen_ms = 42 WHERE identity = $1")
        .bind(&a.identity)
        .execute(&test.state.pool)
        .await
        .unwrap();
    let stale = zoen_relay::presence::get(&test.state.pool, &a.identity)
        .await
        .unwrap();
    send(
        &mut first,
        ClientFrame::SetPresencePrivacy {
            hide_last_seen: true,
        },
    )
    .await;
    assert_eq!(
        presence(&mut first, &a.identity, true).await,
        (None, Some(true))
    );
    assert_eq!(presence(&mut peer, &a.identity, true).await, (None, None));
    let current = zoen_relay::presence::get(&test.state.pool, &a.identity)
        .await
        .unwrap();
    assert!(current.revision > stale.revision);
    // Equivalent to an old NATS publication arriving after the new one; exercise
    // the real websocket writer rather than assuming the bus preserves ordering.
    test.state.fanout.send(
        &[b.identity.clone()],
        &stale.frame(a.identity.clone(), false, false),
        None,
    );
    test.state.fanout.send(
        &[a.identity.clone()],
        &stale.frame(a.identity.clone(), true, true),
        None,
    );
    for socket in [&mut first, &mut peer] {
        send(socket, ClientFrame::Ping).await;
        loop {
            match recv(socket).await.unwrap() {
                ServerFrame::Presence {
                    identity,
                    last_seen_ms,
                    hide_last_seen,
                    ..
                } if identity == a.identity => {
                    assert!(last_seen_ms.is_none(), "delayed public timestamp disclosed");
                    assert_ne!(
                        hide_last_seen,
                        Some(false),
                        "delayed self policy reverted UI"
                    );
                    panic!("stale revision reached the client");
                }
                ServerFrame::Pong { .. } => break,
                _ => {}
            }
        }
    }
    // Final delivery holds a shared row fence until bytes are written: a policy
    // update cannot acknowledge while an old frame is still at that boundary.
    let mut pending = current.frame(a.identity.clone(), false, false);
    let fence = match zoen_relay::presence::delivery(&test.state, &b.identity, &mut pending).await {
        zoen_relay::presence::Delivery::Send(Some(tx)) => tx,
        _ => panic!("missing final delivery fence"),
    };
    let (state, identity, recipient) = (test.state.clone(), a.identity.clone(), b.identity.clone());
    let mut change = tokio::spawn(async move {
        zoen_relay::presence::set_privacy(&state, &identity, false, &[recipient]).await
    });
    let blocked = async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype = 'transactionid' AND NOT granted AND pid IN (SELECT pid FROM pg_stat_activity WHERE datname = current_database()))")
                .fetch_one(&test.state.pool).await.unwrap();
            if waiting {
                break;
            }
            tokio::task::yield_now().await;
        }
    };
    tokio::select! {
        result = &mut change => panic!("policy bypassed final delivery fence: {result:?}"),
        result = tokio::time::timeout(Duration::from_secs(3), blocked) => result.unwrap(),
    }
    drop(fence);
    change.await.unwrap().unwrap();
    assert_eq!(
        presence(&mut first, &a.identity, true).await,
        (None, Some(false))
    );
    first.close(None).await.unwrap();
    peer.close(None).await.unwrap();
    test.close().await;
}
