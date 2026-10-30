//! Normal composer reads, validated selection review and durable draft commands.
use super::{encode, invalid, Client, Command, CoreError};
impl Client {
    pub(super) fn dispatch_draft(&self, command: Command) -> Result<String, CoreError> {
        match command {
            Command::ReviewSharedReply { scope, review } => {
                let quote = self.lock().review_shared_reply(
                    &scope,
                    &review.source_space_id,
                    &review.message_id,
                    review.selection.as_ref(),
                )?;
                if quote.revision != review.expected_source_revision {
                    return Err(CoreError::Stale {
                        reason: crate::i18n::t(
                            "a mensagem original mudou",
                            "the original message changed",
                        ),
                    });
                }
                encode(&quote)
            }
            Command::ReviewReplySelection { scope, selection } => {
                encode(&self.lock().review_reply_selection(&scope, selection)?)
            }
            Command::MessageDraft { scope } => encode(&self.lock().message_draft(&scope)?),
            Command::SaveMessageDraft {
                scope,
                expected_revision,
                content,
            } => encode(&self.lock().save_message_draft_content(
                &scope,
                &expected_revision,
                content,
            )?),
            Command::SendMessageDraft {
                scope,
                expected_revision,
            } => encode(&self.lock().send_message_draft(&scope, &expected_revision)?),
            _ => Err(invalid("Invalid draft command.")),
        }
    }
}
