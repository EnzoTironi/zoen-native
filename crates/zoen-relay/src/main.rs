//! `zoen-relay` — run the Zoen Sync relay.
//!
//! Configuration (env, flags win):
//!   DATABASE_URL        postgres://… (required unless --embedded-pg)
//!   ZOEN_BIND           0.0.0.0:8787
//!   ZOEN_RELAY_NAME     public name signed into logins (default: the bind address)
//!   ZOEN_BLOB_DIR       ./blobs
//!   ZOEN_METRICS_BIND   serve /metrics only on this private address (default: the public one)
//!   ZOEN_APPLE_APP_IDS  comma-separated TEAMID.bundle IDs for apple-app-site-association
//!   LOG_FORMAT=json     structured logs (default: pretty)
//!   RUST_LOG            tracing filter (default: info)
//!   OTEL_EXPORTER_OTLP_ENDPOINT  export traces and logs over OTLP/HTTP (unset = stdout only);
//!                       the standard OTEL_* variables (sampler, headers) apply
//!
//!   FDB_CLUSTER_FILE    FoundationDB cluster file (default: the system one)
//!   ZOEN_FDB_CELL       the cell this relay's logs live in (default: c0)
//!   ZOEN_NATS_URL       NATS for fan-out between relay nodes (--nats; unset = single node)
//!   ZOEN_LIMITS         rate-limit overrides, e.g. `publish_device=20/s:60,register_ip=5/h:10`
//!   ZOEN_CLIENT_IP_HEADER  edge header carrying the client address (`fly-client-ip` on Fly)
//!
//! Dev: `zoen-relay --embedded-pg .tools/pg` downloads and runs a private Postgres
//! (needs the `embedded-pg` feature).
//!
//! Operator commands on the cell's log store (no Postgres needed):
//!   zoen-relay log read <space>           one hex-encoded `Sequenced` per line, in order
//!   zoen-relay log spaces <identity>      the Spaces an identity belongs to
//!   zoen-relay log drop-cell <cell>       clear the whole cell; <cell> must repeat ZOEN_FDB_CELL

use std::net::SocketAddr;

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn fdb_cell() -> String {
    std::env::var("ZOEN_FDB_CELL").unwrap_or_else(|_| "c0".into())
}

async fn log_command(args: &[String]) -> anyhow::Result<()> {
    use zoen_relay::log::{fdb::FdbLog, LogStore};
    let cell = fdb_cell();
    let log = FdbLog::open(std::env::var("FDB_CLUSTER_FILE").ok().as_deref(), &cell)?;
    match args {
        [cmd, space] if cmd == "read" => {
            let mut from = 0;
            loop {
                let page = log.read(space, from, 1000).await?;
                for ev in &page {
                    println!("{}", hex::encode(ev.encode()));
                    from = ev.seq + 1;
                }
                if page.len() < 1000 {
                    return Ok(());
                }
            }
        }
        [cmd, who] if cmd == "spaces" => {
            for s in log.spaces_of(who).await? {
                println!("{s}");
            }
            Ok(())
        }
        [cmd, name] if cmd == "drop-cell" => {
            anyhow::ensure!(
                *name == cell,
                "refusing: {name} isn't ZOEN_FDB_CELL ({cell})"
            );
            Ok(log.drop_cell().await?)
        }
        _ => anyhow::bail!(
            "usage: zoen-relay log read <space> | spaces <identity> | drop-cell <cell>"
        ),
    }
}

fn main() -> anyhow::Result<()> {
    let runtime = || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("log") {
        let _fdb = unsafe { foundationdb::boot() };
        return runtime()?.block_on(log_command(&args[1..]));
    }
    let _telemetry = zoen_relay::telemetry::init("zoen-relay")?;
    runtime()?.block_on(relay())
}

