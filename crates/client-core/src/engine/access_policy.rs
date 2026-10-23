//! Grant, trust and budget evaluation; no storage authority changes.
use super::*;

impl Engine {
    // ── consultas internas ──

    pub(crate) fn persona(&self, id: &str) -> Persona {
        let me = self.me.as_deref();
        match self.identities.get(id) {
            Some(i) => {
                let owner = i.owner.as_ref().and_then(|o| self.identities.get(o));
                let initials: String = i
                    .name
                    .split_whitespace()
                    .filter_map(|w| w.chars().next())
                    .take(2)
                    .collect::<String>()
                    .to_uppercase();
                let photo = self.profile_view(id).ok();
                Persona {
                    id: i.id.clone(),
                    kind: match i.kind {
                        IdentityKind::Person => PersonaKind::Person,
                        IdentityKind::Agent => PersonaKind::Agent,
                    },
                    name: i.name.clone(),
                    handle: i.handle.clone(),
                    initials,
                    tint_hex: i.tint_hex.clone(),
                    glyph: i.glyph.clone(),
                    bio: i.bio.clone(),
                    owner_id: i.owner.clone(),
                    owner_name: owner.map(|o| o.name.clone()),
                    owner_tint_hex: owner.map(|o| o.tint_hex.clone()),
                    photo_sha256: photo.as_ref().and_then(|p| p.photo_sha256.clone()),
                    photo_ready: photo.as_ref().is_some_and(|p| p.photo_ready),
                    photo_mime: photo.and_then(|p| p.photo_mime),
                    is_me: Some(id) == me,
                    is_mine: i.owner.as_deref() == me && me.is_some(),
                }
            }
            None => Persona {
                id: id.to_string(),
                kind: PersonaKind::Person,
                name: t("Desconhecido", "Unknown"),
                handle: String::new(),
                initials: "?".into(),
                tint_hex: "#8E8E93".into(),
                glyph: None,
                bio: String::new(),
                owner_id: None,
                owner_name: None,
                owner_tint_hex: None,
                photo_sha256: None,
                photo_ready: false,
                photo_mime: None,
                is_me: false,
                is_mine: false,
            },
        }
    }

    pub(crate) fn trust(&self, agent: &str, space: &str) -> TrustLevel {
        let owner = self.identities.get(agent).and_then(|a| a.owner.clone());
        self.state
            .grants
            .iter()
            .rev()
            .filter(|g| {
                !g.revoked
                    && g.grant.expires_at_ms.is_none_or(|at| at > now_ms())
                    && g.grant.grantee.as_deref() == Some(agent)
            })
            .filter(|g| Some(g.grant.grantor.as_str()) == owner.as_deref())
            .filter(|g| g.grant.scope == GrantScope::Space(space.to_string()))
            .find_map(|g| match g.grant.capability {
                Capability::Trust(l) => Some(l),
                _ => None,
            })
            // Padrões do documento: seus agentes nos seus Espaços → Agir; os outros → Sugerir.
            .unwrap_or(if owner.as_deref() == self.me.as_deref() {
                TrustLevel::Act
            } else {
                TrustLevel::Suggest
            })
    }

    /// Orçamento do mês. `None` quando o orçamento não vive neste aparelho
    /// (agente de outra pessoa: quem é dono paga).
    pub(crate) fn budget(&self, agent: &str) -> Option<Budget> {
        let owner = self.identities.get(agent)?.owner.as_deref()?;
        let limit = self
            .state
            .grants
            .iter()
            .rev()
            .filter(|g| !g.revoked && g.grant.grantee.as_deref() == Some(agent))
            .filter(|g| g.grant.grantor == owner)
            .find_map(|g| match g.grant.capability {
                Capability::MonthlyBudget { cents } => Some(cents),
                _ => None,
            })?;
        let month = month_key(now_ms());
        let spent = self
            .state
            .usage
            .iter()
            .filter(|(a, _, at)| a == agent && month_key(*at) == month)
            .fold(0i64, |spent, (_, cents, _)| spent.saturating_add(*cents));
        Some(Budget {
            limit_cents: limit,
            spent_cents: spent,
        })
    }

    pub(crate) fn decide(
        &self,
        agent: &str,
        space: &str,
        action: &ActionClass,
        ai_cost: i64,
    ) -> Decision {
        // Agente de outra pessoa sem orçamento local: o dono dele decide o orçamento.
        let budget = self.budget(agent).unwrap_or(Budget {
            limit_cents: i64::MAX / 4,
            spent_cents: 0,
        });
        let decision = evaluate(
            self.trust(agent, space),
            action,
            ai_cost,
            &budget,
            &self.policy,
        );
        // A standing decision from the owner settles what would otherwise be a request:
        // "always deny" blocks it, "always approve" lets it run (never past a red line).
        let Decision::Request(reason) = decision else {
            return decision;
        };
        match self.standing(agent, space, action) {
            Some((_, false)) => Decision::Block(Reason::StandingDeny),
            Some((_, true)) if standing_allow_permitted(action, &self.policy) => Decision::Act {
                undoable: matches!(action, ActionClass::Reversible),
            },
            _ => Decision::Request(reason),
        }
    }

    /// The newest unrevoked standing decision for this agent, kind of action and Space.
    pub(crate) fn standing(
        &self,
        agent: &str,
        space: &str,
        action: &ActionClass,
    ) -> Option<(&GrantState, bool)> {
        let key = standing_key(action);
        let owner = self.identities.get(agent)?.owner.as_deref()?;
        let now = now_ms();
        self.state
            .grants
            .iter()
            .rev()
            .filter(|g| !g.revoked && g.grant.grantee.as_deref() == Some(agent))
            .filter(|g| g.grant.expires_at_ms.is_none_or(|at| at > now))
            .filter(|g| g.grant.grantor == owner)
            .filter(|g| g.grant.scope == GrantScope::Space(space.to_string()))
            .find_map(|g| match &g.grant.capability {
                Capability::Standing { action, allow } if action == key => Some((g, *allow)),
                _ => None,
            })
    }

