use super::*;
use client_core::{AccountDto, Client, SecretVault};
use sqlx::{Connection, PgConnection};
use std::{collections::HashMap, path::PathBuf, sync::Mutex};

#[derive(Default)]
pub(super) struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().expect("synthetic vault").get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.0.lock().expect("synthetic vault").insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        self.0.lock().expect("synthetic vault").remove(&key);
    }
}

pub(super) struct Device {
    pub core: Arc<Client>,
    pub account: AccountDto,
    pub path: PathBuf,
    pub vault: Arc<Vault>,
}
impl Device {
    pub async fn new(directory: &std::path::Path, handle: &str, relay: String) -> Self {
        let path = directory.join(format!("{handle}.sqlite"));
        let core = Client::open(path.to_string_lossy().into_owned(), "en-US".into()).unwrap();
        let vault = Arc::new(Vault::default());
        let account = core
            .create_account(handle.into(), handle.into(), relay, vault.clone())
            .unwrap();
        core.start_sync(None).unwrap();
        let status = core.wait_until_idle(15_000).await;
        assert_eq!(status.state, "online", "{status:?}");
        assert!(status.synced, "{status:?}");
        Self {
            core,
            account,
            path,
            vault,
        }
    }
}
impl Drop for Device {
    fn drop(&mut self) {
        self.core.stop_sync();
    }
}

pub(super) struct Node {
    pub state: crate::Shared,
    pub port: u16,
    server: tokio::task::JoinHandle<()>,
}
impl Node {
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
    async fn new(config: crate::Config, listener: tokio::net::TcpListener) -> Self {
        let (router, state) =
            crate::build_with_managed(&config, crate::managed_devices::ManagedDevices::default())
                .await
                .unwrap();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });
        Self {
            state,
            port: config.bind.port(),
            server,
        }
    }
    async fn close(&self) {
        self.server.abort();
        self.state.pool.close().await;
        self.state.session_auth.close().await;
        self.state.delivery_auth.close().await;
    }
}
impl Drop for Node {
    fn drop(&mut self) {
        self.server.abort();
    }
}

pub(super) struct World {
    pub a: Node,
    pub b: Node,
    pub log: crate::log::fdb::FdbLog,
    database: String,
    admin: PgConnection,
}
impl World {
    pub async fn new(directory: &std::path::Path) -> Self {
        let pg = std::env::var("ZOEN_TEST_PG").expect("explicit owned PostgreSQL URL");
        let cluster = std::env::var("FDB_CLUSTER_FILE").expect("explicit owned FDB cluster file");
        let nats = std::env::var("ZOEN_NATS_URL").expect("explicit owned NATS URL");
        let suffix = event_log::Signer::generate().unwrap().id();
        let database = format!("zoen_publish_{}", &suffix[..24]);
        let cell = types::new_id("publisher-recovery").unwrap();
        let mut admin = PgConnection::connect(&pg).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {database}")))
            .execute(&mut admin)
            .await
            .unwrap();
        let (base, _) = pg
            .rsplit_once('/')
            .expect("owned PostgreSQL URL has database");
        let database_url = format!("{base}/{database}");
        let mut nodes = Vec::new();
        for name in ["a", "b"] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let bind = listener.local_addr().unwrap();
            let config = crate::Config {
                database_url: database_url.clone(),
                bind,
                relay_name: bind.to_string(),
                blob_dir: directory.join(format!("blobs-{name}")),
                max_db_connections: 8,
                metrics_bind: None,
                apple_app_ids: vec![],
                android_packages: vec![],
                android_store_url: None,
                fdb_cluster_file: Some(cluster.clone()),
                fdb_cell: cell.clone(),
                nats_url: Some(nats.clone()),
                limits: String::new(),
                client_ip_header: None,
            };
            nodes.push(Node::new(config, listener).await);
        }
        let b = nodes.pop().unwrap();
        let a = nodes.pop().unwrap();
        assert_ne!(a.state.fanout.node, b.state.fanout.node);
        assert_eq!(a.state.fanout.bus_kind(), "nats");
        assert_eq!(b.state.fanout.bus_kind(), "nats");
        let log = crate::log::fdb::FdbLog::open(Some(&cluster), &cell).unwrap();
        Self {
            a,
            b,
            log,
            database,
            admin,
        }
    }

    pub async fn events(&self, space: &str) -> Vec<protocol::Sequenced> {
        let mut events = Vec::new();
        loop {
            let page = self
                .log
                .read(space, events.len() as u64, 1000)
                .await
                .unwrap();
            let count = page.len();
            events.extend(page);
            if count < 1000 {
                return events;
            }
        }
    }

    pub async fn close(mut self) {
        self.a.close().await;
        self.b.close().await;
        self.log.drop_cell().await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE {} WITH (FORCE)",
            self.database
        )))
        .execute(&mut self.admin)
        .await
        .unwrap();
    }
}

pub(super) fn count(device: &Device, space: &str, id: &str) -> usize {
    device
        .core
        .timeline(space.into())
        .unwrap_or_default()
        .iter()
        .filter(|e| e.id == id)
        .count()
}

pub(super) async fn until(label: &str, check: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(20), async {
        while !check() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{label}"));
}
