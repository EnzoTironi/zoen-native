//! Independent local Agent enrollment. Pairing transfers public owner consent;
//! it never installs an owner device or imports an IdentityBox/account secret.
mod host;
pub use host::{AgentEnrollmentHostOffer, AgentEnrollmentHostProgress, AgentEnrollmentHostRequest};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use event_log::{agent_owner, Author};
use protocol::{Op, Reply};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use types::{AgentOwnerProof, IdentityKind};
use zeroize::{Zeroize, Zeroizing};

use crate::{api::invalid, Client, CoreError};

type R<T> = Result<T, CoreError>;
const PREFIX: &str = "zoen-agent-enroll:1:";
const CODE_LIMIT: usize = 2048;
const ENVELOPE_LIMIT: usize = 8192;
const LIFETIME_MS: u64 = 15 * 60 * 1000;
const RECEIPT_META: &str = "agent.enrollment.owner-receipt.v1";

fn malformed() -> CoreError {
    invalid("This Agent enrollment code is invalid or unsupported.")
}
fn changed() -> CoreError {
    invalid("The account, device or reviewed Agent enrollment changed. Review it again.")
}
fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn canonical_key(value: &str) -> bool {
    agent_owner::enrollment_offer_message(value, &"0".repeat(64)).is_some()
}
fn canonical_secret(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    hex::decode(value).ok()?.try_into().ok()
}
fn canonical_relay(value: &str) -> R<String> {
    crate::provider_auth::relay_base(&crate::net::http_base(value))
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BootstrapDefinition {
    version: u32,
    kind: String,
    persona: String,
    rules: Vec<String>,
    state: serde_json::Value,
    model_tier: String,
}
impl BootstrapDefinition {
    fn parse(value: &str) -> R<Self> {
        if value.len() > 1024 {
            return Err(malformed());
        }
        let definition: Self = serde_json::from_str(value).map_err(|_| malformed())?;
        if definition.version != 1
            || definition.kind != "user"
            || definition.model_tier != "cheap"
            || !definition
                .state
                .as_object()
                .is_some_and(|state| state.is_empty())
            || definition.persona.is_empty()
            || definition.persona.len() > 768
            || definition
                .persona
                .chars()
                .any(|c| c.is_control() && c != '\n')
            || definition.rules.len() > 8
            || definition.rules.iter().any(|rule| {
                rule.is_empty() || rule.len() > 256 || rule.chars().any(char::is_control)
            })
            || serde_json::to_string(&definition).map_err(|_| malformed())? != value
        {
            return Err(invalid("This enrollment supports a User v1 bootstrap with Cheap tier and empty state. Unsupported behavior is not substituted."));
        }
        Ok(definition)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Offer {
    schema: u8,
    agent_id: String,
    expected_owner_id: String,
    relay_origin: String,
    hpke_public: String,
    challenge_sha256: String,
    issued_ms: u64,
    expires_ms: u64,
    name: String,
    handle: String,
    definition_json: String,
}
impl Offer {
    fn validate(&self, now: u64) -> R<BootstrapDefinition> {
        if self.schema != 1
            || !canonical_key(&self.agent_id)
            || !canonical_key(&self.expected_owner_id)
            || self.agent_id == self.expected_owner_id
            || canonical_secret(&self.hpke_public).is_none_or(|public| {
                // Reuse the existing HPKE implementation's contributory-key
                // validation; never consent to a known low-order recipient.
                crate::link::seal(&public, &[0; 32], "agent-enrollment-key-check-v1", b"").is_none()
            })
            || canonical_secret(&self.challenge_sha256).is_none()
            || self.relay_origin.len() > 256
            || canonical_relay(&self.relay_origin)? != self.relay_origin
            || self.name.is_empty()
            || self.name.trim() != self.name
            || self.name.chars().count() > 64
            || self.name.chars().any(char::is_control)
            || protocol::normalize_handle(&self.handle).as_deref() != Some(self.handle.as_str())
            || self.issued_ms > now
            || self.expires_ms <= now
            || self
                .expires_ms
                .checked_sub(self.issued_ms)
                .is_none_or(|ttl| ttl == 0 || ttl > LIFETIME_MS)
        {
            return Err(malformed());
        }
        BootstrapDefinition::parse(&self.definition_json)
    }
    fn digest(&self) -> R<String> {
        Ok(sha(&serde_json::to_vec(&(
            "zoen.agent-enrollment.offer.v1",
            self.schema,
            &self.agent_id,
            &self.expected_owner_id,
            &self.relay_origin,
            &self.hpke_public,
            &self.challenge_sha256,
            self.issued_ms,
            self.expires_ms,
            &self.name,
            &self.handle,
            &self.definition_json,
        ))
        .map_err(|_| malformed())?))
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Code {
    offer: Offer,
    agent_signature: String,
    pairing_secret: String,
}
impl Drop for Code {
    fn drop(&mut self) {
        self.pairing_secret.zeroize();
    }
}

impl Code {
    fn encode(&self) -> R<String> {
        let value = format!(
            "{PREFIX}{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(self).map_err(|_| malformed())?)
        );
        if value.len() > CODE_LIMIT {
            return Err(invalid(
                "The Agent enrollment description is too long for this pairing code.",
            ));
        }
        Ok(value)
    }
    fn parse(value: &str, now: u64) -> R<Self> {
        if value.len() > CODE_LIMIT {
            return Err(malformed());
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(value.strip_prefix(PREFIX).ok_or_else(malformed)?)
            .map_err(|_| malformed())?;
        let code: Self = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
        code.offer.validate(now)?;
        let secret = Zeroizing::new(canonical_secret(&code.pairing_secret).ok_or_else(malformed)?);
        if sha(secret.as_slice()) != code.offer.challenge_sha256
            || !agent_owner::verify_enrollment_offer(
                &code.offer.agent_id,
                &code.offer.digest()?,
                &code.agent_signature,
            )
            || code.encode()? != value
        {
            return Err(malformed());
        }
        Ok(code)
    }
    fn check(&self) -> R<String> {
        let bytes = Sha256::digest(
            format!(
                "zoen-agent-enrollment-pairing-v1\0{}\0{}",
                self.offer.digest()?,
                self.agent_signature
            )
            .as_bytes(),
        );
        Ok(format!(
            "{:06}",
            u32::from_be_bytes(bytes[..4].try_into().unwrap()) % 1_000_000
        ))
    }
    fn box_context(&self) -> R<String> {
        Ok(format!(
            "agent-enrollment-owner-v1:{}",
            self.offer.digest()?
        ))
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerEnvelope {
    schema: u8,
    offer_digest: String,
    owner_id: String,
    agent_id: String,
    owner_proof: AgentOwnerProof,
    owner_offer_signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AgentEnrollmentPreviewDto {
    pub agent_id: String,
    pub offer_digest: String,
    pub name: String,
    pub handle: String,
    pub persona: String,
    pub rules: Vec<String>,
    pub pairing_check: String,
    pub expires_ms: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum AgentEnrollmentDeliveryOutcome {
    Sent,
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AgentEnrollmentDeliveryDto {
    pub receipt_id: String,
    pub agent_id: String,
    pub offer_digest: String,
    pub outcome: AgentEnrollmentDeliveryOutcome,
}
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AgentEnrollmentStatusDto {
    pub preview: AgentEnrollmentPreviewDto,
    pub delivery: AgentEnrollmentDeliveryDto,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerReceipt {
    owner: String,
    device: String,
    certificate_sha256: String,
    relay: String,
    status: AgentEnrollmentStatusDto,
}
struct OwnerContext {
    author: Author,
    relay: String,
    certificate_sha256: String,
}
impl OwnerReceipt {
    fn matches(&self, context: &OwnerContext) -> bool {
        self.status.preview.agent_id == self.status.delivery.agent_id
            && self.status.preview.offer_digest == self.status.delivery.offer_digest
            && canonical_secret(&self.status.preview.offer_digest).is_some()
            && self.status.delivery.receipt_id.len() <= 128
            && !self.status.delivery.receipt_id.is_empty()
            && self.owner == context.author.identity
            && self.device == context.author.key.id()
            && self.certificate_sha256 == context.certificate_sha256
            && self.relay == context.relay
    }
}
fn preview(code: &Code) -> R<AgentEnrollmentPreviewDto> {
    let definition = BootstrapDefinition::parse(&code.offer.definition_json)?;
    Ok(AgentEnrollmentPreviewDto {
        agent_id: code.offer.agent_id.clone(),
        offer_digest: code.offer.digest()?,
        name: code.offer.name.clone(),
        handle: code.offer.handle.clone(),
        persona: definition.persona,
        rules: definition.rules,
        pairing_check: code.check()?,
        expires_ms: code.offer.expires_ms,
    })
}
fn owner_context(e: &crate::Engine, identity: &str, device: &str) -> R<OwnerContext> {
    let account = e.account().ok_or_else(changed)?;
    let author = e.net.author.as_ref().ok_or_else(changed)?;
    if account.identity != identity
        || account.device != device
        || !account.registered
        || e.provider_setup_pending()
        || author.identity != identity
        || author.key.id() != device
        || author.device.as_deref() != Some(device)
        || author.cert.as_deref() != Some(account.cert.as_str())
        || e.my_profile()
            .is_none_or(|profile| profile.kind != IdentityKind::Person)
    {
        return Err(changed());
    }
    Ok(OwnerContext {
        author: author.clone(),
        relay: canonical_relay(&account.relay_url)?,
        certificate_sha256: sha(account.cert.as_bytes()),
    })
}

impl Client {
    fn enrollment_owner_context(&self, identity: &str, device: &str) -> R<OwnerContext> {
        owner_context(&self.lock(), identity, device)
    }
    fn enrollment_review(
        &self,
        value: &str,
        identity: &str,
        device: &str,
    ) -> R<(Code, OwnerContext)> {
        let context = self.enrollment_owner_context(identity, device)?;
        let code = Code::parse(
            value,
            crate::engine::now_ms()
                .try_into()
                .map_err(|_| malformed())?,
        )?;
        if code.offer.expected_owner_id != identity || code.offer.relay_origin != context.relay {
            return Err(changed());
        }
        Ok((code, context))
    }
    pub(crate) fn agent_enrollment_preview(
        &self,
        value: &str,
        identity: &str,
        device: &str,
    ) -> R<AgentEnrollmentPreviewDto> {
        let (code, _) = self.enrollment_review(value, identity, device)?;
        preview(&code)
    }
    pub(crate) fn agent_enrollment_status(
        &self,
        identity: &str,
        device: &str,
    ) -> R<Option<AgentEnrollmentStatusDto>> {
        let e = self.lock();
        let context = owner_context(&e, identity, device)?;
        let Some(value) = e.store.meta(RECEIPT_META)? else {
            return Ok(None);
        };
        if value.len() > ENVELOPE_LIMIT {
            return Err(changed());
        }
        let receipt: OwnerReceipt = serde_json::from_str(&value).map_err(|_| changed())?;
        Ok(receipt.matches(&context).then_some(receipt.status))
    }
    pub(crate) fn agent_enrollment_dismiss(
        &self,
        receipt_id: &str,
        digest: &str,
        identity: &str,
        device: &str,
    ) -> R<bool> {
        let e = self.lock();
        let context = owner_context(&e, identity, device)?;
        let Some(value) = e.store.meta(RECEIPT_META)? else {
            return Ok(false);
        };
        let receipt: OwnerReceipt = serde_json::from_str(&value).map_err(|_| changed())?;
        if !receipt.matches(&context) {
            return Err(changed());
        }
        if receipt.status.preview.offer_digest != digest
            || receipt.status.delivery.receipt_id != receipt_id
        {
            return Ok(false);
        }
        e.store.meta_delete(RECEIPT_META)?;
        Ok(true)
    }
    pub(crate) async fn agent_enrollment_approve(
        &self,
        value: &str,
        digest: &str,
        identity: &str,
        device: &str,
        confirmed_check: &str,
    ) -> R<AgentEnrollmentDeliveryDto> {
        let (code, context) = self.enrollment_review(value, identity, device)?;
        let review = preview(&code)?;
        if review.offer_digest != digest || review.pairing_check != confirmed_check {
            return Err(changed());
        }
        let (proof, signature) =
            agent_owner::authorize_enrollment(&context.author, &code.offer.agent_id, digest)
                .ok_or_else(changed)?;
        let envelope = OwnerEnvelope {
            schema: 1,
            offer_digest: digest.into(),
            owner_id: identity.into(),
            agent_id: code.offer.agent_id.clone(),
            owner_proof: proof,
            owner_offer_signature: signature,
        };
        let plain = serde_json::to_vec(&envelope).map_err(|_| malformed())?;
        let context_label = code.box_context()?;
        let secret = Zeroizing::new(canonical_secret(&code.pairing_secret).ok_or_else(malformed)?);
        let hpke = canonical_secret(&code.offer.hpke_public).ok_or_else(malformed)?;
        let sealed =
            crate::link::seal(&hpke, &secret, &context_label, &plain).ok_or_else(malformed)?;
        if sealed.len() > ENVELOPE_LIMIT {
            return Err(malformed());
        }
        let mut receipt = OwnerReceipt {
            owner: identity.into(),
            device: device.into(),
            certificate_sha256: context.certificate_sha256.clone(),
            relay: context.relay.clone(),
            status: AgentEnrollmentStatusDto {
                preview: review,
                delivery: AgentEnrollmentDeliveryDto {
                    receipt_id: types::new_id("enrollment"),
                    agent_id: code.offer.agent_id.clone(),
                    offer_digest: digest.into(),
                    outcome: AgentEnrollmentDeliveryOutcome::Unknown,
                },
            },
        };
        {
            let e = self.lock();
            let current = owner_context(&e, identity, device)?;
            if !receipt.matches(&current) {
                return Err(changed());
            }
            if let Some(value) = e.store.meta(RECEIPT_META)? {
                let held: OwnerReceipt = serde_json::from_str(&value).map_err(|_| changed())?;
                if held.matches(&current) && held.status.preview == receipt.status.preview {
                    return Ok(held.status.delivery);
                }
                return Err(invalid(
                    "Dismiss the previous Agent enrollment before connecting another host.",
                ));
            }
            let encoded = serde_json::to_string(&receipt).map_err(|_| malformed())?;
            e.store.set_meta(RECEIPT_META, &encoded)?;
            if e.store.meta(RECEIPT_META)?.as_deref() != Some(encoded.as_str()) {
                return Err(changed());
            }
        }
        let result = self
            .request(Op::DeliverLink {
                id: crate::link::box_id(&secret, &context_label),
                sealed,
                device: None,
            })
            .await;
        let e = self.lock();
        let current = owner_context(&e, identity, device)?;
        if !receipt.matches(&current) {
            return Err(changed());
        }
        let held = e
            .store
            .meta(RECEIPT_META)?
            .and_then(|value| serde_json::from_str::<OwnerReceipt>(&value).ok());
        if !held.as_ref().is_some_and(|held| {
            held.matches(&current)
                && held.status.preview == receipt.status.preview
                && held.status.delivery.receipt_id == receipt.status.delivery.receipt_id
        }) {
            return Err(changed());
        }
        if matches!(result, Ok(Reply::Done)) {
            receipt.status.delivery.outcome = AgentEnrollmentDeliveryOutcome::Sent;
            // Failure to retain an ACK leaves the pre-dispatch Unknown receipt.
            if e.store
                .set_meta(
                    RECEIPT_META,
                    &serde_json::to_string(&receipt).map_err(|_| malformed())?,
                )
                .is_err()
            {
                receipt.status.delivery.outcome = AgentEnrollmentDeliveryOutcome::Unknown;
            }
        }
        Ok(receipt.status.delivery)
    }
}
