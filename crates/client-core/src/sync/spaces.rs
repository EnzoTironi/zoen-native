//! Relay Space creation and membership transitions.

use super::*;

impl Engine {
    // ── Spaces on the relay ──

    pub fn create_synced_space(
        &mut self,
        title: &str,
        kind: SpaceKind,
        privacy: Privacy,
        members: &[IdentityId],
    ) -> R<SpaceId> {
        let me = self.me_id()?;
        if self.net.author.is_none() {
            return Err(CoreError::Forbidden {
                reason: t("Entre na sua conta primeiro.", "Sign in first."),
            });
        }
        let space = new_id("sp");
        self.store.set_synced(&space)?;
        self.net.synced.insert(space.clone());
        self.append(
            &space,
            &me,
            EventBody::SpaceCreated {
                title: title.to_string(),
                kind,
                privacy,
            },
        )?;
        if privacy == Privacy::EndToEnd {
            self.create_mls_group(&space)?;
        }
        for m in members.iter().filter(|m| **m != me) {
            self.append(
                &space,
                &me,
                EventBody::MemberAdded {
                    identity: m.clone(),
                    role: Role::Member,
                },
            )?;
        }
        Ok(space)
    }

    /// The existing DM with `who` on the relay, if any.
    pub fn direct_with(&self, who: &str) -> Option<SpaceId> {
        let me = self.me.clone()?;
        self.space_order
            .iter()
            .filter(|s| self.net.synced.contains(*s))
            .find(|s| {
                self.state.spaces.get(*s).is_some_and(|st| {
                    st.kind == SpaceKind::Direct
                        && st.members.len() == 2
                        && st.members.iter().any(|(m, _)| *m == me)
                        && st.members.iter().any(|(m, _)| m == who)
                })
            })
            .cloned()
    }

    pub fn start_direct(&mut self, who: &str) -> R<SpaceId> {
        if let Some(s) = self.direct_with(who) {
            return Ok(s);
        }
        if !self.identities.contains_key(who) {
            return Err(CoreError::NotFound {
                what: t("pessoa", "person"),
            });
        }
        self.create_synced_space("", SpaceKind::Direct, Privacy::EndToEnd, &[who.to_string()])
    }

    /// Makes a relay-readable chat or group end-to-end from here on (ADR 0027). Earlier
    /// messages stay as they were; the relay reads nothing written after this.
    pub fn encrypt_space(&mut self, space: &str) -> R<()> {
        let me = self.me_id()?;
        let s = self
            .state
            .spaces
            .get(space)
            .ok_or_else(|| CoreError::NotFound {
                what: t("conversa", "chat"),
            })?;
        if s.privacy == Privacy::EndToEnd {
            return Ok(());
        }
        let refuse = |pt: &str, en: &str| Err(CoreError::Invalid { reason: t(pt, en) });
        if !self.net.synced.contains(space) {
            return refuse(
                "Esta conversa só existe neste aparelho.",
                "This chat lives only on this device.",
            );
        }
        if s.privacy == Privacy::Public || !matches!(s.kind, SpaceKind::Direct | SpaceKind::Group) {
            return refuse(
                "Espaços públicos continuam legíveis.",
                "Public spaces stay readable.",
            );
        }
        let role = s.members.iter().find(|(m, _)| *m == me).map(|(_, r)| *r);
        let allowed = matches!(role, Some(Role::Owner | Role::Admin))
            || (role == Some(Role::Member) && s.kind == SpaceKind::Direct);
        if !allowed {
            return Err(CoreError::Forbidden {
                reason: t(
                    "Só quem administra o grupo ativa a criptografia.",
                    "Only the group's owners and admins can turn on encryption.",
                ),
            });
        }
        self.append(space, &me, EventBody::SpaceEncrypted)?;
        Ok(())
    }

    pub fn add_member(&mut self, space: &str, who: &str, role: Role) -> R<()> {
        let me = self.me_id()?;
        self.append(
            space,
            &me,
            EventBody::MemberAdded {
                identity: who.to_string(),
                role,
            },
        )?;
        Ok(())
    }

    /// Takes someone out of a chat. In an end-to-end chat they read nothing written after
    /// the commit that removes them, and nobody seals a message before it lands.
    pub fn remove_member(&mut self, space: &str, who: &str) -> R<()> {
        let me = self.me_id()?;
        let s = self
            .state
            .spaces
            .get(space)
            .ok_or_else(|| CoreError::NotFound {
                what: t("conversa", "chat"),
            })?;
        let role_of = |id: &str| s.members.iter().find(|(m, _)| m == id).map(|(_, r)| *r);
        let allowed = match (role_of(&me), role_of(who)) {
            (_, None) => {
                return Err(CoreError::NotFound {
                    what: t("membro", "member"),
                });
            }
            (Some(Role::Owner), _) => true,
            (Some(Role::Admin), Some(target)) => target != Role::Owner,
            _ => false,
        };
        if !allowed {
            return Err(CoreError::Forbidden {
                reason: t(
                    "Só quem administra a conversa remove pessoas.",
                    "Only the chat's owners and admins remove people.",
                ),
            });
        }
        self.append(
            space,
            &me,
            EventBody::MemberRemoved {
                identity: who.to_string(),
            },
        )?;
        Ok(())
    }

    pub fn leave_space(&mut self, space: &str) -> R<()> {
        let me = self.me_id()?;
        self.append(
            space,
            &me,
            EventBody::MemberRemoved {
                identity: me.clone(),
            },
        )?;
        Ok(())
    }

    /// Queues "I'm joining" for a Space we don't have yet; the relay checks the code.
    pub fn queue_join(&mut self, space: &str, role: Role, code: &str) -> R<()> {
        let me = self.me_id()?;
        let author = self.author_for(&me)?;
        let at = crate::engine::now_ms();
        let client_id = new_ulid(at);
        let e = author.sign_event(
            space,
            &client_id,
            at,
            None,
            EventBody::MemberAdded { identity: me, role },
        );
        self.store.set_meta(&format!("invite:{client_id}"), code)?;
        self.store.outbox_put(&e)?;
        self.store.set_synced(space)?;
        self.net.synced.insert(space.to_string());
        self.net.pending.insert(client_id, space.to_string());
        self.net.wake();
        Ok(())
    }
}
