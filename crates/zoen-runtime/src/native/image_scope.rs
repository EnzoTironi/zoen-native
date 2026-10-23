//! Images-only authority derived from a live anchor, certified device and real
//! mandatory relay binding. Neither callers nor fixtures construct this type.
use super::*;
use crate::{execution::Execution, managed_images::ImageAnchor};
use async_trait::async_trait;
use foundationdb::Transaction;
use protocol::runtime_binding::ManagedCheckpoint;

#[async_trait]
pub(crate) trait ImageOutputAuthority: Send + Sync {
    async fn check_in(
        &self,
        execution: &Execution,
        transaction: &Transaction,
    ) -> Result<(), RuntimeError>;
    fn check_context(&self, runtime: &RuntimeAuthority) -> Result<(), RuntimeError>;
    fn check_device(&self, agent: &str, device: &str) -> Result<(), RuntimeError>;
    fn check_target(&self, target: &ImageDeliveryTarget) -> Result<(), RuntimeError>;
}

pub(crate) struct ManagedImageOutput {
    anchor: ImageAnchor,
    target: ImageDeliveryTarget,
}

impl ManagedImageOutput {
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
impl ImageOutputAuthority for ManagedImageOutput {
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

#[cfg(test)]
#[async_trait]
impl ImageOutputAuthority for FreshOutputNamespace {
    async fn check_in(
        &self,
        execution: &Execution,
        transaction: &Transaction,
    ) -> Result<(), RuntimeError> {
        FreshOutputNamespace::check_in(self, execution, transaction).await
    }
    fn check_context(&self, runtime: &RuntimeAuthority) -> Result<(), RuntimeError> {
        runtime.continuity()
    }
    fn check_device(&self, _: &str, _: &str) -> Result<(), RuntimeError> {
        Ok(())
    }
    fn check_target(&self, _: &ImageDeliveryTarget) -> Result<(), RuntimeError> {
        Ok(())
    }
}

impl RuntimeAuthority {
    /// Qualify only Images on this exact operator route. No provider call or
    /// publication occurs: the live socket proves legacy binding is refused
    /// and the managed, privately paired checkpoint binding is actually served.
    pub async fn qualify_managed_images_output(
        mut self,
        target: ImageDeliveryTarget,
    ) -> Result<Self, RuntimeError> {
        if self.image_output.is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
        let anchor = self.image_output_anchor().await?;
        let deployment = &anchor.deployment;
        if target.source != deployment.source
            || self
                .configured_image_deployment(&deployment.agent, &deployment.device)
                .await?
                != *deployment
        {
            return Err(RuntimeError::DeploymentMismatch);
        }
        let credential = self
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .credential(&deployment.agent, &deployment.device)?;
        let actor = event_log::Author::certified(
            deployment.agent.clone(),
            event_log::Signer::from_secret(&credential.secret),
            credential.certificate.clone(),
        );
        match Box::pin(self.connect_bound_output(&target.url, &target.relay, actor.clone())).await {
            Err(RuntimeError::Denied) => {}
            Err(error) => return Err(error),
            Ok(_) => return Err(RuntimeError::CoreAuthorityUnavailable),
        }
        let checkpoint = &anchor.checkpoint;
        let expected = self
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
        let transport = Box::pin(Self::connect_output_expectation(
            &target.url,
            &target.relay,
            actor,
            expected,
        ))
        .await?;
        drop(transport);
        self.image_output = Some(ManagedImageOutput { anchor, target });
        Ok(self)
    }

    pub(crate) fn production_image_output_scope(
        &self,
    ) -> Result<&dyn ImageOutputAuthority, RuntimeError> {
        self.image_output
            .as_ref()
            .map(|scope| scope as &dyn ImageOutputAuthority)
            .ok_or(RuntimeError::CoreAuthorityUnavailable)
    }
    pub(crate) fn image_output_scope(&self) -> Result<&dyn ImageOutputAuthority, RuntimeError> {
        if let Some(scope) = &self.image_output {
            return Ok(scope);
        }
        #[cfg(test)]
        if let Some(scope) = &self.output_fixture {
            return Ok(scope);
        }
        Err(RuntimeError::CoreAuthorityUnavailable)
    }
    pub(crate) fn image_continuity(&self) -> Result<(), RuntimeError> {
        self.image_output_scope()?.check_context(self)
    }
    pub(crate) fn step_continuity(&self, step: &crate::VerifiedStep) -> Result<(), RuntimeError> {
        match step {
            crate::VerifiedStep::NativeImage(_) => self.image_continuity(),
            #[cfg(test)]
            crate::VerifiedStep::Fixture(step)
                if step.request.operation == zoen_models::Operation::ImageGeneration =>
            {
                self.image_continuity()
            }
            _ => self.continuity(),
        }
    }
}
