//! App manifests/resources, creation and installation.
use super::*;

impl Engine {
    pub fn app_specs(&self) -> Vec<AppSpecDto> {
        apps::specs()
            .into_iter()
            .map(|s| AppSpecDto {
                id: s.id.into(),
                name: s.name.into(),
                description: s.description.into(),
                resource_uri: s.resource_uri.into(),
                has_view: s.html.is_some(),
                tools: s
                    .tools
                    .into_iter()
                    .map(|t| AppToolDto {
                        name: t.name.into(),
                        title: t.title.into(),
                        description: t.description.into(),
                        input_schema_json: t.schema.to_string(),
                        visibility: t.visibility.iter().map(|v| v.to_string()).collect(),
                        action_label: action_label(&t.action).into(),
                        read_only: t.read_only,
                    })
                    .collect(),
            })
            .collect()
    }

    /// `resources/read` de um recurso `ui://` embutido.
    pub fn read_app_resource(&self, uri: &str) -> R<AppResourceDto> {
        let spec = apps::spec_for_uri(uri)
            .ok_or_else(|| not_found(&t("recurso de interface", "UI resource")))?;
        let html = spec.html.ok_or_else(|| {
            not_found(&t(
                "View MCP (este app é desenhado nativamente)",
                "MCP View (this app is drawn natively)",
            ))
        })?;
        Ok(AppResourceDto {
            uri: uri.into(),
            mime_type: apps::MIME.into(),
            text: html.into(),
            prefers_border: false,
            manifest_json: spec.manifest.unwrap_or("").into(),
        })
    }

