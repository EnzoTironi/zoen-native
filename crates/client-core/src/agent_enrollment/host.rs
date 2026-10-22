//! Host-only ports. The caller must hold its exclusive local home lock for the
//! entire Client/vault lifetime. The per-Client pickup gate is not a process lock.
use std::{sync::Arc, time::Duration};

use event_log::{profile::AgreementKey, Signer};
use zeroize::{Zeroize, Zeroizing};

use super::*;
use crate::{
    profile::VAULT_AGREEMENT,
    sync::{PreparedAccount, VAULT_DEVICE, VAULT_IDENTITY},
    SecretVault,
};

const PENDING: &str = "agent.enrollment.pending.v1";
const PREPARING: &str = "agent.enrollment.preparing.v1";
const ADMISSION: &str = "agent.enrollment.admission.v1";
const ROOT: &str = "agent_root";
const RECORD_LIMIT: usize = 16 * 1024;

/// Trusted local configuration. expected_owner_id must come from trusted setup,
/// before displaying a code; it is never inferred from a mailbox response.
pub struct AgentEnrollmentHostRequest {
    pub expected_owner_id: String,
    pub relay_url: String,
    pub name: String,
    pub handle: String,
    pub definition_json: String,
}

/// Contains a one-time pairing secret. Do not log or persist it outside the vault
/// or keep it in an owner's navigation/history state.
pub struct AgentEnrollmentHostOffer {
    pub code: String,
    pub pairing_check: String,
    pub offer_digest: String,
    pub expires_ms: u64,
}

