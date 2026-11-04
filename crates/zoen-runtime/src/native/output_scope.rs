//! Shared checks for native output. A retained locator is never publication authority.
use super::*;
use crate::execution::Execution;
use crate::managed_images::ImageAnchor;
use async_trait::async_trait;
use foundationdb::Transaction;
use protocol::runtime_binding::ManagedCheckpoint;

#[async_trait]
pub(crate) trait NativeOutputAuthority: Send + Sync {
    async fn check_in(
        &self,
        execution: &Execution,
        transaction: &Transaction,
    ) -> Result<(), RuntimeError>;
    fn check_context(&self, runtime: &RuntimeAuthority) -> Result<(), RuntimeError>;
    fn check_device(&self, agent: &str, device: &str) -> Result<(), RuntimeError>;
    fn check_target(&self, target: &ImageDeliveryTarget) -> Result<(), RuntimeError>;
    #[cfg(test)]
    fn observe(&self, _: &str, _: &protocol::Envelope) -> Result<(), RuntimeError> {
        Ok(())
    }
    #[cfg(test)]
    fn expiry_remaining(&self) -> Option<std::sync::Arc<std::sync::atomic::AtomicU64>> {
        None
    }
}

pub(crate) struct ManagedNativeOutput {
    anchor: ImageAnchor,
    target: ImageDeliveryTarget,
}

impl ManagedNativeOutput {
    pub(super) async fn qualify(
        runtime: &RuntimeAuthority,
        target: ImageDeliveryTarget,
    ) -> Result<Self, RuntimeError> {
        let anchor = runtime.image_output_anchor().await?;
        let deployment = &anchor.deployment;
        if target.source != deployment.source
            || runtime
                .configured_image_deployment(&deployment.agent, &deployment.device)
                .await?
                != *deployment
        {
            return Err(RuntimeError::DeploymentMismatch);
        }
        let credential = runtime
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .credential(&deployment.agent, &deployment.device)?;
        let actor = event_log::Author::certified(
            deployment.agent.clone(),
            event_log::Signer::from_secret(&credential.secret),
            credential.certificate.clone(),
        );
        match Box::pin(runtime.connect_bound_output(&target.url, &target.relay, actor.clone()))
            .await
        {
            Err(RuntimeError::Denied) => {}
            Err(error) => return Err(error),
            Ok(_) => return Err(RuntimeError::CoreAuthorityUnavailable),
        }
        let checkpoint = &anchor.checkpoint;
        let expected = runtime
            .bound_output_expectation(
                &target.relay,
                &deployment.agent,
                &deployment.device,
                Some(ManagedCheckpoint {
                    generation: checkpoint.generation,
                    sequence: checkpoint.sequence,
                    digest: checkpoint.digest.clone(),
                }),
            )
            .await?;
        let transport = Box::pin(RuntimeAuthority::connect_output_expectation(
            &target.url,
            &target.relay,
            actor,
            expected,
        ))
        .await?;
        drop(transport);
        Ok(Self { anchor, target })
    }

    fn execution(&self, execution: &Execution) -> Result<(), RuntimeError> {
        let deployment = &self.anchor.deployment;
        if execution.root.bytes()
            != foundationdb::tuple::pack(&("zoen-runtime-v1", deployment.namespace.as_str()))
            || execution.deployment != deployment.witness
            || execution.reply_source()?.cell() != deployment.source
        {
            return Err(RuntimeError::DeploymentMismatch);
        }
        Ok(())
    }
}

#[async_trait]
impl NativeOutputAuthority for ManagedNativeOutput {
    async fn check_in(
        &self,
        execution: &Execution,
        transaction: &Transaction,
    ) -> Result<(), RuntimeError> {
        self.execution(execution)?;
        let source = transaction
            .get(&execution.root.pack(&("relay-source/1",)), false)
            .await?;
        let bytes = transaction
            .get(
                &execution.root.pack(&("managed-images/checkpoint/1",)),
                false,
            )
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let checkpoint: relay_admission::managed_images::Checkpoint =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
        if checkpoint.generation != self.anchor.checkpoint.generation
            || source.as_deref() != Some(self.anchor.deployment.source.as_bytes())
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        self.anchor
            .reader
            .check_store_checkpoint(&checkpoint)
            .map_err(crate::managed_images::error)
    }

    fn check_context(&self, runtime: &RuntimeAuthority) -> Result<(), RuntimeError> {
        // The private scope retains the original OS generation lock. Actual
        // mutation additionally checks current FDB/FS in its owner transaction.
        if runtime.managed_images.is_none() {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        self.execution(&runtime.execution)
    }
    fn check_device(&self, agent: &str, device: &str) -> Result<(), RuntimeError> {
        if self.anchor.deployment.agent != agent || self.anchor.deployment.device != device {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
    fn check_target(&self, target: &ImageDeliveryTarget) -> Result<(), RuntimeError> {
        if target != &self.target {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
}
