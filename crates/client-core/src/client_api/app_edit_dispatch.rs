//! The existing request result carries a closed, optional refusal classification.
use super::{encode, Client, Command, CoreError};
use crate::app_current_share::{AppShareContext, AppShareResult};
impl Client {
    pub(super) fn request_app_edit_command(&self, command: Command) -> Result<String, CoreError> {
        let Command::RequestAppEdit {
            expected_identity_id,
            expected_device_id,
            space_id,
            item_id,
            expected_version,
            expected_sha256,
            text,
            mutation_id,
            share_current,
            target_agent_id,
            expected_target_revision,
            expected_resource_uri,
        } = command
        else {
            return Err(super::invalid("Invalid App edit command."));
        };
        self.request_app_edit_bridge(crate::app_edit_requests::AppEditRequestInput {
            expected_identity_id,
            expected_device_id,
            space_id,
            item_id,
            expected_version,
            expected_sha256,
            text,
            mutation_id,
            share_current: share_current.unwrap_or(false),
            target_agent_id,
            expected_target_revision,
            expected_resource_uri,
        })
    }
    pub(super) fn share_app_current_bridge(
        &self,
        context: AppShareContext,
        mutation: &str,
    ) -> Result<String, CoreError> {
        let result = match self.share_app_current(context, mutation) {
            Ok(receipt) => AppShareResult {
                receipt: Some(receipt),
                conflict: None,
            },
            Err(CoreError::Stale { reason }) => AppShareResult {
                receipt: None,
                conflict: Some(reason),
            },
            Err(error) => return Err(error),
        };
        encode(&result)
    }

    pub(super) fn request_app_edit_bridge(
        &self,
        input: crate::app_edit_requests::AppEditRequestInput,
    ) -> Result<String, CoreError> {
        let mut engine = self.lock();
        let kind = engine.app_edit_conflict_kind(&input);
        let result = match engine.request_app_edit(input) {
            Ok(entry) => crate::app_edit_requests::AppEditRequestResult {
                entry: Some(entry),
                conflict: None,
                conflict_kind: None,
            },
            Err(CoreError::Stale { reason }) => crate::app_edit_requests::AppEditRequestResult {
                entry: None,
                conflict: Some(reason),
                conflict_kind: Some(kind),
            },
            Err(error) => return Err(error),
        };
        encode(&result)
    }
}
