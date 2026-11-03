//! Browser profile publication using the existing journal.
use super::{
    profile, BTreeSet, Client, ClientFrame, DeviceSigned, EventBody, Op, ProfileKey, SealedProfile,
    Signer, Upload, AGREEMENT_LOOKUP, AGREEMENT_PUBLISH, BATCH,
};

impl<S: Signer> Client<S> {
    pub(in crate::client) fn profile_deadline(&self) -> Option<i64> {
        [
            self.profile_net.agreement_retry,
            self.profile_net.agreement_deadline,
            self.profile_net.profiles_retry,
            self.profile_net.profiles_deadline,
            self.profile_net.upload_deadline,
        ]
        .into_iter()
        .filter(|deadline| *deadline > self.view_now_ms)
        .min()
    }

    pub(in crate::client) fn profile_frames(
        &mut self,
        frames: &mut Vec<Vec<u8>>,
    ) -> Result<(), String> {
        if !self.synced || self.agreement_key().is_none() || self.my_profile_fields().is_err() {
            return Ok(());
        }
        if !self.profile_net.publish {
            let public = self
                .agreement_key()
                .ok_or("profile agreement key missing after validation")?
                .public_hex();
            if self.saved.profile.published_agreement != public {
                let signed = self.signed_profile_statement(&profile::agreement_message(
                    &self.account.identity,
                    &public,
                ))?;
                frames.push(
                    ClientFrame::Req {
                        id: AGREEMENT_PUBLISH,
                        op: Op::PublishAgreementKey { public, signed },
                    }
                    .encode(),
                );
                self.profile_net.publish = true;
            }
        }
        if self.profile_net.upload_deadline > 0
            && self.profile_net.upload_deadline <= self.view_now_ms
        {
            self.profile_net.upload = false;
            self.profile_net.upload_failed = false;
            self.profile_net.upload_request = None;
            self.profile_net.upload_version = None;
            self.profile_net.upload_deadline = 0;
        }
        let (key, version, fields) = self.my_profile_fields()?;
        if self.saved.profile.photo_uploads.is_empty()
            && !self.profile_net.upload
            && !self.profile_net.upload_failed
            && self.profile_net.fetched.contains(&self.account.identity)
            && self
                .saved
                .profile
                .own_sync
                .as_ref()
                .is_some_and(|sync| sync.pending == Some(version) && !sync.key_unavailable())
        {
            if self
                .saved
                .profile
                .upload
                .as_ref()
                .is_none_or(|upload| upload.version != version)
            {
                let ciphertext =
                    profile::seal_profile(&key, &self.account.identity, version, &fields)
                        .map_err(|error| error.to_string())?;
                let signed = self.signed_profile_statement(&profile::upload_message(
                    &self.account.identity,
                    version,
                    &ciphertext,
                ))?;
                self.saved.profile.upload = Some(Upload {
                    version,
                    ciphertext,
                    signature: signed.sig,
                });
            }
            let request = self.next_retained_request_id()?;
            let upload = self
                .saved
                .profile
                .upload
                .as_ref()
                .ok_or("profile upload missing after preparation")?;
            frames.push(
                ClientFrame::Req {
                    id: request,
                    op: Op::PutProfile {
                        profile: SealedProfile {
                            identity: self.account.identity.clone(),
                            version: upload.version,
                            ciphertext: upload.ciphertext.clone(),
                            signed: DeviceSigned {
                                device: self.account.device.clone(),
                                cert: self.account.cert.clone(),
                                sig: upload.signature.clone(),
                            },
                        },
                    },
                }
                .encode(),
            );
            self.profile_net.upload = true;
            self.profile_net.upload_version = Some(version);
            self.profile_net.upload_request = Some(request);
            self.profile_net.upload_deadline = self.view_now_ms.saturating_add(15_000);
        }
        self.share_profile_key()?;
        self.profile_lookup_frames(frames)
    }

