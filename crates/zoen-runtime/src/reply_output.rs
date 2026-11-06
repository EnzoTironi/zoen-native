//! Public reply output boundaries share one privately qualified authority.
use crate::*;

impl RuntimeAuthority {
    pub(crate) fn production_output_scope(
        &self,
    ) -> Result<&dyn native::NativeOutputAuthority, RuntimeError> {
        // The typed route attests current pairing/relay binding. Per-run
        // activation and full deployment continuity remain separate boundaries.
        self.reply_output
            .as_ref()
            .map(|scope| scope as &dyn native::NativeOutputAuthority)
            .ok_or(RuntimeError::CoreAuthorityUnavailable)
    }

    pub(crate) fn reply_continuity(&self) -> Result<(), RuntimeError> {
        if self.runtime_continuity.is_some() {
            self.production_output_scope()?.check_context(self)
        } else {
            self.continuity()
        }
    }

    pub(crate) async fn check_reply_continuity(&self) -> Result<(), RuntimeError> {
        self.reply_continuity()?;
        if self.runtime_continuity.is_some() {
            self.check_shared_runtime().await?;
            self.production_output_scope()?
                .check_in(&self.execution, &self.execution.transaction().await?)
                .await?;
        }
        Ok(())
    }

    /// Activate the retained original under current per-run/native authority.
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

    /// Send only an already activated original under current native authority.
    /// Retained metadata alone cannot authorize another transport effect.
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
