//! Existing projection on the same canonical State authority.

use super::*;

impl State {
    /// Replay and live ingestion use the same authorization checks as local decisions.
    /// A signed but unauthorized event remains in the verified chain without changing
    /// an approval, output or usage projection.
    pub(crate) fn approval_event_valid(
        &self,
        event: &Event,
        identities: &HashMap<IdentityId, Identity>,
        context: &BeforeEvent,
    ) -> Projection {
        if !context.matches(event) || !context.scope_valid() {
            return Projection::Refused;
        }
        if self.waiting_spaces.contains(&event.space) {
            return Projection::Waiting;
        }
        if !self.approval_event_guards(event, identities) {
            return Projection::Refused;
        }
        if let EventBody::RequestResolved {
            request,
            approved: true,
            ..
        } = &event.body
        {
            let request = &self.requests[request];
            return match self.request_currentness(&request.req, &request.space, context.plan()) {
                Currentness::Current => Projection::Applied,
                Currentness::Changed => Projection::Refused,
                Currentness::Waiting => Projection::Waiting,
            };
        }
        Projection::Applied
    }

    pub(crate) fn approval_event_guards(
        &self,
        e: &Event,
        identities: &HashMap<IdentityId, Identity>,
    ) -> bool {
        match &e.body {
            EventBody::ItemCreated {
                item,
                kind,
                content,
                ..
            } => {
                !item.is_empty()
                    && !self.items.contains_key(item)
                    && self.writer(&e.space, &e.author)
                    && content_matches(*kind, content)
            }
            EventBody::ItemVersioned { item, content, .. } => {
                !self.is_recovery_item(item)
                    && self.writer(&e.space, &e.author)
                    && self.items.get(item).is_some_and(|it| {
                        it.space == e.space
                            && it.voice_edit_allowed(&e.author, Some(content))
                            && (content_matches(it.kind, content)
                                || matches!(
                                    (it.kind, content),
                                    (
                                        ItemKind::Plan,
                                        ItemContent::Page(PageContent {
                                            manifest: Some(PageManifest::PlanV1(_)),
                                            ..
                                        })
                                    )
                                ))
                    })
            }
            EventBody::ItemReverted {
                item, to_version, ..
            } => {
                !self.is_recovery_item(item)
                    && self.writer(&e.space, &e.author)
                    && self.items.get(item).is_some_and(|it| {
                        it.space == e.space
                            && it.voice_edit_allowed(&e.author, None)
                            && it.versions.iter().any(|v| v.number == *to_version)
                    })
            }
            EventBody::GrantIssued { grant } => {
                if grant.grantor != e.author
                    || !self.writer(&e.space, &e.author)
                    || self.grants.iter().any(|g| g.grant.id == grant.id)
                    || grant.id.starts_with("gr_app_")
                    || grant.id.starts_with("gr_standing_")
                    || matches!(grant.capability, Capability::MonthlyBudget { cents } if cents < 0)
                {
                    return false;
                }
                if let Some(agent) = grant.grantee.as_ref().and_then(|id| identities.get(id)) {
                    if agent.kind == IdentityKind::Agent
                        && agent.owner.as_deref() != Some(e.author.as_str())
                    {
                        return false;
                    }
                }
                match &grant.scope {
                    GrantScope::Space(space) => space == &e.space,
                    GrantScope::Item(item) => {
                        self.items.get(item).is_some_and(|it| it.space == e.space)
                    }
                    GrantScope::Everywhere => true,
                }
            }
            EventBody::GrantRevoked { grant } => self.grants.iter().any(|g| {
                &g.grant.id == grant
                    && g.grant.grantor == e.author
                    && self.writer(&e.space, &e.author)
            }),
            EventBody::RequestOpened { request } => {
                e.author == request.agent
                    && !request.id.is_empty()
                    && !request.content_hash.is_empty()
                    && !self.requests.contains_key(&request.id)
                    && self.tool_review_successor_valid(request, e.at_ms)
                    && self.writer(&e.space, &e.author)
                    && identities
                        .get(&request.agent)
                        .is_some_and(|i| i.kind == IdentityKind::Agent && i.owner.is_some())
                    && request.tool_proposal.as_ref().is_none_or(|p| {
                        request.proposal.is_none()
                            && self.tool_request_matches(request, &e.space)
                            && p.expires_at_ms > e.at_ms
                            && p.expires_at_ms <= e.at_ms.saturating_add(15 * 60 * 1000)
                            && e.device.as_deref() == Some(p.device.as_str())
                            && identities.get(&request.agent).is_some_and(|i| {
                                i.owner.as_deref() == Some(p.owner.as_str())
                                    && event_log::agent_owner::profile_authorized(i)
                            })
                            && self.tool_app_act(p, e.at_ms)
                    })
                    && (request.proposal.is_none() || self.request_matches(request, &e.space, None))
                    && request.proposal.as_ref().is_none_or(|p| {
                        p.ai_cost_cents == 0
                            || identities
                                .get(&request.agent)
                                .and_then(|agent| agent.owner.as_ref())
                                .is_some_and(|owner| {
                                    self.grants.iter().any(|g| {
                                        !g.revoked
                                            && g.grant.expires_at_ms.is_none_or(|at| at > e.at_ms)
                                            && &g.grant.grantor == owner
                                            && g.grant.grantee.as_deref()
                                                == Some(request.agent.as_str())
                                            && g.grant.scope == GrantScope::Space(e.space.clone())
                                            && matches!(g.grant.capability, Capability::Trust(_))
                                    })
                                })
                    })
            }
            EventBody::ToolReviewRequested {
                request,
                content_hash,
            } => self.tool_review_intent_valid(e, request, content_hash, identities),
            EventBody::RequestResolved {
                request,
                approved,
                content_hash,
                resolution,
            } => {
                let Some(r) = self.requests.get(request) else {
                    return false;
                };
                if r.status != ReqStatus::Pending
                    || r.space != e.space
                    || &r.req.content_hash != content_hash
                    || !self.writer(&e.space, &e.author)
                    || identities
                        .get(&r.req.agent)
                        .and_then(|a| a.owner.as_deref())
                        != Some(e.author.as_str())
                    || (*approved && !self.writer(&r.space, &r.req.agent))
                    || (*approved
                        && match (&r.req.item, &r.req.line) {
                            (Some(item), Some(_)) => !self
                                .items
                                .get(item)
                                .is_some_and(|item| item.space == r.space),
                            (None, None) => false,
                            _ => true,
                        })
                {
                    return false;
                }
                if let Some(p) = &r.req.tool_proposal {
                    let Some(receipt) = resolution else {
                        return false;
                    };
                    return receipt.created_item.is_none()
                        && receipt.ai_cost_cents == 0
                        && receipt.standing_grant.is_none()
                        && matches!(
                            (approved, receipt.tool_decision),
                            (true, Some(ToolDecision::ApproveOnce))
                                | (false, Some(ToolDecision::Deny | ToolDecision::Cancel))
                        )
                        && (!approved
                            || (p.expires_at_ms > e.at_ms && self.tool_app_act(p, e.at_ms)));
                }
                if resolution
                    .as_ref()
                    .is_some_and(|receipt| receipt.tool_decision.is_some())
                {
                    return false;
                }
                let Some(receipt) = resolution else {
                    return r.req.proposal.is_none();
                };
                let proposal = r.req.proposal.as_ref();
                if *approved
                    && proposal.is_some_and(|p| p.kind == ItemKind::App)
                    && self
                        .grants
                        .iter()
                        .any(|g| g.grant.id == format!("gr_app_{}", r.req.id))
                {
                    return false;
                }
                let expected_item = if *approved {
                    proposal.map(|p| &p.item)
                } else {
                    None
                };
                if receipt.created_item.as_ref() != expected_item
                    || receipt.ai_cost_cents != proposal.map_or(0, |p| p.ai_cost_cents)
                    || (*approved && proposal.is_some_and(|p| receipt.message != p.message))
                {
                    return false;
                }
                receipt.standing_grant.as_ref().is_none_or(|g| {
                    g.id == format!("gr_standing_{}", r.req.id)
                        && !self.grants.iter().any(|old| old.grant.id == g.id)
                        && g.grantor == e.author
                        && g.grantee.as_deref() == Some(r.req.agent.as_str())
                        && g.scope == GrantScope::Space(e.space.clone())
                        && g.expires_at_ms.is_none()
                        && matches!(&g.capability, Capability::Standing { action, allow }
                            if action == standing_key(&r.req.action) && allow == approved)
                        && (!*approved
                            || standing_allow_permitted(&r.req.action, &grants::Policy::default()))
                })
            }
            EventBody::UsageRecorded { agent, cents, .. } => {
                *cents >= 0
                    && agent == &e.author
                    && self.writer(&e.space, agent)
                    && identities
                        .get(agent)
                        .is_some_and(|i| i.kind == IdentityKind::Agent && i.owner.is_some())
            }
            _ => true,
        }
    }

