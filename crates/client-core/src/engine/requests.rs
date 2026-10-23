//! Canonical request views, decisions and standing grants.
use super::*;

impl Engine {
    pub(crate) fn request_dto(&self, r: &RequestState) -> AgentRequestDto {
        let mut status = match r.status {
            ReqStatus::Pending => RequestStatus::Pending,
            ReqStatus::Approved => RequestStatus::Approved,
            ReqStatus::Denied => {
                if r.resolution.as_ref().and_then(|r| r.tool_decision) == Some(ToolDecision::Cancel)
                {
                    RequestStatus::Cancelled
                } else {
                    RequestStatus::Denied
                }
            }
        };
        if status == RequestStatus::Pending
            && matches!(self.request_currentness(&r.req), Ok(Currentness::Changed))
        {
            status = RequestStatus::Stale;
        }
        let decision = self.decide(&r.req.agent, &r.space, &r.req.action, 0);
        AgentRequestDto {
            tool_approval: r.req.tool_proposal.as_ref().map(|p| ToolApprovalDto {
                content_hash: r.req.content_hash.clone(),
                artifact_id: p.artifact.clone(),
                artifact_title: self.state.items.get(&p.artifact)
                    .and_then(|i|i.versions.iter().find(|version|version.number==p.version))
                    .map(|version|version.content.title()).unwrap_or_default(),
                version: p.version,
                operation: crate::agent_binary_documents::tool_approvals::operation(&p.canonical_tool),
                expires_ms: p.expires_at_ms,
                review: Some(self.tool_review_dto(r)),
            }),
            id: r.req.id.clone(),
            agent: self.persona(&r.req.agent),
            space_id: r.space.clone(),
            space_title: self
                .state
                .spaces
                .get(&r.space)
                .map(|s| s.title.clone())
                .unwrap_or_default(),
            title: r.req.title.clone(),
            detail: r.req.detail.clone(),
            audience: r.req.audience.clone(),
            action_label: action_label(&r.req.action).into(),
            cost_cents: match r.req.action {
                ActionClass::Money { cents } => Some(cents),
                _ => None,
            },
            reason: match decision {
                Decision::Request(reason) | Decision::Block(reason) => {
                    reason_label(&reason, &self.policy)
                }
                Decision::Act { .. } => t(
                    "Pedido aberto antes de um nível de confiança maior",
                    "Request opened before a higher trust level",
                ),
            },
            status,
            opened_ms: r.opened_ms,
            resolved_ms: r.resolved_ms,
            item_id: r.req.item.clone().or_else(|| r.resolution.as_ref().and_then(|r| r.created_item.clone())),
            line_id: r.req.line.clone(),
            action_key: standing_key(&r.req.action).into(),
            can_always_approve: r.req.tool_proposal.is_none() && standing_allow_permitted(&r.req.action, &self.policy),
            by_standing: r.req.tool_proposal.is_none() && r.resolved_ms.is_some_and(|at| {
                let key = standing_key(&r.req.action);
                self.state.grants.iter().any(|g| {
                    g.at_ms <= at
                        && g.grant.grantee.as_deref() == Some(r.req.agent.as_str())
                        && g.grant.scope == GrantScope::Space(r.space.clone())
                        && matches!(&g.grant.capability, Capability::Standing { action, .. } if action == key)
                })
            }),
        }
    }

    pub(crate) fn request_currentness(&self, req: &AgentRequest) -> R<Currentness> {
        if self.approval_replay_needed {
            return Ok(Currentness::Waiting);
        }
        let Some(request) = self.state.requests.get(&req.id) else {
            return Ok(Currentness::Changed);
        };
        if let Some(p) = &req.tool_proposal {
            return Ok(
                if p.expires_at_ms > now_ms()
                    && self.state.tool_review_current(request)
                    && self.state.tool_request_matches(req, &request.space)
                    && self.state.tool_app_act(p, now_ms())
                {
                    Currentness::Current
                } else {
                    Currentness::Changed
                },
            );
        }
        PlanReplay::new(&self.store, &self.logs).current(&self.state, req, &request.space)
    }

    pub(crate) fn request_is_current(&self, req: &AgentRequest) -> bool {
        matches!(self.request_currentness(req), Ok(Currentness::Current))
    }

    // ── pedidos (Atividade) ──

