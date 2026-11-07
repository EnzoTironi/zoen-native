//! Explicit first conversation: private context and a minimal, idempotent grant.
use super::*;

impl Client {
    pub(crate) async fn chat_with_managed_agent(
        &self,
        operation: &str,
        agent: &str,
        identity: &str,
        device: &str,
    ) -> R<String> {
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
            if record.state != "registered" || record.agent != agent {
                return Err(changed());
            }
            let space = engine.start_owner_policy_direct(identity, device, agent)?;
            // A retry must neither downgrade an existing grant nor resurrect a
            // revoked one. The normal permissions surface owns later changes.
            if engine
                .state
                .trust_baseline(&space, identity, agent)
                .is_none()
            {
                engine.set_trust(agent, &space, types::TrustLevel::Listen)?;
            }
            Ok(space)
        })
    }
}