    pub fn set_trust(&mut self, agent: &str, space: &str, level: TrustLevel) -> R<()> {
        let me = self.me_id()?;
        let owner = self.identities.get(agent).and_then(|a| a.owner.clone());
        if owner.as_deref() != Some(me.as_str()) {
            let owner_name = owner.map(|o| self.persona(&o).name).unwrap_or_default();
            return Err(CoreError::Forbidden {
                reason: tr!(
                    "Só {owner_name} muda a confiança deste agente.",
                    "Only {owner_name} can change this agent’s trust."
                ),
            });
        }
        if !self.is_member(space, agent) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "o agente não é membro deste Espaço",
                    "the agent is not a member of this Space",
                ),
            });
        }
        let grant = Grant {
            id: new_id("gr"),
            grantor: me.clone(),
            grantee: Some(agent.into()),
            scope: GrantScope::Space(space.into()),
            capability: Capability::Trust(level),
            expires_at_ms: None,
        };
        self.append(space, &me, EventBody::GrantIssued { grant })?;
        Ok(())
    }

    pub fn raise_budget(&mut self, agent: &str, extra_cents: i64) -> R<AgentProfile> {
        let me = self.me_id()?;
        let current = self.budget(agent).ok_or_else(|| CoreError::Forbidden {
            reason: t(
                "este orçamento é de outra pessoa",
                "this budget belongs to someone else",
            ),
        })?;
        let limit_cents = (extra_cents > 0)
            .then(|| current.limit_cents.checked_add(extra_cents))
            .flatten()
            .ok_or_else(|| CoreError::Invalid {
                reason: t(
                    "O aumento deve ser positivo e caber no orçamento.",
                    "The increase must be positive and fit the budget.",
                ),
            })?;
        let personal = self
            .personal_space()
            .ok_or_else(|| not_found(&t("Espaço pessoal", "Personal space")))?;
        let grant = Grant {
            id: new_id("gr"),
            grantor: me.clone(),
            grantee: Some(agent.into()),
            scope: GrantScope::Everywhere,
            capability: Capability::MonthlyBudget { cents: limit_cents },
            expires_at_ms: None,
        };
        self.append(&personal, &me, EventBody::GrantIssued { grant })?;
        Ok(self.agent_profile(agent))
    }

    /// Mostra, sem executar nada, o que o avaliador decidiria para ações típicas.
    pub fn preview_decisions(&self, agent: &str, space: &str) -> Vec<DecisionPreview> {
        let name = self.persona(agent).name;
        let cases: Vec<(&str, &str, ActionClass)> = vec![
            (
                ts("Responder", "Reply"),
                ts("Responder quando chamado", "Reply when called"),
                ActionClass::Reply,
            ),
            (
                ts("Editar o plano", "Edit the plan"),
                ts(
                    "Trocar o restaurante do jantar",
                    "Swap the dinner restaurant",
                ),
                ActionClass::Reversible,
            ),
            (
                ts("Mandar para fora", "Send outside"),
                ts(
                    "Enviar o roteiro por WhatsApp",
                    "Send the itinerary on WhatsApp",
                ),
                ActionClass::External,
            ),
            (
                ts("Pagamento pequeno", "Small payment"),
                ts("Pagar estacionamento de R$ 40", "Pay $40 for parking"),
                ActionClass::Money { cents: 4_000 },
            ),
            (
                ts("Pagamento grande", "Large payment"),
                ts("Reservar a pousada por R$ 640", "Book the inn for $640"),
                ActionClass::Money { cents: 64_000 },
            ),
            (
                ts("Apagar de verdade", "Delete for real"),
                ts("Apagar o histórico do plano", "Delete the plan’s history"),
                ActionClass::Irreversible,
            ),
            (
                ts("Publicar", "Publish"),
                ts(
                    "Postar o roteiro no seu perfil público",
                    "Post the itinerary on your public profile",
                ),
                ActionClass::PublicAudience,
            ),
        ];
        cases
            .into_iter()
            .map(|(action, example, class)| {
                let red_line = matches!(class, ActionClass::PublicAudience | ActionClass::ThirdPartyData)
                    || matches!(class, ActionClass::Money { cents } if cents > self.policy.money_ceiling_cents);
                let d = self.decide(agent, space, &class, 0);
                let (kind, explanation) = match &d {
                    Decision::Act { undoable: true } => (DecisionKind::ActWithUndo, tr!("{name} faz e você pode desfazer", "{name} does it and you can undo")),
                    Decision::Act { undoable: false } => (DecisionKind::Act, tr!("{name} faz sozinho", "{name} does it alone")),
                    Decision::Request(r) => (DecisionKind::Request, reason_label(r, &self.policy)),
                    Decision::Block(r) => (DecisionKind::Block, reason_label(r, &self.policy)),
                };
                DecisionPreview { action: action.into(), example: example.into(), kind, explanation, red_line }
            })
            .collect()
    }
}

impl State {
    pub(crate) fn latest_trust(
        &self,
        space: &str,
        owner: &str,
        agent: &str,
    ) -> Option<&GrantState> {
        // A revoked newest grant cannot revive an earlier, more permissive grant.
        self.grants
            .iter()
            .rev()
            .find(|g| {
                g.grant.grantor == owner
                    && g.grant.grantee.as_deref() == Some(agent)
                    && g.grant.scope == GrantScope::Space(space.into())
                    && matches!(g.grant.capability, Capability::Trust(_))
            })
            .filter(|g| !g.revoked)
    }
}
