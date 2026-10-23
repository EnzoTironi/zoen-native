//! Whole World operations borrowing its original fields and process ownership.

use super::*;

impl World {
    pub async fn new(name: &str) -> World {
        World::new_with(name, None, &[]).await
    }

    /// A world whose relays run with extra environment (e.g. `ZOEN_LIMITS`).
    pub async fn with_env(name: &str, env: &[(&str, &str)]) -> World {
        World::new_with(name, None, env).await
    }

    /// Prepares offline fixture homes and relay environment before the first spawn.
    pub async fn with_prepared_env(
        name: &str,
        env: &[(&str, &str)],
        prepare: impl FnOnce(&World) -> Vec<(String, String)>,
    ) -> World {
        Self::new_prepared(name, None, env, PortReservation::acquire(None), prepare).await
    }

    /// A world whose relays fan out through NATS (`ZOEN_NATS_URL`, see scripts/nats.sh).
    pub async fn with_nats(name: &str) -> World {
        let url = std::env::var("ZOEN_NATS_URL")
            .expect("set ZOEN_NATS_URL (eval \"$(scripts/nats.sh env)\")");
        World::new_with(name, Some(url), &[]).await
    }

    pub async fn new_with(name: &str, nats: Option<String>, env: &[(&str, &str)]) -> World {
        Self::new_with_reservation(name, nats, env, PortReservation::acquire(None)).await
    }

    /// A controlled allocator candidate for cross-process isolation journeys.
    pub async fn new_preferring_port(name: &str, port: u16) -> World {
        Self::new_with_reservation(name, None, &[], PortReservation::acquire(Some(port))).await
    }

    pub(super) async fn new_with_reservation(
        name: &str,
        nats: Option<String>,
        env: &[(&str, &str)],
        reservation: PortReservation,
    ) -> World {
        Self::new_prepared(name, nats, env, reservation, |_| Vec::new()).await
    }

    async fn new_prepared(
        name: &str,
        nats: Option<String>,
        env: &[(&str, &str)],
        reservation: PortReservation,
        prepare: impl FnOnce(&World) -> Vec<(String, String)>,
    ) -> World {
        let admin = std::env::var("ZOEN_TEST_PG")
            .expect("set ZOEN_TEST_PG=postgres://user@host:port/postgres");
        let db = database_ident(name, &rand_hex(4));
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
            port: reservation.port,
            nats,
            relay_env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            client_env: Vec::new(),
            relay: None,
            relay_name: String::new(),
            nodes: Vec::new(),
            ports: vec![reservation],
            diagnostics: diagnostics::JourneyDiagnostics::from_env(name),
        };
        let prepared_env = prepare(&w);
        w.relay_env.extend(prepared_env);
        w.start_relay();
        w
    }

    pub fn relay_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn relay_name(&self) -> &str {
        &self.relay_name
    }

    pub fn start_relay(&mut self) {
        self.try_start_relay()
            .unwrap_or_else(|error| panic!("{error}"));
    }

    pub fn try_start_relay(&mut self) -> Result<(), String> {
        assert!(
            self.relay.is_none(),
            "this World's relay is already running"
        );
        let (child, relay_name) = self.spawn_relay(self.port, "relay.log")?;
        self.relay = Some(child);
        self.relay_name = relay_name;
        Ok(())
    }

    /// Another relay node on the same Postgres, cell and bus; returns its port.
    pub fn start_node(&mut self) -> u16 {
        let reservation = PortReservation::acquire(None);
        let port = reservation.port;
        let (child, _) = self
            .spawn_relay(port, &format!("relay-{port}.log"))
            .unwrap_or_else(|error| panic!("{error}"));
        self.nodes.push(child);
        self.ports.push(reservation);
        port
    }

    pub(super) fn spawn_relay(&self, port: u16, log_name: &str) -> Result<(Child, String), String> {
        let path = self.dir.join(log_name);
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        let start = log.metadata().unwrap().len();
        let relay_name = format!("journey-{}", rand_hex(16));
        let output = || -> String {
            let mut bytes = Vec::new();
            let read = File::open(&path).and_then(|mut file| {
                file.seek(SeekFrom::Start(start))?;
                file.read_to_end(&mut bytes)
            });
            match read {
                Ok(_) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(error) => format!("could not read relay output: {error}"),
            }
        };
        let mut cmd = Command::new(relay_bin());
        cmd.args([
            "--bind",
            &format!("127.0.0.1:{port}"),
            "--relay-name",
            &relay_name,
        ])
        .env("DATABASE_URL", &self.db_url)
        .env("ZOEN_FDB_CELL", &self.cell)
        .env("ZOEN_BLOB_DIR", self.dir.join("blobs"))
        .env_remove("ZOEN_NATS_URL")
        .env_remove("ZOEN_DEV_ALLOW_UNAUTHENTICATED_PASSWORD_BACKUP")
        .stdout(log.try_clone().unwrap())
        .stderr(log);
        if let Some(url) = &self.nats {
            cmd.env("ZOEN_NATS_URL", url);
        }
        cmd.envs(self.relay_env.iter().map(|(k, v)| (k, v)));
        let mut child = cmd
            .spawn()
            .map_err(|error| format!("start relay: {error}"))?;
        for _ in 0..100 {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let _ = child.wait();
                    return Err(format!(
                        "relay child {} exited with {status} before listening on :{port}; {}\n{}",
                        child.id(),
                        path.display(),
                        output()
                    ));
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "relay child status: {error}; {}\n{}",
                        path.display(),
                        output()
                    ));
                }
            }
            // Only this spawn can emit its token after successfully binding. An
            // unrelated listener, or a previous successful restart, is not readiness.
            let listening = output()
                .lines()
                .any(|line| line.contains("zoen-relay listening") && line.contains(&relay_name));
            if listening && std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return Ok((child, relay_name));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        Err(format!(
            "relay didn't start; {}\n{}",
            path.display(),
            output()
        ))
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

    /// Updates the environment used on the next relay restart.
    pub fn configure_relay_env(&mut self, name: &str, value: Option<&str>) {
        self.relay_env.retain(|(key, _)| key != name);
        if let Some(value) = value {
            self.relay_env.push((name.into(), value.into()));
        }
    }
}
