//! A sealed acceptance remains true when its current presentation is unavailable.
use super::*;

impl Engine {
    pub(super) fn draft_source_revision(&self, space: &str, message: &str) -> Option<String> {
        let source = self
            .space_state(space)
            .ok()?
            .entries
            .iter()
            .find(|entry| entry.id() == message)?;
        match &source.body {
            EntryBody::Message { marks, .. } if !marks.deleted => Some(
                marks
                    .text_revision
                    .as_deref()
                    .unwrap_or(source.id())
                    .to_owned(),
            ),
            _ => None,
        }
    }

    pub(super) fn message_draft_acceptance(
        &self,
        scope: &DraftScope,
        receipt: &Receipt,
        draft: MessageDraftDto,
    ) -> MessageDraftSendResult {
        // The caller already authenticated the exact owner/device/scope and
        // decrypted a matching receipt. A projection failure cannot undo it.
        let entry = self.timeline(&scope.space_id).ok().and_then(|rows| {
            rows.into_iter()
                .find(|entry| entry.id == receipt.message_id && !entry.deleted)
        });
        MessageDraftSendResult {
            message_id: receipt.message_id.clone(),
            text_revision: receipt.text_revision.clone(),
            entry,
            draft,
        }
    }
}
