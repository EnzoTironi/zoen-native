//! Encrypted server backup, device side (ADR 0046).
//!
//! A backup is the device database minus everything tied to this device (MLS state, outbox,
//! search index), plus the identity and agreement secrets, sealed under a random backup key
//! `K`. `K` is wrapped either by a password, through an OPRF on the relay's vault (so the
//! relay can count guesses but never learn or test the password), or by 64 digits only the
//! person keeps. Restoring makes a new device key certified by the restored identity.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use curve25519_dalek::{ristretto::CompressedRistretto, RistrettoPoint, Scalar};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};

use crate::engine::Engine;
use crate::i18n::t;
use crate::CoreError;

type R<T> = Result<T, CoreError>;

/// Vault item holding `K` on this device while the backup is on.
pub(crate) const VAULT_BACKUP: &str = "zoen.backup.v1";
pub(crate) const VAULT_BACKUP_GENERATION: &str = "zoen.backup.generation.v1";
const MAGIC: &[u8; 8] = b"ZOENBK1\0";
const PAYLOAD_VERSION: u32 = 1;
/// Tables a new device can use. Everything else in the device database belongs to the old
/// device (its MLS groups, outbox, uploads in flight) or is rebuilt (the search index).
const KEEP_TABLES: &[&str] = &[
    "identities",
    "events",
    "meta",
    "profile_keys",
    "peer_agreement_keys",
    "blocked",
    "synced_spaces",
    "media_keys",
];
pub const MIN_PASSWORD_CHARS: usize = 8;
pub const RECOVERY_DIGITS: usize = 64;

fn invalid(reason: impl Into<String>) -> CoreError {
    CoreError::Invalid {
        reason: reason.into(),
    }
}

fn storage(e: impl std::fmt::Display) -> CoreError {
    CoreError::Storage {
        message: e.to_string(),
    }
}

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("system randomness");
    b
}

// ───────────────────────────── key derivation ─────────────────────────────

/// The two keys a password or recovery key yields: one wraps `K`, the other proves to the
/// relay that you know the secret (it stores only `sha256(auth)`).
pub struct Keys {
    pub wrap: [u8; 32],
    pub auth: [u8; 32],
}

fn split(ikm: &[u8], salt: &[u8]) -> Keys {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut wrap = [0u8; 32];
    let mut auth = [0u8; 32];
    hk.expand(b"zoen-backup-wrap-v1", &mut wrap).expect("32");
    hk.expand(b"zoen-backup-auth-v1", &mut auth).expect("32");
    Keys { wrap, auth }
}

