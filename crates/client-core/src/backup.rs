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
/// device (its MLS groups, outbox, uploads in flight) or is rebuilt (the search index,
/// peer receipt cache). A new device learns peer watermarks from relay catch-up.
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

fn random<const N: usize>() -> R<[u8; N]> {
    Ok(crate::link::random()?)
}

// ───────────────────────────── key derivation ─────────────────────────────

/// The two keys a password or recovery key yields: one wraps `K`, the other proves to the
/// relay that you know the secret (it stores only `sha256(auth)`).
pub struct Keys {
    pub wrap: [u8; 32],
    pub auth: [u8; 32],
}

fn split(ikm: &[u8], salt: &[u8]) -> R<Keys> {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut wrap = [0u8; 32];
    let mut auth = [0u8; 32];
    hk.expand(b"zoen-backup-wrap-v1", &mut wrap)
        .map_err(storage)?;
    hk.expand(b"zoen-backup-auth-v1", &mut auth)
        .map_err(storage)?;
    Ok(Keys { wrap, auth })
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

/// # Errors
/// Refuses unavailable operating-system entropy before producing a blinding scalar.
pub fn blind(password: &str) -> R<Blind> {
    let r = Scalar::from_bytes_mod_order_wide(&random::<64>()?);
    Ok(Blind {
        r,
        blinded: (r * password_point(password)).compress().to_bytes(),
    })
}

/// From the relay's evaluation to the keys: unblind, hash with the password, stretch with
/// Argon2id salted by the identity, split.
/// # Errors
/// Rejects an invalid relay evaluation or unsupported KDF parameters, and propagates
/// Argon2 or HKDF failures without returning partially derived keys.
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
    split(&stretched, b"zoen-backup-password-v1")
}

/// 64 random digits, in groups of four.
/// # Errors
/// Propagates unavailable entropy without returning a partial recovery key.
pub fn new_recovery_key() -> R<String> {
    recovery_key_with_random(random::<32>)
}

fn recovery_key_with_random(mut fill: impl FnMut() -> R<[u8; 32]>) -> R<String> {
    let mut digits = String::with_capacity(RECOVERY_DIGITS);
    while digits.len() < RECOVERY_DIGITS {
        for b in fill()? {
            // Rejection sampling keeps every digit uniform.
            if b < 250 && digits.len() < RECOVERY_DIGITS {
                digits.push(char::from(b'0' + b % 10));
            }
        }
    }
    Ok(digits
        .as_bytes()
        .chunks(4)
        .map(|chunk| chunk.iter().copied().map(char::from).collect::<String>())
        .collect::<Vec<_>>()
        .join(" "))
}

/// # Errors
/// Rejects malformed recovery digits and propagates key-derivation failures.
pub fn recovery_keys(recovery_key: &str) -> R<Keys> {
    let digits: String = recovery_key
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    if digits.len() != RECOVERY_DIGITS
        || !recovery_key
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_whitespace())
    {
        return Err(invalid(t(
            "A chave de recuperação tem 64 números.",
            "The recovery key has 64 digits.",
        )));
    }
    split(digits.as_bytes(), b"zoen-backup-recovery-key-v1")
}

