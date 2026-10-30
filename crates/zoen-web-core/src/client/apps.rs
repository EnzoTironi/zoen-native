//! Browser host for the shared portable mini-app server. No mini-app state machine
//! lives here: miniapps owns creation, tool effects, concurrent reconciliation,
//! schedule roles, resources and privacy projections on every platform.
use super::*;
use grants::{evaluate_permission, Decision, Policy};
use localization::t;
use miniapps as apps;
use serde_json::{json, Value};
use types::{AppDoc, Capability, Grant, GrantScope, ItemContent, ItemKind, TrustLevel};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    AppSpecs {},
    AppResource {
        uri: String,
    },
    InstallApp {
        space_id: String,
        app_id: String,
        args_json: String,
    },
    AppCallTool {
        item_id: String,
        tool: String,
        args_json: String,
        confirmed: bool,
    },
    AppDeviceAllowed {
        item_id: String,
        capability: String,
    },
    GrantAppDevice {
        item_id: String,
        capability: String,
        purpose: String,
        always: bool,
    },
}

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn action_label(action: &types::ActionClass) -> &'static str {
    match action {
        types::ActionClass::Reply => "Responder",
        types::ActionClass::Reversible => "Alteração reversível",
        types::ActionClass::External => "Enviar para fora",
        types::ActionClass::Irreversible => "Ação irreversível",
        types::ActionClass::Money { .. } => "Pagamento",
        types::ActionClass::PublicAudience => "Publicar",
        types::ActionClass::ThirdPartyData => "Usar dados de terceiros",
    }
}

impl<S: Signer> Client<S> {
    pub(super) fn app_state_value(
        &self,
        item: &documents::Item,
        now: i64,
    ) -> Result<Option<Value>, String> {
        let ItemContent::App(document) = &item.current().content else {
            return Ok(None);
        };
        let state: Value = serde_json::from_str(&document.state_json).map_err(error)?;
        let (headline, metrics) = apps::headline(&document.app, &state, now);
        let trust = self
            .active_app_grants(&item.id, now)?
            .into_iter()
            .rev()
            .find_map(|grant| match grant.capability {
                Capability::Trust(level) => Some(level),
                _ => None,
            })
            .unwrap_or(TrustLevel::Listen);
        Ok(Some(
            json!({"app_id":document.app,"resource_uri":document.resource_uri,
            "name":apps::spec(&document.app).map(|spec|spec.name).unwrap_or(""),
            "view_json":apps::view_for(&document.app,&state,now,&self.account.identity).to_string(),
            "headline":headline,"metrics":metrics.into_iter().map(|(label,value)|json!({"label":label,"value":value})).collect::<Vec<_>>(),
            "last_action":apps::last_action(&state),"trust":trust,
            "snapshot_json":apps::snapshot(&document.app,&state,&item.id,now).map(|value|value.to_string()).unwrap_or_default()}),
        ))
    }

