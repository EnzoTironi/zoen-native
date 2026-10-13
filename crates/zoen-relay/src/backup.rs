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
//!   PUT    /v1/backup/blob            (device-signed) a JSON plan, then ciphertext chunks
//!   DELETE /v1/backup                 (device-signed)
//!
//! The plan answer names the account ceiling before any chunk is stored. Each chunk is
//! 8 MiB, except the last. A single body that begins with `ZOENBG1` is refused and is
//! not stored.
//!   POST   /v1/backup/restore/start   {handle, blinded?} → {identity, mode, kdf, evaluated?}
//!   POST   /v1/backup/restore/open    {handle, auth_key} → {wrapped_key, size}
//!   POST   /v1/backup/restore/enroll  {handle, auth_key, generation, device, cert, sig}
//!   GET    /v1/backup/restore/blob?handle=…   x-zoen-backup-auth: auth_key

// Helpers return the refusal itself, ready to send; it's built once per request, never hot.
#![allow(clippy::result_large_err)]

use std::{collections::BTreeSet, sync::Arc, time::Duration};

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
use object_store::{path::Path as ObjPath, ObjectStoreExt, PutPayload};
use roda_log::verify_sig;
use roda_proto::backup_message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Postgres, Row, Transaction};

use crate::Shared;

/// Restore guesses without a success before the vault destroys the OPRF key.
pub const MAX_GUESSES: i32 = 10;
/// Ciphertext bytes in one upload request. A dropped connection loses at most this piece.
pub const CHUNK_BYTES: usize = 8 * 1024 * 1024;
/// HTTP body budget for one chunk, including the 56-byte frame.
pub const CHUNK_BODY_LIMIT: usize = CHUNK_BYTES + 1024;
/// Hard per-account ceiling. `ZOEN_BACKUP_CEILING_BYTES` may name a lower one.
pub const ACCOUNT_CEILING_BYTES: u64 = 1024 * 1024 * 1024;
/// `ZOENBC1` + generation + index + count + total.
pub const CHUNK_MAGIC: &[u8; 8] = b"ZOENBC1\0";
const CHUNK_FRAME: usize = 8 + 32 + 4 + 4 + 8;
const MAX_CHUNKS: u32 = (ACCOUNT_CEILING_BYTES / CHUNK_BYTES as u64) as u32;
const SQLITE_HEADER: &[u8] = b"SQLite format 3\0";
const MAX_SKEW_MS: i64 = 5 * 60 * 1000;

#[derive(Clone, Debug)]
pub struct Settings {
    /// Development only. Public password recovery currently lets any caller exhaust
    /// another person's destructive guess counter. Production needs independent proof.
    pub dev_allow_unauthenticated_password_backup: bool,
    pub storage_timeout: Duration,
    pub request_timeout: Duration,
    pub lock_timeout: Duration,
    /// Sealed bytes this relay will keep for one account. Never above [`ACCOUNT_CEILING_BYTES`].
    pub account_ceiling: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dev_allow_unauthenticated_password_backup: false,
            storage_timeout: Duration::from_secs(15),
            request_timeout: Duration::from_secs(30),
            lock_timeout: Duration::from_secs(5),
            account_ceiling: ACCOUNT_CEILING_BYTES,
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
        let account_ceiling = match std::env::var("ZOEN_BACKUP_CEILING_BYTES") {
            Err(std::env::VarError::NotPresent) => ACCOUNT_CEILING_BYTES,
            Err(std::env::VarError::NotUnicode(_)) => {
                anyhow::bail!("ZOEN_BACKUP_CEILING_BYTES must be a byte count")
            }
            Ok(value) => {
                let ceiling: u64 = value.parse().map_err(|_| {
                    anyhow::anyhow!("ZOEN_BACKUP_CEILING_BYTES must be a byte count")
                })?;
                anyhow::ensure!(
                    (1..=ACCOUNT_CEILING_BYTES).contains(&ceiling),
                    "ZOEN_BACKUP_CEILING_BYTES must be from 1 to {ACCOUNT_CEILING_BYTES} bytes"
                );
                ceiling
            }
        };
        Ok(Self {
            dev_allow_unauthenticated_password_backup,
            storage_timeout,
            request_timeout,
            lock_timeout,
            account_ceiling,
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

// ───────────────────────────── the HSM boundary ─────────────────────────────

/// The only code that touches a person's OPRF key. Staging seals keys under a master key
/// from the environment ([`EnvVault`]); production swaps in an HSM or enclave behind the
/// same calls, and `k` never exists outside it.
pub trait Vault: Send + Sync {
    /// A fresh OPRF key for `identity`, sealed.
    fn new_key(&self, identity: &str) -> anyhow::Result<Vec<u8>>;
    /// `k·blinded` for the sealed key; `None` if the key or the point is invalid.
    fn evaluate(&self, sealed: &[u8], identity: &str, blinded: &[u8; 32]) -> Option<[u8; 32]>;
    /// Evaluation under a key that exists for no one (no backup, or unknown handle): the
    /// same for the same handle, so a restore can't tell "no backup" from "wrong password".
    fn evaluate_decoy(&self, handle: &str, blinded: &[u8; 32]) -> Option<[u8; 32]>;
    /// Stable decoy fields from a separate secret PRF, independent of public evaluations.
    fn decoy_metadata(&self, handle: &str) -> ([u8; 32], [u8; 32]);
}

pub struct EnvVault {
    master: [u8; 32],
}

impl EnvVault {
    /// `ZOEN_BACKUP_VAULT_KEY` (64 hex chars). `None` disables password backups.
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let Ok(hex_key) = std::env::var("ZOEN_BACKUP_VAULT_KEY") else {
            return Ok(None);
        };
        let bytes = hex::decode(hex_key.trim())
            .map_err(|_| anyhow::anyhow!("ZOEN_BACKUP_VAULT_KEY must be 64 hex characters"))?;
        let master: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("ZOEN_BACKUP_VAULT_KEY must be 32 bytes"))?;
        Ok(Some(Self { master }))
    }

    fn sub_key(&self, info: &[u8]) -> [u8; 32] {
        let hk = hkdf::Hkdf::<Sha256>::new(Some(b"zoen-backup-vault-v1"), &self.master);
        let mut out = [0u8; 32];
        hk.expand(info, &mut out).expect("32 bytes");
        out
    }

    fn aad(identity: &str) -> Vec<u8> {
        [b"zoen-backup-oprf-key-v1\0".as_slice(), identity.as_bytes()].concat()
    }
}

fn point_mul(k: &Scalar, blinded: &[u8; 32]) -> Option<[u8; 32]> {
    let p = CompressedRistretto(*blinded).decompress()?;
    // The identity point would make the output independent of k.
    if p == curve25519_dalek::RistrettoPoint::default() {
        return None;
    }
    Some((k * p).compress().to_bytes())
}

impl Vault for EnvVault {
    fn new_key(&self, identity: &str) -> anyhow::Result<Vec<u8>> {
        let mut wide = [0u8; 64];
        getrandom::getrandom(&mut wide)?;
        let k = Scalar::from_bytes_mod_order_wide(&wide);
        let mut nonce = [0u8; 24];
        getrandom::getrandom(&mut nonce)?;
        let aead = XChaCha20Poly1305::new((&self.sub_key(b"seal")).into());
        let ct = aead
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: k.as_bytes(),
                    aad: &Self::aad(identity),
                },
            )
            .map_err(|_| anyhow::anyhow!("seal failed"))?;
        Ok([nonce.as_slice(), &ct].concat())
    }

    fn evaluate(&self, sealed: &[u8], identity: &str, blinded: &[u8; 32]) -> Option<[u8; 32]> {
        if sealed.len() < 24 {
            return None;
        }
        let aead = XChaCha20Poly1305::new((&self.sub_key(b"seal")).into());
        let nonce: [u8; 24] = sealed[..24].try_into().ok()?;
        let k = aead
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &sealed[24..],
                    aad: &Self::aad(identity),
                },
            )
            .ok()?;
        let k: [u8; 32] = k.try_into().ok()?;
        let k = Option::<Scalar>::from(Scalar::from_canonical_bytes(k))?;
        point_mul(&k, blinded)
    }

    fn evaluate_decoy(&self, handle: &str, blinded: &[u8; 32]) -> Option<[u8; 32]> {
        let mut wide = [0u8; 64];
        let hk = hkdf::Hkdf::<Sha256>::new(Some(b"zoen-backup-decoy-v1"), &self.master);
        hk.expand(handle.as_bytes(), &mut wide).expect("64 bytes");
        point_mul(&Scalar::from_bytes_mod_order_wide(&wide), blinded)
    }

    fn decoy_metadata(&self, handle: &str) -> ([u8; 32], [u8; 32]) {
        let hk = hkdf::Hkdf::<Sha256>::new(Some(b"zoen-backup-decoy-metadata-v1"), &self.master);
        let mut out = [0u8; 64];
        hk.expand(handle.as_bytes(), &mut out).expect("64 bytes");
        (
            out[..32].try_into().expect("32 bytes"),
            out[32..].try_into().expect("32 bytes"),
        )
    }
}

