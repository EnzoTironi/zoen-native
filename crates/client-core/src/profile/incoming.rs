//! Authenticated profile/agreement arrivals and own lookup settlement.

use super::*;

impl Engine {
    pub fn take_need_agreement(&self) -> Vec<IdentityId> {
        if !self.net.profiles.supported {
            return Vec::new();
        }
        self.net
            .profiles
            .need_agreement
            .iter()
            .take(BATCH)
            .cloned()
            .collect()
    }

    pub fn agreement_keys_arrived(
        &mut self,
        records: Vec<AgreementKeyRecord>,
        asked: &[IdentityId],
    ) -> bool {
        let mut found = HashSet::new();
        let mut failed = HashSet::new();
        for r in records {
            if asked.contains(&r.identity) && verify_agreement(&r.identity, &r.public, &r.signed) {
                if self
                    .store
                    .put_peer_agreement_key(&r.identity, &r.public)
                    .is_ok()
                {
                    failed.remove(&r.identity);
                    found.insert(r.identity);
                } else {
                    failed.insert(r.identity);
                }
            }
        }
        let p = &mut self.net.profiles;
        for id in asked {
            if failed.contains(id) {
                // A signed reply is not a persisted key or evidence of absence.
                p.need_agreement.insert(id.clone());
                p.no_agreement.remove(id);
                continue;
            }
            p.need_agreement.remove(id);
            if found.contains(id) {
                p.no_agreement.remove(id);
            } else {
                p.no_agreement.insert(id.clone());
            }
        }
        p.shares_dirty = true;
        self.net.mls.wake_recovery_many(
            self.net
                .synced
                .iter()
                .filter(|space| {
                    self.mls_roster(space)
                        .is_some_and(|roster| asked.iter().any(|id| roster.contains_key(id)))
                })
                .cloned(),
        );
        !asked
            .iter()
            .any(|id| self.net.profiles.need_agreement.contains(id))
    }

    pub fn take_need_profiles(&mut self) -> Vec<IdentityId> {
        if !self.net.profiles.supported {
            return Vec::new();
        }
        let asked = self
            .net
            .profiles
            .need_profiles
            .iter()
            .take(BATCH)
            .cloned()
            .collect::<Vec<_>>();
        for id in &asked {
            self.net.profiles.fetching.insert(id.clone());
            self.net.profiles.refetch.remove(id);
        }
        asked
    }

    /// Opens what the relay served. Returns whose profile changed on this device.
    pub fn sealed_profiles_arrived(
        &mut self,
        list: Vec<SealedProfile>,
        asked: &[IdentityId],
    ) -> Vec<IdentityId> {
        for id in asked {
            self.net.profiles.fetching.remove(id);
            if !self.net.profiles.refetch.remove(id) {
                self.net.profiles.need_profiles.remove(id);
            }
        }
        let mut changed = Vec::new();
        let me = self.me.clone();
        let own_asked = me.as_ref().is_some_and(|me| asked.contains(me));
        let own_present = me
            .as_ref()
            .is_some_and(|me| list.iter().any(|record| &record.identity == me));
        let own_sync = own_asked
            .then(|| self.own_profile_journal().ok().map(|journal| journal.sync))
            .flatten();
        let mut own_verified = false;
        if own_asked {
            self.net.profiles.own_refreshed = false;
        }
        for p in list {
            if !asked.contains(&p.identity)
                || p.version == 0
                || p.ciphertext.len() > MAX_CIPHERTEXT
                || !verify_upload(&p.identity, p.version, &p.ciphertext, &p.signed)
            {
                continue;
            }
            let Ok(Some(row)) = self.store.profile_key(&p.identity) else {
                continue;
            };
            if self.me.as_deref() == Some(p.identity.as_str()) {
                match self.open_own_profile(&p) {
                    Ok(adopted) => {
                        own_verified |= own_sync
                            .as_ref()
                            .is_some_and(|sync| p.version >= sync.published);
                        if adopted {
                            changed.push(p.identity);
                        }
                    }
                    Err(_) => {
                        self.net.profiles.need_profiles.insert(p.identity);
                    }
                }
                continue;
            }
            if p.version <= row.seen_version || p.version < row.key_version {
                continue;
            }
            let Some(key) = ProfileKey::from_hex(&row.key) else {
                continue;
            };
            let Some(fields) = open_profile(&key, &p.identity, p.version, &p.ciphertext) else {
                continue;
            };
            if self
                .store
                .set_profile_fields(&p.identity, p.version, &fields.to_bytes())
                .is_ok()
                && self.apply_fields(&p.identity).is_ok()
            {
                changed.push(p.identity);
            }
        }
        if own_asked {
            let me = me.expect("own profile was requested");
            let bootstrap_absent = !own_present
                && own_sync.as_ref().is_some_and(|sync| {
                    sync.published == 0 && sync.pending.is_some() && !sync.key_unavailable()
                });
            let complete = (own_verified || bootstrap_absent)
                && !self.net.profiles.need_profiles.contains(&me);
            self.net.profiles.own_refreshed = complete;
            if !complete {
                self.net.profiles.need_profiles.insert(me);
            }
        }
        changed
    }

