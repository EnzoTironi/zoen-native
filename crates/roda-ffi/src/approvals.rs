use std::collections::HashMap;

use roda_grants::{standing_allow_permitted, standing_key};
use roda_log::content_hash;
use roda_types::*;

use crate::dto::{ApproveOutcome, DecisionKind, PlanOutcome};
use crate::engine::{
    Engine, Entry, EntryBody, GrantState, ItemState, ReqStatus, RequestState, State, Version,
};
use crate::i18n::t;
use crate::{tr, CoreError};

type R<T> = Result<T, CoreError>;

fn content_matches(kind: ItemKind, content: &ItemContent) -> bool {
    matches!(
        (kind, content),
        (ItemKind::Plan, ItemContent::Plan(_))
            | (ItemKind::Task | ItemKind::Note, ItemContent::Text { .. })
            | (ItemKind::App, ItemContent::App(_))
            | (ItemKind::Page, ItemContent::Page(_))
            | (ItemKind::File, ItemContent::File(_))
    )
}

impl State {
    pub(crate) fn writer(&self, space: &str, identity: &str) -> bool {
        self.spaces.get(space).is_some_and(|s| {
            s.members
                .iter()
                .any(|(id, role)| id == identity && *role != Role::Reader)
        })
    }

    pub(crate) fn request_is_current(&self, req: &AgentRequest, space: &str) -> bool {
        if !self.writer(space, &req.agent) {
            return false;
        }
        if let Some(p) = &req.proposal {
            let valid_content = match (&p.kind, &p.content) {
                (ItemKind::Plan, ItemContent::Plan(doc)) => {
                    doc.sections.iter().any(|s| !s.lines.is_empty())
                }
                (ItemKind::App, ItemContent::App(doc)) => {
                    crate::apps::spec(&doc.app).is_some_and(|spec| {
                        doc.resource_uri == spec.resource_uri
                            && serde_json::from_str::<serde_json::Value>(&doc.state_json).is_ok()
                    })
                }
                _ => false,
            };
            return req.action == ActionClass::Reversible
                && req.item.is_none()
                && req.line.is_none()
                && p.ai_cost_cents >= 0
                && !p.item.is_empty()
                && valid_content
                && !self.items.contains_key(&p.item)
                && req.proposal_hash(space).as_deref() == Some(req.content_hash.as_str());
        }
        match (&req.item, &req.line) {
            (Some(item), Some(line)) => self.items.get(item).is_some_and(|it| {
                it.space == space && matches!(&it.current().content, ItemContent::Plan(p)
                    if p.line(line).is_some_and(|l| content_hash(&(&l.text, l.cost_cents)) == req.content_hash))
            }),
            (None, None) => {
                // Old creation cards did not retain their output. They can be denied,
                // but cannot claim to create a document that was never persisted.
                req.action != ActionClass::Reversible
                    || content_hash(&(&req.title, &req.detail, &req.audience)) == req.content_hash
            }
            _ => false,
        }
    }

    /// Replay and live ingestion use the same authorization checks as local decisions.
    /// A signed but unauthorized event remains in the verified chain without changing
    /// an approval, output or usage projection.
    pub(crate) fn approval_event_valid(
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
                self.writer(&e.space, &e.author)
                    && self
                        .items
                        .get(item)
                        .is_some_and(|it| it.space == e.space && content_matches(it.kind, content))
            }
            EventBody::ItemReverted {
                item, to_version, ..
            } => {
                self.writer(&e.space, &e.author)
                    && self.items.get(item).is_some_and(|it| {
                        it.space == e.space && it.versions.iter().any(|v| v.number == *to_version)
                    })
            }
            EventBody::GrantIssued { grant } => {
                if grant.grantor != e.author
                    || !self.writer(&e.space, &e.author)
                    || self.grants.iter().any(|g| g.grant.id == grant.id)
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
                    && self.writer(&e.space, &e.author)
                    && identities
                        .get(&request.agent)
                        .is_some_and(|i| i.kind == IdentityKind::Agent && i.owner.is_some())
                    && (request.proposal.is_none() || self.request_is_current(request, &e.space))
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
                    || (*approved && !self.request_is_current(&r.req, &r.space))
                {
                    return false;
                }
                let Some(receipt) = resolution else {
                    return r.req.proposal.is_none();
                };
                let proposal = r.req.proposal.as_ref();
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
                            || standing_allow_permitted(
                                &r.req.action,
                                &roda_grants::Policy::default(),
                            ))
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
                },
            });
            s.last_at_ms = s.last_at_ms.max(e.at_ms);
        }
    }
}

