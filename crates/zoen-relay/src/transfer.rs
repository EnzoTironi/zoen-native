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
use roda_log::verify_sig;
use roda_proto::transfer_message;
use sha2::{Digest, Sha256};

use crate::Shared;

/// Largest chunk.
pub const MAX_CHUNK: usize = 4 * 1024 * 1024;
/// Chunks in one transfer at most.
const MAX_CHUNKS: u32 = 4096;
const MAX_SKEW_MS: i64 = 5 * 60 * 1000;

fn valid_id(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn prefix(id: &str) -> ObjPath {
    ObjPath::from(format!("transfers/{id}"))
}

fn key(id: &str, n: u32) -> ObjPath {
    ObjPath::from(format!("transfers/{id}/{n}"))
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

/// A linked device's signature over `msg(ts)`, or the response refusing it.
#[allow(clippy::result_large_err)] // the error is the HTTP answer itself
async fn signed_by_device(
    st: &Shared,
    headers: &HeaderMap,
    msg: impl Fn(i64) -> Vec<u8>,
) -> Result<String, Response> {
    let h = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };
    let (device, ts, sig) = (h("x-zoen-device"), h("x-zoen-ts"), h("x-zoen-sig"));
    let Ok(ts_ms) = ts.parse::<i64>() else {
        return Err(err(StatusCode::UNAUTHORIZED, "missing signature"));
    };
    if (now_ms() - ts_ms).abs() > MAX_SKEW_MS {
        return Err(err(StatusCode::UNAUTHORIZED, "stale signature"));
    }
    if !verify_sig(&device, &msg(ts_ms), &sig) {
        return Err(err(StatusCode::UNAUTHORIZED, "bad signature"));
    }
    match crate::db::device_known(&st.pool, &device).await {
        Ok(true) => {}
        Ok(false) => return Err(err(StatusCode::FORBIDDEN, "unknown device")),
        Err(_) => return Err(err(StatusCode::SERVICE_UNAVAILABLE, "database unavailable")),
    }
    match crate::db::device_revoked(&st.pool, &device).await {
        Ok(false) => Ok(device),
        Ok(true) => Err(err(StatusCode::FORBIDDEN, "unlinked device")),
        Err(_) => Err(err(StatusCode::SERVICE_UNAVAILABLE, "database unavailable")),
    }
}

pub async fn put(
    State(st): State<Shared>,
    Path((id, n)): Path<(String, u32)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !valid_id(&id) || n >= MAX_CHUNKS {
        return err(StatusCode::BAD_REQUEST, "bad transfer");
    }
    if body.is_empty() || body.len() > MAX_CHUNK {
        return err(StatusCode::PAYLOAD_TOO_LARGE, "empty or too large");
    }
    let sha = hex::encode(Sha256::digest(&body));
    let device = match signed_by_device(&st, &headers, |ts| {
        transfer_message("put", &id, &n.to_string(), &sha, ts)
    })
    .await
    {
        Ok(d) => d,
        Err(r) => return r,
    };
    let kib = body.len().div_ceil(1024).min(u32::MAX as usize) as u32;
    if let Err(wait) = st.limits.blob_kib_device.check_n(&device, kib) {
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
        .put_opts(&key(&id, n), PutPayload::from_bytes(body), opts)
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
    if !valid_id(&id) || n >= MAX_CHUNKS {
        return err(StatusCode::BAD_REQUEST, "bad transfer");
    }
    match st.blobs.get(&key(&id, n)).await {
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
    if !valid_id(&id) {
        return err(StatusCode::BAD_REQUEST, "bad transfer");
    }
    if let Err(r) = signed_by_device(&st, &headers, |ts| {
        transfer_message("delete", &id, "", "", ts)
    })
    .await
    {
        return r;
    }
    let mut listed = st.blobs.list(Some(&prefix(&id)));
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
