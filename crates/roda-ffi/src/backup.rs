//! Encrypted server backup, device side (ADR 0046).
//!
//! A backup is the device database minus everything tied to this device (MLS state, outbox,
//! search index), plus the identity and agreement secrets, sealed under a random backup key
//! `K`. `K` is wrapped either by a password, through an OPRF on the relay's vault (so the
//! relay can count guesses but never learn or test the password), or by 64 digits only the
//! person keeps. Restoring makes a new device key certified by the restored identity.

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

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

fn snapshot_root(database: &str) -> std::path::PathBuf {
    // Android's process temp directory may be /data/local/tmp, outside the app sandbox.
    if database == ":memory:" {
        return std::env::temp_dir();
    }
    std::path::Path::new(database)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf()
}

/// `Drop` does not run after SIGKILL, so the next backup or restore removes only
/// `zoen-backup-<32 hex>` and `zoen-restore-<32 hex>` directories.
fn is_snapshot_leftover(name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    let Some(rest) = name
        .strip_prefix("zoen-backup-")
        .or_else(|| name.strip_prefix("zoen-restore-"))
    else {
        return false;
    };
    rest.len() == 32
        && rest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sweep_snapshot_leftovers(database: &str) {
    let Ok(entries) = std::fs::read_dir(snapshot_root(database)) else {
        return;
    };
    for entry in entries.flatten() {
        if !is_snapshot_leftover(&entry.file_name()) {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        if meta.is_dir() {
            remove_db(&path.join("snapshot.db"));
            let _ = std::fs::remove_dir(&path);
        }
    }
}

const SNAPSHOT_LOCK: &str = "zoen-snapshot.lock";

/// One permanent lock for the folder snapshots are created in. Backup and
/// restore hold it shared from before `mkdir` until after the directory is
/// gone. Only a sweep takes it exclusively, and only with `try_lock`, so a
/// busy folder is left alone. A crashed process drops the flock; the next
/// idle operation removes that leftover.
struct SnapshotFolderLock {
    file: std::fs::File,
}

impl SnapshotFolderLock {
    fn acquire(database: &str) -> R<Self> {
        let path = snapshot_root(database).join(SNAPSHOT_LOCK);
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        options.mode(0o600);
        let file = options.open(&path).map_err(storage)?;
        #[cfg(unix)]
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).map_err(storage)?;
        match file.try_lock() {
            Ok(()) => {
                sweep_snapshot_leftovers(database);
                file.unlock().map_err(storage)?;
            }
            Err(std::fs::TryLockError::WouldBlock) => {}
            Err(std::fs::TryLockError::Error(error)) => return Err(storage(error)),
        }
        file.lock_shared().map_err(storage)?;
        Ok(Self { file })
    }
}

impl Drop for SnapshotFolderLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// `O_EXCL` fails closed when `snapshot.db` is already a file or a symlink.
fn create_exclusive_snapshot(path: &std::path::Path) -> R<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(path).map_err(storage)?;
    #[cfg(unix)]
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(storage)?;
    drop(file);
    Ok(())
}

struct SnapshotDatabase {
    path: std::path::PathBuf,
}

impl SnapshotDatabase {
    fn new(database: &str, tag: &str) -> R<Self> {
        let directory =
            snapshot_root(database).join(format!("zoen-{tag}-{}", hex::encode(random::<16>())));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        builder.mode(0o700);
        builder.create(&directory).map_err(storage)?;
        #[cfg(unix)]
        if let Err(error) =
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
        {
            let _ = std::fs::remove_dir(&directory);
            return Err(storage(error));
        }
        let path = directory.join("snapshot.db");
        if let Err(error) = create_exclusive_snapshot(&path) {
            let _ = std::fs::remove_dir(&directory);
            return Err(error);
        }
        Ok(Self { path })
    }
}

