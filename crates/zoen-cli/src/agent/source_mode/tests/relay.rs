//! Certified synthetic loopback only; records forbidden cursor and read effects.
use client_core::{Client, SecretVault};
use futures_util::{SinkExt, StreamExt};
use protocol::{AgentRequest, AgentStateRecord, ClientFrame, Op, Reply, Sequenced, ServerFrame};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use tokio_tungstenite::tungstenite::Message;

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
    Unsynced,
}
pub(super) struct Relay {
    pub url: String,
    pub calls: Arc<Mutex<Vec<&'static str>>>,
    pub cursors: Arc<Mutex<Vec<(String, u64)>>>,
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
    let calls = Arc::new(Mutex::new(Vec::new()));
    let cursors = Arc::new(Mutex::new(Vec::<(String, u64)>::new()));
    let (seen, positions) = (calls.clone(), cursors.clone());
    let task = tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let (seen, positions) = (seen.clone(), positions.clone());
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
                        capabilities: vec![],
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
                    panic!("Auth")
                };
                assert!(event_log::verify_sig(
                    &device,
                    &protocol::auth_message("source-fixture", "source-fixture"),
                    &sig
                ));
                ws.send(Message::Binary(
                    ServerFrame::Ready {
                        identity,
                        registered: true,
                    }
                    .encode()
                    .into(),
                ))
                .await
                .unwrap();
                let mut logs: BTreeMap<String, Vec<Sequenced>> = BTreeMap::new();
                while let Some(Ok(Message::Binary(bytes))) = ws.next().await {
                    let response = match ClientFrame::decode(&bytes).unwrap() {
                        ClientFrame::Sync { .. } => {
                            seen.lock().unwrap().push("sync");
                            if matches!(mode, Mode::Unsynced) {
                                continue;
                            }
                            ServerFrame::SyncDone
                        }
                        ClientFrame::Req {
                            id,
                            op: Op::PublishKeyPackages { .. },
                        } => {
                            seen.lock().unwrap().push("mls_publication");
                            ServerFrame::Res {
                                id,
                                result: Ok(Reply::Done),
                            }
                        }
                        ClientFrame::Req {
                            id,
                            op: Op::Agent(AgentRequest::State),
                        } => {
                            seen.lock().unwrap().push("agent_state");
                            let result = Ok(Reply::AgentState(AgentStateRecord {
                                status: "active".into(), kind: "User".into(), definition_version: 2,
                                definition: br#"{"version":2,"kind":"user","persona":"Synthetic source fixture","rules":[],"state":{},"model_tier":"cheap"}"#.to_vec(),
                                cursors: positions.lock().unwrap().clone(),
                            }));
                            ServerFrame::Res { id, result }
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
                        ClientFrame::Ephemeral {
                            kind: protocol::EphemeralKind::Read { .. },
                            ..
                        } => {
                            seen.lock().unwrap().push("read_receipt");
                            continue;
                        }
                        ClientFrame::Ping => ServerFrame::Pong {
                            server_time_ms: None,
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
            });
        }
    });
    Relay {
        url,
        calls,
        cursors,
        task,
    }
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
