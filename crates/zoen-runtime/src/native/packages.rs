//! Initial MLS publication: retain private/public bytes first, reuse the relay's
//! SQL hash receipts, then retain a known acknowledgement. No new grant/store.
use super::*;
use crate::NativeKeyPackagePublication;
use client_core::runtime_core::{InitialKeyPackages, NativeImage};

enum Phase {
    Prepare,
    Publish,
}

impl RuntimeAuthority {
    /// Retains the initial batch before publication. Returns no package bytes,
    /// credential, group access or model permission.
    pub async fn prepare_native_key_packages(
        &self,
        agent: &str,
        device: &str,
    ) -> Result<NativeKeyPackagePublication, RuntimeError> {
        self.native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .packages(self, agent, device, Phase::Prepare)
            .await
    }
    /// Publishes exactly the retained initial batch through the existing SQL
    /// directory and replay receipts. No regenerated packages or stock refill.
    pub async fn publish_native_key_packages(
        &self,
        agent: &str,
        device: &str,
    ) -> Result<NativeKeyPackagePublication, RuntimeError> {
        self.native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .packages(self, agent, device, Phase::Publish)
            .await
    }
}

fn report(root: &Root, batch: &InitialKeyPackages) -> NativeKeyPackagePublication {
    NativeKeyPackagePublication {
        native: NativeInspection {
            generation: root.context.generation,
            image_bytes: root.context.image_bytes,
        },
        packages: batch.packages().len() as u32,
        published: batch.acknowledged(),
    }
}
impl NativeCustody {
    async fn packages(
        &self,
        runtime: &RuntimeAuthority,
        agent: &str,
        device: &str,
        phase: Phase,
    ) -> Result<NativeKeyPackagePublication, RuntimeError> {
        let credential = self.credential(agent, device)?;
        let principal = credential.principal();
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
        runtime.execution.collect_native_stage(&principal).await?;
        runtime.execution.collect_native_retired(&principal).await?;
        let fence = runtime.execution.acquire_native_device(&principal).await?;
        // Restored DeviceCore and its nested activation futures belong on the
        // heap: callers can compose concurrent publication without embedding
        // the entire native workspace in every outer async frame.
        let result =
            Box::pin(self.packages_fenced(runtime, credential, &principal, &fence, phase)).await;
        let released = runtime
            .execution
            .release_native_device(&principal, &fence)
            .await;
        match result {
            Err(error) => Err(error),
            Ok(report) => {
                released?;
                Ok(report)
            }
        }
    }
    async fn packages_fenced(
        &self,
        runtime: &RuntimeAuthority,
        credential: &Credential,
        principal: &Principal,
        fence: &crate::execution::Fence,
        phase: Phase,
    ) -> Result<NativeKeyPackagePublication, RuntimeError> {
        let capsule = runtime
            .execution
            .read_native(principal, Some(fence))
            .await?;
        let mut root = capsule.root.clone();
        let mut core =
            DeviceCore::restore(self.open_image(principal, capsule)?, credential.unlocked()?)
                .map_err(|_| RuntimeError::Denied)?;
        let batch = match core
            .initial_key_package_publication()
            .map_err(|_| RuntimeError::InvalidBinding)?
        {
            Some(batch) => batch,
            None => {
                let (batch, image) = core
                    .prepare_initial_key_packages()
                    .map_err(|_| RuntimeError::InvalidBinding)?;
                root = self
                    .activate_package_image(runtime, credential, principal, fence, root, image)
                    .await?;
                batch
            }
        };
        if batch.acknowledged() || matches!(phase, Phase::Prepare) {
            return Ok(report(&root, &batch));
        }
        // Retained root/private material must still be current immediately
        // before SQL publication. SQL separately fences the current directory.
        let current = runtime
            .execution
            .read_native(principal, Some(fence))
            .await?;
        if current.root != root {
            return Err(RuntimeError::Denied);
        }
        drop(current);
        runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                &principal.device,
                &credential.certificate,
            )
            .await?
            .publish_key_packages(batch.packages())
            .await?;
        // A known SQL commit with its acknowledgement discarded. This test cut
        // does not simulate DB wire-level commit-unknown or grant authority.
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 64 {
            return Err(RuntimeError::Unavailable);
        }
        let image = core
            .acknowledge_initial_key_packages(batch.digest())
            .map_err(|_| RuntimeError::InvalidBinding)?;
        root = self
            .activate_package_image(runtime, credential, principal, fence, root, image)
            .await?;
        let acknowledged = core
            .initial_key_package_publication()
            .map_err(|_| RuntimeError::InvalidBinding)?
            .ok_or(RuntimeError::InvalidBinding)?;
        Ok(report(&root, &acknowledged))
    }
    async fn activate_package_image(
        &self,
        runtime: &RuntimeAuthority,
        credential: &Credential,
        principal: &Principal,
        fence: &crate::execution::Fence,
        previous: Root,
        image: NativeImage,
    ) -> Result<Root, RuntimeError> {
        // Known pre-activation cut: generated material has not been published
        // and must not become visible in either relay stock or the active image.
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 65 {
            return Err(RuntimeError::Unavailable);
        }
        let next = self.seal_image(principal, Some(&previous), &image)?;
        let staged = runtime
            .execution
            .stage_native(principal, Some(previous), fence, next)
            .await?;
        let admission = runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                &principal.device,
                &credential.certificate,
            )
            .await?
            .authorize(staged.scope(&runtime.execution, fence))
            .await?;
        runtime
            .execution
            .activate_native(staged, fence, admission)
            .await
    }
}