// ───────────────────────────── helpers ─────────────────────────────

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

fn ceiling_of(st: &Shared) -> u64 {
    st.backup_settings
        .account_ceiling
        .min(ACCOUNT_CEILING_BYTES)
}

fn shows_sqlite(bytes: &[u8]) -> bool {
    bytes
        .windows(SQLITE_HEADER.len())
        .any(|window| window == SQLITE_HEADER)
}

fn piece_count(total: u64) -> Option<u32> {
    if total == 0 {
        return None;
    }
    let count = total.div_ceil(CHUNK_BYTES as u64);
    u32::try_from(count)
        .ok()
        .filter(|count| (1..=MAX_CHUNKS).contains(count))
}

fn piece_len(total: u64, index: u32) -> Option<usize> {
    let count = piece_count(total)?;
    if index >= count {
        return None;
    }
    let start = index as u64 * CHUNK_BYTES as u64;
    Some((total - start).min(CHUNK_BYTES as u64) as usize)
}

fn hex_token(value: &str) -> Option<String> {
    let bytes = hex::decode(value).ok()?;
    if bytes.len() != 32 {
        return None;
    }
    Some(hex::encode(bytes))
}

fn too_big(ceiling: u64) -> Response {
    (StatusCode::PAYLOAD_TOO_LARGE, Json(CeilingBody { ceiling })).into_response()
}

#[derive(Serialize)]
struct CeilingBody {
    ceiling: u64,
}

#[derive(Deserialize)]
struct PlanReq {
    generation: String,
    total_bytes: u64,
    chunks: u32,
    sha: String,
}

#[derive(Serialize)]
struct PlanResp {
    ceiling: u64,
    confirmed: Vec<u32>,
    missing: Vec<u32>,
}

struct ChunkFrame<'a> {
    generation: String,
    index: u32,
    count: u32,
    total: u64,
    payload: &'a [u8],
}

fn parse_chunk(body: &[u8]) -> Option<ChunkFrame<'_>> {
    if body.len() < CHUNK_FRAME || body.get(..8) != Some(CHUNK_MAGIC.as_slice()) {
        return None;
    }
    let generation = hex::encode(&body[8..40]);
    let index = u32::from_be_bytes(body[40..44].try_into().ok()?);
    let count = u32::from_be_bytes(body[44..48].try_into().ok()?);
    let total = u64::from_be_bytes(body[48..56].try_into().ok()?);
    Some(ChunkFrame {
        generation,
        index,
        count,
        total,
        payload: &body[56..],
    })
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

// ───────────────────────────── device side ─────────────────────────────

#[derive(Deserialize)]
pub struct OprfReq {
    blinded: String,
    generation: String,
}

#[derive(Serialize)]
pub struct OprfResp {
    evaluated: String,
}

