//! Images restore checkpoint orchestration. External state never grants a send
//! permit: existing one-shot claims and native output authority remain required.
mod recovery;
mod setup;
mod stores;
use crate::{finance::FreshClaim, Binding, RuntimeAuthority, RuntimeError};
use relay_admission::managed_images::{
    AnchorError, AnchorWriter, Checkpoint, Deployment, GenerationGuard, Prepared, Recovery,
};
use tokio::sync::OwnedMutexGuard;

pub(crate) struct ManagedImages {
    anchor: AnchorWriter,
    deployment: Deployment,
}

/// An annotation of an existing linear claim, not a financial capability.
#[derive(Clone)]
pub(crate) struct DispatchCheckpoint {
    pub(super) deployment: Deployment,
    pub(super) prepared: Prepared,
    pub(super) reference: DispatchReference,
}

/// Existing admission locators only. No amount, grant, output or send permit.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DispatchReference {
    pub(crate) attempt: String,
    pub(crate) nonce: String,
    pub(crate) binding_digest: String,
}

impl DispatchReference {
    pub(crate) fn digest(&self) -> Result<String, RuntimeError> {
        if !crate::id(&self.attempt)
            || [&self.nonce, &self.binding_digest]
                .into_iter()
                .any(|value| {
                    value.len() != 64
                        || !value
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                })
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(crate::hash(
            &serde_json::to_vec(&(
                "managed-image-dispatch/1",
                &self.attempt,
                &self.nonce,
                &self.binding_digest,
            ))
            .map_err(|_| RuntimeError::InvalidBinding)?,
        ))
    }
}

pub(crate) struct Transition {
    guard: OwnedMutexGuard<ManagedImages>,
    checkpoint: DispatchCheckpoint,
}

fn error(error: AnchorError) -> RuntimeError {
    match error {
        AnchorError::Invalid => RuntimeError::InvalidBinding,
        AnchorError::Unavailable | AnchorError::Busy => RuntimeError::Unavailable,
        AnchorError::Unreconciled => RuntimeError::RestoreUnreconciled,
    }
}

impl RuntimeAuthority {
    pub(crate) async fn check_image_checkpoint(&self) -> Result<(), RuntimeError> {
        let Some(managed) = &self.managed_images else {
            return Ok(());
        };
        let mut guard = managed.lock().await;
        let deployment = guard.deployment.clone();
        self.reconcile_image_anchor(&mut guard.anchor, &deployment)
            .await
    }

    pub(crate) async fn begin_image_dispatch_checkpoint(
        &self,
        binding: &Binding,
        claim: &FreshClaim,
    ) -> Result<Option<Transition>, RuntimeError> {
        let Some(managed) = &self.managed_images else {
            return Ok(None);
        };
        let mut guard = managed.clone().lock_owned().await;
        if binding.context.agent != guard.deployment.agent
            || binding.context.device != guard.deployment.device
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let (sql, fdb) = self.image_checkpoints(&guard.deployment).await?;
        let prepared = guard
            .anchor
            .prepare(&claim.effect_digest()?, &sql, &fdb, false)
            .map_err(error)?;
        let checkpoint = DispatchCheckpoint {
            deployment: guard.deployment.clone(),
            prepared,
            reference: claim.checkpoint_reference(),
        };
        Ok(Some(Transition { guard, checkpoint }))
    }
}

impl Transition {
    pub(crate) fn annotation(&self) -> DispatchCheckpoint {
        self.checkpoint.clone()
    }

    pub(crate) async fn finish(
        mut self,
        runtime: &RuntimeAuthority,
    ) -> Result<GenerationGuard, RuntimeError> {
        let (sql, fdb) = runtime
            .image_checkpoints(&self.checkpoint.deployment)
            .await?;
        if sql != self.checkpoint.prepared.next
            || fdb != self.checkpoint.prepared.next
            || self.guard.anchor.reconcile(&sql, &fdb).map_err(error)? != Recovery::BothAfter
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        self.guard
            .anchor
            .reader()
            .generation_guard(&sql)
            .map_err(error)
    }
}