    /// O agente chama a ferramenta `model` de um mini-app (ex.: `adopt_pet`): o núcleo
    /// avalia a Concessão do agente, cria o Item, concede ao mini-app "Agir" só neste
    /// Item e o agente posta o cartão na conversa.
    pub fn agent_create_app(
        &mut self,
        space: &str,
        agent: &str,
        start_tool: &str,
        args_json: &str,
        engine_label: &str,
        prompt: &str,
    ) -> R<PlanOutcome> {
        self.atomic_agent_action(|e| {
            e.agent_create_app_inner(space, agent, start_tool, args_json, engine_label, prompt)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn agent_create_app_inner(
        &mut self,
        space: &str,
        agent: &str,
        start_tool: &str,
        args_json: &str,
        engine_label: &str,
        prompt: &str,
    ) -> R<PlanOutcome> {
        if !self.is_member(space, agent) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "o agente não é membro deste Espaço",
                    "the agent is not a member of this Space",
                ),
            });
        }
        let spec = apps::spec_for_start_tool(start_tool).ok_or_else(|| CoreError::Invalid {
            reason: tr!(
                "ferramenta desconhecida: {start_tool}",
                "unknown tool: {start_tool}"
            ),
        })?;
        let args: Value = serde_json::from_str(args_json).map_err(|err| CoreError::Invalid {
            reason: err.to_string(),
        })?;
        let agent_name = self.persona(agent).name;
        let agent_owner = self.identities.get(agent).and_then(|a| a.owner.clone());
        let creator = apps::Actor {
            id: agent,
            name: &agent_name,
            owner: agent_owner.as_deref(),
        };
        let (title, state) = apps::create_as(spec.id, &args, &creator, now_ms())
            .map_err(|reason| CoreError::Invalid { reason })?;
        let doc = AppDoc {
            document: None,
            app: spec.id.into(),
            resource_uri: spec.resource_uri.into(),
            title: title.clone(),
            state_json: state.to_string(),
        };
        let message = match spec.id {
            "gridcraft" => t("Criei a planilha. Podemos editar o mesmo documento aqui.", "Created the spreadsheet. We can edit the same document here."),
            "deckcraft" => t("Criei a apresentação. Podemos editar o mesmo documento aqui.", "Created the presentation. We can edit the same document here."),
            "pet" => t(
                "Ele mora aqui agora. Dá para dar comida, jogar a Corrida do Jumento e revezar quem cuida dele.",
                "He lives here now. You can feed him, play Donkey Dash and take turns looking after him.",
            ),
            "maptap" => t(
                "MapTap. Os mesmos lugares para todo mundo. Um toque cada.",
                "MapTap. Same places for everyone. One tap each.",
            ),
            "recipe" => tr!(
                "Salvei a {}: vegetariana, 35 minutos, dá para ajustar as porções.",
                "Saved the {}: vegetarian, 35 minutes, servings adjustable.",
                title.to_lowercase_first()
            ),
            "hike" => t(
                "Separei três trilhas boas pra sábado, com mapa e fotos. Comparem e votem: quando fechar, eu monto o roteiro com as caronas.",
                "Found three good trails for Saturday, with maps and photos. Compare and vote: once it’s locked in, I’ll plan the day and the rides.",
            ),
            "countdown" => t("Contagem regressiva criada.", "Countdown’s up."),
            "poll" => t(
                "Abri uma enquete. Quando fechar, a vencedora entra no plano.",
                "Poll’s open. When it closes, the winner goes into the plan.",
            ),
            _ => tr!(
                "Criei a lista “{title}”. Marquem o que vocês levam.",
                "Made the list “{title}”. Check off what you’re bringing."
            ),
        };
        let item = new_id("it");
        let doc = self.attach_app_document(space, &item, doc)?;
        let proposal = ItemProposal {
            item,
            kind: ItemKind::App,
            content: ItemContent::App(doc),
            origin: tr!(
                "{engine_label} · {} · a partir de “{prompt}”",
                "{engine_label} · {} · from “{prompt}”",
                spec.resource_uri
            ),
            message,
            ai_cost_cents: 0,
        };
        self.propose_item(
            space,
            agent,
            prompt,
            tr!("Criar o app “{title}”", "Create the app “{title}”"),
            proposal,
        )
    }

    /// Installs a catalog mini-app as the current user, with an Item-scoped grant.
    pub fn install_app(&mut self, space: &str, app_id: &str, args_json: &str) -> R<ItemDetail> {
        if let Some(format) = apps::binary::Format::for_id(app_id) {
            return match format {
                apps::binary::Format::Deckcraft => self.install_deckcraft(space, args_json),
                apps::binary::Format::Gridcraft | apps::binary::Format::Wordcraft => {
                    self.install_binary(space, format, args_json)
                }
            };
        }
        self.atomic_agent_action(|e| e.install_app_inner(space, app_id, args_json))
    }

    pub(super) fn install_deckcraft(&mut self, space: &str, args_json: &str) -> R<ItemDetail> {
        self.install_binary(space, apps::binary::Format::Deckcraft, args_json)
    }
    fn install_binary(
        &mut self,
        space: &str,
        format: apps::binary::Format,
        args_json: &str,
    ) -> R<ItemDetail> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Args {
            #[serde(default)]
            title: Option<serde_json::Value>,
            #[serde(rename = "mutationId")]
            mutation_id: String,
        }
        let invalid = || CoreError::Invalid {
            reason: t(
                "Argumentos do documento inválidos.",
                "Invalid document arguments.",
            ),
        };
        let args: Args = serde_json::from_str(args_json).map_err(|_| invalid())?;
        // Read the original object too: explicit null is not an omitted title.
        let raw: serde_json::Value = serde_json::from_str(args_json).map_err(|_| invalid())?;
        let title = match args.title {
            Some(serde_json::Value::String(title)) => title,
            None if raw.get("title").is_none() => match format {
                apps::binary::Format::Deckcraft => t("Apresentação", "Presentation"),
                apps::binary::Format::Gridcraft => t("Planilha", "Spreadsheet"),
                apps::binary::Format::Wordcraft => t("Documento", "Document"),
            },
            _ => return Err(invalid()),
        };
        let (title, _) = match format {
            // Reserved canonical format is not an installable App without real UI assets.
            apps::binary::Format::Wordcraft => return Err(invalid()),
            apps::binary::Format::Deckcraft => {
                apps::deckcraft::registration::create(&json!({"title":title}))
            }
            apps::binary::Format::Gridcraft => {
                apps::binary::gridcraft_registration::create(&json!({"title":title}))
            }
        }
        .map_err(|reason| CoreError::Invalid { reason })?;
        use base64::Engine as _;
        let initial = base64::engine::general_purpose::STANDARD.encode(format.seed());
        let accepted =
            self.app_binary_create_format(format, space, &title, &initial, &args.mutation_id)?;
        self.item(&accepted.item_id)
    }

    pub(super) fn install_app_inner(
        &mut self,
        space: &str,
        app_id: &str,
        args_json: &str,
    ) -> R<ItemDetail> {
        let me = self.me_id()?;
        if !self.is_member(space, &me) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "você não é membro deste Espaço",
                    "you are not a member of this Space",
                ),
            });
        }
        let spec = apps::specs()
            .into_iter()
            .find(|spec| spec.id == app_id)
            .ok_or_else(|| CoreError::Invalid {
                reason: t("app desconhecido", "unknown app"),
            })?;
        let args: Value = serde_json::from_str(args_json).map_err(|_| CoreError::Invalid {
            reason: t("argumentos inválidos", "invalid arguments"),
        })?;
        if !args.is_object() {
            return Err(CoreError::Invalid {
                reason: t("argumentos inválidos", "invalid arguments"),
            });
        }
        let my_name = self.persona(&me).name;
        // An agent installs for its owner (the schedule's owner is the barber, the agent
        // runs it), the same as when it creates one from a conversation.
        let my_owner = self.identities.get(&me).and_then(|a| a.owner.clone());
        let creator = apps::Actor {
            id: &me,
            name: &my_name,
            owner: my_owner.as_deref(),
        };
        let (title, state) = apps::create_as(spec.id, &args, &creator, now_ms())
            .map_err(|reason| CoreError::Invalid { reason })?;
        let item = new_id("it");
        let document = self.attach_app_document(
            space,
            &item,
            AppDoc {
                document: None,
                app: spec.id.into(),
                resource_uri: spec.resource_uri.into(),
                title,
                state_json: state.to_string(),
            },
        )?;
        self.append(
            space,
            &me,
            EventBody::ItemCreated {
                item: item.clone(),
                kind: ItemKind::App,
                content: ItemContent::App(document),
                origin: t("Instalado por você", "Installed by you"),
            },
        )?;
        self.append(
            space,
            &me,
            EventBody::GrantIssued {
                grant: Grant {
                    id: new_id("gr"),
                    grantor: me.clone(),
                    grantee: Some(format!("app:{item}")),
                    scope: GrantScope::Item(item.clone()),
                    capability: Capability::Trust(TrustLevel::Act),
                    expires_at_ms: None,
                },
            },
        )?;
        self.post_as(
            space,
            &me,
            &t("App adicionado a este Espaço.", "App added to this Space."),
            Some(item.clone()),
        )?;
        self.item(&item)
    }
}