/// Starts a password setup: a new pending OPRF key, evaluated on the device's blinded input.
pub async fn oprf(State(st): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let vault = match vault(&st) {
        Ok(v) => v.clone(),
        Err(r) => return r,
    };
    let (identity, mut tx) = match signed_identity(&st, &headers, "oprf", &body).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    let Ok(req) = serde_json::from_slice::<OprfReq>(&body) else {
        return err(StatusCode::BAD_REQUEST, "bad request");
    };
    let Some(blinded) = hex32(&req.blinded) else {
        return err(StatusCode::BAD_REQUEST, "bad point");
    };
    if hex32(&req.generation).is_none() {
        return err(StatusCode::BAD_REQUEST, "bad generation");
    }
    let Ok(sealed) = vault.new_key(&identity) else {
        return err(StatusCode::SERVICE_UNAVAILABLE, "vault unavailable");
    };
    let Some(evaluated) = vault.evaluate(&sealed, &identity, &blinded) else {
        return err(StatusCode::BAD_REQUEST, "bad point");
    };
    let r = sqlx::query(
        "INSERT INTO backup_pending (identity, pending_key, generation, device) VALUES ($1, $2, $3, $4)
         ON CONFLICT (identity) DO UPDATE SET pending_key = $2, generation = $3,
             device = $4, created_at = now()",
    )
    .bind(&identity)
    .bind(&sealed)
    .bind(&req.generation)
    .bind(headers.get("x-zoen-device").and_then(|v| v.to_str().ok()).unwrap_or(""))
    .execute(&mut *tx)
    .await;
    if r.is_err() || tx.commit().await.is_err() {
        return unavailable();
    }
    Json(OprfResp {
        evaluated: hex::encode(evaluated),
    })
    .into_response()
}

#[derive(Deserialize)]
pub struct VaultReq {
    generation: String,
    mode: String,
    verifier: String,
    wrapped_key: String,
    kdf: serde_json::Value,
}

/// Stages a configuration. The previous backup remains restorable until its upload commits.
pub async fn put_vault(State(st): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let (identity, mut tx) = match signed_identity(&st, &headers, "vault", &body).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    let Ok(req) = serde_json::from_slice::<VaultReq>(&body) else {
        return err(StatusCode::BAD_REQUEST, "bad request");
    };
    let (Some(verifier), Ok(wrapped)) = (hex32(&req.verifier), hex::decode(&req.wrapped_key))
    else {
        return err(StatusCode::BAD_REQUEST, "bad request");
    };
    if wrapped.len() > 256 || hex32(&req.generation).is_none() {
        return err(StatusCode::BAD_REQUEST, "bad request");
    }
    let oprf_key: Option<Vec<u8>> = match req.mode.as_str() {
        "recovery_key" => None,
        "passphrase" => {
            if let Err(r) = vault(&st) {
                return r;
            }
            let pending: Option<Vec<u8>> = match sqlx::query_scalar(
                "DELETE FROM backup_pending WHERE identity = $1 AND generation = $2
                    AND device = $3 RETURNING pending_key",
            )
            .bind(&identity)
            .bind(&req.generation)
            .bind(
                headers
                    .get("x-zoen-device")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or(""),
            )
            .fetch_optional(&mut *tx)
            .await
            {
                Ok(p) => p,
                Err(_) => return unavailable(),
            };
            match pending {
                Some(p) => Some(p),
                None => return err(StatusCode::CONFLICT, "start a password setup first"),
            }
        }
        _ => return err(StatusCode::BAD_REQUEST, "bad mode"),
    };
    let r = sqlx::query(
        "INSERT INTO backup_setups (identity, mode, oprf_key, verifier, wrapped_key, kdf, generation)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (identity) DO UPDATE SET mode = $2, oprf_key = $3, verifier = $4,
             wrapped_key = $5, kdf = $6, generation = $7, created_at = now()",
    )
    .bind(&identity)
    .bind(&req.mode)
    .bind(&oprf_key)
    .bind(verifier.as_slice())
    .bind(&wrapped)
    .bind(&req.kdf)
    .bind(&req.generation)
    .execute(&mut *tx)
    .await;
    if r.is_err() || tx.commit().await.is_err() {
        return unavailable();
    }
    tracing::info!(identity = %crate::pseudonym::pseudo(&identity), mode = %req.mode, "backup configuration staged");
    StatusCode::NO_CONTENT.into_response()
}

/// Declares the ceiling or accepts one ciphertext chunk. A single `ZOENBG1` body is refused.
pub async fn put_blob(State(st): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let (identity, mut tx) = match signed_identity(&st, &headers, "blob", &body).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    if let Err(response) = passphrase_gate(&st, &mut tx, &identity).await {
        return response;
    }
    if body.first() == Some(&b'{') {
        return declare(&st, identity, tx, &body).await;
    }
    if body.len() >= CHUNK_FRAME && body.get(..8) == Some(CHUNK_MAGIC.as_slice()) {
        return accept_chunk(&st, identity, tx, &body).await;
    }
    if body.starts_with(b"ZOENBG1\0") {
        return err(StatusCode::BAD_REQUEST, "send this backup in chunks");
    }
    err(StatusCode::BAD_REQUEST, "backup generation required")
}

async fn passphrase_gate(
    st: &Shared,
    tx: &mut Transaction<'static, Postgres>,
    identity: &str,
) -> Result<(), Response> {
    let passphrase: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM backup_vaults WHERE identity = $1 AND mode = 'passphrase'
            UNION ALL
            SELECT 1 FROM backup_setups WHERE identity = $1 AND mode = 'passphrase'
         )",
    )
    .bind(identity)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| unavailable())?;
    if passphrase {
        vault(st)?;
    }
    Ok(())
}

async fn generation_open(
    tx: &mut Transaction<'static, Postgres>,
    identity: &str,
    generation: &str,
) -> Result<bool, Response> {
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM backup_vaults
         WHERE identity = $1 AND generation = $2 AND NOT locked)",
    )
    .bind(identity)
    .bind(generation)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| unavailable())?;
    if active {
        return Ok(true);
    }
    let setup: Option<i32> =
        sqlx::query_scalar("SELECT 1 FROM backup_setups WHERE identity = $1 AND generation = $2")
            .bind(identity)
            .bind(generation)
            .fetch_optional(&mut **tx)
            .await
            .map_err(|_| unavailable())?;
    Ok(setup.is_some())
}

