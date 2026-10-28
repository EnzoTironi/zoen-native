//! Disposable agent/approval views over the verified Space logs. Permission decisions
//! use grants; writes use the same signed and MLS-encrypted outbox as messages.
//! No agent key, second document store, or approval authority lives in JavaScript.

use super::{
    documents, BTreeMap, BTreeSet, Client, Deserialize, Event, EventBody, OpenMlsProvider, Role,
    Signer, SpaceKind,
};
use grants::{evaluate, standing_allow_permitted, standing_key, Budget, Decision, Policy, Reason};
use serde_json::{json, Value};
use types::{
    ActionClass, AgentRequest, Capability, Grant, GrantScope, IdentityKind, ItemContent, ItemKind,
    RequestResolution, TrustLevel,
};

#[derive(Clone)]
struct GrantRead {
    grant: Grant,
    space: String,
    at: i64,
    revoked: bool,
}
#[derive(Clone)]
struct RequestRead {
    request: AgentRequest,
    space: String,
    opened: i64,
    resolved: Option<i64>,
    approved: Option<bool>,
    receipt: Option<RequestResolution>,
}
#[derive(Default)]
struct Projection {
    rosters: BTreeMap<String, BTreeMap<String, Role>>,
    grants: Vec<GrantRead>,
    requests: BTreeMap<String, RequestRead>,
    items: BTreeMap<String, documents::Item>,
    usage: Vec<(String, i64, i64)>,
    virtual_items: BTreeMap<(String, String), Event>,
}
#[derive(PartialEq)]
enum RequestScope {
    Current,
    Changed,
    Waiting,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Agents {},
    AgentProfile {
        agent_id: String,
    },
    Requests {},
    StandingDecisions {},
    SetTrust {
        agent_id: String,
        space_id: String,
        level: TrustLevel,
    },
    RaiseBudget {
        agent_id: String,
        extra_cents: String,
    },
    DecideRequest {
        request_id: String,
        decision: RequestDecision,
    },
    RevokeStanding {
        grant_id: String,
    },
    PreviewDecisions {
        agent_id: String,
        space_id: String,
    },
    AgentActivity {
        agent_id: String,
    },
    Mentions {},
    AppDeviceGrants {},
    RevokeAppDevice {
        grant_id: String,
    },
}
#[derive(Clone, Copy, Deserialize)]
enum RequestDecision {
    Approve,
    Deny,
    AlwaysApprove,
    AlwaysDeny,
}

fn writer(state: &Projection, space: &str, id: &str) -> bool {
    state
        .rosters
        .get(space)
        .and_then(|roster| roster.get(id))
        .is_some_and(|role| *role != Role::Reader)
}
fn content_kind(kind: ItemKind, content: &ItemContent) -> bool {
    matches!(
        (kind, content),
        (ItemKind::Plan, ItemContent::Plan(_))
            | (ItemKind::Task | ItemKind::Note, ItemContent::Text { .. })
            | (ItemKind::Page, ItemContent::Page(_))
            | (ItemKind::App, ItemContent::App(_))
            | (ItemKind::File, ItemContent::File(_))
    )
}
fn month(ms: i64) -> i64 {
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }) * 12 + m
}
fn reason(reason: &Reason) -> String {
    match reason {
        Reason::TrustTooLow { .. } => {
            "O nível de autonomia não permite esta ação sem uma decisão sua.".into()
        }
        Reason::LeavesTheSpace => {
            "Esta ação é irreversível ou sai da conversa e pede sua aprovação.".into()
        }
        Reason::MoneyAboveCeiling { .. } => {
            "Este valor ultrapassa o limite para pagamentos sem aprovação.".into()
        }
        Reason::NewPublicAudience => {
            "Uma nova audiência pública sempre exige sua aprovação.".into()
        }
        Reason::ThirdPartyData => "O uso de dados de terceiros sempre exige sua aprovação.".into(),
        Reason::OverBudget { .. } => "O orçamento disponível não cobre esta ação.".into(),
        Reason::StandingDeny => {
            "Você decidiu sempre negar este tipo de ação nesta conversa.".into()
        }
    }
}
fn action_label(action: &ActionClass) -> &'static str {
    match action {
        ActionClass::Reply => "Responder",
        ActionClass::Reversible => "Alteração reversível",
        ActionClass::External => "Enviar para fora",
        ActionClass::Irreversible => "Ação irreversível",
        ActionClass::Money { .. } => "Pagamento",
        ActionClass::PublicAudience => "Publicar",
        ActionClass::ThirdPartyData => "Usar dados de terceiros",
    }
}

fn asking_note(text: &str) -> bool {
    let Some(body) = text
        .trim()
        .strip_prefix("```json")
        .and_then(|body| body.strip_suffix("```"))
    else {
        return false;
    };
    serde_json::from_str::<Value>(body.trim())
        .ok()
        .and_then(|value| {
            value.as_object().map(|object| {
                object.len() == 1
                    && (object.contains_key("ask_owner") || object.contains_key("agent_proposal"))
            })
        })
        .unwrap_or(false)
}