    /// Invalid/stale own records and a missing known publication remain work.
    /// The network layer uses this to apply its ordinary bounded retry deadline.
    pub fn profiles_answer_complete(&self, asked: &[IdentityId]) -> bool {
        !asked
            .iter()
            .any(|id| self.net.profiles.need_profiles.contains(id))
    }

    pub(super) fn open_own_profile(&mut self, profile: &SealedProfile) -> R<bool> {
        let me = self.me_id()?;
        let (key, key_version, local_version, local_fields) = self.my_fields(&me)?;
        let mut journal = self.own_profile_journal()?;
        if profile.version < journal.sync.published || profile.version < key_version {
            return Ok(false);
        }
        let Some(fields) = open_profile(&key, &me, profile.version, &profile.ciphertext) else {
            journal.sync.cannot_open(profile.version);
            self.save_own_profile_journal(&journal)?;
            return Ok(false);
        };
        let exact = journal
            .upload
            .as_ref()
            .is_some_and(|upload| upload.sealed(&me) == *profile);
        let outcome = journal
            .sync
            .observe_opened(profile.version, exact)
            .ok_or_else(|| invalid("profile version".into()))?;
        let (version, adopted) = match outcome {
            Observe::Adopt => (profile.version, Some(fields)),
            Observe::Preserve { version } => (version.max(local_version), Some(local_fields)),
            Observe::Acknowledged | Observe::Ignore => (local_version, None),
        };
        // Old in-flight ACKs are version-scoped; only the current pending intent
        // may clear itself. Invalidate a ciphertext only when its version moved.
        if journal
            .upload
            .as_ref()
            .is_some_and(|upload| Some(upload.version) != journal.sync.pending)
        {
            journal.upload = None;
        }
        self.store.raw_execute("SAVEPOINT own_profile_receive")?;
        let applied = (|| -> R<()> {
            if let Some(fields) = &adopted {
                self.store
                    .set_profile_fields(&me, version, &fields.to_bytes())?;
            }
            self.save_own_profile_journal(&journal)?;
            Ok(())
        })();
        if let Err(error) = applied {
            self.store.raw_execute("ROLLBACK TO own_profile_receive")?;
            self.store.raw_execute("RELEASE own_profile_receive")?;
            return Err(error);
        }
        self.store.raw_execute("RELEASE own_profile_receive")?;
        if adopted.is_some() {
            self.apply_fields(&me)?;
        }
        Ok(matches!(outcome, Observe::Adopt))
    }

    /// The relay says someone's profile moved on.
    pub fn profile_changed(&mut self, id: &str) {
        if matches!(self.store.profile_key(id), Ok(Some(_))) {
            if self.net.profiles.fetching.contains(id) {
                self.net.profiles.refetch.insert(id.to_string());
            }
            self.net.profiles.need_profiles.insert(id.to_string());
            if self.me.as_deref() == Some(id) {
                self.net.profiles.own_refreshed = false;
            }
        }
    }
}

#[cfg(test)]
mod receipt_tests;