    pub fn requests(&self) -> Vec<AgentRequestDto> {
        let mut v: Vec<AgentRequestDto> = self
            .state
            .request_order
            .iter()
            .filter_map(|id| self.state.requests.get(id))
            .map(|r| self.request_dto(r))
            .collect();
        v.sort_by(|a, b| {
            let rank = |s: RequestStatus| match s {
                RequestStatus::Pending => 0,
                RequestStatus::Stale => 1,
                _ => 2,
            };
            rank(a.status).cmp(&rank(b.status)).then(
                b.resolved_ms
                    .unwrap_or(b.opened_ms)
                    .cmp(&a.resolved_ms.unwrap_or(a.opened_ms)),
            )
        });
        v
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors the RequestOpened event fields one to one"
    )]
    pub fn open_request(
        &mut self,
        space: &str,
        agent: &str,
        title: &str,
        detail: &str,
        audience: &str,
        action: ActionClass,
        link: Option<(String, String)>,
    ) -> R<Option<String>> {
        self.open_request_at(
            space,
            agent,
            now_ms(),
            title,
            detail,
            audience,
            action,
            link,
        )
    }

    /// Abre um pedido **só se** o avaliador mandar pedir. `None` = o agente pode fazer direto.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open_request_at(
        &mut self,
        space: &str,
        agent: &str,
        at_ms: i64,
        title: &str,
        detail: &str,
        audience: &str,
        action: ActionClass,
        link: Option<(String, String)>,
    ) -> R<Option<String>> {
        if let Some((item, _)) = &link {
            if self.item_state(item)?.kind == ItemKind::Plan {
                self.require_legacy_plan(item)?;
            }
        }
        match self.decide(agent, space, &action, 0) {
            Decision::Act { .. } => Ok(None),
            Decision::Block(r) => Err(CoreError::Forbidden {
                reason: reason_label(&r, &self.policy),
            }),
            Decision::Request(_) => {
                let content_hash = match &link {
                    Some((item, line)) if self.item_state(item)?.kind == ItemKind::Plan => {
                        let plan = self.plan_snapshot(item)?;
                        Self::line_hash(
                            plan.document
                                .line(line)
                                .ok_or_else(|| not_found(&t("linha", "line")))?,
                        )
                    }
                    Some(_) => content_hash(&title),
                    None => content_hash(&(title, detail, audience)),
                };
                let req = AgentRequest {
                    tool_proposal: None,
                    id: new_id("rq"),
                    agent: agent.into(),
                    title: title.into(),
                    detail: detail.into(),
                    audience: audience.into(),
                    action,
                    content_hash,
                    item: link.as_ref().map(|l| l.0.clone()),
                    line: link.map(|l| l.1),
                    proposal: None,
                };
                let id = req.id.clone();
                self.append_at(
                    space,
                    agent,
                    at_ms,
                    EventBody::RequestOpened { request: req },
                )?;
                Ok(Some(id))
            }
        }
    }

    /// One swipe on the approvals stack. "Sempre" swipes also issue a standing Grant
    /// (agent + kind of action + this Space) and settle the other pending requests it
    /// covers; from then on `decide` applies it without asking.
    pub fn decide_request(&mut self, id: &str, decision: RequestDecision) -> R<DecideOutcome> {
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
        let r = self
            .state
            .requests
            .get(id)
            .cloned()
            .ok_or_else(|| not_found("pedido"))?;
        let (approve, standing) = match decision {
            RequestDecision::Approve => (true, false),
            RequestDecision::Deny => (false, false),
            RequestDecision::AlwaysApprove => (true, true),
            RequestDecision::AlwaysDeny => (false, true),
        };
        if standing && approve && !standing_allow_permitted(&r.req.action, &self.policy) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "Linha vermelha: isso sempre pede. Dá pra aprovar só esta vez.",
                    "Red line: this always asks. You can approve just this once.",
                ),
            });
        }
        let out = self.resolve_request_inner(id, approve, standing)?;
        let mut grant_id = None;
        let mut also = 0u32;
        if standing {
            grant_id = self.state.requests[id]
                .resolution
                .as_ref()
                .and_then(|r| r.standing_grant.as_ref())
                .map(|g| g.id.clone());
            let key = standing_key(&r.req.action);
            let covered: Vec<String> = self
                .state
                .request_order
                .iter()
                .filter_map(|rid| self.state.requests.get(rid))
                .filter(|o| {
                    o.status == ReqStatus::Pending
                        && o.req.tool_proposal.is_none()
                        && o.req.agent == r.req.agent
                        && o.space == r.space
                        && standing_key(&o.req.action) == key
                        && (!approve
                            || (standing_allow_permitted(&o.req.action, &self.policy)
                                && self.request_is_current(&o.req)))
                })
                .map(|o| o.req.id.clone())
                .collect();
            for rid in covered {
                self.resolve_request(&rid, approve)?;
                also += 1;
            }
        }
        let request = self
            .state
            .requests
            .get(id)
            .map(|r| self.request_dto(r))
            .ok_or_else(|| not_found("pedido"))?;
        Ok(DecideOutcome {
            request,
            message: out.message,
            standing_grant_id: grant_id,
            also_resolved: also,
        })
    }

    /// Every standing decision you gave your agents (newest first), to show and revoke.
    pub fn standing_decisions(&self) -> Vec<StandingDecisionDto> {
        let Ok(me) = self.me_id() else { return vec![] };
        self.state
            .grants
            .iter()
            .rev()
            .filter(|g| !g.revoked && g.grant.grantor == me)
            .filter_map(|g| {
                let Capability::Standing { action, allow } = &g.grant.capability else {
                    return None;
                };
                let GrantScope::Space(space) = &g.grant.scope else {
                    return None;
                };
                let agent = g.grant.grantee.as_deref()?;
                Some(StandingDecisionDto {
                    grant_id: g.grant.id.clone(),
                    agent: self.persona(agent),
                    space_id: space.clone(),
                    space_title: self
                        .state
                        .spaces
                        .get(space)
                        .map(|s| s.title.clone())
                        .unwrap_or_default(),
                    action_key: action.clone(),
                    action_label: standing_label(action),
                    allow: *allow,
                    at_ms: g.at_ms,
                })
            })
            .collect()
    }

    /// Revokes a standing decision: the agent asks again next time.
    pub fn revoke_standing(&mut self, grant_id: &str) -> R<()> {
        let g = self
            .state
            .grants
            .iter()
            .find(|g| {
                g.grant.id == grant_id
                    && !g.revoked
                    && matches!(g.grant.capability, Capability::Standing { .. })
            })
            .ok_or_else(|| not_found(&t("decisão", "decision")))?;
        let GrantScope::Space(space) = g.grant.scope.clone() else {
            return Err(CoreError::Invalid {
                reason: t("escopo inválido", "invalid scope"),
            });
        };
        let me = self.me_id()?;
        if g.grant.grantor != me {
            return Err(CoreError::Forbidden {
                reason: t(
                    "só quem decidiu pode revogar",
                    "only whoever decided can revoke it",
                ),
            });
        }
        self.append(
            &space,
            &me,
            EventBody::GrantRevoked {
                grant: grant_id.into(),
            },
        )?;
        Ok(())
    }

    /// Demonstration (showcase seed only): one of your agents asks for something, as if
    /// it came from its run. Goes through the evaluator like any request.
    pub fn demo_open_request(
        &mut self,
        space: &str,
        agent_handle: &str,
        title: &str,
        detail: &str,
        audience: &str,
        action: ActionClass,
    ) -> R<Option<String>> {
        let agent = self
            .identities
            .values()
            .find(|i| i.handle == agent_handle)
            .map(|i| i.id.clone())
            .ok_or_else(|| not_found(&t("agente", "agent")))?;
        self.open_request_at(
            space,
            &agent,
            now_ms(),
            title,
            detail,
            audience,
            action,
            None,
        )
    }

    pub fn approve_all(&mut self, agent: &str) -> R<u32> {
        let ids: Vec<String> = self
            .state
            .request_order
            .iter()
            .filter(|id| {
                self.state
                    .requests
                    .get(*id)
                    .map(|r| {
                        r.req.agent == agent
                            && r.status == ReqStatus::Pending
                            && self.request_is_current(&r.req)
                    })
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        let mut n = 0;
        for id in ids {
            if self.resolve_request(&id, true).is_ok() {
                n += 1;
            }
        }
        Ok(n)
    }
}
