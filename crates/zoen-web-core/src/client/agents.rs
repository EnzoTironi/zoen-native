//! Disposable agent/approval views over the verified Space logs. Permission decisions
//! use roda-grants; writes use the same signed and MLS-encrypted outbox as messages.
//! No agent key, second document store, or approval authority lives in JavaScript.

use super::*;
use roda_grants::{
    evaluate, standing_allow_permitted, standing_key, Budget, Decision, Policy, Reason,
};
use roda_types::{
    ActionClass, AgentRequest, Capability, Grant, GrantScope, IdentityKind, ItemContent, ItemKind,
    RequestResolution, TrustLevel,
};
use serde_json::{json, Value};

#[derive(Clone)]
struct GrantRead {
    grant: Grant,
    space: String,
    at: i64,
    revoked: bool,
}
#[derive(Clone)]
struct RequestRead {
    request: AgentRequest,
    space: String,
    opened: i64,
    resolved: Option<i64>,
    approved: Option<bool>,
    receipt: Option<RequestResolution>,
}
#[derive(Default)]
struct Projection {
    rosters: BTreeMap<String, BTreeMap<String, Role>>,
    grants: Vec<GrantRead>,
    requests: BTreeMap<String, RequestRead>,
    items: BTreeMap<String, documents::Item>,
    usage: Vec<(String, i64, i64)>,
    virtual_items: BTreeMap<(String, String), Event>,
}
#[derive(PartialEq)]
enum Current {
    Current,
    Changed,
    Waiting,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Agents {},
    AgentProfile {
        agent_id: String,
    },
    Requests {},
    StandingDecisions {},
    SetTrust {
        agent_id: String,
        space_id: String,
        level: TrustLevel,
    },
    RaiseBudget {
        agent_id: String,
        extra_cents: String,
    },
    DecideRequest {
        request_id: String,
        decision: RequestDecision,
    },
    RevokeStanding {
        grant_id: String,
    },
    PreviewDecisions {
        agent_id: String,
        space_id: String,
    },
    AgentActivity {
        agent_id: String,
    },
    Mentions {},
    AppDeviceGrants {},
    RevokeAppDevice {
        grant_id: String,
    },
}
#[derive(Clone, Copy, Deserialize)]
enum RequestDecision {
    Approve,
    Deny,
    AlwaysApprove,
    AlwaysDeny,
}

fn writer(state: &Projection, space: &str, id: &str) -> bool {
    state
        .rosters
        .get(space)
        .and_then(|roster| roster.get(id))
        .is_some_and(|role| *role != Role::Reader)
}
fn content_kind(kind: ItemKind, content: &ItemContent) -> bool {
    matches!(
        (kind, content),
        (ItemKind::Plan, ItemContent::Plan(_))
            | (ItemKind::Task | ItemKind::Note, ItemContent::Text { .. })
            | (ItemKind::Page, ItemContent::Page(_))
            | (ItemKind::App, ItemContent::App(_))
            | (ItemKind::File, ItemContent::File(_))
    )
}
fn month(ms: i64) -> i64 {
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }) * 12 + m
}
fn reason(reason: &Reason) -> String {
    match reason {
        Reason::TrustTooLow { .. } => {
            "O nível de autonomia não permite esta ação sem uma decisão sua.".into()
        }
        Reason::LeavesTheSpace => {
            "Esta ação é irreversível ou sai da conversa e pede sua aprovação.".into()
        }
        Reason::MoneyAboveCeiling { .. } => {
            "Este valor ultrapassa o limite para pagamentos sem aprovação.".into()
        }
        Reason::NewPublicAudience => {
            "Uma nova audiência pública sempre exige sua aprovação.".into()
        }
        Reason::ThirdPartyData => "O uso de dados de terceiros sempre exige sua aprovação.".into(),
        Reason::OverBudget { .. } => "O orçamento disponível não cobre esta ação.".into(),
        Reason::StandingDeny => {
            "Você decidiu sempre negar este tipo de ação nesta conversa.".into()
        }
    }
}
fn action_label(action: &ActionClass) -> &'static str {
    match action {
        ActionClass::Reply => "Responder",
        ActionClass::Reversible => "Alteração reversível",
        ActionClass::External => "Enviar para fora",
        ActionClass::Irreversible => "Ação irreversível",
        ActionClass::Money { .. } => "Pagamento",
        ActionClass::PublicAudience => "Publicar",
        ActionClass::ThirdPartyData => "Usar dados de terceiros",
    }
}

fn asking_note(text: &str) -> bool {
    let Some(body) = text
        .trim()
        .strip_prefix("```json")
        .and_then(|body| body.strip_suffix("```"))
    else {
        return false;
    };
    serde_json::from_str::<Value>(body.trim())
        .ok()
        .and_then(|value| {
            value.as_object().map(|object| {
                object.len() == 1
                    && (object.contains_key("ask_owner") || object.contains_key("agent_proposal"))
            })
        })
        .unwrap_or(false)
}

