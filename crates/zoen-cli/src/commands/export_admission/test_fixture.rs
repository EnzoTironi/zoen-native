//! A certified, synthetic User and a real WS carrying a controllable top-up ACK.
use client_core::{Client, PrivacyDto, SecretVault};
use futures_util::{SinkExt, StreamExt};
use protocol::{ClientFrame, Reply, Sequenced, ServerFrame};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

pub(super) const MARKDOWN: &str = "# Export boundary\n\nExact cached UTF-8: ação 🌿.\n";

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

enum Control {
    Low(oneshot::Sender<()>),
    Release(oneshot::Sender<()>),
}

pub(super) struct Fixture {
    pub client: Arc<Client>,
    pub space: String,
    pub directory: PathBuf,
    pub deadline: tokio::time::Instant,
    control: mpsc::UnboundedSender<Control>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.client.stop_sync();
        self.task.abort();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

async fn send(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    frame: ServerFrame,
) {
    socket
        .send(Message::Binary(frame.encode().into()))
        .await
        .unwrap();
}

impl Fixture {
    pub async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay = format!("http://{}", listener.local_addr().unwrap());
        let client = Client::open(":memory:".into(), "en".into()).unwrap();
        client
            .create_account(
                "Synthetic exporter".into(),
                "export_boundary".into(),
                relay,
                Arc::new(Vault::default()),
            )
            .unwrap();
        let space = client
            .create_group_with("Export".into(), Vec::new(), PrivacyDto::Closed)
            .unwrap();
        client
            .page_import_markdown(space.clone(), "page.md".into(), MARKDOWN.into())
            .unwrap();
        let directory = std::env::temp_dir().join(types::new_id("export-mls-boundary"));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&directory).unwrap();
        let (control, mut controls) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
            let Some(Ok(Message::Binary(bytes))) = socket.next().await else {
                panic!("Hello");
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
            send(
                &mut socket,
                ServerFrame::Challenge {
                    nonce: "export-boundary".into(),
                    relay: "export-boundary".into(),
                    protocol: protocol::PROTOCOL_VERSION,
                    capabilities: vec![],
                    server_time_ms: None,
                },
            )
            .await;
            let Some(Ok(Message::Binary(bytes))) = socket.next().await else {
                panic!("Auth");
            };
            let ClientFrame::Auth { sig } = ClientFrame::decode(&bytes).unwrap() else {
                panic!("Auth");
            };
            assert!(event_log::verify_sig(
                &device,
                &protocol::auth_message("export-boundary", "export-boundary"),
                &sig
            ));
            send(
                &mut socket,
                ServerFrame::Ready {
                    identity,
                    registered: true,
                },
            )
            .await;
            let mut logs = HashMap::<String, Vec<Sequenced>>::new();
            let mut requested_top_up = None;
            let mut held_top_up = None;
            loop {
                tokio::select! {
                    command = controls.recv() => match command {
                        Some(Control::Low(seen)) => {
                            assert!(requested_top_up.is_none() && held_top_up.is_none());
                            requested_top_up = Some(seen);
                            send(&mut socket, ServerFrame::KeyPackagesLow { device: device.clone(), remaining: 31 }).await;
                        }
                        Some(Control::Release(done)) => {
                            let id = held_top_up.take().expect("a real publication is awaiting its ACK");
                            send(&mut socket, ServerFrame::Res { id, result: Ok(Reply::Done) }).await;
                            done.send(()).unwrap();
                        }
                        None => break,
                    },
                    frame = socket.next() => {
                        let Some(Ok(Message::Binary(bytes))) = frame else { break; };
                        match ClientFrame::decode(&bytes).unwrap() {
                            ClientFrame::Sync { .. } => send(&mut socket, ServerFrame::SyncDone).await,
                            ClientFrame::Publish { env } => {
                                env.verify().unwrap();
                                let entries = logs.entry(env.space().into()).or_default();
                                let seq = entries.len() as u64;
                                let prev = entries.last().map(|e|e.hash.clone()).unwrap_or_else(||types::GENESIS_PREV.into());
                                let ev = Sequenced { seq, hash: event_log::chain_hash(env.space(), seq, &prev, &env.wire_hash()), prev, env: env.clone() };
                                entries.push(ev.clone());
                                send(&mut socket, ServerFrame::Event { ev }).await;
                                send(&mut socket, ServerFrame::Accepted { space: env.space().into(), client_id: env.client_id().into(), seq }).await;
                            }
                            ClientFrame::Req { id, op: protocol::Op::PublishKeyPackages { packages, last_resort } } if requested_top_up.is_some() => {
                                assert_eq!(packages.len(), 1);
                                assert!(last_resort.is_none());
                                held_top_up = Some(id);
                                requested_top_up.take().unwrap().send(()).unwrap();
                            }
                            ClientFrame::Req { id, .. } => send(&mut socket, ServerFrame::Res { id, result: Ok(Reply::Done) }).await,
                            ClientFrame::Ping => send(&mut socket, ServerFrame::Pong { server_time_ms: None }).await,
                            _ => {}
                        }
                    }
                }
            }
        });
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        client.start_sync(None).unwrap();
        let connection = client.wait_until_settled(5000).await.unwrap();
        assert_eq!(connection.state, "online");
        assert!(connection.synced);
        assert_eq!(connection.pending, 0);
        Self {
            client,
            space,
            directory,
            deadline,
            control,
            task,
        }
    }

    pub async fn hold_top_up(&self) {
        let (sent, seen) = oneshot::channel();
        self.control.send(Control::Low(sent)).unwrap();
        tokio::time::timeout(Duration::from_secs(5), seen)
            .await
            .unwrap()
            .unwrap();
        let connection = self.client.connection();
        assert_eq!(connection.state, "online");
        assert!(connection.synced);
        assert_eq!(connection.pending, 0);
        assert!(
            self.client.wait_until_settled(0).await.is_err(),
            "an observed, unacknowledged top-up is real MLS work"
        );
    }

    pub async fn release_top_up(&self) {
        let (sent, done) = oneshot::channel();
        self.control.send(Control::Release(sent)).unwrap();
        tokio::time::timeout(Duration::from_secs(5), done)
            .await
            .unwrap()
            .unwrap();
    }
}
