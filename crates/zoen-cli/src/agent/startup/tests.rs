//! Synthetic certified accounts and an ephemeral WS fixture, never a real relay/provider.
use super::*;
use client_core::{PrivacyDto, SecretVault};
use futures_util::{SinkExt, StreamExt};
use protocol::{AgentRequest, AgentStateRecord, ClientFrame, Op, Reply, Sequenced, ServerFrame};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tokio::sync::{mpsc, Notify};
use tokio_tungstenite::tungstenite::Message;

struct Home(PathBuf);
impl Home {
    fn new() -> Self {
        let path = std::env::temp_dir().join(types::new_id("zoen-host-startup"));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn marker(&self) -> PathBuf {
        self.0.join("agent-ready")
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

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
struct Citizen(Arc<Client>);
impl Drop for Citizen {
    fn drop(&mut self) {
        self.0.stop_sync();
    }
}
fn citizen(relay: &str) -> Citizen {
    let owner = Client::open(":memory:".into(), "en".into()).unwrap();
    let owner_id = owner
        .create_account(
            "Synthetic owner".into(),
            "startup_owner".into(),
            relay.into(),
            Arc::new(Vault::default()),
        )
        .unwrap()
        .identity_id;
    let root = vec![81; 32];
    let id = client_core::agent_id_for_root(root.clone()).unwrap();
    let proof = owner.authorize_agent_json(id).unwrap();
    let agent = Client::open(":memory:".into(), "en".into()).unwrap();
    agent
        .create_agent_account(
            "Synthetic host".into(),
            "startup_host".into(),
            relay.into(),
            root,
            owner_id,
            proof,
            Arc::new(Vault::default()),
        )
        .unwrap();
    Citizen(agent)
}
#[derive(Clone, Copy)]
enum Mode {
    Normal,
    Unsynced,
    MlsRefused,
    AdvanceRefused,
    HoldAdvances,
    Paged,
    PageLost,
    PageForeign,
}
struct Relay {
    url: String,
    calls: mpsc::UnboundedReceiver<AgentRequest>,
    cursors: Arc<Mutex<BTreeMap<String, u64>>>,
    publications: Arc<AtomicUsize>,
    release: Arc<Notify>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Relay {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn relay(mode: Mode) -> Relay {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, calls) = mpsc::unbounded_channel();
    let cursors = Arc::new(Mutex::new(BTreeMap::new()));
    let publications = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(Notify::new());
    let (remote, published, gate) = (cursors.clone(), publications.clone(), release.clone());
    let task = tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let (tx, remote, published, gate) =
                (tx.clone(), remote.clone(), published.clone(), gate.clone());
            tokio::spawn(async move {
                let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
                let Some(Ok(Message::Binary(bytes))) = ws.next().await else {
                    return;
                };
                let ClientFrame::Hello {
                    identity,
                    device,
                    cert,
                    ..
                } = ClientFrame::decode(&bytes).unwrap()
                else {
                    panic!("Hello");
                };
                assert!(event_log::verify_sig(
                    &identity,
                    &event_log::device_cert_message(&device),
                    &cert
                ));
                ws.send(Message::Binary(
                    ServerFrame::Challenge {
                        nonce: "host-startup".into(),
                        relay: "host-startup".into(),
                        protocol: protocol::PROTOCOL_VERSION,
                        capabilities: if matches!(
                            mode,
                            Mode::Paged | Mode::PageLost | Mode::PageForeign
                        ) {
                            vec![protocol::complete_reads::CAPABILITY.into()]
                        } else {
                            vec![]
                        },
                        server_time_ms: None,
                    }
                    .encode()
                    .into(),
                ))
                .await
                .unwrap();
                let Some(Ok(Message::Binary(bytes))) = ws.next().await else {
                    return;
                };
                let ClientFrame::Auth { sig } = ClientFrame::decode(&bytes).unwrap() else {
                    panic!("Auth");
                };
                assert!(event_log::verify_sig(
                    &device,
                    &protocol::auth_message("host-startup", "host-startup"),
                    &sig
                ));
                ws.send(Message::Binary(
                    ServerFrame::Ready {
                        identity: identity.clone(),
                        registered: true,
                    }
                    .encode()
                    .into(),
                ))
                .await
                .unwrap();
                let mut page_digest = Vec::new();
                let mut logs: BTreeMap<String, Vec<Sequenced>> = BTreeMap::new();
                while let Some(Ok(Message::Binary(bytes))) = ws.next().await {
                    let response = match ClientFrame::decode(&bytes).unwrap() {
                        ClientFrame::Sync { .. } => {
                            if matches!(mode, Mode::Unsynced) {
                                continue;
                            }
                            ServerFrame::SyncDone
                        }
                        ClientFrame::Req {
                            id,
                            op: Op::PublishKeyPackages { .. },
                        } => {
                            published.fetch_add(1, Ordering::SeqCst);
                            ServerFrame::Res {
                                id,
                                result: if matches!(mode, Mode::MlsRefused) {
                                    Err("synthetic MLS publication refusal".into())
                                } else {
                                    Ok(Reply::Done)
                                },
                            }
                        }
                        ClientFrame::Req {
                            id,
                            op: Op::CompleteRead(r),
                        } => {
                            if r.kind == protocol::complete_reads::Kind::AgentState as i32
                                && r.ordinal == 0
                            {
                                let _ = tx.send(AgentRequest::State);
                            }
                            ServerFrame::Res {
                                id,
                                result: paged::response(
                                    mode,
                                    r,
                                    &identity,
                                    &device,
                                    &mut page_digest,
                                    &remote,
                                ),
                            }
                        }
                        ClientFrame::Req {
                            id,
                            op: Op::Agent(op),
                        } => {
                            let _ = tx.send(op.clone());
                            let result = match op {
                                AgentRequest::State => Ok(Reply::AgentState(AgentStateRecord { status: "active".into(), kind: "User".into(), definition_version: 1, definition: br#"{"version":1,"kind":"user","persona":"Synthetic startup only.","rules":[],"state":{},"model_tier":"cheap"}"#.to_vec(), cursors: remote.lock().unwrap().clone().into_iter().collect() })),
                                AgentRequest::Advance { space, seq } => {
                                    if matches!(mode, Mode::HoldAdvances) { gate.notified().await; }
                                    if matches!(mode, Mode::AdvanceRefused) { Err("synthetic cursor ACK refusal".into()) } else { remote.lock().unwrap().insert(space, seq); Ok(Reply::Done) }
                                }
                                _ => panic!("startup must not define/control an Agent"),
                            };
                            ServerFrame::Res { id, result }
                        }
                        ClientFrame::Req { id, .. } => ServerFrame::Res {
                            id,
                            result: Ok(Reply::Done),
                        },
                        ClientFrame::Publish { env } => {
                            env.verify().unwrap();
                            let log = logs.entry(env.space().into()).or_default();
                            let seq = log.len() as u64;
                            let prev = log
                                .last()
                                .map(|e| e.hash.clone())
                                .unwrap_or_else(|| types::GENESIS_PREV.into());
                            let ev = Sequenced {
                                seq,
                                hash: event_log::chain_hash(
                                    env.space(),
                                    seq,
                                    &prev,
                                    &env.wire_hash(),
                                ),
                                prev,
                                env,
                            };
                            let accepted = ServerFrame::Accepted {
                                space: ev.env.space().into(),
                                client_id: ev.env.client_id().into(),
                                seq,
                            };
                            log.push(ev.clone());
                            ws.send(Message::Binary(ServerFrame::Event { ev }.encode().into()))
                                .await
                                .unwrap();
                            accepted
                        }
                        ClientFrame::Ping => ServerFrame::Pong {
                            server_time_ms: None,
                        },
                        _ => continue,
                    };
                    if ws
                        .send(Message::Binary(response.encode().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
    });
    Relay {
        url,
        calls,
        cursors,
        publications,
        release,
        task,
    }
}
async fn online(client: &Client) {
    client.start_sync(None).unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while client.connection().state != "online" {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
async fn prepared(
    client: &Client,
    readiness: &Readiness<'_>,
) -> Result<BTreeMap<String, u64>, String> {
    tokio::time::timeout(Duration::from_secs(3), prepare(client, readiness, 120))
        .await
        .unwrap()
}

#[test]
fn stale_marker_reset_is_locked_scoped_and_normal_drop_removes_fresh_marker() {
    let home = Home::new();
    fs::write(home.marker(), b"stale").unwrap();
    let retained = home.0.join("retained");
    fs::write(&retained, b"untouched").unwrap();
    let lock = HostLock::acquire(&home.0).unwrap();
    let readiness = Readiness::reset(&home.0, &lock).unwrap();
    assert!(!home.marker().exists());
    assert_eq!(fs::read(&retained).unwrap(), b"untouched");
    readiness.publish().unwrap();
    assert!(home.marker().is_file());
    assert!(
        readiness.publish().is_err(),
        "fresh creation must never replace another marker"
    );
    drop(readiness);
    assert!(!home.marker().exists());
    assert_eq!(fs::read(&retained).unwrap(), b"untouched");
}

#[test]
fn invalid_marker_directory_is_refused_without_broad_cleanup() {
    let home = Home::new();
    fs::create_dir(home.marker()).unwrap();
    let retained = home.marker().join("retained");
    fs::write(&retained, b"untouched").unwrap();
    let lock = HostLock::acquire(&home.0).unwrap();
    assert!(Readiness::reset(&home.0, &lock).is_err());
    assert_eq!(fs::read(retained).unwrap(), b"untouched");
}

#[test]
fn unavailable_timeline_is_an_error_instead_of_a_zero_cursor() {
    let citizen = citizen("http://127.0.0.1:1");
    assert!(current_head(&citizen.0, "missing-space".into()).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unsettled_sync_never_reads_agent_state_or_publishes_readiness() {
    let mut relay = relay(Mode::Unsynced).await;
    let citizen = citizen(&relay.url);
    let home = Home::new();
    let lock = HostLock::acquire(&home.0).unwrap();
    let readiness = Readiness::reset(&home.0, &lock).unwrap();
    online(&citizen.0).await;
    assert!(prepared(&citizen.0, &readiness).await.is_err());
    assert!(!citizen.0.connection().synced);
    assert!(relay.calls.try_recv().is_err());
    assert!(!home.marker().exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn online_synced_empty_outbox_with_unsettled_mls_never_becomes_ready() {
    let mut relay = relay(Mode::MlsRefused).await;
    let citizen = citizen(&relay.url);
    let home = Home::new();
    let lock = HostLock::acquire(&home.0).unwrap();
    let readiness = Readiness::reset(&home.0, &lock).unwrap();
    online(&citizen.0).await;
    // Online does not mean the background MLS publisher has run. Wait until the
    // refusal fixture receives the attempt before the bounded readiness check.
    tokio::time::timeout(Duration::from_secs(3), async {
        while relay.publications.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the fixture must observe an MLS publication attempt");
    let error = prepared(&citizen.0, &readiness).await.unwrap_err();
    assert!(
        error.contains("observed online=true synced=true pending=0"),
        "{error}"
    );
    let connection = citizen.0.connection();
    assert_eq!(connection.state, "online");
    assert!(connection.synced);
    assert_eq!(connection.pending, 0);
    assert!(relay.publications.load(Ordering::SeqCst) > 0);
    assert!(relay.calls.try_recv().is_err());
    assert!(!home.marker().exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initial_cursor_ack_refusal_cannot_publish_readiness() {
    let relay = relay(Mode::AdvanceRefused).await;
    let citizen = citizen(&relay.url);
    let space = citizen
        .0
        .create_group_with("Synthetic startup".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    citizen
        .0
        .send_message(
            space,
            "Existing task, never inferred during startup.".into(),
        )
        .unwrap();
    let home = Home::new();
    let lock = HostLock::acquire(&home.0).unwrap();
    let readiness = Readiness::reset(&home.0, &lock).unwrap();
    online(&citizen.0).await;
    assert!(prepared(&citizen.0, &readiness).await.is_err());
    assert!(relay.cursors.lock().unwrap().is_empty());
    assert!(!home.marker().exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn readiness_waits_for_all_fresh_cursor_acks_and_preserves_existing_cursor() {
    let mut relay = relay(Mode::HoldAdvances).await;
    let citizen = citizen(&relay.url);
    let existing = citizen
        .0
        .create_group_with("Existing".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let fresh = citizen
        .0
        .create_group_with("Fresh".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    citizen
        .0
        .send_message(
            fresh.clone(),
            "Existing source, never inferred during startup.".into(),
        )
        .unwrap();
    let empty = citizen
        .0
        .create_group_with("Empty".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    relay.cursors.lock().unwrap().insert(existing.clone(), 77);
    let home = Home::new();
    let lock = HostLock::acquire(&home.0).unwrap();
    let readiness = Readiness::reset(&home.0, &lock).unwrap();
    online(&citizen.0).await;
    citizen.0.wait_until_settled(2000).await.unwrap();
    let expected = BTreeMap::from([(fresh, 1), (empty, 0)]);
    let monitor = async {
        assert_eq!(relay.calls.recv().await.unwrap(), AgentRequest::State);
        for _ in 0..2 {
            let request = tokio::time::timeout(Duration::from_secs(2), relay.calls.recv())
                .await
                .unwrap()
                .unwrap();
            let AgentRequest::Advance { space, seq } = request else {
                panic!("Advance");
            };
            assert_eq!(expected.get(&space), Some(&seq));
            assert!(
                !home.marker().exists(),
                "readiness precedes no pending seed ACK"
            );
            relay.release.notify_one();
        }
    };
    let (result, _) = tokio::join!(prepared(&citizen.0, &readiness), monitor);
    let cursors = result.unwrap();
    assert_eq!(cursors.get(&existing), Some(&77));
    for (space, seq) in expected {
        assert_eq!(cursors.get(&space), Some(&seq));
    }
    assert_eq!(cursors, *relay.cursors.lock().unwrap());
    assert!(home.marker().is_file());
    assert!(relay.calls.try_recv().is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_marker_creation_failure_keeps_startup_failed_after_seed() {
    let relay = relay(Mode::Normal).await;
    let citizen = citizen(&relay.url);
    let home = Home::new();
    let lock = HostLock::acquire(&home.0).unwrap();
    let readiness = Readiness::reset(&home.0, &lock).unwrap();
    fs::create_dir(home.marker()).unwrap(); // Actual OS create_new failure, not an injected policy.
    online(&citizen.0).await;
    assert!(prepared(&citizen.0, &readiness).await.is_err());
    assert!(!home.marker().is_file());
}

mod paged;
