//! Chat summaries, receipts and signed conversation actions.
use super::{
    new_id, not_found, now_ms, reason_label, t, tr, ActionClass, CoreError, Decision, Engine,
    Entry, EntryBody, EventBody, IdentityKind, ItemContent, ItemId, ItemKind, MemberRoleDto,
    Persona, Privacy, PrivacyDto, ReqStatus, Role, SpaceKind, SpaceKindDto, SpaceSummary,
    TimelineEntry, R,
};

impl Engine {
    // ───────────────────────────── API pública ─────────────────────────────

    pub fn me(&self) -> R<Persona> {
        Ok(self.persona(&self.me_id()?))
    }

    pub fn personas(&self) -> Vec<Persona> {
        self.identity_order
            .iter()
            .map(|id| self.persona(id))
            .collect()
    }

    pub fn spaces(&self) -> Vec<SpaceSummary> {
        self.spaces_at(now_ms())
    }

    pub(crate) fn spaces_at(&self, now: i64) -> Vec<SpaceSummary> {
        let mut out: Vec<SpaceSummary> = self
            .space_order
            .iter()
            .filter_map(|id| self.space_summary_at(id, now).ok())
            .collect();
        out.sort_by_key(|s| std::cmp::Reverse(s.last_at_ms));
        out
    }

    pub fn space_summary(&self, id: &str) -> R<SpaceSummary> {
        self.space_summary_at(id, now_ms())
    }

    pub(crate) fn space_summary_at(&self, id: &str, now: i64) -> R<SpaceSummary> {
        let s = self.space_state(id)?;
        let saved = self.saved_messages_space(id);
        let kind = if saved {
            SpaceKindDto::Saved
        } else {
            match s.kind {
                SpaceKind::Direct => SpaceKindDto::Direct,
                SpaceKind::Group => SpaceKindDto::Group,
                SpaceKind::Community => SpaceKindDto::Community,
                SpaceKind::Personal => return Err(not_found(&t("Espaço", "Space"))),
            }
        };
        let me = self.me.clone().unwrap_or_default();
        let members: Vec<Persona> = s.members.iter().map(|(m, _)| self.persona(m)).collect();
        let counterpart = if s.kind == SpaceKind::Direct {
            members.iter().find(|p| p.id != me).cloned()
        } else {
            None
        };
        let private_error = self.private_message_view_error();
        let (last_preview, last_preview_is_markdown, last_author, last_delivery) =
            match s.integrity_error.as_ref().or(private_error.as_ref()) {
                Some(err) => (
                    tr!("⚠︎ Log inválido: {err}", "⚠︎ Invalid log: {err}"),
                    false,
                    None,
                    None,
                ),
                None => self.preview(s),
            };
        let read_seq = self.read_up_to(id);
        let unread = s
            .entries
            .iter()
            .filter(|e| e.author != me && read_seq.is_none_or(|r| e.seq > r))
            .filter(|e| self.visible_message(e))
            .filter(|e| matches!(e.body, EntryBody::Message { .. }))
            .count() as u32;
        let pending = self
            .state
            .requests
            .values()
            .filter(|r| r.space == id && r.status == ReqStatus::Pending)
            .count() as u32;
        let prefs = self.state.chat_prefs.get(id).cloned().unwrap_or_default();
        let muted = prefs.muted_until_ms.is_some_and(|until| until > now);
        Ok(SpaceSummary {
            manual_unread: prefs.manual_unread,
            pinned: prefs.pinned,
            archived: prefs.archived,
            muted,
            muted_until_ms: prefs.muted_until_ms.filter(|_| muted),
            id: s.id.clone(),
            title: if saved {
                t("Mensagens salvas", "Saved Messages")
            } else {
                counterpart
                    .as_ref()
                    .map_or_else(|| s.title.clone(), |c| c.name.clone())
            },
            kind,
            privacy: match s.privacy {
                Privacy::EndToEnd => PrivacyDto::EndToEnd,
                Privacy::Closed => PrivacyDto::Closed,
                Privacy::Public => PrivacyDto::Public,
            },
            can_write: s.integrity_error.is_none()
                && self.state.writer(id, &me)
                && (!saved
                    || (self.saved_messages_canonical(id) && self.ready_epoch(id).is_some())),
            members,
            counterpart,
            last_preview,
            last_preview_is_markdown,
            last_author,
            last_at_ms: if self
                .state
                .deleted_for_self
                .get(&me)
                .is_some_and(|spaces| spaces.contains_key(id))
            {
                s.entries
                    .iter()
                    .filter(|entry| self.visible_message(entry))
                    .map(|entry| entry.at_ms)
                    .max()
                    .unwrap_or(0)
            } else {
                s.last_at_ms
            },
            unread,
            last_delivery,
            pending_requests: pending,
            event_count: self.logs.get(id).map_or(0, |l| l.len() as u64),
        })
    }