    pub(crate) fn apply_resolution(
        &mut self,
        e: &Event,
        r: &RequestState,
        approved: bool,
        receipt: &RequestResolution,
    ) {
        if let Some(grant) = &receipt.standing_grant {
            self.grants.push(GrantState {
                grant: grant.clone(),
                at_ms: e.at_ms,
                revoked: false,
            });
        }
        if approved {
            if let Some(p) = &r.req.proposal {
                self.items.insert(
                    p.item.clone(),
                    ItemState {
                        id: p.item.clone(),
                        space: r.space.clone(),
                        kind: p.kind,
                        origin: p.origin.clone(),
                        has_voice_history: matches!(&p.content, ItemContent::File(file) if file.is_voice()),
                        versions: vec![Version {
                            number: 1,
                            content: p.content.clone(),
                            author: r.req.agent.clone(),
                            at_ms: e.at_ms,
                            note: t("Criado", "Created"),
                            is_undo: false,
                        }],
                    },
                );
                if p.kind == ItemKind::App {
                    self.grants.push(GrantState {
                        grant: Grant {
                            id: format!("gr_app_{}", r.req.id),
                            grantor: e.author.clone(),
                            grantee: Some(format!("app:{}", p.item)),
                            scope: GrantScope::Item(p.item.clone()),
                            capability: Capability::Trust(TrustLevel::Act),
                            expires_at_ms: None,
                        },
                        at_ms: e.at_ms,
                        revoked: false,
                    });
                }
            }
        }
        if let Some(s) = self.spaces.get_mut(&r.space) {
            s.last_at_ms = s.last_at_ms.max(e.at_ms);
            // Canonical tool decisions are control events, not new user messages.
            // The signed RequestResolved still has its ordinary System entry.
            if r.req.tool_proposal.is_some() {
                return;
            }
            s.entries.push(Entry {
                hash: e.hash.clone(),
                client_id: format!("{}:result", e.client_id),
                space: e.space.clone(),
                seq: e.seq,
                author: if approved && r.req.proposal.is_some() {
                    r.req.agent.clone()
                } else {
                    e.author.clone()
                },
                at_ms: e.at_ms,
                body: EntryBody::Message {
                    text: receipt.message.clone(),
                    attaches: receipt.created_item.clone(),
                    reply: None,
                    marks: Default::default(),
                },
            });
        }
    }
}
