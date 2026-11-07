//! Service-side cancellation under the caller's exclusive home lock. Existing
//! accepted consent is recovered, never interpreted as a cancelled identity.
use super::*;
use protocol::managed_agents::Record;
const MARKER: &str = "agent.enrollment.managed-cancel.v1";

impl Client {
    /// True only for a durably unused host. False means consent was already
    /// consumed; the caller must resume registration through the normal path.
    fn unused_managed_agent_offer(
        &self,
        record: &Record,
        relay: &str,
        vault: Arc<dyn SecretVault>,
    ) -> R<bool> {
        self.link_pickup.with_request(|| {
            if !record.valid() || record.state != "cancel_requested" {
                return Err(preserved());
            }
            let request = record.request.as_ref().ok_or_else(preserved)?;
            let context =
                sha(
                    &serde_json::to_vec(&(&record.owner, request, canonical_relay(relay)?))
                        .map_err(|_| preserved())?,
                );
            let engine = self.lock();
            let pending = Pending::load(&vault)?;
            let guard = Preparing::load(&engine)?;
            if let Some(held) = engine.store.meta(MARKER)? {
                if held != context {
                    return Err(preserved());
                }
            }
            if let Some(pending) = &pending {
                bound_offer(pending, record, relay)?;
                pending.validate_vault(&vault)?;
                let guard = guard.as_ref().ok_or_else(preserved)?;
                if guard.binding != pending.binding()? {
                    return Err(preserved());
                }
                if guard.phase == Phase::Preparing {
                    pending.validate_engine(&engine)?;
                    // Accepted consent is distinguished by the caller before
                    // invoking the existing cancellation port.
                    return Ok(false);
                }
                if pending.accepted.is_some() {
                    return Err(preserved());
                }
            }
            empty_host(&engine)?;
            if let Some(guard) = &guard {
                if guard.phase != Phase::Cancelled
                    || (!record.agent.is_empty()
                        && (guard.binding.agent != record.agent
                            || guard.binding.offer_digest != record.offer_digest))
                    || guard.binding.relay != canonical_relay(relay)?
                    || vault
                        .load(ROOT.into())
                        .map(Zeroizing::new)
                        .as_deref()
                        .and_then(|root| {
                            <&[u8; 32]>::try_from(root.as_slice())
                                .map(|bytes| Signer::from_secret(bytes).id())
                                .ok()
                        })
                        .as_deref()
                        != Some(guard.binding.agent.as_str())
                {
                    return Err(preserved());
                }
                if engine.store.meta(MARKER)?.as_deref() != Some(context.as_str()) {
                    return Err(preserved());
                }
                // Complete a crash between the cancelled guard and vault deletion.
                vault.delete(PENDING.into());
                if vault.load(PENDING.into()).is_some() {
                    return Err(preserved());
                }
            } else {
                for key in [
                    ROOT,
                    VAULT_IDENTITY,
                    VAULT_DEVICE,
                    VAULT_AGREEMENT,
                    crate::provider_auth::VAULT_SETUP,
                    crate::link::VAULT_PENDING,
                ] {
                    if vault.load(key.into()).is_some() {
                        return Err(preserved());
                    }
                }
            }
            engine.store.set_meta(MARKER, &context)?;
            if engine.store.meta(MARKER)?.as_deref() != Some(context.as_str()) {
                return Err(preserved());
            }
            Ok(true)
        })
    }

    pub async fn cancel_managed_agent_offer(
        &self,
        record: &Record,
        relay: &str,
        vault: Arc<dyn SecretVault>,
    ) -> R<bool> {
        if self.unused_managed_agent_offer(record, relay, vault.clone())? {
            return Ok(true);
        }
        let digest = self.link_pickup.with_request(|| {
            let engine = self.lock();
            let pending = Pending::load(&vault)?.ok_or_else(preserved)?;
            pending.validate_guard(&engine)?;
            pending.validate_engine(&engine)?;
            if pending.accepted.is_some() {
                return Ok(None);
            }
            let request = record.request.as_ref().ok_or_else(preserved)?;
            let context =
                sha(
                    &serde_json::to_vec(&(&record.owner, request, canonical_relay(relay)?))
                        .map_err(|_| preserved())?,
                );
            engine.store.set_meta(MARKER, &context)?;
            if engine.store.meta(MARKER)?.as_deref() != Some(context.as_str()) {
                return Err(preserved());
            }
            pending.code.offer.digest().map(Some)
        })?;
        let Some(digest) = digest else {
            return Ok(false);
        };
        self.cancel_agent_enrollment(vault, digest).await
    }
}

fn bound_offer(pending: &Pending, record: &Record, relay: &str) -> R<()> {
    let request = record.request.as_ref().ok_or_else(preserved)?;
    let offer = &pending.code.offer;
    if offer.expected_owner_id != record.owner
        || offer.name != request.name
        || offer.handle != request.handle
        || offer.relay_origin != canonical_relay(relay)?
        || request
            .service
            .as_ref()
            .is_none_or(|service| service.template_sha256 != sha(offer.definition_json.as_bytes()))
        || (!record.agent.is_empty()
            && (record.agent != offer.agent_id || record.offer_digest != offer.digest()?))
    {
        return Err(preserved());
    }
    Ok(())
}
