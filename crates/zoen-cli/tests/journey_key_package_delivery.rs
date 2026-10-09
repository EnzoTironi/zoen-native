//! Real relay regressions for low-stock delivery and replayed single-use publications.

mod common;

use common::{RawClient, World};
use futures_util::{SinkExt, StreamExt};
use roda_ffi::{AccountDto, RodaEngine, SecretVault};
use roda_proto::{ClientFrame, Op, Reply, ServerFrame};
use sqlx::{ConnectOptions, Connection, PgConnection};
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{oneshot, Notify},
    task::{JoinHandle, JoinSet},
};
use tokio_tungstenite::tungstenite::Message;

#[derive(Default)]
struct Vault(Mutex<HashMap<String, Vec<u8>>>);

impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&key).cloned()
    }

    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.0.lock().unwrap().insert(key, value);
        true
    }

    fn delete(&self, key: String) {
        self.0.lock().unwrap().remove(&key);
    }
}

struct Device {
    core: Arc<RodaEngine>,
    account: AccountDto,
}

impl Device {
    fn new(w: &World, relay: String) -> Self {
        let core = RodaEngine::open(
            w.dir.join("bruno.sqlite").to_string_lossy().into_owned(),
            "en-US".into(),
        )
        .unwrap();
        let account = core
            .create_account(
                "Bruno".into(),
                "bruno".into(),
                relay,
                Arc::new(Vault::default()),
            )
            .unwrap();
        Self { core, account }
    }

    async fn stock(&self, w: &World) -> i64 {
        w.count(&format!(
            "SELECT count(*) FROM key_packages WHERE identity = '{}' AND NOT last_resort",
            self.account.identity_id
        ))
        .await
    }

    async fn wait_stock(&self, w: &World, expected: i64) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            let stock = self.stock(w).await;
            if stock == expected {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "stock {stock}, expected {expected}; connection {:?}\n{}",
                self.core.connection(),
                w.relay_log_text()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn start(&self) {
        self.core.start_sync(None).unwrap();
        let status = self.core.wait_until_idle(8000).await;
        assert_eq!(status.state, "online", "{status:?}");
        assert!(status.synced, "{status:?}");
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        self.core.stop_sync();
    }
}

async fn claim(ana: &mut RawClient, identity: &str, count: usize) -> Vec<Vec<u8>> {
    let mut claimed = Vec::new();
    for _ in 0..count {
        let Reply::KeyPackages(mut records) = ana
            .request(Op::ClaimKeyPackages {
                ids: vec![identity.into()],
            })
            .await
            .unwrap()
        else {
            panic!("key-package claim reply")
        };
        assert_eq!(records.len(), 1);
        claimed.push(records.pop().unwrap().data);
    }
    claimed
}

struct CommitGate {
    armed: AtomicBool,
    reached: Mutex<Option<oneshot::Sender<()>>>,
    release: Notify,
}

struct PgProxy {
    database_url: String,
    gate: Arc<CommitGate>,
    reached: oneshot::Receiver<()>,
    task: JoinHandle<()>,
}

fn describes_package_hash(body: &[u8]) -> bool {
    let column = b"\0\x01package_hash\0";
    // One named column followed by the 18-byte Postgres field description; bytea OID 17.
    body.len() == column.len() + 18
        && body.starts_with(column)
        && body[column.len() + 6..column.len() + 10] == 17u32.to_be_bytes()
}

impl PgProxy {
    async fn new(database_url: &str) -> Self {
        let options = sqlx::postgres::PgConnectOptions::from_str(database_url).unwrap();
        let upstream = (options.get_host().to_string(), options.get_port());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let database_url = options
            .host("127.0.0.1")
            .port(listener.local_addr().unwrap().port())
            .ssl_mode(sqlx::postgres::PgSslMode::Disable)
            .to_url_lossy()
            .to_string();
        let (reached_tx, reached) = oneshot::channel();
        let gate = Arc::new(CommitGate {
            armed: AtomicBool::new(false),
            reached: Mutex::new(Some(reached_tx)),
            release: Notify::new(),
        });
        let task_gate = gate.clone();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (client, _) = accepted.unwrap();
                        let upstream = upstream.clone();
                        let gate = task_gate.clone();
                        connections.spawn(async move {
                            let server = tokio::net::TcpStream::connect(upstream).await.unwrap();
                            let (mut client_read, mut client_write) = client.into_split();
                            let (mut server_read, mut server_write) = server.into_split();
                            let frontend = async { tokio::io::copy(&mut client_read, &mut server_write).await };
                            let backend = async {
                                let mut receipt_column = false;
                                let mut receipt_rows = 0usize;
                                let mut publication = false;
                                loop {
                                    let mut header = [0u8; 5];
                                    if server_read.read_exact(&mut header).await.is_err() { break }
                                    let length = u32::from_be_bytes(header[1..].try_into().unwrap()) as usize;
                                    assert!(length >= 4);
                                    let mut body = vec![0; length - 4];
                                    server_read.read_exact(&mut body).await.unwrap();
                                    match header[0] {
                                        b'T' => {
                                            receipt_column = describes_package_hash(&body);
                                            receipt_rows = 0;
                                        }
                                        b'n' => {
                                            receipt_column = false;
                                            receipt_rows = 0;
                                        }
                                        b'D' if receipt_column => receipt_rows += 1,
                                        b'C' => {
                                            // Only the publication receipt INSERT returns these hashes.
                                            // Authorization and registration transactions also commit.
                                            if receipt_rows > 0 && body.starts_with(b"INSERT 0 ") {
                                                publication = true;
                                            }
                                            receipt_column = false;
                                            receipt_rows = 0;
                                            if matches!(body.as_slice(), b"BEGIN\0" | b"ROLLBACK\0") {
                                                publication = false;
                                            }
                                        }
                                        b'E' | b'Z' if header[0] == b'E' || body == b"I" => {
                                            receipt_column = false;
                                            receipt_rows = 0;
                                            publication = false;
                                        }
                                        _ => {}
                                    }
                                    // COMMIT is visible in Postgres, but the relay still awaits its
                                    // completion. Claims on other connections now precede its reply.
                                    if header[0] == b'C'
                                        && body == b"COMMIT\0"
                                        && std::mem::take(&mut publication)
                                        && gate.armed.swap(false, Ordering::SeqCst)
                                    {
                                        if let Some(tx) = gate.reached.lock().unwrap().take() { let _ = tx.send(()); }
                                        gate.release.notified().await;
                                    }
                                    if client_write.write_all(&header).await.is_err() { break }
                                    if client_write.write_all(&body).await.is_err() { break }
                                }
                            };
                            tokio::select! { _ = frontend => {}, _ = backend => {} }
                        });
                    }
                    finished = connections.join_next(), if !connections.is_empty() => {
                        finished.unwrap().unwrap();
                    }
                }
            }
        });
        Self {
            database_url,
            gate,
            reached,
            task,
        }
    }
}

