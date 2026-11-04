//! Reviewed message-scoped media edits. Existing signed events are the durable receipts.
mod projection;
use crate::{
    engine::{Engine, Entry, EntryBody, R},
    Client, CoreError, TimelineEntry,
};
pub use protocol::message_media_edit::MessageMediaEdit;
use serde::Serialize;
use types::{Annotation, Event, EventBody, MediaChange};

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct MessageMediaEditResult {
    pub entry: TimelineEntry,
    pub applied_revision: String,
}
fn changed() -> CoreError {
    CoreError::Stale {
        reason: crate::i18n::t(
            "a mensagem ou a mídia mudou; reveja a edição",
            "the message or media changed; review the edit",
        ),
    }
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control)
}
fn valid_change(edit: &MediaChange) -> bool {
    !edit.mutation_id.is_empty()
        && edit.mutation_id.len() <= 128
        && edit
            .mutation_id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
        && valid_id(&edit.expected_revision)
        && valid_id(&edit.replacement.item_id)
        && edit.replacement.version > 0
        && edit.replacement.sha256.len() == 64
        && edit
            .replacement
            .sha256
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        && edit.text.len() <= crate::files::MAX_FILE_CAPTION_BYTES
        && !edit.text.contains('\0')
}
impl Client {
    pub fn replace_message_media(&self, input: MessageMediaEdit) -> R<MessageMediaEditResult> {
        self.lock().replace_message_media(&input)
    }
}
impl Engine {
    /// Incremental confirmation must not apply a remote edit on top of a later optimistic edit.
    pub(crate) fn media_edit_needs_replay(&self, event: &Event) -> bool {
        match &event.body {
            EventBody::MessageAnnotated { note: Annotation::ReplaceMedia { .. }, .. } => true,
            EventBody::MessageAnnotated { target, note: Annotation::Edit { .. } | Annotation::Delete } => {
                self.state.spaces.get(&event.space).is_some_and(|space| space.entries.iter().any(|entry| entry.id() == target && matches!(&entry.body, EntryBody::Message { marks, .. } if marks.replacement().is_some())))
            }
            _ => false,
        }
    }

    pub(crate) fn validate_media_edit_target(&self, input: &MessageMediaEdit) -> R<()> {
        self.replacement_source(input)?;
        if !self
            .space_projections
            .get(&input.space_id)
            .is_some_and(|state| {
                state.message_replacement_target_valid(
                    &input.space_id,
                    &input.expected_identity_id,
                    &input.message_id,
                    &input.edit,
                )
            })
        {
            return Err(changed());
        }
        Ok(())
    }
    fn replacement_source(&self, input: &MessageMediaEdit) -> R<&Entry> {
        self.native_surface_owner(&input.expected_identity_id, &input.expected_device_id)?;
        self.check_private_message_view()?;
        if !valid_id(&input.space_id)
            || !valid_id(&input.message_id)
            || !valid_change(&input.edit)
            || !self
                .state
                .writer(&input.space_id, &input.expected_identity_id)
            || self.checked_integrity_error(&input.space_id)?.is_some()
        {
            return Err(changed());
        }
        self.space_state(&input.space_id)?.entries.iter().find(|entry| {
            entry.id() == input.message_id && entry.author == input.expected_identity_id && self.visible_message(entry)
                && matches!(&entry.body, EntryBody::Message { marks, .. } if !marks.deleted && !marks.forwarded)
        }).ok_or_else(changed)
    }
    fn replacement_receipt(
        &self,
        input: &MessageMediaEdit,
        event: &Event,
        failed: bool,
    ) -> R<Option<String>> {
        let EventBody::MessageAnnotated {
            target,
            note: Annotation::ReplaceMedia { edit },
        } = &event.body
        else {
            return Ok(None);
        };
        if event.author != input.expected_identity_id || edit.mutation_id != input.edit.mutation_id
        {
            return Ok(None);
        }
        if failed
            || event.space != input.space_id
            || target != &input.message_id
            || edit != &input.edit
            || event.device.as_deref() != Some(input.expected_device_id.as_str())
        {
            return Err(changed());
        }
        Ok(Some(event_log::content_hash_of(event)))
    }
    pub(crate) fn replace_message_media(
        &mut self,
        input: &MessageMediaEdit,
    ) -> R<MessageMediaEditResult> {
        self.replacement_source(input)?;
        let mut accepted = None;
        for log in self.logs.values() {
            for event in log.events() {
                if let Some(receipt) = self.replacement_receipt(input, event, false)? {
                    accepted = Some(receipt);
                }
            }
        }
        for pending in self.store.outbox()? {
            if let Some(receipt) =
                self.replacement_receipt(input, &pending.event, pending.failed)?
            {
                accepted = Some(receipt);
            }
        }
        let applied_revision = if let Some(receipt) = accepted {
            receipt
        } else {
            let body = EventBody::MessageAnnotated {
                target: input.message_id.clone(),
                note: Annotation::ReplaceMedia {
                    edit: input.edit.clone(),
                },
            };
            if !self
                .space_projections
                .get(&input.space_id)
                .is_some_and(|state| {
                    state.message_replacement_valid(
                        &input.space_id,
                        &input.expected_identity_id,
                        &body,
                    )
                })
            {
                return Err(changed());
            }
            let event = self.append(&input.space_id, &input.expected_identity_id, body)?;
            event_log::content_hash_of(&event)
        };
        let source = self.replacement_source(input)?;
        if !matches!(&source.body, EntryBody::Message { marks, .. } if marks.media_edit_receipt(&input.edit.mutation_id) == Some(applied_revision.as_str()))
        {
            return Err(changed());
        }
        Ok(MessageMediaEditResult {
            entry: self.entry_dto(source),
            applied_revision,
        })
    }
}
