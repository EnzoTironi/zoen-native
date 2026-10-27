//! Reviewed forwards use the same Client/Engine owner, not an alternate writer.
use super::{encode, invalid, Client, Command, CoreError};
impl Client {
    pub(super) fn dispatch_forwarding(&self, command: Command) -> Result<String, CoreError> {
        match command {
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
