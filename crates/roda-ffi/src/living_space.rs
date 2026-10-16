//! Default-off adapter for the living Space experiment. Parent metadata uses signed
//! messages with the parent's existing privacy; join requests use signed MLS DMs.
use crate::{engine::Engine, CoreError, RodaEngine};
use roda_types::{
    living_space::{LivingSpace, LivingSpaceExperiment},
    EventBody, ItemKind, Privacy, Role, SpaceKind,
};
use serde::Deserialize;
use serde_json::{json, Value};

type R<T> = Result<T, CoreError>;
const ENABLED: &str = "experiment.living_space.enabled";
const PREFIX: &str = "zoen-living-space-request-v1:";
pub(crate) const META_PREFIX: &str = "zoen-living-space-meta-v1:";
fn invalid(reason: impl ToString) -> CoreError {
    CoreError::Invalid {
        reason: reason.to_string(),
    }
}
fn forbidden() -> CoreError {
    CoreError::Forbidden {
        reason: "Living Space administrators only".into(),
    }
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Home,
    SetHome { page_id: String },
    SetCover { file_id: String },
    Announce { item_id: String },
    CreateChild { title: String },
    RequestJoin,
    DemoJoinRequest,
    ApproveJoin { identity_id: String },
    RejectJoin { identity_id: String },
}

#[uniffi::export]
impl RodaEngine {
    pub fn set_living_space_enabled(&self, enabled: bool) -> R<()> {
        self.lock()
            .store
            .set_meta(ENABLED, if enabled { "true" } else { "false" })?;
        Ok(())
    }

    pub fn living_space_command(&self, space_id: String, command_json: String) -> R<String> {
        self.lock().living_space_command(&space_id, &command_json)
    }
}

impl Engine {
    pub(crate) fn living_enabled(&self) -> bool {
        self.store.meta(ENABLED).ok().flatten().as_deref() == Some("true")
    }

    /// Confirmed history plus accepted local outbox, in signing order. Metadata becomes
    /// immediately usable offline without treating refused events as authoritative.
    pub(crate) fn living_events(&self, space: &str) -> Vec<roda_types::Event> {
        let mut events = self
            .logs
            .get(space)
            .map(|log| log.events().to_vec())
            .unwrap_or_default();
        let confirmed: std::collections::BTreeSet<_> =
            events.iter().map(|e| e.client_id.clone()).collect();
        if let Ok(pending) = self.store.outbox() {
            events.extend(
                pending
                    .into_iter()
                    .filter(|p| {
                        !p.failed
                            && p.event.space == space
                            && !confirmed.contains(&p.event.client_id)
                    })
                    .map(|p| p.event),
            );
        }
        events
    }

