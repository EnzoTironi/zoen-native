//! Reviewed forwards use the same Client/Engine owner, not an alternate writer.
use super::{encode, invalid, Client, Command, CoreError};
impl Client {
    pub(super) fn dispatch_forwarding(&self, command: Command) -> Result<String, CoreError> {
        match command {
            Command::ForwardBatchStatus { source } => encode(&self.forward_batch_status(source)?),
            Command::ForwardBatchReview { scope } => encode(&self.forward_batch_review(scope)?),
            Command::ConfirmForwardBatch {
                source,
                batch_id,
                review_revision,
            } => encode(&self.confirm_forward_batch(source, batch_id, review_revision)?),
            Command::AbandonForwardBatch {
                source,
                batch_id,
                review_revision,
            } => encode(&self.abandon_forward_batch(source, batch_id, review_revision)?),
            Command::FinishForwardBatch { source, batch_id } => {
                encode(&self.finish_forward_batch(source, batch_id)?)
            }
            Command::ForwardReview { scope } => encode(&self.forward_review(scope)?),
            Command::ForwardStatus { source } => encode(&self.forward_status(source)?),
            Command::ConfirmForward {
                scope,
                operation_id,
                source_revision,
            } => encode(&self.confirm_forward(scope, operation_id, source_revision)?),
            Command::DismissForward {
                scope,
                operation_id,
                accepted_message_id,
            } => {
                self.dismiss_forward(scope, operation_id, accepted_message_id)?;
                encode(&())
            }
            _ => Err(invalid("Not a forwarding command.")),
        }
    }
}