async fn take_partials(
    tx: &mut Transaction<'static, Postgres>,
    identity: &str,
    generation: Option<&str>,
) -> Result<Vec<String>, Response> {
    let keys: Vec<String> = match generation {
        Some(generation) => sqlx::query_scalar(
            "DELETE FROM backup_chunks WHERE identity = $1 AND generation = $2 RETURNING object_key",
        )
        .bind(identity)
        .bind(generation)
        .fetch_all(&mut **tx)
        .await
        .map_err(|_| unavailable())?,
        None => sqlx::query_scalar(
            "DELETE FROM backup_chunks WHERE identity = $1 RETURNING object_key",
        )
        .bind(identity)
        .fetch_all(&mut **tx)
        .await
        .map_err(|_| unavailable())?,
    };
    let removed = match generation {
        Some(generation) => {
            sqlx::query("DELETE FROM backup_uploads WHERE identity = $1 AND generation = $2")
                .bind(identity)
                .bind(generation)
                .execute(&mut **tx)
                .await
        }
        None => {
            sqlx::query("DELETE FROM backup_uploads WHERE identity = $1")
                .bind(identity)
                .execute(&mut **tx)
                .await
        }
    };
    if removed.is_err() {
        return Err(unavailable());
    }
    Ok(keys)
}

async fn cleanup_keys(st: &Shared, keys: &[String]) {
    for key in keys {
        cleanup(st, &ObjPath::from(key.as_str())).await;
    }
}

fn plan_lists(have: &BTreeSet<i32>, chunks: u32) -> (Vec<u32>, Vec<u32>) {
    let mut confirmed = Vec::new();
    let mut missing = Vec::new();
    for index in 0..chunks {
        if have.contains(&(index as i32)) {
            confirmed.push(index);
        } else {
            missing.push(index);
        }
    }
    (confirmed, missing)
}

async fn declare(
    st: &Shared,
    identity: String,
    mut tx: Transaction<'static, Postgres>,
    body: &[u8],
) -> Response {
    let ceiling = ceiling_of(st);
    let Ok(req) = serde_json::from_slice::<PlanReq>(body) else {
        return err(StatusCode::BAD_REQUEST, "bad request");
    };
    if req.total_bytes > ceiling {
        return too_big(ceiling);
    }
    let (Some(generation), Some(sha)) = (hex_token(&req.generation), hex_token(&req.sha)) else {
        return err(StatusCode::BAD_REQUEST, "bad request");
    };
    let Some(chunks) = piece_count(req.total_bytes) else {
        return err(StatusCode::BAD_REQUEST, "bad request");
    };
    if req.chunks != chunks {
        return err(StatusCode::BAD_REQUEST, "bad request");
    }
    let open = match generation_open(&mut tx, &identity, &generation).await {
        Ok(open) => open,
        Err(r) => return r,
    };
    if !open {
        return err(
            StatusCode::CONFLICT,
            "backup configuration changed; set it up again on this device",
        );
    }
    let published: Option<(Option<String>, Option<i64>)> = match sqlx::query_as(
        "SELECT blob_sha, blob_bytes FROM backup_vaults
         WHERE identity = $1 AND generation = $2 AND NOT locked",
    )
    .bind(&identity)
    .bind(&generation)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(row) => row,
        Err(_) => return unavailable(),
    };
    let already = published.is_some_and(|(blob_sha, blob_bytes)| {
        blob_sha.as_deref() == Some(sha.as_str()) && blob_bytes == Some(req.total_bytes as i64)
    });
    if already {
        let orphans = match take_partials(&mut tx, &identity, Some(&generation)).await {
            Ok(keys) => keys,
            Err(r) => return r,
        };
        if tx.commit().await.is_err() {
            return unavailable();
        }
        cleanup_keys(st, &orphans).await;
        let (confirmed, missing) = plan_lists(&(0..chunks as i32).collect(), chunks);
        return Json(PlanResp {
            ceiling,
            confirmed,
            missing,
        })
        .into_response();
    }
    let existing: Option<(String, i64, i32)> = match sqlx::query_as(
        "SELECT sha, total_bytes, chunk_count FROM backup_uploads WHERE identity = $1 AND generation = $2",
    )
    .bind(&identity)
    .bind(&generation)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(row) => row,
        Err(_) => return unavailable(),
    };
    let mut orphans = Vec::new();
    let reset = existing.as_ref().is_some_and(|(have_sha, total, count)| {
        have_sha != &sha || *total != req.total_bytes as i64 || *count != chunks as i32
    });
    if existing.is_some() && reset {
        orphans = match take_partials(&mut tx, &identity, Some(&generation)).await {
            Ok(keys) => keys,
            Err(r) => return r,
        };
    }
    if (existing.is_none() || reset)
        && sqlx::query(
            "INSERT INTO backup_uploads (identity, generation, total_bytes, chunk_count, sha)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&identity)
        .bind(&generation)
        .bind(req.total_bytes as i64)
        .bind(chunks as i32)
        .bind(&sha)
        .execute(&mut *tx)
        .await
        .is_err()
    {
        return unavailable();
    }
    let indexes: Vec<i32> = match sqlx::query_scalar(
        "SELECT idx FROM backup_chunks WHERE identity = $1 AND generation = $2 ORDER BY idx",
    )
    .bind(&identity)
    .bind(&generation)
    .fetch_all(&mut *tx)
    .await
    {
        Ok(rows) => rows,
        Err(_) => return unavailable(),
    };
    if tx.commit().await.is_err() {
        return unavailable();
    }
    cleanup_keys(st, &orphans).await;
    let (confirmed, missing) = plan_lists(&indexes.into_iter().collect(), chunks);
    Json(PlanResp {
        ceiling,
        confirmed,
        missing,
    })
    .into_response()
}

