//! Remote config (ADR 0044): `GET /v1/config` serves the newest stored document (or the
//! built-in one), `PUT /admin/config` stores a new one. Devices evaluate it themselves;
//! the relay uses it to check what they report (`POST /v1/report`).

use std::sync::RwLock;

use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use roda_log::verify_sig;
use roda_proto::experiments::{report_message, valid_unit, ClientReport, RemoteConfig};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::Shared;

/// A report is a few hundred bytes; anything bigger isn't one.
const MAX_REPORT: usize = 8 * 1024;
const MAX_SKEW_MS: i64 = 5 * 60 * 1000;

pub struct Current {
    pub config: RemoteConfig,
    pub json: String,
    pub etag: String,
}

impl Current {
    fn new(config: RemoteConfig) -> Current {
        let json = serde_json::to_string(&config).expect("config serializes");
        let etag = format!(
            "\"v{}-{}\"",
            config.version,
            &hex::encode(Sha256::digest(json.as_bytes()))[..12]
        );
        Current { config, json, etag }
    }
}

pub struct ConfigStore(RwLock<Current>);

impl Default for ConfigStore {
    fn default() -> Self {
        ConfigStore(RwLock::new(Current::new(RemoteConfig::builtin())))
    }
}

impl ConfigStore {
    pub fn get(&self) -> std::sync::RwLockReadGuard<'_, Current> {
        self.0.read().unwrap_or_else(|e| e.into_inner())
    }

    fn set(&self, c: RemoteConfig) {
        *self.0.write().unwrap_or_else(|e| e.into_inner()) = Current::new(c);
    }

    /// Picks up a document another node stored.
    pub async fn refresh(&self, pool: &PgPool) -> anyhow::Result<()> {
        let row: Option<(i64, serde_json::Value)> = sqlx::query_as(
            "SELECT version, document FROM remote_config ORDER BY version DESC LIMIT 1",
        )
        .fetch_optional(pool)
        .await?;
        if let Some((version, doc)) = row {
            if self.get().config.version != version as u64 {
                let mut c: RemoteConfig = serde_json::from_value(doc)?;
                c.version = version as u64;
                self.set(c);
            }
        }
        Ok(())
    }
}

pub async fn get(State(st): State<Shared>, headers: HeaderMap) -> Response {
    let cur = st.analytics.config.get();
    let fresh = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == cur.etag);
    let cache = [
        (header::ETAG, cur.etag.clone()),
        (header::CACHE_CONTROL, "public, max-age=300".to_string()),
    ];
    if fresh {
        return (StatusCode::NOT_MODIFIED, cache).into_response();
    }
    (
        cache,
        [(header::CONTENT_TYPE, "application/json")],
        cur.json.clone(),
    )
        .into_response()
}

pub async fn put(State(st): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(expected) = super::admin::token() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !super::admin::authorized(&headers, &expected) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(text) = std::str::from_utf8(&body) else {
        return (StatusCode::BAD_REQUEST, "not UTF-8").into_response();
    };
    let config = match RemoteConfig::parse(text) {
        Ok(c) => c,
        Err(e) => return (StatusCode::UNPROCESSABLE_ENTITY, e).into_response(),
    };
    let doc = match serde_json::to_value(&config) {
        Ok(d) => d,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let version: Result<i64, _> =
        sqlx::query_scalar("INSERT INTO remote_config (document) VALUES ($1) RETURNING version")
            .bind(doc)
            .fetch_one(&st.pool)
            .await;
    match version {
        Ok(v) => {
            let mut c = config;
            c.version = v as u64;
            st.analytics.config.set(c);
            tracing::info!(version = v, "remote config stored");
            Json(serde_json::json!({ "version": v })).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "remote config store failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// `POST /v1/report`, signed by a registered device like a blob upload.
pub async fn report(State(st): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let h = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };
    let (device, ts, sig) = (h("x-zoen-device"), h("x-zoen-ts"), h("x-zoen-sig"));
    let Ok(ts_ms) = ts.parse::<i64>() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if (now_ms() - ts_ms).abs() > MAX_SKEW_MS || body.len() > MAX_REPORT {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let sha = hex::encode(Sha256::digest(&body));
    if !verify_sig(&device, &report_message(&sha, ts_ms, &st.relay_name), &sig) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if st.limits.request_device.check(&device).is_err() {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let identity: Option<String> =
        match sqlx::query_scalar("SELECT identity FROM devices WHERE device = $1")
            .bind(&device)
            .fetch_optional(&st.pool)
            .await
        {
            Ok(i) => i,
            Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
        };
    let Some(identity) = identity else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let Ok(r) = serde_json::from_slice::<ClientReport>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let accepted = record(&st, &identity, r);
    Json(serde_json::json!({ "exposures_accepted": accepted })).into_response()
}

/// Keeps what checks out: a known source kind and clean campaign, arms the config really
/// gives this account, bounded health counts.
fn record(st: &Shared, identity: &str, r: ClientReport) -> usize {
    let a = &st.analytics;
    if let Some(src) = r.attribution {
        let campaign = src
            .campaign
            .as_deref()
            .and_then(roda_proto::experiments::clean_campaign);
        a.source(identity, src.kind.as_str(), campaign);
    }
    let cfg = a.config.get();
    if !valid_unit(&r.unit) {
        return 0;
    }
    let unit = r.unit.as_str();
    let mut accepted = 0;
    for e in r.exposures.iter().take(32) {
        let Some(asg) = cfg.config.assign(&e.flag, unit) else {
            continue;
        };
        if asg.enrolled && asg.variant == e.variant && cfg.config.flags[&e.flag].experiment {
            a.exposure(identity, &e.flag, &e.variant);
            accepted += 1;
        }
    }
    if let Some(h) = r.health {
        let (s, c) = (
            i64::from(h.sessions.min(1000)),
            i64::from(h.crashes.min(1000)),
        );
        a.count("client_sessions", s);
        a.count("client_crashes", c.min(s));
        for (key, flag) in &cfg.config.flags {
            if !flag.experiment || !flag.guardrails.iter().any(|g| g == "crash_free") {
                continue;
            }
            if let Some(asg) = cfg.config.assign(key, unit).filter(|x| x.enrolled) {
                a.count(&format!("client_sessions:{key}:{}", asg.variant), s);
                a.count(&format!("client_crashes:{key}:{}", asg.variant), c.min(s));
            }
        }
    }
    accepted
}
