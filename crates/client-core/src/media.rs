//! Encrypted media on the relay (ADR 0007).
//!
//! In a relay-synced chat an attachment travels as an encrypted copy: a fresh random key
//! per attachment, XChaCha20-Poly1305, nonce prepended. The relay stores the copy under the
//! sha256 of the ciphertext and never sees the key; the key rides inside the chat's log
//! (MLS-encrypted from M2 on). Locally the plaintext stays content-addressed as before.

use crate::engine::{now_ms, Engine, EntryBody, R};
pub(crate) use event_log::media::{open, seal};
use sha2::{Digest, Sha256};
use types::{IdentityId, MediaRef};

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
    /// Descriptor bound, including the 24-byte nonce and tag. Legacy references
    /// without an exact length still use the existing profile/background policy.
    pub maximum_sealed_bytes: Option<u64>,
}

impl Wanted {
    pub(crate) fn sealed_limit(&self) -> u64 {
        if self.profile.is_some() {
            let policy = event_log::profile::MAX_PHOTO_BYTES as u64 + 40;
            self.maximum_sealed_bytes.unwrap_or(policy).min(policy)
        } else {
            self.maximum_sealed_bytes
                .unwrap_or(crate::universal::MAX_MEDIA_BYTES as u64 + 40)
        }
    }
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
        let (key, sealed) = seal(&plain)?;
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
        if self.store.upload_done(blob).is_ok() {
            self.net.wake();
        }
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
            if m.bytes == 0 || m.bytes > crate::universal::MAX_MEDIA_BYTES as u64 {
                continue;
            }
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
                maximum_sealed_bytes: m.bytes.checked_add(40),
            });
        }
        out.extend(self.wanted_profile_photos());
        out.extend(self.wanted_item_blobs());
        out
    }

    /// Stores a downloaded copy if it is exactly what the event promised: the ciphertext
    /// hashes to `blob`, opens with `key`, and the plaintext hashes to `plain`.
    pub fn media_arrived(&mut self, w: &Wanted, sealed: &[u8]) -> bool {
        if sealed.len() as u64 > w.sealed_limit() {
            return false;
        }
        let plain = if w.profile.is_some() {
            let photo = event_log::profile::ProfilePhoto {
                sha256: w.plain.clone(),
                blob: w.blob.clone(),
                key: w.key.clone(),
                mime: w.mime.clone(),
            };
            let Some(plain) = photo.open(sealed) else {
                return false;
            };
            plain
        } else {
            if hex::encode(Sha256::digest(sealed)) != w.blob {
                return false;
            }
            let Ok(key) = hex::decode(&w.key) else {
                return false;
            };
            let Some(plain) = open(&key, sealed) else {
                return false;
            };
            plain
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

    fn wanted(
        bytes: &[u8],
        profile: bool,
    ) -> std::result::Result<(Wanted, Vec<u8>), Box<dyn std::error::Error>> {
        let (key, sealed) = seal(bytes)?;
        Ok((
            Wanted {
                plain: hex::encode(Sha256::digest(bytes)),
                blob: hex::encode(Sha256::digest(&sealed)),
                key: hex::encode(key),
                mime: "image/jpeg".into(),
                space: String::new(),
                profile: profile.then(|| "synthetic-profile".into()),
                maximum_sealed_bytes: None,
            },
            sealed,
        ))
    }

    #[test]
    fn profile_cache_matches_existing_nonempty_and_inclusive_size_policy() {
        let mut engine = Engine::open(":memory:").unwrap();
        let cap = event_log::profile::MAX_PHOTO_BYTES;
        for size in [0, cap, cap + 1] {
            let bytes = vec![29; size];
            let (mut request, sealed) = wanted(&bytes, true).unwrap();
            request.maximum_sealed_bytes = Some(u64::MAX);
            assert_eq!(request.sealed_limit(), cap as u64 + 40);
            let accepted = engine.media_arrived(&request, &sealed);
            assert_eq!(accepted, size == cap, "profile size {size}");
            assert_eq!(
                engine.media(&request.plain).unwrap(),
                accepted.then_some(bytes)
            );
            assert_eq!(
                engine.store.media_key(&request.plain).unwrap().is_some(),
                accepted
            );
        }
        let (request, mut sealed) = wanted(b"valid following profile", true).unwrap();
        *sealed.last_mut().unwrap() ^= 1;
        assert!(!engine.media_arrived(&request, &sealed));
        assert!(engine.media(&request.plain).unwrap().is_none());
        let (request, sealed) = wanted(b"valid following profile", true).unwrap();
        assert!(engine.media_arrived(&request, &sealed));
        assert_eq!(
            engine.media(&request.plain).unwrap(),
            Some(b"valid following profile".to_vec())
        );
    }

    #[test]
    fn background_demand_binds_declared_size_and_refuses_empty_oversize_or_overflow() {
        let mut engine = Engine::open(":memory:").unwrap();
        engine
            .create_account("Synthetic", "synthetic_bound", "http://127.0.0.1:1")
            .unwrap();
        let space = engine.ensure_personal_space().unwrap();
        engine.net.synced.insert(space.clone());
        let bytes = b"synthetic background";
        for declared in [
            bytes.len() as u64,
            0,
            crate::universal::MAX_MEDIA_BYTES as u64 + 1,
            u64::MAX,
        ] {
            let mut reference = engine.put_media(bytes, "image/jpeg", 1, 1).unwrap();
            reference.bytes = declared;
            let background = crate::BackgroundDto {
                style: "photo".into(),
                media: Some(reference.clone()),
                zoom_pm: 1000,
                offset_x_pm: 0,
                offset_y_pm: 0,
                dim_pm: None,
                blur_pm: 0,
                appearance: "auto".into(),
            };
            engine.set_background(&space, &background).unwrap();
            engine
                .store
                .conn()
                .execute("DELETE FROM media WHERE sha256 = ?1", [&reference.sha256])
                .unwrap();
            let actual: Vec<_> = engine
                .wanted_media()
                .into_iter()
                .filter(|w| w.space == space)
                .collect();
            if declared == bytes.len() as u64 {
                assert_eq!(actual.len(), 1);
                assert_eq!(actual[0].maximum_sealed_bytes, Some(declared + 40));
            } else {
                assert!(actual.is_empty(), "invalid declared size {declared}");
            }
        }
        let (request, _) = wanted(b"legacy reference", false).unwrap();
        assert_eq!(
            request.sealed_limit(),
            crate::universal::MAX_MEDIA_BYTES as u64 + 40
        );
        let mut explicit = request;
        explicit.maximum_sealed_bytes = Some(12 * 1024 * 1024);
        assert_eq!(
            explicit.sealed_limit(),
            12 * 1024 * 1024,
            "File/page descriptor keeps its own bound"
        );
    }

    #[test]
    fn seal_opens_with_its_key_only() {
        let (key, sealed) = seal(b"a photo").unwrap();
        assert_eq!(open(&key, &sealed).as_deref(), Some(&b"a photo"[..]));
        let mut other = key;
        other[0] ^= 1;
        assert!(open(&other, &sealed).is_none());
        let mut tampered = sealed.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(open(&key, &tampered).is_none());
        let (_, again) = seal(b"a photo").unwrap();
        assert_ne!(sealed, again, "a fresh key and nonce every time");
    }
}
