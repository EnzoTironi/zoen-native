//! Trusted composition for native/MCP hosts. User input carries no tariff,
//! provider key, certificate, native image, financial binding or output permit.
#[cfg(test)]
mod tests;
use crate::*;
use client_core::{
    image_tasks::{ImageTaskDefinition, ImageTaskInput},
    SecretVault,
};
use std::time::Duration;
use zoen_models::ModelLimits;

/// Immutable operator configuration. A host must not deserialize this from a
/// model tool call or expose its vault locator as a user setting.
pub struct ImageHostConfig {
    pub agent: String,
    pub device: String,
    pub definition_version: String,
    pub price: ImagePriceProfile,
    pub credential_key: String,
    pub timeout: Duration,
    pub connect_timeout: Duration,
    pub target: ImageDeliveryTarget,
}

/// A scoped installation observation, not active budget or a dispatch permit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnerPolicyInstallation {
    Installed(String),
    Refused(String),
}

pub struct ImageRuntimeHost {
    runtime: Arc<RuntimeAuthority>,
    vault: Arc<dyn SecretVault>,
    config: ImageHostConfig,
    definition: ImageTaskDefinition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageHostProgress {
    CatchingUp {
        through: types::Seen,
    },
    /// An original attempt exists but has no currently deliverable retained
    /// output. This cannot be interpreted as zero cost or permission to retry.
    AwaitingRecovery {
        run: String,
        financial: FinancialState,
    },
    Output {
        run: String,
        output: ImageOutputInspection,
        delivery: ImageDeliveryProgress,
    },
}

impl ImageRuntimeHost {
    pub fn configured(
        runtime: Arc<RuntimeAuthority>,
        vault: Arc<dyn SecretVault>,
        config: ImageHostConfig,
    ) -> Result<Self, RuntimeError> {
        let definition = definition(&config)?;
        Ok(Self {
            runtime,
            vault,
            config,
            definition,
        })
    }

    /// Only the configured agent's actual owner can install a signed policy.
    /// Native directory and policy installation share the same SQL transaction.
    pub async fn import_owner_policy(
        &self,
        signed: types::owner_budget::SignedOwnerPolicy,
    ) -> Result<OwnerPolicyInstallation, RuntimeError> {
        self.runtime
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .install_owner_policy(&self.runtime, &self.config, signed)
            .await
    }

    /// Read-only quote/version context for an exact confirmed image review.
    /// This does not approve the request or admit a financial/provider effect.
    pub async fn owner_policy_options(
        &self,
        signed: &types::owner_policy_context::SignedOwnerPolicyInquiry,
    ) -> Result<types::owner_policy_context::OwnerPolicyOptions, RuntimeError> {
        self.runtime
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .owner_policy_options(&self.runtime, &self.config, &self.definition, signed)
            .await
    }

    /// The trusted profile determines every reviewed specification and tariff.
    /// Stage retains and publishes the runtime device's own original review;
    /// the normal owner's ApproveOnce must arrive through the actual source.
    pub async fn stage(
        &self,
        space: String,
        source: String,
        prompt: String,
    ) -> Result<ImageReviewProgress, RuntimeError> {
        self.runtime
            .stage_image_task_review(
                ImageReviewIntent {
                    agent: self.config.agent.clone(),
                    device: self.config.device.clone(),
                    space,
                    source,
                    input: ImageTaskInput {
                        prompt,
                        definition: self.definition.clone(),
                    },
                },
                self.config.target.clone(),
            )
            .await
    }