    fn living_replay(&self, space: &str) -> R<(LivingSpace, Option<String>)> {
        let mut roles = std::collections::BTreeMap::new();
        let mut model = None;
        let mut cover = None;
        {
            for event in self.living_events(space) {
                match &event.body {
                    EventBody::SpaceCreated { kind, .. } => {
                        roles.insert(event.author.clone(), Role::Owner);
                        model = Some(
                            LivingSpaceExperiment { enabled: true }
                                .create(space.into(), *kind, event.author.clone())
                                .map_err(|e| invalid(format!("{e:?}")))?,
                        );
                    }
                    EventBody::MemberAdded { identity, role } => {
                        roles.insert(identity.clone(), *role);
                    }
                    EventBody::MemberRemoved { identity } => {
                        roles.remove(identity);
                    }
                    EventBody::MessagePosted { text, .. } => {
                        if !matches!(roles.get(&event.author), Some(Role::Owner | Role::Admin)) {
                            continue;
                        }
                        let Some(raw) = text.strip_prefix(META_PREFIX) else {
                            continue;
                        };
                        let Ok(value) = serde_json::from_str::<Value>(raw) else {
                            continue;
                        };
                        let Some(state) = model.as_mut() else {
                            continue;
                        };
                        state.reconcile_members(roles.clone());
                        let item = value
                            .get("page_id")
                            .or_else(|| value.get("item_id"))
                            .or_else(|| value.get("file_id"))
                            .and_then(Value::as_str);
                        let valid_item = item
                            .and_then(|id| self.state.items.get(id))
                            .filter(|i| i.space == space);
                        match value["action"].as_str() {
                            Some("set_home") if valid_item.is_some_and(|i| i.kind == ItemKind::Page) => { let _ = state.set_home(&event.author, item.unwrap().into()); }
                            Some("announce") if valid_item.is_some() => { let _ = state.publish_announcement(&event.author, item.unwrap().into()); }
                            Some("set_cover") if valid_item.is_some_and(|i| matches!(&i.current().content, roda_types::ItemContent::File(f) if f.mime.starts_with("image/"))) => { cover = item.map(str::to_owned); }
                            Some("register_child" | "conversation_partition") => {
                                if let Some(child) = value["child"].as_str() { let _ = state.register_child_group(&event.author, child.into(), SpaceKind::Group, Privacy::EndToEnd); }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut model = model.ok_or_else(|| invalid("Community log unavailable"))?;
        model.reconcile_members(self.space_state(space)?.members.clone());
        Ok((model, cover))
    }

    pub(crate) fn living_child_hidden(&self, child: &str) -> bool {
        if !self.living_enabled() {
            return false;
        }
        self.state.spaces.keys().any(|parent| {
            self.living_replay(parent)
                .ok()
                .is_some_and(|(s, _)| s.is_child(child))
        })
    }

    pub(crate) fn living_guard_read_space(&self, space: &str, who: &str) -> R<()> {
        self.living_tier_guard_space(space, who)?;
        for (root, parent) in &self.state.spaces {
            if parent.kind != SpaceKind::Community {
                continue;
            }
            let registered = self
                .living_replay(root)
                .ok()
                .is_some_and(|(s, _)| s.is_child(space));
            let team = self
                .living_hackathon_replay(root)
                .is_some_and(|h| h.teams.iter().any(|t| t.space == space));
            if (registered || team) && !self.is_member(root, who) {
                return Err(forbidden());
            }
        }
        Ok(())
    }
    pub(crate) fn living_guard_read_resource(&self, item: &str) -> R<()> {
        self.living_tier_guard_resource(item)?;
        let state = self.item_state(item)?;
        self.living_guard_read_space(&state.space, self.me.as_deref().unwrap_or_default())
    }

    pub(crate) fn living_children_remove(&mut self, root: &str, who: &str) -> R<()> {
        if self.space_state(root)?.kind != SpaceKind::Community {
            return Ok(());
        }
        let model = self.living_replay(root)?.0;
        let hack = self.living_hackathon_replay(root);
        let children: Vec<_> = self
            .state
            .spaces
            .keys()
            .filter(|id| {
                model.is_child(id)
                    || hack
                        .as_ref()
                        .is_some_and(|h| h.teams.iter().any(|t| &t.space == *id))
            })
            .cloned()
            .collect();
        let me = self.me_id()?;
        for child in children {
            if self.is_member(&child, who) {
                if who == me {
                    self.leave_space(&child)?;
                } else {
                    self.remove_member(&child, who)?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn living_guard_write(&self, space: &str, author: &str, body: &EventBody) -> R<()> {
        // Existing partition authorization survives turning the experiment UI off.
        let self_leave = matches!(body, EventBody::MemberRemoved { identity } if identity == author && self.is_member(space, author));
        if !self_leave {
            self.living_guard_read_space(space, author)?;
        }
        if let EventBody::MessagePosted { text, .. } = body {
            if self.living_enabled()
                && self.net.author.is_some()
                && self
                    .state
                    .spaces
                    .get(space)
                    .is_some_and(|s| s.kind == SpaceKind::Community)
                && !text.starts_with(META_PREFIX)
                && !text.starts_with(crate::living_hackathon::HACK_META)
                && !text.starts_with(crate::living_tiers::TIER_META)
            {
                return Err(invalid(
                    "Use the encrypted conversation_space, not the parent Community timeline",
                ));
            }
            if (text.starts_with(META_PREFIX) || text.starts_with(crate::living_tiers::TIER_META))
                && !self
                    .space_state(space)?
                    .members
                    .iter()
                    .any(|(id, role)| id == author && matches!(role, Role::Owner | Role::Admin))
            {
                return Err(forbidden());
            }
        }
        let target = match body {
            EventBody::ItemVersioned { item, .. } | EventBody::ItemReverted { item, .. } => item,
            _ => return Ok(()),
        };
        if let Ok((model, _)) = self.living_replay(space) {
            // Metadata carries IDs only; authority always comes from current log state.
            if model.is_announcement(target)
                && !self
                    .space_state(space)?
                    .members
                    .iter()
                    .any(|(id, role)| id == author && matches!(role, Role::Owner | Role::Admin))
            {
                return Err(forbidden());
            }
        }
        Ok(())
    }

    fn living_direct(&mut self, who: &str) -> R<String> {
        let me = self.me_id()?;
        if let Some(id) = self
            .state
            .spaces
            .values()
            .find(|s| {
                s.kind == SpaceKind::Direct
                    && s.privacy == Privacy::EndToEnd
                    && s.members.len() == 2
                    && s.members.iter().any(|(id, _)| id == who)
                    && s.members.iter().any(|(id, _)| id == &me)
            })
            .map(|s| s.id.clone())
        {
            return Ok(id);
        }
        self.start_direct(who)
    }

    /// Requests are authenticated by the signed event author, never a claimed JSON ID.
    /// Only E2E direct conversations with the current administrator are considered.
    fn living_requests(&self, space: &str) -> Vec<String> {
        let Some(me) = self.me.as_deref() else {
            return vec![];
        };
        let mut requests = std::collections::BTreeSet::new();
        for (id, state) in &self.state.spaces {
            if state.kind != SpaceKind::Direct
                || state.privacy != Privacy::EndToEnd
                || !state.members.iter().any(|(id, _)| id == me)
            {
                continue;
            }
            {
                for event in self.living_events(id) {
                    if let EventBody::MessagePosted { text, .. } = &event.body {
                        if let Some(payload) = text.strip_prefix(PREFIX) {
                            if let Ok(value) = serde_json::from_str::<Value>(payload) {
                                if value.get("space").and_then(Value::as_str) != Some(space) {
                                    continue;
                                }
                                if value.get("status").and_then(Value::as_str) == Some("pending")
                                    && event.author != me
                                {
                                    requests.insert(event.author.clone());
                                } else if event.author == me {
                                    if let Some(who) = value.get("identity").and_then(Value::as_str)
                                    {
                                        requests.remove(who);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        requests
            .into_iter()
            .filter(|who| {
                !self.state.spaces[space]
                    .members
                    .iter()
                    .any(|(id, _)| id == who)
            })
            .collect()
    }

    pub(crate) fn living_space_command(&mut self, space: &str, command_json: &str) -> R<String> {
        if !self.living_enabled() {
            return Err(invalid("Living Space experiment is disabled"));
        }
        let value: Value = serde_json::from_str(command_json).map_err(invalid)?;
        let action = value["action"]
            .as_str()
            .ok_or_else(|| invalid("Action required"))?;
        if action == "conversation_policy" {
            return serde_json::to_string(&self.living_conversation_set_cap(space, &value)?)
                .map_err(invalid);
        }
        if action.starts_with("tier_") || action == "set_access" {
            if self.space_state(space)?.kind != SpaceKind::Community {
                return Err(invalid("Community required"));
            }
            return serde_json::to_string(&self.living_tier_command(space, &value)?)
                .map_err(invalid);
        }
        if matches!(
            action,
            "hackathon_create"
                | "register"
                | "team_create"
                | "team_join"
                | "project_submit"
                | "vote"
                | "judge"
                | "finalize"
                | "schedule_event"
                | "set_judge"
        ) {
            if self.space_state(space)?.kind != SpaceKind::Community {
                return Err(invalid("Community required"));
            }
            return serde_json::to_string(&self.living_hackathon_command(space, &value)?)
                .map_err(invalid);
        }
        let command: Command = serde_json::from_value(value).map_err(invalid)?;
        if matches!(&command, Command::Home) {
            self.living_conversation_ensure(space)?;
        }
        let me = self.me_id()?;
        let parent = self.space_state(space)?;
        if parent.kind != SpaceKind::Community {
            return Err(invalid("A living Space must be a Community"));
        }
        let members = parent.members.clone();
        let admin = members
            .iter()
            .any(|(id, role)| id == &me && matches!(role, Role::Owner | Role::Admin));
        let owner = members
            .iter()
            .find(|(_, role)| *role == Role::Owner)
            .map(|(id, _)| id.clone())
            .ok_or_else(forbidden)?;
        let (mut model, cover) = self.living_replay(space)?;
        let mut metadata = None;
        match command {
            Command::Home => {
                let mut value = serde_json::to_value(model.home(&me).map_err(|_| forbidden())?)
                    .map_err(invalid)?;
                value["pending_requests"] = json!(if admin {
                    self.living_requests(space)
                } else {
                    vec![]
                });
                value["cover_file"] = json!(cover);
                value["conversation_space"] = json!(self.living_conversation_space(space, &me));
                value["conversation_member_cap"] = json!(self.living_conversation_policy(space).0);
                value["hackathon"] = self.living_hackathon_home(space);
                let tier_home = self.living_tier_home(space)?;
                if let (Some(home), Some(tiers)) = (value.as_object_mut(), tier_home.as_object()) {
                    home.extend(tiers.clone());
                }
                value["conversations"] = json!(self
                    .state
                    .spaces
                    .values()
                    .filter(|s| model.is_child(&s.id) && s.members.iter().any(|(id, _)| id == &me))
                    .map(|s| json!({"id":s.id,"title":s.title}))
                    .collect::<Vec<_>>());
                value["file_items"] = json!(self
                    .state
                    .items
                    .values()
                    .filter(|i| i.space == space && i.kind == ItemKind::File)
                    .map(|i| i.id.clone())
                    .collect::<Vec<_>>());
                value["event_items"] = json!(self.state.items.values().filter(|i| i.space == space && matches!(&i.current().content, roda_types::ItemContent::App(app) if app.app == "schedule")).map(|i| i.id.clone()).collect::<Vec<_>>());
                return serde_json::to_string(&value).map_err(invalid);
            }
            Command::DemoJoinRequest => {
                // Capture fixture only: demo identities already have local signing keys.
                // Never accepts an arbitrary actor, and cannot run for a real account.
                if !cfg!(debug_assertions)
                    || self.net.author.is_some()
                    || !admin
                    || self.signers.len() < 2
                {
                    return Err(forbidden());
                }
                let applicant = self
                    .identity_order
                    .iter()
                    .find(|id| {
                        **id != me
                            && self.signers.contains_key(*id)
                            && !members.iter().any(|(m, _)| m == *id)
                            && self
                                .identities
                                .get(*id)
                                .is_some_and(|i| i.kind == roda_types::IdentityKind::Person)
                    })
                    .cloned()
                    .ok_or_else(|| invalid("No demo applicant"))?;
                let dm = roda_types::new_id("sp");
                self.append(
                    &dm,
                    &me,
                    EventBody::SpaceCreated {
                        title: "Join request fixture".into(),
                        kind: SpaceKind::Direct,
                        privacy: Privacy::EndToEnd,
                    },
                )?;
                self.add_member(&dm, &applicant, Role::Member)?;
                self.append(
                    &dm,
                    &applicant,
                    EventBody::MessagePosted {
                        message: roda_types::new_id("msg"),
                        text: format!("{PREFIX}{}", json!({"space":space,"status":"pending"})),
                        attaches: None,
                        reply: None,
                        forwarded: false,
                    },
                )?;
            }
            Command::RequestJoin => {
                if members.iter().any(|(id, _)| id == &me) {
                    return Err(invalid("Already a member"));
                }
                let dm = self.living_direct(&owner)?;
                self.send_message(
                    &dm,
                    &format!("{PREFIX}{}", json!({"space":space,"status":"pending"})),
                )?;
            }
            Command::ApproveJoin { identity_id } | Command::RejectJoin { identity_id } => {
                // Parsed again only to distinguish the two variants without accepting extra fields.
                let approve = serde_json::from_str::<Value>(command_json).map_err(invalid)?
                    ["action"]
                    == "approve_join";
                if !admin {
                    return Err(forbidden());
                }
                if !self.living_requests(space).contains(&identity_id) {
                    return Err(invalid("No pending join request"));
                }
                if approve {
                    self.add_member(space, &identity_id, Role::Member)?;
                }
                let dm = self.living_direct(&identity_id)?;
                self.send_message(&dm, &format!("{PREFIX}{}", json!({"space":space,"identity":identity_id,"status":if approve {"approved"} else {"rejected"}})))?;
            }
            Command::SetCover { file_id } => {
                if !admin {
                    return Err(forbidden());
                }
                let item = self
                    .state
                    .items
                    .get(&file_id)
                    .ok_or_else(|| invalid("Cover file not found"))?;
                if item.space != space
                    || !matches!(&item.current().content, roda_types::ItemContent::File(file) if file.mime.starts_with("image/"))
                {
                    return Err(invalid("Cover must be an image File in this Space"));
                }
                metadata = Some(json!({"action":"set_cover","file_id":file_id}));
            }
            Command::SetHome { page_id } => {
                if !admin {
                    return Err(forbidden());
                }
                let item = self
                    .state
                    .items
                    .get(&page_id)
                    .ok_or_else(|| invalid("Page not found"))?;
                if item.space != space || item.kind != ItemKind::Page {
                    return Err(invalid("Home must reference a Page in this Space"));
                }
                metadata = Some(json!({"action":"set_home","page_id":page_id}));
                model
                    .set_home(&me, page_id)
                    .map_err(|e| invalid(format!("{e:?}")))?;
            }
            Command::Announce { item_id } => {
                if !admin {
                    return Err(forbidden());
                }
                let item = self
                    .state
                    .items
                    .get(&item_id)
                    .ok_or_else(|| invalid("Announcement not found"))?;
                if item.space != space {
                    return Err(invalid("Announcement must belong to this Space"));
                }
                metadata = Some(json!({"action":"announce","item_id":item_id}));
                model
                    .publish_announcement(&me, item_id)
                    .map_err(|e| invalid(format!("{e:?}")))?;
            }
            Command::CreateChild { title } => {
                if !admin {
                    return Err(forbidden());
                }
                if title.trim().is_empty() {
                    return Err(invalid("Group title required"));
                }
                let child =
                    self.create_synced_space(&title, SpaceKind::Group, Privacy::EndToEnd, &[])?;
                metadata = Some(json!({"action":"register_child","child":child}));
                model
                    .register_child_group(&me, child, SpaceKind::Group, Privacy::EndToEnd)
                    .map_err(|e| invalid(format!("{e:?}")))?;
            }
        }
        if let Some(metadata) = metadata {
            self.append(
                space,
                &me,
                EventBody::MessagePosted {
                    message: roda_types::new_id("msg"),
                    text: format!("{META_PREFIX}{metadata}"),
                    attaches: None,
                    reply: None,
                    forwarded: false,
                },
            )?;
        }
        Ok("{\"ok\":true}".into())
    }
}
