//! Retained enrollment validation and canonical installation.
use super::*;

impl Pending {
    pub(super) fn binding(&self) -> R<Admission> {
        Ok(Admission {
            agent: self.code.offer.agent_id.clone(),
            device: Signer::from_secret(&self.device).id(),
            relay: self.code.offer.relay_origin.clone(),
            offer_digest: self.code.offer.digest()?,
            expires_ms: self.code.offer.expires_ms,
        })
    }
    pub(super) fn validate_guard(&self, engine: &crate::Engine) -> R<()> {
        let guard = Preparing::load(engine)?.ok_or_else(preserved)?;
        if guard.phase != Phase::Preparing || guard.binding != self.binding()? {
            return Err(preserved());
        }
        Ok(())
    }
    pub(super) fn validate(&self) -> R<()> {
        // Expired material is still loadable for exact recovery, never a new code
        // or a fresh registration. Structural validation uses its original issue time.
        self.code.offer.validate(self.code.offer.issued_ms)?;
        let bytes = self.code.encode()?;
        Code::parse(&bytes, self.code.offer.issued_ms)?;
        if self.version != 1
            || self.root == self.device
            || self.root == self.pickup
            || self.device == self.pickup
            || self.agreement == self.hpke
            || Signer::from_secret(&self.root).id() != self.code.offer.agent_id
            || AgreementKey::from_secret(self.hpke).public_hex() != self.code.offer.hpke_public
        {
            return Err(preserved());
        }
        if let Some(envelope) = &self.accepted {
            self.verify(envelope)?;
        }
        Ok(())
    }
    pub(super) fn load(vault: &Arc<dyn SecretVault>) -> R<Option<Self>> {
        let Some(bytes) = vault.load(PENDING.into()).map(Zeroizing::new) else {
            return Ok(None);
        };
        if bytes.len() > RECORD_LIMIT {
            return Err(preserved());
        }
        let pending: Self = serde_json::from_slice(&bytes).map_err(|_| preserved())?;
        pending.validate()?;
        Ok(Some(pending))
    }
    pub(super) fn save(&self, vault: &Arc<dyn SecretVault>) -> R<()> {
        self.validate()?;
        let bytes = Zeroizing::new(serde_json::to_vec(self).map_err(|_| preserved())?);
        if bytes.len() > RECORD_LIMIT {
            return Err(preserved());
        }
        let saved = vault.save(PENDING.into(), bytes.to_vec());
        let held = vault.load(PENDING.into()).map(Zeroizing::new);
        if !saved || held.as_deref().map(Vec::as_slice) != Some(bytes.as_slice()) {
            return Err(invalid("Could not retain this host's enrollment keys. Retry with the same protected setup."));
        }
        Ok(())
    }
    pub(super) fn transport(&self) -> crate::link::Pending {
        crate::link::Pending {
            device_secret: hex::encode(self.pickup),
            hpke_secret: hex::encode(self.hpke),
            hpke_public: self.code.offer.hpke_public.clone(),
            secret: self.code.pairing_secret.clone(),
            relay: self.code.offer.relay_origin.clone(),
        }
    }
    pub(super) fn verify(&self, envelope: &OwnerEnvelope) -> R<()> {
        if envelope.schema != 1
            || envelope.offer_digest != self.code.offer.digest()?
            || envelope.owner_id != self.code.offer.expected_owner_id
            || envelope.agent_id != self.code.offer.agent_id
            || !agent_owner::verify_enrollment(
                &envelope.owner_id,
                &envelope.agent_id,
                &envelope.offer_digest,
                &envelope.owner_proof,
                &envelope.owner_offer_signature,
            )
        {
            return Err(invalid(
                "The owner approval does not match this host's reviewed enrollment.",
            ));
        }
        Ok(())
    }
    pub(super) fn open(&self, sealed: &[u8]) -> R<OwnerEnvelope> {
        if sealed.len() > ENVELOPE_LIMIT {
            return Err(malformed());
        }
        let secret =
            Zeroizing::new(canonical_secret(&self.code.pairing_secret).ok_or_else(malformed)?);
        let plain = Zeroizing::new(
            crate::link::open(&self.hpke, &secret, &self.code.box_context()?, sealed)
                .ok_or_else(malformed)?,
        );
        if plain.len() > ENVELOPE_LIMIT {
            return Err(malformed());
        }
        // Deliberately no IdentityBox decoder: unknown/duplicate fields are rejected.
        let envelope: OwnerEnvelope = serde_json::from_slice(&plain).map_err(|_| malformed())?;
        if serde_json::to_vec(&envelope).map_err(|_| malformed())? != *plain {
            return Err(malformed());
        }
        self.verify(&envelope)?;
        Ok(envelope)
    }
    pub(super) fn offer(&self) -> R<AgentEnrollmentHostOffer> {
        self.code.offer.validate(now())?;
        Ok(AgentEnrollmentHostOffer {
            code: self.code.encode()?,
            pairing_check: self.code.check()?,
            offer_digest: self.code.offer.digest()?,
            expires_ms: self.code.offer.expires_ms,
        })
    }
    pub(super) fn validate_vault(&self, vault: &Arc<dyn SecretVault>) -> R<()> {
        if vault
            .load(crate::provider_auth::VAULT_SETUP.into())
            .map(Zeroizing::new)
            .is_some()
            || vault
                .load(crate::link::VAULT_PENDING.into())
                .map(Zeroizing::new)
                .is_some()
        {
            return Err(preserved());
        }
        for (key, secret) in [
            (ROOT, &self.root),
            (VAULT_IDENTITY, &self.root),
            (VAULT_DEVICE, &self.device),
            (VAULT_AGREEMENT, &self.agreement),
        ] {
            if vault
                .load(key.into())
                .map(Zeroizing::new)
                .as_ref()
                .is_some_and(|held| held.as_slice() != secret)
            {
                return Err(preserved());
            }
        }
        Ok(())
    }
    pub(super) fn validate_engine(&self, engine: &crate::Engine) -> R<()> {
        self.validate_guard(engine)?;
        if engine.provider_setup_pending()
            || engine.store.meta("account")?.is_some() && engine.account().is_none()
        {
            return Err(preserved());
        }
        let identity = &self.code.offer.agent_id;
        let device = Signer::from_secret(&self.device);
        let certificate =
            Signer::from_secret(&self.root).sign(&event_log::device_cert_message(&device.id()));
        if let Some(account) = engine.account() {
            let envelope = self.accepted.as_ref().ok_or_else(preserved)?;
            let profile = engine.my_profile().ok_or_else(preserved)?;
            if account.identity != *identity
                || account.device != device.id()
                || account.cert != certificate
                || canonical_relay(&account.relay_url)? != self.code.offer.relay_origin
                || profile.kind != IdentityKind::Agent
                || profile.owner.as_deref() != Some(envelope.owner_id.as_str())
                || profile.owner_proof.as_deref() != Some(&envelope.owner_proof)
                || !agent_owner::profile_authorized(&profile)
            {
                return Err(preserved());
            }
            let marker = engine.store.meta(ADMISSION)?.ok_or_else(preserved)?;
            if marker.len() > 2048 {
                return Err(preserved());
            }
            let admission: Admission = serde_json::from_str(&marker).map_err(|_| preserved())?;
            let profile_key = engine.store.profile_key(identity)?;
            if admission.agent != *identity
                || admission.device != device.id()
                || admission.relay != self.code.offer.relay_origin
                || admission.offer_digest != self.code.offer.digest()?
                || admission.expires_ms != self.code.offer.expires_ms
                || profile_key.as_ref().is_none_or(|row| {
                    event_log::profile::ProfileKey::from_hex(&row.key).is_none()
                        || row
                            .fields
                            .as_deref()
                            .and_then(event_log::profile::ProfileFields::from_bytes)
                            .is_none()
                })
            {
                return Err(preserved());
            }
        } else if engine.store.meta(ADMISSION)?.is_some() {
            return Err(preserved());
        }
        // Never repair or wipe a Person/foreign Agent or unrelated local content.
        // Installed own-account recovery permits its normal subsequently synced data.
        if engine.account().is_none()
            && (engine.me.is_some()
                || !engine.identities.is_empty()
                || !engine.logs.is_empty()
                || !engine.broken_spaces.is_empty()
                || !engine.store.synced_spaces()?.is_empty()
                || !engine.store.profile_keys()?.is_empty())
        {
            return Err(preserved());
        }
        Ok(())
    }
    pub(super) fn install(
        &self,
        engine: &mut crate::Engine,
        vault: &Arc<dyn SecretVault>,
    ) -> R<AgentEnrollmentHostProgress> {
        self.validate()?;
        self.validate_vault(vault)?;
        self.validate_engine(engine)?;
        let envelope = self.accepted.as_ref().ok_or_else(preserved)?;
        let installed = engine.account().is_some();
        if !installed {
            self.code.offer.validate(now())?;
        }
        // All complete secrets already exist in the verified pending record.
        for (key, secret) in [
            (ROOT, &self.root),
            (VAULT_IDENTITY, &self.root),
            (VAULT_DEVICE, &self.device),
            (VAULT_AGREEMENT, &self.agreement),
        ] {
            if vault.load(key.into()).map(Zeroizing::new).is_none()
                && !vault.save(key.into(), secret.to_vec())
            {
                return Err(preserved());
            }
            if vault
                .load(key.into())
                .map(Zeroizing::new)
                .as_deref()
                .map(Vec::as_slice)
                != Some(secret.as_slice())
            {
                return Err(preserved());
            }
        }
        if !installed {
            let admission = serde_json::to_string(&Admission {
                agent: self.code.offer.agent_id.clone(),
                device: Signer::from_secret(&self.device).id(),
                relay: self.code.offer.relay_origin.clone(),
                offer_digest: self.code.offer.digest()?,
                expires_ms: self.code.offer.expires_ms,
            })
            .map_err(|_| preserved())?;
            engine.install_prepared_account(
                PreparedAccount {
                    root: &self.root,
                    device: &self.device,
                    agreement: &self.agreement,
                    profile_key: None,
                    personal_agent_key: None,
                    name: &self.code.offer.name,
                    handle: &self.code.offer.handle,
                    relay: &self.code.offer.relay_origin,
                    agent: Some((&envelope.owner_id, &envelope.owner_proof)),
                },
                false,
                |engine| {
                    engine.store.set_meta(ADMISSION, &admission)?;
                    self.validate_engine(engine)
                },
            )?;
        }
        self.validate_engine(engine)?;
        if !engine.unlock(Some(self.device.to_vec()))? {
            return Err(preserved());
        }
        engine.unlock_profile(Some(self.agreement.to_vec()))?;
        let account = engine.account().ok_or_else(preserved)?;
        Ok(AgentEnrollmentHostProgress {
            agent_id: account.identity.clone(),
            owner_id: envelope.owner_id.clone(),
            relay_url: account.relay_url.clone(),
            offer_digest: self.code.offer.digest()?,
            definition_json: self.code.offer.definition_json.clone(),
            registered: account.registered,
        })
    }
}
