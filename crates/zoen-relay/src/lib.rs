//! # zoen-relay
//!
//! The Zoen Sync relay. It does three things and nothing else:
//! 1. **Orders**: each Space has one append-only log in FoundationDB; one transaction
//!    assigns the sequence number and chains the hash (ADR 0008).
//! 2. **Stores**: signed envelopes, verbatim (ciphertext in E2EE Spaces).
//! 3. **Fans out**: sequenced events and ephemeral signals (typing, presence) to the
//!    members who are online; everyone else catches up with `Sync` on reconnect.
//!
//! Membership is the only thing the relay must understand, because it decides who may
//! write and who receives. Everything else is the clients' business.

pub mod analytics;
pub mod blobs;
pub mod db;
pub mod fanout;
pub mod hub;
pub mod limits;
pub mod log;
pub mod metrics;
pub mod ownership;
pub mod pseudonym;
pub mod session;
pub mod telemetry;
pub mod transfer;

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    extract::{DefaultBodyLimit, State, WebSocketUpgrade},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    serve::ListenerExt,
    Router,
};
use sqlx::{postgres::PgPoolOptions, PgPool};

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub bind: SocketAddr,
    /// Public name of this relay (e.g. `relay.zoen.app` or `127.0.0.1:8787`). Devices
    /// sign it into their login so a login can't be replayed against another relay.
    pub relay_name: String,
    pub blob_dir: PathBuf,
    pub max_db_connections: u32,
    /// When set, `/metrics` is served only here (a private port), not on the public listener.
    pub metrics_bind: Option<SocketAddr>,
    /// `TEAMID.bundle` app IDs allowed to use passkeys and universal links on this host.
    pub apple_app_ids: Vec<String>,
    /// FoundationDB cluster file (`None` = `FDB_CLUSTER_FILE` or the system default).
    pub fdb_cluster_file: Option<String>,
    /// Key prefix for this cell's data; tests use a fresh one each.
    pub fdb_cell: String,
    /// NATS for fan-out between relay nodes (`None` = this node is the whole cell).
    pub nats_url: Option<String>,
    /// Rate-limit overrides on top of the defaults (`limits::Limits::from_spec`).
    pub limits: String,
    /// A header the edge in front of the relay sets to the client's address and that clients
    /// can't forge there (`fly-client-ip` on Fly). `None`: the TCP peer address.
    pub client_ip_header: Option<String>,
}

pub struct AppState {
    pub pool: PgPool,
    /// Separate bounded lanes: an authorized Sync can fill its output queue while its
    /// writer checks authorization without waiting for the inbound lane's connection.
    pub session_auth: PgPool,
    pub delivery_auth: PgPool,
    pub log: Arc<dyn log::LogStore>,
    pub fanout: fanout::Fanout,
    pub limits: limits::Limits,
    pub client_ip_header: Option<axum::http::HeaderName>,
    pub relay_name: String,
    pub metrics: metrics::Metrics,
    pub blobs: Arc<dyn object_store::ObjectStore>,
    pub apple_app_ids: Vec<String>,
    /// This node's Space-partition leases (S4). One process owns every partition until
    /// S5 brings a multi-node lease exchange over NATS.
    pub owner: ownership::NodeOwner,
    /// Product metrics, counted without content (ADR 0043).
    pub analytics: analytics::Analytics,
}

pub type Shared = Arc<AppState>;

