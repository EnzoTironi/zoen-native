//! Public reply output boundaries share one privately qualified authority.
use crate::*;

impl RuntimeAuthority {
    /// Closed until actual custody, continuity and old-writer barrier evidence
    /// can authorize this boundary. A locator never creates that evidence.
    pub async fn prepare_reply_output(
        &self,
        run: &str,
    ) -> Result<ReplyOutputInspection, RuntimeError> {
        if !id(run) {
            return Err(RuntimeError::InvalidBinding);
        }
        let scope = self.production_output_scope()?;
        self.native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .prepare_output(self, run, scope)
            .await
    }

    pub async fn reconcile_reply_receipt(
        &self,
        run: &str,
    ) -> Result<ReplyReceiptProgress, RuntimeError> {
        if !id(run) {
            return Err(RuntimeError::InvalidBinding);
        }
        let scope = self.production_output_scope()?;
        self.native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .reconcile_output(self, run, scope)
            .await
    }

    /// Send only an already activated original. Retained metadata cannot open
    /// the same closed production authority boundary as preparation/receipt.
    pub async fn deliver_reply_output(
        &self,
        run: &str,
        target: ImageDeliveryTarget,
    ) -> Result<ReplyReceiptProgress, RuntimeError> {
        if !id(run) {
            return Err(RuntimeError::InvalidBinding);
        }
        let scope = self.production_output_scope()?;
        self.native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .deliver_output(self, run, &target, scope)
            .await
    }
}
