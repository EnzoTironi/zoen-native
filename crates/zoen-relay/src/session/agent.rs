//! Authenticated agent operations share the canonical citizen authority.
use super::*;

impl Session {
    /// Agent citizens: the runtime's own state and cursors, or an owner's control.
    pub(super) async fn agent(&self, req: AgentRequest) -> Result<Reply, String> {
        use crate::agents;
        let pool = &self.st.pool;
        let me = &self.identity;
        let r = match req {
            AgentRequest::ManagedService => {
                return agents::provision::availability(pool)
                    .await
                    .map(Reply::ManagedService)
                    .map_err(|e| e.to_string());
            }
            AgentRequest::PrepareManaged(request) => {
                return agents::provision::prepare(pool, me, &self.device, &request)
                    .await
                    .map(Reply::ManagedAgent)
                    .map_err(|e| e.to_string());
            }
            AgentRequest::ManagedStatus { service, operation } => {
                return agents::provision::status(pool, me, &self.device, &service, &operation)
                    .await
                    .map(Reply::ManagedAgent)
                    .map_err(|e| e.to_string());
            }
            AgentRequest::State => {
                return self
                    .complete_legacy(protocol::complete_reads::Kind::AgentState)
                    .await;
            }
            AgentRequest::Advance { space, seq } => {
                if !matches!(self.st.log.role(&space, me).await, Ok(Some(_))) {
                    return Err(agents::AgentCitizenError::NotMember.to_string());
                }
                agents::advance_cursor(pool, me, &self.device, &space, seq).await
            }
            AgentRequest::Define {
                agent,
                version,
                kind,
                definition,
            } => {
                if definition.len() > 64 * 1024 {
                    return Err("definition too large".into());
                }
                agents::define(pool, me, &self.device, &agent, version, &kind, &definition).await
            }
            AgentRequest::Status { agent } => {
                return agents::owner_status(pool, me, &agent)
                    .await
                    .map(Reply::AgentState)
                    .map_err(|e| e.to_string());
            }
            AgentRequest::ControlStatus { agent } => {
                return agents::control_status(pool, me, &agent)
                    .await
                    .map(Reply::AgentControl)
                    .map_err(|e| e.to_string());
            }
            AgentRequest::ControlReviewed {
                agent,
                action,
                operation,
                revision,
            } => {
                let result = agents::control_reviewed(
                    pool,
                    me,
                    &self.device,
                    &agent,
                    action,
                    &operation,
                    revision,
                )
                .await
                .map_err(|e| e.to_string())?;
                if result.status == "revoked" {
                    self.st
                        .log
                        .revoke_agent_invite(&agent)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                return Ok(Reply::AgentControl(result));
            }
            AgentRequest::Control { agent, action } => {
                agents::control(pool, me, &agent, action)
                    .await
                    .map_err(|e| e.to_string())?;
                // Turning an agent off also takes its share link down, here on the relay
                // so no client can leave it up.
                if action == protocol::AgentAction::Revoke {
                    self.st.log.revoke_agent_invite(&agent).await?;
                }
                return Ok(Reply::Done);
            }
        };
        r.map(|()| Reply::Done).map_err(|e| e.to_string())
    }
}
