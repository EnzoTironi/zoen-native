//! The same synthetic Agent account has a cached one-page snapshot and genuine
//! signed two-page relay history. Only the second event and SyncDone are held.
use client_core::{Client, PrivacyDto, SecretVault};
use futures_util::{SinkExt, StreamExt};
use protocol::{ClientFrame, Envelope, Reply, Sequenced, ServerFrame};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Output,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio_tungstenite::tungstenite::Message;

const CACHED: &str = "# Cached\n\nAlready on this device.\n";
const REMOTE: &str = "# Remote\n\nHeld until catch-up finishes.\n";

struct Vault(PathBuf);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        std::fs::read(self.0.join(key)).ok()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        std::fs::write(self.0.join(key), value).is_ok()
    }
    fn delete(&self, key: String) {
        let _ = std::fs::remove_file(self.0.join(key));
    }
}

pub struct Fixture {
    directory: PathBuf,
    home: PathBuf,
    space: String,
    cached: String,
    remote: String,
    released: Arc<AtomicBool>,
    connections: Arc<AtomicUsize>,
    syncs: Arc<AtomicUsize>,
    completed: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn private_directory(path: &Path) {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).unwrap();
}

fn open(home: &Path) -> Arc<Client> {
    Client::open(
        home.join("zoen.sqlite").to_string_lossy().into(),
        "en".into(),
    )
    .unwrap()
}
fn vault(home: &Path) -> Arc<dyn SecretVault> {
    Arc::new(Vault(home.join("vault")))
}

