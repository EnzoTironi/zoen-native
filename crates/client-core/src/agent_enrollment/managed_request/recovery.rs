//! Recover the immutable relay original through the existing linked-owner keys.
use super::*;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ManagedRecoveryDto {
    pub operation_id: String,
    pub name: String,
    pub state: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ManagedRecoveryPageDto {
    pub requests: Vec<ManagedRecoveryDto>,
    pub next_operation: Option<String>,
}

fn bound(engine: &crate::Engine, record: &Record, identity: &str, device: &str) -> R<()> {
    owner_context(engine, identity, device)?;
    let request = record.request.as_ref().ok_or_else(changed)?;
    if !record.valid()
        || record.owner != identity
        || engine
            .net
            .profiles
            .agreement
            .as_ref()
            .ok_or_else(changed)?
            .public_hex()
            != request.recipient_public_key
    {
        return Err(changed());
    }
    Ok(())
}
impl Client {
    pub(crate) async fn discover_managed_enrollments(
        &self,
        after: &str,
        identity: &str,
        device: &str,
    ) -> R<ManagedRecoveryPageDto> {
        if !after.is_empty() && !protocol::managed_agents::digest(after) {
            return Err(changed());
        }
        let (owner, channel) = self.device_directory_owner()?;
        self.with_device_directory_completion(&owner, &channel, |engine| {
            owner_context(engine, identity, device)?;
            Ok(())
        })?;
        let reply = channel
            .clone()
            .send(Op::Agent(AgentRequest::DiscoverManaged {
                after: after.into(),
            }))
            .await
            .map_err(invalid)?;
        self.with_device_directory_completion(&owner, &channel, |engine| {
            owner_context(engine, identity, device)?;
            let Reply::ManagedAgents(page) = reply else {
                return Err(changed());
            };
            if !page.valid() {
                return Err(changed());
            }
            let mut requests = Vec::with_capacity(page.records.len());
            for record in page.records {
                bound(engine, &record, identity, device)?;
                let request = record.request.ok_or_else(changed)?;
                if request.operation.as_str() <= after {
                    return Err(changed());
                }
                requests.push(ManagedRecoveryDto {
                    operation_id: request.operation,
                    name: request.name,
                    state: record.state,
                });
            }
            Ok(ManagedRecoveryPageDto {
                requests,
                next_operation: (!page.next_operation.is_empty()).then_some(page.next_operation),
            })
        })
    }

    pub(crate) async fn recover_managed_enrollment_view(
        &self,
        operation: &str,
        identity: &str,
        device: &str,
    ) -> R<super::super::ManagedEnrollmentDto> {
        if !protocol::managed_agents::digest(operation) {
            return Err(changed());
        }
        let (owner, channel) = self.device_directory_owner()?;
        self.with_device_directory_completion(&owner, &channel, |engine| {
            if Intent::load(engine, identity, device)?
                .is_some_and(|held| held.request.operation != operation)
            {
                return Err(changed());
            }
            Ok(())
        })?;
        let reply = channel
            .clone()
            .send(Op::Agent(AgentRequest::RecoverManaged {
                operation: operation.into(),
            }))
            .await
            .map_err(invalid)?;
        let record = self.with_device_directory_completion(&owner, &channel, |engine| {
            let Reply::ManagedAgent(record) = reply else {
                return Err(changed());
            };
            bound(engine, &record, identity, device)?;
            let request = record.request.as_ref().ok_or_else(changed)?;
            if request.operation != operation {
                return Err(changed());
            }
            if let Some(held) = Intent::load(engine, identity, device)? {
                if held.request != *request {
                    return Err(changed());
                }
            } else {
                // Never erase an unrelated/ambiguous signed consent to adopt a request.
                if engine.store.meta(RECEIPT_META)?.is_some() {
                    return Err(changed());
                }
                let context = owner_context(engine, identity, device)?;
                let intent = Intent {
                    version: 1,
                    owner: identity.into(),
                    device: device.into(),
                    certificate_sha256: context.certificate_sha256,
                    relay: context.relay,
                    request: request.clone(),
                    cancel_requested: record.state == "cancel_requested",
                };
                let value = serde_json::to_string(&intent).map_err(|_| changed())?;
                engine.store.set_meta(META, &value)?;
                if engine.store.meta(META)?.as_deref() != Some(value.as_str()) {
                    return Err(changed());
                }
            }
            Ok(record)
        })?;
        self.managed_enrollment_view(record, identity, device)
    }
}
