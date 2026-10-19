//! Local publication intent, shared by native SQLite and the Web retry journal.
//! This is bookkeeping only: callers must verify the device certificate, signature,
//! identity, and AEAD before calling `observe_opened`.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnProfileSync {
    /// Highest authenticated relay publication observed or acknowledged.
    pub published: u64,
    /// An explicit local edit/bootstrap version. Never inferred from received fields.
    pub pending: Option<u64>,
    /// A validly signed record used a key this device cannot open. Fail closed.
    #[serde(default)]
    pub blocked_at_version: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Observe {
    /// Install the authenticated remote fields at this version.
    Adopt,
    /// Keep the local pending fields; publish them at the returned version.
    Preserve {
        version: u64,
    },
    /// The relay has our exact retained ciphertext; pending intent is acknowledged.
    Acknowledged,
    Ignore,
}

impl OwnProfileSync {
    /// One-time conservative migration. Old journals cannot distinguish a linked
    /// seed from an unacknowledged edit; preserving it avoids discarding user intent.
    pub fn migrate(seen: u64, uploaded: u64) -> Self {
        Self {
            published: uploaded,
            pending: (seen > uploaded).then_some(seen),
            blocked_at_version: None,
        }
    }

    pub fn clean(version: u64) -> Self {
        Self {
            published: version,
            ..Self::default()
        }
    }

    pub fn edit(&mut self, seen: u64) -> Option<u64> {
        let next = seen
            .max(self.published)
            .max(self.blocked_at_version.unwrap_or(0))
            .checked_add(1)?;
        self.pending = Some(next);
        Some(next)
    }

    pub fn key_unavailable(&self) -> bool {
        self.blocked_at_version.is_some()
    }

    /// Called only after identity/certificate/signature verification. Keep this
    /// separate from adopted/acknowledged content: we cannot open this publication.
    /// No reply under the old key clears the latch. Secure key+policy recovery or
    /// a new authenticated link must replace the state explicitly.
    pub fn cannot_open(&mut self, version: u64) {
        self.blocked_at_version = Some(self.blocked_at_version.unwrap_or(0).max(version));
    }

    pub fn acknowledge(&mut self, version: u64) {
        self.published = self.published.max(version);
        if !self.key_unavailable() && self.pending == Some(version) {
            self.pending = None;
        }
    }

    pub fn observe_opened(&mut self, version: u64, exact_retained_upload: bool) -> Option<Observe> {
        if self.key_unavailable() || version < self.published {
            return Some(Observe::Ignore);
        }
        if exact_retained_upload {
            self.acknowledge(version);
            return Some(Observe::Acknowledged);
        }
        let previous = self.published;
        let outcome = match self.pending {
            Some(pending) => {
                let next = if pending <= version {
                    version.checked_add(1)?
                } else {
                    pending
                };
                Observe::Preserve { version: next }
            }
            None if version > previous => Observe::Adopt,
            None => Observe::Ignore,
        };
        // Compute overflow-prone decisions before modifying durable state.
        self.published = self.published.max(version);
        if let Observe::Preserve { version } = outcome {
            self.pending = Some(version);
        }
        Some(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_remote_profile_is_adopted_without_creating_intent() {
        let mut sync = OwnProfileSync::clean(1);
        assert_eq!(sync.observe_opened(2, false), Some(Observe::Adopt));
        assert_eq!(sync.pending, None);
        assert_eq!(sync.published, 2);
        assert_eq!(sync.observe_opened(2, false), Some(Observe::Ignore));
    }

    #[test]
    fn concurrent_remote_rebases_local_intent_and_late_ack_cannot_clear_it() {
        let mut sync = OwnProfileSync::clean(1);
        assert_eq!(sync.edit(1), Some(2));
        assert_eq!(
            sync.observe_opened(4, false),
            Some(Observe::Preserve { version: 5 })
        );
        sync.acknowledge(2);
        assert_eq!(sync.pending, Some(5));
        assert_eq!(sync.published, 4);
        let mut reload: OwnProfileSync =
            serde_json::from_str(&serde_json::to_string(&sync).unwrap()).unwrap();
        reload.acknowledge(5);
        assert_eq!(reload.pending, None);
        assert_eq!(reload.published, 5);
    }

    #[test]
    fn only_exact_lost_ack_ciphertext_can_clear_pending_intent() {
        let mut sync = OwnProfileSync::migrate(2, 1);
        let mut collision = sync.clone();
        assert_eq!(sync.observe_opened(2, true), Some(Observe::Acknowledged));
        assert_eq!(sync.pending, None);
        assert_eq!(
            collision.observe_opened(2, false),
            Some(Observe::Preserve { version: 3 })
        );
    }

    #[test]
    fn migration_preserves_unpublished_work_and_versions_do_not_wrap() {
        assert_eq!(OwnProfileSync::migrate(4, 2).pending, Some(4));
        let mut sync = OwnProfileSync::clean(u64::MAX);
        assert_eq!(sync.edit(1), None);
        assert_eq!(sync.pending, None);
    }
    #[test]
    fn unreadable_newer_key_latches_across_old_records_acks_and_reload() {
        let mut sync = OwnProfileSync::clean(1);
        sync.pending = Some(100);
        sync.cannot_open(8);
        sync.cannot_open(4);
        assert_eq!(sync.blocked_at_version, Some(8));
        assert_eq!(sync.observe_opened(2, false), Some(Observe::Ignore));
        sync.acknowledge(2);
        sync.acknowledge(100);
        assert!(sync.key_unavailable());
        assert_eq!(sync.pending, Some(100));
        assert_eq!(sync.observe_opened(101, true), Some(Observe::Ignore));
        assert!(sync.key_unavailable());
        let reload: OwnProfileSync =
            serde_json::from_str(&serde_json::to_string(&sync).unwrap()).unwrap();
        assert_eq!(reload.blocked_at_version, Some(8));
        assert_eq!(reload.pending, Some(100));
    }
}