async fn read_object(st: &Shared, key: &str) -> Result<Bytes, Response> {
    match tokio::time::timeout(st.backup_settings.storage_timeout, async {
        st.blobs.get(&ObjPath::from(key)).await?.bytes().await
    })
    .await
    {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(_)) | Err(_) => Err(err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable")),
    }
}

async fn accept_chunk(
    st: &Shared,
    identity: String,
    mut tx: Transaction<'static, Postgres>,
    body: &[u8],
) -> Response {
    let Some(frame) = parse_chunk(body) else {
        return err(StatusCode::BAD_REQUEST, "bad request");
    };
    if shows_sqlite(frame.payload) {
        return err(StatusCode::BAD_REQUEST, "backup payload is not sealed");
    }
    let upload: Option<(i64, i32, String)> = match sqlx::query_as(
        "SELECT total_bytes, chunk_count, sha FROM backup_uploads
         WHERE identity = $1 AND generation = $2",
    )
    .bind(&identity)
    .bind(&frame.generation)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(row) => row,
        Err(_) => return unavailable(),
    };
    let Some((total, chunks, sha)) = upload else {
        return err(StatusCode::CONFLICT, "declare the backup size first");
    };
    if total != frame.total as i64
        || chunks != frame.count as i32
        || piece_count(frame.total) != Some(frame.count)
        || piece_len(frame.total, frame.index) != Some(frame.payload.len())
    {
        return err(StatusCode::BAD_REQUEST, "bad request");
    }
    let have: Option<(String, String)> = match sqlx::query_as(
        "SELECT sha, object_key FROM backup_chunks
         WHERE identity = $1 AND generation = $2 AND idx = $3",
    )
    .bind(&identity)
    .bind(&frame.generation)
    .bind(frame.index as i32)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(row) => row,
        Err(_) => return unavailable(),
    };
    let payload_sha = hex::encode(Sha256::digest(frame.payload));
    if let Some((have_sha, _)) = &have {
        if have_sha != &payload_sha {
            return err(StatusCode::CONFLICT, "confirmed piece differs");
        }
    } else {
        let mut object_id = [0u8; 16];
        if getrandom::getrandom(&mut object_id).is_err() {
            return unavailable();
        }
        let key = format!(
            "backups/partial/{identity}/{}/{}",
            frame.generation,
            hex::encode(object_id)
        );
        let path = ObjPath::from(key.as_str());
        match tokio::time::timeout(
            st.backup_settings.storage_timeout,
            st.blobs.put(
                &path,
                PutPayload::from_bytes(Bytes::copy_from_slice(frame.payload)),
            ),
        )
        .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "backup chunk put failed");
                return err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable");
            }
            Err(_) => return err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable"),
        }
        if sqlx::query(
            "INSERT INTO backup_chunks (identity, generation, idx, sha, nbytes, object_key)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&identity)
        .bind(&frame.generation)
        .bind(frame.index as i32)
        .bind(&payload_sha)
        .bind(frame.payload.len() as i32)
        .bind(&key)
        .execute(&mut *tx)
        .await
        .is_err()
        {
            return unavailable();
        }
    }
    let rows: Vec<(i32, String, i32)> = match sqlx::query_as(
        "SELECT idx, object_key, nbytes FROM backup_chunks
         WHERE identity = $1 AND generation = $2 ORDER BY idx",
    )
    .bind(&identity)
    .bind(&frame.generation)
    .fetch_all(&mut *tx)
    .await
    {
        Ok(rows) => rows,
        Err(_) => return unavailable(),
    };
    if rows.len() != chunks as usize {
        if tx.commit().await.is_err() {
            return unavailable();
        }
        let status = if have.is_some() {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        };
        return status.into_response();
    }
    let mut sealed = Vec::with_capacity(total as usize);
    let mut partials = Vec::with_capacity(rows.len());
    for (index, key, nbytes) in &rows {
        if *index as usize != partials.len() {
            return err(StatusCode::BAD_REQUEST, "bad request");
        }
        let bytes = match read_object(st, key).await {
            Ok(bytes) => bytes,
            Err(r) => return r,
        };
        if bytes.len() != *nbytes as usize {
            return err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable");
        }
        sealed.extend_from_slice(&bytes);
        partials.push(key.clone());
    }
    if sealed.len() as i64 != total || hex::encode(Sha256::digest(&sealed)) != sha {
        return err(StatusCode::BAD_REQUEST, "bad request");
    }
    if shows_sqlite(&sealed) {
        let orphans = match take_partials(&mut tx, &identity, Some(&frame.generation)).await {
            Ok(keys) => keys,
            Err(r) => return r,
        };
        if tx.commit().await.is_err() {
            return unavailable();
        }
        cleanup_keys(st, &orphans).await;
        return err(StatusCode::BAD_REQUEST, "backup payload is not sealed");
    }
    publish_ciphertext(
        st,
        identity,
        tx,
        frame.generation,
        Bytes::from(sealed),
        partials,
    )
    .await
}