impl<S: Signer> Client<S> {
    fn agent_owner(&self, agent: &str) -> Option<&str> {
        self.saved
            .profiles
            .get(agent)
            .filter(|profile| {
                profile.kind == IdentityKind::Agent
                    && roda_log::agent_owner::profile_authorized(profile)
            })
            .and_then(|profile| profile.owner.as_deref())
    }
    fn agent_projection(&self) -> Result<Projection, String> {
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
    fn fold_agent_event(
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
                if request.proposal.is_some()
                    && self.agent_request_current(state, request, space)? != Current::Current
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
                        != Current::Current
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
    fn valid_agent_grant(
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
    fn agent_request_current(
        &self,
        state: &Projection,
        request: &AgentRequest,
        space: &str,
    ) -> Result<Current, String> {
        if !writer(state, space, &request.agent) {
            return Ok(Current::Changed);
        }
        if let Some(proposal) = &request.proposal {
            let content_valid = match (&proposal.kind, &proposal.content) {
                (ItemKind::Plan, ItemContent::Plan(plan)) => plan
                    .sections
                    .iter()
                    .any(|section| !section.lines.is_empty()),
                (ItemKind::App, ItemContent::App(app)) => {
                    roda_apps::spec(&app.app).is_some_and(|spec| {
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
                    Current::Current
                } else {
                    Current::Changed
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
                    return Ok(Current::Changed);
                };
                let read = self.plan_read(item)?;
                if !read.ready {
                    return Ok(Current::Waiting);
                }
                Ok(
                    if read.document.line(line).is_some_and(|line| {
                        roda_log::content_hash(&(&line.text, line.cost_cents))
                            == request.content_hash
                    }) {
                        Current::Current
                    } else {
                        Current::Changed
                    },
                )
            }
            (None, None) => Ok(
                if request.action != ActionClass::Reversible
                    || roda_log::content_hash(&(&request.title, &request.detail, &request.audience))
                        == request.content_hash
                {
                    Current::Current
                } else {
                    Current::Changed
                },
            ),
            _ => Ok(Current::Changed),
        }
    }
    fn valid_agent_receipt(
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
    fn live_grants<'a>(
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
    fn agent_trust(&self, state: &Projection, agent: &str, space: &str, now: i64) -> TrustLevel {
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
    fn agent_budget(&self, state: &Projection, agent: &str, now: i64) -> Option<Budget> {
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
    fn agent_standing<'a>(
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
    fn agent_decision(
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
    fn agent_request_value(
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
                if self.agent_request_current(state, request, &read.space)? == Current::Changed {
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
    fn agent_profile_value(&self, state: &Projection, agent: &str, now: i64) -> Value {
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
    /// Disposable virtual ItemCreated at each authenticated proposal receipt position.
    /// Its content bytes remain the original receipt; this is never a signed event.
    pub(super) fn approved_request_items(
        &self,
    ) -> Result<BTreeMap<(String, String), Event>, String> {
        Ok(self.agent_projection()?.virtual_items)
    }

    /// Already-authorized, live Item-scoped grants. The mini-app host uses this
    /// projection too; it must not reconstruct a second permission tape.
    pub(super) fn active_app_grants(&self, item: &str, now: i64) -> Result<Vec<Grant>, String> {
        let state = self.agent_projection()?;
        if !state
            .items
            .get(item)
            .is_some_and(|item| matches!(item.current().content, ItemContent::App(_)))
        {
            return Err("Mini-app indisponível.".into());
        }
        let subject = format!("app:{item}");
        Ok(state
            .grants
            .into_iter()
            .filter(|grant| {
                !grant.revoked
                    && grant.grant.expires_at_ms.is_none_or(|at| at > now)
                    && grant.grant.grantee.as_deref() == Some(subject.as_str())
                    && grant.grant.scope == GrantScope::Item(item.into())
            })
            .map(|grant| grant.grant)
            .collect())
    }

    pub(super) fn grant_browser_app_device(
        &mut self,
        item: &str,
        capability: &str,
        purpose: &str,
        always: bool,
        now: i64,
    ) -> Result<String, String> {
        let state = self.agent_projection()?;
        let app = state
            .items
            .get(item)
            .filter(|item| matches!(item.current().content, ItemContent::App(_)))
            .ok_or("Mini-app indisponível.")?;
        let capability = capability.trim();
        if capability.is_empty()
            || capability.len() > 64
            || !capability
                .chars()
                .all(|value| value.is_ascii_alphanumeric() || ".:-_".contains(value))
        {
            return Err("Capacidade inválida.".into());
        }
        let id = format!("gr_{}", roda_types::new_ulid(now));
        let grant = Grant {
            id: id.clone(),
            grantor: self.account.identity.clone(),
            grantee: Some(format!("app:{item}")),
            scope: GrantScope::Item(item.into()),
            capability: Capability::Device {
                capability: capability.into(),
                purpose: purpose.chars().take(160).collect(),
            },
            expires_at_ms: if always {
                None
            } else {
                Some(now.checked_add(60_000).ok_or("Data fora do limite.")?)
            },
        };
        self.queue_domain_body(
            &app.space,
            EventBody::GrantIssued { grant },
            now,
            Vec::new(),
        )?;
        Ok(id)
    }

    fn queue_agent_bodies(
        &mut self,
        bodies: Vec<(String, EventBody)>,
        now: i64,
    ) -> Result<(), String> {
        let outbox = self.saved.outbox.clone();
        let storage = self
            .provider
            .storage()
            .values
            .read()
            .map_err(|_| "MLS storage lock failed")?
            .clone();
        for (space, body) in bodies {
            if let Err(error) = self.queue_domain_body(&space, body, now, Vec::new()) {
                self.saved.outbox = outbox;
                *self
                    .provider
                    .storage()
                    .values
                    .write()
                    .map_err(|_| "MLS storage lock failed")? = storage;
                return Err(error);
            }
        }
        Ok(())
    }
    fn decide_browser_request(
        &mut self,
        id: &str,
        decision: RequestDecision,
        now: i64,
    ) -> Result<Value, String> {
        let state = self.agent_projection()?;
        let original = state
            .requests
            .get(id)
            .ok_or("Este pedido não está disponível neste aparelho.")?;
        let approve = matches!(
            decision,
            RequestDecision::Approve | RequestDecision::AlwaysApprove
        );
        let standing = matches!(
            decision,
            RequestDecision::AlwaysApprove | RequestDecision::AlwaysDeny
        );
        if self.agent_owner(&original.request.agent) != Some(self.account.identity.as_str())
            || !writer(&state, &original.space, &self.account.identity)
        {
            return Err("Só o dono do agente com permissão nesta conversa pode decidir.".into());
        }
        if standing
            && approve
            && !standing_allow_permitted(&original.request.action, &Policy::default())
        {
            return Err("Esta ação sempre exige uma decisão individual.".into());
        }
        if let Some(previous) = original.approved {
            let receipt = original
                .receipt
                .as_ref()
                .ok_or("Este pedido já foi decidido.")?;
            let same_standing = receipt.standing_grant.as_ref().is_some_and(|given| {
                self.agent_standing(
                    &state,
                    &original.request.agent,
                    &original.space,
                    &original.request.action,
                    now,
                )
                .is_some_and(|(current, allow)| current.grant.id == given.id && allow == approve)
            });
            if previous != approve || standing && !same_standing {
                return Err(
                    "Este pedido já recebeu outra decisão, ou sua permissão permanente mudou."
                        .into(),
                );
            }
            return Ok(
                json!({"request":self.agent_request_value(&state, original, now)?,"message":receipt.message,"standing_grant_id":receipt.standing_grant.as_ref().map(|grant|&grant.id),"also_resolved":0}),
            );
        }
        let mut covered = vec![original.clone()];
        if standing {
            covered.extend(
                state
                    .requests
                    .values()
                    .filter(|other| {
                        other.request.id != id
                            && other.approved.is_none()
                            && other.space == original.space
                            && other.request.agent == original.request.agent
                            && standing_key(&other.request.action)
                                == standing_key(&original.request.action)
                    })
                    .cloned(),
            );
        }
        let mut bodies = Vec::new();
        let mut also = 0;
        let mut message = String::new();
        let mut claimed_items: BTreeSet<_> = state.items.keys().cloned().collect();
        for read in covered {
            if approve {
                let current = self.agent_request_current(&state, &read.request, &read.space)?;
                if current != Current::Current {
                    if read.request.id != id {
                        continue;
                    }
                    return Err(if current == Current::Waiting {
                        "O conteúdo confirmado ainda está chegando."
                    } else {
                        "O conteúdo ou a autorização mudou. Peça uma nova proposta."
                    }
                    .into());
                }
                if !standing_allow_permitted(&read.request.action, &Policy::default()) && standing {
                    continue;
                }
                if let Some(proposal) = &read.request.proposal {
                    if !claimed_items.insert(proposal.item.clone()) {
                        continue;
                    }
                }
                if let Some(item) = &read.request.item {
                    if state
                        .items
                        .get(item)
                        .is_some_and(|item| item.kind == ItemKind::Plan)
                    {
                        let read_plan = self.document_request(
                            &json!({"op":"plan_page","item_id":item}).to_string(),
                            now,
                        )?;
                        if read_plan.result["legacy_edit_admitted"] != true {
                            return Err(
                                "Este plano não admite esta operação; peça uma nova proposta."
                                    .into(),
                            );
                        }
                    }
                }
            }
            let text = if approve {
                read.request
                    .proposal
                    .as_ref()
                    .map(|proposal| proposal.message.clone())
                    .unwrap_or_else(|| {
                        format!(
                            "Aprovado: {}. Aguardando a execução da ferramenta.",
                            read.request.title
                        )
                    })
            } else {
                format!("Ok, não vou {}.", read.request.title)
            };
            let receipt = RequestResolution {
                message: text.clone(),
                created_item: if approve {
                    read.request
                        .proposal
                        .as_ref()
                        .map(|proposal| proposal.item.clone())
                } else {
                    None
                },
                ai_cost_cents: read
                    .request
                    .proposal
                    .as_ref()
                    .map_or(0, |proposal| proposal.ai_cost_cents),
                standing_grant: (standing && read.request.id == id).then(|| Grant {
                    id: format!("gr_standing_{}", read.request.id),
                    grantor: self.account.identity.clone(),
                    grantee: Some(read.request.agent.clone()),
                    scope: GrantScope::Space(read.space.clone()),
                    capability: Capability::Standing {
                        action: standing_key(&read.request.action).into(),
                        allow: approve,
                    },
                    expires_at_ms: None,
                }),
            };
            if read.request.id == id {
                message = text;
            } else {
                also += 1;
            }
            bodies.push((
                read.space,
                EventBody::RequestResolved {
                    request: read.request.id,
                    approved: approve,
                    content_hash: read.request.content_hash,
                    resolution: Some(receipt),
                },
            ));
        }
        self.queue_agent_bodies(bodies, now)?;
        let next = self.agent_projection()?;
        let read = next
            .requests
            .get(id)
            .ok_or("Não foi possível recuperar a decisão.")?;
        Ok(
            json!({"request": self.agent_request_value(&next, read, now)?, "message": message, "standing_grant_id": read.receipt.as_ref().and_then(|receipt| receipt.standing_grant.as_ref()).map(|grant| &grant.id), "also_resolved":also}),
        )
    }

    pub fn agent_request(
        &mut self,
        request: &str,
        now: i64,
    ) -> Result<documents::DocumentResponse, String> {
        if request.len() > 1 << 20 {
            return Err("Pedido muito grande.".into());
        }
        let command: Command = serde_json::from_str(request).map_err(|error| error.to_string())?;
        let revoke_device = matches!(&command, Command::RevokeAppDevice { .. });
        let state = self.agent_projection()?;
        let mut changed = false;
        let result = match command {
            Command::Agents {} => Value::Array(self.saved.profiles.values().filter(|profile| profile.kind == IdentityKind::Agent).map(|profile| self.agent_profile_value(&state, &profile.id, now)).collect()),
            Command::AgentProfile { agent_id } => { if self.agent_owner(&agent_id).is_none() { return Err("Agente indisponível neste aparelho.".into()); } self.agent_profile_value(&state, &agent_id, now) }
            Command::Requests {} => { let mut requests: Vec<_> = state.requests.values().collect(); requests.sort_by_key(|request| std::cmp::Reverse(request.opened)); Value::Array(requests.into_iter().map(|request| self.agent_request_value(&state, request, now)).collect::<Result<_,_>>()?) }
            Command::StandingDecisions {} => Value::Array(state.grants.iter().rev().filter(|grant| !grant.revoked && grant.grant.grantor == self.account.identity && grant.grant.expires_at_ms.is_none_or(|at|at>now)).filter_map(|grant| {
                let Capability::Standing { action, allow } = &grant.grant.capability else { return None; };
                let GrantScope::Space(space) = &grant.grant.scope else { return None; };
                Some(json!({"grant_id": grant.grant.id,"agent":self.persona_value(grant.grant.grantee.as_deref()?),"space_id":space,"space_title":self.metadata(space).map(|(title,_,_)|title).unwrap_or_default(),"action_key":action,"action_label":action,"allow":allow,"at_ms":grant.at.to_string()}))
            }).collect()),
            Command::SetTrust { agent_id, space_id, level } => {
                if self.agent_owner(&agent_id) != Some(self.account.identity.as_str()) || !writer(&state, &space_id, &self.account.identity) || !state.rosters.get(&space_id).is_some_and(|roster|roster.contains_key(&agent_id)) { return Err("Só o dono pode mudar a autonomia de um agente que participa desta conversa.".into()); }
                self.queue_domain_body(&space_id, EventBody::GrantIssued { grant: Grant { id: format!("gr_{}",roda_types::new_ulid(now)), grantor:self.account.identity.clone(),grantee:Some(agent_id),scope:GrantScope::Space(space_id.clone()),capability:Capability::Trust(level),expires_at_ms:None}},now,Vec::new())?;
                changed=true; Value::Null
            }
            Command::RaiseBudget { agent_id, extra_cents } => {
                let extra=extra_cents.parse::<i64>().ok().filter(|extra| *extra>0).ok_or("Informe um acréscimo positivo dentro do limite.")?;
                let budget=self.agent_budget(&state,&agent_id,now).ok_or("Este orçamento não está disponível para esta conta.")?;
                let limit=budget.limit_cents.checked_add(extra).ok_or("O orçamento ultrapassa o limite permitido.")?;
                let personal=self.logs.keys().find(|space| self.metadata(space).is_some_and(|(_,kind,_)|kind==SpaceKind::Personal) && writer(&state,space,&self.account.identity)).cloned().ok_or("O espaço pessoal ainda não chegou neste aparelho.")?;
                self.queue_domain_body(&personal,EventBody::GrantIssued{grant:Grant{id:format!("gr_{}",roda_types::new_ulid(now)),grantor:self.account.identity.clone(),grantee:Some(agent_id.clone()),scope:GrantScope::Everywhere,capability:Capability::MonthlyBudget{cents:limit},expires_at_ms:None}},now,Vec::new())?;
                changed=true; self.agent_profile_value(&self.agent_projection()?,&agent_id,now)
            }
            Command::DecideRequest { request_id, decision } => { let result=self.decide_browser_request(&request_id,decision,now)?; changed=true; result }
            Command::RevokeStanding { grant_id } | Command::RevokeAppDevice { grant_id } => {
                let grant=state.grants.iter().find(|grant|grant.grant.id==grant_id && !grant.revoked && grant.grant.grantor==self.account.identity).ok_or("Esta permissão não está disponível para esta conta.")?;
                let matching = if revoke_device { matches!((&grant.grant.capability, &grant.grant.scope), (Capability::Device { .. }, GrantScope::Item(_))) } else { matches!((&grant.grant.capability, &grant.grant.scope), (Capability::Standing { .. }, GrantScope::Space(_))) };
                if !matching { return Err("Esta permissão não corresponde ao controle solicitado.".into()); }
                self.queue_domain_body(&grant.space,EventBody::GrantRevoked{grant:grant_id},now,Vec::new())?; changed=true; Value::Null
            }
            Command::PreviewDecisions { agent_id, space_id } => {
                let cases=[("Responder","Responder quando chamado",ActionClass::Reply),("Editar o plano","Alterar um plano compartilhado",ActionClass::Reversible),("Mandar para fora","Enviar o roteiro por WhatsApp",ActionClass::External),("Pagamento pequeno","Pagar R$ 40,00",ActionClass::Money{cents:4_000}),("Pagamento grande","Reservar por R$ 640,00",ActionClass::Money{cents:64_000}),("Apagar de verdade","Apagar um histórico",ActionClass::Irreversible),("Publicar","Publicar para uma nova audiência",ActionClass::PublicAudience)];
                Value::Array(cases.into_iter().map(|(action,example,class)| { let decision=self.agent_decision(&state,&agent_id,&space_id,&class,now); let (kind,explanation)=match decision {Decision::Act{undoable:true}=>("ActWithUndo","Executa e você pode desfazer.".into()),Decision::Act{undoable:false}=>("Act","Executa dentro das permissões.".into()),Decision::Request(why)=>("Request",reason(&why)),Decision::Block(why)=>("Block",reason(&why))}; json!({"action":action,"example":example,"kind":kind,"explanation":explanation,"red_line":matches!(class,ActionClass::PublicAudience|ActionClass::ThirdPartyData)||matches!(class,ActionClass::Money{cents}if cents>Policy::default().money_ceiling_cents)}) }).collect())
            }
            Command::AgentActivity { agent_id } => {
                if self.agent_owner(&agent_id).is_none(){return Err("Agente indisponível.".into());}
                let mut entries=Vec::new();
                for(space,log)in &self.logs {for(index,event)in log.events().iter().enumerate(){if event.author!=agent_id{continue;} let(label,detail)=match &event.body{EventBody::MessagePosted{text,attaches,..}=>(if attaches.is_some(){"Entregou um item"}else{"Respondeu"},text.chars().take(90).collect()),EventBody::ItemCreated{content,..}=>("Criou",content.title()),EventBody::ItemVersioned{note,..}=>("Editou",note.clone()),EventBody::RequestOpened{request}=>("Pediu aprovação",request.title.clone()),_=>continue}; let cost=match &event.body{EventBody::RequestOpened{request}=>request.proposal.as_ref().map(|proposal|proposal.ai_cost_cents),_=>None}.or_else(||log.events()[index+1..].iter().take(3).find_map(|next|match &next.body{EventBody::UsageRecorded{agent,cents,..}if agent==&agent_id && next.author==agent_id=>Some(*cents),_=>None})); entries.push((event.at_ms,json!({"label":label,"detail":detail,"space_id":space,"space_title":self.metadata(space).map(|(title,_,_)|title).unwrap_or_default(),"at_ms":event.at_ms.to_string(),"cost_cents":cost.map(|cents|cents.to_string())})));}}
                entries.sort_by_key(|(at,_)|std::cmp::Reverse(*at));Value::Array(entries.into_iter().take(30).map(|(_,value)|value).collect())
            }
            Command::Mentions {} => {
                // The shared message view owns edits/deletions and threads. The host
                // maps these exact references to TimelineEntry, never a fabricated body.
                let token=format!("@{}",self.account.name.to_lowercase());
                let mut mentions=Vec::new();for chat in self.conversations(){for message in &chat.messages{if message.author!=self.account.identity && !message.deleted && message.text.to_lowercase().contains(&token){mentions.push((message.at_ms,json!({"space_id":chat.id,"space_title":chat.title,"message_id":message.entry.as_ref().unwrap_or(&message.id)})));}}}
                mentions.sort_by_key(|(at,_)|std::cmp::Reverse(*at));Value::Array(mentions.into_iter().map(|(_,value)|value).collect())
            }
            Command::AppDeviceGrants {} => Value::Array(state.grants.iter().rev().filter(|grant|!grant.revoked && grant.grant.grantor==self.account.identity && grant.grant.expires_at_ms.is_none_or(|at|at>now)).filter_map(|grant|{let Capability::Device{capability,purpose}=&grant.grant.capability else{return None;};let GrantScope::Item(item)=&grant.grant.scope else{return None;};let item=state.items.get(item)?;let ItemContent::App(app)=&item.current().content else{return None;};Some(json!({"grant_id":grant.grant.id,"item_id":item.id,"item_title":app.title,"app_name":roda_apps::spec(&app.app).map(|spec|spec.name).unwrap_or(&app.app),"space_title":self.metadata(&item.space).map(|(title,_,_)|title).unwrap_or_default(),"capability":capability,"purpose":purpose,"always":grant.grant.expires_at_ms.is_none(),"at_ms":grant.at.to_string()}))}).collect()),
        };
        Ok(documents::DocumentResponse {
            result,
            update: if changed {
                Some(self.update(Vec::new())?)
            } else {
                None
            },
            downloads: Vec::new(),
            uploads: Vec::new(),
        })
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use openmls_traits::{signatures::SignerError, types::SignatureScheme};
    use roda_log::{Author, Signer as NativeSigner};
    use roda_types::{ItemProposal, PlanDoc, PlanLine, PlanSection};

    #[derive(Clone)]
    struct HostSigner(NativeSigner);
    impl Signer for HostSigner {
        fn sign(&self, bytes: &[u8]) -> Result<Vec<u8>, SignerError> {
            hex::decode(self.0.sign(bytes)).map_err(|_| SignerError::SigningError)
        }
        fn signature_scheme(&self) -> SignatureScheme {
            roda_mls::SUITE.signature_algorithm()
        }
    }
    struct Fixture {
        client: Client<HostSigner>,
        owner: Author,
        agent: Author,
        stranger: Author,
        now: i64,
    }
    impl Fixture {
        fn new() -> Self {
            let identity = NativeSigner::generate();
            let device = NativeSigner::generate();
            let owner = Author::device(&identity, NativeSigner::generate());
            let agent_key = NativeSigner::generate();
            let agent = Author::root(agent_key);
            let stranger = Author::root(NativeSigner::generate());
            let account = LinkedAccount {
                identity: identity.id(),
                device: device.id(),
                cert: identity.sign(&roda_log::device_cert_message(&device.id())),
                name: "Owner".into(),
                handle: "owner".into(),
                relay: "http://localhost".into(),
                primary: identity.id(),
                primary_hpke: String::new(),
                hpke_secret: String::new(),
                link_secret: String::new(),
                agreement_secret: String::new(),
                profile_key: String::new(),
                profile_key_version: 0,
                profile_seen_version: 0,
                profile_fields: String::new(),
            };
            let mut client = Client::new(account, HostSigner(device), "").unwrap();
            client.synced = true;
            let profile = Identity {
                id: agent.identity.clone(),
                kind: IdentityKind::Agent,
                name: "Agent".into(),
                handle: "agent".into(),
                tint_hex: "#123456".into(),
                glyph: None,
                owner: Some(identity.id()),
                bio: String::new(),
                owner_proof: Some(Box::new(
                    roda_log::agent_owner::authorize(&owner, &agent.identity).unwrap(),
                )),
            };
            client.saved.profiles.insert(profile.id.clone(), profile);
            let mut fixture = Self {
                client,
                owner,
                agent,
                stranger,
                now: 1_791_200_000_000,
            };
            for (space, kind) in [
                ("sp_personal", SpaceKind::Personal),
                ("sp_chat", SpaceKind::Group),
            ] {
                fixture.append(
                    space,
                    "owner",
                    EventBody::SpaceCreated {
                        title: space.into(),
                        kind,
                        privacy: Privacy::Closed,
                    },
                );
            }
            fixture.append(
                "sp_chat",
                "owner",
                EventBody::MemberAdded {
                    identity: fixture.agent.identity.clone(),
                    role: Role::Member,
                },
            );
            fixture.append(
                "sp_chat",
                "owner",
                EventBody::MemberAdded {
                    identity: fixture.stranger.identity.clone(),
                    role: Role::Member,
                },
            );
            fixture.grant(
                "sp_personal",
                "budget",
                GrantScope::Everywhere,
                Capability::MonthlyBudget {
                    cents: 9_007_199_254_740_993,
                },
            );
            fixture.grant(
                "sp_chat",
                "trust",
                GrantScope::Space("sp_chat".into()),
                Capability::Trust(TrustLevel::Suggest),
            );
            fixture
        }
        fn append(&mut self, space: &str, who: &str, body: EventBody) {
            self.now += 1;
            let author = match who {
                "agent" => &self.agent,
                "stranger" => &self.stranger,
                _ => &self.owner,
            };
            let log = self
                .client
                .logs
                .entry(space.into())
                .or_insert_with(|| SpaceLog::new(space));
            let event = author.sign_event(
                space,
                &roda_types::new_ulid(self.now),
                self.now,
                log.head(),
                body,
            );
            log.sequence(event);
        }
        fn grant(&mut self, space: &str, id: &str, scope: GrantScope, capability: Capability) {
            self.append(
                space,
                "owner",
                EventBody::GrantIssued {
                    grant: Grant {
                        id: id.into(),
                        grantor: self.owner.identity.clone(),
                        grantee: Some(self.agent.identity.clone()),
                        scope,
                        capability,
                        expires_at_ms: None,
                    },
                },
            );
        }
        fn command(&mut self, command: Value) -> Result<documents::DocumentResponse, String> {
            self.now += 1;
            self.client.agent_request(&command.to_string(), self.now)
        }
        fn proposal(&mut self) -> AgentRequest {
            let proposal = ItemProposal {
                item: "it_proposal".into(),
                kind: ItemKind::Plan,
                content: ItemContent::Plan(PlanDoc {
                    title: "Retained".into(),
                    summary: String::new(),
                    budget_cents: None,
                    sections: vec![PlanSection {
                        title: "Actions".into(),
                        lines: vec![PlanLine {
                            id: "ln_one".into(),
                            text: "Exact output".into(),
                            cost_cents: 9_007_199_254_740_993,
                            done: false,
                        }],
                    }],
                }),
                origin: "Agent".into(),
                message: "A proposta foi compartilhada.".into(),
                ai_cost_cents: 37,
            };
            let mut request = AgentRequest {
                id: "rq_proposal".into(),
                agent: self.agent.identity.clone(),
                title: "Create plan".into(),
                detail: "Retained output".into(),
                audience: "This chat".into(),
                action: ActionClass::Reversible,
                content_hash: String::new(),
                item: None,
                line: None,
                proposal: Some(Box::new(proposal)),
            };
            request.content_hash = request.proposal_hash("sp_chat").unwrap();
            request
        }
    }

    #[test]
    fn exact_budgets_ignore_another_members_forged_grant_and_reject_overflow_without_writes() {
        let mut fixture = Fixture::new();
        let id = fixture.agent.identity.clone();
        let profile = fixture
            .command(json!({"op":"agent_profile","agent_id":id}))
            .unwrap()
            .result;
        assert_eq!(profile["budget_limit_cents"], "9007199254740993");
        fixture.append(
            "sp_chat",
            "stranger",
            EventBody::GrantIssued {
                grant: Grant {
                    id: "gr_forged".into(),
                    grantor: fixture.stranger.identity.clone(),
                    grantee: Some(id.clone()),
                    scope: GrantScope::Space("sp_chat".into()),
                    capability: Capability::Trust(TrustLevel::Autonomous),
                    expires_at_ms: None,
                },
            },
        );
        let state = fixture.client.agent_projection().unwrap();
        assert_eq!(
            fixture
                .client
                .agent_trust(&state, &id, "sp_chat", fixture.now),
            TrustLevel::Suggest
        );
        let before = fixture.client.saved.outbox.len();
        assert!(fixture
            .command(json!({"op":"raise_budget","agent_id":id,"extra_cents":"9223372036854775807"}))
            .is_err());
        assert_eq!(fixture.client.saved.outbox.len(), before);
    }
    #[test]
    fn retained_proposal_approval_creates_the_exact_item_and_charges_usage_only_once_after_reload()
    {
        let mut fixture = Fixture::new();
        let request = fixture.proposal();
        fixture.append(
            "sp_chat",
            "agent",
            EventBody::RequestOpened {
                request: request.clone(),
            },
        );
        let decision = fixture
            .command(
                json!({"op":"decide_request","request_id":request.id,"decision":"AlwaysApprove"}),
            )
            .unwrap();
        assert_eq!(decision.result["request"]["status"], "Approved");
        assert_eq!(
            decision.result["standing_grant_id"],
            "gr_standing_rq_proposal"
        );
        let count = fixture.client.saved.outbox.len();
        let retry = fixture
            .command(
                json!({"op":"decide_request","request_id":request.id,"decision":"AlwaysApprove"}),
            )
            .unwrap();
        assert_eq!(fixture.client.saved.outbox.len(), count);
        let snapshot = retry.update.unwrap().snapshot;
        let mut reopened = Client::new(
            fixture.client.account.clone(),
            fixture.client.signer.clone(),
            &snapshot,
        )
        .unwrap();
        let profile = reopened
            .agent_request(
                &json!({"op":"agent_profile","agent_id":fixture.agent.identity}).to_string(),
                fixture.now,
            )
            .unwrap()
            .result;
        assert_eq!(profile["budget_spent_cents"], "37");
        let item = reopened
            .document_request(
                &json!({"op":"item","item_id":"it_proposal"}).to_string(),
                fixture.now,
            )
            .unwrap()
            .result;
        assert_eq!(
            item["plan"]["sections"][0]["lines"][0]["cost_cents"],
            "9007199254740993"
        );
        assert_eq!(item["created_by"]["id"], fixture.agent.identity);
    }
    #[test]
    fn red_lines_cannot_be_always_approved_and_another_member_cannot_resolve_a_request() {
        let mut fixture = Fixture::new();
        let request = AgentRequest {
            id: "rq_money".into(),
            agent: fixture.agent.identity.clone(),
            title: "Book".into(),
            detail: "Hotel".into(),
            audience: "Group".into(),
            action: ActionClass::Money { cents: 64000 },
            content_hash: "exact".into(),
            item: None,
            line: None,
            proposal: None,
        };
        fixture.append(
            "sp_chat",
            "agent",
            EventBody::RequestOpened {
                request: request.clone(),
            },
        );
        fixture.append(
            "sp_chat",
            "stranger",
            EventBody::RequestResolved {
                request: request.id.clone(),
                approved: true,
                content_hash: request.content_hash.clone(),
                resolution: None,
            },
        );
        let read = fixture.command(json!({"op":"requests"})).unwrap().result;
        assert_eq!(read[0]["status"], "Pending");
        assert_eq!(read[0]["can_always_approve"], false);
        let before = fixture.client.saved.outbox.len();
        assert!(fixture
            .command(
                json!({"op":"decide_request","request_id":request.id,"decision":"AlwaysApprove"})
            )
            .is_err());
        assert_eq!(fixture.client.saved.outbox.len(), before);
        let accepted = fixture
            .command(json!({"op":"decide_request","request_id":request.id,"decision":"Approve"}))
            .unwrap()
            .result;
        assert_eq!(accepted["request"]["status"], "Approved");
        assert!(accepted["message"].as_str().unwrap().contains("Aguardando"));
    }
    #[test]
    fn editing_a_line_invalidates_the_exact_content_approval() {
        let mut fixture = Fixture::new();
        let request = fixture.proposal();
        let proposal = request.proposal.unwrap();
        let ItemContent::Plan(mut plan) = proposal.content else {
            panic!()
        };
        fixture.append(
            "sp_chat",
            "owner",
            EventBody::ItemCreated {
                item: proposal.item.clone(),
                kind: ItemKind::Plan,
                content: ItemContent::Plan(plan.clone()),
                origin: "Fixture".into(),
            },
        );
        let line = &plan.sections[0].lines[0];
        let request = AgentRequest {
            id: "rq_line".into(),
            agent: fixture.agent.identity.clone(),
            title: "Execute".into(),
            detail: line.text.clone(),
            audience: "Group".into(),
            action: ActionClass::Money {
                cents: line.cost_cents,
            },
            content_hash: roda_log::content_hash(&(&line.text, line.cost_cents)),
            item: Some(proposal.item.clone()),
            line: Some(line.id.clone()),
            proposal: None,
        };
        fixture.append(
            "sp_chat",
            "agent",
            EventBody::RequestOpened {
                request: request.clone(),
            },
        );
        plan.sections[0].lines[0].text = "Edited".into();
        fixture.append(
            "sp_chat",
            "owner",
            EventBody::ItemVersioned {
                item: proposal.item,
                content: ItemContent::Plan(plan),
                note: "Changed".into(),
            },
        );
        assert_eq!(
            fixture.command(json!({"op":"requests"})).unwrap().result[0]["status"],
            "Stale"
        );
        assert!(fixture
            .command(json!({"op":"decide_request","request_id":request.id,"decision":"Approve"}))
            .is_err());
        assert!(fixture
            .command(json!({"op":"decide_request","request_id":request.id,"decision":"Deny"}))
            .is_ok());
    }

    #[test]
    fn revocation_controls_cannot_revoke_another_kind_of_grant() {
        let mut fixture = Fixture::new();
        let request = fixture.proposal();
        fixture.append(
            "sp_chat",
            "agent",
            EventBody::RequestOpened {
                request: request.clone(),
            },
        );
        fixture
            .command(
                json!({"op":"decide_request","request_id":request.id,"decision":"AlwaysApprove"}),
            )
            .unwrap();
        let count = fixture.client.saved.outbox.len();
        assert!(fixture
            .command(json!({"op":"revoke_app_device","grant_id":"gr_standing_rq_proposal"}))
            .is_err());
        assert_eq!(fixture.client.saved.outbox.len(), count);
        let installed = fixture
            .client
            .app_request(
                &json!({"op":"install_app","space_id":"sp_chat","app_id":"list","args_json":"{}"})
                    .to_string(),
                fixture.now + 1,
                "pt-BR",
            )
            .unwrap()
            .result;
        let item = installed["id"].as_str().unwrap();
        let grant = fixture
            .client
            .grant_browser_app_device(
                item,
                "photos.pick",
                "Choose only these photos",
                false,
                fixture.now + 2,
            )
            .unwrap();
        let count = fixture.client.saved.outbox.len();
        assert!(fixture
            .command(json!({"op":"revoke_standing","grant_id":grant}))
            .is_err());
        assert_eq!(fixture.client.saved.outbox.len(), count);
        fixture
            .command(json!({"op":"revoke_app_device","grant_id":grant}))
            .unwrap();
        assert!(!fixture
            .client
            .active_app_grants(item, fixture.now)
            .unwrap()
            .iter()
            .any(|value| value.id == grant));
    }
}
