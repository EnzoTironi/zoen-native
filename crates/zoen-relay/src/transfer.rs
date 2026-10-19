//! History transfers between two devices of one person (ADR 0045): encrypted chunks the
//! existing device puts and the new one takes, then deletes. The relay never has the key.
//!
//!   PUT    /v1/transfer/{id}/{n}   body = ciphertext; headers x-zoen-device, x-zoen-ts,
//!                                  x-zoen-sig (a linked device signs `transfer_message`)
//!   GET    /v1/transfer/{id}/{n}   → the bytes (the 256-bit id is the capability)
//!   DELETE /v1/transfer/{id}       signed like a put: the whole transfer goes
//!
//! A chunk already there answers 200 to a put, so an interrupted upload resumes by putting
//! everything again; a download resumes by getting what it doesn't have yet.

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use object_store::{path::Path as ObjPath, ObjectStoreExt, PutMode, PutOptions, PutPayload};
use roda_relay_admission::transfer as policy;
use sha2::{Digest, Sha256};

use crate::Shared;

/// Largest chunk.
pub use policy::MAX_CHUNK;

/// Ciphertext downloads use an unguessable capability; mutations still require
/// a linked device signature. Browser clients never send ambient credentials.
pub fn cors() -> tower_http::cors::CorsLayer {
    use axum::http::{HeaderName, Method};
    tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([Method::GET, Method::PUT, Method::DELETE])
        .allow_headers([
            header::CONTENT_TYPE,
            HeaderName::from_static("x-zoen-device"),
            HeaderName::from_static("x-zoen-ts"),
            HeaderName::from_static("x-zoen-sig"),
        ])
        .max_age(std::time::Duration::from_secs(86_400))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn err(code: StatusCode, msg: &'static str) -> Response {
    (code, msg).into_response()
}

fn policy_error(error: policy::Failure) -> Response {
    err(
        StatusCode::from_u16(error.status).expect("transfer policy HTTP status"),
        error.body,
    )
}

async fn active_device(st: &Shared, device: &str) -> Result<(), (StatusCode, &'static str)> {
    match crate::db::device_active(&st.pool, device).await {
        Ok(true) => Ok(()),
        Ok(false) => Err((StatusCode::FORBIDDEN, "unlinked device")),
        Err(_) => Err((StatusCode::SERVICE_UNAVAILABLE, "database unavailable")),
    }
}

fn signed_headers(headers: &HeaderMap) -> (&str, &str, &str) {
    let h = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    };
    (h("x-zoen-device"), h("x-zoen-ts"), h("x-zoen-sig"))
}

pub async fn put(
    State(st): State<Shared>,
    Path((id, n)): Path<(String, u32)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let sha = hex::encode(Sha256::digest(&body));
    let (device, ts, sig) = signed_headers(&headers);
    let admitted =
        match policy::put_policy(&id, n, body.len() as u64, &sha, device, ts, sig, now_ms()) {
            Ok(admitted) => admitted,
            Err(error) => return policy_error(error),
        };
    if let Err((status, body)) = active_device(&st, device).await {
        return err(status, body);
    }
    let kib = admitted.cost_kib;
    if let Err(wait) = st.limits.blob_kib_device.check_n(device, kib) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", wait.as_secs().max(1).to_string())],
            crate::limits::slow_down(wait),
        )
            .into_response();
    }
    let opts = PutOptions {
        mode: PutMode::Create,
        ..Default::default()
    };
    match st
        .blobs
        .put_opts(
            &ObjPath::from(admitted.key),
            PutPayload::from_bytes(body),
            opts,
        )
        .await
    {
        Ok(_) => StatusCode::CREATED.into_response(),
        Err(object_store::Error::AlreadyExists { .. }) => StatusCode::OK.into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "transfer put failed");
            err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable")
        }
    }
}

pub async fn get(State(st): State<Shared>, Path((id, n)): Path<(String, u32)>) -> Response {
    let key = match policy::get_key(&id, n) {
        Ok(key) => ObjPath::from(key),
        Err(error) => return policy_error(error),
    };
    match st.blobs.get(&key).await {
        Ok(r) => match r.bytes().await {
            Ok(b) => (
                [
                    (header::CONTENT_TYPE, "application/octet-stream"),
                    (header::CACHE_CONTROL, "no-store"),
                ],
                b,
            )
                .into_response(),
            Err(_) => err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable"),
        },
        Err(object_store::Error::NotFound { .. }) => err(StatusCode::NOT_FOUND, "missing"),
        Err(e) => {
            tracing::warn!(error = %e, "transfer get failed");
            err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable")
        }
    }
}

pub async fn delete(
    State(st): State<Shared>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let (device, ts, sig) = signed_headers(&headers);
    let prefix = match policy::delete_policy(&id, device, ts, sig, now_ms()) {
        Ok(prefix) => ObjPath::from(prefix),
        Err(error) => return policy_error(error),
    };
    if let Err((status, body)) = active_device(&st, device).await {
        return err(status, body);
    }
    let mut listed = st.blobs.list(Some(&prefix));
    let mut gone = 0;
    while let Some(meta) = listed.next().await {
        let Ok(meta) = meta else {
            return err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable");
        };
        if st.blobs.delete(&meta.location).await.is_err() {
            return err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable");
        }
        gone += 1;
    }
    tracing::info!(chunks = gone, "transfer deleted");
    StatusCode::NO_CONTENT.into_response()
}