impl Drop for PgProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn claims_before_the_publish_reply_are_reconciled_after_it() {
    let mut w = World::with_env(
        "kp_publish_race",
        &[("ZOEN_LIMITS", "lookup_account=1000/m:1000")],
    )
    .await;
    let mut proxy = PgProxy::new(&w.db_url).await;
    w.stop_relay();
    w.set_relay_env("DATABASE_URL", &proxy.database_url);
    w.start_relay();
    let mut ana = RawClient::connect(&w.relay_url(), "ana").await;
    let bruno = Device::new(&w, w.relay_url());
    proxy.gate.armed.store(true, Ordering::SeqCst);
    bruno.core.start_sync(None).unwrap();
    tokio::time::timeout(Duration::from_secs(10), &mut proxy.reached)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        bruno.stock(&w).await,
        32,
        "the held completion follows the committed publication"
    );
    claim(&mut ana, &bruno.account.identity_id, 25).await;
    assert_eq!(bruno.stock(&w).await, 7);
    // Claim replies follow fan-out, so Low7 is already queued before publication Done.
    proxy.gate.release.notify_one();
    bruno.wait_stock(&w, 32).await;
    assert_eq!(bruno.core.wait_until_idle(8000).await.pending, 0);
    assert_eq!(
        bruno.stock(&w).await,
        32,
        "an old Low7 must not produce a second top-up"
    );
    eprintln!("publish race: committed32, claimed25 before Done, reconciled32");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_offline_profile_update_keeps_the_registration_low_notice() {
    let w = World::with_env(
        "kp_register",
        &[("ZOEN_LIMITS", "lookup_account=1000/m:1000")],
    )
    .await;
    let bruno = Device::new(&w, w.relay_url());
    bruno.start().await;
    bruno.wait_stock(&w, 32).await;
    bruno.core.stop_sync();
    let mut ana = RawClient::connect(&w.relay_url(), "ana").await;
    claim(&mut ana, &bruno.account.identity_id, 25).await;
    assert_eq!(bruno.stock(&w).await, 7);
    let account = bruno
        .core
        .update_profile("Bruno".into(), "bruno_novo".into(), "new bio".into())
        .unwrap();
    assert!(!account.registered);
    bruno.start().await;
    bruno.wait_stock(&w, 32).await;
    assert!(bruno.core.account().unwrap().registered);
    assert_eq!(
        w.scalar(&format!(
            "SELECT handle FROM identities WHERE id = '{}'",
            bruno.account.identity_id
        ))
        .await,
        "bruno_novo"
    );
    eprintln!("registration race: offline stock7, changed handle, registered and refilled32");
}