    pub fn member_roles(&self, space: &str) -> R<Vec<MemberRoleDto>> {
        self.space_state(space)?;
        Ok(self
            .receipt_members(space)?
            .iter()
            .map(|(identity, role)| MemberRoleDto {
                identity_id: identity.clone(),
                role: match role {
                    Role::Owner => "owner",
                    Role::Admin => "admin",
                    Role::Member => "member",
                    Role::Reader => "reader",
                }
                .into(),
            })
            .collect())
    }

    /// Marks the chat read here and returns the watermark to tell the others (and my other
    /// devices), if it moved.
    pub fn mark_read(&mut self, space: &str) -> R<Option<u64>> {
        self.space_state(space)?;
        self.commit_chat_read(space, self.read_target_seq(space), None)
    }

    pub(crate) fn read_up_to(&self, space: &str) -> Option<u64> {
        self.store
            .meta(&format!("read:{space}"))
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
    }

    pub fn timeline(&self, space: &str) -> R<Vec<TimelineEntry>> {
        self.check_private_message_view()?;
        let s = self.space_state(space)?;
        let roots: std::collections::HashSet<&str> = s
            .entries
            .iter()
            .filter(|entry| self.visible_message(entry))
            .filter_map(|entry| match &entry.body {
                EntryBody::Message {
                    reply: Some(reply), ..
                } if reply.thread => Some(reply.to.as_str()),
                _ => None,
            })
            .collect();
        let mut out: Vec<TimelineEntry> = s
            .entries
            .iter()
            .filter(|entry| self.visible_message(entry) || roots.contains(entry.id()))
            .map(|e| self.entry_dto(e))
            .collect();
        crate::replies::count_thread_replies(&mut out);
        Ok(out)
    }

    pub fn send_message(&mut self, space: &str, text: &str) -> R<TimelineEntry> {
        if let Some(reason) = self.checked_integrity_error(space)? {
            return Err(CoreError::Invalid { reason });
        }
        let me = self.me_id()?;
        self.post_as(space, &me, text, None)
    }

    pub(crate) fn validate_message_edit_text(
        &self,
        space: &str,
        entry: &Entry,
        text: &str,
    ) -> R<()> {
        // Empty text clears a file's caption, but must never erase the body of
        // an ordinary message, voice note or miniapp card. Resolve the Item
        // in this exact Space instead of trusting the caller's card type.
        let file_caption = match &entry.body {
            EntryBody::Message {
                attaches: Some(item),
                ..
            } => self
                .space_projections
                .get(space)
                .and_then(|scoped| scoped.items.get(item))
                .is_some_and(|item| {
                    item.kind == ItemKind::File
                        && matches!(&item.current().content, ItemContent::File(file) if !file.is_voice())
                }),
            _ => false,
        };
        if file_caption {
            if text.len() > crate::files::MAX_FILE_CAPTION_BYTES || text.contains('\0') {
                return Err(CoreError::Invalid {
                    reason: t("legenda de arquivo inválida", "invalid file caption"),
                });
            }
        } else if text.trim().is_empty() {
            return Err(CoreError::Invalid {
                reason: t("mensagem vazia", "empty message"),
            });
        }
        Ok(())
    }