    pub fn app_request(
        &mut self,
        request: &str,
        now: i64,
        locale: &str,
    ) -> Result<documents::DocumentResponse, String> {
        self.view_now_ms = now;
        if request.len() > 1 << 20 {
            return Err("Solicitação do app grande demais.".into());
        }
        let _language = localization::enter(localization::Lang::from_tag(locale));
        let command: Command = serde_json::from_str(request).map_err(error)?;
        let mut mutated = false;
        let result=match command {
            Command::AppSpecs{}=>json!(apps::specs().into_iter().map(|spec|json!({"id":spec.id,"name":spec.name,"description":spec.description,
                "resource_uri":spec.resource_uri,"has_view":spec.html.is_some(),
                "tools":spec.tools.into_iter().map(|tool|json!({"name":tool.name,"title":tool.title,"description":tool.description,
                    "input_schema_json":tool.schema.to_string(),"visibility":tool.visibility,"action_label":action_label(&tool.action),"read_only":tool.read_only})).collect::<Vec<_>>() })).collect::<Vec<_>>()),
            Command::AppResource{uri}=>{
                let spec=apps::spec_for_uri(&uri).ok_or("Recurso de interface indisponível.")?;
                let html=spec.html.ok_or("Este app usa uma interface nativa.")?;
                json!({"uri":uri,"mime_type":apps::MIME,"text":html,"prefers_border":false,"manifest_json":spec.manifest.unwrap_or("")})
            }
            Command::InstallApp{space_id,app_id,args_json}=>{
                if app_id=="excalidraw" { return self.install_browser_drawing(&space_id,&args_json,now); }
                let spec=apps::spec(&app_id).ok_or("App desconhecido.")?;
                let args:Value=serde_json::from_str(&args_json).map_err(error)?;
                if !args.is_object(){return Err("Argumentos inválidos.".into())}
                let profile=self.saved.profiles.get(&self.account.identity);
                let actor=apps::Actor{id:&self.account.identity,name:profile.map(|profile|profile.name.as_str()).unwrap_or(&self.account.name),owner:profile.and_then(|profile|profile.owner.as_deref())};
                let (title,state)=apps::create_as(spec.id,&args,&actor,now)?;
                let item=browser_id("it", now)?;
                self.queue_domain_batch(&space_id,vec![
                    EventBody::ItemCreated{item:item.clone(),kind:ItemKind::App,content:ItemContent::App(AppDoc{view:None,document:None,app:app_id,resource_uri:spec.resource_uri.into(),title,state_json:state.to_string()}),origin:"Instalado por você".into()},
                    EventBody::GrantIssued{grant:Grant{id:browser_id("gr", now)?,grantor:self.account.identity.clone(),grantee:Some(format!("app:{item}")),scope:GrantScope::Item(item.clone()),capability:Capability::Trust(TrustLevel::Act),expires_at_ms:None}},
                    EventBody::MessagePosted{message:types::new_secret_id("msg").map_err(|error| error.to_string())?,text:t("App adicionado a este Espaço.", "App added to this Space."),attaches:Some(item.clone()),reply:None,media:None,forwarded:false,forwarded_author:None},
                ],now)?;
                mutated=true;
                self.item_value(&self.document_item(&item)?,None,&mut Vec::new())?
            }
            Command::AppCallTool{item_id,tool,args_json,confirmed}=>{
                let (result,changed)=self.call_browser_app(&item_id,&tool,&args_json,confirmed,now)?;
                mutated=changed; result
            }
            Command::AppDeviceAllowed{item_id,capability}=>json!(self.active_app_grants(&item_id,now)?.into_iter().any(|grant|grant.grantor==self.account.identity&&matches!(grant.capability,Capability::Device{capability:ref actual,..}if *actual==capability))),
            Command::GrantAppDevice{item_id,capability,purpose,always}=>{
                let grant=self.grant_browser_app_device(&item_id,&capability,&purpose,always,now)?;mutated=true;json!(grant)
            }
        };
        Ok(documents::DocumentResponse {
            result,
            update: if mutated {
                Some(self.update(Vec::new())?)
            } else {
                None
            },
            downloads: Vec::new(),
            uploads: Vec::new(),
        })
    }

