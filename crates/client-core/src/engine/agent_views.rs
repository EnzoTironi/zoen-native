//! Agent profiles, owner note counts and activity projections.
use super::*;

impl Engine {
    // ── agentes e Concessões ──

    pub fn agents(&self) -> Vec<AgentProfile> {
        self.identity_order
            .iter()
            .filter_map(|id| self.identities.get(id))
            .filter(|i| i.kind == IdentityKind::Agent)
            .map(|i| self.agent_profile(&i.id))
            .collect()
    }

    pub fn agent_profile(&self, agent: &str) -> AgentProfile {
        let budget = self.budget(agent);
        let spaces = self
            .space_order
            .iter()
            .filter_map(|s| self.state.spaces.get(s))
            .filter(|s| s.kind != SpaceKind::Personal && s.members.iter().any(|(m, _)| m == agent))
            .map(|s| AgentSpaceTrust {
                space_id: s.id.clone(),
                space_title: self
                    .space_summary(&s.id)
                    .map(|x| x.title)
                    .unwrap_or(s.title.clone()),
                level: trust_dto(self.trust(agent, &s.id)),
            })
            .collect();
        let resolved: Vec<&RequestState> = self
            .state
            .request_order
            .iter()
            .filter_map(|r| self.state.requests.get(r))
            .filter(|r| r.req.agent == agent)
            .collect();
        let streak = self
            .state
            .resolved_order
            .iter()
            .rev()
            .filter_map(|r| self.state.requests.get(r))
            .filter(|r| r.req.agent == agent)
            .take_while(|r| r.status == ReqStatus::Approved)
            .count() as u32;
        AgentProfile {
            persona: self.persona(agent),
            budget_limit_cents: budget.map(|b| b.limit_cents),
            budget_spent_cents: budget.map(|b| b.spent_cents),
            near_limit: budget.map(|b| b.near_limit()).unwrap_or(false),
            spaces,
            pending_requests: resolved
                .iter()
                .filter(|r| r.status == ReqStatus::Pending)
                .count() as u32,
            approvals_streak: streak,
            runs_on: t(
                "Neste aparelho · Apple Intelligence quando disponível",
                "On this device · Apple Intelligence when available",
            ),
            owner_notes_pending: self.owner_notes_pending(agent),
        }
    }

    /// Your agent's notes asking for you, in your direct chat with it, since you last
    /// wrote there (answering is what clears them).
    pub(super) fn owner_notes_pending(&self, agent: &str) -> u32 {
        let (Some(me), Some(author)) = (self.me.as_deref(), self.identities.get(agent)) else {
            return 0;
        };
        self.state
            .spaces
            .values()
            .filter(|s| s.kind == SpaceKind::Direct && s.members.iter().any(|(m, _)| m == agent))
            .map(|s| {
                crate::agent_notes::pending_owner_notes(
                    me,
                    author,
                    s.entries.iter().map(|e| match &e.body {
                        EntryBody::Message { text, .. } => (e.author.as_str(), Some(text.as_str())),
                        _ => (e.author.as_str(), None),
                    }),
                )
            })
            .sum()
    }

    /// Ações de um agente em todos os Espaços locais, mais recentes primeiro.
    /// O custo vem do `UsageRecorded` que o mesmo agente assinou logo em seguida.
    pub fn agent_activity(&self, agent: &str, limit: usize) -> R<Vec<AgentActivityDto>> {
        if !self.identities.contains_key(agent) {
            return Err(not_found("agente"));
        }
        let mut out: Vec<AgentActivityDto> = Vec::new();
        for (space, log) in &self.logs {
            let title = self
                .space_summary(space)
                .map(|s| s.title)
                .unwrap_or_default();
            let evs = log.events();
            for (i, e) in evs.iter().enumerate() {
                if e.author != agent {
                    continue;
                }
                let (label, detail) = match &e.body {
                    EventBody::MessagePosted { text, attaches, .. } => {
                        let short: String = text.chars().take(90).collect();
                        (
                            if attaches.is_some() {
                                t("Entregou um Item", "Delivered an Item")
                            } else {
                                t("Respondeu", "Replied")
                            },
                            short,
                        )
                    }
                    EventBody::ItemCreated { content, .. } => {
                        (t("Criou", "Created"), content.title())
                    }
                    EventBody::ItemVersioned { note, .. } => (t("Editou", "Edited"), note.clone()),
                    EventBody::RequestOpened { request } => (
                        t("Pediu aprovação", "Asked for approval"),
                        request.title.clone(),
                    ),
                    _ => continue,
                };
                let proposal_cost = match &e.body {
                    EventBody::RequestOpened { request } => {
                        request.proposal.as_ref().map(|p| p.ai_cost_cents)
                    }
                    _ => None,
                };
                let cost = proposal_cost.or_else(|| {
                    evs[i + 1..].iter().take(3).find_map(|n| match &n.body {
                        EventBody::UsageRecorded {
                            agent: a, cents, ..
                        } if a == agent && n.author == agent => Some(*cents),
                        _ => None,
                    })
                });
                out.push(AgentActivityDto {
                    label,
                    detail,
                    space_id: space.clone(),
                    space_title: title.clone(),
                    at_ms: e.at_ms,
                    cost_cents: cost,
                });
            }
        }
        for r in self
            .state
            .requests
            .values()
            .filter(|r| r.req.agent == agent && r.status == ReqStatus::Approved)
        {
            if let Some(item) = r
                .resolution
                .as_ref()
                .and_then(|receipt| receipt.created_item.as_ref())
                .and_then(|id| self.state.items.get(id))
            {
                out.push(AgentActivityDto {
                    label: t("Criou", "Created"),
                    detail: item.current().content.title(),
                    space_id: r.space.clone(),
                    space_title: self
                        .state
                        .spaces
                        .get(&r.space)
                        .map(|s| s.title.clone())
                        .unwrap_or_default(),
                    at_ms: r.resolved_ms.unwrap_or(r.opened_ms),
                    cost_cents: None,
                });
            }
        }
        out.sort_by_key(|a| std::cmp::Reverse(a.at_ms));
        out.truncate(limit);
        Ok(out)
    }
}