impl Fixture {
    pub async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let directory = std::env::temp_dir().join(types::new_id("export-admission"));
        private_directory(&directory);
        let source = directory.join("source");
        let home = directory.join("cached");
        for path in [&source, &home] {
            private_directory(path);
            private_directory(&path.join("vault"));
        }
        private_directory(&directory.join("owner-vault"));
        let owner = Client::open(":memory:".into(), "en".into()).unwrap();
        let owner_id = owner
            .create_account(
                "Owner".into(),
                "export_owner".into(),
                url.clone(),
                Arc::new(Vault(directory.join("owner-vault"))),
            )
            .unwrap()
            .identity_id;
        let root = vec![72; 32];
        let id = client_core::agent_id_for_root(root.clone()).unwrap();
        let proof = owner.authorize_agent_json(id).unwrap();
        let client = open(&source);
        client
            .create_agent_account(
                "Export fixture".into(),
                "export_fixture".into(),
                url,
                root,
                owner_id,
                proof,
                vault(&source),
            )
            .unwrap();
        let space = client
            .create_group_with("Pages".into(), vec![], PrivacyDto::Closed)
            .unwrap();
        let cached = client
            .page_import_markdown(space.clone(), "cached.md".into(), CACHED.into())
            .unwrap()
            .id;
        drop(client);
        // The database is closed before taking the retained local snapshot.
        std::fs::copy(source.join("zoen.sqlite"), home.join("zoen.sqlite")).unwrap();
        for entry in std::fs::read_dir(source.join("vault")).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), home.join("vault").join(entry.file_name())).unwrap();
        }
        let client = open(&source);
        assert!(client.unlock(vault(&source)).unwrap());
        let remote = client
            .page_import_markdown(space.clone(), "remote.md".into(), REMOTE.into())
            .unwrap()
            .id;
        drop(client);
        let store = store::Store::open(source.join("zoen.sqlite").to_str().unwrap()).unwrap();
        let mut logs = BTreeMap::<String, Vec<Sequenced>>::new();
        for pending in store.outbox().unwrap() {
            let event = pending.event;
            let env = Envelope::new(event.content, event.sig, event.cert, None).unwrap();
            env.verify().unwrap();
            let log = logs.entry(env.space().into()).or_default();
            let seq = log.len() as u64;
            let prev = log
                .last()
                .map(|e| e.hash.clone())
                .unwrap_or_else(|| types::GENESIS_PREV.into());
            log.push(Sequenced {
                seq,
                hash: event_log::chain_hash(env.space(), seq, &prev, &env.wire_hash()),
                prev,
                env,
            });
        }
        drop(store);
        let remote_client_id = logs[&space].last().unwrap().env.client_id().to_owned();
        let released = Arc::new(AtomicBool::new(false));
        let connections = Arc::new(AtomicUsize::new(0));
        let syncs = Arc::new(AtomicUsize::new(0));
        let completed = Arc::new(AtomicUsize::new(0));
        let state = (
            released.clone(),
            connections.clone(),
            syncs.clone(),
            completed.clone(),
        );
        let logs = Arc::new(logs);
        let task = tokio::spawn(async move {
            let mut clients = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (socket, _) = accepted.unwrap();
                        let (released, connections, syncs, completed) = state.clone();
                        let logs = logs.clone();
                        let held = remote_client_id.clone();
                        clients.spawn(async move {
                            let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
                            let Some(Ok(Message::Binary(bytes))) = ws.next().await else { return; };
                            let ClientFrame::Hello { identity, device, cert, .. } = ClientFrame::decode(&bytes).unwrap() else { panic!("Hello"); };
                            assert!(event_log::verify_sig(&identity, &event_log::device_cert_message(&device), &cert));
                            let send = |frame: ServerFrame| Message::Binary(frame.encode().into());
                            ws.send(send(ServerFrame::Challenge { nonce: "export-admission".into(), relay: "export-admission".into(), protocol: protocol::PROTOCOL_VERSION, capabilities: vec![], server_time_ms: None })).await.unwrap();
                            let Some(Ok(Message::Binary(bytes))) = ws.next().await else { return; };
                            let ClientFrame::Auth { sig } = ClientFrame::decode(&bytes).unwrap() else { panic!("Auth"); };
                            assert!(event_log::verify_sig(&device, &protocol::auth_message("export-admission", "export-admission"), &sig));
                            ws.send(send(ServerFrame::Ready { identity, registered: true })).await.unwrap();
                            connections.fetch_add(1, Ordering::SeqCst);
                            while let Some(Ok(Message::Binary(bytes))) = ws.next().await {
                                let response = match ClientFrame::decode(&bytes).unwrap() {
                                    ClientFrame::Sync { cursors, .. } => {
                                        syncs.fetch_add(1, Ordering::SeqCst);
                                        let release = released.load(Ordering::SeqCst);
                                        for (space, log) in logs.iter() {
                                            let next = cursors.iter().find(|c| &c.space == space).map_or(0, |c| c.next_seq);
                                            for ev in log.iter().filter(|e| e.seq >= next && (release || e.env.client_id() != held)) {
                                                ws.send(send(ServerFrame::Event { ev: ev.clone() })).await.unwrap();
                                            }
                                        }
                                        if !release { continue; }
                                        completed.fetch_add(1, Ordering::SeqCst);
                                        ServerFrame::SyncDone
                                    }
                                    ClientFrame::Publish { env } => {
                                        env.verify().unwrap();
                                        let ev = logs[env.space()].iter().find(|e| e.env.client_id() == env.client_id()).expect("only original cached publications");
                                        assert_eq!(ev.env, env);
                                        ServerFrame::Accepted { space: env.space().into(), client_id: env.client_id().into(), seq: ev.seq }
                                    }
                                    ClientFrame::Req { id, .. } => ServerFrame::Res { id, result: Ok(Reply::Done) },
                                    ClientFrame::Ping => ServerFrame::Pong { server_time_ms: None },
                                    _ => continue,
                                };
                                if ws.send(send(response)).await.is_err() { break; }
                            }
                        });
                    }
                    result = clients.join_next(), if !clients.is_empty() => { if let Some(result) = result { result.unwrap(); } }
                }
            }
        });
        Self {
            directory,
            home,
            space,
            cached,
            remote,
            released,
            connections,
            syncs,
            completed,
            task,
        }
    }

    pub fn assert_cached_subset(&self) {
        let client = open(&self.home);
        assert!(client.unlock(vault(&self.home)).unwrap());
        assert_eq!(client.items().len(), 1);
        assert!(client.page(self.cached.clone()).unwrap().ready);
        assert_eq!(client.page_markdown(self.cached.clone()).unwrap(), CACHED);
        assert!(client.item(self.remote.clone()).is_err());
    }
    pub fn output(&self, name: &str) -> PathBuf {
        self.directory.join(name)
    }
    pub fn release(&self) {
        self.released.store(true, Ordering::SeqCst);
    }
    pub fn assert_sync_without_completion(&self) {
        assert!(self.connections.load(Ordering::SeqCst) > 0);
        assert!(self.syncs.load(Ordering::SeqCst) > 0);
        assert_eq!(self.completed.load(Ordering::SeqCst), 0);
    }
    pub fn assert_no_connection(&self) {
        assert_eq!(self.connections.load(Ordering::SeqCst), 0);
    }
    pub fn assert_exact_export(&self, name: &str, remote: bool) {
        assert_eq!(
            std::fs::read(self.output(name).join("cached.md")).unwrap(),
            CACHED.as_bytes()
        );
        if remote {
            assert_eq!(
                std::fs::read(self.output(name).join("remote.md")).unwrap(),
                REMOTE.as_bytes()
            );
        } else {
            assert!(!self.output(name).join("remote.md").exists());
        }
    }
    pub async fn export(&self, name: &str, offline: bool) -> Output {
        let home = self.home.clone();
        let space = self.space.clone();
        let output = self.output(name);
        tokio::time::timeout(
            Duration::from_secs(10),
            tokio::task::spawn_blocking(move || {
                let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_zoen"));
                for (name, _) in std::env::vars_os() {
                    let key = name.to_string_lossy();
                    if ["ZOEN_", "CLOUDFLARE_", "OPENAI_", "ANTHROPIC_", "CODEX_"]
                        .iter()
                        .any(|prefix| key.starts_with(prefix))
                    {
                        command.env_remove(name);
                    }
                }
                command
                    .arg("--home")
                    .arg(home)
                    .args(["--timeout", "1000", "page", "export-all"])
                    .arg(space)
                    .arg("--dir")
                    .arg(output);
                if offline {
                    command.arg("--offline");
                }
                let mut child = command
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .unwrap();
                let deadline = std::time::Instant::now() + Duration::from_secs(8);
                loop {
                    if child.try_wait().unwrap().is_some() {
                        break;
                    }
                    if std::time::Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        panic!("synthetic CLI export exceeded its owned child deadline");
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                child.wait_with_output().unwrap()
            }),
        )
        .await
        .expect("bounded synthetic CLI export")
        .unwrap()
    }
}
