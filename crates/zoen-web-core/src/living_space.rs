//! Living Space projection and access policy over verified history and accepted outbox.
use crate::client::Client;
use openmls_traits::signatures::Signer;
use roda_types::{
    living_space::LivingSpaceExperiment, EventBody, ItemContent, ItemKind, Role, SpaceKind,
};
use serde_json::{json, Value};
const PREFIX: &str = "zoen-living-space-meta-v1:";

fn merge_tier_delta(state: &mut Value, delta: &Value) {
    if !state["invites"].is_array() {
        state["invites"] = json!([]);
    }
    for incoming in delta["invites"].as_array().into_iter().flatten() {
        let invites = state["invites"].as_array_mut().expect("invite array");
        if let Some(prior) = invites.iter_mut().find(|i| i["id"] == incoming["id"]) {
            if prior["tier_id"] == incoming["tier_id"]
                && prior["issuer"] == incoming["issuer"]
                && prior["digest"] == incoming["digest"]
            {
                prior["revoked"] = json!(
                    prior["revoked"].as_bool().unwrap_or(false)
                        || incoming["revoked"].as_bool().unwrap_or(false)
                );
            }
        } else {
            invites.push(incoming.clone());
        }
    }
    for incoming in delta["tiers"].as_array().into_iter().flatten() {
        let tiers = state["tiers"].as_array_mut().expect("tier array");
        if let Some(prior) = tiers.iter_mut().find(|t| t["id"] == incoming["id"]) {
            if prior["space"] != incoming["space"] {
                continue;
            }
            for field in ["groups", "resources"] {
                let mut merged = prior[field].as_array().cloned().unwrap_or_default();
                for value in incoming[field].as_array().into_iter().flatten() {
                    if !merged.contains(value) {
                        merged.push(value.clone());
                    }
                }
                prior[field] = json!(merged);
            }
            prior["name"] = incoming["name"].clone();
        } else {
            tiers.push(incoming.clone());
        }
    }
    for grant in delta["grants"].as_array().into_iter().flatten() {
        if !matches!(grant["source"].as_str(), Some("manual" | "invite_link")) {
            continue;
        }
        let entries = state["entitlements"]
            .as_array_mut()
            .expect("entitlement array");
        entries.retain(|e| {
            !(e["identity_id"] == grant["identity_id"] && e["tier_id"] == grant["tier_id"])
        });
        entries.push(grant.clone());
    }
    for revoke in delta["revokes"].as_array().into_iter().flatten() {
        state["entitlements"]
            .as_array_mut()
            .expect("entitlement array")
            .retain(|e| {
                !(e["identity_id"] == revoke["identity_id"] && e["tier_id"] == revoke["tier_id"])
            });
    }
    let entitlements = state["entitlements"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for tier in state["tiers"].as_array_mut().expect("tier array") {
        tier["members"] = json!(entitlements
            .iter()
            .filter(|e| e["tier_id"] == tier["id"])
            .map(|e| e["identity_id"].clone())
            .collect::<Vec<_>>());
    }
}

impl<S: Signer> Client<S> {
    pub(crate) fn living_tier_allows(&self, space: &str) -> bool {
        for root in self.living_roots() {
            let mut roles = std::collections::BTreeMap::new();
            let mut state = json!({"tiers":[],"entitlements":[]});
            let mut child = false;
            let Ok(events) = self.living_events(&root) else {
                continue;
            };
            for e in events {
                match &e.body {
                    EventBody::SpaceCreated { .. } => {
                        roles.insert(e.author.clone(), Role::Owner);
                    }
                    EventBody::MemberAdded { identity, role } => {
                        roles.insert(identity.clone(), *role);
                    }
                    EventBody::MemberRemoved { identity } => {
                        roles.remove(identity);
                    }
                    EventBody::MessagePosted { text, .. }
                        if text.starts_with("zoen-living-space-hackathon-v1:")
                            && roles.contains_key(&e.author) =>
                    {
                        if let Ok(value) = serde_json::from_str::<Value>(
                            text.trim_start_matches("zoen-living-space-hackathon-v1:"),
                        ) {
                            if value["action"] == "team_create" && value["space"] == space {
                                child = true;
                            }
                        }
                    }
                    EventBody::MessagePosted { text, .. }
                        if matches!(roles.get(&e.author), Some(Role::Owner | Role::Admin)) =>
                    {
                        if let Some(raw) = text.strip_prefix(PREFIX) {
                            if let Ok(value) = serde_json::from_str::<Value>(raw) {
                                if matches!(
                                    value["action"].as_str(),
                                    Some("register_child" | "conversation_partition")
                                ) && value["child"] == space
                                {
                                    child = true;
                                }
                            }
                        }
                        if let Some(raw) = text.strip_prefix("\u{001e}zoen.space.tiers:") {
                            if let Ok(delta) = serde_json::from_str::<Value>(raw) {
                                merge_tier_delta(&mut state, &delta);
                            }
                        }
                    }
                    _ => {}
                }
            }
            if child
                && (!roles.contains_key(self.living_identity()) || self.page_role(space).is_none())
            {
                return false;
            }
            for tier in state["tiers"].as_array().into_iter().flatten() {
                if tier["space"] == space
                    || tier["groups"]
                        .as_array()
                        .is_some_and(|g| g.iter().any(|g| g == space))
                {
                    let Some(role) = roles.get(self.living_identity()) else {
                        return false;
                    };
                    return self.page_role(space).is_some()
                        && (matches!(role, Role::Owner | Role::Admin)
                            || state["entitlements"].as_array().is_some_and(|es| {
                                es.iter().any(|e| {
                                    e["identity_id"] == self.living_identity()
                                        && e["tier_id"] == tier["id"]
                                        && e["source"] != "paid"
                                })
                            }));
                }
            }
        }
        true
    }

    pub fn living_space_home(&self, space: &str) -> Result<Value, String> {
        let events = self.living_events(space)?;
        let mut roles = std::collections::BTreeMap::new();
        let mut model = None;
        let mut cover = None;
        let mut conversation_cap = 256;
        let mut conversation_groups = vec![];
        let mut hack: Option<roda_types::hackathon::Hackathon> = None;
        let mut tiers = json!({"tiers":[],"entitlements":[],"tier_claims":[]});
        let mut items = std::collections::BTreeMap::new();
        for event in events {
            match &event.body {
                EventBody::SpaceCreated { kind, .. } if *kind == SpaceKind::Community => {
                    roles.insert(event.author.clone(), Role::Owner);
                    model = Some(
                        LivingSpaceExperiment { enabled: true }
                            .create(space.into(), *kind, event.author.clone())
                            .map_err(|e| format!("{e:?}"))?,
                    );
                }
                EventBody::MemberAdded { identity, role } => {
                    roles.insert(identity.clone(), *role);
                }
                EventBody::MemberRemoved { identity } => {
                    roles.remove(identity);
                }
                EventBody::ItemCreated {
                    item,
                    kind,
                    content,
                    ..
                } => {
                    items.insert(item.clone(), (*kind, content.clone()));
                }
                EventBody::ItemVersioned { item, content, .. } => {
                    if let Some(i) = items.get_mut(item) {
                        i.1 = content.clone();
                    }
                }
                EventBody::MessagePosted { text, .. }
                    if text.starts_with("zoen-living-space-hackathon-v1:") =>
                {
                    let Some(role) = roles.get(&event.author).copied() else {
                        continue;
                    };
                    let Ok(value) = serde_json::from_str::<Value>(
                        text.trim_start_matches("zoen-living-space-hackathon-v1:"),
                    ) else {
                        continue;
                    };
                    if value["action"] == "hackathon_create" && hack.is_none() {
                        hack = roda_types::hackathon::Hackathon::create(
                            &event.author,
                            role,
                            event.at_ms,
                            &value,
                        )
                        .ok();
                    } else if let Some(h) = hack.as_mut() {
                        let _ = h.apply(&event.author, role, event.at_ms, &value);
                    }
                }
                EventBody::MessagePosted { text, .. }
                    if text.starts_with("\u{001e}zoen.space.tiers:")
                        && matches!(roles.get(&event.author), Some(Role::Owner | Role::Admin)) =>
                {
                    if let Ok(value) = serde_json::from_str::<Value>(
                        text.trim_start_matches("\u{001e}zoen.space.tiers:"),
                    ) {
                        merge_tier_delta(&mut tiers, &value);
                    }
                }
                EventBody::MessagePosted { text, .. }
                    if matches!(roles.get(&event.author), Some(Role::Owner | Role::Admin)) =>
                {
                    let Some(raw) = text.strip_prefix(PREFIX) else {
                        continue;
                    };
                    let Ok(value) = serde_json::from_str::<Value>(raw) else {
                        continue;
                    };
                    let Some(state) = model.as_mut() else {
                        continue;
                    };
                    state.reconcile_members(roles.clone());
                    match value["action"].as_str() {
                        Some("set_home") => if let Some(id) = value["page_id"].as_str().filter(|id| items.get(*id).is_some_and(|i| i.0 == ItemKind::Page)) { let _ = state.set_home(&event.author, id.into()); },
                        Some("announce") => if let Some(id) = value["item_id"].as_str().filter(|id| items.contains_key(*id)) { let _ = state.publish_announcement(&event.author, id.into()); },
                        Some("set_cover") => if let Some(id) = value["file_id"].as_str().filter(|id| items.get(*id).is_some_and(|i| matches!(&i.1, ItemContent::File(f) if f.mime.starts_with("image/")))) { cover = Some(id.to_owned()); },
                        Some("register_child" | "conversation_partition") => if let Some(id) = value["child"].as_str() { let _ = state.register_child_group(&event.author,id.into(),SpaceKind::Group,roda_types::Privacy::EndToEnd); if value["action"]=="conversation_partition" { conversation_groups.push(id.to_owned()); } },
                        Some("conversation_policy") => if let Some(n)=value["member_cap"].as_u64().filter(|n|(2..=4096).contains(n)) {conversation_cap=n;}
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        let mut model = model.ok_or("Not a Community")?;
        let my_role = roles
            .get(self.living_identity())
            .copied()
            .ok_or("Membership required")?;
        let member_ids: Vec<_> = roles.keys().cloned().collect();
        model.reconcile_members(roles);
        let mut value = serde_json::to_value(
            model
                .home(self.living_identity())
                .map_err(|e| format!("{e:?}"))?,
        )
        .map_err(|e| e.to_string())?;
        value["member_details"] = json!(self.living_member_names(&member_ids));
        value["mutation_status"] = self.living_mutation_status();
        value["cover_file"] = json!(cover);
        value["conversation_space"] = json!(conversation_groups
            .into_iter()
            .find(|g| self.page_role(g).is_some()));
        value["conversation_member_cap"] = json!(conversation_cap);
        value["hackathon"] = hack
            .map(|h| h.projection(self.living_identity(), my_role))
            .unwrap_or(Value::Null);
        let me = self.living_identity();
        let admin = matches!(my_role, Role::Owner | Role::Admin);
        value["entitlements"] = tiers["entitlements"].clone();
        value["tiers"]=json!(tiers["tiers"].as_array().into_iter().flatten().map(|t| {
            let allowed=admin || (t["space"].as_str().is_some_and(|s|self.page_role(s).is_some()) && tiers["entitlements"].as_array().is_some_and(|es|es.iter().any(|e|e["identity_id"]==me && e["tier_id"]==t["id"] && e["source"]!="paid")));
            json!({"id":t["id"],"name":t["name"],"space":if allowed {t["space"].clone()} else {Value::Null},"groups":if allowed {t["groups"].clone()} else {json!([])},"resources":if allowed {t["resources"].clone()} else {json!([])},"members":tiers["entitlements"].as_array().into_iter().flatten().filter(|e|e["tier_id"]==t["id"]).map(|e|e["identity_id"].clone()).collect::<Vec<_>>()})
        }).collect::<Vec<_>>());
        value["tier_claims"] = json!(if admin {
            self.living_tier_claims(space)
        } else {
            vec![]
        });
        value["invitations"]=json!(tiers["invites"].as_array().into_iter().flatten().filter(|_|admin).map(|i|json!({"id":i["id"],"tier_id":i["tier_id"],"expires_at_ms":i["expires_at_ms"],"revoked":i["revoked"]})).collect::<Vec<_>>());
        let announcement_ids = value["announcement_items"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        value["announcement_details"]=json!(announcement_ids.iter().filter_map(|id| {
            let id=id.as_str()?;
            let (kind,content)=items.get(id)?;
            let text=match content {ItemContent::Text {text}=>text.clone(),_=>String::new()};
            Some(json!({"id":id,"title":content.title(),"kind":format!("{kind:?}").to_lowercase(),"text":text}))
        }).collect::<Vec<_>>());
        value["page_details"] = json!(items
            .iter()
            .filter(|(_, (kind, _))| *kind == ItemKind::Page)
            .map(|(id, (_, content))| json!({"id":id,"title":content.title()}))
            .collect::<Vec<_>>());
        value["file_details"] = json!(items
            .iter()
            .filter_map(|(id, (_, content))| match content {
                ItemContent::File(file) =>
                    Some(json!({"id":id,"title":file.name,"mime":file.mime,"bytes":file.bytes})),
                _ => None,
            })
            .collect::<Vec<_>>());
        value["event_details"] = json!(items
            .iter()
            .filter_map(|(id, (_, content))| match content {
                ItemContent::App(app) if app.app == "schedule" =>
                    Some(json!({"id":id,"title":app.title,"state_json":app.state_json})),
                _ => None,
            })
            .collect::<Vec<_>>());
        value["file_items"] = json!(items
            .iter()
            .filter(|(_, (kind, _))| *kind == ItemKind::File)
            .map(|(id, _)| id)
            .collect::<Vec<_>>());
        value["event_items"] = json!(items.iter().filter(|(_, (_, content))| matches!(content, ItemContent::App(app) if app.app == "schedule")).map(|(id, _)| id).collect::<Vec<_>>());
        let requests = if admin {
            self.living_join_requests(space)
        } else {
            vec![]
        };
        value["pending_request_details"] = json!(self.living_member_names(&requests));
        value["pending_requests"] = json!(requests);
        value["conversations"] = json!(self.living_child_views(&model));
        value["read_only"] = json!(false);
        Ok(value)
    }
}