async fn publish_ciphertext(
    st: &Shared,
    identity: String,
    mut tx: Transaction<'static, Postgres>,
    generation: String,
    body: Bytes,
    mut partials: Vec<String>,
) -> Response {
    if body.is_empty() || body.len() as u64 > ceiling_of(st) {
        return too_big(ceiling_of(st));
    }
    if shows_sqlite(&body) {
        return err(StatusCode::BAD_REQUEST, "backup payload is not sealed");
    }
    let active = match sqlx::query(
        "SELECT generation, mode, locked, blob_key FROM backup_vaults WHERE identity = $1 FOR UPDATE",
    )
    .bind(&identity)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(row) => row,
        Err(_) => return unavailable(),
    };
    let setup = match sqlx::query(
        "SELECT mode, oprf_key, verifier, wrapped_key, kdf FROM backup_setups
         WHERE identity = $1 AND generation = $2",
    )
    .bind(&identity)
    .bind(&generation)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(row) => row,
        Err(_) => return unavailable(),
    };
    let active_matches = active.as_ref().is_some_and(|row| {
        row.get::<Option<String>, _>("generation").as_deref() == Some(&generation)
            && !row.get::<bool, _>("locked")
    });
    if setup.is_none() && !active_matches {
        return err(
            StatusCode::CONFLICT,
            "backup configuration changed; set it up again on this device",
        );
    }
    let mode = setup
        .as_ref()
        .or(active.as_ref())
        .expect("matching setup or active vault")
        .get::<String, _>("mode");
    if mode == "passphrase" {
        if let Err(r) = vault(st) {
            return r;
        }
    }
    let sha = hex::encode(Sha256::digest(&body));
    let n = body.len();
    let mut object_id = [0u8; 32];
    if getrandom::getrandom(&mut object_id).is_err() {
        return unavailable();
    }
    let key = format!("backups/v2/{identity}/{}", hex::encode(object_id));
    let path = object_key(&identity, Some(&key));
    match tokio::time::timeout(
        st.backup_settings.storage_timeout,
        st.blobs.put(&path, PutPayload::from_bytes(body)),
    )
    .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "backup put failed");
            return err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable");
        }
        Err(_) => {
            // The remote PUT may still finish. This unique key is an orphan, never an
            // active pointer: publication only follows an acknowledged durable write.
            tracing::warn!("backup put timed out");
            return err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable");
        }
    }
    let r = if let Some(setup) = setup {
        sqlx::query(
            "INSERT INTO backup_vaults (identity, mode, oprf_key, verifier, wrapped_key,
                kdf, generation, blob_sha, blob_bytes, blob_key) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
             ON CONFLICT (identity) DO UPDATE SET mode = $2, oprf_key = $3, verifier = $4,
                wrapped_key = $5, kdf = $6, generation = $7, blob_sha = $8, blob_bytes = $9,
                blob_key = $10, guesses = 0, armed = false, locked = false, updated_at = now()",
        )
        .bind(&identity)
        .bind(setup.get::<String, _>("mode"))
        .bind(setup.get::<Option<Vec<u8>>, _>("oprf_key"))
        .bind(setup.get::<Vec<u8>, _>("verifier"))
        .bind(setup.get::<Vec<u8>, _>("wrapped_key"))
        .bind(setup.get::<serde_json::Value, _>("kdf"))
        .bind(&generation)
        .bind(&sha)
        .bind(n as i64)
        .bind(&key)
        .execute(&mut *tx)
        .await
    } else {
        sqlx::query(
            "UPDATE backup_vaults SET blob_sha = $2, blob_bytes = $3, blob_key = $4, updated_at = now()
             WHERE identity = $1",
        )
        .bind(&identity)
        .bind(&sha)
        .bind(n as i64)
        .bind(&key)
        .execute(&mut *tx)
        .await
    };
    if r.is_err() {
        return unavailable();
    }
    match take_partials(&mut tx, &identity, None).await {
        Ok(keys) => partials.extend(keys),
        Err(response) => return response,
    }
    if sqlx::query("DELETE FROM backup_setups WHERE identity = $1 AND generation = $2")
        .bind(&identity)
        .bind(&generation)
        .execute(&mut *tx)
        .await
        .is_err()
        || tx.commit().await.is_err()
    {
        return unavailable();
    }
    if let Some(old) = active {
        let old_key: Option<String> = old.get("blob_key");
        let old_path = object_key(&identity, old_key.as_deref());
        if old_path != path {
            cleanup(st, &old_path).await;
        }
    }
    cleanup_keys(st, &partials).await;
    tracing::info!(identity = %crate::pseudonym::pseudo(&identity), bytes = n, "backup stored");
    StatusCode::CREATED.into_response()
}

/// Turns the backup off: forgets the vault and the object.
pub async fn delete(State(st): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let (identity, mut tx) = match signed_identity(&st, &headers, "delete", &body).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    let old = match sqlx::query("DELETE FROM backup_vaults WHERE identity = $1 RETURNING blob_key")
        .bind(&identity)
        .fetch_optional(&mut *tx)
        .await
    {
        Ok(row) => row,
        Err(_) => return unavailable(),
    };
    for table in ["backup_pending", "backup_setups"] {
        let sql = format!("DELETE FROM {table} WHERE identity = $1");
        if sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(&identity)
            .execute(&mut *tx)
            .await
            .is_err()
        {
            return unavailable();
        }
    }
    let partials = match take_partials(&mut tx, &identity, None).await {
        Ok(keys) => keys,
        Err(response) => return response,
    };
    if tx.commit().await.is_err() {
        return unavailable();
    }
    cleanup_keys(&st, &partials).await;
    if let Some(row) = old {
        let key: Option<String> = row.get("blob_key");
        cleanup(&st, &object_key(&identity, key.as_deref())).await;
    }
    StatusCode::NO_CONTENT.into_response()
}

async fn cleanup(st: &Shared, path: &ObjPath) {
    match tokio::time::timeout(st.backup_settings.storage_timeout, st.blobs.delete(path)).await {
        Ok(Ok(())) | Ok(Err(object_store::Error::NotFound { .. })) => {}
        Ok(Err(e)) => tracing::warn!(error = %e, "backup cleanup failed; object retained for GC"),
        Err(_) => tracing::warn!("backup cleanup timed out; object retained for GC"),
    }
}

// ───────────────────────────── restore (no device yet) ─────────────────────────────

struct VaultRow {
    identity: String,
    mode: String,
    oprf_key: Option<Vec<u8>>,
    verifier: Vec<u8>,
    wrapped_key: Vec<u8>,
    kdf: serde_json::Value,
    locked: bool,
    blob_bytes: Option<i64>,
    blob_key: Option<String>,
    generation: Option<String>,
}

