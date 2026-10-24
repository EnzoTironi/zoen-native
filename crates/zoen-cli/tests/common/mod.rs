#![allow(dead_code)]
//! Test world: a fresh Postgres database, a fresh FoundationDB cell, a relay process on a
//! free port, and `zoen` homes in a temp dir. Needs `ZOEN_TEST_PG` and a running
//! FoundationDB (`eval "$(scripts/fdb.sh env)"`).

pub mod claim_proxy;
mod diagnostics;
mod relay_build;
pub mod watch;
mod watch_readiness;
mod world_commands;
mod world_service;

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

use event_log::{content::SignedContent, Author, Signer};
use futures_util::{SinkExt, StreamExt};
use protocol::{auth_message, ClientFrame, Envelope, Op, Sequenced, ServerFrame, PROTOCOL_VERSION};
use sqlx::{Connection, PgConnection};
use tokio_tungstenite::tungstenite::Message;
use types::{EventBody, Identity, IdentityKind, Seen};

/// The relay binary, built from this checkout once per test run: a stale relay that
/// predates a wire-type change silently drops new fields and breaks every signature.
pub fn relay_bin() -> PathBuf {
    static BUILT: std::sync::Once = std::sync::Once::new();
    // A package-only build has a different feature graph from the workspace tests.
    // cdylib dependencies emit un-hashed rlibs, so sharing the outer target would
    // overwrite the metadata still needed by later workspace doctests.
    let target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("journey-relay");
    BUILT.call_once(|| {
        let mut command = Command::new(env!("CARGO"));
        relay_build::configure(
            &mut command,
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
            &target,
        );
        let ok = command.status().expect("cargo").success();
        assert!(ok, "cargo build -p zoen-relay failed");
    });
    target
        .join("debug")
        .join(format!("zoen-relay{}", std::env::consts::EXE_SUFFIX))
}

pub struct PortReservation {
    port: u16,
    _lock: File,
}

impl PortReservation {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn acquire(mut preferred: Option<u16>) -> Self {
        let dir = std::env::temp_dir().join("zoen-journey-port-locks-v1");
        std::fs::create_dir_all(&dir).expect("port lock directory");
        for _ in 0..1000 {
            let (port, _socket) = match preferred.take() {
                Some(port) => (port, None),
                None => {
                    let listener = TcpListener::bind("127.0.0.1:0").expect("candidate port");
                    let port = listener.local_addr().unwrap().port();
                    (port, Some(listener))
                }
            };
            // Keep the same inode: unlinking a released lock file would let another
            // process lock a new file while a previous contender holds the old one.
            let lock = File::options()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(dir.join(format!("{port}.lock")))
                .expect("port lock file");
            match lock.try_lock() {
                Ok(()) => return Self { port, _lock: lock },
                Err(std::fs::TryLockError::WouldBlock) => {}
                Err(error) => panic!("port reservation failed: {error:?}"),
            }
        }
        panic!("could not reserve a journey port");
    }
}

fn rand_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    getrandom::getrandom(&mut b).unwrap();
    hex::encode(b)
}

/// Postgres rejects an unquoted name that contains `-` (or anything outside
/// `[a-z0-9_]`). The cell and the temp dir use this same identifier.
fn database_ident(name: &str, suffix: &str) -> String {
    const MAX: usize = 63;
    let body: String = name
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '_' => c,
            _ => '_',
        })
        .collect();
    let prefix = "zoen_t_";
    let tail = format!("_{suffix}");
    let room = MAX.saturating_sub(prefix.len() + tail.len());
    let body: String = body.chars().take(room).collect();
    format!("{prefix}{body}{tail}")
}

#[test]
fn database_ident_maps_hyphens_and_stays_within_63() {
    let db = database_ident("page-completion", "ab12");
    assert_eq!(db, "zoen_t_page_completion_ab12");
    assert!(db.chars().all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_')));
    let long = database_ident(&"n-".repeat(80), "abcd");
    assert!(long.len() <= 63, "{long}");
    assert!(long.starts_with("zoen_t_"));
    assert!(long.ends_with("_abcd"));
}

pub struct World {
    pub dir: PathBuf,
    pub db_url: String,
    pub cell: String,
    pub port: u16,
    pub nats: Option<String>,
    relay_env: Vec<(String, String)>,
    client_env: Vec<(String, String)>,
    relay: Option<Child>,
    relay_name: String,
    nodes: Vec<Child>,
    // A stopped relay still owns its port until this World and all its children end.
    ports: Vec<PortReservation>,
    diagnostics: diagnostics::JourneyDiagnostics,
}

impl Drop for World {
    fn drop(&mut self) {
        self.stop_relay();
        for mut c in self.nodes.drain(..) {
            let _ = c.kill();
            let _ = c.wait();
        }
        // Release reservations only after every child was reaped.
        self.ports.clear();
        // Keep the database and homes of a failed journey for a post-mortem.
        if std::thread::panicking() {
            self.diagnostics.publish_failure();
            eprintln!(
                "journey state kept: {} (db {})",
                self.dir.display(),
                self.db_url
            );
            return;
        }
        let _ = std::fs::remove_dir_all(&self.dir);
        let _ = Command::new(relay_bin())
            .args(["log", "drop-cell", &self.cell])
            .env("ZOEN_FDB_CELL", &self.cell)
            .status();
        let (admin, db) = (
            std::env::var("ZOEN_TEST_PG").unwrap_or_default(),
            self.db_url
                .rsplit_once('/')
                .map(|x| x.1.to_string())
                .unwrap_or_default(),
        );
        let _ = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                if let Ok(mut c) = PgConnection::connect(&admin).await {
                    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
                        "DROP DATABASE IF EXISTS {db} WITH (FORCE)"
                    )))
                    .execute(&mut c)
                    .await;
                }
            });
        })
        .join();
    }
}

