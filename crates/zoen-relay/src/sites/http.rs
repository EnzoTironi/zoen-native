use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use protocol::sites::{SignedMutation, MAX_HTML_BYTES};
use serde::Deserialize;

use super::{Error, Service};

// HTML may have JSON escapes, but decoded bytes are separately bounded by policy.
const MAX_REQUEST_BYTES: usize = MAX_HTML_BYTES as usize * 6 + 16 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    signed: SignedMutation,
    html: Option<String>,
}

pub fn management_router(service: Service) -> Router {
    Router::new()
        .route("/v1/sites/publications", post(publish))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .layer(axum::middleware::from_fn_with_state(
            service.clone(),
            admission,
        ))
        .with_state(service)
}

/// Intentionally has no API, directory, cookies, CORS credentials or ambient
/// authentication. Deploy this listener on a different host from management.
pub fn public_router(service: Service) -> Router {
    Router::new()
        .route("/s/{site_id}", get(document))
        .layer(axum::middleware::from_fn_with_state(
            service.clone(),
            admission,
        ))
        .with_state(service)
}

// Before JSON/body extraction or storage fetch, not merely before the SQL write.
// This bounds handler work, not socket buffers or total process RSS.
async fn admission(
    State(service): State<Service>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let Ok(_permit) = service.http_in_flight.try_acquire() else {
        return error(Error::Limited);
    };
    next.run(request).await
}

fn error(error: Error) -> Response {
    let (code, body) = match error {
        Error::Invalid => (StatusCode::BAD_REQUEST, "invalid publication"),
        Error::Unauthorized => (StatusCode::FORBIDDEN, "publication not authorized"),
        Error::Conflict => (StatusCode::CONFLICT, "publication changed"),
        Error::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "publication unavailable"),
        Error::Missing => (StatusCode::NOT_FOUND, "not found"),
        Error::Limited => (StatusCode::TOO_MANY_REQUESTS, "publication busy"),
    };
    (code, [(header::CACHE_CONTROL, "no-store")], body).into_response()
}

async fn publish(State(service): State<Service>, Json(request): Json<Request>) -> Response {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0);
    match service
        .apply(
            &request.signed,
            request.html.as_deref().map(str::as_bytes),
            now_ms,
        )
        .await
    {
        Ok(receipt) => ([(header::CACHE_CONTROL, "no-store")], Json(receipt)).into_response(),
        Err(reason) => error(reason),
    }
}

async fn document(State(service): State<Service>, Path(site_id): Path<String>) -> Response {
    match service.public_bytes(&site_id).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-store"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                (header::REFERRER_POLICY, "no-referrer"),
                (header::CONTENT_SECURITY_POLICY,
                 "sandbox allow-scripts; default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; connect-src 'none'; form-action 'none'; base-uri 'none'; frame-src 'none'; frame-ancestors 'none'"),
                (header::HeaderName::from_static("permissions-policy"),
                 "camera=(), microphone=(), geolocation=(), payment=(), usb=()"),
                (header::HeaderName::from_static("cross-origin-opener-policy"), "same-origin"),
                (header::HeaderName::from_static("cross-origin-resource-policy"), "same-origin"),
            ],
            bytes,
        ).into_response(),
        Err(reason) => error(reason),
    }
}
