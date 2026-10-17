//! Bounded, hidden MLS partitions for a living Community's human conversations.
//! This is an admission policy, not evidence of large-scale throughput or cross-shard
//! message fanout: each member uses the partition to which it was admitted.
use crate::{engine::Engine, CoreError};
use roda_types::{EventBody, Privacy, Role, SpaceKind};
use serde_json::{json, Value};
type R<T> = Result<T, CoreError>;
fn invalid(s: &str) -> CoreError {
    CoreError::Invalid { reason: s.into() }
}

impl Engine {
    pub(crate) fn living_conversation_policy(&self, root: &str) -> (usize, Vec<String>) {
        let mut cap = 256;
        let mut groups = vec![];
        let mut roles = std::collections::BTreeMap::new();
        for event in self.living_events(root) {
            match event.body {
                EventBody::SpaceCreated { .. } => {
                    roles.insert(event.author, Role::Owner);
                }
                EventBody::MemberAdded { identity, role } => {
                    roles.insert(identity, role);
                }
                EventBody::MemberRemoved { identity } => {
                    roles.remove(&identity);
                }
                EventBody::MessagePosted { text, .. }
                    if matches!(roles.get(&event.author), Some(Role::Owner | Role::Admin)) =>
                {
                    let Some(raw) = text.strip_prefix(crate::living_space::META_PREFIX) else {
                        continue;
                    };
                    let Ok(value) = serde_json::from_str::<Value>(raw) else {
                        continue;
                    };
                    match value["action"].as_str() {
                        Some("conversation_policy") => {
                            if let Some(n) = value["member_cap"]
                                .as_u64()
                                .filter(|n| (2..=4096).contains(n))
                            {
                                cap = n as usize;
                            }
                        }
                        Some("conversation_partition") => {
                            if let Some(id) = value["child"].as_str() {
                                if !groups.iter().any(|g| g == id) {
                                    groups.push(id.to_owned());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        (cap, groups)
    }
    fn living_conversation_meta(&mut self, root: &str, value: Value) -> R<()> {
        let me = self.me_id()?;
        self.append(
            root,
            &me,
            EventBody::MessagePosted {
                message: roda_types::new_id("msg"),
                text: format!("{}{}", crate::living_space::META_PREFIX, value),
                attaches: None,
                reply: None,
                forwarded: false,
            },
        )?;
        Ok(())
    }
    pub(crate) fn living_conversation_set_cap(&mut self, root: &str, value: &Value) -> R<Value> {
        let me = self.me_id()?;
        if !self
            .space_state(root)?
            .members
            .iter()
            .any(|(id, r)| id == &me && matches!(r, Role::Owner | Role::Admin))
        {
            return Err(invalid("Administrators only"));
        }
        let cap = value["member_cap"]
            .as_u64()
            .filter(|n| (2..=4096).contains(n))
            .ok_or_else(|| invalid("Member cap must be 2–4096"))?;
        let (_, groups) = self.living_conversation_policy(root);
        if groups.iter().any(|id| {
            self.state
                .spaces
                .get(id)
                .is_some_and(|s| s.members.len() > cap as usize)
        }) {
            return Err(invalid(
                "Cannot shrink below an existing encrypted partition's membership",
            ));
        }
        self.living_conversation_meta(
            root,
            json!({"action":"conversation_policy","member_cap":cap}),
        )?;
        self.living_conversation_ensure(root)?;
        Ok(json!({"ok":true}))
    }
    pub(crate) fn living_conversation_ensure(&mut self, root: &str) -> R<()> {
        if self.net.author.is_none() {
            return Ok(());
        } // Demo has no actual MLS identity.
        let me = self.me_id()?;
        let parent = self.space_state(root)?;
        if parent.kind != SpaceKind::Community
            || !parent
                .members
                .iter()
                .any(|(id, r)| id == &me && matches!(r, Role::Owner | Role::Admin))
        {
            return Ok(());
        }
        let members = parent.members.clone();
        let (cap, mut groups) = self.living_conversation_policy(root);
        for (identity, role) in members {
            if self
                .identities
                .get(&identity)
                .is_some_and(|i| i.kind == roda_types::IdentityKind::Agent)
            {
                continue;
            }
            if groups.iter().any(|g| self.is_member(g, &identity)) {
                continue;
            }
            let available = groups
                .iter()
                .find(|g| {
                    self.state.spaces.get(*g).is_some_and(|s| {
                        s.privacy == Privacy::EndToEnd
                            && s.members.len() < cap
                            && s.members
                                .iter()
                                .any(|(id, r)| id == &me && matches!(r, Role::Owner | Role::Admin))
                    })
                })
                .cloned();
            let child = match available {
                Some(child) => child,
                None => {
                    let child = self.create_synced_space(
                        "Conversations",
                        SpaceKind::Group,
                        Privacy::EndToEnd,
                        &[],
                    )?;
                    self.living_conversation_meta(
                        root,
                        json!({"action":"conversation_partition","child":child}),
                    )?;
                    groups.push(child.clone());
                    child
                }
            };
            if identity != me {
                self.add_member(
                    &child,
                    &identity,
                    if matches!(role, Role::Owner | Role::Admin) {
                        Role::Admin
                    } else {
                        role
                    },
                )?;
            }
        }
        Ok(())
    }
    pub(crate) fn living_conversation_space(&self, root: &str, who: &str) -> Option<String> {
        if !self.is_member(root, who) {
            return None;
        }
        self.living_conversation_policy(root)
            .1
            .into_iter()
            .find(|g| self.is_member(g, who))
    }
    pub(crate) fn living_conversation_remove(&mut self, root: &str, who: &str) -> R<()> {
        if self.space_state(root)?.kind != SpaceKind::Community {
            return Ok(());
        }
        let me = self.me_id()?;
        for child in self.living_conversation_policy(root).1 {
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
}
