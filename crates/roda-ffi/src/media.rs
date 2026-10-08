//! Encrypted media on the relay (ADR 0007).
//!
//! In a relay-synced chat an attachment travels as an encrypted copy: a fresh random key
//! per attachment, XChaCha20-Poly1305, nonce prepended. The relay stores the copy under the
//! sha256 of the ciphertext and never sees the key; the key rides inside the chat's log
//! (MLS-encrypted from M2 on). Locally the plaintext stays content-addressed as before.

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use roda_types::{IdentityId, MediaRef};
use sha2::{Digest, Sha256};

use crate::engine::{now_ms, Engine, EntryBody, R};

const NONCE: usize = 24;

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("system randomness");
    b
}

/// `nonce ‖ ciphertext+tag` under a new random key.
pub(crate) fn seal(plain: &[u8]) -> ([u8; 32], Vec<u8>) {
    let key = random::<32>();
    let nonce = random::<NONCE>();
    let cipher = XChaCha20Poly1305::new_from_slice(&key).expect("32-byte key");
    let ct = cipher
        .encrypt(&XNonce::from(nonce), plain)
        .expect("encrypt");
    let mut out = Vec::with_capacity(NONCE + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    (key, out)
}

/// The plaintext, or `None` if the key is wrong or a byte changed.
pub(crate) fn open(key: &[u8], blob: &[u8]) -> Option<Vec<u8>> {
    if blob.len() < NONCE + 16 {
        return None;
    }
    let cipher = XChaCha20Poly1305::new_from_slice(key).ok()?;
    let nonce: [u8; NONCE] = blob[..NONCE].try_into().ok()?;
    cipher.decrypt(&XNonce::from(nonce), &blob[NONCE..]).ok()
}

/// An attachment someone shared that this device doesn't have yet.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Wanted {
    pub plain: String,
    pub blob: String,
    pub key: String,
    pub mime: String,
    /// The chat showing it, or empty for a profile photo.
    pub space: String,
    /// Whose profile photo it is.
    pub profile: Option<IdentityId>,
}

impl Engine {
    /// Gives `m` its encrypted copy (made once per attachment, then reused) and queues the
    /// upload. Only for relay-synced chats; local chats keep plain references.
    pub(crate) fn seal_for_relay(&mut self, m: &mut MediaRef) -> R<()> {
        if let Some((blob, key)) = self.store.media_key(&m.sha256)? {
            m.blob = Some(blob);
            m.key = Some(key);
            return Ok(());
        }
        let Some(plain) = self.media(&m.sha256)? else {
            return Err(crate::CoreError::Invalid {
                reason: crate::i18n::t("anexo não encontrado", "attachment not found"),
            });
        };
        let (key, sealed) = seal(&plain);
        let blob = hex::encode(Sha256::digest(&sealed));
        let key = hex::encode(key);
        self.store
            .seal_media(&m.sha256, &blob, &key, &sealed, now_ms())?;
        m.blob = Some(blob);
        m.key = Some(key);
        Ok(())
    }

    /// Encrypted copies still to upload (oldest first).
    pub fn pending_uploads(&self, limit: u32) -> Vec<(String, Vec<u8>)> {
        self.store.pending_uploads(limit).unwrap_or_default()
    }

    pub fn uploads_pending(&self) -> u64 {
        self.store.uploads_pending().unwrap_or(0)
    }

    pub fn upload_done(&self, blob: &str) {
        let _ = self.store.upload_done(blob);
    }

    /// Photos the chats on the relay and the profiles I can read show right now that this
    /// device is missing.
    pub fn wanted_media(&self) -> Vec<Wanted> {
        let mut out = Vec::new();
        for sid in &self.net.synced {
            let Ok(s) = self.space_state(sid) else {
                continue;
            };
            let latest = s.entries.iter().rev().find_map(|e| match &e.body {
                EntryBody::Background { spec } => Some(spec),
                _ => None,
            });
            let Some(m) = latest.and_then(|b| b.media.as_ref()) else {
                continue;
            };
            let (Some(key), Some(blob)) = (&m.key, &m.blob) else {
                continue;
            };
            if matches!(self.store.media(&m.sha256), Ok(Some(_))) {
                continue;
            }
            out.push(Wanted {
                plain: m.sha256.clone(),
                blob: blob.clone(),
                key: key.clone(),
                mime: m.mime.clone(),
                space: sid.clone(),
                profile: None,
            });
        }
        out.extend(self.wanted_profile_photos());
        out
    }

    /// Stores a downloaded copy if it is exactly what the event promised: the ciphertext
    /// hashes to `blob`, opens with `key`, and the plaintext hashes to `plain`.
    pub fn media_arrived(&mut self, w: &Wanted, sealed: &[u8]) -> bool {
        if hex::encode(Sha256::digest(sealed)) != w.blob {
            return false;
        }
        let Ok(key) = hex::decode(&w.key) else {
            return false;
        };
        let Some(plain) = open(&key, sealed) else {
            return false;
        };
        if !matches!(self.import_media(&w.plain, &plain, &w.mime), Ok(true)) {
            return false;
        }
        let _ = self.store.learn_media_key(&w.plain, &w.blob, &w.key);
        self.index_dirty = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_opens_with_its_key_only() {
        let (key, sealed) = seal(b"a photo");
        assert_eq!(open(&key, &sealed).as_deref(), Some(&b"a photo"[..]));
        let mut other = key;
        other[0] ^= 1;
        assert!(open(&other, &sealed).is_none());
        let mut tampered = sealed.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(open(&key, &tampered).is_none());
        let (_, again) = seal(b"a photo");
        assert_ne!(sealed, again, "a fresh key and nonce every time");
    }
}