async fn relay() -> anyhow::Result<()> {
    let bind: SocketAddr = arg("--bind")
        .or_else(|| std::env::var("ZOEN_BIND").ok())
        .unwrap_or_else(|| "0.0.0.0:8787".into())
        .parse()?;
    let relay_name = arg("--relay-name")
        .or_else(|| std::env::var("ZOEN_RELAY_NAME").ok())
        .unwrap_or_else(|| bind.to_string());
    let blob_dir = arg("--blob-dir")
        .or_else(|| std::env::var("ZOEN_BLOB_DIR").ok())
        .unwrap_or_else(|| "blobs".into());

    #[cfg(feature = "embedded-pg")]
    let _pg;
    let database_url = match arg("--embedded-pg").or_else(|| std::env::var("ZOEN_EMBEDDED_PG").ok())
    {
        #[cfg(feature = "embedded-pg")]
        Some(dir) => {
            let (pg, url) = embedded::start(&dir).await?;
            _pg = pg;
            url
        }
        #[cfg(not(feature = "embedded-pg"))]
        Some(_) => anyhow::bail!("--embedded-pg needs a build with `--features embedded-pg`"),
        None => arg("--database-url")
            .or_else(|| std::env::var("DATABASE_URL").ok())
            .ok_or_else(|| anyhow::anyhow!("set DATABASE_URL or pass --embedded-pg <dir>"))?,
    };

    let _fdb = unsafe { foundationdb::boot() };
    zoen_relay::serve(zoen_relay::Config {
        fdb_cluster_file: std::env::var("FDB_CLUSTER_FILE").ok(),
        fdb_cell: fdb_cell(),
        nats_url: arg("--nats").or_else(|| std::env::var("ZOEN_NATS_URL").ok()),
        limits: std::env::var("ZOEN_LIMITS").unwrap_or_default(),
        client_ip_header: std::env::var("ZOEN_CLIENT_IP_HEADER")
            .ok()
            .filter(|v| !v.is_empty()),
        database_url,
        bind,
        relay_name,
        blob_dir: blob_dir.into(),
        max_db_connections: std::env::var("ZOEN_DB_POOL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(16),
        metrics_bind: std::env::var("ZOEN_METRICS_BIND")
            .ok()
            .map(|v| v.parse())
            .transpose()?,
        apple_app_ids: std::env::var("ZOEN_APPLE_APP_IDS")
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
    })
    .await
}

#[cfg(feature = "embedded-pg")]
mod embedded {
    use postgresql_embedded::{PostgreSQL, Settings};

    /// A persistent private Postgres under `dir` (binaries in `dir/install`, data in
    /// `dir/data`). Nothing is installed system-wide.
    pub async fn start(dir: &str) -> anyhow::Result<(PostgreSQL, String)> {
        let dir = std::path::Path::new(dir)
            .canonicalize()
            .unwrap_or_else(|_| dir.into());
        std::fs::create_dir_all(&dir)?;
        let settings = Settings {
            // Zonky's binaries come from Maven Central: no GitHub API rate limits.
            releases_url: "https://github.com/zonkyio/embedded-postgres-binaries".into(),
            version: postgresql_embedded::VersionReq::parse("^17")?,
            installation_dir: dir.join("install"),
            data_dir: dir.join("data"),
            password_file: dir.join(".pgpass"),
            port: std::env::var("ZOEN_EMBEDDED_PG_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(55433),
            temporary: false,
            password: "zoen-dev-only".into(),
            // The first start on a laptop (initdb, cold caches) can take longer than the 5 s default.
            timeout: Some(std::time::Duration::from_secs(60)),
            ..Settings::default()
        };
        let mut pg = PostgreSQL::new(settings);
        pg.setup().await?;
        pg.start().await?;
        if !pg.database_exists("zoen_relay").await? {
            pg.create_database("zoen_relay").await?;
        }
        let url = pg.settings().url("zoen_relay");
        tracing::info!(dir = %dir.display(), "embedded postgres up");
        Ok((pg, url))
    }
}
