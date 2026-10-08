//! Content-addressed blob store for media. Clients encrypt before uploading (ADR 0007), so
//! the relay only ever holds ciphertext addressed by its sha256.
//!
//!   PUT /v1/blobs/{sha256}   body = ciphertext; headers x-zoen-device, x-zoen-ts, x-zoen-sig
//!   GET /v1/blobs/{sha256}   → the bytes (immutable; the hash is the capability)
//!
//! Storage is any `object_store` backend: a directory in dev, S3-compatible (AWS, R2,
//! Tigris, MinIO) in production.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use object_store::{
    path::Path as ObjPath, ObjectStore, ObjectStoreExt, PutMode, PutOptions, PutPayload,
};
use roda_log::verify_sig;
use roda_proto::blob_put_message;
use sha2::{Digest, Sha256};

use crate::Shared;

/// Largest blob accepted (a resized photo is ~0.5 MB; voice notes and files come later).
pub const MAX_BLOB_BYTES: usize = 25 * 1024 * 1024;
/// How far a signed upload's timestamp may be from the relay's clock.
const MAX_SKEW_MS: i64 = 5 * 60 * 1000;

/// Where blobs live, from the environment:
/// `ZOEN_S3_BUCKET` (+ the usual `AWS_*` variables, `AWS_ENDPOINT_URL` for R2/Tigris/MinIO)
/// selects S3; otherwise a directory (`ZOEN_BLOB_DIR`, default `./blobs`).
pub fn store_from_env(dir: &std::path::Path) -> anyhow::Result<(Arc<dyn ObjectStore>, String)> {
    if let Ok(bucket) = std::env::var("ZOEN_S3_BUCKET") {
        let mut b = object_store::aws::AmazonS3Builder::from_env().with_bucket_name(&bucket);
        if let Ok(endpoint) =
            std::env::var("AWS_ENDPOINT_URL_S3").or_else(|_| std::env::var("AWS_ENDPOINT_URL"))
        {
            b = b
                .with_endpoint(endpoint)
                .with_virtual_hosted_style_request(false);
        }
        return Ok((Arc::new(b.build()?), format!("s3://{bucket}")));
    }
    std::fs::create_dir_all(dir)?;
    let local = object_store::local::LocalFileSystem::new_with_prefix(dir)?;
    Ok((Arc::new(local), dir.display().to_string()))
}

fn valid_sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn key(sha: &str) -> ObjPath {
    // Two-level fan-out keeps directories (and S3 listings) small.
    ObjPath::from(format!("blobs/{}/{}", &sha[..2], sha))
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

pub async fn put(
    State(st): State<Shared>,
    Path(sha): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !valid_sha(&sha) {
        return err(StatusCode::BAD_REQUEST, "bad hash");
    }
    let h = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };
    let (device, ts, sig) = (h("x-zoen-device"), h("x-zoen-ts"), h("x-zoen-sig"));
    let Ok(ts_ms) = ts.parse::<i64>() else {
        return err(StatusCode::UNAUTHORIZED, "missing signature");
    };
    if (now_ms() - ts_ms).abs() > MAX_SKEW_MS {
        return err(StatusCode::UNAUTHORIZED, "stale signature");
    }
    if !verify_sig(
        &device,
        &blob_put_message(&sha, ts_ms, &st.relay_name),
        &sig,
    ) {
        return err(StatusCode::UNAUTHORIZED, "bad signature");
    }
    match crate::db::device_known(&st.pool, &device).await {
        Ok(true) => {}
        Ok(false) => return err(StatusCode::FORBIDDEN, "unknown device"),
        Err(_) => return err(StatusCode::SERVICE_UNAVAILABLE, "database unavailable"),
    }
    if body.is_empty() || body.len() > MAX_BLOB_BYTES {
        return err(StatusCode::PAYLOAD_TOO_LARGE, "empty or too large");
    }
    let kib = body.len().div_ceil(1024).min(u32::MAX as usize) as u32;
    if let Err(wait) = st.limits.blob_kib_device.check_n(&device, kib) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", wait.as_secs().max(1).to_string())],
            crate::limits::slow_down(wait),
        )
            .into_response();
    }
    if hex::encode(Sha256::digest(&body)) != sha {
        return err(StatusCode::BAD_REQUEST, "hash mismatch");
    }
    let n = body.len();
    let opts = PutOptions {
        mode: PutMode::Create,
        ..Default::default()
    };
    match st
        .blobs
        .put_opts(&key(&sha), PutPayload::from_bytes(body), opts)
        .await
    {
        Ok(_) => {
            st.metrics.blob_stored(n);
            tracing::info!(sha = %&sha[..12], bytes = n, "blob stored");
            StatusCode::CREATED.into_response()
        }
        Err(object_store::Error::AlreadyExists { .. }) => StatusCode::OK.into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "blob put failed");
            err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable")
        }
    }
}

pub async fn get(State(st): State<Shared>, Path(sha): Path<String>) -> Response {
    if !valid_sha(&sha) {
        return err(StatusCode::BAD_REQUEST, "bad hash");
    }
    match st.blobs.get(&key(&sha)).await {
        Ok(r) => match r.bytes().await {
            Ok(b) => (
                [
                    (header::CONTENT_TYPE, "application/octet-stream"),
                    (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
                ],
                b,
            )
                .into_response(),
            Err(_) => err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable"),
        },
        Err(object_store::Error::NotFound { .. }) => err(StatusCode::NOT_FOUND, "missing"),
        Err(e) => {
            tracing::warn!(error = %e, "blob get failed");
            err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable")
        }
    }
}
