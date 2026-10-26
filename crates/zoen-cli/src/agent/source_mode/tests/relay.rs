//! Certified synthetic loopback only; records forbidden cursor and read effects.
use client_core::{Client, SecretVault};
use futures_util::{SinkExt, StreamExt};
use protocol::{AgentRequest, AgentStateRecord, ClientFrame, Op, Reply, Sequenced, ServerFrame};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Notify;
use tokio_tungstenite::tungstenite::Message;
mod auth;
pub(super) use auth::AuthGate;

#[derive(Default)]
pub(super) struct Vault(Mutex<HashMap<String, Vec<u8>>>);
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

#[derive(Clone, Copy)]
pub(super) enum Mode {
    Normal,
    Paused,
    Revoked,
    CursorAck,
    Unsynced,
    InvalidStatus,
    InvalidKind,
    DefinitionKindMismatch,
    DefinitionVersionMismatch,
}
pub(super) struct Relay {
    pub url: String,
    pub auth: Arc<AuthGate>,
    pub calls: Arc<Mutex<Vec<&'static str>>>,
    pub cursors: Arc<Mutex<Vec<(String, u64)>>>,
    pub delivered: Arc<Mutex<Option<(String, String, u64)>>>,
    pub released: Arc<AtomicBool>,
    pub release: Arc<Notify>,
    pub packages: Arc<Mutex<BTreeMap<String, Vec<protocol::KeyPackageRecord>>>>,
    pub histories: Arc<Mutex<BTreeMap<String, Vec<Sequenced>>>>,
    pub uploads: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Relay {
    fn drop(&mut self) {
        self.task.abort();
    }
}
pub(super) async fn relay(mode: Mode) -> Relay {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let auth = Arc::new(AuthGate::default());
    let authentication = auth.clone();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let cursors = Arc::new(Mutex::new(Vec::<(String, u64)>::new()));
    let delivered = Arc::new(Mutex::new(None::<(String, String, u64)>));
    let released = Arc::new(AtomicBool::new(!matches!(mode, Mode::Unsynced)));
    let release = Arc::new(Notify::new());
    let packages = Arc::new(Mutex::new(BTreeMap::<
        String,
        Vec<protocol::KeyPackageRecord>,
    >::new()));
    let claims = Arc::new(Mutex::new(BTreeMap::<
        (String, String),
        Vec<protocol::KeyPackageRecord>,
    >::new()));
    let (published_packages, claim_receipts) = (packages.clone(), claims.clone());
    let histories = Arc::new(Mutex::new(BTreeMap::<String, Vec<Sequenced>>::new()));
    let uploads = Arc::new(Mutex::new(BTreeMap::<String, Vec<u8>>::new()));
    let devices = Arc::new(Mutex::new(BTreeSet::<String>::new()));
    let uploaded = uploads.clone();
    let (release_state, release_signal, history) =
        (released.clone(), release.clone(), histories.clone());
    let (seen, positions, receipts) = (calls.clone(), cursors.clone(), delivered.clone());
    let task = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let (seen, positions, receipts) = (seen.clone(), positions.clone(), receipts.clone());
            let (release_state, release_signal, history) = (
                release_state.clone(),
                release_signal.clone(),
                history.clone(),
            );
            let (published_packages, claim_receipts) =
                (published_packages.clone(), claim_receipts.clone());
            let (uploaded, devices) = (uploaded.clone(), devices.clone());
            let authentication = authentication.clone();
            tokio::spawn(async move {
                let mut first = [0];
                if socket.peek(&mut first).await.unwrap() == 0 {
                    return Ok(());
                }
                if first[0] == b'P' {
                    blob_put(&mut socket, &devices, &uploaded).await;
                    seen.lock().unwrap().push("blob_upload");
                    return Ok(());
                }
                let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
                let Some(Ok(Message::Binary(bytes))) = ws.next().await else {
                    return Ok(());
                };
                let ClientFrame::Hello {
                    identity,
                    device,
                    cert,
                    ..
                } = ClientFrame::decode(&bytes).unwrap()
                else {
                    panic!("Hello")
                };
                assert!(event_log::verify_sig(
                    &identity,
                    &event_log::device_cert_message(&device),
                    &cert
                ));
                ws.send(Message::Binary(
                    ServerFrame::Challenge {
                        nonce: "source-fixture".into(),
                        relay: "source-fixture".into(),
                        protocol: protocol::PROTOCOL_VERSION,
                        capabilities: vec![protocol::KEY_PACKAGE_CLAIM_CAPABILITY.into()],
                        server_time_ms: Some(now_ms()),
                    }
                    .encode()
                    .into(),
                ))
                .await
                .unwrap();
                let Some(Ok(Message::Binary(bytes))) = ws.next().await else {
                    return Ok(());
                };
                let ClientFrame::Auth { sig } = ClientFrame::decode(&bytes).unwrap() else {
                    panic!("Auth")
                };
                assert!(event_log::verify_sig(
                    &device,
                    &protocol::auth_message("source-fixture", "source-fixture"),
                    &sig
                ));
                authentication.before_ready().await;
                devices.lock().unwrap().insert(device.clone());
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
                loop {
                    let message = tokio::select! {
                        message = ws.next() => message,
                        _ = release_signal.notified() => {
                            ws.send(Message::Binary(ServerFrame::SyncDone.encode().into())).await.unwrap();
                            continue;
                        }
                    };
                    let Some(Ok(Message::Binary(bytes))) = message else {
                        break;
                    };
                    let response = match ClientFrame::decode(&bytes).unwrap() {
                        ClientFrame::Sync { .. } => {
                            seen.lock().unwrap().push("sync");
                            if !release_state.load(Ordering::SeqCst) {
                                continue;
                            }
                            let delivered = receipts.lock().unwrap().take();
                            if let Some((space, from, seq)) = delivered {
                                ws.send(Message::Binary(
                                    ServerFrame::Ephemeral {
                                        space,
                                        from,
                                        kind: protocol::EphemeralKind::Delivered { seq },
                                    }
                                    .encode()
                                    .into(),
                                ))
                                .await
                                .unwrap();
                            }
                            ServerFrame::SyncDone
                        }
                        ClientFrame::Req {
                            id,
                            op:
                                Op::PublishKeyPackages {
                                    packages,
                                    last_resort,
                                },
                        } => {
                            seen.lock().unwrap().push("mls_publication");
                            let records = packages
                                .into_iter()
                                .chain(last_resort)
                                .map(|data| protocol::KeyPackageRecord {
                                    identity: identity.clone(),
                                    device: device.clone(),
                                    data,
                                })
                                .collect();
                            published_packages
                                .lock()
                                .unwrap()
                                .insert(identity.clone(), records);
                            ServerFrame::Res {
                                id,
                                result: Ok(Reply::Done),
                            }
                        }
                        ClientFrame::Req {
                            id,
                            op: Op::ClaimKeyPackages { ids, operation_id },
                        } => {
                            seen.lock().unwrap().push("mls_claim");
                            let operation_id =
                                operation_id.expect("new fixture claims must be durable");
                            assert!(protocol::key_package_claim_time_ms(&operation_id).is_some());
                            let key = (identity.clone(), operation_id);
                            let records = claim_receipts
                                .lock()
                                .unwrap()
                                .entry(key)
                                .or_insert_with(|| {
                                    let mut packages = published_packages.lock().unwrap();
                                    ids.into_iter()
                                        .filter_map(|id| packages.get_mut(&id).and_then(Vec::pop))
                                        .collect()
                                })
                                .clone();
                            ServerFrame::Res {
                                id,
                                result: Ok(Reply::KeyPackages(records)),
                            }
                        }
                        ClientFrame::Req {
                            id,
                            op: Op::Agent(AgentRequest::State),
                        } => {
                            seen.lock().unwrap().push("agent_state");
                            let result = Ok(Reply::AgentState(AgentStateRecord {
                                status: match mode { Mode::InvalidStatus => "future-status", Mode::Paused => "paused", Mode::Revoked => "revoked", _ => "active" }.into(),
                                kind: if matches!(mode, Mode::InvalidKind) { "future-kind" } else if matches!(mode, Mode::DefinitionKindMismatch) { "Creator" } else { "User" }.into(),
                                definition_version: if matches!(mode, Mode::DefinitionVersionMismatch) { 3 } else { 2 },
                                definition: br#"{"version":2,"kind":"user","persona":"Synthetic source fixture","rules":[],"state":{},"model_tier":"cheap"}"#.to_vec(),
                                cursors: positions.lock().unwrap().clone(),
                            }));
                            ServerFrame::Res { id, result }
                        }
                        ClientFrame::Req {
                            id,
                            op: Op::Agent(AgentRequest::Advance { space, seq }),
                        } if matches!(mode, Mode::CursorAck) => {
                            seen.lock().unwrap().push("agent_advance");
                            let mut cursors = positions.lock().unwrap();
                            if let Some((_, prior)) =
                                cursors.iter_mut().find(|(id, _)| id == &space)
                            {
                                *prior = (*prior).max(seq);
                            } else {
                                cursors.push((space, seq));
                            }
                            ServerFrame::Res {
                                id,
                                result: Ok(Reply::Done),
                            }
                        }
                        ClientFrame::Req {
                            id,
                            op: Op::Agent(_),
                        } => {
                            seen.lock().unwrap().push("forbidden_agent_operation");
                            ServerFrame::Res {
                                id,
                                result: Err("inspection must never mutate Agent state".into()),
                            }
                        }
                        ClientFrame::Req { id, .. } => {
                            seen.lock().unwrap().push("other_request");
                            ServerFrame::Res {
                                id,
                                result: Ok(Reply::Done),
                            }
                        }
                        ClientFrame::Publish { env } => {
                            seen.lock().unwrap().push("event_publish");
                            env.verify().unwrap();
                            let (ev, accepted) = {
                                let mut logs = history.lock().unwrap();
                                let log = logs.entry(env.space().into()).or_default();
                                let seq = log.len() as u64;
                                let prev = log
                                    .last()
                                    .map(|ev| ev.hash.clone())
                                    .unwrap_or_else(|| types::GENESIS_PREV.into());
                                let ev = Sequenced {
                                    seq,
                                    hash: event_log::chain_hash(
                                        env.space(),
                                        seq,
                                        &prev,
                                        &env.wire_hash(),
                                    )?,
                                    prev,
                                    env,
                                };
                                let accepted = ServerFrame::Accepted {
                                    space: ev.env.space().into(),
                                    client_id: ev.env.client_id().into(),
                                    seq,
                                };
                                log.push(ev.clone());
                                (ev, accepted)
                            };
                            ws.send(Message::Binary(ServerFrame::Event { ev }.encode().into()))
                                .await
                                .unwrap();
                            accepted
                        }
                        ClientFrame::Ephemeral {
                            kind: protocol::EphemeralKind::Read { .. },
                            ..
                        } => {
                            seen.lock().unwrap().push("read_receipt");
                            continue;
                        }
                        ClientFrame::Ping => ServerFrame::Pong {
                            server_time_ms: Some(now_ms()),
                        },
                        _ => {
                            seen.lock().unwrap().push("other_frame");
                            continue;
                        }
                    };
                    if ws
                        .send(Message::Binary(response.encode().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok::<(), serde_json::Error>(())
            });
        }
    });
    Relay {
        url,
        auth,
        calls,
        cursors,
        delivered,
        packages,
        released,
        release,
        histories,
        uploads,
        task,
    }
}

// A static App fixture queues encrypted bytes before the host starts. Let the
// normal sync worker drain its exact signed PUT; this is transport, not a turn.
async fn blob_put(
    socket: &mut tokio::net::TcpStream,
    devices: &Mutex<BTreeSet<String>>,
    uploaded: &Mutex<BTreeMap<String, Vec<u8>>>,
) {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let mut one = [0];
        socket.read_exact(&mut one).await.unwrap();
        header.push(one[0]);
        assert!(header.len() <= 16 * 1024);
    }
    let header = String::from_utf8(header).unwrap();
    let request = header.lines().next().unwrap();
    let sha = request
        .strip_prefix("PUT /v1/blobs/")
        .and_then(|value| value.strip_suffix(" HTTP/1.1"))
        .expect("only the normal queued blob PUT is supported");
    let field = |name: &str| {
        header
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.eq_ignore_ascii_case(name).then(|| value.trim())
            })
            .unwrap()
    };
    let device = field("x-zoen-device");
    assert!(devices.lock().unwrap().contains(device));
    let timestamp = field("x-zoen-ts").parse::<i64>().unwrap();
    assert!(timestamp >= 0);
    assert!(event_log::verify_sig(
        device,
        &protocol::blob_put_message(sha, timestamp, "source-fixture"),
        field("x-zoen-sig"),
    ));
    let length = field("content-length").parse::<usize>().unwrap();
    assert!((1..=25 * 1024 * 1024).contains(&length));
    let mut body = vec![0; length];
    socket.read_exact(&mut body).await.unwrap();
    assert_eq!(hex::encode(Sha256::digest(&body)), sha);
    let previous = uploaded.lock().unwrap().insert(sha.into(), body.clone());
    assert!(previous.is_none_or(|previous| previous == body));
    socket
        .write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
}

pub(super) async fn online(client: &Client, settle: bool) {
    client.start_sync(None).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while client.connection().state != "online" {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    if settle {
        client.wait_until_settled(3000).await.unwrap();
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
