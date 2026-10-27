//! Normal composer reads, validated selection review and durable draft commands.
use super::{encode, invalid, Client, Command, CoreError};
impl Client {
    pub(super) fn dispatch_draft(&self, command: Command) -> Result<String, CoreError> {
        use Command::{MessageDraft, ReviewReplySelection, SaveMessageDraft, SendMessageDraft};
        match command {
            ReviewReplySelection {
                expected_identity_id,
                expected_device_id,
                space_id,
                thread_root_id,
                selection,
            } => {
                let scope = crate::message_drafts::DraftScope {
                    expected_identity_id,
                    expected_device_id,
                    space_id,
                    thread_root_id,
                };
                encode(&self.lock().review_reply_selection(&scope, selection)?)
            }
            MessageDraft {
                expected_identity_id,
                expected_device_id,
                space_id,
                thread_root_id,
            } => {
                let scope = crate::message_drafts::DraftScope {
                    expected_identity_id,
                    expected_device_id,
                    space_id,
                    thread_root_id,
                };
                encode(&self.lock().message_draft(&scope)?)
            }
            SaveMessageDraft {
                expected_identity_id,
                expected_device_id,
                space_id,
                thread_root_id,
                expected_revision,
                text,
                reply_to,
                reply_quote,
            } => {
                let scope = crate::message_drafts::DraftScope {
                    expected_identity_id,
                    expected_device_id,
                    space_id,
                    thread_root_id,
                };
                encode(&self.lock().save_message_draft_selected(
                    &scope,
                    &expected_revision,
                    text,
                    reply_to,
                    reply_quote,
                )?)
            }
            SendMessageDraft {
                expected_identity_id,
                expected_device_id,
                space_id,
                thread_root_id,
                expected_revision,
            } => {
                let scope = crate::message_drafts::DraftScope {
                    expected_identity_id,
                    expected_device_id,
                    space_id,
                    thread_root_id,
                };
                encode(&self.lock().send_message_draft(&scope, &expected_revision)?)
            }
            _ => Err(invalid("Invalid draft command.")),
        }
    }
}
