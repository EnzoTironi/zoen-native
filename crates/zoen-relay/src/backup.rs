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
//!   GET    /v1/backup/restore/blob?handle=…   x-zoen-backup-auth: auth_key

// Helpers return the refusal itself, ready to send; it's built once per request, never hot.
#![allow(clippy::result_large_err)]

use std::sync::Arc;

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
use roda_proto::{backup_message, BACKUP_UPLOAD_MAGIC};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Postgres, Row, Transaction};

use crate::Shared;

/// Restore guesses without a success before the vault destroys the OPRF key.
pub const MAX_GUESSES: i32 = 10;
/// Largest sealed backup accepted (text history; media stays on the blob path).
pub const MAX_BACKUP_BYTES: usize = 64 * 1024 * 1024;
const MAX_SKEW_MS: i64 = 5 * 60 * 1000;

#[derive(Clone, Debug, Default)]
pub struct Settings {
    /// Development only. Public password recovery currently lets any caller exhaust
    /// another person's destructive guess counter. Production needs independent proof.
    pub dev_allow_unauthenticated_password_backup: bool,
}

impl Settings {
    pub fn from_env() -> anyhow::Result<Self> {
        let dev_allow_unauthenticated_password_backup =
            match std::env::var("ZOEN_DEV_ALLOW_UNAUTHENTICATED_PASSWORD_BACKUP").as_deref() {
                Err(std::env::VarError::NotPresent) | Ok("0") => false,
                Ok("1") => true,
                _ => anyhow::bail!("ZOEN_DEV_ALLOW_UNAUTHENTICATED_PASSWORD_BACKUP must be 0 or 1"),
            };
        Ok(Self {
            dev_allow_unauthenticated_password_backup,
        })
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

fn unavailable() -> Response {
    err(StatusCode::SERVICE_UNAVAILABLE, "database unavailable")
}

fn hex32(s: &str) -> Option<[u8; 32]> {
    hex::decode(s).ok()?.try_into().ok()
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
    let mut tx = st.pool.begin().await.map_err(|_| unavailable())?;
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

/// Stores the newest sealed backup (replaces the previous one).
pub async fn put_blob(State(st): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let (identity, mut tx) = match signed_identity(&st, &headers, "blob", &body).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    const PREFIX: usize = 8 + 32;
    if body.len() <= PREFIX || body.len() > MAX_BACKUP_BYTES + PREFIX {
        return err(StatusCode::PAYLOAD_TOO_LARGE, "empty or too large");
    }
    if body.get(..8) != Some(BACKUP_UPLOAD_MAGIC.as_slice()) {
        return err(StatusCode::BAD_REQUEST, "backup generation required");
    }
    let generation = hex::encode(&body[8..PREFIX]);
    let body = body.slice(PREFIX..);
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
        if let Err(r) = vault(&st) {
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
    if let Err(e) = st.blobs.put(&path, PutPayload::from_bytes(body)).await {
        tracing::warn!(error = %e, "backup put failed");
        return err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable");
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
            let _ = st.blobs.delete(&old_path).await;
        }
    }
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
    if tx.commit().await.is_err() {
        return unavailable();
    }
    if let Some(row) = old {
        let key: Option<String> = row.get("blob_key");
        let _ = st
            .blobs
            .delete(&object_key(&identity, key.as_deref()))
            .await;
    }
    StatusCode::NO_CONTENT.into_response()
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
    let mut tx = st.pool.begin().await.map_err(|_| unavailable())?;
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
    match st
        .blobs
        .get(&object_key(&row.identity, row.blob_key.as_deref()))
        .await
    {
        Ok(r) => match r.bytes().await {
            Ok(b) => {
                if tx.commit().await.is_err() {
                    return unavailable();
                }
                ([("content-type", "application/octet-stream")], b).into_response()
            }
            Err(_) => err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable"),
        },
        Err(object_store::Error::NotFound { .. }) => err(StatusCode::NOT_FOUND, "missing"),
        Err(_) => err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable"),
    }
}