impl Engine {
    pub(crate) fn propose_item(
        &mut self,
        space: &str,
        agent: &str,
        prompt: &str,
        title: String,
        proposal: ItemProposal,
    ) -> R<PlanOutcome> {
        if !self.state.writer(space, agent)
            || !self
                .identities
                .get(agent)
                .is_some_and(|a| a.kind == IdentityKind::Agent && a.owner.is_some())
        {
            return Err(CoreError::Forbidden {
                reason: t(
                    "O agente precisa de um dono e permissão para escrever neste Espaço.",
                    "The agent needs an owner and permission to write in this Space.",
                ),
            });
        }
        match self.decide(
            agent,
            space,
            &ActionClass::Reversible,
            proposal.ai_cost_cents,
        ) {
            roda_grants::Decision::Block(reason) => Ok(PlanOutcome {
                kind: DecisionKind::Block,
                item: None,
                message: crate::engine::reason_label(&reason, &self.policy),
            }),
            roda_grants::Decision::Request(reason) => {
                let mut request = AgentRequest {
                    id: new_id("rq"),
                    agent: agent.into(),
                    title,
                    detail: prompt.into(),
                    audience: t("Este Espaço", "This Space"),
                    action: ActionClass::Reversible,
                    content_hash: String::new(),
                    item: None,
                    line: None,
                    proposal: Some(Box::new(proposal)),
                };
                request.content_hash = request.proposal_hash(space).expect("proposal present");
                self.append(space, agent, EventBody::RequestOpened { request })?;
                Ok(PlanOutcome {
                    kind: DecisionKind::Request,
                    item: None,
                    message: tr!(
                        "Preparei uma proposta para sua aprovação · {}",
                        "Prepared a proposal for your approval · {}",
                        crate::engine::reason_label(&reason, &self.policy)
                    ),
                })
            }
            roda_grants::Decision::Act { .. } => self.atomic_agent_action(|e| {
                e.append(
                    space,
                    agent,
                    EventBody::ItemCreated {
                        item: proposal.item.clone(),
                        kind: proposal.kind,
                        content: proposal.content,
                        origin: proposal.origin,
                    },
                )?;
                if proposal.kind == ItemKind::App {
                    let me = e.me_id()?;
                    e.append(
                        space,
                        &me,
                        EventBody::GrantIssued {
                            grant: Grant {
                                id: new_id("gr"),
                                grantor: me.clone(),
                                grantee: Some(format!("app:{}", proposal.item)),
                                scope: GrantScope::Item(proposal.item.clone()),
                                capability: Capability::Trust(TrustLevel::Act),
                                expires_at_ms: None,
                            },
                        },
                    )?;
                }
                e.post_as(space, agent, &proposal.message, Some(proposal.item.clone()))?;
                if proposal.ai_cost_cents > 0 {
                    e.append(
                        space,
                        agent,
                        EventBody::UsageRecorded {
                            agent: agent.into(),
                            cents: proposal.ai_cost_cents,
                            what: "item".into(),
                        },
                    )?;
                }
                Ok(PlanOutcome {
                    kind: DecisionKind::ActWithUndo,
                    item: Some(e.item(&proposal.item)?),
                    message: proposal.message,
                })
            }),
        }
    }

    pub fn resolve_request(&mut self, id: &str, approve: bool) -> R<ApproveOutcome> {
        self.resolve_request_inner(id, approve, false)
    }

