//! Existing decisions on the same canonical Engine authority.

use super::{
    new_id, standing_allow_permitted, standing_key, t, tr, ActionClass, AgentRequest,
    ApproveOutcome, Capability, CoreError, Currentness, DecisionKind, Engine, EventBody, Grant,
    GrantScope, IdentityKind, ItemKind, ItemProposal, PlanOutcome, ReqStatus, RequestResolution,
    TrustLevel, R,
};

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
            grants::Decision::Block(reason) => Ok(PlanOutcome {
                kind: DecisionKind::Block,
                item: None,
                message: crate::engine::reason_label(&reason, &self.policy),
            }),
            grants::Decision::Request(reason) => {
                let mut request = AgentRequest {
                    tool_proposal: None,
                    id: new_id("rq")?,
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
                request.content_hash =
                    request
                        .proposal_hash(space)
                        .ok_or_else(|| CoreError::Invalid {
                            reason: "The proposed item could not be bound to its review.".into(),
                        })?;
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
            grants::Decision::Act { .. } => self.atomic_agent_action(|e| {
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
                                id: new_id("gr")?,
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
        if self
            .state
            .requests
            .get(id)
            .is_some_and(|r| r.req.tool_proposal.is_some())
        {
            return Err(CoreError::Invalid {
                reason: "Use the exact reviewed-action decision for this request.".into(),
            });
        }
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
                                self.standing(&r.req.agent, &r.space, &r.req.action)
                                    .is_some_and(|(current, allow)| {
                                        current.grant.id == given.id && allow == approve
                                    })
                            })
                        {
                            return Err(CoreError::Forbidden {
                                reason: t(
                                    "A decisão permanente mudou, expirou ou foi revogada.",
                                    "The standing decision changed, expired or was revoked.",
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
        if approve && !self.trust_available(&r.req.agent, &r.space) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "A confiança foi revogada ou expirou.",
                    "Trust was revoked or expired.",
                ),
            });
        }
        if approve {
            match self.request_currentness(&r.req)? {
                Currentness::Current => {}
                Currentness::Waiting => {
                    return Err(CoreError::Forbidden {
                        reason: t(
                            "O conteúdo confirmado ainda está chegando.",
                            "The confirmed content is still arriving.",
                        ),
                    })
                }
                Currentness::Changed => {
                    return Err(CoreError::Stale {
                        reason: t(
                            "O conteúdo ou a autorização mudou. Peça uma nova proposta.",
                            "The content or authorization changed. Request a new proposal.",
                        ),
                    })
                }
            }
        }
        if approve {
            if let Some(item) = &r.req.item {
                if self.item_state(item)?.kind == ItemKind::Plan {
                    self.require_legacy_plan(item)?;
                }
            }
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
            proposal.map_or_else(
                || {
                    tr!(
                        "Aprovado: {}. Aguardando a execução da ferramenta.",
                        "Approved: {}. Waiting for tool execution.",
                        r.req.title
                    )
                },
                |p| p.message.clone(),
            )
        } else {
            tr!("Ok, não vou {}.", "Ok, I won't {}.", r.req.title)
        };
        let receipt = RequestResolution {
            tool_decision: None,
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
            store::StoreError::from(e).into()
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
