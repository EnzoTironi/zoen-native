use crate::{
    engine::{now_ms, Engine},
    CoreError,
};
use roda_types::{
    hackathon::Hackathon, EventBody, ItemContent, ItemKind, Privacy, Role, SpaceKind,
};
use serde_json::{json, Value};
pub(crate) const HACK_META: &str = "zoen-living-space-hackathon-v1:";
type R<T> = Result<T, CoreError>;
fn invalid(s: impl ToString) -> CoreError {
    CoreError::Invalid {
        reason: s.to_string(),
    }
}

impl Engine {
    pub(crate) fn living_hackathon_replay(&self, space: &str) -> Option<Hackathon> {
        let mut roles = std::collections::BTreeMap::new();
        let mut hack = None;
        for event in self.living_events(space) {
            match &event.body {
                EventBody::SpaceCreated {
                    kind: SpaceKind::Community,
                    ..
                } => {
                    roles.insert(event.author.clone(), Role::Owner);
                }
                EventBody::MemberAdded { identity, role } => {
                    roles.insert(identity.clone(), *role);
                }
                EventBody::MemberRemoved { identity } => {
                    roles.remove(identity);
                }
                EventBody::MessagePosted { text, .. } => {
                    let Some(role) = roles.get(&event.author).copied() else {
                        continue;
                    };
                    let Some(raw) = text.strip_prefix(HACK_META) else {
                        continue;
                    };
                    let Ok(value) = serde_json::from_str::<Value>(raw) else {
                        continue;
                    };
                    if value["action"] == "hackathon_create" && hack.is_none() {
                        hack = Hackathon::create(&event.author, role, event.at_ms, &value).ok();
                    } else if let Some(state) = hack.as_mut() {
                        let _ = state.apply(&event.author, role, event.at_ms, &value);
                    }
                }
                _ => {}
            }
        }
        hack
    }

    pub(crate) fn living_hackathon_home(&self, space: &str) -> Value {
        let Some(me) = self.me.as_deref() else {
            return Value::Null;
        };
        let role = self
            .state
            .spaces
            .get(space)
            .and_then(|s| s.members.iter().find(|(id, _)| id == me).map(|(_, r)| *r));
        match (self.living_hackathon_replay(space), role) {
            (Some(h), Some(role)) => h.projection(me, role),
            _ => Value::Null,
        }
    }
    pub(crate) fn living_hackathon_hidden(&self, child: &str) -> bool {
        self.state.spaces.keys().any(|space| {
            self.living_hackathon_replay(space)
                .is_some_and(|h| h.teams.iter().any(|t| t.space == child))
        })
    }
    pub(crate) fn living_hackathon_command(&mut self, space: &str, command: &Value) -> R<Value> {
        let me = self.me_id()?;
        let role = self
            .space_state(space)?
            .members
            .iter()
            .find(|(id, _)| id == &me)
            .map(|(_, r)| *r)
            .ok_or_else(|| invalid("Space membership required"))?;
        let mut command = command.clone();
        let action = command["action"]
            .as_str()
            .ok_or_else(|| invalid("Action required"))?
            .to_owned();
        let now = now_ms();
        let mut hack = self.living_hackathon_replay(space);
        if action == "hackathon_create" {
            if hack.is_some() {
                return Err(invalid("A hackathon already exists"));
            }
            command["id"] = json!(roda_types::new_id("hack"));
            hack = Some(Hackathon::create(&me, role, now, &command).map_err(invalid)?);
        } else {
            let state = hack.as_mut().ok_or_else(|| invalid("No hackathon"))?;
            if action == "finalize" && state.finalized {
                self.living_publish_winners(space, state)?;
                return Ok(json!({"ok":true}));
            }
            if action == "team_create" {
                command["id"] = json!(roda_types::new_id("team"));
                // Validate before creating a real encrypted group; replace this placeholder
                // only after the existing MLS creation path succeeds.
                command["space"] = json!("pending-mls-validation");
                let mut trial = state.clone();
                trial.apply(&me, role, now, &command).map_err(invalid)?;
                let name = command["name"]
                    .as_str()
                    .ok_or_else(|| invalid("Team name required"))?;
                let child =
                    self.create_synced_space(name, SpaceKind::Group, Privacy::EndToEnd, &[])?;
                command["space"] = json!(child);
            }
            if action == "project_submit" {
                let team_id = command["team_id"]
                    .as_str()
                    .ok_or_else(|| invalid("Team required"))?;
                let team = state
                    .teams
                    .iter()
                    .find(|t| t.id == team_id)
                    .ok_or_else(|| invalid("Team not found"))?;
                for file in command["file_ids"]
                    .as_array()
                    .ok_or_else(|| invalid("Invalid file IDs"))?
                {
                    let id = file.as_str().ok_or_else(|| invalid("Invalid file ID"))?;
                    let item = self
                        .state
                        .items
                        .get(id)
                        .ok_or_else(|| invalid("Submission file not found"))?;
                    if item.kind != ItemKind::File
                        || (item.space != space && item.space != team.space)
                    {
                        return Err(invalid(
                            "Submission files must belong to this Space or team",
                        ));
                    }
                }
            }
            let mut trial = state.clone();
            trial.apply(&me, role, now, &command).map_err(invalid)?;
            if action == "team_join" {
                if let Some(who) = command["identity_id"].as_str().filter(|who| *who != me) {
                    let team = state
                        .teams
                        .iter()
                        .find(|t| Some(t.id.as_str()) == command["team_id"].as_str())
                        .ok_or_else(|| invalid("Team not found"))?;
                    // Existing group owner/admin authorization and MLS membership commit
                    // are authoritative; a parent admin without group authority is refused.
                    self.add_member(&team.space, who, Role::Member)?;
                }
            }
            *state = trial;
        }
        self.append(
            space,
            &me,
            EventBody::MessagePosted {
                message: roda_types::new_id("msg"),
                text: format!("{HACK_META}{command}"),
                attaches: None,
                reply: None,
                forwarded: false,
            },
        )?;
        if action == "finalize" {
            self.living_publish_winners(space, hack.as_ref().expect("validated hackathon"))?;
        }
        Ok(json!({"ok":true}))
    }
    fn living_publish_winners(&mut self, space: &str, hack: &Hackathon) -> R<()> {
        let me = self.me_id()?;
        let item = format!("it_{}_winners", hack.id);
        if !self.state.items.contains_key(&item) {
            let names = hack
                .teams
                .iter()
                .filter(|t| hack.winners.contains(&t.id))
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            self.append(
                space,
                &me,
                EventBody::ItemCreated {
                    item: item.clone(),
                    kind: ItemKind::Note,
                    content: ItemContent::Text {
                        text: format!("{} — Winners: {}", hack.title, names),
                    },
                    origin: "Hackathon results".into(),
                },
            )?;
        }
        if self
            .state
            .living_announcements
            .get(space)
            .is_some_and(|items| items.contains(&item))
        {
            return Ok(());
        }
        // Idempotent announcement labels; stable Item ID avoids duplicate result items
        // if publication is retried after a partial send.
        self.append(
            space,
            &me,
            EventBody::MessagePosted {
                message: roda_types::new_id("msg"),
                text: format!(
                    "{}{}",
                    crate::living_space::META_PREFIX,
                    json!({"action":"announce","item_id":item})
                ),
                attaches: None,
                reply: None,
                forwarded: false,
            },
        )?;
        Ok(())
    }
}