impl Keys {
    pub fn verifier(&self) -> [u8; 32] {
        Sha256::digest(self.auth).into()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Kdf {
    pub alg: String,
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
    pub v: u32,
}

impl Default for Kdf {
    fn default() -> Self {
        Kdf {
            alg: "argon2id".into(),
            m_kib: 64 * 1024,
            t: 3,
            p: 1,
            v: 1,
        }
    }
}

/// OPRF client state for one password: the blinding scalar and the point it sends.
pub struct Blind {
    r: Scalar,
    pub blinded: [u8; 32],
}

fn password_point(password: &str) -> RistrettoPoint {
    RistrettoPoint::hash_from_bytes::<Sha512>(
        &[b"zoen-backup-oprf-v1\0".as_slice(), password.as_bytes()].concat(),
    )
}

pub fn blind(password: &str) -> Blind {
    let r = Scalar::from_bytes_mod_order_wide(&random::<64>());
    Blind {
        r,
        blinded: (r * password_point(password)).compress().to_bytes(),
    }
}

/// From the relay's evaluation to the keys: unblind, hash with the password, stretch with
/// Argon2id salted by the identity, split.
pub fn password_keys(
    password: &str,
    b: &Blind,
    evaluated: &[u8; 32],
    identity: &str,
    kdf: &Kdf,
) -> R<Keys> {
    let z = CompressedRistretto(*evaluated)
        .decompress()
        .ok_or_else(|| invalid("bad evaluation"))?;
    let n = b.r.invert() * z;
    let rwd = Sha512::new()
        .chain_update(b"zoen-backup-oprf-v1-finalize\0")
        .chain_update((password.len() as u64).to_be_bytes())
        .chain_update(password.as_bytes())
        .chain_update(n.compress().as_bytes())
        .finalize();
    if kdf.alg != "argon2id" || kdf.m_kib > 1024 * 1024 || kdf.t > 16 || kdf.p > 8 {
        return Err(invalid("unsupported backup format"));
    }
    let params =
        Params::new(kdf.m_kib, kdf.t, kdf.p, Some(32)).map_err(|e| invalid(e.to_string()))?;
    let salt: [u8; 32] = Sha256::new()
        .chain_update(b"zoen-backup-salt-v1\0")
        .chain_update(identity.as_bytes())
        .finalize()
        .into();
    let mut stretched = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(&rwd, &salt, &mut stretched)
        .map_err(|e| invalid(e.to_string()))?;
    Ok(split(&stretched, b"zoen-backup-password-v1"))
}

/// 64 random digits, in groups of four.
pub fn new_recovery_key() -> String {
    let mut digits = String::with_capacity(RECOVERY_DIGITS);
    while digits.len() < RECOVERY_DIGITS {
        for b in random::<32>() {
            // Rejection sampling keeps every digit uniform.
            if b < 250 && digits.len() < RECOVERY_DIGITS {
                digits.push(char::from(b'0' + b % 10));
            }
        }
    }
    digits
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn recovery_keys(recovery_key: &str) -> R<Keys> {
    let digits: String = recovery_key
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    if digits.len() != RECOVERY_DIGITS {
        return Err(invalid(t(
            "A chave de recuperação tem 64 números.",
            "The recovery key has 64 digits.",
        )));
    }
    Ok(split(digits.as_bytes(), b"zoen-backup-recovery-key-v1"))
}

fn seal(key: &[u8; 32], aad: &[u8], plain: &[u8]) -> Vec<u8> {
    let nonce = random::<24>();
    let ct = XChaCha20Poly1305::new(key.into())
        .encrypt(&XNonce::from(nonce), Payload { msg: plain, aad })
        .expect("encrypt");
    [nonce.as_slice(), &ct].concat()
}

fn open(key: &[u8; 32], aad: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() < 24 {
        return None;
    }
    let nonce: [u8; 24] = sealed[..24].try_into().ok()?;
    XChaCha20Poly1305::new(key.into())
        .decrypt(
            &XNonce::from(nonce),
            Payload {
                msg: &sealed[24..],
                aad,
            },
        )
        .ok()
}

fn key_aad(identity: &str) -> Vec<u8> {
    [b"zoen-backup-key-v1\0".as_slice(), identity.as_bytes()].concat()
}

fn payload_aad(identity: &str) -> Vec<u8> {
    [b"zoen-backup-v1\0".as_slice(), identity.as_bytes()].concat()
}

pub fn wrap_key(keys: &Keys, identity: &str, k: &[u8; 32]) -> Vec<u8> {
    seal(&keys.wrap, &key_aad(identity), k)
}

pub fn unwrap_key(keys: &Keys, identity: &str, wrapped: &[u8]) -> R<[u8; 32]> {
    open(&keys.wrap, &key_aad(identity), wrapped)
        .and_then(|k| k.try_into().ok())
        .ok_or_else(|| invalid("backup key doesn't open"))
}

pub fn new_backup_key() -> [u8; 32] {
    random()
}

// ───────────────────────────── payload ─────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct Header {
    pub version: u32,
    pub identity: String,
    pub relay_url: String,
    pub identity_secret: String,
    pub agreement_secret: String,
    pub created_ms: i64,
}

pub fn seal_payload(k: &[u8; 32], header: &Header, db: &[u8]) -> R<Vec<u8>> {
    let h = serde_json::to_vec(header).map_err(storage)?;
    let mut plain = Vec::with_capacity(12 + h.len() + db.len());
    plain.extend_from_slice(MAGIC);
    plain.extend_from_slice(&(h.len() as u32).to_be_bytes());
    plain.extend_from_slice(&h);
    plain.extend_from_slice(db);
    Ok(seal(k, &payload_aad(&header.identity), &plain))
}

pub fn open_payload(k: &[u8; 32], identity: &str, sealed: &[u8]) -> R<(Header, Vec<u8>)> {
    let plain = open(k, &payload_aad(identity), sealed)
        .ok_or_else(|| invalid(t("O backup não abriu.", "The backup didn't open.")))?;
    let bad = || invalid(t("O backup está corrompido.", "The backup is corrupted."));
    if plain.len() < 12 || &plain[..8] != MAGIC {
        return Err(bad());
    }
    let n = u32::from_be_bytes(plain[8..12].try_into().map_err(|_| bad())?) as usize;
    let h = plain.get(12..12 + n).ok_or_else(bad)?;
    let header: Header = serde_json::from_slice(h).map_err(|_| bad())?;
    if header.version != PAYLOAD_VERSION || header.identity != identity {
        return Err(bad());
    }
    Ok((header, plain[12 + n..].to_vec()))
}

fn temp_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("zoen-{tag}-{}.db", hex::encode(random::<8>())))
}

/// Removes a temp database and its WAL/SHM siblings.
fn remove_db(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    for ext in ["-wal", "-shm", "-journal"] {
        let mut p = path.as_os_str().to_owned();
        p.push(ext);
        let _ = std::fs::remove_file(p);
    }
}

