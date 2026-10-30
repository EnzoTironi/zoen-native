use super::{changed, valid_id, MessageDeletion};
use crate::{
    engine::{Engine, Entry, EntryBody, State, R},
    CoreError, EntryKind, TimelineEntry,
};
use types::{EventBody, SpaceKind};

impl State {
    pub(crate) fn apply_private_message_deletion(
        &mut self,
        author: &str,
        space: &str,
        messages: &[String],
    ) {
        self.deleted_for_self
            .entry(author.to_owned())
            .or_default()
            .entry(space.to_owned())
            .or_default()
            .extend(messages.iter().cloned());
    }
    pub(crate) fn private_message_deletion_valid(
        &self,
        log: &str,
        author: &str,
        body: &EventBody,
    ) -> bool {
        let EventBody::MessagesDeletedForSelf { space, messages } = body else {
            return true;
        };
        let mut seen = std::collections::HashSet::new();
        space != log
            && valid_id(space)
            && !messages.is_empty()
            && messages.len() <= 100
            && messages.iter().all(|id| valid_id(id) && seen.insert(id))
            && self.spaces.get(log).is_some_and(|personal| {
                personal.kind == SpaceKind::Personal
                    && personal
                        .members
                        .iter()
                        .any(|(id, role)| id == author && *role == types::Role::Owner)
            })
    }
}

impl Engine {
    pub(crate) fn private_message_view_error(&self) -> Option<String> {
        self.broken_spaces
            .values()
            .any(|space| {
                space.members.is_empty()
                    || (space.kind == SpaceKind::Personal
                        && space
                            .members
                            .iter()
                            .any(|(owner, _)| Some(owner.as_str()) == self.me.as_deref()))
            })
            .then(|| {
                crate::i18n::t(
                    "não foi possível verificar o histórico privado",
                    "private history could not be verified",
                )
            })
    }

    pub(crate) fn check_private_message_view(&self) -> R<()> {
        if let Some(message) = self.private_message_view_error() {
            return Err(CoreError::Storage { message });
        }
        Ok(())
    }
    pub(crate) fn deleted_for_self(&self, space: &str, message: &str) -> bool {
        self.me.as_ref().is_some_and(|owner| {
            self.state
                .deleted_for_self
                .get(owner)
                .and_then(|spaces| spaces.get(space))
                .is_some_and(|messages| messages.contains(message))
        })
    }

    pub(crate) fn visible_message(&self, entry: &Entry) -> bool {
        self.private_message_view_error().is_none()
            && !self.deleted_for_self(&entry.space, entry.id())
    }

    /// A hidden thread parent can retain a content-free anchor for visible replies.
    pub(crate) fn redact_deleted_for_self(
        &self,
        entry: &Entry,
        mut dto: TimelineEntry,
    ) -> TimelineEntry {
        if !self.visible_message(entry) {
            dto.kind = EntryKind::Message {
                text: String::new(),
                card: None,
            };
            dto.deleted = true;
            dto.text_revision = None;
            dto.media = None;
            dto.voice = None;
            dto.reply_to = None;
            dto.reactions.clear();
            dto.forwarded = false;
            dto.forwarded_author = None;
            dto.edited = false;
        }
        dto
    }

    pub(super) fn delete_messages_for_self(&mut self, input: &MessageDeletion) -> R<Vec<String>> {
        if !self.is_member(&input.space_id, &input.expected_identity_id)
            || self.space_state(&input.space_id)?.kind == SpaceKind::Personal
        {
            return Err(CoreError::Forbidden {
                reason: crate::i18n::t("o acesso à conversa mudou", "conversation access changed"),
            });
        }
        if let Some(reason) = self.checked_integrity_error(&input.space_id)? {
            return Err(CoreError::Invalid { reason });
        }
        let state = self.space_state(&input.space_id)?;
        let mut pending = Vec::new();
        for target in &input.targets {
            if self.deleted_for_self(&input.space_id, &target.message_id) {
                continue;
            }
            let entry = state
                .entries
                .iter()
                .find(|entry| entry.id() == target.message_id)
                .ok_or_else(changed)?;
            let EntryBody::Message { marks, .. } = &entry.body else {
                return Err(changed());
            };
            let revision = if marks.deleted {
                entry.id()
            } else {
                marks.text_revision.as_deref().unwrap_or(entry.id())
            };
            if revision != target.expected_revision {
                return Err(changed());
            }
            pending.push(target.message_id.clone());
        }
        if !pending.is_empty() {
            self.atomic_agent_action(|engine| {
                let personal = engine.ensure_personal_space()?;
                if let Some(reason) = engine.checked_integrity_error(&personal)? {
                    return Err(CoreError::Invalid { reason });
                }
                // A local preference can never enter a shared/optimistic outbox.
                if engine.net.synced.contains(&personal) {
                    return Err(changed());
                }
                engine.append(
                    &personal,
                    &input.expected_identity_id,
                    EventBody::MessagesDeletedForSelf {
                        space: input.space_id.clone(),
                        messages: pending,
                    },
                )?;
                Ok(())
            })?;
        }
        Ok(input
            .targets
            .iter()
            .map(|target| target.message_id.clone())
            .collect())
    }
}
