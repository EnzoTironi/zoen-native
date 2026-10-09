//! `GET /admin/metrics` (JSON) and `GET /admin` (a page that draws it). Only with
//! `Authorization: Bearer <ZOEN_ADMIN_TOKEN>`; without that variable both are 404, so a
//! relay that wasn't given a token has no admin surface at all.

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    Json,
};
use sha2::{Digest, Sha256};

use crate::Shared;

pub(crate) fn token() -> Option<String> {
    std::env::var("ZOEN_ADMIN_TOKEN")
        .ok()
        .filter(|t| t.len() >= 16)
}

/// Compares digests, so the time taken doesn't leak how much of the token matched.
pub(crate) fn authorized(headers: &HeaderMap, expected: &str) -> bool {
    let Some(given) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return false;
    };
    let a = Sha256::digest(given.trim().as_bytes());
    let b = Sha256::digest(expected.as_bytes());
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[derive(serde::Deserialize)]
pub struct Params {
    population: Option<String>,
}

pub async fn metrics(
    State(st): State<Shared>,
    headers: HeaderMap,
    Query(p): Query<Params>,
) -> Response {
    let Some(expected) = token() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !authorized(&headers, &expected) {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
        )
            .into_response();
    }
    if let Err(e) = st.analytics.flush(&st.pool).await {
        tracing::warn!(error = %e, "metrics flush before report failed");
    }
    let everyone = p.population.as_deref() == Some("everyone");
    let config = st.analytics.config.get().config.clone();
    match super::report::report(&st.pool, everyone, &config).await {
        Ok(r) => ([(header::CACHE_CONTROL, "no-store")], Json(r)).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "metrics report failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

pub async fn page() -> Response {
    if token().is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    (
        [
            (header::CACHE_CONTROL, "no-store"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'",
            ),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        Html(include_str!("dashboard.html")),
    )
        .into_response()
}