    /// Edit, delete for everyone, react or unreact on a message in `space`. Only the author
    /// may edit or delete; everyone in the Space may react.
    pub fn annotate(&mut self, space: &str, target: &str, note: types::Annotation) -> R<()> {
        let me = self.me_id()?;
        let s = self.space_state(space)?;
        let Some(
            entry @ Entry {
                author,
                body: EntryBody::Message { marks, .. },
                ..
            },
        ) = s.entries.iter().find(|e| e.id() == target)
        else {
            return Err(not_found(&t("Mensagem", "Message")));
        };
        if !self.visible_message(entry) {
            return Err(not_found(&t("Mensagem", "Message")));
        }
        if marks.deleted || (note.author_only() && *author != me) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "só quem escreveu edita ou apaga",
                    "only the author can edit or delete",
                ),
            });
        }
        if let types::Annotation::React { emoji } = &note {
            if emoji.is_empty() || emoji.len() > 32 {
                return Err(CoreError::Invalid {
                    reason: t("reação inválida", "invalid reaction"),
                });
            }
        }
        if let types::Annotation::Edit { text } = &note {
            self.validate_message_edit_text(space, entry, text)?;
        }
        if !self.state.writer(space, &me) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "só membros escrevem neste Espaço",
                    "only members can write in this Space",
                ),
            });
        }
        self.append(
            space,
            &me,
            EventBody::MessageAnnotated {
                target: target.to_string(),
                note,
            },
        )?;
        Ok(())
    }

    /// Sends a copy of a message (text and attachment) to another chat, marked forwarded.
    pub fn forward(&mut self, from_space: &str, target: &str, to_space: &str) -> R<TimelineEntry> {
        let me = self.me_id()?;
        if !self.is_member(from_space, &me) || !self.state.writer(to_space, &me) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "só membros encaminham mensagens para conversas em que podem escrever",
                    "only members can forward messages to chats they can write in",
                ),
            });
        }
        let source = self
            .space_state(from_space)?
            .entries
            .iter()
            .find(|entry| entry.id() == target)
            .ok_or_else(|| not_found(&t("Mensagem", "Message")))?;
        if !self.visible_message(source) {
            return Err(not_found(&t("Mensagem", "Message")));
        }
        let EntryBody::Message {
            text,
            attaches,
            marks,
            ..
        } = &source.body
        else {
            return Err(not_found(&t("Mensagem", "Message")));
        };
        if marks.deleted {
            return Err(not_found(&t("Mensagem", "Message")));
        }
        let media = marks.media.as_ref().map(types::MessageMedia::without_group);
        let (text, attaches) = (text.clone(), attaches.clone());
        // Resolve the canonical entry, including old Android markers. Its current
        // audio and metadata are copied; source item/reply/thread ids never travel.
        if from_space != to_space {
            if let Some(voice) = self.entry_voice(source) {
                return self.forward_file_message(
                    from_space,
                    &voice.item_id,
                    to_space,
                    "",
                    Some(&voice),
                    None,
                );
            }
            if text.starts_with("⟦voice:") {
                return Err(CoreError::Invalid {
                    reason: t(
                        "esta mensagem de voz não está mais disponível",
                        "this voice message is no longer available",
                    ),
                });
            }
            if let Some(item) = attaches.as_deref() {
                if self.state.items.get(item).is_some_and(|item| item.space==from_space && matches!(&item.current().content,ItemContent::App(app) if app.app=="excalidraw")) {
                    return self.forward_app_document(from_space,item,to_space,&text);
                }
                if !self.state.items.get(item).is_some_and(|item| {
                    item.space == from_space
                        && matches!(&item.current().content, ItemContent::File(_))
                }) {
                    return Err(CoreError::Invalid {
                        reason: t(
                            "este item ainda não pode ser encaminhado a outra conversa",
                            "this item cannot be forwarded to another chat yet",
                        ),
                    });
                }
                return self.forward_file_message(from_space, item, to_space, &text, None, media);
            }
        }
        let appended = self.append(
            to_space,
            &me,
            EventBody::MessagePosted {
                message: new_id("msg")?,
                text,
                attaches,
                reply: None,
                media,
                forwarded: true,
            },
        )?;
        let s = self.space_state(to_space)?;
        let entry = s
            .entries
            .iter()
            .find(|entry| entry.id() == appended.client_id)
            .ok_or_else(|| CoreError::Storage {
                message: "The appended entry is missing from its projection.".into(),
            })?;
        Ok(self.entry_dto(entry))
    }

    /// The Engine lock covers owner validation, current preference merge and the
    /// existing signed Personal-log writer. This is not distributed compare-and-set.
    pub(crate) fn set_chat_muted(
        &mut self,
        space: &str,
        muted: bool,
        identity: &str,
        device: &str,
    ) -> R<()> {
        self.chat_mute_owner(space, identity, device)?;
        let mut prefs = self
            .state
            .chat_prefs
            .get(space)
            .cloned()
            .unwrap_or_default();
        prefs.muted_until_ms = muted.then_some(i64::MAX);
        self.set_chat_prefs(space, prefs)
    }

    /// Pins, archives or mutes one of my chats in its signed local Personal log.
    /// Replication of this log across devices is a separate transport contract.
    pub fn set_chat_prefs(&mut self, space: &str, prefs: types::ChatPrefs) -> R<()> {
        let me = self.me_id()?;
        self.space_state(space)?;
        if self.state.chat_prefs.get(space) == Some(&prefs) {
            return Ok(());
        }
        let personal = self.ensure_personal_space()?;
        self.append(
            &personal,
            &me,
            EventBody::ChatPrefsSet {
                space: space.to_string(),
                prefs,
                operation: None,
            },
        )?;
        Ok(())
    }

    pub(crate) fn post_as(
        &mut self,
        space: &str,
        author: &str,
        text: &str,
        attaches: Option<ItemId>,
    ) -> R<TimelineEntry> {
        if !self.state.writer(space, author) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "só membros escrevem neste Espaço",
                    "only members can write in this Space",
                ),
            });
        }
        let text = text.trim();
        if text.is_empty() && attaches.is_none() {
            return Err(CoreError::Invalid {
                reason: t("mensagem vazia", "empty message"),
            });
        }
        let appended = self.append(
            space,
            author,
            EventBody::MessagePosted {
                message: new_id("msg")?,
                text: text.to_string(),
                attaches,
                reply: None,
                media: None,
                forwarded: false,
            },
        )?;
        let s = self.space_state(space)?;
        let entry = s
            .entries
            .iter()
            .find(|entry| entry.id() == appended.client_id)
            .ok_or_else(|| CoreError::Storage {
                message: "The appended entry is missing from its projection.".into(),
            })?;
        Ok(self.entry_dto(entry))
    }

    /// O agente responde com texto (ação `Reply`, avaliada contra Concessões e orçamento).
    pub fn agent_say(
        &mut self,
        space: &str,
        agent: &str,
        text: &str,
        ai_cost_cents: i64,
    ) -> R<TimelineEntry> {
        if !self.state.writer(space, agent)
            || !self
                .identities
                .get(agent)
                .is_some_and(|a| a.kind == IdentityKind::Agent && a.owner.is_some())
        {
            return Err(CoreError::Forbidden {
                reason: t(
                    "Agente sem permissão para escrever.",
                    "Agent is not allowed to write.",
                ),
            });
        }
        if ai_cost_cents < 0 {
            return Err(CoreError::Invalid {
                reason: t("custo de IA negativo", "negative AI cost"),
            });
        }
        match self.decide(agent, space, &ActionClass::Reply, ai_cost_cents) {
            Decision::Block(r) => Err(CoreError::Forbidden {
                reason: reason_label(&r, &self.policy),
            }),
            _ => self.atomic_agent_action(|engine| {
                let e = engine.post_as(space, agent, text, None)?;
                if ai_cost_cents > 0 {
                    engine.append(
                        space,
                        agent,
                        EventBody::UsageRecorded {
                            agent: agent.into(),
                            cents: ai_cost_cents,
                            what: "resposta".into(),
                        },
                    )?;
                }
                Ok(e)
            }),
        }
    }
}