    pub(crate) fn resolve_request_inner(
        &mut self,
        id: &str,
        approve: bool,
        standing: bool,
    ) -> R<ApproveOutcome> {
        let me = self.me_id()?;
        let r = self
            .state
            .requests
            .get(id)
            .cloned()
            .ok_or_else(|| CoreError::NotFound {
                what: t("pedido", "request"),
            })?;
        if self
            .identities
            .get(&r.req.agent)
            .and_then(|a| a.owner.as_deref())
            != Some(me.as_str())
            || !self.state.writer(&r.space, &me)
        {
            return Err(CoreError::Forbidden {
                reason: t(
                    "Só o dono do agente neste Espaço pode decidir.",
                    "Only the agent's owner in this Space can decide.",
                ),
            });
        }
        if r.status != ReqStatus::Pending {
            if (r.status == ReqStatus::Approved) == approve {
                if let Some(receipt) = &r.resolution {
                    if !standing || receipt.standing_grant.is_some() {
                        if standing
                            && !receipt.standing_grant.as_ref().is_some_and(|given| {
                                self.state
                                    .grants
                                    .iter()
                                    .any(|g| g.grant.id == given.id && !g.revoked)
                            })
                        {
                            return Err(CoreError::Forbidden {
                                reason: t(
                                    "A decisão permanente foi revogada.",
                                    "The standing decision was revoked.",
                                ),
                            });
                        }
                        return Ok(ApproveOutcome {
                            request: self.request_dto(&r),
                            message: receipt.message.clone(),
                        });
                    }
                }
            }
            return Err(CoreError::Invalid {
                reason: t(
                    "Este pedido já recebeu outra decisão.",
                    "This request already has a different decision.",
                ),
            });
        }
        if approve && !self.state.request_is_current(&r.req, &r.space) {
            return Err(CoreError::Stale {
                reason: t(
                    "O conteúdo ou a autorização mudou. Peça uma nova proposta.",
                    "The content or authorization changed. Request a new proposal.",
                ),
            });
        }
        if standing && approve && !standing_allow_permitted(&r.req.action, &self.policy) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "Esta ação sempre exige uma decisão individual.",
                    "This action always requires an individual decision.",
                ),
            });
        }
        let grant = standing.then(|| Grant {
            id: format!("gr_standing_{}", r.req.id),
            grantor: me.clone(),
            grantee: Some(r.req.agent.clone()),
            scope: GrantScope::Space(r.space.clone()),
            capability: Capability::Standing {
                action: standing_key(&r.req.action).into(),
                allow: approve,
            },
            expires_at_ms: None,
        });
        let proposal = r.req.proposal.as_ref();
        let message = if approve {
            proposal.map(|p| p.message.clone()).unwrap_or_else(|| {
                tr!(
                    "Aprovado: {}. Aguardando a execução da ferramenta.",
                    "Approved: {}. Waiting for tool execution.",
                    r.req.title
                )
            })
        } else {
            tr!("Ok, não vou {}.", "Ok, I won't {}.", r.req.title)
        };
        let receipt = RequestResolution {
            message: message.clone(),
            created_item: if approve {
                proposal.map(|p| p.item.clone())
            } else {
                None
            },
            ai_cost_cents: proposal.map_or(0, |p| p.ai_cost_cents),
            standing_grant: grant,
        };
        self.append(
            &r.space,
            &me,
            EventBody::RequestResolved {
                request: id.into(),
                approved: approve,
                content_hash: r.req.content_hash.clone(),
                resolution: Some(receipt),
            },
        )?;
        Ok(ApproveOutcome {
            request: self.request_dto(&self.state.requests[id]),
            message,
        })
    }

    pub(crate) fn atomic_agent_action<T>(&mut self, f: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        fn storage(e: rusqlite::Error) -> CoreError {
            roda_store::StoreError::from(e).into()
        }
        self.store
            .conn()
            .execute_batch("SAVEPOINT agent_write")
            .map_err(storage)?;
        let result = f(self);
        let result = result.and_then(|value| {
            self.store
                .conn()
                .execute_batch("RELEASE agent_write")
                .map_err(storage)?;
            Ok(value)
        });
        if result.is_err() {
            self.store
                .conn()
                .execute_batch("ROLLBACK TO agent_write; RELEASE agent_write")
                .map_err(storage)?;
            self.reload()?;
        }
        result
    }
}

#[cfg(test)]
mod tests;