struct RetryProxy {
    relay_url: String,
    first: oneshot::Receiver<Vec<Vec<u8>>>,
    retried: oneshot::Receiver<Vec<Vec<u8>>>,
    release: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

impl RetryProxy {
    async fn new(relay_port: u16) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay_url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let (first_tx, first) = oneshot::channel();
        let (retried_tx, retried) = oneshot::channel();
        let (release, mut release_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let client = tokio_tungstenite::accept_async(socket).await.unwrap();
            let (server, _) =
                tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{relay_port}/v1/sync"))
                    .await
                    .unwrap();
            let (mut to_client, mut from_client) = client.split();
            let (mut to_relay, mut from_relay) = server.split();
            let mut first_tx = Some(first_tx);
            let mut retried_tx = Some(retried_tx);
            let mut requests = Vec::new();
            let mut held = Vec::new();
            let mut holding = false;
            loop {
                tokio::select! {
                    frame = from_client.next() => {
                        let Some(Ok(frame)) = frame else { break };
                        if let Message::Binary(bytes) = &frame {
                            if let Ok(ClientFrame::Req { id, op: Op::PublishKeyPackages { packages, .. } }) = ClientFrame::decode(bytes) {
                                requests.push((id, packages.clone()));
                                if let Some(tx) = first_tx.take() { let _ = tx.send(packages); }
                            }
                        }
                        if to_relay.send(frame).await.is_err() { break }
                    }
                    frame = from_relay.next() => {
                        let Some(Ok(frame)) = frame else { break };
                        if let Message::Binary(bytes) = &frame {
                            if let Ok(ServerFrame::Res { id, result: Ok(Reply::Done) }) = ServerFrame::decode(bytes) {
                                if requests.first().is_some_and(|(req, _)| *req == id) {
                                    // Lose the first acknowledgment. The real 30 s client deadline
                                    // must resend its retained batch, rather than a test-made retry.
                                    continue;
                                }
                                if requests.get(1).is_some_and(|(req, _)| *req == id) {
                                    holding = true;
                                    if let Some(tx) = retried_tx.take() { let _ = tx.send(requests[1].1.clone()); }
                                }
                            }
                        }
                        if holding { held.push(frame); }
                        else if to_client.send(frame).await.is_err() { break }
                    }
                    _ = &mut release_rx, if holding => {
                        holding = false;
                        for frame in held.drain(..) {
                            if to_client.send(frame).await.is_err() { return }
                        }
                    }
                }
            }
        });
        Self {
            relay_url,
            first,
            retried,
            release: Some(release),
            task,
        }
    }
}

impl Drop for RetryProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lost_publish_ack_does_not_reoffer_consumed_single_use_packages() {
    let w = World::with_env("kp_retry", &[("ZOEN_LIMITS", "lookup_account=1000/m:1000")]).await;
    let mut proxy = RetryProxy::new(w.port).await;
    let mut ana = RawClient::connect(&w.relay_url(), "ana").await;
    let bruno = Device::new(&w, proxy.relay_url.clone());
    bruno.core.start_sync(None).unwrap();
    let first = tokio::time::timeout(Duration::from_secs(10), &mut proxy.first)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.len(), 32);
    bruno.wait_stock(&w, 32).await;
    let consumed = claim(&mut ana, &bruno.account.identity_id, 32).await;
    assert_eq!(bruno.stock(&w).await, 0);
    let retried = tokio::time::timeout(Duration::from_secs(35), &mut proxy.retried)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retried, first, "the client must retry its original bytes");
    assert_eq!(
        bruno.stock(&w).await,
        0,
        "a committed retry must not reinsert consumed packages"
    );
    proxy.release.take().unwrap().send(()).unwrap();
    bruno.wait_stock(&w, 32).await;
    let mut connection = PgConnection::connect(&w.db_url).await.unwrap();
    let old: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM key_packages WHERE identity = $1 AND NOT last_resort AND data = ANY($2::bytea[])",
    )
    .bind(&bruno.account.identity_id)
    .bind(&consumed)
    .fetch_one(&mut connection)
    .await
    .unwrap();
    assert_eq!(
        old, 0,
        "every replacement package must have a fresh private key"
    );
    let distinct = w.count(&format!("SELECT count(DISTINCT data) FROM key_packages WHERE identity = '{}' AND NOT last_resort", bruno.account.identity_id)).await;
    assert_eq!(distinct, 32);
    eprintln!("lost ack: actual deadline retry preserved consumed stock0, then generated32 fresh packages");
}