/// Speaks the wire protocol directly, like an attacker with their own client would.
pub struct RawClient {
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    pub author: Author,
}

impl RawClient {
    pub async fn reconnect(relay: &str, author: Author) -> RawClient {
        let url = format!("{}/v1/sync", relay.replace("http://", "ws://"));
        let (ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let mut c = RawClient { ws, author };
        c.send(&ClientFrame::Hello {
            protocol: PROTOCOL_VERSION,
            capabilities: Vec::new(),
            identity: c.identity(),
            device: c.author.device.clone().unwrap(),
            cert: c.author.cert.clone().unwrap(),
        })
        .await;
        let ServerFrame::Challenge { nonce, relay, .. } = c.recv().await else {
            panic!("challenge")
        };
        let sig = c.author.key.sign(&auth_message(&nonce, &relay));
        c.send(&ClientFrame::Auth { sig }).await;
        assert!(matches!(c.recv().await, ServerFrame::Ready { .. }));
        c
    }

    pub async fn request(&mut self, op: Op) -> Result<protocol::Reply, String> {
        self.send(&ClientFrame::Req { id: 1, op }).await;
        loop {
            if let ServerFrame::Res { id: 1, result } = self.recv().await {
                return result;
            }
        }
    }

    pub async fn connect(relay: &str, handle: &str) -> RawClient {
        Self::connect_registering(relay, handle).await.0
    }

    /// Connects, registers `handle`, and returns the relay's answer to the registration.
    pub async fn connect_registering(relay: &str, handle: &str) -> (RawClient, Result<(), String>) {
        Self::connect_registering_with(relay, handle, &[]).await
    }

    /// Like `connect_registering`, with extra HTTP headers on the upgrade (an edge's client-IP
    /// header, say).
    pub async fn connect_registering_with(
        relay: &str,
        handle: &str,
        headers: &[(&'static str, &str)],
    ) -> (RawClient, Result<(), String>) {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let url = format!("{}/v1/sync", relay.replace("http://", "ws://"));
        let mut req = url.as_str().into_client_request().unwrap();
        for (k, v) in headers {
            req.headers_mut().insert(*k, v.parse().unwrap());
        }
        let (ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
        let root = Signer::generate();
        let author = Author::device(&root, Signer::generate());
        let mut c = RawClient { ws, author };
        c.send(&ClientFrame::Hello {
            protocol: PROTOCOL_VERSION,
            capabilities: Vec::new(),
            identity: root.id(),
            device: c.author.device.clone().unwrap(),
            cert: c.author.cert.clone().unwrap(),
        })
        .await;
        let ServerFrame::Challenge { nonce, relay, .. } = c.recv().await else {
            panic!()
        };
        let sig = c.author.key.sign(&auth_message(&nonce, &relay));
        c.send(&ClientFrame::Auth { sig }).await;
        let ServerFrame::Ready { .. } = c.recv().await else {
            panic!()
        };
        let profile = Identity {
            owner_proof: None,
            id: root.id(),
            kind: IdentityKind::Person,
            name: "Mallory".into(),
            handle: handle.into(),
            tint_hex: "#000".into(),
            glyph: None,
            owner: None,
            bio: String::new(),
        };
        c.send(&ClientFrame::Req {
            id: 0,
            op: Op::Register { profile },
        })
        .await;
        let answer = loop {
            match c.recv().await {
                ServerFrame::Res { id: 0, result } => break result.map(|_| ()),
                _ => continue,
            }
        };
        (c, answer)
    }

    async fn send(&mut self, f: &ClientFrame) {
        self.ws
            .send(Message::Binary(f.encode().into()))
            .await
            .unwrap();
    }

    /// A revoked socket may already be closed when its next operation is attempted.
    pub async fn send_if_open(&mut self, f: &ClientFrame) -> bool {
        self.ws
            .send(Message::Binary(f.encode().into()))
            .await
            .is_ok()
    }

    pub async fn recv_or_close(&mut self) -> Option<ServerFrame> {
        loop {
            match tokio::time::timeout(Duration::from_secs(5), self.ws.next())
                .await
                .expect("socket did not respond or close")
            {
                Some(Ok(Message::Binary(bytes))) => {
                    return Some(ServerFrame::decode(&bytes).unwrap())
                }
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return None,
                _ => {}
            }
        }
    }

    pub async fn recv(&mut self) -> ServerFrame {
        loop {
            match tokio::time::timeout(Duration::from_secs(5), self.ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap()
            {
                Message::Binary(b) => return ServerFrame::decode(&b).unwrap(),
                _ => continue,
            }
        }
    }

    /// Returns the relay's answer as text ("accepted …" or the rejection reason).
    pub async fn publish_message(&mut self, space: &str, text: &str) -> String {
        let e = self.author.sign_event(
            space,
            &types::new_ulid(1),
            1,
            None,
            EventBody::MessagePosted {
                message: "m".into(),
                text: text.into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        );
        self.publish(Envelope::plain(&e)).await
    }

    pub async fn publish_as(&mut self, space: &str, someone: &str, text: &str) -> String {
        let e = self.author.sign_event(
            space,
            &types::new_ulid(1),
            1,
            None,
            EventBody::MessagePosted {
                message: "m".into(),
                text: text.into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        );
        let mut forged = SignedContent::parse(&e.content).unwrap();
        forged.author = someone.to_string();
        self.publish(Envelope::new(forged.encode(), e.sig, e.cert, None).unwrap())
            .await
    }

    pub fn identity(&self) -> String {
        self.author.identity.clone()
    }

    /// Signs `body` on top of `seen` and waits for the relay's sequenced echo.
    pub async fn publish_body(
        &mut self,
        space: &str,
        seen: Option<Seen>,
        body: EventBody,
    ) -> Result<Sequenced, String> {
        let e = self
            .author
            .sign_event(space, &types::new_ulid(now_ms()), now_ms(), seen, body)
            .map_err(|error| error.to_string())?;
        self.publish_signed(e.content, e.sig, e.cert).await
    }

    /// Publishes bytes this client signed itself (any content a newer client could write).
    pub async fn publish_content(&mut self, content: Vec<u8>) -> Result<Sequenced, String> {
        let sig = self.author.key.sign(&event_log::content::signature_message(
            &content,
            &event_log::content::signed_hash(&content),
        ));
        let cert = self.author.cert.clone();
        self.publish_signed(content, sig, cert).await
    }

    /// Joins `space` with an invite code, as a client with no MLS support would.
    pub async fn join(&mut self, space: &str, code: &str) -> Result<Sequenced, String> {
        let body = EventBody::MemberAdded {
            identity: self.identity(),
            role: types::Role::Member,
        };
        let e = self
            .author
            .sign_event(space, &types::new_ulid(now_ms()), now_ms(), None, body)
            .map_err(|error| error.to_string())?;
        let mut env = Envelope::new(e.content, e.sig, e.cert, None).ok_or("not a v3 content")?;
        env.invite = Some(code.to_string());
        self.publish_env(env).await
    }

    async fn publish_signed(
        &mut self,
        content: Vec<u8>,
        sig: String,
        cert: Option<String>,
    ) -> Result<Sequenced, String> {
        let env = Envelope::new(content, sig, cert, None).ok_or("not a v3 content")?;
        self.publish_env(env).await
    }

    pub async fn publish_env(&mut self, env: Envelope) -> Result<Sequenced, String> {
        let client_id = env.client_id().to_string();
        self.send(&ClientFrame::Publish { env }).await;
        loop {
            match self.recv().await {
                ServerFrame::Rejected {
                    reason,
                    client_id: c,
                    ..
                } if c == client_id => return Err(reason),
                ServerFrame::Event { ev } if ev.env.client_id() == client_id => return Ok(ev),
                _ => continue,
            }
        }
    }

    async fn publish(&mut self, env: Envelope) -> String {
        self.send(&ClientFrame::Publish { env }).await;
        loop {
            match self.recv().await {
                ServerFrame::Rejected { reason, .. } => return reason,
                ServerFrame::Accepted { seq, .. } => return format!("accepted {seq}"),
                _ => continue,
            }
        }
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Opens a socket and says only hello, as a client of protocol `version` would.
pub async fn hello_only(relay: &str, version: u32) -> ServerFrame {
    let url = format!("{}/v1/sync", relay.replace("http://", "ws://"));
    let (mut ws, _) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .unwrap();
    let root = Signer::generate();
    let me = Author::device(&root, Signer::generate());
    let hello = ClientFrame::Hello {
        protocol: version,
        capabilities: Vec::new(),
        identity: root.id(),
        device: me.device.unwrap(),
        cert: me.cert.unwrap(),
    };
    ws.send(Message::Binary(hello.encode().into()))
        .await
        .unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
        {
            Message::Binary(b) => return ServerFrame::decode(&b).unwrap(),
            _ => continue,
        }
    }
}

/// With `ZOEN_NET_DEBUG` set, each client's network log is shown under its name.
fn debug(who: &str, out: std::process::Output) -> std::process::Output {
    if std::env::var_os("ZOEN_NET_DEBUG").is_some() {
        for line in String::from_utf8_lossy(&out.stderr).lines() {
            eprintln!("[{who}] {line}");
        }
    }
    out
}
