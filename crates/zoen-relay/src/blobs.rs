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
use relay_admission::blobs as policy;

use crate::Shared;

/// Largest encrypted media object accepted by the shared policy.
pub use policy::MAX_BLOB_BYTES;

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
        StatusCode::from_u16(error.status).expect("media policy HTTP status"),
        error.body,
    )
}

pub async fn put(
    State(st): State<Shared>,
    Path(sha): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let h = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };
    let (device, ts, sig) = (h("x-zoen-device"), h("x-zoen-ts"), h("x-zoen-sig"));
    let admitted = match policy::put_policy(&sha, &device, &ts, &sig, &st.relay_name, now_ms()) {
        Ok(admitted) => admitted,
        Err(error) => return policy_error(error),
    };
    match crate::db::device_active(&st.pool, &device).await {
        Ok(true) => {}
        Ok(false) => return err(StatusCode::FORBIDDEN, "unlinked device"),
        Err(_) => return err(StatusCode::SERVICE_UNAVAILABLE, "database unavailable"),
    }
    let kib = match policy::body_cost(body.len() as u64) {
        Ok(cost) => cost,
        Err(error) => return policy_error(error),
    };
    if let Err(wait) = st.limits.blob_kib_device.check_n(&device, kib) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", wait.as_secs().max(1).to_string())],
            crate::limits::slow_down(wait),
        )
            .into_response();
    }
    if let Err(error) = policy::verify_body(&body, &sha) {
        return policy_error(error);
    }
    let n = body.len();
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
        Ok(_) => {
            st.metrics.blob_stored(n);
            st.analytics.count("blobs_stored", 1);
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

/// CORS for the browser client: GET and signed PUT from any origin, no credentials.
pub fn cors() -> tower_http::cors::CorsLayer {
    use axum::http::{HeaderName, Method};
    tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([Method::GET, Method::PUT])
        .allow_headers([
            header::CONTENT_TYPE,
            HeaderName::from_static("x-zoen-device"),
            HeaderName::from_static("x-zoen-ts"),
            HeaderName::from_static("x-zoen-sig"),
        ])
        .max_age(std::time::Duration::from_secs(86_400))
}

pub async fn get(State(st): State<Shared>, Path(sha): Path<String>) -> Response {
    let key = match policy::get_key(&sha) {
        Ok(key) => ObjPath::from(key),
        Err(error) => return policy_error(error),
    };
    match st.blobs.get(&key).await {
        Ok(r) => match r.bytes().await {
            Ok(b) => (
                [
                    (header::CONTENT_TYPE, "application/octet-stream"),
                    (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
                    (
                        header::HeaderName::from_static("cross-origin-resource-policy"),
                        "cross-origin",
                    ),
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
