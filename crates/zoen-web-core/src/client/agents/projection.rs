//! Implementation within the existing browser client authority.
use super::*;

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
            EventBody::ItemCreated {
                item,
                kind,
                content,
                origin,
            } if writer(state, space, &event.author)
                && !item.is_empty()
                && content_kind(*kind, content) =>
            {
                if !state.items.contains_key(item) {
                    state.items.insert(
                        item.clone(),
                        documents::Item {
                            id: item.clone(),
                            space: space.into(),
                            kind: *kind,
                            origin: origin.clone(),
                            versions: vec![documents::Version {
                                content: content.clone(),
                                event: event.clone(),
                                note: origin.clone(),
                                is_undo: false,
                                pending,
                                failure: None,
                            }],
                        },
                    );
                }
            }
            EventBody::ItemVersioned {
                item,
                content,
                note,
            } if writer(state, space, &event.author) => {
                if let Some(item) = state.items.get_mut(item).filter(|item| {
                    item.space == space
                        && (content_kind(item.kind, content)
                            || matches!(
                                (item.kind, content),
                                (ItemKind::Plan, ItemContent::Page(_))
                            ))
                }) {
                    item.versions.push(documents::Version {
                        content: content.clone(),
                        event: event.clone(),
                        note: note.clone(),
                        is_undo: false,
                        pending,
                        failure: None,
                    });
                }
            }
            EventBody::ItemReverted {
                item,
                to_version,
                note,
            } if writer(state, space, &event.author) => {
                if let Some(item) = state.items.get_mut(item).filter(|item| item.space == space) {
                    if let Some(content) = to_version
                        .checked_sub(1)
                        .and_then(|index| item.versions.get(index as usize))
                        .map(|version| version.content.clone())
                    {
                        item.versions.push(documents::Version {
                            content,
                            event: event.clone(),
                            note: note.clone(),
                            is_undo: true,
                            pending,
                            failure: None,
                        });
                    }
                }
            }
            EventBody::GrantIssued { grant }
                if self.valid_agent_grant(state, space, event, grant) =>
            {
                state.grants.push(GrantRead {
                    grant: grant.clone(),
                    space: space.into(),
                    at: event.at_ms,
                    revoked: false,
                })
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
                state.usage.push((agent.clone(), *cents, event.at_ms))
            }
            EventBody::RequestOpened { request }
                if event.author == request.agent
                    && !request.id.is_empty()
                    && !request.content_hash.is_empty()
                    && !state.requests.contains_key(&request.id)
                    && writer(state, space, &event.author)
                    && self.agent_owner(&request.agent).is_some() =>
            {
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
                                && Some(grant.grant.grantor.as_str())
                                    == self.agent_owner(&request.agent)
                                && grant.grant.grantee.as_deref() == Some(&request.agent)
                                && grant.grant.scope == GrantScope::Space(space.into())
                                && matches!(grant.grant.capability, Capability::Trust(_))
                        })
                    {
                        return Ok(());
                    }
                    if proposal.ai_cost_cents > 0 {
                        state.usage.push((
                            request.agent.clone(),
                            proposal.ai_cost_cents,
                            event.at_ms,
                        ));
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
            }
            EventBody::RequestResolved {
                request,
                approved,
                content_hash,
                resolution,
            } => {
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
                    && self.agent_request_current(state, &original.request, space)?
                        != RequestScope::Current
                {
                    return Ok(());
                }
                if !self.valid_agent_receipt(
                    state,
                    &original,
                    event,
                    *approved,
                    resolution.as_ref(),
                ) {
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
                        if let Some(proposal) = &original.request.proposal {
                            let mut virtual_event = event.clone();
                            virtual_event.author = original.request.agent.clone();
                            virtual_event.body = EventBody::ItemCreated {
                                item: proposal.item.clone(),
                                kind: proposal.kind,
                                content: proposal.content.clone(),
                                origin: proposal.origin.clone(),
                            };
                            state.virtual_items.insert(
                                (space.into(), event.client_id.clone()),
                                virtual_event.clone(),
                            );
                            state.items.insert(
                                proposal.item.clone(),
                                documents::Item {
                                    id: proposal.item.clone(),
                                    space: space.into(),
                                    kind: proposal.kind,
                                    origin: proposal.origin.clone(),
                                    versions: vec![documents::Version {
                                        content: proposal.content.clone(),
                                        event: virtual_event,
                                        note: "Criado".into(),
                                        is_undo: false,
                                        pending,
                                        failure: None,
                                    }],
                                },
                            );
                            if proposal.kind == ItemKind::App {
                                state.grants.push(GrantRead {
                                    grant: Grant {
                                        id: format!("gr_app_{}", original.request.id),
                                        grantor: event.author.clone(),
                                        grantee: Some(format!("app:{}", proposal.item)),
                                        scope: GrantScope::Item(proposal.item.clone()),
                                        capability: Capability::Trust(TrustLevel::Act),
                                        expires_at_ms: None,
                                    },
                                    space: space.into(),
                                    at: event.at_ms,
                                    revoked: false,
                                });
                            }
                        }
                    }
                }
                if let Some(request) = state.requests.get_mut(request) {
                    request.approved = Some(*approved);
                    request.resolved = Some(event.at_ms);
                    request.receipt = resolution.clone();
                }
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