    fn profile_lookup_frames(&mut self, frames: &mut Vec<Vec<u8>>) -> Result<(), String> {
        if !self.profile_net.agreement.is_empty()
            && self.profile_net.agreement_deadline <= self.view_now_ms
        {
            self.profile_net.agreement.clear();
        }
        if self.profile_net.agreement.is_empty()
            && self.profile_net.agreement_retry <= self.view_now_ms
        {
            let missing = self
                .logs
                .keys()
                .flat_map(|space| self.roster(space).into_keys())
                .filter(|id| {
                    id != &self.account.identity
                        && !self.saved.profile.agreement.contains_key(id)
                        && !self.profile_net.no_agreement.contains(id)
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .take(BATCH)
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                frames.push(
                    ClientFrame::Req {
                        id: AGREEMENT_LOOKUP,
                        op: Op::AgreementKeys {
                            ids: missing.clone(),
                        },
                    }
                    .encode(),
                );
                self.profile_net.agreement = missing;
                self.profile_net.agreement_deadline = self.view_now_ms.saturating_add(15_000);
            }
        }
        if !self.profile_net.profiles.is_empty()
            && self.profile_net.profiles_deadline <= self.view_now_ms
        {
            self.profile_net.profiles.clear();
            self.profile_net.profiles_request = None;
        }
        if self.profile_net.profiles.is_empty()
            && self.profile_net.profiles_retry <= self.view_now_ms
        {
            let missing = self
                .saved
                .profile
                .rows
                .keys()
                .filter(|id| !self.profile_net.fetched.contains(*id))
                .take(BATCH)
                .cloned()
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                let request = self.next_retained_request_id()?;
                self.profile_net.profiles_request = Some(request);
                frames.push(
                    ClientFrame::Req {
                        id: request,
                        op: Op::GetProfiles {
                            ids: missing.clone(),
                        },
                    }
                    .encode(),
                );
                for id in &missing {
                    self.profile_net.refetch.remove(id);
                }
                self.profile_net.profiles = missing;
                self.profile_net.profiles_deadline = self.view_now_ms.saturating_add(15_000);
            }
        }
        Ok(())
    }

    fn share_profile_key(&mut self) -> Result<(), String> {
        let row = self
            .saved
            .profile
            .rows
            .get(&self.account.identity)
            .ok_or("Perfil indisponível.")?;
        let key = ProfileKey::from_hex(&row.key).ok_or("Chave de perfil inválida.")?;
        let version = row.key_version;
        let me = &self.account.identity;
        let mut recipients = BTreeSet::new();
        let pending = self
            .saved
            .outbox
            .values()
            .filter(|pending| pending.failure.is_none())
            .filter_map(|pending| pending.envelope().ok())
            .filter_map(|envelope| {
                event_log::event_from_content(
                    envelope.content().to_vec(),
                    envelope.sig,
                    envelope.cert,
                    0,
                    String::new(),
                    String::new(),
                )
                .ok()
            })
            .collect::<Vec<_>>();
        for event in self
            .logs
            .values()
            .flat_map(event_log::SpaceLog::events)
            .chain(pending.iter())
        {
            if let EventBody::ProfileKeyShared { version: v, shares } = &event.body {
                if event.author == *me && *v == version {
                    recipients.extend(shares.iter().map(|share| share.to.clone()));
                }
            }
        }
        let mut bodies = Vec::new();
        for space in self.logs.keys() {
            let roster = self.roster(space);
            if !roster.contains_key(me) {
                continue;
            }
            let mut shares = Vec::new();
            for id in roster.keys() {
                if id == me
                    || recipients.contains(id)
                    || self.saved.profile.blocked.contains(id)
                    || self
                        .saved
                        .profiles
                        .get(id)
                        .is_some_and(|identity| identity.kind == types::IdentityKind::Agent)
                {
                    continue;
                }
                let Some(public) = self.saved.profile.agreement.get(id) else {
                    continue;
                };
                if let Some(sealed) = profile::seal_share(
                    &key,
                    version,
                    public,
                    &profile::share_context(space, me, id),
                ) {
                    shares.push(types::ProfileKeyShare {
                        to: id.clone(),
                        ephemeral: sealed.ephemeral,
                        sealed: sealed.sealed,
                    });
                    recipients.insert(id.clone());
                }
            }
            for shares in shares.chunks(200) {
                bodies.push((
                    space.clone(),
                    EventBody::ProfileKeyShared {
                        version,
                        shares: shares.to_vec(),
                    },
                ));
            }
        }
        for (space, body) in bodies {
            let pending = self.clear_event(&space, &body, self.view_now_ms)?;
            let id = pending.envelope()?.client_id().to_string();
            self.saved.outbox.insert(id, pending);
        }
        Ok(())
    }
}