pub async fn connect(cfg: &Config) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(cfg.max_db_connections)
        .connect(&cfg.database_url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

pub fn router(state: Shared) -> Router {
    Router::new()
        .route("/v1/sync", get(ws))
        .route(
            "/v1/blobs/{sha}",
            put(blobs::put)
                .get(blobs::get)
                .layer(DefaultBodyLimit::max(blobs::MAX_BLOB_BYTES + 1024)),
        )
        .route(
            "/v1/transfer/{id}/{n}",
            put(transfer::put)
                .get(transfer::get)
                .layer(DefaultBodyLimit::max(transfer::MAX_CHUNK + 1024)),
        )
        .route("/v1/transfer/{id}", delete(transfer::delete))
        .route(
            "/.well-known/apple-app-site-association",
            get(apple_app_site_association),
        )
        .route("/admin", get(analytics::admin::page))
        .route("/admin/metrics", get(analytics::admin::metrics))
        .route("/admin/config", put(analytics::config::put))
        .route("/v1/config", get(analytics::config::get))
        .route("/v1/report", post(analytics::config::report))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .with_state(state)
        .layer(
            tower_http::trace::TraceLayer::new_for_http()
                .make_span_with(http_span)
                .on_response(
                    |res: &axum::http::Response<_>,
                     _: std::time::Duration,
                     span: &tracing::Span| {
                        span.record("http.response.status_code", res.status().as_u16());
                    },
                ),
        )
}

/// The method and the matched route template, never the URL: paths carry blob hashes and
/// query strings could carry anything. Health probes aren't traced.
fn http_span<B>(req: &axum::http::Request<B>) -> tracing::Span {
    let route = req
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map_or("unmatched", |m| m.as_str());
    if matches!(route, "/healthz" | "/readyz") {
        return tracing::Span::none();
    }
    tracing::info_span!(
        target: "zoen_relay::http",
        "http",
        otel.name = format!("{} {route}", req.method()),
        otel.kind = "server",
        http.request.method = %req.method(),
        http.route = route,
        http.response.status_code = tracing::field::Empty,
    )
}

pub fn metrics_router(state: Shared) -> Router {
    Router::new()
        .route("/metrics", get(metrics))
        .with_state(state)
}

pub async fn build(cfg: &Config) -> anyhow::Result<(Router, Shared)> {
    let pool = connect(cfg).await?;
    let auth_pool = || {
        PgPoolOptions::new()
            .min_connections(0)
            .max_connections(cfg.max_db_connections.clamp(1, 16))
            .acquire_timeout(std::time::Duration::from_secs(2))
            .connect_lazy(&cfg.database_url)
    };
    let session_auth = auth_pool()?;
    let delivery_auth = auth_pool()?;
    let (blobs, where_) = blobs::store_from_env(&cfg.blob_dir)?;
    tracing::info!(blobs = %where_, "blob store ready");
    let log = log::fdb::FdbLog::open(cfg.fdb_cluster_file.as_deref(), &cfg.fdb_cell)?;
    tracing::info!(cell = %cfg.fdb_cell, "log store ready (FoundationDB)");
    let node = fanout::new_node_id();
    let fanout = match &cfg.nats_url {
        Some(url) => fanout::Fanout::nats(node.clone(), url, &cfg.fdb_cell).await?,
        None => fanout::Fanout::local(node.clone()),
    };
    tracing::info!(node = %node, bus = fanout.bus_kind(), "fan-out ready");
    let owner = ownership::NodeOwner::claim_all(&node);
    tracing::info!(
        node = %owner.node,
        partitions = ownership::PARTITION_COUNT,
        "space ownership ready (single node owns every partition)"
    );
    let analytics = analytics::Analytics::load(&pool).await?;
    let state = Arc::new(AppState {
        pool,
        session_auth,
        delivery_auth,
        log: Arc::new(log),
        fanout,
        limits: limits::Limits::from_spec(&cfg.limits)?,
        client_ip_header: cfg
            .client_ip_header
            .as_deref()
            .map(axum::http::HeaderName::try_from)
            .transpose()?,
        relay_name: cfg.relay_name.clone(),
        metrics: metrics::Metrics::default(),
        blobs,
        apple_app_ids: cfg.apple_app_ids.clone(),
        owner,
        analytics,
    });
    analytics::spawn(state.clone());
    let app = match cfg.metrics_bind {
        Some(_) => router(state.clone()),
        None => router(state.clone()).merge(metrics_router(state.clone())),
    };
    Ok((app, state))
}

/// Initial read and write buffer per WebSocket (see `ws`, ADR 0022).
const SOCKET_BUFFER: usize = 8 * 1024;

/// Runs until Ctrl-C / SIGTERM.
pub async fn serve(cfg: Config) -> anyhow::Result<()> {
    let (app, state) = build(&cfg).await?;
    if let Some(bind) = cfg.metrics_bind {
        let listener = tokio::net::TcpListener::bind(bind).await?;
        tracing::info!(bind = %bind, "metrics listening");
        let st = state.clone();
        tokio::spawn(async move { axum::serve(listener, metrics_router(st)).await });
    }
    // Frames are small and latency is the product: no Nagle delay on any connection.
    let listener = tokio::net::TcpListener::bind(cfg.bind)
        .await?
        .tap_io(|tcp| {
            let _ = tcp.set_nodelay(true);
        });
    tracing::info!(bind = %cfg.bind, relay = %cfg.relay_name, "zoen-relay listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown_signal().await;
        // What this node counted since its last flush, before it goes.
        if let Err(e) = state.analytics.flush(&state.pool).await {
            tracing::warn!(error = %e, "metrics flush at shutdown failed");
        }
    })
    .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    tracing::info!("shutting down");
}

async fn ws(
    ws: WebSocketUpgrade,
    State(st): State<Shared>,
    ClientIp(ip): ClientIp,
) -> axum::response::Response {
    // Frames are small; the default 128 KiB read and write buffers per socket would cost a
    // million connections 256 GB of reserved memory. Both still grow for a large sync batch.
    ws.max_message_size(session::MAX_FRAME)
        .read_buffer_size(SOCKET_BUFFER)
        .write_buffer_size(SOCKET_BUFFER)
        .on_upgrade(move |socket| session::run(socket, st, ip))
}

/// The client's address: the trusted edge header when one is configured, else the TCP peer.
pub struct ClientIp(pub String);

impl axum::extract::FromRequestParts<Shared> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        st: &Shared,
    ) -> Result<Self, Self::Rejection> {
        let from_header = st.client_ip_header.as_ref().and_then(|h| {
            parts
                .headers
                .get(h)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split(',').next())
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        });
        let peer = || {
            parts
                .extensions
                .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                .map(|c| c.0.ip().to_string())
        };
        Ok(ClientIp(
            from_header
                .or_else(peer)
                .unwrap_or_else(|| "unknown".into()),
        ))
    }
}

async fn healthz() -> &'static str {
    "ok"
}

/// Lets the app use passkeys (`webcredentials`) and universal links (`applinks`) on this host.
async fn apple_app_site_association(State(st): State<Shared>) -> axum::response::Response {
    if st.apple_app_ids.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let body = serde_json::json!({
        "applinks": { "details": [{ "appIDs": st.apple_app_ids, "components": [{ "/": "/i/*" }, { "/": "/u/*" }] }] },
        "webcredentials": { "apps": st.apple_app_ids },
    });
    ([("content-type", "application/json")], body.to_string()).into_response()
}

async fn readyz(State(st): State<Shared>) -> impl IntoResponse {
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&st.pool)
        .await
    {
        Ok(_) => (StatusCode::OK, "ready"),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "database unavailable"),
    }
}

async fn metrics(State(st): State<Shared>) -> impl IntoResponse {
    (
        [("content-type", "text/plain; version=0.0.4")],
        st.metrics.render(st.fanout.hub.online_sessions())
            + &st.fanout.render_metrics()
            + &st.limits.render_metrics(),
    )
}
