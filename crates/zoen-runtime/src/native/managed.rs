//! Native custody configuration authenticates a real directory and an existing
//! vault. It grants neither restore continuity nor permission to send output.
use super::*;
use client_core::SecretVault;
use std::sync::Arc;

fn vault_key(
    namespace: &str,
    purpose: &str,
    agent: &str,
    device: &str,
    version: u32,
) -> Result<String, RuntimeError> {
    if !id(namespace) || version == 0 || (!agent.is_empty() && (!id(agent) || !id(device))) {
        return Err(RuntimeError::InvalidBinding);
    }
    Ok(format!(
        "runtime_{}",
        hash(
            &serde_json::to_vec(&("native-vault/1", namespace, purpose, agent, device, version,))
                .map_err(|_| RuntimeError::InvalidBinding)?
        )
    ))
}
pub(super) fn secret(
    vault: &dyn SecretVault,
    key: String,
) -> Result<Zeroizing<[u8; 32]>, RuntimeError> {
    let bytes = Zeroizing::new(
        vault
            .load(key)
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?,
    );
    Ok(Zeroizing::new(
        bytes
            .as_slice()
            .try_into()
            .map_err(|_| RuntimeError::InvalidBinding)?,
    ))
}

impl RuntimeAuthority {
    /// Current certified native principal and SQL/FDB deployment locators for
    /// trusted operator registration. This observation grants no effect permit.
    pub async fn native_deployment(
        &self,
        agent: &str,
        device: &str,
    ) -> Result<relay_admission::managed_images::Deployment, RuntimeError> {
        self.configured_image_deployment(agent, device).await
    }

    pub(crate) async fn configured_image_deployment(
        &self,
        agent: &str,
        device: &str,
    ) -> Result<relay_admission::managed_images::Deployment, RuntimeError> {
        let credential = self
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .credential(agent, device)?;
        self.finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                device,
                &credential.certificate,
            )
            .await?
            .commit()
            .await?;
        let (namespace, witness): (String, String) = sqlx::query_as(
            "SELECT namespace,witness FROM runtime_deployment_binding WHERE singleton",
        )
        .fetch_one(&self.finance.pool)
        .await?;
        if witness != self.execution.deployment
            || self.execution.root.bytes()
                != foundationdb::tuple::pack(&("zoen-runtime-v1", namespace.as_str()))
        {
            return Err(RuntimeError::DeploymentMismatch);
        }
        self.execution.transaction().await?;
        Ok(relay_admission::managed_images::Deployment {
            namespace,
            witness,
            source: self.execution.reply_source()?.cell().into(),
            agent: agent.into(),
            device: device.into(),
        })
    }

    /// Opaque vault locators, not credentials or enrollment permissions. The
    /// service never generates a replacement for a missing/invalid retained key.
    pub fn native_capsule_vault_key(namespace: &str, version: u32) -> Result<String, RuntimeError> {
        vault_key(namespace, "capsule", "", "", version)
    }
    pub fn native_device_vault_key(
        namespace: &str,
        agent: &str,
        device: &str,
    ) -> Result<String, RuntimeError> {
        if !id(agent) || !id(device) {
            return Err(RuntimeError::InvalidBinding);
        }
        vault_key(namespace, "device", agent, device, 1)
    }

    /// Opens one explicitly scoped certified device using an operator-provided
    /// implementation of the same vault contract as the native clients/CLI.
    /// All public profiles/certificates come from SQL; neither a credential map
    /// nor a caller-supplied image can enter this boundary. No vault writes.
    pub async fn open_with_native_device(
        config: crate::RuntimeConfig,
        vault: Arc<dyn SecretVault>,
        agent: &str,
        device: &str,
        key_version: u32,
    ) -> Result<Self, RuntimeError> {
        Self::native_device_vault_key(&config.namespace, agent, device)?;
        let runtime = Self::open_native_service(config, vault.clone(), key_version).await?;
        runtime.attach_native_device(vault, agent, device).await?;
        Ok(runtime)
    }

    /// Creates only an empty native device capsule. No conversation/grant/image
    /// import, MLS key-package publication, paid dispatch or output permission.
    /// Retrying after an uncertain result inspects the retained capsule instead
    /// of resetting its generation, state or future MLS material.
    pub async fn initialize_native_device(
        &self,
        agent: &str,
        device: &str,
    ) -> Result<NativeInspection, RuntimeError> {
        self.native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .initialize(self, agent, device)
            .await
    }
}

impl NativeCustody {
    async fn initialize(
        &self,
        runtime: &RuntimeAuthority,
        agent: &str,
        device: &str,
    ) -> Result<NativeInspection, RuntimeError> {
        runtime.check_shared_runtime().await?;
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
        runtime.collect_reply_retired(&principal).await?;
        let fence = runtime.execution.acquire_native_device(&principal).await?;
        let result = self
            .initialize_fenced(runtime, &credential, &principal, &fence)
            .await;
        let released = runtime
            .execution
            .release_native_device(&principal, &fence)
            .await;
        match result {
            Err(error) => Err(error),
            Ok(inspection) => {
                released?;
                Ok(inspection)
            }
        }
    }
    async fn initialize_fenced(
        &self,
        runtime: &RuntimeAuthority,
        credential: &Credential,
        principal: &Principal,
        fence: &crate::execution::Fence,
    ) -> Result<NativeInspection, RuntimeError> {
        let trx = runtime.execution.transaction().await?;
        let existing = runtime
            .execution
            .native_root(&trx, principal)
            .await?
            .is_some();
        drop(trx);
        if existing {
            return self
                .inspect(runtime, &principal.agent, &principal.device)
                .await;
        }
        // An internal source tag, never a caller-selected network endpoint.
        // Native DeviceCore here is driven by RuntimeSource, not Client::sync.
        let origin = format!("runtime+fdb://{}", hash(runtime.execution.root.bytes()));
        let core = DeviceCore::bootstrap(credential.unlocked()?, &origin)
            .map_err(|_| RuntimeError::Denied)?;
        let image = core.image().map_err(|_| RuntimeError::InvalidBinding)?;
        let candidate = self.seal_image(principal, None, &image)?;
        let staged = runtime
            .execution
            .stage_native(principal, None, fence, candidate)
            .await?;
        let root = self
            .activate_maintenance(runtime, credential, staged, fence, None)
            .await?;
        Ok(NativeInspection {
            generation: root.context.generation,
            image_bytes: root.context.image_bytes,
        })
    }
}
