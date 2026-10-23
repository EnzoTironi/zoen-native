use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use protocol::sites::{SignedInspection, SignedMutation, MAX_HTML_BYTES};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};

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
        .route(
            "/v1/sites/inspect",
            post(inspect).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .layer(axum::middleware::from_fn_with_state(
            HttpGate::new(),
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
            HttpGate::new(),
            admission,
        ))
        .with_state(service)
}

// Before JSON/body extraction or storage fetch, not merely before the SQL write.
// This bounds handler work, not socket buffers or total process RSS.
#[derive(Clone)]
struct HttpGate {
    slots: Arc<tokio::sync::Semaphore>,
    deadline: Duration,
}

impl HttpGate {
    fn new() -> Self {
        Self {
            slots: Arc::new(tokio::sync::Semaphore::new(16)),
            deadline: Duration::from_secs(20),
        }
    }
}

async fn admission(
    State(gate): State<HttpGate>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let Ok(_permit) = gate.slots.try_acquire() else {
        return error(Error::Limited);
    };
    match tokio::time::timeout(gate.deadline, next.run(request)).await {
        Ok(response) => response,
        Err(_) => (
            StatusCode::REQUEST_TIMEOUT,
            [(header::CACHE_CONTROL, "no-store")],
            "request expired",
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn sites_unfinished_body_expires_and_releases_its_handler_slot() {
        let gate = HttpGate {
            deadline: Duration::from_millis(20),
            ..HttpGate::new()
        };
        let router = Router::new()
            .route("/", post(|bytes: axum::body::Bytes| async { bytes }))
            .layer(axum::middleware::from_fn_with_state(
                gate.clone(),
                admission,
            ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        stream
            .write_all(b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\n\r\n")
            .await
            .unwrap();
        let mut bytes = [0; 512];
        let read = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut bytes))
            .await
            .unwrap()
            .unwrap();
        assert!(std::str::from_utf8(&bytes[..read])
            .unwrap()
            .starts_with("HTTP/1.1 408"));
        assert_eq!(gate.slots.available_permits(), 16);
        let response = reqwest::Client::new()
            .post(format!("http://{address}/"))
            .body("ready")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.bytes().await.unwrap().as_ref(), b"ready");
        server.abort();
    }
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
    match service
        .apply(
            &request.signed,
            request.html.as_deref().map(str::as_bytes),
            now_ms(),
        )
        .await
    {
        Ok(receipt) => ([(header::CACHE_CONTROL, "no-store")], Json(receipt)).into_response(),
        Err(reason) => error(reason),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

async fn inspect(
    State(service): State<Service>,
    Json(request): Json<SignedInspection>,
) -> Response {
    match service.inspect(&request, now_ms()).await {
        Ok(inspection) => ([(header::CACHE_CONTROL, "no-store")], Json(inspection)).into_response(),
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
