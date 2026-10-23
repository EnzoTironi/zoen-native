//! App and agent tool calls through the existing authority path.
use super::*;

impl Engine {
    /// `tools/call` vindo da interface de um mini-app. Toda chamada passa pelo avaliador
    /// de Concessões: reversível roda (e vira versão do Item); irreversível ou externo
    /// volta como `NeedsConfirmation` e só roda com `confirmed = true` (a folha nativa).
    pub fn app_call_tool(
        &mut self,
        item: &str,
        tool_name: &str,
        args_json: &str,
        confirmed: bool,
    ) -> R<AppCallOutcome> {
        let me = self.me_id()?;
        self.app_call_as(item, &me, tool_name, args_json, confirmed, None)
    }

    /// `tools/call` from this device's agent while it answers `serving` (the author of the
    /// message it is replying to): a booking it makes can only be for that member.
    /// When the agent turn loop gains tools it must route them here with `t.author.id`
    /// as `serving`; `app_call_tool` gives an agent no one to book for.
    pub fn agent_call_tool(
        &mut self,
        item: &str,
        tool_name: &str,
        args_json: &str,
        serving: &str,
    ) -> R<AppCallOutcome> {
        let me = self.me_id()?;
        self.app_call_as(item, &me, tool_name, args_json, false, Some(serving))
    }

    /// The tools an agent may call in `space`: every mini-app tool there that a call
    /// through [`Self::agent_call_tool`] can reach, as each manifest declares it.
    pub fn agent_tools(&self, space: &str) -> Vec<AgentTool> {
        let mut items: Vec<&ItemState> = self
            .state
            .items
            .values()
            .filter(|i| i.space == space)
            .collect();
        items.sort_by(|a, b| a.id.cmp(&b.id));
        let mut out = Vec::new();
        for it in items {
            let ItemContent::App(doc) = &it.current().content else {
                continue;
            };
            let Some(spec) = apps::spec(&doc.app) else {
                continue;
            };
            for tool in spec.tools.iter().filter(|t| t.visibility.contains(&"app")) {
                out.push(AgentTool {
                    item_id: it.id.clone(),
                    item_title: doc.title.clone(),
                    name: tool.name.to_string(),
                    description: tool.description.to_string(),
                    schema: tool.schema.clone(),
                });
            }
        }
        out
    }

    /// Demonstração: a ação de outro membro chegando como se viesse do aparelho dele
    /// (assinada pela identidade dele no log). No produto isto vem pela sincronização.
    pub fn demo_member_app_call(
        &mut self,
        item: &str,
        member_handle: &str,
        tool_name: &str,
        args_json: &str,
    ) -> R<AppCallOutcome> {
        let who = self.member_by_handle(item, member_handle)?;
        self.app_call_as(item, &who, tool_name, args_json, false, None)
    }

    /// Demonstração: mensagem de outro membro (como se tivesse chegado pela sincronização).
    pub fn demo_member_say(
        &mut self,
        space: &str,
        member_handle: &str,
        text: &str,
    ) -> R<TimelineEntry> {
        let who = self
            .personas()
            .into_iter()
            .find(|p| p.handle == member_handle && !p.is_me)
            .map(|p| p.id)
            .ok_or_else(|| not_found("membro"))?;
        self.post_as(space, &who, text, None)
    }

    pub(super) fn member_by_handle(&self, item: &str, handle: &str) -> R<String> {
        let space = self.item_state(item)?.space.clone();
        self.personas()
            .into_iter()
            .find(|p| p.handle == handle && !p.is_me && self.is_member(&space, &p.id))
            .map(|p| p.id)
            .ok_or_else(|| not_found(&t("membro deste Espaço", "member of this Space")))
    }

