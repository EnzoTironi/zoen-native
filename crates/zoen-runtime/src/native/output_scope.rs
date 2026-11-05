//! Shared checks for native output. A retained locator is never publication authority.
use super::*;
use crate::execution::Execution;
mod anchor;
use anchor::OutputAnchor;
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
    anchor: OutputAnchor,
    target: ImageDeliveryTarget,
    principals: BTreeMap<(String, String), Principal>,
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
        let principals = BTreeMap::from([(
            (deployment.agent.clone(), deployment.device.clone()),
            credential.principal(),
        )]);
        Self::qualify_principals(runtime, OutputAnchor::Images(anchor), target, principals).await
    }

    pub(super) async fn qualify_shared(
        runtime: &RuntimeAuthority,
        target: ImageDeliveryTarget,
    ) -> Result<Self, RuntimeError> {
        let anchor = runtime.shared_output_anchor().await?;
        if target.source != anchor.deployment.source {
            return Err(RuntimeError::DeploymentMismatch);
        }
        let custody = runtime
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
        // Same configured-principal bound as the existing relay registry.
        if custody.credentials.is_empty() || custody.credentials.len() > 256 {
            return Err(RuntimeError::InvalidBinding);
        }
        let principals = custody
            .credentials
            .iter()
            .map(|(key, credential)| (key.clone(), credential.principal()))
            .collect();
        Self::qualify_principals(runtime, OutputAnchor::Shared(anchor), target, principals).await
    }

    async fn qualify_principals(
        runtime: &RuntimeAuthority,
        anchor: OutputAnchor,
        target: ImageDeliveryTarget,
        principals: BTreeMap<(String, String), Principal>,
    ) -> Result<Self, RuntimeError> {
        let native = runtime
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
        for ((agent, device), principal) in &principals {
            let credential = native.credential(agent, device)?;
            if credential.principal() != *principal {
                return Err(RuntimeError::InvalidBinding);
            }
            runtime
                .finance
                .native_directory(
                    &credential.agent,
                    &credential.owner,
                    device,
                    &credential.certificate,
                )
                .await?
                .commit()
                .await?;
            let actor = event_log::Author::certified(
                agent.clone(),
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
            let checkpoint = anchor.checkpoint();
            let expected = runtime
                .bound_output_expectation(
                    &target.relay,
                    agent,
                    device,
                    Some(ManagedCheckpoint {
                        generation: checkpoint.generation,
                        sequence: checkpoint.sequence,
                        digest: checkpoint.digest.clone(),
                        source_fence: None,
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
        }
        let scope = Self {
            anchor,
            target,
            principals,
        };
        scope.check_context(runtime)?;
        let tx = runtime.execution.transaction().await?;
        scope.check_in(&runtime.execution, &tx).await?;
        Ok(scope)
    }

    fn execution(&self, execution: &Execution) -> Result<(), RuntimeError> {
        let (namespace, witness, source) = self.anchor.binding();
        if execution.root.bytes() != foundationdb::tuple::pack(&("zoen-runtime-v1", namespace))
            || execution.deployment != witness
            || execution.reply_source()?.cell() != source
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
        if checkpoint.generation != self.anchor.checkpoint().generation
            || source.as_deref() != Some(self.anchor.binding().2.as_bytes())
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        self.anchor
            .reader()
            .check_store_checkpoint(&checkpoint)
            .map_err(crate::managed_images::error)
    }

    fn check_context(&self, runtime: &RuntimeAuthority) -> Result<(), RuntimeError> {
        // The private scope retains the original OS generation lock. Actual
        // mutation additionally checks current FDB/FS in its owner transaction.
        self.anchor.check_context(runtime)?;
        let native = runtime
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
        for ((agent, device), principal) in &self.principals {
            if native.credential(agent, device)?.principal() != *principal {
                return Err(RuntimeError::InvalidBinding);
            }
        }
        self.execution(&runtime.execution)
    }
    fn check_device(&self, agent: &str, device: &str) -> Result<(), RuntimeError> {
        if self.principals.contains_key(&(agent.into(), device.into())) {
            Ok(())
        } else {
            Err(RuntimeError::InvalidBinding)
        }
    }
    fn check_target(&self, target: &ImageDeliveryTarget) -> Result<(), RuntimeError> {
        if target != &self.target {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
}

impl RuntimeAuthority {
    /// Qualify the actual managed relay route for every configured native
    /// principal. This alone cannot authorize a run, activation or publication.
    pub async fn qualify_shared_reply_output(
        mut self,
        target: ImageDeliveryTarget,
    ) -> Result<Self, RuntimeError> {
        if self.reply_output.is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
        self.reply_output = Some(ManagedNativeOutput::qualify_shared(&self, target).await?);
        Ok(self)
    }
}
