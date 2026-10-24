//! Whole World operations borrowing its original fields and process ownership.

use super::*;

impl World {
    pub(super) fn cmd(&self, who: &str, args: &[&str]) -> Command {
        self.cmd_at(self.port, who, args)
    }

    pub(super) fn cmd_at(&self, port: u16, who: &str, args: &[&str]) -> Command {
        let relay = format!("http://127.0.0.1:{port}");
        // After init the CLI connects to its saved account, not ZOEN_RELAY.
        // Move that fixture account too, so *_at really exercises the chosen node.
        let db = self.dir.join(who).join("zoen.sqlite");
        if db.exists() {
            rusqlite::Connection::open(db)
                .expect("client database")
                .execute(
                    "UPDATE meta SET value = json_set(value, '$.relay_url', ?1) WHERE key = 'account'",
                    [&relay],
                )
                .expect("route client to relay");
        }
        let mut c = Command::new(env!("CARGO_BIN_EXE_zoen"));
        c.arg("--home")
            .arg(self.dir.join(who))
            .args(args)
            .env("ZOEN_RELAY", relay)
            .envs(self.client_env.iter().map(|(k, v)| (k, v)));
        if self.diagnostics.enabled() {
            c.env("ZOEN_SYNC_WAIT_DIAGNOSTICS", "1");
        }
        c
    }

    /// Environment every `zoen` of this world runs with (e.g. `ZOEN_CHECKPOINT_EVERY`).
    pub fn set_client_env(&mut self, key: &str, value: &str) {
        self.client_env.push((key.to_string(), value.to_string()));
    }

    pub fn set_relay_env(&mut self, key: &str, value: &str) {
        self.relay_env.push((key.to_string(), value.to_string()));
    }

    /// Runs `zoen` as `who` against the relay node on `port`.
    pub fn zoen_at(&self, port: u16, who: &str, args: &[&str]) -> String {
        let out = debug(
            who,
            self.cmd_at(port, who, args).output().expect("run zoen"),
        );
        self.diagnostics.observe(args, &out);
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
        self.zoen_env(who, args, &[])
    }

    /// Extra environment for this one synthetic consumer; its original failure
    /// still includes the actual child's stderr, without a retry or second wait.
    pub fn zoen_env(&self, who: &str, args: &[&str], env: &[(&str, &str)]) -> String {
        let mut command = self.cmd(who, args);
        command.envs(env.iter().copied());
        let out = debug(who, command.output().expect("run zoen"));
        self.diagnostics.observe(args, &out);
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
        self.diagnostics.observe(args, &out);
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
            if out.contains("connection=online synced=true") && done(&out) {
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

    pub fn spawn_zoen_logged_streams(
        &self,
        who: &str,
        args: &[&str],
        stdout_name: &str,
        stderr_name: &str,
    ) -> Child {
        self.cmd(who, args)
            .stdout(std::fs::File::create(self.dir.join(stdout_name)).expect("client stdout"))
            .stderr(std::fs::File::create(self.dir.join(stderr_name)).expect("client stderr"))
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
    pub(super) fn relay_log(&self, args: &[&str]) -> Vec<String> {
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

    /// Ends an agent link now, straight in the relay's store (no 60 s wait in tests).
    pub fn expire_agent_invite(&self, code: &str) {
        self.relay_log(&["expire-agent-invite", code]);
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
