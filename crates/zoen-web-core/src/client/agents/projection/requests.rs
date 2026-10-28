//! Request admission and owner receipt projection over the verified event order.
use super::super::{
    writer, AgentRequest, Capability, Client, Event, EventBody, GrantRead, GrantScope, Projection,
    RequestRead, RequestScope, Signer,
};
use super::accepted;

impl<S: Signer> Client<S> {
    /// Admit an agent's request only after its proposal and paid-use grant are current.
    ///
    /// # Errors
    /// Propagates the current-proposal validation error instead of admitting the request.
    pub(super) fn fold_open_request(
        &self,
        state: &mut Projection,
        space: &str,
        event: &Event,
        request: &AgentRequest,
    ) -> Result<(), String> {
        if event.author != request.agent
            || request.id.is_empty()
            || request.content_hash.is_empty()
            || state.requests.contains_key(&request.id)
            || !writer(state, space, &event.author)
            || self.agent_owner(&request.agent).is_none()
        {
            return Ok(());
        }
        // This client has no typed tool-approval execution port. Fail closed.
        if request.tool_proposal.is_some() {
            return Ok(());
        }
        if request.proposal.is_some()
            && self.agent_request_current(state, request, space)? != RequestScope::Current
        {
            return Ok(());
        }
        if let Some(proposal) = &request.proposal {
            if proposal.ai_cost_cents > 0
                && !state.grants.iter().any(|grant| {
                    !grant.revoked
                        && grant.grant.expires_at_ms.is_none_or(|at| at > event.at_ms)
                        && Some(grant.grant.grantor.as_str()) == self.agent_owner(&request.agent)
                        && grant.grant.grantee.as_deref() == Some(&request.agent)
                        && grant.grant.scope == GrantScope::Space(space.into())
                        && matches!(grant.grant.capability, Capability::Trust(_))
                })
            {
                return Ok(());
            }
            if proposal.ai_cost_cents > 0 {
                state
                    .usage
                    .push((request.agent.clone(), proposal.ai_cost_cents, event.at_ms));
            }
        }
        state.requests.insert(
            request.id.clone(),
            RequestRead {
                request: request.clone(),
                space: space.into(),
                opened: event.at_ms,
                resolved: None,
                approved: None,
                receipt: None,
            },
        );
        Ok(())
    }

    /// Project the first valid owner resolution, including its authenticated receipt effects.
    ///
    /// # Errors
    /// Returns the current-request validation error if approved content cannot be read or hashed.
    pub(super) fn fold_resolved_request(
        &self,
        state: &mut Projection,
        space: &str,
        event: &Event,
        pending: bool,
    ) -> Result<(), String> {
        let EventBody::RequestResolved {
            request,
            approved,
            content_hash,
            resolution,
        } = &event.body
        else {
            return Ok(());
        };
        let Some(original) = state.requests.get(request).cloned() else {
            return Ok(());
        };
        if original.approved.is_some()
            || original.space != space
            || original.request.content_hash != *content_hash
            || !writer(state, space, &event.author)
            || self.agent_owner(&original.request.agent) != Some(event.author.as_str())
        {
            return Ok(());
        }
        if *approved
            && self.agent_request_current(state, &original.request, space)? != RequestScope::Current
        {
            return Ok(());
        }
        if !Self::valid_agent_receipt(state, &original, event, *approved, resolution.as_ref()) {
            return Ok(());
        }
        if let Some(receipt) = resolution {
            if let Some(grant) = &receipt.standing_grant {
                state.grants.push(GrantRead {
                    grant: grant.clone(),
                    space: space.into(),
                    at: event.at_ms,
                    revoked: false,
                });
            }
            if *approved {
                accepted::apply_item(state, space, event, pending, &original);
            }
        }
        if let Some(request) = state.requests.get_mut(request) {
            request.approved = Some(*approved);
            request.resolved = Some(event.at_ms);
            request.receipt.clone_from(resolution);
        }
        Ok(())
    }
}
