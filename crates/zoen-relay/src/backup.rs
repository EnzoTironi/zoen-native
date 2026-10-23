//! Encrypted server backups (ADR 0046). The relay keeps, per person, one sealed object and a
//! vault that guards the key to it. It never sees the payload, the backup key or the
//! password:
//!
//! - **Password mode**: an OPRF (2HashDH over ristretto255) with a per-person key `k` that
//!   only the [`Vault`] touches; every restore guess is one evaluation, and after
//!   [`MAX_GUESSES`] without a success `k` is destroyed and the backup is gone for good.
//! - **Recovery-key mode**: the device derives everything from 64 digits; the relay keeps a
//!   verifier and the wrapped key.
//!
//!   POST   /v1/backup/oprf            (device-signed) {blinded} → {evaluated}
//!   PUT    /v1/backup/vault           (device-signed) {mode, verifier, wrapped_key, kdf}
//!   PUT    /v1/backup/blob            (device-signed) sealed payload
//!   DELETE /v1/backup                 (device-signed)
//!   POST   /v1/backup/restore/start   {handle, blinded?} → {identity, mode, kdf, evaluated?}
//!   POST   /v1/backup/restore/open    {handle, auth_key} → {wrapped_key, size}
//!   POST   /v1/backup/restore/enroll  {handle, auth_key, generation, device, cert, sig}
//!   GET    /v1/backup/restore/blob?handle=…   x-zoen-backup-auth: auth_key

// Helpers return the refusal itself, ready to send; it's built once per request, never hot.
#![allow(clippy::result_large_err)]

use std::{sync::Arc, time::Duration};

use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use curve25519_dalek::{ristretto::CompressedRistretto, Scalar};
use event_log::verify_sig;
use object_store::{path::Path as ObjPath, ObjectStoreExt, PutPayload};
use protocol::{backup_message, BACKUP_UPLOAD_MAGIC};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Postgres, Row, Transaction};

use crate::Shared;

/// Restore guesses without a success before the vault destroys the OPRF key.
pub const MAX_GUESSES: i32 = 10;
/// Largest sealed backup accepted (text history; media stays on the blob path).
pub const MAX_BACKUP_BYTES: usize = 64 * 1024 * 1024;
const MAX_SKEW_MS: i64 = 5 * 60 * 1000;

#[derive(Clone, Debug)]
pub struct Settings {
    /// Development only. Public password recovery currently lets any caller exhaust
    /// another person's destructive guess counter. Production needs independent proof.
    pub dev_allow_unauthenticated_password_backup: bool,
    pub storage_timeout: Duration,
    pub request_timeout: Duration,
    pub lock_timeout: Duration,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dev_allow_unauthenticated_password_backup: false,
            storage_timeout: Duration::from_secs(15),
            request_timeout: Duration::from_secs(30),
            lock_timeout: Duration::from_secs(5),
        }
    }
}

impl Settings {
    pub fn from_env() -> anyhow::Result<Self> {
        let dev_allow_unauthenticated_password_backup =
            match std::env::var("ZOEN_DEV_ALLOW_UNAUTHENTICATED_PASSWORD_BACKUP").as_deref() {
                Err(std::env::VarError::NotPresent) | Ok("0") => false,
                Ok("1") => true,
                _ => anyhow::bail!("ZOEN_DEV_ALLOW_UNAUTHENTICATED_PASSWORD_BACKUP must be 0 or 1"),
            };
        let defaults = Self::default();
        let storage_timeout =
            timeout_from_env("ZOEN_BACKUP_STORAGE_TIMEOUT_MS", defaults.storage_timeout)?;
        let request_timeout =
            timeout_from_env("ZOEN_BACKUP_REQUEST_TIMEOUT_MS", defaults.request_timeout)?;
        let lock_timeout = timeout_from_env(
            "ZOEN_BACKUP_LOCK_TIMEOUT_MS",
            defaults.lock_timeout.min(request_timeout),
        )?;
        anyhow::ensure!(
            storage_timeout < request_timeout && lock_timeout <= request_timeout,
            "backup storage timeout must be below the request timeout; lock timeout must not exceed it"
        );
        Ok(Self {
            dev_allow_unauthenticated_password_backup,
            storage_timeout,
            request_timeout,
            lock_timeout,
        })
    }
}

