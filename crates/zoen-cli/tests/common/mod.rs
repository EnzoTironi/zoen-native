#![allow(dead_code)]
//! Test world: a fresh Postgres database, a fresh FoundationDB cell, a relay process on a
//! free port, and `zoen` homes in a temp dir. Needs `ZOEN_TEST_PG` and a running
//! FoundationDB (`eval "$(scripts/fdb.sh env)"`).

use std::{
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use roda_log::{content::SignedContent, Author, Signer};
use roda_proto::{
    auth_message, ClientFrame, Envelope, Op, Sequenced, ServerFrame, PROTOCOL_VERSION,
};
use roda_types::{EventBody, Identity, IdentityKind, Seen};
use sqlx::{Connection, PgConnection};
use tokio_tungstenite::tungstenite::Message;

/// The relay binary, built from this checkout once per test run: a stale relay that
/// predates a wire-type change silently drops new fields and breaks every signature.
pub fn relay_bin() -> PathBuf {
    static BUILT: std::sync::Once = std::sync::Once::new();
    BUILT.call_once(|| {
        let ok = Command::new(env!("CARGO"))
            .args(["build", "-q", "-p", "zoen-relay"])
            .status()
            .expect("cargo")
            .success();
        assert!(ok, "cargo build -p zoen-relay failed");
    });
    PathBuf::from(env!("CARGO_BIN_EXE_zoen")).with_file_name("zoen-relay")
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn rand_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    getrandom::getrandom(&mut b).unwrap();
    hex::encode(b)
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
    nodes: Vec<Child>,
}

impl World {
    pub async fn new(name: &str) -> World {
        World::new_with(name, None, &[]).await
    }

    /// A world whose relays run with extra environment (e.g. `ZOEN_LIMITS`).
    pub async fn with_env(name: &str, env: &[(&str, &str)]) -> World {
        World::new_with(name, None, env).await
    }

    /// A world whose relays fan out through NATS (`ZOEN_NATS_URL`, see scripts/nats.sh).
    pub async fn with_nats(name: &str) -> World {
        let url = std::env::var("ZOEN_NATS_URL")
            .expect("set ZOEN_NATS_URL (eval \"$(scripts/nats.sh env)\")");
        World::new_with(name, Some(url), &[]).await
    }

    pub async fn new_with(name: &str, nats: Option<String>, env: &[(&str, &str)]) -> World {
        let admin = std::env::var("ZOEN_TEST_PG")
            .expect("set ZOEN_TEST_PG=postgres://user@host:port/postgres");
        let db = format!("zoen_t_{name}_{}", rand_hex(4));
        let mut c = PgConnection::connect(&admin).await.expect("connect admin");
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {db}")))
            .execute(&mut c)
            .await
            .expect("create db");
        let base = admin.rsplit_once('/').unwrap().0;
        let dir = std::env::temp_dir().join(format!("zoen-journey-{db}"));
        std::fs::create_dir_all(&dir).unwrap();
        let mut w = World {
            dir,
            db_url: format!("{base}/{db}"),
            cell: db,
            port: free_port(),
            nats,
            relay_env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            client_env: Vec::new(),
            relay: None,
            nodes: Vec::new(),
        };
        w.start_relay();
        w
    }

    pub fn relay_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn start_relay(&mut self) {
        let child = self.spawn_relay(self.port, "relay.log");
        self.relay = Some(child);
    }

    /// Another relay node on the same Postgres, cell and bus; returns its port.
    pub fn start_node(&mut self) -> u16 {
        let port = free_port();
        let child = self.spawn_relay(port, &format!("relay-{port}.log"));
        self.nodes.push(child);
        port
    }

    fn spawn_relay(&self, port: u16, log_name: &str) -> Child {
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join(log_name))
            .unwrap();
        let mut cmd = Command::new(relay_bin());
        cmd.args(["--bind", &format!("127.0.0.1:{port}")])
            .env("DATABASE_URL", &self.db_url)
            .env("ZOEN_FDB_CELL", &self.cell)
            .env("ZOEN_BLOB_DIR", self.dir.join("blobs"))
            .env_remove("ZOEN_NATS_URL")
            .stdout(log.try_clone().unwrap())
            .stderr(log);
        if let Some(url) = &self.nats {
            cmd.env("ZOEN_NATS_URL", url);
        }
        cmd.envs(self.relay_env.iter().map(|(k, v)| (k, v)));
        let mut child = cmd.spawn().expect("start relay");
        for _ in 0..100 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return child;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!(
            "relay didn't start; see {}",
            self.dir.join(log_name).display()
        );
    }

    pub fn relay_log_text(&self) -> String {
        let mut out = String::new();
        for e in std::fs::read_dir(&self.dir).unwrap().flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with("relay") && n.ends_with(".log") {
                out += &std::fs::read_to_string(e.path()).unwrap_or_default();
            }
        }
        out
    }

    pub fn stop_relay(&mut self) {
        if let Some(mut c) = self.relay.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    fn cmd(&self, who: &str, args: &[&str]) -> Command {
        self.cmd_at(self.port, who, args)
    }

    fn cmd_at(&self, port: u16, who: &str, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_zoen"));
        c.arg("--home")
            .arg(self.dir.join(who))
            .args(args)
            .env("ZOEN_RELAY", format!("http://127.0.0.1:{port}"))
            .envs(self.client_env.iter().map(|(k, v)| (k, v)));
        c
    }

    /// Environment every `zoen` of this world runs with (e.g. `ZOEN_CHECKPOINT_EVERY`).
    pub fn set_client_env(&mut self, key: &str, value: &str) {
        self.client_env.push((key.to_string(), value.to_string()));
    }

    /// Runs `zoen` as `who` against the relay node on `port`.
    pub fn zoen_at(&self, port: u16, who: &str, args: &[&str]) -> String {
        let out = debug(
            who,
            self.cmd_at(port, who, args).output().expect("run zoen"),
        );
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        assert!(
            out.status.success(),
            "zoen {who} {args:?} via :{port} failed: {}\n{stdout}",
            String::from_utf8_lossy(&out.stderr)
        );
        stdout
    }

    pub fn spawn_zoen_at(&self, port: u16, who: &str, args: &[&str]) -> Child {
        self.cmd_at(port, who, args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn zoen")
    }

    /// Runs `zoen` as `who` and returns stdout; panics with stderr on failure.
    pub fn zoen(&self, who: &str, args: &[&str]) -> String {
        let out = debug(who, self.cmd(who, args).output().expect("run zoen"));
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if !out.status.success() {
            panic!(
                "zoen {who} {args:?} failed: {}\n{stdout}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        stdout
    }

    /// Runs `zoen` as `who`; stdout on success, stderr+stdout on failure.
    pub fn try_zoen(&self, who: &str, args: &[&str]) -> Result<String, String> {
        let out = debug(who, self.cmd(who, args).output().expect("run zoen"));
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if out.status.success() {
            Ok(stdout)
        } else {
            Err(format!("{}{stdout}", String::from_utf8_lossy(&out.stderr)))
        }
    }

    /// The relay's Prometheus text.
    pub fn metrics(&self) -> String {
        use std::io::{Read, Write};
        let mut s = std::net::TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        s.write_all(b"GET /metrics HTTP/1.0\r\nHost: x\r\n\r\n")
            .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        out
    }

    /// Authenticated sessions the relay has online now.
    pub fn sessions_online(&self) -> u64 {
        self.metrics()
            .lines()
            .find_map(|l| l.strip_prefix("zoen_relay_sessions_online "))
            .and_then(|v| v.trim().parse::<f64>().ok())
            .unwrap_or(0.0) as u64
    }

    /// Waits until at least `n` sessions are online (e.g. spawned watchers are connected).
    pub fn wait_online(&self, n: u64) {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while self.sessions_online() < n {
            assert!(
                std::time::Instant::now() < deadline,
                "{n} sessions never came online (have {})",
                self.sessions_online()
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// Runs `zoen sync` for `who` until its output satisfies `done` (30 s at most): the
    /// device converges over several round trips, and how many depends on timing.
    pub fn sync_until(&self, who: &str, done: impl Fn(&str) -> bool) -> String {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let out = self.zoen(who, &["sync"]);
            if done(&out) {
                return out;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{who} never got there: {out}"
            );
            std::thread::sleep(Duration::from_millis(300));
        }
    }

    pub fn spawn_zoen(&self, who: &str, args: &[&str]) -> Child {
        self.cmd(who, args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn zoen")
    }

    /// Like `spawn_zoen`, with extra environment for this one process.
    pub fn spawn_zoen_env(&self, who: &str, args: &[&str], env: &[(&str, &str)]) -> Child {
        let mut c = self.cmd(who, args);
        c.envs(env.iter().copied())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn zoen")
    }

    /// Every file under the relay's object store, by path.
    pub fn blobs(&self) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = vec![];
        let mut stack = vec![self.dir.join("blobs")];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                if e.path().is_dir() {
                    stack.push(e.path())
                } else {
                    out.push((e.path(), std::fs::read(e.path()).unwrap()))
                }
            }
        }
        out
    }

    /// A background client whose output is retained without filling an unread pipe.
    pub fn spawn_zoen_logged(&self, who: &str, args: &[&str], log_name: &str) -> Child {
        let log = std::fs::File::create(self.dir.join(log_name)).expect("client log");
        self.cmd(who, args)
            .stdout(log.try_clone().expect("clone client log"))
            .stderr(log)
            .spawn()
            .expect("spawn zoen")
    }

    pub fn init(&self, handle: &str, name: &str) {
        let out = self.zoen(handle, &["init", "--name", name, "--handle", handle]);
        assert!(out.contains("registered"), "{out}");
    }

    pub async fn count(&self, sql: &str) -> i64 {
        let mut c = PgConnection::connect(&self.db_url).await.unwrap();
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_string()))
            .fetch_one(&mut c)
            .await
            .unwrap()
    }

    pub async fn scalar(&self, sql: &str) -> String {
        let mut c = PgConnection::connect(&self.db_url).await.unwrap();
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_string()))
            .fetch_one(&mut c)
            .await
            .unwrap()
    }

    pub async fn id_of(&self, handle: &str) -> String {
        self.scalar(&format!(
            "SELECT id FROM identities WHERE handle = '{handle}'"
        ))
        .await
    }

    /// `zoen-relay log …` against this world's cell: what the relay actually stored.
    fn relay_log(&self, args: &[&str]) -> Vec<String> {
        let out = Command::new(relay_bin())
            .arg("log")
            .args(args)
            .env("ZOEN_FDB_CELL", &self.cell)
            .output()
            .expect("zoen-relay log");
        assert!(
            out.status.success(),
            "zoen-relay log {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The relay's log of one Space, read back from FoundationDB.
    pub async fn events_in(&self, space: &str) -> Vec<Sequenced> {
        self.relay_log(&["read", space])
            .iter()
            .map(|l| Sequenced::decode(&hex::decode(l).unwrap()).expect("stored entry"))
            .collect()
    }

    pub async fn spaces_of_id(&self, id: &str) -> Vec<String> {
        self.relay_log(&["spaces", id])
    }

    pub async fn spaces_of(&self, handle: &str) -> Vec<String> {
        self.spaces_of_id(&self.id_of(handle).await).await
    }

    /// Every event of every Space someone registered here belongs to.
    pub async fn events(&self) -> Vec<Sequenced> {
        let mut c = PgConnection::connect(&self.db_url).await.unwrap();
        let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM identities")
            .fetch_all(&mut c)
            .await
            .unwrap();
        let mut spaces = Vec::new();
        for id in ids {
            for s in self.spaces_of_id(&id).await {
                if !spaces.contains(&s) {
                    spaces.push(s);
                }
            }
        }
        let mut out = Vec::new();
        for s in spaces {
            out.extend(self.events_in(&s).await);
        }
        out
    }
}

impl Drop for World {
    fn drop(&mut self) {
        self.stop_relay();
        for mut c in self.nodes.drain(..) {
            let _ = c.kill();
            let _ = c.wait();
        }
        // Keep the database and homes of a failed journey for a post-mortem.
        if std::thread::panicking() {
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

    async fn recv(&mut self) -> ServerFrame {
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
            &roda_types::new_ulid(1),
            1,
            None,
            EventBody::MessagePosted {
                message: "m".into(),
                text: text.into(),
                attaches: None,
                reply: None,
            },
        );
        self.publish(Envelope::plain(&e)).await
    }

    pub async fn publish_as(&mut self, space: &str, someone: &str, text: &str) -> String {
        let e = self.author.sign_event(
            space,
            &roda_types::new_ulid(1),
            1,
            None,
            EventBody::MessagePosted {
                message: "m".into(),
                text: text.into(),
                attaches: None,
                reply: None,
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
        let e =
            self.author
                .sign_event(space, &roda_types::new_ulid(now_ms()), now_ms(), seen, body);
        self.publish_signed(e.content, e.sig, e.cert).await
    }

    /// Publishes bytes this client signed itself (any content a newer client could write).
    pub async fn publish_content(&mut self, content: Vec<u8>) -> Result<Sequenced, String> {
        let sig = self
            .author
            .key
            .sign(roda_log::content::signed_hash(&content).as_bytes());
        let cert = self.author.cert.clone();
        self.publish_signed(content, sig, cert).await
    }

    /// Joins `space` with an invite code, as a client with no MLS support would.
    pub async fn join(&mut self, space: &str, code: &str) -> Result<Sequenced, String> {
        let body = EventBody::MemberAdded {
            identity: self.identity(),
            role: roda_types::Role::Member,
        };
        let e =
            self.author
                .sign_event(space, &roda_types::new_ulid(now_ms()), now_ms(), None, body);
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

    async fn publish_env(&mut self, env: Envelope) -> Result<Sequenced, String> {
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
