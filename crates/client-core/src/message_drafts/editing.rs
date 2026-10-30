//! The edit slot shares the existing sealed composer record, CAS and quota.
use super::*;
use crate::MessageEdit;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct MessageDraftEdit {
    pub message_id: String,
    pub source_revision: String,
    /// Private preview only; never proof of source authority.
    pub base_text: String,
    pub text: String,
    /// Presentation hint; publication resolves the real attachment in the Space.
    pub is_caption: bool,
}

impl MessageDraftEdit {
    fn input(&self, scope: &DraftScope) -> MessageEdit {
        MessageEdit {
            expected_identity_id: scope.expected_identity_id.clone(),
            expected_device_id: scope.expected_device_id.clone(),
            space_id: scope.space_id.clone(),
            message_id: self.message_id.clone(),
            expected_revision: self.source_revision.clone(),
            text: self.text.clone(),
        }
    }
}
fn rejected(error: CoreError) -> CoreError {
    match error {
        CoreError::Stale { reason }
        | CoreError::Forbidden { reason }
        | CoreError::Invalid { reason } => CoreError::DraftRejected { reason },
        other => other,
    }
}
impl Engine {
    pub(super) fn validate_edit_shape(edit: Option<&MessageDraftEdit>) -> R<()> {
        if edit.is_some_and(|e| {
            !valid_id(&e.message_id)
                || !valid_id(&e.source_revision)
                || e.text.len() > crate::client_api::MAX_REQUEST_BYTES
                || e.base_text.len() > crate::client_api::MAX_REQUEST_BYTES
        }) {
            return Err(invalid());
        }
        Ok(())
    }

    pub(crate) fn send_message_edit_draft(
        &mut self,
        scope: &DraftScope,
        expected: &str,
    ) -> R<MessageDraftSendResult> {
        let storage = self.draft_storage_scope(scope)?;
        let mut record = self.load_message_draft(scope, &storage)?;
        if !valid_revision(expected) || expected == MESSAGE_DRAFT_REVISION_ZERO {
            return Err(stale());
        }
        if let Some(receipt) = record
            .last_accepted_edit
            .as_ref()
            .filter(|r| r.input_revision == expected)
        {
            return Ok(self.message_draft_acceptance(scope, receipt, record.draft));
        }
        if record.draft.revision != expected {
            return Err(stale());
        }
        let edit = record.draft.edit.as_ref().ok_or_else(stale)?;
        let input = edit.input(scope);
        // Receipt absence and exact CAS are established before a definitive
        // refusal. Storage/transport failures never get that classification.
        self.draft_target(scope, Some(&input.message_id))
            .map_err(rejected)?;
        let unchanged = self.validate_reviewed_message(&input).map_err(rejected)?;
        let id = edit.message_id.clone();
        self.atomic_agent_action(|engine| {
            if !unchanged {
                engine.apply_reviewed_message(input)?;
            }
            record.draft.revision = types::new_id("draft")?;
            record.draft.edit = None;
            record.last_accepted_edit = Some(Receipt {
                input_revision: expected.into(),
                message_id: id.clone(),
                text_revision: engine.draft_source_revision(&scope.space_id, &id),
            });
            engine.persist_message_draft(&storage, &record)?;
            let receipt = record.last_accepted_edit.as_ref().ok_or_else(stale)?;
            Ok(engine.message_draft_acceptance(scope, receipt, record.draft))
        })
    }
}