fn timeout_from_env(name: &str, default: Duration) -> anyhow::Result<Duration> {
    let value = match std::env::var(name) {
        Err(std::env::VarError::NotPresent) => return Ok(default),
        value => value?,
    };
    let milliseconds: u64 = value
        .parse()
        .map_err(|_| anyhow::anyhow!("{name} must be milliseconds"))?;
    anyhow::ensure!(
        (1..=120_000).contains(&milliseconds),
        "{name} must be between 1 and 120000 milliseconds"
    );
    Ok(Duration::from_millis(milliseconds))
}

pub(crate) async fn deadline(
    State(st): State<Shared>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    match tokio::time::timeout(st.backup_settings.request_timeout, next.run(request)).await {
        Ok(response) => response,
        Err(_) => err(
            StatusCode::SERVICE_UNAVAILABLE,
            "backup operation timed out",
        ),
    }
}

mod restore;
mod upload;
mod vault_key;
pub use restore::{
    restore_blob, restore_enroll, restore_open, restore_start, BlobQuery, EnrollReq, OpenReq,
    OpenResp, StartReq, StartResp,
};
pub use upload::{delete, oprf, put_blob, put_vault, OprfReq, OprfResp, VaultReq};
pub use vault_key::{EnvVault, Vault};

fn err(code: StatusCode, msg: &'static str) -> Response {
    (code, msg).into_response()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn object_key(identity: &str, key: Option<&str>) -> ObjPath {
    match key {
        Some(key) => ObjPath::from(key),
        None => ObjPath::from(format!("backups/{identity}")),
    }
}

fn unavailable() -> Response {
    err(StatusCode::SERVICE_UNAVAILABLE, "database unavailable")
}

fn hex32(s: &str) -> Option<[u8; 32]> {
    hex::decode(s).ok()?.try_into().ok()
}

async fn begin(st: &Shared) -> Result<Transaction<'static, Postgres>, Response> {
    let mut tx = st.pool.begin().await.map_err(|_| unavailable())?;
    let request = format!("{}ms", st.backup_settings.request_timeout.as_millis());
    let lock = format!("{}ms", st.backup_settings.lock_timeout.as_millis());
    // These are server-side backstops if a cancelled request cannot send its rollback.
    // In particular, idle timeout releases device/restore locks during stalled storage.
    sqlx::query(
        "SELECT set_config('lock_timeout', $1, true),
                set_config('statement_timeout', $2, true),
                set_config('idle_in_transaction_session_timeout', $2, true)",
    )
    .bind(lock)
    .bind(request)
    .execute(&mut *tx)
    .await
    .map_err(|_| unavailable())?;
    Ok(tx)
}

/// The identity behind a signed backup write, or the response that refuses it.
async fn signed_identity(
    st: &Shared,
    headers: &HeaderMap,
    op: &str,
    body: &[u8],
) -> Result<(String, Transaction<'static, Postgres>), Response> {
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
    let mut tx = begin(st).await?;
    // Unlink waits for this authorization lock. A write cannot commit after revocation.
    let identity: Option<String> = match sqlx::query_scalar(
        "SELECT identity FROM devices WHERE device = $1 AND revoked_at IS NULL FOR SHARE",
    )
    .bind(&device)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(i) => i,
        Err(_) => return Err(unavailable()),
    };
    let Some(identity) = identity else {
        return Err(err(StatusCode::FORBIDDEN, "unknown device"));
    };
    let sha = hex::encode(Sha256::digest(body));
    if !verify_sig(
        &device,
        &backup_message(&st.relay_name, &identity, op, &sha, ts_ms),
        &sig,
    ) {
        return Err(err(StatusCode::UNAUTHORIZED, "bad signature"));
    }
    if let Err(wait) = st.limits.backup_device.check(&device) {
        return Err(slow(wait));
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&identity)
        .execute(&mut *tx)
        .await
        .map_err(|_| unavailable())?;
    Ok((identity, tx))
}

fn slow(wait: std::time::Duration) -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [("retry-after", wait.as_secs().max(1).to_string())],
        crate::limits::slow_down(wait),
    )
        .into_response()
}

fn vault(st: &Shared) -> Result<&Arc<dyn Vault>, Response> {
    if !st.backup_settings.dev_allow_unauthenticated_password_backup {
        return Err(password_disabled());
    }
    st.backup_vault.as_ref().ok_or_else(|| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "password backups are not configured on this relay",
        )
    })
}

fn password_disabled() -> Response {
    err(
        StatusCode::SERVICE_UNAVAILABLE,
        "password backups are not configured for safe recovery on this relay",
    )
}
