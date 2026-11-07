//! A retained local disposition fences signing before dispatching cancellation.
use super::*;
impl Client {
    pub(crate) fn managed_enrollment_cancelling(&self, identity: &str, device: &str) -> R<bool> {
        Ok(Intent::load(&self.lock(), identity, device)?
            .is_some_and(|intent| intent.cancel_requested))
    }
    pub(crate) async fn cancel_managed_enrollment(
        &self,
        operation: &str,
        identity: &str,
        device: &str,
    ) -> R<Record> {
        let (owner, channel) = self.device_directory_owner()?;
        let request = self.with_device_directory_completion(&owner, &channel, |engine| {
            let mut intent = Intent::load(engine, identity, device)?.ok_or_else(changed)?;
            if intent.request.operation != operation || engine.store.meta(RECEIPT_META)?.is_some() {
                return Err(changed());
            }
            intent.cancel_requested = true;
            let value = serde_json::to_string(&intent).map_err(|_| changed())?;
            engine.store.set_meta(META, &value)?;
            if engine.store.meta(META)?.as_deref() != Some(value.as_str()) {
                return Err(changed());
            }
            Ok(intent.request)
        })?;
        let response = channel
            .clone()
            .send(Op::Agent(AgentRequest::CancelManaged(request.clone())))
            .await
            .map_err(invalid)?;
        self.with_device_directory_completion(&owner, &channel, |engine| {
            managed_record(engine, &request, response, identity, device)
        })
    }
}