impl Engine {
    /// The device database as a backup sees it: only [`KEEP_TABLES`], without this
    /// device's account, MLS bookkeeping or pending invites.
    pub(crate) fn backup_snapshot(&self) -> R<Vec<u8>> {
        let path = temp_path("backup");
        let out = (|| -> R<Vec<u8>> {
            self.store
                .conn()
                .execute(
                    "VACUUM INTO ?1",
                    [path.to_str().ok_or_else(|| storage("temp path"))?],
                )
                .map_err(storage)?;
            let c = rusqlite::Connection::open(&path).map_err(storage)?;
            let names = |sql: &str| -> R<Vec<String>> {
                let mut st = c.prepare(sql).map_err(storage)?;
                let rows = st
                    .query_map([], |r| r.get::<_, String>(0))
                    .map_err(storage)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(storage);
                rows
            };
            // Virtual tables (the search index) first: dropping one takes its shadow tables.
            for v in names(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND sql LIKE 'CREATE VIRTUAL%'",
            )? {
                c.execute_batch(&format!("DROP TABLE IF EXISTS \"{v}\""))
                    .map_err(storage)?;
            }
            for n in names(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            )? {
                if !KEEP_TABLES.contains(&n.as_str()) {
                    c.execute_batch(&format!("DROP TABLE IF EXISTS \"{n}\""))
                        .map_err(storage)?;
                }
            }
            c.execute_batch(
                "DELETE FROM meta WHERE key = 'account' OR key = 'relay_name' OR key LIKE 'backup.%'
                    OR key LIKE 'mls.%' OR key LIKE 'invite:%';
                 VACUUM;",
            )
            .map_err(storage)?;
            drop(c);
            std::fs::read(&path).map_err(storage)
        })();
        remove_db(&path);
        out
    }

    /// Installs a backup's tables on this (empty) device and makes it the identity's new
    /// device. Returns the new device secret; the caller stores it and the restored
    /// secrets in the vault.
    pub(crate) fn restore_snapshot(
        &mut self,
        header: &Header,
        db: &[u8],
        device: roda_log::Signer,
    ) -> R<[u8; 32]> {
        if self.account().is_some() {
            return Err(invalid(t(
                "Este aparelho já tem uma conta.",
                "This device already has an account.",
            )));
        }
        let root_secret: [u8; 32] = hex::decode(&header.identity_secret)
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| invalid("backup identity"))?;
        let root = roda_log::Signer::from_secret(&root_secret);
        if root.id() != header.identity {
            return Err(invalid("backup identity"));
        }
        if !self.is_empty() {
            self.wipe()?;
        }
        let path = temp_path("restore");
        let res = (|| -> R<()> {
            std::fs::write(&path, db).map_err(storage)?;
            let conn = self.store.conn();
            conn.execute(
                "ATTACH DATABASE ?1 AS bk",
                [path.to_str().ok_or_else(|| storage("temp path"))?],
            )
            .map_err(storage)?;
            let copy = (|| -> R<()> {
                conn.execute_batch("BEGIN").map_err(storage)?;
                for table in KEEP_TABLES {
                    let cols = |schema: &str| -> R<Vec<String>> {
                        let mut st = conn
                            .prepare(&format!("PRAGMA {schema}.table_info(\"{table}\")"))
                            .map_err(storage)?;
                        let v = st
                            .query_map([], |r| r.get::<_, String>(1))
                            .map_err(storage)?
                            .collect::<Result<Vec<_>, _>>()
                            .map_err(storage);
                        v
                    };
                    let theirs = cols("bk")?;
                    let shared: Vec<String> = cols("main")?
                        .into_iter()
                        .filter(|c| theirs.contains(c))
                        .map(|c| format!("\"{c}\""))
                        .collect();
                    if shared.is_empty() {
                        continue;
                    }
                    let list = shared.join(", ");
                    conn.execute_batch(&format!(
                        "DELETE FROM main.\"{table}\";
                         INSERT INTO main.\"{table}\" ({list}) SELECT {list} FROM bk.\"{table}\";"
                    ))
                    .map_err(storage)?;
                }
                conn.execute_batch("COMMIT").map_err(storage)
            })();
            if copy.is_err() {
                let _ = conn.execute_batch("ROLLBACK");
            }
            let _ = conn.execute_batch("DETACH DATABASE bk");
            copy
        })();
        remove_db(&path);
        res?;
        let device_secret = device.secret();
        let device_id = device.id();
        self.install_restored_account(&root, device, &header.relay_url)?;
        // The snapshot has history but no MLS state. Persist requests for this new
        // device so a surviving group admin can add its leaf, even after a relaunch.
        let me = root.id();
        let spaces: Vec<_> = self
            .net
            .synced
            .iter()
            .filter(|space| {
                self.is_e2e(space) && self.mls_roster(space).is_some_and(|r| r.contains_key(&me))
            })
            .cloned()
            .collect();
        for space in spaces {
            self.append(
                &space,
                &me,
                roda_types::EventBody::DeviceJoining {
                    device: device_id.clone(),
                },
            )?;
        }
        Ok(device_secret)
    }
}
