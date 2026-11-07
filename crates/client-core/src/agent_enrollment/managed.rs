//! Confidential delivery of an existing enrollment offer. This transport does
//! not grant consent, certify the service, or assert executor readiness.
use protocol::managed_agents::{Record, Request};

use super::*;

const DOMAIN: &str = "zoen-managed-agent-offer-v1";

fn binding(owner: &str, request: &Request) -> R<[u8; 32]> {
    if !canonical_key(owner) || !request.valid() {
        return Err(malformed());
    }
    let service = request.service.as_ref().ok_or_else(malformed)?;
    let bytes = serde_json::to_vec(&(
        DOMAIN,
        owner,
        &request.operation,
        &service.id,
        &service.template_sha256,
        &request.name,
        &request.handle,
        &request.recipient_public_key,
    ))
    .map_err(|_| malformed())?;
    Ok(Sha256::digest(bytes).into())
}

fn matches_request(code: &Code, owner: &str, request: &Request, relay: &str) -> R<()> {
    let service = request.service.as_ref().ok_or_else(malformed)?;
    if code.offer.expected_owner_id != owner
        || code.offer.name != request.name
        || code.offer.handle != request.handle
        || code.offer.relay_origin != canonical_relay(relay)?
        || sha(code.offer.definition_json.as_bytes()) != service.template_sha256
    {
        return Err(changed());
    }
    Ok(())
}

impl AgentEnrollmentHostOffer {
    /// Consume the pairing code into ciphertext addressed to the captured owner
    /// agreement key. The caller retains the original result before publication.
    pub fn seal_managed(self, owner: &str, request: &Request, relay: &str) -> R<Record> {
        let value = Zeroizing::new(self.code);
        let nonce = binding(owner, request)?;
        let now = crate::engine::now_ms()
            .try_into()
            .map_err(|_| malformed())?;
        let code = Code::parse(&value, now)?;
        matches_request(&code, owner, request, relay)?;
        if self.offer_digest != code.offer.digest()?
            || self.expires_ms != code.offer.expires_ms
            || self.pairing_check != code.check()?
        {
            return Err(changed());
        }
        let recipient = canonical_secret(&request.recipient_public_key).ok_or_else(malformed)?;
        let sealed_offer = crate::link::seal(&recipient, &nonce, DOMAIN, value.as_bytes())
            .ok_or_else(malformed)?;
        let record = Record {
            request: Some(request.clone()),
            owner: owner.into(),
            state: "offered".into(),
            agent: code.offer.agent_id.clone(),
            offer_digest: self.offer_digest,
            sealed_offer,
            expires_ms: self.expires_ms,
            onboarding_completed: false,
        };
        if !record.valid() {
            return Err(malformed());
        }
        Ok(record)
    }
}

impl Client {
    fn managed_enrollment_code(
        &self,
        request: &Request,
        record: &Record,
        identity: &str,
        device: &str,
    ) -> R<Zeroizing<String>> {
        let engine = self.lock();
        let context = owner_context(&engine, identity, device)?;
        if !record.valid()
            || record.request.as_ref() != Some(request)
            || record.owner != identity
            || !matches!(record.state.as_str(), "offered" | "registered")
        {
            return Err(changed());
        }
        let agreement = engine.net.profiles.agreement.as_ref().ok_or_else(changed)?;
        if agreement.public_hex() != request.recipient_public_key {
            return Err(changed());
        }
        let secret = Zeroizing::new(agreement.secret());
        let nonce = binding(identity, request)?;
        let plain = Zeroizing::new(
            crate::link::open(secret.as_slice(), &nonce, DOMAIN, &record.sealed_offer)
                .ok_or_else(malformed)?,
        );
        let value = Zeroizing::new(
            std::str::from_utf8(&plain)
                .map_err(|_| malformed())?
                .to_owned(),
        );
        let now = crate::engine::now_ms()
            .try_into()
            .map_err(|_| malformed())?;
        let code = Code::parse(&value, now)?;
        matches_request(&code, identity, request, &context.relay)?;
        if code.offer.agent_id != record.agent
            || code.offer.digest()? != record.offer_digest
            || code.offer.expires_ms != record.expires_ms
        {
            return Err(changed());
        }
        Ok(value)
    }

    /// Returns only public review data; the pairing code remains in native code.
    pub fn managed_enrollment_preview(
        &self,
        captured_request: &Request,
        record: &Record,
        identity: &str,
        device: &str,
    ) -> R<AgentEnrollmentPreviewDto> {
        let value = self.managed_enrollment_code(captured_request, record, identity, device)?;
        self.agent_enrollment_preview(&value, identity, device)
    }

    /// Consent still uses the existing signed, durably retained owner receipt.
    pub async fn approve_managed_enrollment(
        &self,
        captured_request: &Request,
        record: &Record,
        reviewed_digest: &str,
        identity: &str,
        device: &str,
    ) -> R<AgentEnrollmentDeliveryDto> {
        let value = self.managed_enrollment_code(captured_request, record, identity, device)?;
        let review = self.agent_enrollment_preview(&value, identity, device)?;
        self.agent_enrollment_approve(
            &value,
            reviewed_digest,
            identity,
            device,
            &review.pairing_check,
        )
        .await
    }
}
