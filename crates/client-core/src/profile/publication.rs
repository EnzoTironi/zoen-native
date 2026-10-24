//! Session publication and durable exact upload replay.

use super::{
    seal_profile, sign_agreement, sign_upload, DeviceSigned, Engine, SealedProfile, StoredUpload,
    MAX_CIPHERTEXT, META_AGREEMENT,
};

impl Engine {
    // ── relay traffic ──

    /// A new session: refresh every profile I hold a key for and retry missing shares.
    pub fn profiles_session_start(&mut self, supported: bool) {
        self.net
            .mls
            .wake_recovery_many(self.net.synced.iter().cloned());
        let p = &mut self.net.profiles;
        p.supported = supported;
        p.no_agreement.clear();
        p.shares_dirty = true;
        p.own_refreshed = false;
        p.fetching.clear();
        p.refetch.clear();
        p.need_profiles = self
            .store
            .profile_keys()
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.identity)
            .collect();
    }

    pub fn agreement_to_publish(&self) -> Option<(String, DeviceSigned)> {
        if !self.net.profiles.supported {
            return None;
        }
        let public = self.net.profiles.agreement.as_ref()?.public_hex();
        if self.store.meta(META_AGREEMENT).ok().flatten().as_deref() == Some(public.as_str()) {
            return None;
        }
        let signed = sign_agreement(self.net.author.as_ref()?, &public)?;
        Some((public, signed))
    }

    pub fn agreement_published(&mut self, public: &str) {
        let _ = self.store.set_meta(META_AGREEMENT, public);
    }

    pub(super) fn upload_due(&self) -> bool {
        self.net.profiles.supported
            && self.net.profiles.own_refreshed
            && self.own_profile_journal().is_ok_and(|journal| {
                journal.sync.pending.is_some() && !journal.sync.key_unavailable()
            })
    }

    /// My sealed profile when the relay doesn't have the current version yet.
    pub fn profile_to_upload(&mut self) -> Option<SealedProfile> {
        if !self.upload_due() {
            return None;
        }
        let me = self.me.clone()?;
        let (key, _, version, fields) = self.my_fields(&me).ok()?;
        // Publishing the reference before its encrypted blob would strand contacts.
        if fields
            .photo
            .as_ref()
            .is_some_and(|photo| self.store.upload_pending(&photo.blob).unwrap_or(true))
        {
            return None;
        }
        let mut journal = self.own_profile_journal().ok()?;
        if journal.sync.pending != Some(version) {
            return None;
        }
        if let Some(upload) = &journal.upload {
            if upload.version == version
                && self.net.author.as_ref().is_some_and(|author| {
                    author.device.as_deref() == Some(upload.device.as_str())
                        && author.cert.as_deref() == Some(upload.cert.as_str())
                })
            {
                return Some(upload.sealed(&me));
            }
        }
        let ciphertext = seal_profile(&key, &me, version, &fields).ok()?;
        if ciphertext.len() > MAX_CIPHERTEXT {
            return None;
        }
        let signed = sign_upload(self.net.author.as_ref()?, version, &ciphertext)?;
        let s = SealedProfile {
            identity: me,
            version,
            ciphertext,
            signed,
        };
        journal.upload = Some(StoredUpload {
            version,
            ciphertext: s.ciphertext.clone(),
            device: s.signed.device.clone(),
            cert: s.signed.cert.clone(),
            sig: s.signed.sig.clone(),
        });
        self.save_own_profile_journal(&journal).ok()?;
        Some(s)
    }

    /// The relay answered an upload. A stale version means another of my devices got
    /// further: move past it.
    pub fn profile_upload_answered(&mut self, version: u64, result: Result<(), String>) {
        let Ok(mut journal) = self.own_profile_journal() else {
            return;
        };
        match result {
            Ok(()) => {
                journal.sync.acknowledge(version);
                if journal
                    .upload
                    .as_ref()
                    .is_some_and(|upload| upload.version == version)
                {
                    journal.upload = None;
                }
                let _ = self.save_own_profile_journal(&journal);
            }
            Err(error) if error.contains("stale profile version") => {
                // Fetch the signed relay floor before rebasing. Never blindly increment
                // an old-key payload or mistake a lost acknowledgement for an edit.
                if let Some(me) = self.me.clone() {
                    self.net.profiles.need_profiles.insert(me);
                    self.net.profiles.own_refreshed = false;
                }
            }
            Err(error) => crate::net::tracing_like(&format!("profile upload refused: {error}")),
        }
    }
}
