use super::*;
use crate::managed_images::PublishCheckpoint;

impl NativeDirectoryGuard {
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
