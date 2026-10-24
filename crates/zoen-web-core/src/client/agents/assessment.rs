//! Implementation within the existing browser client authority.
use super::*;

impl<S: Signer> Client<S> {
    pub(super) fn agent_request_current(
        &self,
        state: &Projection,
        request: &AgentRequest,
        space: &str,
    ) -> Result<RequestScope, String> {
        if !writer(state, space, &request.agent) {
            return Ok(RequestScope::Changed);
        }
        if let Some(proposal) = &request.proposal {
            let content_valid = match (&proposal.kind, &proposal.content) {
                (ItemKind::Plan, ItemContent::Plan(plan)) => plan
                    .sections
                    .iter()
                    .any(|section| !section.lines.is_empty()),
                (ItemKind::App, ItemContent::App(app)) => {
                    miniapps::spec(&app.app).is_some_and(|spec| {
                        app.resource_uri == spec.resource_uri
                            && serde_json::from_str::<Value>(&app.state_json).is_ok()
                    })
                }
                _ => false,
            };
            return Ok(
                if request.action == ActionClass::Reversible
                    && request.item.is_none()
                    && request.line.is_none()
                    && proposal.ai_cost_cents >= 0
                    && !proposal.item.is_empty()
                    && content_valid
                    && !state.items.contains_key(&proposal.item)
                    && request.proposal_hash(space).as_deref() == Some(&request.content_hash)
                {
                    RequestScope::Current
                } else {
                    RequestScope::Changed
                },
            );
        }
        match (&request.item, &request.line) {
            (Some(id), Some(line)) => {
                let Some(item) = state
                    .items
                    .get(id)
                    .filter(|item| item.space == space && item.kind == ItemKind::Plan)
                else {
                    return Ok(RequestScope::Changed);
                };
                let read = self.plan_read(item)?;
                if !read.ready {
                    return Ok(RequestScope::Waiting);
                }
                let Some(line) = read.document.line(line) else {
                    return Ok(RequestScope::Changed);
                };
                let hash = event_log::content_hash(&(&line.text, line.cost_cents))
                    .map_err(|error| error.to_string())?;
                Ok(if hash == request.content_hash {
                    RequestScope::Current
                } else {
                    RequestScope::Changed
                })
            }
            (None, None) => Ok(
                if request.action != ActionClass::Reversible
                    || event_log::content_hash(&(
                        &request.title,
                        &request.detail,
                        &request.audience,
                    ))
                    .map_err(|error| error.to_string())?
                        == request.content_hash
                {
                    RequestScope::Current
                } else {
                    RequestScope::Changed
                },
            ),
            _ => Ok(RequestScope::Changed),
        }
    }
    pub(super) fn valid_agent_receipt(
        &self,
        state: &Projection,
        original: &RequestRead,
        event: &Event,
        approved: bool,
        receipt: Option<&RequestResolution>,
    ) -> bool {
        let request = &original.request;
        let Some(receipt) = receipt else {
            return request.proposal.is_none();
        };
        let proposal = request.proposal.as_ref();
        if approved
            && proposal.is_some_and(|proposal| proposal.kind == ItemKind::App)
            && state
                .grants
                .iter()
                .any(|grant| grant.grant.id == format!("gr_app_{}", request.id))
        {
            return false;
        }
        if receipt.created_item.as_ref()
            != if approved {
                proposal.map(|proposal| &proposal.item)
            } else {
                None
            }
            || receipt.ai_cost_cents != proposal.map_or(0, |proposal| proposal.ai_cost_cents)
            || approved && proposal.is_some_and(|proposal| receipt.message != proposal.message)
        {
            return false;
        }
        receipt.standing_grant.as_ref().is_none_or(|grant| grant.id == format!("gr_standing_{}", request.id) && !state.grants.iter().any(|old| old.grant.id == grant.id) && grant.grantor == event.author && grant.grantee.as_deref() == Some(&request.agent) && grant.scope == GrantScope::Space(event.space.clone()) && grant.expires_at_ms.is_none() && matches!(&grant.capability, Capability::Standing { action, allow } if action == standing_key(&request.action) && *allow == approved) && (!approved || standing_allow_permitted(&request.action, &Policy::default())))
    }
    pub(super) fn live_grants<'a>(
        &'a self,
        state: &'a Projection,
        agent: &'a str,
        now: i64,
    ) -> impl Iterator<Item = &'a GrantRead> {
        state.grants.iter().rev().filter(move |grant| {
            !grant.revoked
                && grant.grant.expires_at_ms.is_none_or(|at| at > now)
                && grant.grant.grantee.as_deref() == Some(agent)
                && Some(grant.grant.grantor.as_str()) == self.agent_owner(agent)
        })
    }
    pub(super) fn agent_trust(
        &self,
        state: &Projection,
        agent: &str,
        space: &str,
        now: i64,
    ) -> TrustLevel {
        self.live_grants(state, agent, now)
            .filter(|grant| grant.grant.scope == GrantScope::Space(space.into()))
            .find_map(|grant| match grant.grant.capability {
                Capability::Trust(level) => Some(level),
                _ => None,
            })
            .unwrap_or(
                if self.agent_owner(agent) == Some(self.account.identity.as_str()) {
                    TrustLevel::Act
                } else {
                    TrustLevel::Suggest
                },
            )
    }
    pub(super) fn agent_budget(&self, state: &Projection, agent: &str, now: i64) -> Option<Budget> {
        if self.agent_owner(agent) != Some(self.account.identity.as_str()) {
            return None;
        }
        let limit =
            self.live_grants(state, agent, now)
                .find_map(|grant| match grant.grant.capability {
                    Capability::MonthlyBudget { cents } => Some(cents),
                    _ => None,
                })?;
        Some(Budget {
            limit_cents: limit,
            spent_cents: state
                .usage
                .iter()
                .filter(|(id, _, at)| id == agent && month(*at) == month(now))
                .fold(0_i64, |sum, (_, cents, _)| sum.saturating_add(*cents)),
        })
    }
    pub(super) fn agent_standing<'a>(
        &'a self,
        state: &'a Projection,
        agent: &'a str,
        space: &str,
        action: &ActionClass,
        now: i64,
    ) -> Option<(&'a GrantRead, bool)> {
        self.live_grants(state, agent, now)
            .filter(|grant| grant.grant.scope == GrantScope::Space(space.into()))
            .find_map(|grant| match &grant.grant.capability {
                Capability::Standing { action: key, allow } if key == standing_key(action) => {
                    Some((grant, *allow))
                }
                _ => None,
            })
    }
    pub(super) fn agent_decision(
        &self,
        state: &Projection,
        agent: &str,
        space: &str,
        action: &ActionClass,
        now: i64,
    ) -> Decision {
        let budget = self.agent_budget(state, agent, now).unwrap_or(Budget {
            limit_cents: i64::MAX / 4,
            spent_cents: 0,
        });
        let decision = evaluate(
            self.agent_trust(state, agent, space, now),
            action,
            0,
            &budget,
            &Policy::default(),
        );
        if let Decision::Request(_) = &decision {
            match self.agent_standing(state, agent, space, action, now) {
                Some((_, false)) => return Decision::Block(Reason::StandingDeny),
                Some((_, true)) if standing_allow_permitted(action, &Policy::default()) => {
                    return Decision::Act {
                        undoable: matches!(action, ActionClass::Reversible),
                    };
                }
                _ => {}
            }
        }
        decision
    }
    pub(super) fn agent_request_value(
        &self,
        state: &Projection,
        read: &RequestRead,
        now: i64,
    ) -> Result<Value, String> {
        let request = &read.request;
        let status = match read.approved {
            Some(true) => "Approved",
            Some(false) => "Denied",
            None => {
                if self.agent_request_current(state, request, &read.space)? == RequestScope::Changed
                {
                    "Stale"
                } else {
                    "Pending"
                }
            }
        };
        let decision =
            self.agent_decision(state, &request.agent, &read.space, &request.action, now);
        let explanation = match decision {
            Decision::Request(why) | Decision::Block(why) => reason(&why),
            Decision::Act { .. } => "Pedido aberto antes de um nível de autonomia maior.".into(),
        };
        Ok(
            json!({ "id": request.id, "agent": self.persona_value(&request.agent), "space_id": read.space,
            "space_title": self.metadata(&read.space).map(|(title, _, _)| title).unwrap_or_default(), "title": request.title,
            "detail": request.detail, "audience": request.audience, "action_label": action_label(&request.action),
            "cost_cents": match request.action { ActionClass::Money { cents } => Some(cents.to_string()), _ => None },
            "reason": explanation, "status": status, "opened_ms": read.opened.to_string(), "resolved_ms": read.resolved.map(|at| at.to_string()),
            "item_id": request.item.as_ref().or_else(|| read.receipt.as_ref().and_then(|receipt| receipt.created_item.as_ref())), "line_id": request.line,
            "action_key": standing_key(&request.action), "can_always_approve": standing_allow_permitted(&request.action, &Policy::default()),
            "by_standing": read.receipt.as_ref().is_some_and(|receipt| receipt.standing_grant.is_some()) }),
        )
    }
    pub(super) fn agent_profile_value(&self, state: &Projection, agent: &str, now: i64) -> Value {
        let budget = self.agent_budget(state, agent, now);
        let mut resolved: Vec<_> = state
            .requests
            .values()
            .filter(|request| request.request.agent == agent && request.resolved.is_some())
            .collect();
        resolved.sort_by_key(|request| std::cmp::Reverse(request.resolved));
        let mut owner_notes = 0;
        if self.agent_owner(agent) == Some(self.account.identity.as_str()) {
            for (space, log) in &self.logs {
                if self
                    .metadata(space)
                    .is_some_and(|(_, kind, _)| kind == SpaceKind::Direct)
                    && state
                        .rosters
                        .get(space)
                        .is_some_and(|roster| roster.contains_key(agent))
                {
                    let mut pending_notes = 0;
                    for event in log.events() {
                        if let EventBody::MessagePosted { text, .. } = &event.body {
                            if event.author == self.account.identity {
                                pending_notes = 0;
                            } else if event.author == agent && asking_note(text) {
                                pending_notes += 1;
                            }
                        }
                    }
                    owner_notes += pending_notes;
                }
            }
        }
        json!({ "persona": self.persona_value(agent), "budget_limit_cents": budget.map(|budget| budget.limit_cents.to_string()),
            "budget_spent_cents": budget.map(|budget| budget.spent_cents.to_string()), "near_limit": budget.is_some_and(|budget| budget.near_limit()),
            "spaces": state.rosters.iter().filter(|(space, roster)| roster.contains_key(agent) && self.metadata(space).is_some_and(|(_, kind, _)| kind != SpaceKind::Personal)).map(|(space, _)| json!({"space_id": space, "space_title": self.metadata(space).map(|(title, _, _)| title).unwrap_or_default(), "level": self.agent_trust(state, agent, space, now)})).collect::<Vec<_>>(),
            "pending_requests": state.requests.values().filter(|request| request.request.agent == agent && request.approved.is_none()).count(),
            "approvals_streak": resolved.iter().take_while(|request| request.approved == Some(true)).count(),
            "runs_on": "O agente roda no aparelho ou serviço configurado pelo dono.", "owner_notes_pending": owner_notes })
    }
}
