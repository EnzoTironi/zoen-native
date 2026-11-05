//! Route qualification borrows the existing shared generation lock. It does not
//! authorize a run, activate a capsule, publish, or release a financial hold.
#![deny(clippy::pedantic)]
use super::{error, RuntimeAuthority, RuntimeError};
use relay_admission::managed_images::{
    AnchorReader, Checkpoint, GenerationGuard, RuntimeDeployment,
};

pub(crate) struct SharedOutputAnchor {
    pub(crate) reader: AnchorReader,
    pub(crate) deployment: RuntimeDeployment,
    pub(crate) checkpoint: Checkpoint,
    pub(crate) _generation: GenerationGuard,
}
impl RuntimeAuthority {
    pub(crate) async fn shared_output_anchor(&self) -> Result<SharedOutputAnchor, RuntimeError> {
        let mut managed = self
            .runtime_continuity
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .lock()
            .await;
        if self.shared_deployment().await? != managed.deployment {
            return Err(RuntimeError::DeploymentMismatch);
        }
        Box::pin(managed.reconcile(self)).await?;
        let reader = managed.anchor.reader();
        let checkpoint = reader.checkpoint().map_err(error)?;
        let mut sql = self.finance.continuity_transaction().await?;
        let tx = self.execution.transaction().await?;
        if managed.sql_checkpoint(&mut sql).await? != checkpoint
            || managed.fdb_checkpoint(self, &tx).await? != checkpoint
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        sql.commit().await?;
        let generation = reader.generation_guard(&checkpoint).map_err(error)?;
        Ok(SharedOutputAnchor {
            reader,
            deployment: managed.deployment.clone(),
            checkpoint,
            _generation: generation,
        })
    }
}