impl<S: Signer> Client<S> {
    ///
    /// # Errors
    /// Returns an error for invalid requests, unavailable agents or grants,
    /// insufficient ownership or conversation authority, invalid budgets,
    /// or failed projection, signing, encryption or client updates.
    pub fn agent_request(
        &mut self,
        request: &str,
        now: i64,
    ) -> Result<documents::DocumentResponse, String> {
        if request.len() > 1 << 20 {
            return Err("Pedido muito grande.".into());
        }
        let command: Command = serde_json::from_str(request).map_err(|error| error.to_string())?;
        let revoke_device = matches!(&command, Command::RevokeAppDevice { .. });
        let state = self.agent_projection()?;
        let mut changed = false;
        let result = match command {
            Command::Agents {} => Value::Array(self.saved.profiles.values().filter(|profile| profile.kind == IdentityKind::Agent).map(|profile| self.agent_profile_value(&state, &profile.id, now)).collect()),
            Command::AgentProfile { agent_id } => { if self.agent_owner(&agent_id).is_none() { return Err("Agente indisponível neste aparelho.".into()); } self.agent_profile_value(&state, &agent_id, now) }
            Command::Requests {} => { let mut requests: Vec<_> = state.requests.values().collect(); requests.sort_by_key(|request| std::cmp::Reverse(request.opened)); Value::Array(requests.into_iter().map(|request| self.agent_request_value(&state, request, now)).collect::<Result<_,_>>()?) }
            Command::StandingDecisions {} => Value::Array(state.grants.iter().rev().filter(|grant| !grant.revoked && grant.grant.grantor == self.account.identity && grant.grant.expires_at_ms.is_none_or(|at|at>now)).filter_map(|grant| {
                let Capability::Standing { action, allow } = &grant.grant.capability else { return None; };
                let GrantScope::Space(space) = &grant.grant.scope else { return None; };
                Some(json!({"grant_id": grant.grant.id,"agent":self.persona_value(grant.grant.grantee.as_deref()?),"space_id":space,"space_title":self.metadata(space).map(|(title,_,_)|title).unwrap_or_default(),"action_key":action,"action_label":action,"allow":allow,"at_ms":grant.at.to_string()}))
            }).collect()),
            Command::SetTrust { agent_id, space_id, level } => {
                if self.agent_owner(&agent_id) != Some(self.account.identity.as_str()) || !writer(&state, &space_id, &self.account.identity) || !state.rosters.get(&space_id).is_some_and(|roster|roster.contains_key(&agent_id)) { return Err("Só o dono pode mudar a autonomia de um agente que participa desta conversa.".into()); }
                self.queue_domain_body(&space_id, &EventBody::GrantIssued { grant: Grant { id: format!("gr_{}",types::new_ulid(now).map_err(|error| error.to_string())?), grantor:self.account.identity.clone(),grantee:Some(agent_id),scope:GrantScope::Space(space_id.clone()),capability:Capability::Trust(level),expires_at_ms:None}},now,Vec::new())?;
                changed=true; Value::Null
            }
            Command::RaiseBudget { agent_id, extra_cents } => {
                let extra=extra_cents.parse::<i64>().ok().filter(|extra| *extra>0).ok_or("Informe um acréscimo positivo dentro do limite.")?;
                let budget=self.agent_budget(&state,&agent_id,now).ok_or("Este orçamento não está disponível para esta conta.")?;
                let limit=budget.limit_cents.checked_add(extra).ok_or("O orçamento ultrapassa o limite permitido.")?;
                let personal=self.logs.keys().find(|space| self.metadata(space).is_some_and(|(_,kind,_)|kind==SpaceKind::Personal) && writer(&state,space,&self.account.identity)).cloned().ok_or("O espaço pessoal ainda não chegou neste aparelho.")?;
                self.queue_domain_body(&personal,&EventBody::GrantIssued{grant:Grant{id:format!("gr_{}",types::new_ulid(now).map_err(|error| error.to_string())?),grantor:self.account.identity.clone(),grantee:Some(agent_id.clone()),scope:GrantScope::Everywhere,capability:Capability::MonthlyBudget{cents:limit},expires_at_ms:None}},now,Vec::new())?;
                changed=true; self.agent_profile_value(&self.agent_projection()?,&agent_id,now)
            }
            Command::DecideRequest { request_id, decision } => { let result=self.decide_browser_request(&request_id,decision,now)?; changed=true; result }
            Command::RevokeStanding { grant_id } | Command::RevokeAppDevice { grant_id } => {
                let grant=state.grants.iter().find(|grant|grant.grant.id==grant_id && !grant.revoked && grant.grant.grantor==self.account.identity).ok_or("Esta permissão não está disponível para esta conta.")?;
                let matching = if revoke_device { matches!((&grant.grant.capability, &grant.grant.scope), (Capability::Device { .. }, GrantScope::Item(_))) } else { matches!((&grant.grant.capability, &grant.grant.scope), (Capability::Standing { .. }, GrantScope::Space(_))) };
                if !matching { return Err("Esta permissão não corresponde ao controle solicitado.".into()); }
                self.queue_domain_body(&grant.space,&EventBody::GrantRevoked{grant:grant_id},now,Vec::new())?; changed=true; Value::Null
            }
            Command::PreviewDecisions { agent_id, space_id } => {
                let cases=[("Responder","Responder quando chamado",ActionClass::Reply),("Editar o plano","Alterar um plano compartilhado",ActionClass::Reversible),("Mandar para fora","Enviar o roteiro por WhatsApp",ActionClass::External),("Pagamento pequeno","Pagar R$ 40,00",ActionClass::Money{cents:4_000}),("Pagamento grande","Reservar por R$ 640,00",ActionClass::Money{cents:64_000}),("Apagar de verdade","Apagar um histórico",ActionClass::Irreversible),("Publicar","Publicar para uma nova audiência",ActionClass::PublicAudience)];
                Value::Array(cases.into_iter().map(|(action,example,class)| { let decision=self.agent_decision(&state,&agent_id,&space_id,&class,now); let (kind,explanation)=match decision {Decision::Act{undoable:true}=>("ActWithUndo","Executa e você pode desfazer.".into()),Decision::Act{undoable:false}=>("Act","Executa dentro das permissões.".into()),Decision::Request(why)=>("Request",reason(&why)),Decision::Block(why)=>("Block",reason(&why))}; json!({"action":action,"example":example,"kind":kind,"explanation":explanation,"red_line":matches!(class,ActionClass::PublicAudience|ActionClass::ThirdPartyData)||matches!(class,ActionClass::Money{cents}if cents>Policy::default().money_ceiling_cents)}) }).collect())
            }
            Command::AgentActivity { agent_id } => {
                if self.agent_owner(&agent_id).is_none(){return Err("Agente indisponível.".into());}
                let mut entries=Vec::new();
                for(space,log)in &self.logs {for(index,event)in log.events().iter().enumerate(){if event.author!=agent_id{continue;} let(label,detail)=match &event.body{EventBody::MessagePosted{text,attaches,..}=>(if attaches.is_some(){"Entregou um item"}else{"Respondeu"},text.chars().take(90).collect()),EventBody::ItemCreated{content,..}=>("Criou",content.title()),EventBody::ItemVersioned{note,..}=>("Editou",note.clone()),EventBody::RequestOpened{request}=>("Pediu aprovação",request.title.clone()),_=>continue}; let cost=match &event.body{EventBody::RequestOpened{request}=>request.proposal.as_ref().map(|proposal|proposal.ai_cost_cents),_=>None}.or_else(||log.events()[index+1..].iter().take(3).find_map(|next|match &next.body{EventBody::UsageRecorded{agent,cents,..}if agent==&agent_id && next.author==agent_id=>Some(*cents),_=>None})); entries.push((event.at_ms,json!({"label":label,"detail":detail,"space_id":space,"space_title":self.metadata(space).map(|(title,_,_)|title).unwrap_or_default(),"at_ms":event.at_ms.to_string(),"cost_cents":cost.map(|cents|cents.to_string())})));}}
                entries.sort_by_key(|(at,_)|std::cmp::Reverse(*at));Value::Array(entries.into_iter().take(30).map(|(_,value)|value).collect())
            }
            Command::Mentions {} => {
                // The shared message view owns edits/deletions and threads. The host
                // maps these exact references to TimelineEntry, never a fabricated body.
                let token=format!("@{}",self.account.name.to_lowercase());
                let mut mentions=Vec::new();for chat in self.conversations(){for message in &chat.messages{if message.author!=self.account.identity && !message.deleted && message.text.to_lowercase().contains(&token){mentions.push((message.at_ms,json!({"space_id":chat.id,"space_title":chat.title,"message_id":message.entry.as_ref().unwrap_or(&message.id)})));}}}
                mentions.sort_by_key(|(at,_)|std::cmp::Reverse(*at));Value::Array(mentions.into_iter().map(|(_,value)|value).collect())
            }
            Command::AppDeviceGrants {} => Value::Array(state.grants.iter().rev().filter(|grant|!grant.revoked && grant.grant.grantor==self.account.identity && grant.grant.expires_at_ms.is_none_or(|at|at>now)).filter_map(|grant|{let Capability::Device{capability,purpose}=&grant.grant.capability else{return None;};let GrantScope::Item(item)=&grant.grant.scope else{return None;};let item=state.items.get(item)?;let ItemContent::App(app)=&item.current().content else{return None;};Some(json!({"grant_id":grant.grant.id,"item_id":item.id,"item_title":app.title,"app_name":miniapps::spec(&app.app).map_or(app.app.as_str(), |spec|spec.name),"space_title":self.metadata(&item.space).map(|(title,_,_)|title).unwrap_or_default(),"capability":capability,"purpose":purpose,"always":grant.grant.expires_at_ms.is_none(),"at_ms":grant.at.to_string()}))}).collect()),
        };
        Ok(documents::DocumentResponse {
            result,
            update: if changed {
                Some(self.update(Vec::new())?)
            } else {
                None
            },
            downloads: Vec::new(),
            uploads: Vec::new(),
        })
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

mod mutations;

mod assessment;

mod projection;
