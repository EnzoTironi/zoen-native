//! Owned native linking fixture. Real sockets and certificate/auth signatures;
//! registration acceptance is synthetic, not production relay qualification.
use super::*;
use futures_util::{SinkExt, StreamExt};
use protocol::{ClientFrame, DeviceRecord, ServerFrame, PROTOCOL_VERSION};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{atomic::AtomicUsize, Mutex},
};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

#[derive(Default)]
pub(super) struct Vault {
    pub(super) values: Mutex<HashMap<String, Vec<u8>>>,
    pub(super) reads: AtomicUsize,
    pub(super) writes: AtomicUsize,
    pub(super) ambiguous_peer_save: AtomicBool,
}
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.values.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.writes.fetch_add(1, Ordering::SeqCst);
        let fail = key == VAULT_PEERS && self.ambiguous_peer_save.load(Ordering::SeqCst);
        self.values.lock().unwrap().insert(key, value);
        !fail
    }
    fn delete(&self, key: String) {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.values.lock().unwrap().remove(&key);
    }
}
pub(super) struct Call {
    pub(super) op: Op,
    pub(super) result: oneshot::Sender<Result<Reply, String>>,
}
pub(super) async fn next_call(calls: &mut mpsc::UnboundedReceiver<Call>) -> Call {
    tokio::time::timeout(Duration::from_secs(5), calls.recv())
        .await
        .unwrap()
        .unwrap()
}
pub(super) struct Relay {
    pub(super) url: String,
    pub(super) calls: mpsc::UnboundedReceiver<Call>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Relay {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn relay() -> Relay {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, calls) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let mut sockets = tokio::task::JoinSet::new();
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let tx = tx.clone();
            sockets.spawn(async move {
                let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
                let Some(Ok(Message::Binary(bytes))) = socket.next().await else {
                    return;
                };
                let ClientFrame::Hello {
                    identity,
                    device,
                    cert,
                    ..
                } = ClientFrame::decode(&bytes).unwrap()
                else {
                    panic!("signed Hello");
                };
                assert!(event_log::verify_sig(
                    &identity,
                    &event_log::device_cert_message(&device),
                    &cert
                ));
                socket
                    .send(Message::Binary(
                        ServerFrame::Challenge {
                            nonce: "link-owner-fixture".into(),
                            relay: "link-owner-fixture".into(),
                            protocol: PROTOCOL_VERSION,
                            capabilities: Vec::new(),
                            server_time_ms: None,
                        }
                        .encode()
                        .into(),
                    ))
                    .await
                    .unwrap();
                let Some(Ok(Message::Binary(bytes))) = socket.next().await else {
                    return;
                };
                let ClientFrame::Auth { sig } = ClientFrame::decode(&bytes).unwrap() else {
                    panic!("device Auth");
                };
                assert!(event_log::verify_sig(
                    &device,
                    &protocol::auth_message("link-owner-fixture", "link-owner-fixture"),
                    &sig
                ));
                socket
                    .send(Message::Binary(
                        ServerFrame::Ready {
                            identity,
                            registered: true,
                        }
                        .encode()
                        .into(),
                    ))
                    .await
                    .unwrap();
                while let Some(Ok(Message::Binary(bytes))) = socket.next().await {
                    let response = match ClientFrame::decode(&bytes).unwrap() {
                        ClientFrame::Req {
                            id,
                            op: op @ (Op::DeliverLink { .. } | Op::FetchLink { .. }),
                        } => {
                            let (result, rx) = oneshot::channel();
                            tx.send(Call { op, result }).unwrap();
                            let Ok(result) = rx.await else {
                                break;
                            };
                            ServerFrame::Res { id, result }
                        }
                        ClientFrame::Req {
                            id,
                            op: Op::Devices,
                        } => ServerFrame::Res {
                            id,
                            result: Ok(Reply::Devices(vec![DeviceRecord {
                                device: device.clone(),
                                revoked: false,
                            }])),
                        },
                        ClientFrame::Req { id, .. } => ServerFrame::Res {
                            id,
                            result: Ok(Reply::Done),
                        },
                        ClientFrame::Sync { .. } => ServerFrame::SyncDone,
                        ClientFrame::Ping => ServerFrame::Pong {
                            server_time_ms: None,
                        },
                        _ => continue,
                    };
                    if socket
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
    Relay { url, calls, task }
}

pub(super) struct Fixture {
    client: Option<Arc<Client>>,
    pub(super) vault: Arc<Vault>,
    pub(super) relay: Relay,
    directory: PathBuf,
}
impl Fixture {
    pub(super) async fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let relay = relay().await;
        let directory = std::env::temp_dir().join(types::new_id("zoen-link-owner")?);
        std::fs::create_dir(&directory).unwrap();
        let client = Client::open(
            directory.join("owner.sqlite").to_string_lossy().into(),
            "en".into(),
        )
        .unwrap();
        let vault = Arc::new(Vault::default());
        client
            .create_account(
                "Link owner".into(),
                "link_owner".into(),
                relay.url.clone(),
                vault.clone(),
            )
            .unwrap();
        client.lock().set_registered(true).unwrap();
        Ok(Self {
            client: Some(client),
            vault,
            relay,
            directory,
        })
    }
    pub(super) fn client(&self) -> &Arc<Client> {
        self.client.as_ref().unwrap()
    }
    pub(super) async fn online(&self) {
        self.client().start_sync(None).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while self.client().connection().state != "online" || !self.client().connection().synced
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    pub(super) fn reopen(&mut self) {
        let client = self.client.take().unwrap();
        client.stop_sync();
        assert_eq!(
            Arc::strong_count(&client),
            1,
            "fixture must release all admitted work first"
        );
        drop(client);
        let client = Client::open(
            self.directory.join("owner.sqlite").to_string_lossy().into(),
            "en".into(),
        )
        .unwrap();
        assert!(client.unlock(self.vault.clone()).unwrap());
        self.client = Some(client);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(client) = self.client.take() {
            client.stop_sync();
            drop(client);
        }
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
