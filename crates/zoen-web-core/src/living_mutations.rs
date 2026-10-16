//! Signed Living Space mutations. Never report a mutation without a durable outbox update.
use crate::client::{Client, Update};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use openmls_traits::signatures::Signer;
use roda_types::{EventBody, ItemContent, ItemKind, Role};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const META: &str = "zoen-living-space-meta-v1:";
const HACK: &str = "zoen-living-space-hackathon-v1:";
#[derive(Serialize)]
pub struct LivingMutation {
    pub update: Update,
    pub result: Value,
}
fn field<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{key} required"))
}
impl<S: Signer> Client<S> {
    pub fn living_space_mutate(
        &mut self,
        space: &str,
        raw: &str,
        now_ms: i64,
        edit_id: &str,
    ) -> Result<LivingMutation, String> {
        self.living_transaction(|client| {
            client.living_space_mutate_inner(space, raw, now_ms, edit_id)
        })
    }
    fn living_space_mutate_inner(
        &mut self,
        space: &str,
        raw: &str,
        now_ms: i64,
        edit_id: &str,
    ) -> Result<LivingMutation, String> {
        let mut command: Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;
        let action = field(&command, "action")?.to_owned();
        if action == "request_join" || action == "approve_join" || action == "reject_join" {
            return self.living_consent_mutate(space, &command, now_ms, edit_id, raw);
        }
        if action.starts_with("tier_") {
            return self.living_tier_mutate(space, &command, now_ms, edit_id, raw);
        }
        self.living_space_home(space)?;
        if action == "home" {
            self.living_conversation_ensure(space, now_ms)?;
            return Ok(LivingMutation {
                update: self.update(vec![])?,
                result: self.living_space_home(space)?,
            });
        }
        if edit_id.is_empty() {
            return Err("Edit ID required".into());
        }
        let digest = hex::encode(roda_log::content::content_hash(raw.as_bytes()));
        if self.living_edit_seen(edit_id, &digest)? {
            return Ok(LivingMutation {
                update: self.update(vec![])?,
                result: json!({"ok":!self.living_has_pending(),"pending":self.living_has_pending(),"replayed":true}),
            });
        }
        let role = self.page_role(space).ok_or("Membership required")?;
        let admin = matches!(role, Role::Owner | Role::Admin);
        let mut result = json!({"ok":true});
        let prefix;
        match action.as_str() {
            "set_home" | "announce" | "set_cover" => {
                if !admin {
                    return Err("Living Space administrators only".into());
                }
                let key = match action.as_str() {
                    "set_home" => "page_id",
                    "announce" => "item_id",
                    _ => "file_id",
                };
                let id = field(&command, key)?;
                let mut item = None;
                for e in self.page_events().into_iter().filter(|e| e.space == space) {
                    match &e.body {
                        EventBody::ItemCreated {
                            item: i,
                            kind,
                            content,
                            ..
                        } if i == id => item = Some((*kind, content)),
                        EventBody::ItemVersioned {
                            item: i, content, ..
                        } if i == id => {
                            if let Some(p) = item.as_mut() {
                                p.1 = content;
                            }
                        }
                        _ => {}
                    }
                }
                let (kind, content) = item.ok_or("Item not found in Community")?;
                if action == "set_home" && kind != ItemKind::Page {
                    return Err("Home must be a page".into());
                }
                if action == "set_cover"
                    && !matches!(content,ItemContent::File(f) if f.mime.starts_with("image/"))
                {
                    return Err("Cover must be an image".into());
                }
                prefix = META;
            }
            "create_child" => {
                if !admin {
                    return Err("Living Space administrators only".into());
                }
                let child = self.living_create_group(field(&command, "title")?, now_ms)?;
                command = json!({"action":"register_child","child":child});
                result = json!({"ok":true,"space":child});
                prefix = META;
            }
            "conversation_policy" => {
                if !admin {
                    return Err("Living Space administrators only".into());
                }
                let cap = command["member_cap"]
                    .as_u64()
                    .filter(|n| (2..=4096).contains(n))
                    .ok_or("Member cap must be between 2 and 4096")?;
                if self
                    .living_conversation_policy(space)?
                    .1
                    .iter()
                    .any(|g| self.living_members(g).len() > cap as usize)
                {
                    return Err(
                        "Cannot shrink below existing encrypted partition membership".into(),
                    );
                }
                command = json!({"action":"conversation_policy","member_cap":cap});
                self.living_queue_application(
                    space,
                    EventBody::MessagePosted {
                        message: roda_types::new_id("msg"),
                        text: format!("{META}{command}"),
                        attaches: None,
                        reply: None,
                        forwarded: false,
                    },
                    now_ms,
                )?;
                self.living_conversation_ensure(space, now_ms)?;
                self.living_mark_edit(edit_id, &digest);
                return Ok(LivingMutation {
                    update: self.update(vec![])?,
                    result: json!({"ok":true,"pending":self.living_has_pending()}),
                });
            }
            "hackathon_create" | "project_submit" | "set_judge" | "judge" | "finalize"
            | "team_create" | "team_join" | "register" | "vote" | "schedule_event" => {
                let mut hack = self.living_hack_replay(space)?;
                if action == "hackathon_create" {
                    if hack.is_some() {
                        return Err("A hackathon already exists".into());
                    }
                    command["id"] = json!(roda_types::new_id("hack"));
                    roda_types::hackathon::Hackathon::create(
                        self.living_identity(),
                        role,
                        now_ms,
                        &command,
                    )?;
                } else {
                    let h = hack.as_mut().ok_or("No hackathon")?;
                    if action == "team_create" {
                        command["id"] = json!(roda_types::new_id("team"));
                        command["space"] = json!("validation");
                        h.clone()
                            .apply(self.living_identity(), role, now_ms, &command)?;
                        command["space"] =
                            json!(self.living_create_group(field(&command, "name")?, now_ms)?);
                    }
                    if action == "project_submit" {
                        let team = h
                            .teams
                            .iter()
                            .find(|t| Some(t.id.as_str()) == command["team_id"].as_str())
                            .ok_or("Team not found")?;
                        for file in command["file_ids"].as_array().ok_or("File IDs required")? {
                            let id = file.as_str().ok_or("Invalid file ID")?;
                            if !self.page_events().iter().any(|e| (e.space==space||e.space==team.space)&&matches!(&e.body,EventBody::ItemCreated{item,kind:ItemKind::File,..} if item==id)){return Err("Submission file not found in Space or team".into());}
                        }
                    }
                    if action == "finalize" && h.finalized {
                        if !admin {
                            return Err("Living Space administrators only".into());
                        }
                    } else {
                        h.apply(self.living_identity(), role, now_ms, &command)?;
                    }
                    if action == "team_join" {
                        let who = command["identity_id"]
                            .as_str()
                            .unwrap_or(self.living_identity())
                            .to_owned();
                        let team = h
                            .teams
                            .iter()
                            .find(|t| Some(t.id.as_str()) == command["team_id"].as_str())
                            .ok_or("Team not found")?;
                        if who != self.living_identity() {
                            let body = EventBody::MessagePosted {
                                message: roda_types::new_id("msg"),
                                text: format!("{HACK}{command}"),
                                attaches: None,
                                reply: None,
                                forwarded: false,
                            };
                            self.living_admit(
                                &team.space,
                                &who,
                                now_ms,
                                Some((space.into(), body)),
                            )?;
                            self.living_mark_edit(edit_id, &digest);
                            return Ok(LivingMutation {
                                update: self.update(vec![])?,
                                result: json!({"pending":true,"operation":"mls_admission"}),
                            });
                        }
                    }
                    if action == "finalize" {
                        let item = format!("it_{}_winners", h.id);
                        let names = h
                            .teams
                            .iter()
                            .filter(|t| h.winners.contains(&t.id))
                            .map(|t| t.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        if !self.page_events().iter().any(
                            |e| matches!(&e.body,EventBody::ItemCreated{item:id,..} if id==&item),
                        ) {
                            self.living_queue_application(
                                space,
                                EventBody::ItemCreated {
                                    item: item.clone(),
                                    kind: ItemKind::Note,
                                    content: ItemContent::Text {
                                        text: format!("{} — Winners: {}", h.title, names),
                                    },
                                    origin: "Hackathon results".into(),
                                },
                                now_ms,
                            )?;
                        }
                        if !self.living_space_home(space)?["announcement_items"]
                            .as_array()
                            .is_some_and(|ids| ids.iter().any(|id| id == &item))
                        {
                            self.living_queue_application(
                                space,
                                EventBody::MessagePosted {
                                    message: roda_types::new_id("msg"),
                                    text: format!(
                                        "{META}{}",
                                        json!({"action":"announce","item_id":item})
                                    ),
                                    attaches: None,
                                    reply: None,
                                    forwarded: false,
                                },
                                now_ms,
                            )?;
                        }
                    }
                }
                prefix = HACK;
            }
            _ => return Err(format!("Unsupported browser mutation: {action}")),
        }
        let body = EventBody::MessagePosted {
            message: roda_types::new_id("msg"),
            text: format!("{prefix}{command}"),
            attaches: None,
            reply: None,
            forwarded: false,
        };
        let update = self.queue_page_body(space, body, now_ms, edit_id, &digest)?;
        Ok(LivingMutation { update, result })
    }
    fn living_conversation_policy(&self, space: &str) -> Result<(usize, Vec<String>), String> {
        let mut cap = 256;
        let mut groups = vec![];
        let mut roles = std::collections::BTreeMap::new();
        for e in self.living_events(space)? {
            match e.body {
                EventBody::SpaceCreated { .. } => {
                    roles.insert(e.author, Role::Owner);
                }
                EventBody::MemberAdded { identity, role } => {
                    roles.insert(identity, role);
                }
                EventBody::MemberRemoved { identity } => {
                    roles.remove(&identity);
                }
                EventBody::MessagePosted { text, .. }
                    if matches!(roles.get(&e.author), Some(Role::Owner | Role::Admin)) =>
                {
                    let Some(raw) = text.strip_prefix(META) else {
                        continue;
                    };
                    let Ok(v) = serde_json::from_str::<Value>(raw) else {
                        continue;
                    };
                    if v["action"] == "conversation_policy" {
                        if let Some(n) = v["member_cap"].as_u64().filter(|n| (2..=4096).contains(n))
                        {
                            cap = n as usize;
                        }
                    } else if v["action"] == "conversation_partition" {
                        if let Some(id) = v["child"].as_str() {
                            if !groups.iter().any(|g| g == id) {
                                groups.push(id.to_owned());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok((cap, groups))
    }
    pub(crate) fn living_conversation_ensure(
        &mut self,
        space: &str,
        now_ms: i64,
    ) -> Result<(), String> {
        if !matches!(self.page_role(space), Some(Role::Owner | Role::Admin)) {
            return Ok(());
        }
        self.living_track_conversations(space, now_ms);
        if self.living_has_pending() {
            return Ok(());
        }
        let (cap, mut groups) = self.living_conversation_policy(space)?;
        let mut planned: std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<String, Role>,
        > = groups
            .iter()
            .map(|g| (g.clone(), self.living_members(g)))
            .collect();
        let mut admissions = vec![];
        for (who, role) in self.living_members(space) {
            if planned.values().any(|m| m.contains_key(&who)) {
                continue;
            }
            let group = if let Some(group) = groups.iter().find(|g| {
                planned.get(*g).is_some_and(|m| {
                    m.len() < cap
                        && matches!(
                            m.get(self.living_identity()),
                            Some(Role::Owner | Role::Admin)
                        )
                })
            }) {
                group.clone()
            } else {
                let group = self.living_create_group("Conversations", now_ms)?;
                self.living_queue_application(
                    space,
                    EventBody::MessagePosted {
                        message: roda_types::new_id("msg"),
                        text: format!(
                            "{META}{}",
                            json!({"action":"conversation_partition","child":group})
                        ),
                        attaches: None,
                        reply: None,
                        forwarded: false,
                    },
                    now_ms,
                )?;
                planned.insert(group.clone(), self.living_members(&group));
                groups.push(group.clone());
                group
            };
            if who != self.living_identity() {
                let role = if matches!(role, Role::Owner | Role::Admin) {
                    Role::Admin
                } else {
                    role
                };
                planned.get_mut(&group).unwrap().insert(who.clone(), role);
                admissions.push((group, who, role));
            }
        }
        if !admissions.is_empty() {
            self.living_admit_batch(admissions, now_ms, space, None)?;
        }
        Ok(())
    }
    pub(crate) fn living_join_requests(&self, space: &str) -> Vec<String> {
        let mut pending = std::collections::BTreeSet::new();
        for dm in self.living_spaces() {
            if !self.living_metadata(&dm).is_some_and(|(_, k, p)| {
                k == roda_types::SpaceKind::Direct && p == roda_types::Privacy::EndToEnd
            }) || self.page_role(&dm).is_none()
            {
                continue;
            }
            for e in self.living_events(&dm).unwrap_or_default() {
                if let EventBody::MessagePosted { text, .. } = &e.body {
                    let Some(raw) = text.strip_prefix("zoen-living-space-request-v1:") else {
                        continue;
                    };
                    let Ok(v) = serde_json::from_str::<Value>(raw) else {
                        continue;
                    };
                    if v["space"] != space {
                        continue;
                    }
                    if v["status"] == "pending" && e.author != self.living_identity() {
                        pending.insert(e.author);
                    } else if e.author == self.living_identity() {
                        if let Some(who) = v["identity"].as_str() {
                            pending.remove(who);
                        }
                    }
                }
            }
        }
        pending
            .into_iter()
            .filter(|id| !self.living_members(space).contains_key(id))
            .collect()
    }
    fn living_consent_mutate(
        &mut self,
        space: &str,
        c: &Value,
        now_ms: i64,
        edit_id: &str,
        raw: &str,
    ) -> Result<LivingMutation, String> {
        let digest = hex::encode(Sha256::digest(raw.as_bytes()));
        if edit_id.is_empty() {
            return Err("Edit ID required".into());
        }
        if self.living_edit_seen(edit_id, &digest)? {
            return Ok(LivingMutation {
                update: self.update(vec![])?,
                result: json!({"ok":!self.living_has_pending(),"pending":self.living_has_pending(),"replayed":true}),
            });
        }
        if !self
            .living_metadata(space)
            .is_some_and(|(_, k, _)| k == roda_types::SpaceKind::Community)
        {
            return Err("Community required".into());
        }
        let members = self.living_members(space);
        let action = field(c, "action")?;
        let pending = if action == "request_join" {
            if members.contains_key(self.living_identity()) {
                return Err("Already a member".into());
            }
            let owner = members
                .iter()
                .find(|(_, r)| **r == Role::Owner)
                .map(|(id, _)| id.clone())
                .ok_or("Community owner unavailable")?;
            self.living_direct_send(
                &owner,
                &format!(
                    "zoen-living-space-request-v1:{}",
                    json!({"space":space,"status":"pending"})
                ),
                now_ms,
            )?
        } else {
            if !matches!(
                members.get(self.living_identity()),
                Some(Role::Owner | Role::Admin)
            ) {
                return Err("Community administrators only".into());
            }
            let who = field(c, "identity_id")?;
            if !self.living_join_requests(space).iter().any(|id| id == who) {
                return Err("No pending join request".into());
            }
            if action == "approve_join"
                && self
                    .living_metadata(space)
                    .is_some_and(|(_, _, p)| p == roda_types::Privacy::EndToEnd)
            {
                let dm = self
                    .living_direct_with(who)
                    .ok_or("Authenticated request DM unavailable")?;
                let body = EventBody::MessagePosted {
                    message: roda_types::new_id("msg"),
                    text: format!(
                        "zoen-living-space-request-v1:{}",
                        json!({"space":space,"identity":who,"status":"approved"})
                    ),
                    attaches: None,
                    reply: None,
                    forwarded: false,
                };
                self.living_track_conversations(space, now_ms);
                self.living_admit(space, who, now_ms, Some((dm, body)))?;
                self.living_mark_edit(edit_id, &digest);
                return Ok(LivingMutation {
                    update: self.update(vec![])?,
                    result: json!({"pending":true,"operation":"mls_admission"}),
                });
            }
            if action == "approve_join" {
                self.living_add_parent_member(space, who, now_ms)?;
            }
            self.living_direct_send(who,&format!("zoen-living-space-request-v1:{}",json!({"space":space,"identity":who,"status":if action=="approve_join"{"approved"}else{"rejected"}})),now_ms)?
        };
        if action == "approve_join" {
            self.living_conversation_ensure(space, now_ms)?;
        }
        self.living_mark_edit(edit_id, &digest);
        Ok(LivingMutation {
            update: self.update(vec![])?,
            result: json!({"pending":pending||self.living_has_pending(),"ok":!pending}),
        })
    }
    fn living_tier_state(&self, space: &str) -> Result<Value, String> {
        let mut state = json!({"tiers":[],"entitlements":[],"invites":[]});
        let mut roles = std::collections::BTreeMap::new();
        for e in self.living_events(space)? {
            match e.body {
                EventBody::SpaceCreated { .. } => {
                    roles.insert(e.author.clone(), Role::Owner);
                }
                EventBody::MemberAdded { identity, role } => {
                    roles.insert(identity, role);
                }
                EventBody::MemberRemoved { identity } => {
                    roles.remove(&identity);
                }
                EventBody::MessagePosted { text, .. }
                    if matches!(roles.get(&e.author), Some(Role::Owner | Role::Admin)) =>
                {
                    let Some(raw) = text.strip_prefix("\u{001e}zoen.space.tiers:") else {
                        continue;
                    };
                    let Ok(delta) = serde_json::from_str::<Value>(raw) else {
                        continue;
                    };
                    for tier in delta["tiers"].as_array().into_iter().flatten() {
                        let entries = state["tiers"].as_array_mut().unwrap();
                        if let Some(old) = entries.iter_mut().find(|t| t["id"] == tier["id"]) {
                            if old["space"] != tier["space"] {
                                continue;
                            }
                            for key in ["groups", "resources"] {
                                let mut merged = old[key].as_array().cloned().unwrap_or_default();
                                for value in tier[key].as_array().into_iter().flatten() {
                                    if !merged.contains(value) {
                                        merged.push(value.clone());
                                    }
                                }
                                old[key] = json!(merged);
                            }
                            old["name"] = tier["name"].clone();
                        } else {
                            entries.push(tier.clone());
                        }
                    }
                    for grant in delta["grants"].as_array().into_iter().flatten() {
                        if grant["source"] != "manual" && grant["source"] != "invite_link" {
                            continue;
                        }
                        let entries = state["entitlements"].as_array_mut().unwrap();
                        entries.retain(|e| {
                            e["tier_id"] != grant["tier_id"]
                                || e["identity_id"] != grant["identity_id"]
                        });
                        entries.push(grant.clone());
                    }
                    for revoke in delta["revokes"].as_array().into_iter().flatten() {
                        state["entitlements"].as_array_mut().unwrap().retain(|e| {
                            e["tier_id"] != revoke["tier_id"]
                                || e["identity_id"] != revoke["identity_id"]
                        });
                    }
                    for invite in delta["invites"].as_array().into_iter().flatten() {
                        let entries = state["invites"].as_array_mut().unwrap();
                        if let Some(old) = entries.iter_mut().find(|i| i["id"] == invite["id"]) {
                            if invite["revoked"] == true {
                                old["revoked"] = json!(true);
                            }
                        } else {
                            entries.push(invite.clone());
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(state)
    }
    pub(crate) fn living_tier_claims(&self, space: &str) -> Vec<Value> {
        if !matches!(self.page_role(space), Some(Role::Owner | Role::Admin)) {
            return vec![];
        }
        let Ok(state) = self.living_tier_state(space) else {
            return vec![];
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(i64::MAX);
        let mut claims = vec![];
        for dm in self.living_spaces() {
            if !self.living_metadata(&dm).is_some_and(|(_, k, p)| {
                k == roda_types::SpaceKind::Direct && p == roda_types::Privacy::EndToEnd
            }) || self.page_role(&dm).is_none()
            {
                continue;
            }
            for e in self.living_events(&dm).unwrap_or_default() {
                if e.author == self.living_identity()
                    || !self.living_members(&dm).contains_key(&e.author)
                {
                    continue;
                }
                if let EventBody::MessagePosted { text, .. } = e.body {
                    let Some(raw) = text.strip_prefix("\u{001e}zoen.space.tier-claim:") else {
                        continue;
                    };
                    let Ok(c) = serde_json::from_str::<Value>(raw) else {
                        continue;
                    };
                    let Some(raw) = c["token"].as_str() else {
                        continue;
                    };
                    let Ok(token) = self.living_validate_invitation(space, &state, raw, now) else {
                        continue;
                    };
                    let v = json!({"identity_id":e.author,"invite_id":token["invite_id"],"tier_id":token["tier_id"]});
                    if !claims.contains(&v) {
                        claims.push(v);
                    }
                }
            }
        }
        claims
    }
    fn living_validate_invitation(
        &self,
        space: &str,
        state: &Value,
        raw: &str,
        now_ms: i64,
    ) -> Result<Value, String> {
        if raw.len() > 4096 {
            return Err("Invalid invitation".into());
        }
        let token: Value = serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(raw)
                .map_err(|_| "Invalid invitation")?,
        )
        .map_err(|_| "Invalid invitation")?;
        let tier = field(&token, "tier_id")?;
        let id = field(&token, "invite_id")?;
        let secret = field(&token, "secret")?;
        if token["space"] != space
            || secret.len() != 43
            || !state["tiers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["id"] == tier)
        {
            return Err("Invitation scope mismatch".into());
        }
        let invite = state["invites"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == id && i["tier_id"] == tier)
            .ok_or("Invitation not found")?;
        let digest = hex::encode(Sha256::digest(
            format!("zoen-tier-invite\0{space}\0{tier}\0{id}\0{secret}").as_bytes(),
        ));
        if invite["revoked"] == true
            || invite["expires_at_ms"].as_i64().is_none_or(|e| e <= now_ms)
            || invite["digest"] != digest
            || !matches!(
                invite["issuer"]
                    .as_str()
                    .and_then(|id| self.living_members(space).get(id).copied()),
                Some(Role::Owner | Role::Admin)
            )
        {
            return Err("Invitation is expired, revoked or invalid".into());
        }
        Ok(token)
    }
    fn living_tier_mutate(
        &mut self,
        space: &str,
        command: &Value,
        now_ms: i64,
        edit_id: &str,
        raw: &str,
    ) -> Result<LivingMutation, String> {
        let action = field(command, "action")?;
        let digest = hex::encode(Sha256::digest(raw.as_bytes()));
        if edit_id.is_empty() {
            return Err("Edit ID required".into());
        }
        if self.living_edit_seen(edit_id, &digest)? {
            return Ok(LivingMutation {
                update: self.update(vec![])?,
                result: json!({"ok":!self.living_has_pending(),"pending":self.living_has_pending(),"replayed":true}),
            });
        }
        let state = self.living_tier_state(space)?;
        if action == "tier_redeem" {
            let raw_token = field(command, "token")?;
            let raw_token = if let Some(q) = raw_token.strip_prefix("zoen://tier?") {
                q.split('&')
                    .find_map(|f| f.strip_prefix("token="))
                    .ok_or("Invalid invitation")?
            } else {
                raw_token
            };
            let token = self.living_validate_invitation(space, &state, raw_token, now_ms)?;
            let issuer = state["invites"]
                .as_array()
                .unwrap()
                .iter()
                .find(|i| i["id"] == token["invite_id"])
                .and_then(|i| i["issuer"].as_str())
                .ok_or("Invalid invitation issuer")?;
            let pending = self.living_direct_send(
                issuer,
                &format!(
                    "\u{001e}zoen.space.tier-claim:{}",
                    json!({"token":raw_token})
                ),
                now_ms,
            )?;
            self.living_mark_edit(edit_id, &digest);
            return Ok(LivingMutation {
                update: self.update(vec![])?,
                result: json!({"pending":true,"mls_pending":pending}),
            });
        }
        if !matches!(self.page_role(space), Some(Role::Owner | Role::Admin)) {
            return Err("Living Space administrators only".into());
        }
        let mut normalized = command.clone();
        if action == "tier_claim_approve" {
            let who = field(command, "identity_id")?;
            let invite_id = field(command, "invite_id")?;
            let mut token = None;
            for dm in self.living_spaces() {
                if !self.living_metadata(&dm).is_some_and(|(_, k, p)| {
                    k == roda_types::SpaceKind::Direct && p == roda_types::Privacy::EndToEnd
                }) || self.page_role(&dm).is_none()
                    || !self.living_members(&dm).contains_key(who)
                {
                    continue;
                }
                for e in self.living_events(&dm).unwrap_or_default() {
                    if e.author != who {
                        continue;
                    }
                    if let EventBody::MessagePosted { text, .. } = e.body {
                        let Some(raw) = text.strip_prefix("\u{001e}zoen.space.tier-claim:") else {
                            continue;
                        };
                        let Ok(claim) = serde_json::from_str::<Value>(raw) else {
                            continue;
                        };
                        let Some(raw) = claim["token"].as_str() else {
                            continue;
                        };
                        if let Ok(t) = self.living_validate_invitation(space, &state, raw, now_ms) {
                            if t["invite_id"] == invite_id {
                                token = Some(t);
                            }
                        }
                    }
                }
            }
            normalized["tier_id"] =
                token.ok_or("No authenticated valid tier claim")?["tier_id"].clone();
        }
        let command = &normalized;
        let mut delta = json!({"tiers":[],"grants":[],"revokes":[],"invites":[]});
        let mut result = json!({"ok":true});
        let mut admissions = vec![];
        match action {
            "tier_create" => {
                let name = field(command, "name")?.trim();
                let tiers = state["tiers"].as_array().unwrap();
                if name.is_empty()
                    || name.chars().count() > 60
                    || tiers.len() >= 64
                    || tiers.iter().any(|t| {
                        t["name"]
                            .as_str()
                            .is_some_and(|s| s.to_lowercase() == name.to_lowercase())
                    })
                {
                    return Err("Choose a different membership name".into());
                }
                let group = self.living_create_group(name, now_ms)?;
                let id = roda_types::new_id("tier");
                delta["tiers"] = json!([{ "id":id,"name":name,"space":group,"groups":[],"resources":[],"members":[] }]);
                for (who, role) in self.living_members(space) {
                    if who != self.living_identity() && matches!(role, Role::Owner | Role::Admin) {
                        admissions.push((group.clone(), who, Role::Admin));
                    }
                }
                result = json!({"ok":true,"tier_id":id,"space":group});
            }
            "tier_invite" => {
                let tier_id = field(command, "tier_id")?;
                if !state["tiers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|t| t["id"] == tier_id)
                    || state["invites"].as_array().unwrap().len() >= 512
                {
                    return Err("Tier not found or invite limit reached".into());
                }
                let expiry = command["expires_at_ms"]
                    .as_i64()
                    .filter(|e| *e > now_ms && e.saturating_sub(now_ms) <= 30 * 24 * 60 * 60 * 1000)
                    .ok_or("Invitations can last up to 30 days")?;
                let mut random = [0u8; 32];
                getrandom::getrandom(&mut random).map_err(|_| "Could not create invitation")?;
                let secret = URL_SAFE_NO_PAD.encode(random);
                let id = roda_types::new_id("invite");
                let token = json!({"space":space,"tier_id":tier_id,"invite_id":id,"secret":secret});
                let hash = hex::encode(Sha256::digest(
                    format!("zoen-tier-invite\0{space}\0{tier_id}\0{id}\0{secret}").as_bytes(),
                ));
                delta["invites"] = json!([{"id":id,"tier_id":tier_id,"issuer":self.living_identity(),"digest":hash,"expires_at_ms":expiry,"revoked":false}]);
                result = json!({"invite_url":format!("zoen://tier?space={space}&token={}",URL_SAFE_NO_PAD.encode(serde_json::to_vec(&token).map_err(|e|e.to_string())?)),"invite_id":id,"expires_at_ms":expiry});
            }
            "tier_invite_revoke" => {
                let id = field(command, "invite_id")?;
                let mut invite = state["invites"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|i| i["id"] == id)
                    .cloned()
                    .ok_or("Invite not found")?;
                invite["revoked"] = json!(true);
                delta["invites"] = json!([invite]);
            }
            "tier_assign" | "tier_claim_approve" | "tier_revoke" | "tier_bind"
            | "tier_group_create" => {
                let id = field(command, "tier_id")?;
                let mut tier = state["tiers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|t| t["id"] == id)
                    .cloned()
                    .ok_or("Tier not found")?;
                let partition = field(&tier, "space")?.to_owned();
                if action == "tier_assign" || action == "tier_claim_approve" {
                    if command.get("source").is_some_and(|s| s != "manual") {
                        return Err("Paid entitlement is not supported".into());
                    }
                    let who = field(command, "identity_id")?;
                    if !self.living_members(space).contains_key(who) {
                        return Err("Community membership required".into());
                    }
                    for group in std::iter::once(Value::String(partition.clone()))
                        .chain(tier["groups"].as_array().cloned().unwrap_or_default())
                    {
                        admissions.push((
                            field(&json!({"id":group}), "id")?.into(),
                            who.into(),
                            Role::Member,
                        ));
                    }
                    delta["grants"] = json!([{"identity_id":who,"tier_id":id,"source":if action=="tier_claim_approve"{"invite_link"}else{"manual"}}]);
                } else if action == "tier_revoke" {
                    let who = field(command, "identity_id")?;
                    if matches!(
                        self.living_members(space).get(who),
                        Some(Role::Owner | Role::Admin)
                    ) {
                        return Err("Administrators keep access".into());
                    }
                    for group in std::iter::once(Value::String(partition.clone()))
                        .chain(tier["groups"].as_array().cloned().unwrap_or_default())
                    {
                        self.living_remove(
                            group.as_str().ok_or("Invalid tier group")?,
                            who,
                            now_ms,
                        )?;
                    }
                    delta["revokes"] = json!([{"identity_id":who,"tier_id":id}]);
                } else if action == "tier_bind" {
                    let item = field(command, "item_id")?;
                    let kind = field(command, "kind")?;
                    if !self.page_events().iter().any(|e|e.space==partition&&matches!(&e.body,EventBody::ItemCreated{item:i,kind:k,..} if i==item&&matches!((kind,k),("page",ItemKind::Page)|("event",ItemKind::App)))){return Err("Create content inside private membership Space first".into());}
                    tier["resources"]
                        .as_array_mut()
                        .ok_or("Invalid tier resources")?
                        .push(json!({"id":item,"kind":kind}));
                    delta["tiers"] = json!([tier]);
                } else {
                    let title = field(command, "title")?.trim();
                    if title.is_empty() || title.chars().count() > 100 {
                        return Err("Choose a shorter conversation name".into());
                    }
                    let group = self.living_create_group(title, now_ms)?;
                    if tier["groups"].as_array().is_some_and(|g| g.len() >= 64) {
                        return Err("Membership conversation limit reached".into());
                    }
                    for grant in state["entitlements"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|e| e["tier_id"] == id)
                    {
                        admissions.push((
                            group.clone(),
                            field(grant, "identity_id")?.into(),
                            Role::Member,
                        ));
                    }
                    for (who, role) in self.living_members(space) {
                        if who != self.living_identity()
                            && matches!(role, Role::Owner | Role::Admin)
                        {
                            admissions.push((group.clone(), who, Role::Admin));
                        }
                    }
                    tier["groups"]
                        .as_array_mut()
                        .ok_or("Invalid tier groups")?
                        .push(json!(group));
                    delta["tiers"] = json!([tier]);
                }
            }
            _ => return Err(format!("Unsupported browser mutation: {action}")),
        }
        let body = EventBody::MessagePosted {
            message: roda_types::new_id("msg"),
            text: format!("\u{001e}zoen.space.tiers:{delta}"),
            attaches: None,
            reply: None,
            forwarded: false,
        };
        if !admissions.is_empty() {
            self.living_admit_batch(admissions, now_ms, space, Some((space.into(), body)))?;
            self.living_mark_edit(edit_id, &digest);
            result = json!({"pending":true,"operation":"mls_admission"});
            return Ok(LivingMutation {
                update: self.update(vec![])?,
                result,
            });
        }
        Ok(LivingMutation {
            update: self.queue_page_body(space, body, now_ms, edit_id, &digest)?,
            result,
        })
    }
    fn living_hack_replay(
        &self,
        space: &str,
    ) -> Result<Option<roda_types::hackathon::Hackathon>, String> {
        let mut roles = std::collections::BTreeMap::new();
        let mut hack = None;
        for e in self.living_events(space)? {
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
                EventBody::MessagePosted { text, .. } => {
                    let Some(raw) = text.strip_prefix(HACK) else {
                        continue;
                    };
                    let Some(role) = roles.get(&e.author) else {
                        continue;
                    };
                    let Ok(v) = serde_json::from_str::<Value>(raw) else {
                        continue;
                    };
                    if v["action"] == "hackathon_create" && hack.is_none() {
                        hack =
                            roda_types::hackathon::Hackathon::create(&e.author, *role, e.at_ms, &v)
                                .ok();
                    } else if let Some(h) = hack.as_mut() {
                        let _ = h.apply(&e.author, *role, e.at_ms, &v);
                    }
                }
                _ => {}
            }
        }
        Ok(hack)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LinkedAccount;
    use openmls_traits::{signatures::SignerError, types::SignatureScheme};
    use roda_log::{Author, Signer as Key, SpaceLog};
    use roda_proto::{ClientFrame, Envelope, KeyPackageRecord, Op, Reply, Sequenced, ServerFrame};
    use std::collections::BTreeMap;
    #[derive(Clone)]
    struct Host(Key);
    impl Signer for Host {
        fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SignerError> {
            hex::decode(self.0.sign(payload)).map_err(|_| SignerError::SigningError)
        }
        fn signature_scheme(&self) -> SignatureScheme {
            roda_mls::SUITE.signature_algorithm()
        }
    }
    fn fixture(n: u8) -> (Client<Host>, Author, LinkedAccount, Host) {
        let identity = Key::from_secret(&[n; 32]);
        let host = Host(Key::from_secret(&[n + 1; 32]));
        let account = LinkedAccount {
            identity: identity.id(),
            device: host.0.id(),
            cert: identity.sign(&roda_log::device_cert_message(&host.0.id())),
            name: "Fixture".into(),
            handle: "fixture".into(),
            relay: "http://localhost:9000".into(),
            primary: identity.id(),
            primary_hpke: String::new(),
            hpke_secret: String::new(),
            link_secret: String::new(),
            agreement_secret: String::new(),
            profile_key: String::new(),
            profile_key_version: 0,
            profile_seen_version: 0,
            profile_fields: String::new(),
        };
        (
            Client::new(account.clone(), host.clone(), "").unwrap(),
            Author::device(&identity, host.0.clone()),
            account,
            host,
        )
    }
    fn sequence(log: &mut SpaceLog, env: Envelope) -> Vec<u8> {
        let event = roda_log::event_from_content(
            env.content().to_vec(),
            env.sig.clone(),
            env.cert.clone(),
            0,
            String::new(),
            String::new(),
        )
        .unwrap();
        let event = log.sequence(event);
        ServerFrame::Event {
            ev: Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env,
            },
        }
        .encode()
    }
    fn community(client: &mut Client<Host>, author: &Author) -> SpaceLog {
        let mut log = SpaceLog::new("root");
        let event = author.sign_event(
            "root",
            &roda_types::new_ulid(1),
            1,
            None,
            EventBody::SpaceCreated {
                title: "Community".into(),
                kind: roda_types::SpaceKind::Community,
                privacy: roda_types::Privacy::Closed,
            },
        );
        client
            .receive(&sequence(&mut log, Envelope::plain(&event)), 1)
            .unwrap();
        client.receive(&ServerFrame::SyncDone.encode(), 2).unwrap();
        log
    }
    fn relay(
        frames: Vec<Vec<u8>>,
        logs: &mut BTreeMap<String, SpaceLog>,
        a: &mut Client<Host>,
        b: &mut Client<Host>,
    ) {
        let mut frames: std::collections::VecDeque<_> = frames.into();
        while let Some(frame) = frames.pop_front() {
            if let ClientFrame::Publish { env } = ClientFrame::decode(&frame).unwrap() {
                let space = env.space().to_owned();
                let log = logs
                    .entry(space.clone())
                    .or_insert_with(|| SpaceLog::new(&space));
                let bytes = sequence(log, env);
                frames.extend(a.receive(&bytes, 50).unwrap().frames);
                frames.extend(b.receive(&bytes, 50).unwrap().frames);
            }
        }
    }
    #[test]
    fn living_mutation_child_is_real_clear_idempotent_and_pending_projection() {
        let (mut client, author, account, host) = fixture(31);
        community(&mut client, &author);
        let mutation = client
            .living_space_mutate(
                "root",
                r#"{"action":"create_child","title":"Private group"}"#,
                10,
                "child-edit",
            )
            .unwrap();
        let id = mutation.result["space"].as_str().unwrap();
        let mut genesis = false;
        for bytes in &mutation.update.frames {
            if let ClientFrame::Publish { env } = ClientFrame::decode(bytes).unwrap() {
                if env.space() == id {
                    assert!(matches!(
                        env.plain_event().unwrap().body,
                        EventBody::SpaceCreated {
                            privacy: roda_types::Privacy::EndToEnd,
                            ..
                        }
                    ));
                    genesis = true;
                }
            }
        }
        assert!(genesis);
        assert!(client.living_space_home("root").unwrap()["conversations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == id));
        let retry = client
            .living_space_mutate(
                "root",
                r#"{"action":"create_child","title":"Private group"}"#,
                11,
                "child-edit",
            )
            .unwrap();
        assert_eq!(retry.result["replayed"], true);
        assert!(client
            .living_space_mutate(
                "root",
                r#"{"action":"create_child","title":"Other"}"#,
                11,
                "child-edit"
            )
            .is_err());
        let reloaded = Client::new(account, host, &mutation.update.snapshot).unwrap();
        assert_eq!(reloaded.living_metadata(id).unwrap().0, "Private group");
    }
    #[test]
    fn living_mutation_real_mls_claim_welcome_message_and_remove() {
        let (mut owner, author, _, _) = fixture(41);
        let root = community(&mut owner, &author);
        let (mut member, _, member_account, _) = fixture(51);
        let bootstrap = member.bootstrap().unwrap();
        let package = bootstrap
            .frames
            .iter()
            .find_map(|f| match ClientFrame::decode(f).unwrap() {
                ClientFrame::Req {
                    op: Op::PublishKeyPackages { packages, .. },
                    ..
                } => packages.into_iter().next(),
                _ => None,
            })
            .unwrap();
        member.receive(&ServerFrame::SyncDone.encode(), 2).unwrap();
        let create = owner
            .living_space_mutate(
                "root",
                r#"{"action":"create_child","title":"Crypto"}"#,
                10,
                "crypto-group",
            )
            .unwrap();
        let group = create.result["space"].as_str().unwrap().to_owned();
        let mut logs = BTreeMap::from([("root".into(), root)]);
        relay(create.update.frames, &mut logs, &mut owner, &mut member);
        owner
            .living_admit(&group, &member_account.identity, 20, None)
            .unwrap();
        let claim = owner.update(vec![]).unwrap();
        assert!(claim.frames.iter().any(|f| matches!(
            ClientFrame::decode(f).unwrap(),
            ClientFrame::Req {
                op: Op::ClaimKeyPackages { .. },
                ..
            }
        )));
        let response = ServerFrame::Res {
            id: 0x8000_0003,
            result: Ok(Reply::KeyPackages(vec![KeyPackageRecord {
                identity: member_account.identity.clone(),
                device: member_account.device.clone(),
                data: package,
            }])),
        }
        .encode();
        let admission = owner.receive(&response, 20).unwrap();
        relay(admission.frames, &mut logs, &mut owner, &mut member);
        let message = owner
            .queue_message(&group, "Real encrypted message", 30)
            .unwrap();
        relay(message.frames, &mut logs, &mut owner, &mut member);
        assert!(serde_json::to_value(member.conversations())
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == group
                && c["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|m| m["text"] == "Real encrypted message")));
        owner
            .living_remove(&group, &member_account.identity, 40)
            .unwrap();
        let removal = owner.update(vec![]).unwrap();
        relay(removal.frames, &mut logs, &mut owner, &mut member);
        assert!(member
            .queue_message(&group, "Denied after removal", 50)
            .is_err());
    }
    #[test]
    fn living_mutation_consent_is_signed_encrypted_and_approval_creates_partition() {
        let (mut owner, author, owner_account, _) = fixture(71);
        let owner_keys = owner.bootstrap().unwrap();
        let root = community(&mut owner, &author);
        let (mut applicant, _, applicant_account, _) = fixture(81);
        let applicant_keys = applicant.bootstrap().unwrap();
        for e in root.events() {
            let bytes = ServerFrame::Event {
                ev: Sequenced {
                    seq: e.seq,
                    prev: e.prev.clone(),
                    hash: e.hash.clone(),
                    env: Envelope::plain(e),
                },
            }
            .encode();
            applicant.receive(&bytes, 2).unwrap();
        }
        applicant
            .receive(&ServerFrame::SyncDone.encode(), 2)
            .unwrap();
        let package = owner_keys
            .frames
            .iter()
            .find_map(|f| match ClientFrame::decode(f).unwrap() {
                ClientFrame::Req {
                    op: Op::PublishKeyPackages { packages, .. },
                    ..
                } => packages.into_iter().next(),
                _ => None,
            })
            .unwrap();
        let request = applicant
            .living_space_mutate("root", r#"{"action":"request_join"}"#, 10, "request")
            .unwrap();
        assert_eq!(request.result["pending"], true);
        let mut logs = BTreeMap::from([("root".into(), root)]);
        relay(request.update.frames, &mut logs, &mut applicant, &mut owner);
        let response = ServerFrame::Res {
            id: 0x8000_0003,
            result: Ok(Reply::KeyPackages(vec![KeyPackageRecord {
                identity: owner_account.identity,
                device: owner_account.device,
                data: package,
            }])),
        }
        .encode();
        let admission = applicant.receive(&response, 20).unwrap();
        relay(admission.frames, &mut logs, &mut applicant, &mut owner);
        assert_eq!(
            owner.living_join_requests("root"),
            vec![applicant_account.identity.clone()]
        );
        let approve = owner
            .living_space_mutate(
                "root",
                &json!({"action":"approve_join","identity_id":applicant_account.identity})
                    .to_string(),
                30,
                "approve",
            )
            .unwrap();
        assert!(owner
            .living_members("root")
            .contains_key(&applicant_account.identity));
        relay(approve.update.frames, &mut logs, &mut owner, &mut applicant);
        let package = applicant_keys
            .frames
            .iter()
            .find_map(|f| match ClientFrame::decode(f).unwrap() {
                ClientFrame::Req {
                    op: Op::PublishKeyPackages { packages, .. },
                    ..
                } => packages.into_iter().next(),
                _ => None,
            })
            .unwrap();
        let response = ServerFrame::Res {
            id: 0x8000_0003,
            result: Ok(Reply::KeyPackages(vec![KeyPackageRecord {
                identity: applicant_account.identity.clone(),
                device: applicant_account.device,
                data: package,
            }])),
        }
        .encode();
        let update = owner.receive(&response, 40).unwrap();
        relay(update.frames, &mut logs, &mut owner, &mut applicant);
        assert!(owner.living_join_requests("root").is_empty());
        assert!(applicant.living_space_home("root").unwrap()["conversation_space"].is_string());
        assert!(owner
            .living_space_mutate(
                "root",
                &json!({"action":"approve_join","identity_id":"forged"}).to_string(),
                50,
                "forged"
            )
            .is_err());
    }
    #[test]
    fn living_mutation_hackathon_finalization_publishes_one_real_winner_note() {
        let (mut client, author, _, _) = fixture(121);
        community(&mut client, &author);
        client
            .living_space_mutate(
                "root",
                r#"{"action":"hackathon_create","title":"Real hack","deadline_ms":100}"#,
                10,
                "hack",
            )
            .unwrap();
        client
            .living_space_mutate(
                "root",
                r#"{"action":"register","skills":"Rust"}"#,
                11,
                "register",
            )
            .unwrap();
        client
            .living_space_mutate(
                "root",
                r#"{"action":"team_create","name":"Builders"}"#,
                12,
                "team",
            )
            .unwrap();
        let hack = client.living_hack_replay("root").unwrap().unwrap();
        let team = hack.teams[0].id.clone();
        assert!(client.living_metadata(&hack.teams[0].space).is_some());
        client.living_space_mutate("root",&json!({"action":"project_submit","team_id":team,"url":"https://example.org/project","file_ids":[]}).to_string(),13,"submit").unwrap();
        assert!(client
            .living_space_mutate("root", r#"{"action":"finalize"}"#, 99, "too-early")
            .is_err());
        client
            .living_space_mutate("root", r#"{"action":"finalize"}"#, 101, "finalize")
            .unwrap();
        client
            .living_space_mutate("root", r#"{"action":"finalize"}"#, 102, "finalize-retry")
            .unwrap();
        let item = format!("it_{}_winners", hack.id);
        let events = client.living_events("root").unwrap();
        assert_eq!(events.iter().filter(|e|matches!(&e.body,EventBody::ItemCreated{item:id,kind:ItemKind::Note,content:ItemContent::Text{text},..} if id==&item&&text=="Real hack — Winners: Builders")).count(),1);
        assert_eq!(events.iter().filter(|e|matches!(&e.body,EventBody::MessagePosted{text,..} if text==&format!("{META}{}",json!({"action":"announce","item_id":item})))).count(),1);
        assert!(
            client.living_space_home("root").unwrap()["announcement_items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|id| id == &item)
        );
    }
    #[test]
    fn living_mutation_tier_grant_waits_for_crypto_and_revocation_removes_keys() {
        let (mut owner, author, _, _) = fixture(91);
        let root = community(&mut owner, &author);
        let (mut member, _, member_account, _) = fixture(101);
        let bootstrap = member.bootstrap().unwrap();
        for e in root.events() {
            member
                .receive(
                    &ServerFrame::Event {
                        ev: Sequenced {
                            seq: e.seq,
                            prev: e.prev.clone(),
                            hash: e.hash.clone(),
                            env: Envelope::plain(e),
                        },
                    }
                    .encode(),
                    2,
                )
                .unwrap();
        }
        member.receive(&ServerFrame::SyncDone.encode(), 2).unwrap();
        let mut logs = BTreeMap::from([("root".into(), root)]);
        owner
            .living_add_parent_member("root", &member_account.identity, 3)
            .unwrap();
        let update = owner.update(vec![]).unwrap();
        relay(update.frames, &mut logs, &mut owner, &mut member);
        let create = owner
            .living_space_mutate(
                "root",
                r#"{"action":"tier_create","name":"Members"}"#,
                10,
                "tier",
            )
            .unwrap();
        let tier = create.result["tier_id"].as_str().unwrap().to_owned();
        let partition = create.result["space"].as_str().unwrap().to_owned();
        relay(create.update.frames, &mut logs, &mut owner, &mut member);
        let assign=owner.living_space_mutate("root",&json!({"action":"tier_assign","tier_id":tier,"identity_id":member_account.identity}).to_string(),20,"assign").unwrap();
        assert_eq!(assign.result["pending"], true);
        assert!(owner.living_tier_state("root").unwrap()["entitlements"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(!member.living_tier_allows(&partition));
        let package = bootstrap
            .frames
            .iter()
            .find_map(|f| match ClientFrame::decode(f).unwrap() {
                ClientFrame::Req {
                    op: Op::PublishKeyPackages { packages, .. },
                    ..
                } => packages.into_iter().next(),
                _ => None,
            })
            .unwrap();
        let response = ServerFrame::Res {
            id: 0x8000_0003,
            result: Ok(Reply::KeyPackages(vec![KeyPackageRecord {
                identity: member_account.identity.clone(),
                device: member_account.device.clone(),
                data: package,
            }])),
        }
        .encode();
        let update = owner.receive(&response, 21).unwrap();
        assert!(owner.living_tier_state("root").unwrap()["entitlements"]
            .as_array()
            .unwrap()
            .is_empty());
        relay(update.frames, &mut logs, &mut owner, &mut member);
        assert!(member.living_tier_allows(&partition));
        assert_eq!(
            owner.living_tier_state("root").unwrap()["entitlements"][0]["identity_id"],
            member_account.identity
        );
        let revoke=owner.living_space_mutate("root",&json!({"action":"tier_revoke","tier_id":tier,"identity_id":member_account.identity}).to_string(),30,"revoke").unwrap();
        relay(revoke.update.frames, &mut logs, &mut owner, &mut member);
        assert!(!member.living_tier_allows(&partition));
        assert!(member.queue_message(&partition, "Denied", 40).is_err());
        assert!(owner.living_tier_state("root").unwrap()["entitlements"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn living_mutation_claim_failure_is_durable_and_never_grants() {
        let (mut owner, author, account, host) = fixture(111);
        community(&mut owner, &author);
        let who = Key::from_secret(&[113; 32]).id();
        owner.living_add_parent_member("root", &who, 3).unwrap();
        let create = owner
            .living_space_mutate(
                "root",
                r#"{"action":"tier_create","name":"Failure"}"#,
                10,
                "tier",
            )
            .unwrap();
        let tier = create.result["tier_id"].as_str().unwrap();
        let command = json!({"action":"tier_assign","tier_id":tier,"identity_id":who}).to_string();
        owner
            .living_space_mutate("root", &command, 20, "assign")
            .unwrap();
        let failed = owner
            .receive(
                &ServerFrame::Res {
                    id: 0x8000_0003,
                    result: Ok(Reply::KeyPackages(vec![])),
                }
                .encode(),
                21,
            )
            .unwrap();
        assert!(owner.living_mutation_status()["failure"].is_string());
        assert!(owner.living_tier_state("root").unwrap()["entitlements"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(owner
            .living_space_mutate("root", &command, 22, "assign")
            .is_err());
        let mut restored = Client::new(account, host, &failed.snapshot).unwrap();
        assert!(restored.living_mutation_status()["failure"].is_string());
        let update = restored
            .receive(&ServerFrame::SyncDone.encode(), 23)
            .unwrap();
        assert!(!update.frames.iter().any(|f| matches!(
            ClientFrame::decode(f).unwrap(),
            ClientFrame::Req {
                op: Op::ClaimKeyPackages { .. },
                ..
            }
        )));
    }
    #[test]
    fn living_mutation_tier_invitation_hash_scope_expiry_and_paid_refusal() {
        let (mut client, author, _, _) = fixture(61);
        community(&mut client, &author);
        let created = client
            .living_space_mutate(
                "root",
                r#"{"action":"tier_create","name":"Supporters"}"#,
                10,
                "tier",
            )
            .unwrap();
        let tier = created.result["tier_id"].as_str().unwrap();
        let command = json!({"action":"tier_invite","tier_id":tier,"expires_at_ms":100});
        let invite = client
            .living_space_mutate("root", &command.to_string(), 11, "invite")
            .unwrap();
        let url = invite.result["invite_url"].as_str().unwrap();
        let raw = url.split("token=").nth(1).unwrap();
        let state = client.living_tier_state("root").unwrap();
        assert!(client
            .living_validate_invitation("root", &state, raw, 12)
            .is_ok());
        assert!(client
            .living_validate_invitation("other", &state, raw, 12)
            .is_err());
        assert!(client
            .living_validate_invitation("root", &state, raw, 100)
            .is_err());
        let token: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(raw).unwrap()).unwrap();
        assert!(!serde_json::to_string(&state)
            .unwrap()
            .contains(token["secret"].as_str().unwrap()));
        assert!(client.living_space_mutate("root",&json!({"action":"tier_assign","tier_id":tier,"identity_id":client.living_identity(),"source":"paid"}).to_string(),12,"paid").is_err());
        let revoke = json!({"action":"tier_invite_revoke","invite_id":invite.result["invite_id"]});
        client
            .living_space_mutate("root", &revoke.to_string(), 13, "revoke")
            .unwrap();
        assert!(client
            .living_validate_invitation("root", &client.living_tier_state("root").unwrap(), raw, 14)
            .is_err());
    }
}
