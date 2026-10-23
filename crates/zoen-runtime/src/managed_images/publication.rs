//! Bind the original native publication and its capsule to one checkpoint in
//! the existing activation transactions. This never consumes a relay permit.
use super::*;
use protocol::Envelope;

pub(crate) struct PublishTransition {
    guard: OwnedMutexGuard<ManagedImages>,
    checkpoint: PublishCheckpoint,
}

impl RuntimeAuthority {
    /// An original retained checkpoint is a transport binding, never another
    /// permission. Omitting the anchor cannot downgrade a managed publication.
    pub(crate) async fn validate_image_publish_checkpoint(
        &self,
        envelope: &Envelope,
        checkpoint: Option<&Checkpoint>,
    ) -> Result<(), RuntimeError> {
        let Some(managed) = &self.managed_images else {
            return if checkpoint.is_none() {
                Ok(())
            } else {
                Err(RuntimeError::RestoreUnreconciled)
            };
        };
        let checkpoint = checkpoint.ok_or(RuntimeError::RestoreUnreconciled)?;
        self.check_image_checkpoint().await?;
        let guard = managed.lock().await;
        let facts = guard
            .anchor
            .reader()
            .inspect_publish(checkpoint, envelope)
            .map_err(error)?;
        let transaction = self.execution.transaction().await?;
        let key = self
            .execution
            .root
            .pack(&("managed-images/effect/1", checkpoint.sequence));
        let bytes = transaction
            .get(&key, false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if crate::hash(&bytes) != facts.checkpoint_record_digest {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(())
    }

    pub(crate) async fn begin_image_publish_checkpoint(
        &self,
        envelope: &Envelope,
    ) -> Result<Option<PublishTransition>, RuntimeError> {
        let Some(managed) = &self.managed_images else {
            return Ok(None);
        };
        let mut guard = managed.clone().lock_owned().await;
        if envelope.author() != guard.deployment.agent
            || envelope.device() != Some(guard.deployment.device.as_str())
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let deployment = guard.deployment.clone();
        self.reconcile_image_anchor(&mut guard.anchor, &deployment)
            .await?;
        let (sql, fdb) = self.image_checkpoints(&deployment).await?;
        let digest =
            relay_admission::managed_images::publish_digest(&deployment.namespace, envelope)
                .map_err(error)?;
        let prepared = guard
            .anchor
            .prepare(&digest, &sql, &fdb, false)
            .map_err(error)?;
        Ok(Some(PublishTransition {
            guard,
            checkpoint: PublishCheckpoint {
                deployment,
                prepared,
            },
        }))
    }
}

impl PublishTransition {
    pub(crate) fn annotation(&self) -> &PublishCheckpoint {
        &self.checkpoint
    }
    pub(crate) fn checkpoint(&self) -> Checkpoint {
        self.checkpoint.prepared.next.clone()
    }
    pub(crate) async fn finish(mut self, runtime: &RuntimeAuthority) -> Result<(), RuntimeError> {
        let (sql, fdb) = runtime
            .image_checkpoints(&self.checkpoint.deployment)
            .await?;
        if sql != self.checkpoint.prepared.next
            || fdb != self.checkpoint.prepared.next
            || self.guard.anchor.reconcile(&sql, &fdb).map_err(error)? != Recovery::BothAfter
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(())
    }
}