    /// A repeated host command recovers the original run/output. An existing
    /// financial attempt without retained output reports recovery rather than
    /// issuing another provider call. All effects still use RuntimeAuthority's
    /// current native, directory, budget and closed production admission.
    pub async fn advance(
        &self,
        space: &str,
        request: &str,
    ) -> Result<ImageHostProgress, RuntimeError> {
        self.runtime.image_continuity()?;
        let scope = self.runtime.production_image_output_scope()?;
        scope.check_device(&self.config.agent, &self.config.device)?;
        scope.check_target(&self.config.target)?;
        let progress = self
            .runtime
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .refresh_source(
                &self.runtime,
                &self.config.agent,
                &self.config.device,
                space,
            )
            .await?;
        if !progress.caught_up {
            return Ok(ImageHostProgress::CatchingUp {
                through: progress.through,
            });
        }
        let run = self
            .runtime
            .image_host_run(
                &self.config.agent,
                &self.config.device,
                space,
                request,
                &self.definition,
                &self.config.price,
                self.gateway()?,
            )
            .await?;
        let output = match self
            .runtime
            .prepare_image_task_output(&run, self.gateway()?)
            .await
        {
            Ok(output) => output,
            Err(RuntimeError::Unavailable) => {
                let financial = self
                    .runtime
                    .image_host_financial(&self.config.agent, &self.config.device, &run)
                    .await?;
                if let Some(financial) = financial {
                    return Ok(ImageHostProgress::AwaitingRecovery { run, financial });
                }
                // SQL absence is only an observation. run_image_task still
                // performs the existing one-shot cross-store admission.
                self.runtime.run_image_task(&run, self.gateway()?).await?;
                self.runtime
                    .prepare_image_task_output(&run, self.gateway()?)
                    .await?
            }
            Err(error) => return Err(error),
        };
        let delivery = self
            .runtime
            .deliver_image_task_output(&run, self.config.target.clone())
            .await?;
        Ok(ImageHostProgress::Output {
            run,
            output,
            delivery,
        })
    }

    fn gateway(&self) -> Result<GatewayConfig, RuntimeError> {
        gateway(&self.config, self.vault.as_ref())
    }
}

fn definition(config: &ImageHostConfig) -> Result<ImageTaskDefinition, RuntimeError> {
    if !id(&config.agent)
        || !id(&config.device)
        || config.credential_key.is_empty()
        || config.credential_key.len() > 512
        || config.credential_key.chars().any(char::is_control)
        || config.timeout.is_zero()
        || config.timeout > Duration::from_secs(120)
        || config.connect_timeout.is_zero()
        || config.connect_timeout > config.timeout
    {
        return Err(RuntimeError::InvalidProfile);
    }
    fn text(value: impl Serialize) -> Result<String, RuntimeError> {
        serde_json::to_value(value)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or(RuntimeError::InvalidProfile)
    }
    let definition = ImageTaskDefinition {
        version: config.definition_version.clone(),
        price_digest: config.price.digest()?,
        size: text(config.price.image.size)?,
        quality: text(config.price.image.quality)?,
        background: text(config.price.image.background)?,
        max_image_bytes: config.price.image.max_image_bytes,
    };
    ImageTaskInput {
        prompt: "Validate the trusted definition".into(),
        definition: definition.clone(),
    }
    .binding()
    .map_err(|_| RuntimeError::InvalidProfile)?;
    Ok(definition)
}

fn gateway(
    config: &ImageHostConfig,
    vault: &dyn SecretVault,
) -> Result<GatewayConfig, RuntimeError> {
    definition(config)?;
    let api_key = zeroize::Zeroizing::new(
        vault
            .load(config.credential_key.clone())
            .ok_or(RuntimeError::Unavailable)?,
    );
    let api_key = std::str::from_utf8(&api_key)
        .map_err(|_| RuntimeError::InvalidProfile)?
        .to_owned();
    let base_url = config
        .price
        .endpoint
        .strip_suffix("/images/generations")
        .ok_or(RuntimeError::InvalidProfile)?
        .to_owned();
    Ok(GatewayConfig {
        base_url,
        endpoint_policy: config.price.endpoint_policy.gateway(),
        model: config.price.model.clone(),
        credential_ref: config.price.credential_ref.clone(),
        api_key,
        limits: ModelLimits {
            max_request_bytes: usize::try_from(config.price.max_request_bytes)
                .map_err(|_| RuntimeError::InvalidProfile)?,
            max_response_bytes: usize::try_from(config.price.max_response_bytes)
                .map_err(|_| RuntimeError::InvalidProfile)?,
            // Images does not send a max_output_tokens field. The shared
            // gateway requires a positive generic limit, not an Image cost cap.
            max_output_tokens: 1,
            timeout: config.timeout,
            connect_timeout: config.connect_timeout,
        },
    })
}
