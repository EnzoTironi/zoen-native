//! Typed owner controls, separate from model turns and execution admission.
use super::*;
use crate::{OwnerPolicyContextFailure, OwnerPolicyContextStage as Stage};
use client_core::image_tasks::ImageTaskDefinition;
use types::{
    owner_budget::SignedOwnerPolicy,
    owner_policy_context::{OwnerPolicyOptions, SignedOwnerPolicyInquiry},
};

impl NativeCustody {
    pub(crate) async fn install_owner_policy(
        &self,
        runtime: &RuntimeAuthority,
        agent: &str,
        device: &str,
        signed: SignedOwnerPolicy,
    ) -> Result<crate::OwnerPolicyInstallation, RuntimeError> {
        let credential = self.credential(agent, device)?;
        if signed.policy.owner != credential.owner.id {
            return Err(RuntimeError::Denied);
        }
        let guard = runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                device,
                &credential.certificate,
            )
            .await?;
        if runtime.runtime_continuity.is_some() {
            return runtime.install_continuous_owner_policy(signed, guard).await;
        }
        guard.install_owner_policy(signed).await
    }

    pub(crate) async fn owner_policy_options(
        &self,
        runtime: &RuntimeAuthority,
        agent: &str,
        device: &str,
        price: &crate::pricing::BoundPrice,
        definition: Option<&ImageTaskDefinition>,
        signed: &SignedOwnerPolicyInquiry,
    ) -> Result<OwnerPolicyOptions, OwnerPolicyContextFailure> {
        let credential = self
            .credential(agent, device)
            .map_err(|error| Stage::Credential.failed(error))?;
        if (matches!(price, crate::pricing::BoundPrice::Chat(_))
            && signed.inquiry.purpose.is_none())
            || signed.inquiry.owner != credential.owner.id
            || signed.inquiry.agent != agent
            || (signed.inquiry.purpose.is_none()
                && signed.inquiry.price_digest
                    != price
                        .digest()
                        .map_err(|error| Stage::Inquiry.failed(error))?)
            || !event_log::owner_policy_inquiry::verify(signed)
        {
            return Err(Stage::Inquiry.failed(RuntimeError::Denied));
        }
        let progress = self
            .refresh_source(runtime, agent, device, &signed.inquiry.space)
            .await
            .map_err(|error| Stage::Source.failed(error))?;
        if !progress.caught_up {
            return Err(Stage::Source.failed(RuntimeError::Unavailable));
        }
        let principal = credential.principal();
        let fence = runtime
            .execution
            .acquire_native_device(&principal)
            .await
            .map_err(|error| Stage::Capsule.failed(error))?;
        let result = Box::pin(async {
            let capsule = runtime
                .execution
                .read_native(&principal, Some(&fence))
                .await
                .map_err(|error| Stage::Capsule.failed(error))?;
            let core = Box::new(
                DeviceCore::restore(
                    self.open_image(&principal, capsule)
                        .map_err(|error| Stage::Capsule.failed(error))?,
                    credential
                        .unlocked()
                        .map_err(|error| Stage::Credential.failed(error))?,
                )
                .map_err(|_| Stage::Capsule.failed(RuntimeError::Denied))?,
            );
            let now = runtime
                .finance
                .reply_clock()
                .await
                .map_err(|error| Stage::Clock.failed(error))?;
            if signed.inquiry.purpose.is_some() {
                core.owner_policy_account_admission(signed, now)
                    .map_err(|reason| Stage::from(reason).failed(RuntimeError::Denied))?;
            } else {
                let input = core
                    .owner_policy_image_context(signed, now)
                    .map_err(|_| Stage::Admission.failed(RuntimeError::Denied))?;
                if definition != Some(&input.definition) {
                    return Err(Stage::Admission.failed(RuntimeError::InvalidBinding));
                }
            }
            runtime
                .finance
                .native_directory(
                    &credential.agent,
                    &credential.owner,
                    device,
                    &credential.certificate,
                )
                .await
                .map_err(|error| Stage::Directory.failed(error))?
                .owner_policy_options(signed, price)
                .await
                .map_err(|error| Stage::Finance.failed(error))
        })
        .await;
        let released = runtime
            .execution
            .release_native_device(&principal, &fence)
            .await;
        match result {
            Err(error) => Err(error),
            Ok(options) => {
                released.map_err(|error| Stage::Release.failed(error))?;
                Ok(options)
            }
        }
    }
}