/// Local installation evidence only. registered is the existing account flag;
/// preparing an account does not assert registration, presence or task readiness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEnrollmentHostProgress {
    pub agent_id: String,
    pub owner_id: String,
    pub relay_url: String,
    pub offer_digest: String,
    pub definition_json: String,
    pub registered: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    version: u8,
    root: [u8; 32],
    device: [u8; 32],
    agreement: [u8; 32],
    pickup: [u8; 32],
    hpke: [u8; 32],
    code: Code,
    accepted: Option<OwnerEnvelope>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.root.zeroize();
        self.device.zeroize();
        self.agreement.zeroize();
        self.pickup.zeroize();
        self.hpke.zeroize();
        self.code.pairing_secret.zeroize();
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Admission {
    agent: String,
    device: String,
    relay: String,
    offer_digest: String,
    expires_ms: u64,
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
enum Phase {
    Preparing,
    Cancelled,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preparing {
    phase: Phase,
    binding: Admission,
}
impl Preparing {
    fn load(engine: &crate::Engine) -> R<Option<Self>> {
        let Some(value) = engine.store.meta(PREPARING)? else {
            return Ok(None);
        };
        if value.len() > 2048 {
            return Err(preserved());
        }
        let guard: Self = serde_json::from_str(&value).map_err(|_| preserved())?;
        if !canonical_key(&guard.binding.agent)
            || !canonical_key(&guard.binding.device)
            || canonical_secret(&guard.binding.offer_digest).is_none()
            || canonical_relay(&guard.binding.relay)? != guard.binding.relay
        {
            return Err(preserved());
        }
        Ok(Some(guard))
    }
    fn save(&self, engine: &crate::Engine) -> R<()> {
        let encoded = serde_json::to_string(self).map_err(|_| preserved())?;
        engine.store.set_meta(PREPARING, &encoded)?;
        if engine.store.meta(PREPARING)?.as_deref() != Some(encoded.as_str()) {
            return Err(preserved());
        }
        Ok(())
    }
}
fn empty_host(engine: &crate::Engine) -> R<()> {
    if engine.account().is_some()
        || engine.provider_setup_pending()
        || engine.store.meta("account")?.is_some()
        || engine.store.meta(ADMISSION)?.is_some()
        || engine.me.is_some()
        || !engine.identities.is_empty()
        || !engine.logs.is_empty()
        || !engine.broken_spaces.is_empty()
        || !engine.store.synced_spaces()?.is_empty()
        || !engine.store.profile_keys()?.is_empty()
    {
        return Err(preserved());
    }
    Ok(())
}

fn now() -> u64 {
    crate::engine::now_ms().max(0) as u64
}
fn preserved() -> CoreError {
    invalid("This enrollment does not match this host. Existing keys and data were preserved.")
}

impl Pending {
    fn binding(&self) -> R<Admission> {
        Ok(Admission {
            agent: self.code.offer.agent_id.clone(),
            device: Signer::from_secret(&self.device).id(),
            relay: self.code.offer.relay_origin.clone(),
            offer_digest: self.code.offer.digest()?,
            expires_ms: self.code.offer.expires_ms,
        })
    }
    fn validate_guard(&self, engine: &crate::Engine) -> R<()> {
        let guard = Preparing::load(engine)?.ok_or_else(preserved)?;
        if guard.phase != Phase::Preparing || guard.binding != self.binding()? {
            return Err(preserved());
        }
        Ok(())
    }
    fn validate(&self) -> R<()> {
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
    fn load(vault: &Arc<dyn SecretVault>) -> R<Option<Self>> {
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
    fn save(&self, vault: &Arc<dyn SecretVault>) -> R<()> {
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
    fn transport(&self) -> crate::link::Pending {
        crate::link::Pending {
            device_secret: hex::encode(self.pickup),
            hpke_secret: hex::encode(self.hpke),
            hpke_public: self.code.offer.hpke_public.clone(),
            secret: self.code.pairing_secret.clone(),
            relay: self.code.offer.relay_origin.clone(),
        }
    }
    fn verify(&self, envelope: &OwnerEnvelope) -> R<()> {
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
    fn open(&self, sealed: &[u8]) -> R<OwnerEnvelope> {
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
    fn offer(&self) -> R<AgentEnrollmentHostOffer> {
        self.code.offer.validate(now())?;
        Ok(AgentEnrollmentHostOffer {
            code: self.code.encode()?,
            pairing_check: self.code.check()?,
            offer_digest: self.code.offer.digest()?,
            expires_ms: self.code.offer.expires_ms,
        })
    }
    fn validate_vault(&self, vault: &Arc<dyn SecretVault>) -> R<()> {
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
    fn validate_engine(&self, engine: &crate::Engine) -> R<()> {
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
    fn install(
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

impl Client {
    /// Idempotently prepare the same code/material. Accepted or expired setup
    /// requires explicit recovery/cancellation, never silently generates new keys.
    pub fn prepare_agent_enrollment(
        &self,
        request: AgentEnrollmentHostRequest,
        vault: Arc<dyn SecretVault>,
    ) -> R<AgentEnrollmentHostOffer> {
        self.link_pickup.with_request(|| {
            let engine = self.lock();
            let guard = Preparing::load(&engine)?;
            let held = Pending::load(&vault)?;
            if guard
                .as_ref()
                .is_some_and(|guard| guard.phase == Phase::Preparing)
            {
                let pending = held.as_ref().ok_or_else(preserved)?;
                pending.validate_vault(&vault)?;
                pending.validate_engine(&engine)?;
                let offer = &pending.code.offer;
                if pending.accepted.is_some()
                    || offer.expected_owner_id != request.expected_owner_id
                    || offer.relay_origin != canonical_relay(&request.relay_url)?
                    || offer.name != request.name
                    || offer.handle != request.handle
                    || offer.definition_json != request.definition_json
                {
                    return Err(preserved());
                }
                return pending.offer();
            }
            empty_host(&engine)?;
            if held
                .as_ref()
                .is_some_and(|pending| pending.accepted.is_some())
                || guard.is_none() && held.is_some()
            {
                return Err(preserved());
            }
            for key in [
                VAULT_IDENTITY,
                VAULT_DEVICE,
                VAULT_AGREEMENT,
                crate::provider_auth::VAULT_SETUP,
                crate::link::VAULT_PENDING,
            ] {
                if vault.load(key.into()).map(Zeroizing::new).is_some() {
                    return Err(preserved());
                }
            }
            let root = match vault.load(ROOT.into()).map(Zeroizing::new) {
                Some(bytes) => bytes.as_slice().try_into().map_err(|_| preserved())?,
                None if guard.is_none() => Signer::generate().secret(),
                None => return Err(preserved()),
            };
            if guard
                .as_ref()
                .is_some_and(|guard| Signer::from_secret(&root).id() != guard.binding.agent)
                || held.as_ref().is_some_and(|pending| pending.root != root)
            {
                return Err(preserved());
            }
            let (hpke_secret, hpke_public) = crate::link::key_pair();
            let hpke_secret = Zeroizing::new(hpke_secret);
            let secret = Zeroizing::new(crate::link::random::<32>());
            let issued_ms = now();
            let offer = Offer {
                schema: 1,
                agent_id: Signer::from_secret(&root).id(),
                expected_owner_id: request.expected_owner_id,
                relay_origin: canonical_relay(&request.relay_url)?,
                hpke_public: hex::encode(hpke_public),
                challenge_sha256: sha(secret.as_slice()),
                issued_ms,
                expires_ms: issued_ms.checked_add(LIFETIME_MS).ok_or_else(malformed)?,
                name: request.name,
                handle: request.handle,
                definition_json: request.definition_json,
            };
            offer.validate(issued_ms)?;
            let signature = Signer::from_secret(&root).sign(
                &agent_owner::enrollment_offer_message(&offer.agent_id, &offer.digest()?)
                    .ok_or_else(malformed)?,
            );
            let pending = Pending {
                version: 1,
                root,
                device: Signer::generate().secret(),
                agreement: AgreementKey::generate().secret(),
                pickup: Signer::generate().secret(),
                hpke: hpke_secret.as_slice().try_into().map_err(|_| malformed())?,
                code: Code {
                    offer,
                    agent_signature: signature,
                    pairing_secret: hex::encode(secret.as_slice()),
                },
                accepted: None,
            };
            let result = pending.offer()?;
            // Public evidence precedes the protected save: load(None) must never
            // make a known setup look like a brand-new host after an IO failure.
            Preparing {
                phase: Phase::Preparing,
                binding: pending.binding()?,
            }
            .save(&engine)?;
            pending.save(&vault)?;
            Ok(result)
        })
    }

    /// Cold recovery never re-fetches or reauthorizes consumed consent.
    pub fn resume_agent_enrollment(
        &self,
        vault: Arc<dyn SecretVault>,
    ) -> R<Option<AgentEnrollmentHostProgress>> {
        self.link_pickup.with_request(|| {
            let Some(pending) = Pending::load(&vault)? else {
                if Preparing::load(&self.lock())?
                    .is_some_and(|guard| guard.phase == Phase::Preparing)
                {
                    return Err(preserved());
                }
                return Ok(None);
            };
            pending.validate_guard(&self.lock())?;
            if pending.accepted.is_none() {
                return Ok(None);
            }
            pending.install(&mut self.lock(), &vault).map(Some)
        })
    }

    pub async fn wait_agent_enrollment(
        &self,
        vault: Arc<dyn SecretVault>,
        timeout_ms: u64,
    ) -> R<AgentEnrollmentHostProgress> {
        let pending = Pending::load(&vault)?.ok_or_else(preserved)?;
        let digest = pending.code.offer.digest()?;
        if pending.accepted.is_some() {
            return self.resume_agent_enrollment(vault)?.ok_or_else(preserved);
        }
        pending.code.offer.validate(now())?;
        let budget = Duration::from_millis(
            timeout_ms.min(pending.code.offer.expires_ms.saturating_sub(now())),
        );
        if budget.is_zero() {
            return Err(malformed());
        }
        let mut pickup = self.link_pickup.begin_agent(
            || {
                let held = Pending::load(&vault)?.ok_or_else(preserved)?;
                if held.code.offer.digest()? != digest || held.accepted.is_some() {
                    return Err(preserved());
                }
                held.validate_vault(&vault)?;
                held.validate_engine(&self.lock())?;
                held.code.offer.validate(now())?;
                Ok(held.transport())
            },
            &digest,
            budget,
            self.lang,
        )?;
        drop(pending);
        let sealed = match pickup.wait().await {
            Ok(sealed) => sealed,
            Err(error) => {
                drop(pickup);
                self.link_pickup.drain().await;
                return Err(error);
            }
        };
        let result = pickup.with_install(|transport, signer| {
            let mut pending = Pending::load(&vault)?.ok_or_else(preserved)?;
            if pending.accepted.is_some()
                || pending.code.offer.digest()? != digest
                || transport.hpke_public != pending.code.offer.hpke_public
                || transport.secret != pending.code.pairing_secret
                || signer.id() != Signer::from_secret(&pending.pickup).id()
            {
                return Err(preserved());
            }
            pending.code.offer.validate(now())?;
            let envelope = pending.open(&sealed)?;
            pending.accepted = Some(envelope);
            // Consume durably before any canonical key slot/account write.
            pending.save(&vault)?;
            pending.install(&mut self.lock(), &vault)
        });
        drop(pickup);
        self.link_pickup.drain().await;
        result
    }

    /// Only unaccepted, uninstalled offers can be discarded. Explicit cancellation
    /// preserves agent_root and never erases or revokes an Agent identity.
    pub async fn cancel_agent_enrollment(
        &self,
        vault: Arc<dyn SecretVault>,
        observed_offer_digest: String,
    ) -> R<bool> {
        let cancelled = self.link_pickup.cancel_request(|| {
            let Some(pending) = Pending::load(&vault)? else {
                if Preparing::load(&self.lock())?
                    .is_some_and(|guard| guard.phase == Phase::Preparing)
                {
                    return Err(preserved());
                }
                return Ok(false);
            };
            pending.validate_guard(&self.lock())?;
            if pending.code.offer.digest()? != observed_offer_digest {
                return Ok(false);
            }
            if pending.accepted.is_some() || self.lock().account().is_some() {
                return Err(preserved());
            }
            Ok(true)
        })?;
        if !cancelled {
            return Ok(false);
        }
        self.link_pickup.drain().await;
        self.link_pickup.with_request(|| {
            let Some(pending) = Pending::load(&vault)? else {
                if Preparing::load(&self.lock())?
                    .is_some_and(|guard| guard.phase == Phase::Preparing)
                {
                    return Err(preserved());
                }
                return Ok(false);
            };
            pending.validate_guard(&self.lock())?;
            if pending.code.offer.digest()? != observed_offer_digest {
                return Ok(false);
            }
            if pending.accepted.is_some() || self.lock().account().is_some() {
                return Err(preserved());
            }
            pending.validate_vault(&vault)?;
            // Retain the same Agent root before discarding only the unused code.
            if vault.load(ROOT.into()).map(Zeroizing::new).is_none()
                && !vault.save(ROOT.into(), pending.root.to_vec())
            {
                return Err(preserved());
            }
            if vault
                .load(ROOT.into())
                .map(Zeroizing::new)
                .as_deref()
                .map(Vec::as_slice)
                != Some(pending.root.as_slice())
            {
                return Err(preserved());
            }
            Preparing {
                phase: Phase::Cancelled,
                binding: pending.binding()?,
            }
            .save(&self.lock())?;
            vault.delete(PENDING.into());
            if vault.load(PENDING.into()).map(Zeroizing::new).is_some() {
                return Err(preserved());
            }
            Ok(true)
        })
    }
}

impl crate::Engine {
    /// Only a Ready from the authenticated relay reconciles a registration whose
    /// commit acknowledgment was lost. Mailbox disappearance never does so.
    pub(crate) fn agent_enrollment_registration_allowed(&self, relay_knows_me: bool) -> R<()> {
        let Some(value) = self.store.meta(ADMISSION)? else {
            return Ok(());
        };
        if value.len() > 2048 {
            return Err(preserved());
        }
        let admission: Admission = serde_json::from_str(&value).map_err(|_| preserved())?;
        let account = self.account().ok_or_else(preserved)?;
        if admission.agent != account.identity
            || admission.device != account.device
            || admission.relay != canonical_relay(&account.relay_url)?
            || canonical_secret(&admission.offer_digest).is_none()
        {
            return Err(preserved());
        }
        if !relay_knows_me && now() >= admission.expires_ms {
            return Err(invalid("This Agent enrollment expired before registration. Its retained identity requires explicit recovery."));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
