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
            // Even an Unknown approval may have been delivered. Preserve every
            // retained approval, including an unrelated manual connection.
            if engine.store.meta(RECEIPT_META)?.is_some()
                || !(record.state == "rejected"
                    || (record.state == "expired"
                        && record.expires_ms
                            <= u64::try_from(crate::engine::now_ms()).map_err(|_| changed())?))
            {
                return Err(changed());
            }
            engine.store.meta_delete(META)?;
            Ok(true)
        })
    }
}