impl Drop for SnapshotDatabase {
    fn drop(&mut self) {
        remove_db(&self.path);
        if let Some(directory) = self.path.parent() {
            // The lock stays for the life of the folder. It is unlinked only
            // when this temp database directory itself is going away.
            if std::fs::remove_dir(directory).is_err() {
                let _ = std::fs::remove_file(directory.join(SNAPSHOT_LOCK));
                let _ = std::fs::remove_dir(directory);
            }
        }
    }
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
        let _lock = SnapshotFolderLock::acquire(&self.db_path)?;
        let scratch = SnapshotDatabase::new(&self.db_path, "backup")?;
        let path = &scratch.path;
        let out = (|| -> R<Vec<u8>> {
            self.store
                .conn()
                .execute(
                    "VACUUM INTO ?1",
                    [path.to_str().ok_or_else(|| storage("temp path"))?],
                )
                .map_err(storage)?;
            let c = rusqlite::Connection::open(path).map_err(storage)?;
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
                    OR key LIKE 'mls.%' OR key LIKE 'invite:%' OR key LIKE 'page.draft.%';
                 VACUUM;",
            )
            .map_err(storage)?;
            drop(c);
            std::fs::read(path).map_err(storage)
        })();
        drop(scratch);
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
        let _lock = SnapshotFolderLock::acquire(&self.db_path)?;
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
        let scratch = SnapshotDatabase::new(&self.db_path, "restore")?;
        let path = &scratch.path;
        let res = (|| -> R<()> {
            std::fs::write(path, db).map_err(storage)?;
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
        drop(scratch);
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

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    #[test]
    fn plaintext_snapshot_stays_beside_the_database_in_a_private_directory() {
        let database = SnapshotDatabase::new(":memory:", "fixture").unwrap();
        let scratch = SnapshotDatabase::new(database.path.to_str().unwrap(), "backup").unwrap();
        let directory = scratch.path.parent().unwrap().to_path_buf();
        assert_eq!(directory.parent(), database.path.parent());
        #[cfg(unix)]
        assert_eq!(
            std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for ext in ["", "-wal", "-shm", "-journal"] {
            let mut path = scratch.path.as_os_str().to_owned();
            path.push(ext);
            std::fs::write(path, b"private plaintext").unwrap();
        }
        drop(scratch);
        assert!(!directory.exists());
    }

    #[test]
    fn successful_and_failed_backup_snapshots_leave_no_plaintext_files() {
        let database = SnapshotDatabase::new(":memory:", "fixture").unwrap();
        let engine = Engine::open(database.path.to_str().unwrap()).unwrap();
        assert!(engine
            .backup_snapshot()
            .unwrap()
            .starts_with(b"SQLite format 3\0"));
        engine
            .store
            .conn()
            .execute_batch(r#"CREATE TABLE "bad""name" (value TEXT);"#)
            .unwrap();
        assert!(engine.backup_snapshot().is_err());
        assert!(std::fs::read_dir(database.path.parent().unwrap())
            .unwrap()
            .all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("zoen-backup-")
            }));
    }

    #[test]
    fn backup_snapshot_omits_page_draft_keys() {
        let database = SnapshotDatabase::new(":memory:", "fixture").unwrap();
        let engine = Engine::open(database.path.to_str().unwrap()).unwrap();
        engine
            .store
            .conn()
            .execute(
                "INSERT INTO meta (key, value) VALUES ('page.draft.1', 'secret'), ('kept', 'yes')",
                [],
            )
            .unwrap();
        let bytes = engine.backup_snapshot().unwrap();
        let scratch = SnapshotDatabase::new(database.path.to_str().unwrap(), "read").unwrap();
        std::fs::write(&scratch.path, &bytes).unwrap();
        let connection = rusqlite::Connection::open(&scratch.path).unwrap();
        let draft: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key = 'page.draft.1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let kept: String = connection
            .query_row("SELECT value FROM meta WHERE key = 'kept'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(draft, 0);
        assert_eq!(kept, "yes");
    }

    #[test]
    fn next_backup_removes_a_crashed_snapshot_and_keeps_neighbors() {
        let database = SnapshotDatabase::new(":memory:", "fixture").unwrap();
        let engine = Engine::open(database.path.to_str().unwrap()).unwrap();
        let parent = database.path.parent().unwrap();
        let crashed = parent.join("zoen-backup-0123456789abcdef0123456789abcdef");
        std::fs::create_dir(&crashed).unwrap();
        std::fs::write(crashed.join("snapshot.db"), b"plaintext secret").unwrap();
        std::fs::write(crashed.join("snapshot.db-wal"), b"wal secret").unwrap();
        let notes = parent.join("field-notes");
        std::fs::create_dir(&notes).unwrap();
        std::fs::write(notes.join("keep.txt"), b"stay").unwrap();
        let note = parent.join("readme.txt");
        std::fs::write(&note, b"stay-file").unwrap();
        let almost = parent.join("zoen-backup-short");
        std::fs::create_dir(&almost).unwrap();
        std::fs::write(almost.join("snapshot.db"), b"not-ours").unwrap();
        let regular = parent.join("zoen-restore-11111111111111111111111111111111");
        std::fs::write(&regular, b"not-a-directory").unwrap();
        #[cfg(unix)]
        let link = parent.join("zoen-backup-abcdefabcdefabcdefabcdefabcdefab");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&notes, &link).unwrap();

        assert!(engine
            .backup_snapshot()
            .unwrap()
            .starts_with(b"SQLite format 3\0"));
        assert!(!crashed.exists());
        assert_eq!(std::fs::read(notes.join("keep.txt")).unwrap(), b"stay");
        assert_eq!(std::fs::read(&note).unwrap(), b"stay-file");
        assert_eq!(
            std::fs::read(almost.join("snapshot.db")).unwrap(),
            b"not-ours"
        );
        assert_eq!(std::fs::read(&regular).unwrap(), b"not-a-directory");
        #[cfg(unix)]
        assert!(std::fs::symlink_metadata(&link).is_err());

        std::fs::remove_dir_all(&notes).unwrap();
        std::fs::remove_file(&note).unwrap();
        std::fs::remove_dir_all(&almost).unwrap();
        std::fs::remove_file(&regular).unwrap();
    }

    #[test]
    fn restore_removes_a_crashed_snapshot_before_rejecting_the_header() {
        let database = SnapshotDatabase::new(":memory:", "fixture").unwrap();
        let mut engine = Engine::open(database.path.to_str().unwrap()).unwrap();
        let parent = database.path.parent().unwrap();
        let crashed = parent.join("zoen-restore-fedcba9876543210fedcba9876543210");
        std::fs::create_dir(&crashed).unwrap();
        std::fs::write(crashed.join("snapshot.db"), b"plaintext secret").unwrap();
        std::fs::write(crashed.join("snapshot.db-journal"), b"journal secret").unwrap();
        let notes = parent.join("field-notes");
        std::fs::create_dir(&notes).unwrap();
        std::fs::write(notes.join("keep.txt"), b"stay").unwrap();

        let header = Header {
            version: 1,
            identity: "not-the-device".into(),
            relay_url: "https://relay.invalid".into(),
            identity_secret: "00".repeat(32),
            agreement_secret: "11".repeat(32),
            created_ms: 0,
        };
        assert!(engine
            .restore_snapshot(&header, b"not a database", roda_log::Signer::generate())
            .is_err());
        assert!(!crashed.exists());
        assert_eq!(std::fs::read(notes.join("keep.txt")).unwrap(), b"stay");
        std::fs::remove_dir_all(&notes).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn exclusive_snapshot_refuses_a_symlink_and_keeps_the_target() {
        let root =
            std::env::temp_dir().join(format!("zoen-exclusive-{}", hex::encode(random::<8>())));
        std::fs::create_dir(&root).unwrap();
        let victim = root.join("victim");
        std::fs::write(&victim, b"keep-me").unwrap();
        let link = root.join("snapshot.db");
        std::os::unix::fs::symlink(&victim, &link).unwrap();
        assert!(create_exclusive_snapshot(&link).is_err());
        assert_eq!(std::fs::read(&victim).unwrap(), b"keep-me");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn exclusive_snapshot_refuses_a_planted_file_and_creates_mode_0600() {
        let root =
            std::env::temp_dir().join(format!("zoen-exclusive-{}", hex::encode(random::<8>())));
        std::fs::create_dir(&root).unwrap();
        let planted = root.join("planted.db");
        std::fs::write(&planted, b"planted").unwrap();
        assert!(create_exclusive_snapshot(&planted).is_err());
        assert_eq!(std::fs::read(&planted).unwrap(), b"planted");
        let created = root.join("snapshot.db");
        create_exclusive_snapshot(&created).unwrap();
        assert_eq!(std::fs::read(&created).unwrap(), b"");
        assert_eq!(
            std::fs::metadata(&created).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn open_snapshot_lock(path: &std::path::Path) -> std::fs::File {
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        options.mode(0o600);
        options.open(path).unwrap()
    }

    #[test]
    fn shared_lock_keeps_a_matching_dir_through_backup() {
        let database = SnapshotDatabase::new(":memory:", "fixture").unwrap();
        let engine = Engine::open(database.path.to_str().unwrap()).unwrap();
        let parent = database.path.parent().unwrap();
        let lock_path = parent.join("zoen-snapshot.lock");
        let lock = open_snapshot_lock(&lock_path);
        lock.lock_shared().unwrap();
        let live = parent.join("zoen-backup-0123456789abcdef0123456789abcdef");
        std::fs::create_dir(&live).unwrap();
        std::fs::write(live.join("snapshot.db"), b"plaintext secret").unwrap();

        assert!(engine
            .backup_snapshot()
            .unwrap()
            .starts_with(b"SQLite format 3\0"));
        assert_eq!(
            std::fs::read(live.join("snapshot.db")).unwrap(),
            b"plaintext secret"
        );
        lock.unlock().unwrap();
        drop(lock);
        std::fs::remove_dir_all(&live).unwrap();
        std::fs::remove_file(&lock_path).unwrap();
    }

    #[test]
    fn restore_on_an_account_device_leaves_an_in_progress_snapshot() {
        let database = SnapshotDatabase::new(":memory:", "fixture").unwrap();
        let mut engine = Engine::open(database.path.to_str().unwrap()).unwrap();
        engine
            .create_account("Ana", "ana", "https://relay.invalid")
            .unwrap();
        let parent = database.path.parent().unwrap();
        let live = parent.join("zoen-restore-fedcba9876543210fedcba9876543210");
        std::fs::create_dir(&live).unwrap();
        std::fs::write(live.join("snapshot.db"), b"plaintext secret").unwrap();
        let header = Header {
            version: 1,
            identity: "not-the-device".into(),
            relay_url: "https://relay.invalid".into(),
            identity_secret: "00".repeat(32),
            agreement_secret: "11".repeat(32),
            created_ms: 0,
        };
        let error = engine
            .restore_snapshot(&header, b"not a database", roda_log::Signer::generate())
            .unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("already has an account") || message.contains("já tem uma conta"),
            "{message}"
        );
        assert_eq!(
            std::fs::read(live.join("snapshot.db")).unwrap(),
            b"plaintext secret"
        );
        assert!(!parent.join("zoen-snapshot.lock").exists());
        std::fs::remove_dir_all(&live).unwrap();
    }

    fn release_exclusive_database_lock(engine: &Engine) {
        // Store::open enters WAL under locking_mode=EXCLUSIVE, which stores the
        // wal-index in heap memory and then refuses to leave exclusive mode.
        // Delete the journal first so NORMAL can stick, then open WAL again.
        let conn = engine.store.conn();
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let deleted: String = conn
            .query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))
            .unwrap();
        assert_eq!(deleted, "delete", "journal stayed {deleted}");
        let mode: String = conn
            .query_row("PRAGMA locking_mode=NORMAL", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "normal", "locking stayed {mode}");
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap();
        let wal: String = conn
            .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
            .unwrap();
        assert_eq!(wal, "wal", "journal stayed {wal}");
    }

    fn assert_backup_decrypts(engine: &crate::RodaEngine) {
        let db = engine.lock().backup_snapshot().expect("backup");
        let header = Header {
            version: PAYLOAD_VERSION,
            identity: "same-file".into(),
            relay_url: "https://relay.invalid".into(),
            identity_secret: "ab".repeat(32),
            agreement_secret: "cd".repeat(32),
            created_ms: 1,
        };
        let key = [7u8; 32];
        let sealed = seal_payload(&key, &header, &db).expect("seal");
        let (opened, plain) = open_payload(&key, &header.identity, &sealed).expect("open");
        assert_eq!(opened.identity, header.identity);
        assert_eq!(plain, db);
        assert!(plain.starts_with(b"SQLite format 3\0"));
    }

    #[test]
    fn two_engines_backup_the_same_file_concurrently() {
        let database = SnapshotDatabase::new(":memory:", "fixture").unwrap();
        let path = database.path.to_str().unwrap().to_string();
        let left = crate::RodaEngine::open(path.clone(), "pt-BR".into()).unwrap();
        {
            let engine = left.lock();
            engine
                .store
                .conn()
                .execute_batch("CREATE TABLE filler (n INTEGER PRIMARY KEY, blob BLOB);")
                .unwrap();
            for n in 0..32 {
                engine
                    .store
                    .conn()
                    .execute(
                        "INSERT INTO filler (n, blob) VALUES (?1, zeroblob(8192))",
                        [n],
                    )
                    .unwrap();
            }
            release_exclusive_database_lock(&engine);
        }
        let right = crate::RodaEngine::open(path, "en".into()).expect("second engine");
        release_exclusive_database_lock(&right.lock());
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            for engine in [&left, &right] {
                scope.spawn(|| {
                    for _ in 0..20 {
                        barrier.wait();
                        assert_backup_decrypts(engine);
                    }
                });
            }
        });
    }

    #[cfg(unix)]
    #[test]
    fn snapshot_lock_is_mode_0600_under_umask_000() {
        #[cfg(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "watchos",
            target_os = "tvos"
        ))]
        type Mode = u16;
        #[cfg(not(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "watchos",
            target_os = "tvos"
        )))]
        type Mode = u32;
        extern "C" {
            fn umask(mask: Mode) -> Mode;
        }
        struct Restore(Mode);
        impl Drop for Restore {
            fn drop(&mut self) {
                unsafe {
                    umask(self.0);
                }
            }
        }
        let _restore = Restore(unsafe { umask(0) });
        let database = SnapshotDatabase::new(":memory:", "fixture").unwrap();
        let engine = Engine::open(database.path.to_str().unwrap()).unwrap();
        engine.backup_snapshot().unwrap();
        let lock = database.path.parent().unwrap().join("zoen-snapshot.lock");
        assert_eq!(
            std::fs::metadata(&lock).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn memory_database_puts_the_snapshot_lock_in_the_temp_dir() {
        let engine = Engine::open(":memory:").unwrap();
        let bytes = engine.backup_snapshot().unwrap();
        assert!(bytes.starts_with(b"SQLite format 3\0"));
        let lock = std::env::temp_dir().join("zoen-snapshot.lock");
        assert!(lock.is_file());
        assert_eq!(
            std::fs::metadata(&lock).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    const CROSS_PROCESS_TEST: &str =
        "backup::snapshot_tests::another_process_holding_the_shared_lock_keeps_the_snapshot";

    #[cfg(unix)]
    fn hold_shared_snapshot_lock() {
        let lock_path = std::env::var("ZOEN_R41_HOLD_SNAPSHOT_LOCK").unwrap();
        let ready = std::env::var("ZOEN_R41_HOLD_READY").unwrap();
        let release = std::env::var("ZOEN_R41_HOLD_RELEASE").unwrap();
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(lock_path)
            .unwrap();
        file.lock_shared().unwrap();
        std::fs::write(&ready, b"ready").unwrap();
        let started = std::time::Instant::now();
        while !std::path::Path::new(&release).exists() {
            if started.elapsed() > std::time::Duration::from_secs(4) {
                panic!("parent did not release the lock holder");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        file.unlock().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn another_process_holding_the_shared_lock_keeps_the_snapshot() {
        if std::env::var_os("ZOEN_R41_HOLD_SNAPSHOT_LOCK").is_some() {
            hold_shared_snapshot_lock();
            return;
        }
        let started = std::time::Instant::now();
        let database = SnapshotDatabase::new(":memory:", "fixture").unwrap();
        let engine = Engine::open(database.path.to_str().unwrap()).unwrap();
        let parent = database.path.parent().unwrap();
        let lock_path = parent.join("zoen-snapshot.lock");
        drop(open_snapshot_lock(&lock_path));
        let ready = parent.join("holder-ready");
        let release = parent.join("holder-release");
        struct ReleaseOnDrop(std::path::PathBuf);
        impl Drop for ReleaseOnDrop {
            fn drop(&mut self) {
                let _ = std::fs::write(&self.0, b"release");
            }
        }
        struct ChildGuard(Option<std::process::Child>);
        impl Drop for ChildGuard {
            fn drop(&mut self) {
                if let Some(child) = self.0.as_mut() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("--test-threads=1")
            .arg(CROSS_PROCESS_TEST)
            .env("ZOEN_R41_HOLD_SNAPSHOT_LOCK", &lock_path)
            .env("ZOEN_R41_HOLD_READY", &ready)
            .env("ZOEN_R41_HOLD_RELEASE", &release)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .unwrap();
        let mut child = ChildGuard(Some(child));
        let release_on_drop = ReleaseOnDrop(release);
        let wait_started = std::time::Instant::now();
        while !ready.exists() {
            if wait_started.elapsed() > std::time::Duration::from_secs(3) {
                panic!("child did not take the shared lock");
            }
            match child.0.as_mut().unwrap().try_wait() {
                Ok(Some(status)) => panic!("child exited before locking: {status}"),
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(error) => panic!("child wait failed: {error}"),
            }
        }
        let live = parent.join("zoen-backup-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        std::fs::create_dir(&live).unwrap();
        std::fs::write(live.join("snapshot.db"), b"held by child").unwrap();
        assert!(engine
            .backup_snapshot()
            .unwrap()
            .starts_with(b"SQLite format 3\0"));
        assert_eq!(
            std::fs::read(live.join("snapshot.db")).unwrap(),
            b"held by child"
        );
        drop(release_on_drop);
        let mut child = child.0.take().unwrap();
        let wait_exit = std::time::Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if wait_exit.elapsed() > std::time::Duration::from_secs(2) {
                panic!("child did not exit");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert!(status.success());
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        std::fs::remove_dir_all(&live).unwrap();
        std::fs::remove_file(&lock_path).unwrap();
    }
}
