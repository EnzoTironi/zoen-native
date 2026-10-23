//! Typed owner controls, separate from model turns and execution admission.
use super::*;
use client_core::image_tasks::ImageTaskDefinition;
use types::{
    owner_budget::SignedOwnerPolicy,
    owner_policy_context::{OwnerPolicyOptions, SignedOwnerPolicyInquiry},
};

impl NativeCustody {
    pub(crate) async fn install_owner_policy(
        &self,
        runtime: &RuntimeAuthority,
        config: &crate::ImageHostConfig,
        signed: SignedOwnerPolicy,
    ) -> Result<crate::OwnerPolicyInstallation, RuntimeError> {
        let credential = self.credential(&config.agent, &config.device)?;
        if signed.policy.owner != credential.owner.id {
            return Err(RuntimeError::Denied);
        }
        runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                &config.device,
                &credential.certificate,
            )
            .await?
            .install_owner_policy(signed)
            .await
    }

    pub(crate) async fn owner_policy_options(
        &self,
        runtime: &RuntimeAuthority,
        config: &crate::ImageHostConfig,
        definition: &ImageTaskDefinition,
        signed: &SignedOwnerPolicyInquiry,
    ) -> Result<OwnerPolicyOptions, RuntimeError> {
        let credential = self.credential(&config.agent, &config.device)?;
        if signed.inquiry.owner != credential.owner.id
            || signed.inquiry.agent != config.agent
            || signed.inquiry.price_digest != config.price.digest()?
            || !event_log::owner_policy_inquiry::verify(signed)
        {
            return Err(RuntimeError::Denied);
        }
        let progress = self
            .refresh_source(
                runtime,
                &config.agent,
                &config.device,
                &signed.inquiry.space,
            )
            .await?;
        if !progress.caught_up {
            return Err(RuntimeError::Unavailable);
        }
        let principal = credential.principal();
        let fence = runtime.execution.acquire_native_device(&principal).await?;
        let result = Box::pin(async {
            let capsule = runtime
                .execution
                .read_native(&principal, Some(&fence))
                .await?;
            let core = Box::new(
                DeviceCore::restore(
                    self.open_image(&principal, capsule)?,
                    credential.unlocked()?,
                )
                .map_err(|_| RuntimeError::Denied)?,
            );
            let now = runtime.finance.reply_clock().await?;
            let input = core
                .owner_policy_image_context(signed, now)
                .map_err(|_| RuntimeError::Denied)?;
            if input.definition != *definition {
                return Err(RuntimeError::InvalidBinding);
            }
            runtime
                .finance
                .native_directory(
                    &credential.agent,
                    &credential.owner,
                    &config.device,
                    &credential.certificate,
                )
                .await?
                .owner_policy_options(signed, &config.price)
                .await
        })
        .await;
        let released = runtime
            .execution
            .release_native_device(&principal, &fence)
            .await;
        match result {
            Err(error) => Err(error),
            Ok(options) => {
                released?;
                Ok(options)
            }
        }
    }
}
