//! Public onboarding projection. Pairing codes and encrypted envelopes stay native.
use super::*;
use protocol::managed_agents::Record;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ManagedEnrollmentDto {
    pub operation_id: String,
    pub state: String,
    pub name: String,
    pub handle: String,
    pub agent_id: Option<String>,
    pub preview: Option<AgentEnrollmentPreviewDto>,
}

impl Client {
    pub(crate) async fn managed_enrollment_start_view(
        &self,
        name: &str,
        identity: &str,
        device: &str,
    ) -> R<ManagedEnrollmentDto> {
        let held = self.pending_managed_enrollment(identity, device)?;
        let (service, handle) = if let Some(held) = held {
            if held.name != name {
                return Err(changed());
            }
            (held.service.ok_or_else(changed)?, held.handle)
        } else {
            (
                self.managed_agent_service(identity, device)
                    .await?
                    .ok_or_else(|| invalid("Agent creation is temporarily unavailable."))?,
                new_handle()?,
            )
        };
        let record = self
            .prepare_managed_enrollment(&service, name, &handle, identity, device)
            .await?;
        self.managed_enrollment_view(record, identity, device)
    }

    pub(crate) async fn managed_enrollment_read_view(
        &self,
        identity: &str,
        device: &str,
    ) -> R<Option<ManagedEnrollmentDto>> {
        let Some(request) = self.pending_managed_enrollment(identity, device)? else {
            return Ok(None);
        };
        let record = if self.managed_enrollment_cancelling(identity, device)? {
            self.cancel_managed_enrollment(&request.operation, identity, device)
                .await?
        } else {
            // Exact replay also recovers a creation that never reached the relay.
            self.prepare_managed_enrollment(
                request.service.as_ref().ok_or_else(changed)?,
                &request.name,
                &request.handle,
                identity,
                device,
            )
            .await?
        };
        if record.onboarding_completed {
            self.retire_completed_managed_enrollment(&record, identity, device)?;
            return Ok(None);
        }
        self.managed_enrollment_view(record, identity, device)
            .map(Some)
    }

    pub(crate) async fn managed_enrollment_approve_view(
        &self,
        operation: &str,
        digest: &str,
        identity: &str,
        device: &str,
    ) -> R<ManagedEnrollmentDto> {
        let request = self
            .pending_managed_enrollment(identity, device)?
            .ok_or_else(changed)?;
        if request.operation != operation {
            return Err(changed());
        }
        let record = self.managed_enrollment_status(identity, device).await?;
        self.approve_managed_enrollment(&request, &record, digest, identity, device)
            .await?;
        self.managed_enrollment_view(record, identity, device)
    }

    pub(crate) async fn managed_enrollment_cancel_view(
        &self,
        operation: &str,
        identity: &str,
        device: &str,
    ) -> R<ManagedEnrollmentDto> {
        let record = self
            .cancel_managed_enrollment(operation, identity, device)
            .await?;
        self.managed_enrollment_view(record, identity, device)
    }

    pub(super) fn managed_enrollment_view(
        &self,
        record: Record,
        identity: &str,
        device: &str,
    ) -> R<ManagedEnrollmentDto> {
        let request = self
            .pending_managed_enrollment(identity, device)?
            .ok_or_else(changed)?;
        if !record.valid() || record.owner != identity || record.request.as_ref() != Some(&request)
        {
            return Err(changed());
        }
        let receipt = self.agent_enrollment_status(identity, device)?;
        let receipt = receipt.filter(|receipt| {
            receipt.preview.offer_digest == record.offer_digest
                && receipt.preview.agent_id == record.agent
        });
        let mut preview = None;
        let state = match record.state.as_str() {
            "registered" => "registered",
            "cancel_requested" => "cancelling",
            "cancelled" => "cancelled",
            "offered" | "expired" if receipt.is_some() => "installing",
            "offered"
                if record.expires_ms
                    > u64::try_from(crate::engine::now_ms()).map_err(|_| malformed())? =>
            {
                preview =
                    Some(self.managed_enrollment_preview(&request, &record, identity, device)?);
                "review"
            }
            "offered" | "expired" => "expired",
            "queued" => "queued",
            "rejected" => "rejected",
            _ => return Err(changed()),
        };
        if let Some(preview) = preview.as_mut() {
            preview.pairing_check.clear();
        }
        Ok(ManagedEnrollmentDto {
            operation_id: request.operation,
            state: state.into(),
            name: request.name,
            handle: request.handle,
            agent_id: (!record.agent.is_empty()).then_some(record.agent),
            preview,
        })
    }
}

fn new_handle() -> R<String> {
    // Directory handles are at most 24 characters. This is an identifier, not
    // an enrollment credential; the independent operation retains 256 bits.
    Ok(format!("a_{}", hex::encode(crate::link::random::<11>()?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn managed_handle_obeys_the_canonical_directory_contract() -> R<()> {
        for _ in 0..32 {
            let handle = new_handle()?;
            assert_eq!(handle.len(), 24);
            assert_eq!(
                protocol::normalize_handle(&handle).as_deref(),
                Some(handle.as_str())
            );
        }
        Ok(())
    }
}
