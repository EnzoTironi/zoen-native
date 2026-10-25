//! The optional human disclosure uses the same canonical share, not a task log.
use super::*;
use crate::app_current_share::AppShareScope;
impl Engine {
    pub(super) fn share_app_for_request(
        &mut self,
        input: &AppEditRequestInput,
        binding: &ArtifactRequest,
        target: &str,
    ) -> R<()> {
        let scope = AppShareScope {
            expected_identity_id: input.expected_identity_id.clone(),
            expected_device_id: input.expected_device_id.clone(),
            space_id: input.space_id.clone(),
            item_id: input.item_id.clone(),
            target_id: target.into(),
        };
        let mutation = crate::app_current_share::request_share_mutation(&binding.request_id);
        self.share_request_current(scope, binding.version, &binding.sha256, &mutation)
    }
}
