//! Transfer an existing live anchor lock into Images-only output authority.
use super::*;
use relay_admission::managed_images::AnchorReader;

pub(crate) struct ImageAnchor {
    pub(crate) reader: AnchorReader,
    pub(crate) deployment: Deployment,
    pub(crate) checkpoint: Checkpoint,
    pub(crate) _generation: GenerationGuard,
}

impl RuntimeAuthority {
    pub(crate) async fn image_output_anchor(&self) -> Result<ImageAnchor, RuntimeError> {
        let managed = self
            .managed_images
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
        let mut guard = managed.lock().await;
        let deployment = guard.deployment.clone();
        self.reconcile_image_anchor(&mut guard.anchor, &deployment)
            .await?;
        let reader = guard.anchor.reader();
        let checkpoint = reader.checkpoint().map_err(error)?;
        let (sql, fdb) = self.image_checkpoints(&deployment).await?;
        if sql != checkpoint || fdb != checkpoint {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let generation = reader.generation_guard(&checkpoint).map_err(error)?;
        Ok(ImageAnchor {
            reader,
            deployment,
            checkpoint,
            _generation: generation,
        })
    }
}