fn seal(key: &[u8; 32], aad: &[u8], plain: &[u8]) -> R<Vec<u8>> {
    let nonce = random::<24>()?;
    let ct = XChaCha20Poly1305::new(key.into())
        .encrypt(&XNonce::from(nonce), Payload { msg: plain, aad })
        .map_err(storage)?;
    Ok([nonce.as_slice(), &ct].concat())
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

/// # Errors
/// Refuses unavailable nonce entropy or authenticated-encryption failure.
pub fn wrap_key(keys: &Keys, identity: &str, k: &[u8; 32]) -> R<Vec<u8>> {
    seal(&keys.wrap, &key_aad(identity), k)
}

/// # Errors
/// Rejects failed authentication or a decrypted key with the wrong size.
pub fn unwrap_key(keys: &Keys, identity: &str, wrapped: &[u8]) -> R<[u8; 32]> {
    open(&keys.wrap, &key_aad(identity), wrapped)
        .and_then(|k| k.try_into().ok())
        .ok_or_else(|| invalid("backup key doesn't open"))
}

/// # Errors
/// Refuses unavailable entropy; no replacement or partially filled key is returned.
pub fn new_backup_key() -> R<[u8; 32]> {
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

/// # Errors
/// Rejects an unserializable or oversized header and propagates nonce entropy
/// and authenticated-encryption failures before returning an envelope.
pub fn seal_payload(k: &[u8; 32], header: &Header, db: &[u8]) -> R<Vec<u8>> {
    let h = serde_json::to_vec(header).map_err(storage)?;
    let header_len = u32::try_from(h.len()).map_err(|_| invalid("backup header is too large"))?;
    let mut plain = Vec::with_capacity(12 + h.len() + db.len());
    plain.extend_from_slice(MAGIC);
    plain.extend_from_slice(&header_len.to_be_bytes());
    plain.extend_from_slice(&h);
    plain.extend_from_slice(db);
    seal(k, &payload_aad(&header.identity), &plain)
}

/// # Errors
/// Rejects authentication failures, invalid framing or JSON, unsupported versions
/// and a header that does not belong to the expected identity.
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

mod snapshot;
pub(crate) use snapshot::preflight_snapshot;

#[cfg(test)]
mod payload_tests {
    use super::*;
    use std::collections::HashSet;

    fn fixture() -> ([u8; 32], Header, Vec<u8>) {
        let identity = event_log::Signer::from_secret(&[31; 32]);
        let header = Header {
            version: PAYLOAD_VERSION,
            identity: identity.id(),
            relay_url: "https://relay.invalid".into(),
            identity_secret: hex::encode(identity.secret()),
            agreement_secret: hex::encode([32; 32]),
            created_ms: 123_456,
        };
        ([33; 32], header, (0..=u8::MAX).collect())
    }

    fn assert_invalid(key: &[u8; 32], identity: &str, bytes: &[u8], case: &str) {
        assert!(
            matches!(
                open_payload(key, identity, bytes),
                Err(CoreError::Invalid { .. })
            ),
            "{case} must not produce a readable backup"
        );
    }

    #[test]
    fn repeated_backup_payloads_use_fresh_nonces_and_preserve_every_byte() {
        let (key, header, database) = fixture();
        let mut nonces = HashSet::new();
        for _ in 0..8 {
            let sealed = seal_payload(&key, &header, &database).unwrap();
            assert!(
                nonces.insert(sealed[..24].to_vec()),
                "identical backup content reused its encryption nonce"
            );
            let (opened_header, opened_database) =
                open_payload(&key, &header.identity, &sealed).unwrap();
            assert_eq!(
                serde_json::to_value(opened_header).unwrap(),
                serde_json::to_value(&header).unwrap()
            );
            assert_eq!(opened_database, database);
        }
    }

    #[test]
    fn backup_payload_authentication_rejects_mutation_truncation_and_other_owners() {
        let (key, header, database) = fixture();
        let sealed = seal_payload(&key, &header, &database).unwrap();
        let other_owner = event_log::Signer::from_secret(&[34; 32]).id();
        assert_invalid(&[35; 32], &header.identity, &sealed, "wrong key");
        assert_invalid(&key, &other_owner, &sealed, "wrong owner");
        for (case, index) in [
            ("first nonce byte", 0),
            ("last nonce byte", 23),
            ("ciphertext byte", 24),
            ("last ciphertext byte", sealed.len() - 17),
            ("authentication tag", sealed.len() - 1),
        ] {
            let mut changed = sealed.clone();
            changed[index] ^= 1;
            assert_invalid(&key, &header.identity, &changed, case);
        }
        for length in [0, 1, 23, 24, 25, sealed.len() - 17, sealed.len() - 1] {
            assert_invalid(
                &key,
                &header.identity,
                &sealed[..length],
                "truncated envelope",
            );
        }
        let mut appended = sealed;
        appended.push(0);
        assert_invalid(&key, &header.identity, &appended, "appended bytes");
    }

    #[test]
    fn authenticated_backup_payloads_reject_invalid_framing_version_and_inner_owner() -> R<()> {
        let (key, mut header, database) = fixture();
        let owner = header.identity.clone();
        let sealed = seal_payload(&key, &header, &database).unwrap();
        let plain = open(&key, &payload_aad(&owner), &sealed).unwrap();
        let mut bad_magic = plain.clone();
        bad_magic[0] ^= 1;
        let mut header_overrun = plain.clone();
        header_overrun[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        let mut invalid_json = Vec::from(MAGIC.as_slice());
        invalid_json.extend_from_slice(&2u32.to_be_bytes());
        invalid_json.extend_from_slice(b"{!");
        for (case, malformed) in [
            ("empty authenticated payload", Vec::new()),
            ("truncated framing", plain[..11].to_vec()),
            ("wrong magic", bad_magic),
            ("declared header exceeds payload", header_overrun),
            ("invalid header JSON", invalid_json),
        ] {
            // Re-seal deliberately bad framing so rejection must inspect the authenticated body.
            let sealed = seal(&key, &payload_aad(&owner), &malformed)?;
            assert_invalid(&key, &owner, &sealed, case);
        }
        header.version = PAYLOAD_VERSION + 1;
        let wrong_version = seal_payload(&key, &header, &database).unwrap();
        assert_invalid(&key, &owner, &wrong_version, "unsupported payload version");

        header.version = PAYLOAD_VERSION;
        header.identity = event_log::Signer::from_secret(&[36; 32]).id();
        let inner_owner_payload = seal_payload(&key, &header, &database).unwrap();
        let inner_owner_plain =
            open(&key, &payload_aad(&header.identity), &inner_owner_payload).unwrap();
        let wrong_inner_owner = seal(&key, &payload_aad(&owner), &inner_owner_plain)?;
        assert_invalid(
            &key,
            &owner,
            &wrong_inner_owner,
            "authenticated inner owner mismatch",
        );
        Ok(())
    }
}

#[cfg(test)]
mod snapshot_tests;

#[cfg(test)]
mod entropy_tests;
