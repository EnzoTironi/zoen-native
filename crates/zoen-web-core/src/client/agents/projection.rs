//! Implementation within the existing browser client authority.
use super::{
    writer, Capability, Client, Event, EventBody, Grant, GrantRead, GrantScope, IdentityKind,
    Projection, Role, Signer,
};

mod accepted;
mod items;
mod requests;

impl<S: Signer> Client<S> {
    pub(super) fn agent_owner(&self, agent: &str) -> Option<&str> {
        self.saved
            .profiles
            .get(agent)
            .filter(|profile| {
                profile.kind == IdentityKind::Agent
                    && event_log::agent_owner::profile_authorized(profile)
            })
            .and_then(|profile| profile.owner.as_deref())
    }
    /// Rebuild the agent view from ordered verified events, then pending outbox events.
    ///
    /// # Errors
    /// Returns a request validation error if its referenced content cannot be read or hashed.
    pub(super) fn agent_projection(&self) -> Result<Projection, String> {
        let mut state = Projection::default();
        // Within each Space, the relay's verified sequence owns precedence. Pending
        // local receipts are applied last and cannot overrule an ordered decision.
        for (space, log) in &self.logs {
            for event in log.events() {
                self.fold_agent_event(&mut state, space, event, false)?;
            }
        }
        let mut pending: Vec<_> = self.saved.outbox.values().collect();
        pending.sort_by(|left, right| {
            (left.order.unwrap_or(0), &left.content)
                .cmp(&(right.order.unwrap_or(0), &right.content))
        });
        for pending in pending {
            if pending.failure.is_some() {
                continue;
            }
            if let Some(event) = pending
                .inner
                .clone()
                .or_else(|| pending.envelope().ok()?.plain_event())
            {
                self.fold_agent_event(&mut state, &event.space, &event, true)?;
            }
        }
        Ok(state)
    }
    /// Apply one event without admitting requests or receipts that fail their authority checks.
    ///
    /// # Errors
    /// Returns a request validation error if its referenced content cannot be read or hashed.
    pub(super) fn fold_agent_event(
        &self,
        state: &mut Projection,
        space: &str,
        event: &Event,
        pending: bool,
    ) -> Result<(), String> {
        match &event.body {
            EventBody::SpaceCreated { .. } => {
                state
                    .rosters
                    .entry(space.into())
                    .or_default()
                    .entry(event.author.clone())
                    .or_insert(Role::Owner);
            }
            EventBody::MemberAdded { identity, role } => {
                state
                    .rosters
                    .entry(space.into())
                    .or_default()
                    .insert(identity.clone(), *role);
            }
            EventBody::MemberRemoved { identity } => {
                state
                    .rosters
                    .entry(space.into())
                    .or_default()
                    .remove(identity);
            }
            EventBody::ItemCreated { .. }
            | EventBody::ItemVersioned { .. }
            | EventBody::ItemReverted { .. } => items::fold_event(state, space, event, pending),
            EventBody::GrantIssued { grant }
                if self.valid_agent_grant(state, space, event, grant) =>
            {
                state.grants.push(GrantRead {
                    grant: grant.clone(),
                    space: space.into(),
                    at: event.at_ms,
                    revoked: false,
                });
            }
            EventBody::GrantRevoked { grant } if writer(state, space, &event.author) => {
                for old in state
                    .grants
                    .iter_mut()
                    .filter(|old| old.grant.id == *grant && old.grant.grantor == event.author)
                {
                    old.revoked = true;
                }
            }
            EventBody::UsageRecorded { agent, cents, .. }
                if *cents >= 0
                    && agent == &event.author
                    && writer(state, space, agent)
                    && self.agent_owner(agent).is_some() =>
            {
                state.usage.push((agent.clone(), *cents, event.at_ms));
            }
            EventBody::RequestOpened { request } => {
                self.fold_open_request(state, space, event, request)?;
            }
            EventBody::RequestResolved { .. } => {
                self.fold_resolved_request(state, space, event, pending)?;
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn valid_agent_grant(
        &self,
        state: &Projection,
        space: &str,
        event: &Event,
        grant: &Grant,
    ) -> bool {
        if grant.grantor != event.author
            || !writer(state, space, &event.author)
            || state.grants.iter().any(|old| old.grant.id == grant.id)
            || grant.id.starts_with("gr_app_")
            || grant.id.starts_with("gr_standing_")
            || matches!(grant.capability, Capability::MonthlyBudget { cents } if cents < 0)
        {
            return false;
        }
        if self
            .saved
            .profiles
            .get(grant.grantee.as_deref().unwrap_or(""))
            .is_some_and(|profile| {
                profile.kind == IdentityKind::Agent
                    && profile.owner.as_deref() != Some(event.author.as_str())
            })
        {
            return false;
        }
        match &grant.scope {
            GrantScope::Space(id) => id == space,
            GrantScope::Item(id) => state.items.get(id).is_some_and(|item| item.space == space),
            GrantScope::Everywhere => true,
        }
    }
}