async fn vault_by_handle(
    st: &Shared,
    handle: &str,
) -> Result<(Option<VaultRow>, Transaction<'static, Postgres>), Response> {
    let mut tx = begin(st).await?;
    let row = sqlx::query(
        "SELECT v.identity, v.mode, v.oprf_key, v.verifier, v.wrapped_key, v.kdf, v.locked,
                v.blob_bytes, v.blob_key, v.generation
         FROM backup_vaults v JOIN identities i ON i.id = v.identity
         WHERE i.handle = $1 FOR UPDATE OF v",
    )
    .bind(handle)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| unavailable())?;
    Ok((
        row.map(|r| VaultRow {
            identity: r.get("identity"),
            mode: r.get("mode"),
            oprf_key: r.get("oprf_key"),
            verifier: r.get("verifier"),
            wrapped_key: r.get("wrapped_key"),
            kdf: r.get("kdf"),
            locked: r.get("locked"),
            blob_bytes: r.get("blob_bytes"),
            blob_key: r.get("blob_key"),
            generation: r.get("generation"),
        }),
        tx,
    ))
}

/// One more guess; at the limit the OPRF key is destroyed and the vault locked for good.
/// `armed`: the guess was an OPRF evaluation, so the open that follows it is free.
async fn spend_guess(
    conn: &mut PgConnection,
    identity: &str,
    armed: bool,
) -> Result<bool, Response> {
    let locked: bool = sqlx::query_scalar(
        "UPDATE backup_vaults SET guesses = guesses + 1, armed = $3,
             locked = locked OR guesses + 1 >= $2,
             oprf_key = CASE WHEN guesses + 1 >= $2 THEN NULL ELSE oprf_key END
         WHERE identity = $1 RETURNING locked",
    )
    .bind(identity)
    .bind(MAX_GUESSES)
    .bind(armed)
    .fetch_one(conn)
    .await
    .map_err(|_| unavailable())?;
    if locked {
        tracing::warn!(identity = %crate::pseudonym::pseudo(identity), "backup vault locked after too many guesses");
    }
    Ok(locked)
}

fn restore_limit(st: &Shared, ip: &str, handle: &str) -> Result<(), Response> {
    st.limits.backup_restore_ip.check(ip).map_err(slow)?;
    st.limits
        .backup_restore_handle
        .check(handle)
        .map_err(slow)?;
    Ok(())
}

fn default_kdf() -> serde_json::Value {
    serde_json::json!({"alg": "argon2id", "m_kib": 65536, "t": 3, "p": 1, "v": 1})
}

#[derive(Deserialize)]
pub struct StartReq {
    handle: String,
    blinded: Option<String>,
}

#[derive(Serialize)]
pub struct StartResp {
    identity: String,
    mode: String,
    kdf: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    evaluated: Option<String>,
    generation: Option<String>,
}

fn locked_response() -> Response {
    err(
        StatusCode::LOCKED,
        "too many wrong attempts; this backup can no longer be opened",
    )
}