    pub(crate) fn app_call_as(
        &mut self,
        item: &str,
        actor_id: &str,
        tool_name: &str,
        args_json: &str,
        confirmed: bool,
        serving: Option<&str>,
    ) -> R<AppCallOutcome> {
        let it = self.item_state(item)?.clone();
        let ItemContent::App(doc) = it.current().content.clone() else {
            return Err(CoreError::Invalid {
                reason: t("este Item não é um app", "this Item is not an app"),
            });
        };
        let spec = apps::spec(&doc.app).ok_or_else(|| not_found("app"))?;
        if doc.app == "excalidraw" {
            return Err(CoreError::Invalid {
                reason: t(
                    "use a operação de documento com versão observada",
                    "use the document operation with an observed version",
                ),
            });
        }
        let tool = spec
            .tools
            .iter()
            .find(|t| t.name == tool_name)
            .ok_or_else(|| CoreError::Invalid {
                reason: tr!(
                    "ferramenta desconhecida: {tool_name}",
                    "unknown tool: {tool_name}"
                ),
            })?;
        // MCP Apps: a interface só chama ferramentas com "app" na visibilidade.
        if !tool.visibility.contains(&"app") {
            return Err(CoreError::Forbidden {
                reason: tr!(
                    "{tool_name} não é visível para a interface (visibility)",
                    "{tool_name} is not visible to the UI (visibility)"
                ),
            });
        }
        let me = actor_id.to_string();
        let actor = self.persona(&me).name;
        let now = now_ms();
        let state: Value = serde_json::from_str(&doc.state_json).unwrap_or(Value::Null);
        if tool.read_only {
            let view = apps::view_for(&doc.app, &state, now, &me);
            return Ok(AppCallOutcome {
                status: AppCallStatus::Done,
                result_json: apps::call_tool_result(
                    &t("Estado atual.", "Current state."),
                    &view,
                    false,
                )
                .to_string(),
                message: String::new(),
                confirm_title: None,
                confirm_detail: None,
                item: Some(self.item(item)?),
            });
        }
        let level = self.app_trust(item);
        let unlimited = Budget {
            limit_cents: i64::MAX / 4,
            spent_cents: 0,
        };
        let decision = evaluate(level, &tool.action, 0, &unlimited, &self.policy);
        let denied = |msg: String| AppCallOutcome {
            status: AppCallStatus::Denied,
            result_json: apps::call_tool_result(&msg, &Value::Null, true).to_string(),
            message: msg,
            confirm_title: None,
            confirm_detail: None,
            item: None,
        };
        if !self.app_trust_available(item) {
            return Ok(denied(t(
                "A confiança foi revogada ou expirou.",
                "Trust was revoked or expired.",
            )));
        }
        // An App grant cannot substitute for the caller agent's own current
        // authority. Human interaction keeps the existing single-action review.
        let decision = if self
            .identities
            .get(&me)
            .is_some_and(|i| i.kind == IdentityKind::Agent)
        {
            match (decision, self.decide(&me, &it.space, &tool.action, 0)) {
                (Decision::Block(reason), _) | (_, Decision::Block(reason)) => {
                    Decision::Block(reason)
                }
                (Decision::Request(reason), _) | (_, Decision::Request(reason)) => {
                    Decision::Request(reason)
                }
                (app, _) => app,
            }
        } else {
            decision
        };
        match decision {
            Decision::Block(reason) => return Ok(denied(reason_label(&reason, &self.policy))),
            Decision::Request(reason) if !confirmed => {
                return Ok(AppCallOutcome {
                    status: AppCallStatus::NeedsConfirmation,
                    result_json: apps::call_tool_result(
                        &t(
                            "Esperando sua confirmação.",
                            "Waiting for your confirmation.",
                        ),
                        &Value::Null,
                        true,
                    )
                    .to_string(),
                    message: reason_label(&reason, &self.policy),
                    confirm_title: Some(tool.title.to_string()),
                    confirm_detail: Some(format!(
                        "{} {}",
                        tool.description,
                        reason_label(&reason, &self.policy)
                    )),
                    item: None,
                });
            }
            _ => {}
        }
        let mut args: Value = serde_json::from_str(args_json).unwrap_or(json!({}));
        let actor_owner = self.identities.get(&me).and_then(|a| a.owner.clone());
        let caller = apps::Actor {
            id: &me,
            name: &actor,
            owner: actor_owner.as_deref(),
        };
        // An agent acts for whoever it is serving, never for a client a message names.
        // Its name comes from the served member's profile, not from whatever the call says.
        let pinned = match (&actor_owner, serving) {
            (Some(_), Some(s)) => {
                // A tool that books for someone books for the member being served unless
                // the call names them: the model never has to know their id.
                if let Some(a) = args
                    .as_object_mut()
                    .filter(|_| tool.schema["properties"].get("forId").is_some())
                {
                    if a.get("forId").is_none_or(Value::is_null) {
                        a.insert("forId".into(), s.into());
                    }
                }
                apps::pin_client(&mut args, Some(s)).map(|_| {
                    if let Some(a) = args.as_object_mut().filter(|a| a.contains_key("forId")) {
                        a.insert("forName".into(), self.persona(s).name.into());
                    }
                })
            }
            (Some(_), None) => apps::pin_client(&mut args, None),
            (None, _) => Ok(()),
        };
        // The same agent call for the same source message already landed (a restart
        // between the call and the host's journal): report it instead of failing on it.
        if let (Ok(()), Some(_), Some(text)) = (
            &pinned,
            &actor_owner,
            apps::replayed(&doc.app, tool_name, &args, &state),
        ) {
            let view = apps::view_for(&doc.app, &state, now, &me);
            return Ok(AppCallOutcome {
                status: AppCallStatus::Done,
                result_json: apps::call_tool_result(&text, &view, false).to_string(),
                message: text,
                confirm_title: None,
                confirm_detail: None,
                item: Some(self.item(item)?),
            });
        }
        // Serving someone, the agent may only do what they could do themselves (a client
        // can't resolve requests or change the hours by asking it to).
        let pinned = pinned.and_then(|_| match (&actor_owner, serving) {
            (Some(_), Some(s)) => {
                let name = self.persona(s).name;
                let them = apps::Actor {
                    id: s,
                    name: &name,
                    owner: None,
                };
                apps::apply_as(&doc.app, tool_name, &args, &state, &them, now).map(|_| ())
            }
            _ => Ok(()),
        });
        let applied = match pinned
            .and_then(|_| apps::apply_as(&doc.app, tool_name, &args, &state, &caller, now))
        {
            Ok(a) => a,
            Err(msg) => {
                let view = apps::view_for(&doc.app, &state, now, &me);
                return Ok(AppCallOutcome {
                    status: AppCallStatus::Done,
                    result_json: apps::call_tool_result(&msg, &view, true).to_string(),
                    message: msg,
                    confirm_title: None,
                    confirm_detail: None,
                    item: Some(self.item(item)?),
                });
            }
        };
        let new_doc = AppDoc {
            state_json: applied.state.to_string(),
            title: applied.title.clone().unwrap_or_else(|| doc.title.clone()),
            ..doc.clone()
        };
        let target_plan = if matches!(&applied.effect, Some(apps::Effect::AddPlanLine { .. })) {
            let target = self
                .state
                .items
                .values()
                .filter(|item| item.space == it.space && item.kind == ItemKind::Plan)
                .max_by_key(|item| item.current().at_ms)
                .map(|item| item.id.clone());
            if let Some(target) = &target {
                // Refuse the legacy effect before changing the poll or posting its result.
                self.require_legacy_plan(target)?;
            }
            target
        } else {
            None
        };
        self.append(
            &it.space,
            &me,
            EventBody::ItemVersioned {
                item: item.into(),
                content: ItemContent::App(new_doc),
                note: applied.note.clone(),
            },
        )?;
        // O agente que criou o mini-app comenta eventos marcantes (recorde, nome novo…).
        if let Some(comment) = &applied.comment {
            let agent = it.versions[0].author.clone();
            if self.is_member(&it.space, &agent) {
                self.post_as(&it.space, &agent, comment, None)?;
            }
        }
        if let Some(apps::Effect::AddPlanLine { text, cost_cents }) = applied.effect {
            if let Some(plan) = target_plan {
                self.add_plan_line_unbound(&plan, 0, &text, cost_cents)?;
            }
        }
        let view = apps::view_for(&doc.app, &applied.state, now, &me);
        Ok(AppCallOutcome {
            status: AppCallStatus::Done,
            result_json: apps::call_tool_result(&applied.text, &view, false).to_string(),
            message: applied.text,
            confirm_title: None,
            confirm_detail: None,
            item: Some(self.item(item)?),
        })
    }
}

// Rust-only agent host port; native UI exports remain in their original module.
impl crate::Client {
    /// The agent host's `tools/call`, made while answering `serving` (the author of the
    /// message being replied to). Not exported: only the agent runtime calls it.
    pub fn agent_call_tool(
        &self,
        item_id: &str,
        tool: &str,
        args_json: &str,
        serving: &str,
    ) -> Result<AppCallOutcome, CoreError> {
        self.lock()
            .agent_call_tool(item_id, tool, args_json, serving)
    }

    /// The mini-app tools the agent host may call in `space` (through
    /// [`Self::agent_call_tool`]). Not exported.
    pub fn agent_tools(&self, space: &str) -> Vec<crate::engine::AgentTool> {
        self.lock().agent_tools(space)
    }
}
