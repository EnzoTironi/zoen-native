//! Reviewed forwards use the same Client/Engine owner, not an alternate writer.
use super::forwarding_command::ForwardingCommand;
use super::{encode, Client, CoreError};
impl Client {
    pub(super) fn dispatch_forwarding(
        &self,
        command: ForwardingCommand,
    ) -> Result<String, CoreError> {
        match command {
            ForwardingCommand::SetForwardOptions {
                scope,
                operation_id,
                source_revision,
                options,
            } => encode(&self.lock().set_forward_options(
                &scope,
                &operation_id,
                &source_revision,
                options,
            )?),
            ForwardingCommand::SetForwardBatchOptions {
                source,
                batch_id,
                review_revision,
                options,
            } => encode(&self.lock().set_forward_batch_options(
                &source,
                &batch_id,
                &review_revision,
                options,
            )?),
            ForwardingCommand::ForwardBatchStatus { source } => {
                encode(&self.forward_batch_status(source)?)
            }
            ForwardingCommand::ForwardBatchReview { scope } => {
                encode(&self.forward_batch_review(scope)?)
            }
            ForwardingCommand::ConfirmForwardBatch {
                source,
                batch_id,
                review_revision,
            } => encode(&self.confirm_forward_batch(source, batch_id, review_revision)?),
            ForwardingCommand::AbandonForwardBatch {
                source,
                batch_id,
                review_revision,
            } => encode(&self.abandon_forward_batch(source, batch_id, review_revision)?),
            ForwardingCommand::FinishForwardBatch { source, batch_id } => {
                encode(&self.finish_forward_batch(source, batch_id)?)
            }
            ForwardingCommand::ForwardReview { scope } => encode(&self.forward_review(scope)?),
            ForwardingCommand::ForwardStatus { source } => encode(&self.forward_status(source)?),
            ForwardingCommand::ConfirmForward {
                scope,
                operation_id,
                source_revision,
            } => encode(&self.confirm_forward(scope, operation_id, source_revision)?),
            ForwardingCommand::DismissForward {
                scope,
                operation_id,
                accepted_message_id,
            } => {
                self.dismiss_forward(scope, operation_id, accepted_message_id)?;
                encode(&())
            }
        }
    }
}
