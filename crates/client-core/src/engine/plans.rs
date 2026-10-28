//! Plan creation, guarded edits and undo/version restoration.
use super::{
    money, new_id, not_found, plan_from_dto, t, tr, ts, CoreError, EditOutcome, Engine, EventBody,
    IdentityKind, ItemContent, ItemDetail, ItemKind, ItemProposal, PlanDoc, PlanDto, PlanLine,
    PlanOutcome, Privacy, Role, SpaceId, SpaceKind, UndoToken, R,
};

impl Engine {
    /// O agente transforma um pedido num Item (plano). O plano em si vem do cliente
    /// (Foundation Models no aparelho, ou o planejador local de fallback); o núcleo
    /// decide se o agente pode, assina, guarda e versiona.
    pub fn agent_create_plan(
        &mut self,
        space: &str,
        agent: &str,
        prompt: &str,
        plan: PlanDto,
        engine_label: &str,
        ai_cost_cents: i64,
    ) -> R<PlanOutcome> {
        if !self.is_member(space, agent) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "o agente não é membro deste Espaço",
                    "the agent is not a member of this Space",
                ),
            });
        }
        if ai_cost_cents < 0 {
            return Err(CoreError::Invalid {
                reason: t("custo de IA negativo", "negative AI cost"),
            });
        }
        let doc = plan_from_dto(plan)?;
        if doc.sections.iter().all(|s| s.lines.is_empty()) {
            return Err(CoreError::Invalid {
                reason: t("plano vazio", "empty plan"),
            });
        }
        let total = Self::plan_total(&doc).ok_or_else(|| CoreError::Invalid {
            reason: t(
                "total do plano fora do limite",
                "Plan total exceeds the limit",
            ),
        })?;
        let title = doc.title.clone();
        let message = match doc.budget_cents {
            Some(b) if total <= b => tr!(
                "Montei “{title}”: {} de {} · sobra {}. Toque para editar.",
                "Here’s “{title}”: {} of {} · {} left. Tap to edit.",
                money(total),
                money(b),
                money(b - total)
            ),
            Some(b) => tr!(
                "Montei “{title}”, mas deu {} — passou {} do teto de {}. Quer que eu corte algo?",
                "Here’s “{title}”, but it came to {} — {} over the {} cap. Want me to cut something?",
                money(total),
                money(total - b),
                money(b)
            ),
            None => tr!(
                "Montei “{title}” · total {}. Toque para editar.",
                "Here’s “{title}” · total {}. Tap to edit.",
                money(total)
            ),
        };
        let proposal = ItemProposal {
            item: new_id("it")?,
            kind: ItemKind::Plan,
            content: ItemContent::Plan(doc),
            origin: tr!(
                "{engine_label} · a partir de “{prompt}”",
                "{engine_label} · from “{prompt}”"
            ),
            message,
            ai_cost_cents,
        };
        self.propose_item(
            space,
            agent,
            prompt,
            tr!("Criar o plano “{title}”", "Create the plan “{title}”"),
            proposal,
        )
    }

    pub(crate) fn personal_space(&self) -> Option<SpaceId> {
        let me = self.me.as_deref()?;
        self.space_order
            .iter()
            .find(|s| {
                self.state.spaces.get(*s).is_some_and(|space| {
                    space.kind == SpaceKind::Personal
                        && space
                            .members
                            .iter()
                            .any(|(id, role)| id == me && *role == Role::Owner)
                })
            })
            .cloned()
    }

    /// Local signed bootstrap, also used to repair linked databases created before
    /// Personal Spaces were installed there. This does not establish relay sync.
    pub(crate) fn ensure_personal_space(&mut self) -> R<SpaceId> {
        let me = self.me_id()?;
        if let Some(personal) = self.personal_space() {
            return Ok(personal);
        }
        let personal = new_id("sp")?;
        self.append(
            &personal,
            &me,
            EventBody::SpaceCreated {
                title: "Personal".into(),
                kind: SpaceKind::Personal,
                privacy: Privacy::EndToEnd,
            },
        )?;
        Ok(personal)
    }

    pub(super) fn page_changed() -> CoreError {
        CoreError::Stale {
            reason: t(
                "A página mudou. Revise antes de editar.",
                "The page changed. Review it before editing.",
            ),
        }
    }

    /// Edita o plano como "eu". Toda edição é uma versão nova → sempre desfazível.
    /// `observed_hash` é o vínculo que a pessoa viu; `None` só para escritas internas
    /// (ver `add_plan_line_unbound`), que não vêm de uma tela aberta.
    pub(super) fn edit_plan(
        &mut self,
        item: &str,
        observed_hash: Option<&str>,
        note: String,
        f: impl FnOnce(&mut PlanDoc) -> R<()>,
    ) -> R<EditOutcome> {
        let me = self.me_id()?;
        self.require_legacy_plan(item)?;
        let observed = self.plan_page(item)?;
        if !observed.ready {
            return Err(CoreError::Invalid {
                reason: t("o plano ainda não está pronto", "this plan is not ready"),
            });
        }
        if observed_hash.is_some_and(|hash| observed.content_hash != hash) {
            return Err(Self::page_changed());
        }
        let it = self.item_state(item)?;
        let space = it.space.clone();
        let before = it.current().number;
        let ItemContent::Plan(mut doc) = it.current().content.clone() else {
            return Err(CoreError::Invalid {
                reason: t("este Item não é um plano", "this Item is not a plan"),
            });
        };
        let creator = it.versions.initial().author.clone();
        let old_total = Self::plan_total(&doc).ok_or_else(|| CoreError::Invalid {
            reason: t(
                "total do plano fora do limite",
                "Plan total exceeds the limit",
            ),
        })?;
        f(&mut doc)?;
        let new_total = Self::plan_total(&doc).ok_or_else(|| CoreError::Invalid {
            reason: t(
                "total do plano fora do limite",
                "Plan total exceeds the limit",
            ),
        })?;
        let budget = doc.budget_cents;
        self.append(
            &space,
            &me,
            EventBody::ItemVersioned {
                item: item.into(),
                content: ItemContent::Plan(doc),
                note: note.clone(),
            },
        )?;

        // Reação determinística (regra do núcleo, não IA): quem cuida de dinheiro no
        // Espaço comenta quando o total muda. Preferimos o agente "Financeiro" se ele
        // for membro; senão, o agente que criou o plano.
        let mut reaction = None;
        if new_total != old_total {
            let watcher = self
                .space_state(&space)?
                .members
                .iter()
                .map(|(m, _)| m.clone())
                .find(|m| {
                    self.identities
                        .get(m)
                        .is_some_and(|i| i.kind == IdentityKind::Agent && i.handle == "financeiro")
                })
                .or_else(|| {
                    self.identities
                        .get(&creator)
                        .filter(|i| i.kind == IdentityKind::Agent)
                        .map(|i| i.id.clone())
                });
            if let Some(agent) = watcher {
                let text = match budget {
                    Some(b) if new_total <= b => tr!(
                        "Recalculei: {} de {} · sobra {}.",
                        "Recalculated: {} of {} · {} left.",
                        money(new_total),
                        money(b),
                        money(b - new_total)
                    ),
                    Some(b) => tr!(
                        "Atenção: o plano passou {} do teto de {}. Quer que eu ache algo mais barato?",
                        "Heads up: the plan is {} over the {} cap. Want me to find something cheaper?",
                        money(new_total - b),
                        money(b)
                    ),
                    None => tr!(
                        "Recalculei: total {}.",
                        "Recalculated: total {}.",
                        money(new_total)
                    ),
                };
                if self.agent_say(&space, &agent, &text, 0).is_ok() {
                    reaction = Some(text);
                }
            }
        }
        let detail = self.item(item)?;
        // The undo token is the only consumer of the "after" binding. Hash the PlanDoc
        // just written instead of rebuilding the whole page read.
        let binding = self.plan_doc_hash(item)?;
        let result_version = detail.version;
        Ok(EditOutcome {
            item: detail,
            undo: UndoToken {
                item_id: item.into(),
                restore_version: before,
                observed_hash: binding,
                result_version,
                label: note,
            },
            reaction,
        })
    }

    pub fn edit_plan_line(
        &mut self,
        item: &str,
        line: &str,
        text: &str,
        cost_cents: i64,
        observed_hash: &str,
    ) -> R<EditOutcome> {
        let text = text.trim().to_string();
        if text.is_empty() {
            return Err(CoreError::Invalid {
                reason: t("a linha precisa de um texto", "the line needs some text"),
            });
        }
        if cost_cents < 0 {
            return Err(CoreError::Invalid {
                reason: t("custo negativo", "negative cost"),
            });
        }
        let note = tr!("Editou “{}”", "Edited “{}”", text);
        self.edit_plan(item, Some(observed_hash), note, |doc| {
            let l = doc
                .line_mut(line)
                .ok_or_else(|| not_found(&t("linha", "line")))?;
            l.text = text;
            l.cost_cents = cost_cents;
            Ok(())
        })
    }

    pub fn toggle_plan_line(
        &mut self,
        item: &str,
        line: &str,
        observed_hash: &str,
    ) -> R<EditOutcome> {
        let (label, done) = {
            let it = self.item_state(item)?;
            match &it.current().content {
                ItemContent::Plan(p) => {
                    let l = p.line(line).ok_or_else(|| not_found(&t("linha", "line")))?;
                    (l.text.clone(), !l.done)
                }
                _ => {
                    return Err(CoreError::Invalid {
                        reason: t("não é um plano", "not a plan"),
                    });
                }
            }
        };
        let note = format!(
            "{} “{}”",
            if done {
                ts("Concluiu", "Completed")
            } else {
                ts("Reabriu", "Reopened")
            },
            label
        );
        self.edit_plan(item, Some(observed_hash), note, |doc| {
            doc.line_mut(line)
                .ok_or_else(|| not_found(&t("linha", "line")))?
                .done = done;
            Ok(())
        })
    }

    pub fn add_plan_line(
        &mut self,
        item: &str,
        section: u32,
        text: &str,
        cost_cents: i64,
        observed_hash: &str,
    ) -> R<EditOutcome> {
        self.add_plan_line_at(item, section, text, cost_cents, Some(observed_hash))
    }

    /// Internal write with no observed binding. A mini-app effect (`AddPlanLine`) is not
    /// an edit someone made on an open Plan screen, so there is no hash they saw; reading
    /// the current hash just to pass it back would be a check that can never fail. The
    /// plan still has to be ready and legacy-editable, and the edit is a new, undoable
    /// version like any other.
    pub(super) fn add_plan_line_unbound(
        &mut self,
        item: &str,
        section: u32,
        text: &str,
        cost_cents: i64,
    ) -> R<EditOutcome> {
        self.add_plan_line_at(item, section, text, cost_cents, None)
    }

    pub(super) fn add_plan_line_at(
        &mut self,
        item: &str,
        section: u32,
        text: &str,
        cost_cents: i64,
        observed_hash: Option<&str>,
    ) -> R<EditOutcome> {
        let text = text.trim().to_string();
        if text.is_empty() {
            return Err(CoreError::Invalid {
                reason: t("a linha precisa de um texto", "the line needs some text"),
            });
        }
        if cost_cents < 0 {
            return Err(CoreError::Invalid {
                reason: t("custo negativo", "negative cost"),
            });
        }
        let note = tr!("Adicionou “{}”", "Added “{}”", text);
        self.edit_plan(item, observed_hash, note, |doc| {
            let s = doc
                .sections
                .get_mut(section as usize)
                .ok_or_else(|| not_found(&t("seção", "section")))?;
            s.lines.push(PlanLine {
                id: new_id("ln")?,
                text,
                cost_cents,
                done: false,
            });
            Ok(())
        })
    }

    pub fn remove_plan_line(
        &mut self,
        item: &str,
        line: &str,
        observed_hash: &str,
    ) -> R<EditOutcome> {
        let label = match &self.item_state(item)?.current().content {
            ItemContent::Plan(p) => p
                .line(line)
                .map(|l| l.text.clone())
                .ok_or_else(|| not_found(&t("linha", "line")))?,
            _ => {
                return Err(CoreError::Invalid {
                    reason: t("não é um plano", "not a plan"),
                });
            }
        };
        // "Apagar" uma linha é reversível: a versão anterior continua no log.
        self.edit_plan(
            item,
            Some(observed_hash),
            tr!("Removeu “{label}”", "Removed “{label}”"),
            |doc| {
                doc.remove_line(line)
                    .map(|_| ())
                    .ok_or_else(|| not_found(&t("linha", "line")))
            },
        )
    }

    /// Desfazer = nova versão igual à anterior. Nada é apagado.
    pub fn undo(&mut self, token: &UndoToken) -> R<ItemDetail> {
        let it = self.item_state(&token.item_id)?;
        if it.kind != ItemKind::Plan {
            return Err(CoreError::Invalid {
                reason: t("desfazer vale para um plano", "undo applies to a plan"),
            });
        }
        if it.current().number != token.result_version {
            return Err(Self::page_changed());
        }
        let note = tr!(
            "Desfez: {}",
            "Undid: {}",
            token
                .label
                .trim_start_matches("Desfez: ")
                .trim_start_matches("Undid: ")
        );
        self.revert_plan(
            &token.item_id,
            token.restore_version,
            &token.observed_hash,
            &note,
        )
    }

    pub fn restore_plan_version(
        &mut self,
        item: &str,
        version: u32,
        observed_hash: &str,
    ) -> R<ItemDetail> {
        self.revert_plan(
            item,
            version,
            observed_hash,
            &tr!("Restaurou a v{version}", "Restored v{version}"),
        )
    }

    pub(super) fn revert_plan(
        &mut self,
        item: &str,
        version: u32,
        observed_hash: &str,
        note: &str,
    ) -> R<ItemDetail> {
        let me = self.me_id()?;
        let it = self.item_state(item)?;
        if it.kind != ItemKind::Plan {
            return Err(CoreError::Invalid {
                reason: t("este Item não é um plano", "this Item is not a plan"),
            });
        }
        if !it.versions.iter().any(|v| v.number == version) {
            return Err(not_found(&t("versão", "version")));
        }
        self.require_legacy_plan(item)?;
        let observed = self.plan_page(item)?;
        if !observed.ready {
            return Err(CoreError::Invalid {
                reason: t("o plano ainda não está pronto", "this plan is not ready"),
            });
        }
        if observed.content_hash != observed_hash {
            return Err(Self::page_changed());
        }
        let space = it.space.clone();
        self.append(
            &space,
            &me,
            EventBody::ItemReverted {
                item: item.into(),
                to_version: version,
                note: note.into(),
            },
        )?;
        self.item(item)
    }
}
