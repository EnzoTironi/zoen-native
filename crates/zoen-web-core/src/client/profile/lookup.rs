//! Browser profile lookup using the existing journal.
use super::{
    profile, BTreeSet, Client, Observe, ProfileKey, Reply, SealedProfile, Signer, AGREEMENT_LOOKUP,
    AGREEMENT_PUBLISH,
};

impl<S: Signer> Client<S> {
    pub(in crate::client) fn profile_reply(
        &mut self,
        id: u64,
        result: &Result<Reply, String>,
    ) -> Result<bool, String> {
        match id {
            AGREEMENT_PUBLISH => {
                self.profile_net.publish = false;
                if matches!(result, Ok(Reply::Done)) {
                    self.saved.profile.published_agreement = self
                        .agreement_key()
                        .map(|key| key.public_hex())
                        .unwrap_or_default();
                } else {
                    self.profile_net.publish = true;
                }
            }
            request if self.profile_net.upload_request == Some(request) => {
                self.profile_net.upload_request = None;
                self.profile_net.upload_deadline = 0;
                self.profile_net.upload = false;
                let sent_version = self.profile_net.upload_version.take();
                if matches!(result, Ok(Reply::Done)) {
                    if let Some(version) = sent_version {
                        if let Some(sync) = self.saved.profile.own_sync.as_mut() {
                            sync.acknowledge(version);
                        }
                        if self
                            .saved
                            .profile
                            .upload
                            .as_ref()
                            .is_some_and(|upload| upload.version == version)
                        {
                            self.saved.profile.upload = None;
                        }
                    }
                } else if result
                    .as_ref()
                    .err()
                    .is_some_and(|error| error.contains("stale profile version"))
                {
                    self.profile_net.fetched.remove(&self.account.identity);
                    // Retain exact ciphertext until the authenticated lookup can
                    // distinguish lost ACK from another device's publication.
                } else {
                    self.profile_net.upload_failed = true;
                    self.profile_net.upload_deadline = self.view_now_ms.saturating_add(5000);
                }
            }
            AGREEMENT_LOOKUP => {
                let asked = std::mem::take(&mut self.profile_net.agreement);
                self.profile_net.agreement_deadline = 0;
                let Ok(Reply::AgreementKeys(records)) = result else {
                    self.profile_net.agreement_retry = self.view_now_ms.saturating_add(5000);
                    return Ok(true);
                };
                let mut found = BTreeSet::new();
                for record in records
                    .iter()
                    .filter(|record| asked.contains(&record.identity))
                {
                    if profile::verify_agreement(&record.identity, &record.public, &record.signed) {
                        self.saved
                            .profile
                            .agreement
                            .insert(record.identity.clone(), record.public.clone());
                        found.insert(record.identity.clone());
                    }
                }
                self.profile_net
                    .no_agreement
                    .extend(asked.into_iter().filter(|id| !found.contains(id)));
            }
            request if self.profile_net.profiles_request == Some(request) => {
                self.receive_profile_records(result)?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn receive_profile_records(&mut self, result: &Result<Reply, String>) -> Result<(), String> {
        self.profile_net.profiles_request = None;
        let asked = std::mem::take(&mut self.profile_net.profiles);
        self.profile_net.profiles_deadline = 0;
        let Ok(Reply::SealedProfiles(records)) = result else {
            self.profile_net.profiles_retry = self.view_now_ms.saturating_add(5000);
            return Ok(());
        };
        let own_asked = asked.contains(&self.account.identity);
        let own_present = records
            .iter()
            .any(|record| record.identity == self.account.identity);
        let own_sync = self.saved.profile.own_sync.clone();
        let mut own_verified = false;
        self.profile_net.fetched.extend(asked.iter().cloned());
        for id in &asked {
            if self.profile_net.refetch.remove(id) {
                self.profile_net.fetched.remove(id);
            }
        }
        for record in records
            .iter()
            .filter(|record| asked.contains(&record.identity))
        {
            if record.version == 0
                || record.ciphertext.len() > profile::MAX_CIPHERTEXT
                || !profile::verify_upload(
                    &record.identity,
                    record.version,
                    &record.ciphertext,
                    &record.signed,
                )
            {
                continue;
            }
            if record.identity == self.account.identity {
                own_verified |= own_sync
                    .as_ref()
                    .is_some_and(|sync| record.version >= sync.published);
                self.open_own_profile(record)?;
                continue;
            }
            let Some(row) = self.saved.profile.rows.get_mut(&record.identity) else {
                continue;
            };
            if record.version <= row.seen_version || record.version < row.key_version {
                continue;
            }
            let Some(key) = ProfileKey::from_hex(&row.key) else {
                continue;
            };
            if let Some(fields) =
                profile::open_profile(&key, &record.identity, record.version, &record.ciphertext)
            {
                row.seen_version = record.version;
                row.fields = Some(fields.to_bytes());
            }
        }
        if own_asked {
            let bootstrap_absent = !own_present
                && own_sync.as_ref().is_some_and(|sync| {
                    sync.published == 0 && sync.pending.is_some() && !sync.key_unavailable()
                });
            if !own_verified && !bootstrap_absent {
                self.profile_net.fetched.remove(&self.account.identity);
                self.profile_net.profiles_retry = self.view_now_ms.saturating_add(5000);
            }
        }
        self.overlay_saved_profiles();
        Ok(())
    }

    fn open_own_profile(&mut self, record: &SealedProfile) -> Result<(), String> {
        let (key, local_version, local_fields) = self.my_profile_fields()?;
        let row = self
            .saved
            .profile
            .rows
            .get(&self.account.identity)
            .ok_or("Perfil indisponível.")?;
        let mut sync = self
            .saved
            .profile
            .own_sync
            .clone()
            .ok_or("Estado do perfil indisponível.")?;
        if record.version < sync.published || record.version < row.key_version {
            return Ok(());
        }
        let Some(fields) =
            profile::open_profile(&key, &record.identity, record.version, &record.ciphertext)
        else {
            sync.cannot_open(record.version);
            self.saved.profile.own_sync = Some(sync);
            return Ok(());
        };
        let exact = self.saved.profile.upload.as_ref().is_some_and(|upload| {
            upload.version == record.version
                && upload.ciphertext == record.ciphertext
                && upload.signature == record.signed.sig
                && record.signed.device == self.account.device
                && record.signed.cert == self.account.cert
        });
        let outcome = sync
            .observe_opened(record.version, exact)
            .ok_or("O perfil atingiu o limite de versões.")?;
        if matches!(outcome, Observe::Acknowledged)
            && self.profile_net.upload_version == Some(record.version)
        {
            self.profile_net.upload = false;
            self.profile_net.upload_request = None;
            self.profile_net.upload_version = None;
            self.profile_net.upload_deadline = 0;
        }
        let changed = match outcome {
            Observe::Adopt => Some((record.version, fields)),
            Observe::Preserve { version } => Some((version.max(local_version), local_fields)),
            Observe::Acknowledged | Observe::Ignore => None,
        };
        if let Some((version, fields)) = changed {
            let row = self
                .saved
                .profile
                .rows
                .get_mut(&self.account.identity)
                .ok_or("profile row missing after sync validation")?;
            row.seen_version = version;
            row.fields = Some(fields.to_bytes());
        }
        if self
            .saved
            .profile
            .upload
            .as_ref()
            .is_some_and(|upload| Some(upload.version) != sync.pending)
        {
            self.saved.profile.upload = None;
        }
        self.saved.profile.own_sync = Some(sync);
        self.profile_net.upload_failed = false;
        Ok(())
    }
}
