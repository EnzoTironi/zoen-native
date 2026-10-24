use super::*;
use crate::managed_images::PublishCheckpoint;

impl NativeDirectoryGuard {
    /// Recovery only reauthorizes current device maintenance. The original
    /// checkpoint is either still previous or already exactly next.
    pub(crate) async fn image_recovery_checkpoint(
        mut self,
        checkpoint: &PublishCheckpoint,
    ) -> Result<Self, RuntimeError> {
        if checkpoint.deployment().agent != self.agent
            || checkpoint.deployment().device != self.device
        {
            return Err(RuntimeError::InvalidBinding);
        }
        checkpoint.recover_sql(&mut self.tx).await?;
        Ok(self)
    }

    /// The existing directory locks and SQL commit own this detector update;
    /// neither checkpoint facts nor decoding them grant native admission.
    pub(crate) async fn image_checkpoint(
        mut self,
        checkpoint: Option<&PublishCheckpoint>,
    ) -> Result<Self, RuntimeError> {
        if let Some(checkpoint) = checkpoint {
            if checkpoint.deployment.agent != self.agent
                || checkpoint.deployment.device != self.device
            {
                return Err(RuntimeError::InvalidBinding);
            }
            checkpoint.write_sql(&mut self.tx).await?;
        }
        Ok(self)
    }
}
