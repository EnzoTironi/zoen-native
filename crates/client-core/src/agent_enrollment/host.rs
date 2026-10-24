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

/// Trusted local configuration. `expected_owner_id` must come from trusted setup,
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
                None if guard.is_none() => Signer::generate()?.secret(),
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
                device: Signer::generate()?.secret(),
                agreement: AgreementKey::generate()?.secret(),
                pickup: Signer::generate()?.secret(),
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
    /// preserves `agent_root` and never erases or revokes an Agent identity.
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

mod pending;
