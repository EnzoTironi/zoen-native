//! Retire an unapproved, terminal offer; never cancel an in-flight installation.
use super::*;

impl Client {
    pub(crate) async fn discard_managed_enrollment(
        &self,
        operation: &str,
        identity: &str,
        device: &str,
    ) -> R<bool> {
        let (owner, channel) = self.device_directory_owner()?;
        let request = self.with_device_directory_completion(&owner, &channel, |engine| {
            let request = Intent::load(engine, identity, device)?
                .ok_or_else(changed)?
                .request;
            if request.operation != operation {
                return Err(changed());
            }
            Ok(request)
        })?;
        let response = channel
            .clone()
            .send(Op::Agent(AgentRequest::ManagedStatus {
                service: request.service.as_ref().ok_or_else(changed)?.id.clone(),
                operation: operation.into(),
            }))
            .await
            .map_err(invalid)?;
        self.with_device_directory_completion(&owner, &channel, |engine| {
            let record = managed_record(engine, &request, response, identity, device)?;
            retire(engine, &record, identity, device)
        })
    }
}

pub(super) fn retire(
    engine: &crate::Engine,
    record: &Record,
    identity: &str,
    device: &str,
) -> R<bool> {
    if !(matches!(record.state.as_str(), "rejected" | "cancelled")
        || (record.state == "expired"
            && record.expires_ms
                <= u64::try_from(crate::engine::now_ms()).map_err(|_| changed())?))
    {
        return Err(changed());
    }
    let held = engine.store.meta(RECEIPT_META)?;
    if let Some(value) = held.as_ref() {
        // An Unknown delivery is preserved unless the host explicitly proved
        // THIS exact offer was unused. A terminal timeout alone is not proof.
        if record.state != "cancelled" || value.len() > ENVELOPE_LIMIT {
            return Err(changed());
        }
        let receipt: OwnerReceipt = serde_json::from_str(value).map_err(|_| changed())?;
        let context = owner_context(engine, identity, device)?;
        if !receipt.matches(&context)
            || receipt.status.preview.agent_id != record.agent
            || receipt.status.preview.offer_digest != record.offer_digest
            || receipt.status.delivery.agent_id != record.agent
            || receipt.status.delivery.offer_digest != record.offer_digest
            || receipt.status.delivery.receipt_id.is_empty()
        {
            return Err(changed());
        }
    }
    if held.is_some() {
        engine.store.meta_delete(RECEIPT_META)?;
    }
    engine.store.meta_delete(META)?;
    Ok(true)
}