    fn call_browser_app(
        &mut self,
        id: &str,
        tool_name: &str,
        args_json: &str,
        confirmed: bool,
        now: i64,
    ) -> Result<(Value, bool), String> {
        let item = self.document_item(id)?;
        let ItemContent::App(document) = &item.current().content else {
            return Err("Este arquivo não é um app.".into());
        };
        if document.app == "excalidraw" {
            return Err(localization::t(
                "Este desenho precisa do host nativo de documentos.",
                "This drawing requires the native document host.",
            ));
        }
        let spec = apps::spec(&document.app).ok_or("App desconhecido.")?;
        let tool = spec
            .tools
            .iter()
            .find(|tool| tool.name == tool_name && tool.visibility.contains(&"app"))
            .ok_or("Esta ferramenta não está disponível para a interface.")?;
        let state: Value = serde_json::from_str(&document.state_json).map_err(error)?;
        let current_item = || self.item_value(&item, None, &mut Vec::new());
        let response = |status: &str,
                        text: &str,
                        structured: &Value,
                        is_error: bool,
                        item: Option<Value>,
                        confirmation: bool| {
            json!({
            "status":status,"result_json":apps::call_tool_result(text,structured,is_error).to_string(),"message":text,
            "confirm_title":if confirmation{Some(tool.title)}else{None},"confirm_detail":if confirmation{Some(tool.description)}else{None},"item":item})
        };
        if tool.read_only {
            return Ok((
                response(
                    "Done",
                    "Estado atual.",
                    &apps::view_for(&document.app, &state, now, &self.account.identity),
                    false,
                    Some(current_item()?),
                    false,
                ),
                false,
            ));
        }
        let level = self
            .active_app_grants(id, now)?
            .into_iter()
            .rev()
            .find_map(|grant| match grant.capability {
                Capability::Trust(level) => Some(level),
                _ => None,
            })
            .unwrap_or(TrustLevel::Listen);
        match evaluate_permission(level, &tool.action, &Policy::default()) {
            Decision::Block(_) => {
                return Ok((
                    response(
                        "Denied",
                        "A autonomia deste app não permite esta ação.",
                        &Value::Null,
                        true,
                        None,
                        false,
                    ),
                    false,
                ));
            }
            Decision::Request(_) if !confirmed => {
                return Ok((
                    response(
                        "NeedsConfirmation",
                        "Esta ação exige sua confirmação.",
                        &Value::Null,
                        true,
                        None,
                        true,
                    ),
                    false,
                ));
            }
            _ => {}
        }
        let mut args: Value = serde_json::from_str(args_json).map_err(error)?;
        if !args.is_object() {
            return Err("Argumentos inválidos.".into());
        }
        let profile = self.saved.profiles.get(&self.account.identity);
        let owner = profile.and_then(|profile| profile.owner.as_deref());
        if owner.is_some() {
            apps::pin_client(&mut args, None)?;
        }
        let actor = apps::Actor {
            id: &self.account.identity,
            name: profile
                .map(|profile| profile.name.as_str())
                .unwrap_or(&self.account.name),
            owner,
        };
        let applied = match apps::apply_as(&document.app, tool_name, &args, &state, &actor, now) {
            Ok(applied) => applied,
            Err(reason) => {
                return Ok((
                    response(
                        "Done",
                        &reason,
                        &apps::view_for(&document.app, &state, now, &self.account.identity),
                        true,
                        Some(current_item()?),
                        false,
                    ),
                    false,
                ));
            }
        };
        let next = AppDoc {
            state_json: applied.state.to_string(),
            title: applied
                .title
                .clone()
                .unwrap_or_else(|| document.title.clone()),
            ..document.clone()
        };
        let mut bodies = vec![EventBody::ItemVersioned {
            item: id.into(),
            content: ItemContent::App(next),
            note: applied.note.clone(),
        }];
        // The shared app may change another Item (for example, a poll winner enters
        // the latest Plan). Validate the entire effect before queuing either change.
        if let Some(apps::Effect::AddPlanLine { text, cost_cents }) = &applied.effect {
            if let Some(target) = self
                .document_items()?
                .into_values()
                .filter(|candidate| {
                    candidate.space == item.space && candidate.kind == ItemKind::Plan
                })
                .max_by_key(|candidate| candidate.current().event.at_ms)
            {
                let read = self.plan_read(&target)?;
                if !read.ready || read.error.is_some() || !self.plan_legacy_writable(&target)? {
                    return Err(
                        "O plano precisa do editor de página; o app não foi alterado.".into(),
                    );
                }
                let mut plan = read.document;
                let section = plan
                    .sections
                    .first_mut()
                    .ok_or("O plano não tem uma seção para receber o resultado.")?;
                section.lines.push(types::PlanLine {
                    id: browser_id("ln", now)?,
                    text: text.clone(),
                    cost_cents: *cost_cents,
                    done: false,
                });
                if plan
                    .sections
                    .iter()
                    .flat_map(|section| &section.lines)
                    .try_fold(0_i128, |sum, line| {
                        sum.checked_add(i128::from(line.cost_cents))
                    })
                    .and_then(|sum| i64::try_from(sum).ok())
                    .is_none()
                {
                    return Err(
                        "O total do plano excederia o limite; o app não foi alterado.".into(),
                    );
                }
                bodies.push(EventBody::ItemVersioned {
                    item: target.id,
                    content: ItemContent::Plan(plan),
                    note: applied.note.clone(),
                });
            }
        }
        self.queue_domain_batch(&item.space, bodies, now)?;
        let updated = self.document_item(id)?;
        let view = apps::view_for(&document.app, &applied.state, now, &self.account.identity);
        Ok((
            response(
                "Done",
                &applied.text,
                &view,
                false,
                Some(self.item_value(&updated, None, &mut Vec::new())?),
                false,
            ),
            true,
        ))
    }
}
