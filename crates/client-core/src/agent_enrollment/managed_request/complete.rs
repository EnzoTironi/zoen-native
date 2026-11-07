//! Retire only the completed local intent, never the agent or its permissions.
use super::*;
const DONE: &str = "agent.enrollment.managed-completed.v1";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Completed {
    owner: String,
    device: String,
    certificate: String,
    relay: String,
    operation: String,
    agent: String,
}
impl Completed {
    fn matches(&self, context: &OwnerContext, operation: &str, agent: &str) -> bool {
        self.owner == context.author.identity
            && self.device == context.author.key.id()
            && self.certificate == context.certificate_sha256
            && self.relay == context.relay
            && self.operation == operation
            && self.agent == agent
    }
}

impl Client {
    /// Confirm the exact canonical installation before permitting another local
    /// enrollment. An ACK loss replays the same completion, not another agent.
    pub async fn complete_managed_enrollment(
        &self,
        operation: &str,
        agent: &str,
        identity: &str,
        device: &str,
    ) -> R<bool> {
        if !protocol::managed_agents::digest(operation) || !canonical_key(agent) {
            return Err(changed());
        }
        let (owner, channel) = self.device_directory_owner()?;
        let pending = self.with_device_directory_completion(&owner, &channel, |engine| {
            if let Some(intent) = Intent::load(engine, identity, device)? {
                if intent.request.operation != operation {
                    return Err(changed());
                }
                return Ok(Some(intent.request));
            }
            let context = owner_context(engine, identity, device)?;
            let value = engine.store.meta(DONE)?.ok_or_else(changed)?;
            if value.len() > 2048 {
                return Err(changed());
            }
            let completed: Completed = serde_json::from_str(&value).map_err(|_| changed())?;
            if !completed.matches(&context, operation, agent) {
                return Err(changed());
            }
            Ok(None)
        })?;
        let Some(request) = pending else {
            return Ok(true);
        };
        let response = channel
            .clone()
            .send(Op::Agent(AgentRequest::CompleteManaged {
                request: request.clone(),
                agent: agent.into(),
            }))
            .await
            .map_err(invalid)?;
        self.with_device_directory_completion(&owner, &channel, |engine| {
            let record = managed_record(engine, &request, response, identity, device)?;
            complete(engine, &record, operation, agent, identity, device)
        })
    }
}

pub(super) fn complete(
    engine: &crate::Engine,
    record: &Record,
    operation: &str,
    agent: &str,
    identity: &str,
    device: &str,
) -> R<bool> {
    if !record.onboarding_completed
        || record.state != "registered"
        || record.agent != agent
        || record
            .request
            .as_ref()
            .is_none_or(|request| request.operation != operation)
    {
        return Err(changed());
    }
    let context = owner_context(engine, identity, device)?;
    if let Some(value) = engine.store.meta(RECEIPT_META)? {
        if value.len() > ENVELOPE_LIMIT {
            return Err(changed());
        }
        let receipt: OwnerReceipt = serde_json::from_str(&value).map_err(|_| changed())?;
        if !receipt.matches(&context)
            || receipt.status.preview.agent_id != agent
            || receipt.status.preview.offer_digest != record.offer_digest
        {
            return Err(changed());
        }
    }
    let value = serde_json::to_string(&Completed {
        owner: identity.into(),
        device: device.into(),
        certificate: context.certificate_sha256,
        relay: context.relay,
        operation: operation.into(),
        agent: agent.into(),
    })
    .map_err(|_| changed())?;
    engine.store.set_meta(DONE, &value)?;
    if engine.store.meta(DONE)?.as_deref() != Some(value.as_str()) {
        return Err(changed());
    }
    engine.store.meta_delete(RECEIPT_META)?;
    engine.store.meta_delete(META)?;
    Ok(true)
}
