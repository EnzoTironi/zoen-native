//! Protected local profile intent and fixed-key initialization.

use super::{
    invalid, AgreementKey, CoreError, Engine, OwnJournal, OwnProfileSync, ProfileFields,
    ProfileKey, META_OWN_SYNC, META_UPLOADED, R,
};

impl Engine {
    pub(super) fn own_profile_journal(&self) -> R<OwnJournal> {
        if let Some(encoded) = self.store.meta(META_OWN_SYNC)? {
            return serde_json::from_str(&encoded).map_err(|_| invalid("profile journal".into()));
        }
        let me = self.me_id()?;
        let (_, _, seen, _) = self.my_fields(&me)?;
        let uploaded = self
            .store
            .meta(META_UPLOADED)?
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        Ok(OwnJournal {
            sync: OwnProfileSync::migrate(seen, uploaded),
            upload: None,
        })
    }

    pub(super) fn save_own_profile_journal(&self, journal: &OwnJournal) -> R<()> {
        self.store.set_meta(
            META_OWN_SYNC,
            &serde_json::to_string(journal).map_err(|_| invalid("profile journal".into()))?,
        )?;
        Ok(())
    }

    pub(super) fn next_profile_version(&self, seen: u64) -> R<u64> {
        self.own_profile_journal()?
            .sync
            .edit(seen)
            .ok_or_else(|| invalid("profile version".into()))
    }

    pub(super) fn mark_profile_intent(&self, version: u64) -> R<()> {
        let mut journal = self.own_profile_journal()?;
        journal.sync.pending = Some(version);
        journal.upload = None;
        self.save_own_profile_journal(&journal)
    }

    pub(crate) fn initialize_linked_profile_sync(&self) -> R<()> {
        self.save_own_profile_journal(&OwnJournal {
            sync: OwnProfileSync::default(),
            upload: None,
        })
    }
    pub(super) fn my_fields(&self, me: &str) -> R<(ProfileKey, u64, u64, ProfileFields)> {
        let row = self
            .store
            .profile_key(me)?
            .ok_or_else(|| CoreError::NotFound {
                what: "profile".into(),
            })?;
        let key = ProfileKey::from_hex(&row.key).ok_or_else(|| invalid("profile key".into()))?;
        let fields = row
            .fields
            .as_deref()
            .and_then(ProfileFields::from_bytes)
            .unwrap_or_default();
        Ok((key, row.key_version, row.seen_version, fields))
    }

    pub(crate) fn fields_of(&self, id: &str) -> Option<(u64, ProfileFields)> {
        let row = self.store.profile_key(id).ok()??;
        let fields = ProfileFields::from_bytes(row.fields.as_deref()?)?;
        Some((row.seen_version, fields))
    }

    /// A new account's profile key, first version and agreement key. Returns the agreement
    /// secret for the vault.
    #[cfg(test)]
    pub(crate) fn init_profile(&mut self, me: &str, name: &str) -> R<[u8; 32]> {
        self.init_profile_with_agreement(
            me,
            name,
            AgreementKey::generate()?,
            ProfileKey::generate()?,
        )
    }

    pub(crate) fn init_profile_with_agreement(
        &mut self,
        me: &str,
        name: &str,
        agreement: AgreementKey,
        key: ProfileKey,
    ) -> R<[u8; 32]> {
        self.store.put_profile_key(me, &key.to_hex(), 1)?;
        let fields = ProfileFields {
            name: name.to_string(),
            ..Default::default()
        };
        self.store.set_profile_fields(me, 1, &fields.to_bytes())?;
        self.mark_profile_intent(1)?;
        let secret = agreement.secret();
        self.net.profiles.agreement = Some(agreement);
        self.net.profiles.shares_dirty = true;
        self.net
            .mls
            .wake_recovery_many(self.net.synced.iter().cloned());
        Ok(secret)
    }

    /// Loads the agreement key at unlock. An account from before profiles gets its profile
    /// key here; `Some(secret)` means a new agreement key to keep in the vault.
    pub(crate) fn unlock_profile(&mut self, secret: Option<Vec<u8>>) -> R<Option<[u8; 32]>> {
        let me = self.me_id()?;
        if self.store.profile_key(&me)?.is_none() {
            let name = self.my_profile().map(|p| p.name).unwrap_or_default();
            let key = ProfileKey::generate()?;
            self.store.put_profile_key(&me, &key.to_hex(), 1)?;
            let fields = ProfileFields {
                name,
                ..Default::default()
            };
            self.store.set_profile_fields(&me, 1, &fields.to_bytes())?;
            self.mark_profile_intent(1)?;
        }
        self.net.profiles.shares_dirty = true;
        self.net
            .mls
            .wake_recovery_many(self.net.synced.iter().cloned());
        match secret.and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok()) {
            Some(s) => {
                self.net.profiles.agreement = Some(AgreementKey::from_secret(s));
                Ok(None)
            }
            None => {
                let a = AgreementKey::generate()?;
                let s = a.secret();
                self.net.profiles.agreement = Some(a);
                Ok(Some(s))
            }
        }
    }

    pub(super) fn write_my_fields(
        &mut self,
        me: &str,
        version: u64,
        fields: &ProfileFields,
    ) -> R<()> {
        self.store.raw_execute("SAVEPOINT profile_fields")?;
        let changed = (|| -> R<()> {
            self.store
                .set_profile_fields(me, version, &fields.to_bytes())?;
            self.mark_profile_intent(version)?;
            Ok(())
        })();
        if let Err(error) = changed {
            self.store.raw_execute("ROLLBACK TO profile_fields")?;
            self.store.raw_execute("RELEASE profile_fields")?;
            return Err(error);
        }
        self.store.raw_execute("RELEASE profile_fields")?;
        if let Some(mut ident) = self.identities.get(me).cloned() {
            ident.name = fields.name.clone();
            ident.bio = fields.bio.clone();
            self.store.put_identity(&ident, None)?;
            self.identities.insert(me.to_string(), ident);
            self.index_dirty = true;
        }
        Ok(())
    }
}
