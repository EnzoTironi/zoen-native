//! The existing request result carries a closed, optional refusal classification.
use super::{encode, Client, CoreError};
use crate::app_current_share::{AppShareContext, AppShareResult};
impl Client {
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