pub async fn restore_start(
    State(st): State<Shared>,
    crate::ClientIp(ip): crate::ClientIp,
    Json(req): Json<StartReq>,
) -> Response {
    let Some(handle) = roda_proto::normalize_handle(&req.handle) else {
        return err(StatusCode::BAD_REQUEST, "bad handle");
    };
    if let Err(r) = restore_limit(&st, &ip, &handle) {
        return r;
    }
    let blinded = req.blinded.as_deref().and_then(hex32);
    let (row, mut tx) = match vault_by_handle(&st, &handle).await {
        Ok(r) => r,
        Err(r) => return r,
    };
    let decoy_identity: Option<String> = if row.is_none() {
        match sqlx::query_scalar("SELECT id FROM identities WHERE handle = $1")
            .bind(&handle)
            .fetch_optional(&mut *tx)
            .await
        {
            Ok(identity) => identity,
            Err(_) => return unavailable(),
        }
    } else {
        None
    };
    let decoy = |st: &Shared| -> Response {
        // No backup here: answer exactly like a password vault would.
        let v = match vault(st) {
            Ok(v) => v,
            Err(r) => return r,
        };
        let Some(b) = blinded else {
            return err(StatusCode::BAD_REQUEST, "bad point");
        };
        let Some(e) = v.evaluate_decoy(&handle, &b) else {
            return err(StatusCode::BAD_REQUEST, "bad point");
        };
        let (fake, generation) = v.decoy_metadata(&handle);
        Json(StartResp {
            identity: decoy_identity.clone().unwrap_or_else(|| hex::encode(fake)),
            mode: "passphrase".into(),
            kdf: default_kdf(),
            evaluated: Some(hex::encode(e)),
            generation: Some(hex::encode(generation)),
        })
        .into_response()
    };
    let Some(row) = row else {
        return decoy(&st);
    };
    if row.mode != "recovery_key" {
        if let Err(r) = vault(&st) {
            return r;
        }
    }
    if row.locked && row.mode != "recovery_key" {
        return locked_response();
    }
    match row.mode.as_str() {
        "recovery_key" => Json(StartResp {
            identity: row.identity,
            mode: row.mode,
            kdf: row.kdf,
            evaluated: None,
            generation: row.generation,
        })
        .into_response(),
        _ => {
            let (Ok(vault), Some(sealed), Some(b)) = (vault(&st), row.oprf_key.as_ref(), blinded)
            else {
                return err(StatusCode::BAD_REQUEST, "bad point");
            };
            let locked = match spend_guess(&mut tx, &row.identity, true).await {
                Ok(locked) => locked,
                Err(r) => return r,
            };
            if tx.commit().await.is_err() {
                return unavailable();
            }
            if locked {
                return locked_response();
            }
            let Some(e) = vault.evaluate(sealed, &row.identity, &b) else {
                return err(StatusCode::BAD_REQUEST, "bad point");
            };
            Json(StartResp {
                identity: row.identity,
                mode: row.mode,
                kdf: row.kdf,
                evaluated: Some(hex::encode(e)),
                generation: row.generation,
            })
            .into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct OpenReq {
    handle: String,
    auth_key: String,
    generation: Option<String>,
}

#[derive(Serialize)]
pub struct OpenResp {
    wrapped_key: String,
    size: i64,
    generation: Option<String>,
}

#[derive(Deserialize)]
pub struct EnrollReq {
    handle: String,
    auth_key: String,
    generation: String,
    device: String,
    cert: String,
    sig: String,
}

/// A root certificate is necessary, but recovery authority must authorize its enrollment.
pub async fn restore_enroll(
    State(st): State<Shared>,
    crate::ClientIp(ip): crate::ClientIp,
    Json(req): Json<EnrollReq>,
) -> Response {
    let Some(handle) = roda_proto::normalize_handle(&req.handle) else {
        return err(StatusCode::BAD_REQUEST, "bad handle");
    };
    if let Err(r) = restore_limit(&st, &ip, &handle) {
        return r;
    }
    let (row, mut tx) = match authorize(&st, &handle, &req.auth_key, Some(&req.generation)).await {
        Ok(authorized) => authorized,
        Err(r) => return r,
    };
    if row.blob_bytes.is_none() {
        return err(StatusCode::NOT_FOUND, "no backup yet");
    }
    if !verify_sig(
        &req.device,
        &roda_proto::backup_enroll_message(&row.identity, &req.device, &req.cert, &req.generation),
        &req.sig,
    ) {
        return err(StatusCode::FORBIDDEN, "invalid device enrollment");
    }
    match crate::db::enroll_device(&mut tx, &row.identity, &req.device, &req.cert).await {
        Ok(()) => {}
        Err(crate::db::EnrollmentError::Database(_)) => return unavailable(),
        Err(_) => return err(StatusCode::FORBIDDEN, "device enrollment refused"),
    }
    if tx.commit().await.is_err() {
        return unavailable();
    }
    StatusCode::NO_CONTENT.into_response()
}

/// Checks `auth_key` against the vault; `Ok` resets the guess counter.
async fn authorize(
    st: &Shared,
    handle: &str,
    auth_key: &str,
    generation: Option<&str>,
) -> Result<(VaultRow, Transaction<'static, Postgres>), Response> {
    let wrong = || err(StatusCode::FORBIDDEN, "wrong password or key");
    let (row, mut tx) = vault_by_handle(st, handle).await?;
    let Some(row) = row else {
        return Err(
            if st.backup_settings.dev_allow_unauthenticated_password_backup {
                wrong()
            } else {
                password_disabled()
            },
        );
    };
    if row.mode != "recovery_key" {
        vault(st)?;
    }
    if row.locked && row.mode != "recovery_key" {
        return Err(locked_response());
    }
    let ok = hex::decode(auth_key)
        .ok()
        .is_some_and(|k| ct_eq(&Sha256::digest(&k), &row.verifier));
    if !ok {
        // A random 64-digit key already resists guessing. Public failed attempts must
        // never destroy someone else's ability to recover with that key.
        if row.mode == "recovery_key" {
            return Err(wrong());
        }
        // The open after an evaluation is that evaluation's guess; any other failed open
        // costs one, so skipping the OPRF step never buys free guesses.
        let armed: bool = sqlx::query_scalar(
            "UPDATE backup_vaults v SET armed = false FROM backup_vaults old
             WHERE v.identity = $1 AND old.identity = v.identity RETURNING old.armed",
        )
        .bind(&row.identity)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| unavailable())?;
        let locked = if armed {
            false
        } else {
            spend_guess(&mut tx, &row.identity, false).await?
        };
        tx.commit().await.map_err(|_| unavailable())?;
        return Err(if locked { locked_response() } else { wrong() });
    }
    if row.generation.as_deref() != generation {
        return Err(err(
            StatusCode::CONFLICT,
            "backup configuration changed; restart restore",
        ));
    }
    sqlx::query(
        "UPDATE backup_vaults SET guesses = 0, armed = false, locked = false WHERE identity = $1",
    )
    .bind(&row.identity)
    .execute(&mut *tx)
    .await
    .map_err(|_| unavailable())?;
    Ok((row, tx))
}

pub async fn restore_open(
    State(st): State<Shared>,
    crate::ClientIp(ip): crate::ClientIp,
    Json(req): Json<OpenReq>,
) -> Response {
    let Some(handle) = roda_proto::normalize_handle(&req.handle) else {
        return err(StatusCode::BAD_REQUEST, "bad handle");
    };
    if let Err(r) = restore_limit(&st, &ip, &handle) {
        return r;
    }
    match authorize(&st, &handle, &req.auth_key, req.generation.as_deref()).await {
        Ok((row, tx)) => {
            if tx.commit().await.is_err() {
                return unavailable();
            }
            match row.blob_bytes {
                Some(size) => Json(OpenResp {
                    wrapped_key: hex::encode(row.wrapped_key),
                    size,
                    generation: row.generation,
                })
                .into_response(),
                None => err(StatusCode::NOT_FOUND, "no backup uploaded yet"),
            }
        }
        Err(r) => r,
    }
}

#[derive(Deserialize)]
pub struct BlobQuery {
    handle: String,
    generation: Option<String>,
}

pub async fn restore_blob(
    State(st): State<Shared>,
    crate::ClientIp(ip): crate::ClientIp,
    Query(q): Query<BlobQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(handle) = roda_proto::normalize_handle(&q.handle) else {
        return err(StatusCode::BAD_REQUEST, "bad handle");
    };
    if let Err(r) = restore_limit(&st, &ip, &handle) {
        return r;
    }
    let auth = headers
        .get("x-zoen-backup-auth")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let (row, tx) = match authorize(&st, &handle, auth, q.generation.as_deref()).await {
        Ok(r) => r,
        Err(r) => return r,
    };
    let path = object_key(&row.identity, row.blob_key.as_deref());
    // Keep the snapshot row locked through headers and the entire bounded body fetch.
    match tokio::time::timeout(st.backup_settings.storage_timeout, async {
        st.blobs.get(&path).await?.bytes().await
    })
    .await
    {
        Ok(Ok(b)) => {
            if tx.commit().await.is_err() {
                return unavailable();
            }
            ([("content-type", "application/octet-stream")], b).into_response()
        }
        Ok(Err(object_store::Error::NotFound { .. })) => err(StatusCode::NOT_FOUND, "missing"),
        Ok(Err(_)) | Err(_) => err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable"),
    }
}
