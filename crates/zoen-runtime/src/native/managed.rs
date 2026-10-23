//! Native custody configuration authenticates a real directory and an existing
//! vault. It grants neither restore continuity nor permission to send output.
use super::*;
use client_core::SecretVault;
use sqlx::types::Json;
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
fn secret(vault: &dyn SecretVault, key: String) -> Result<Zeroizing<[u8; 32]>, RuntimeError> {
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
        let namespace = config.namespace.clone();
        let capsule_key = Self::native_capsule_vault_key(&namespace, key_version)?;
        let device_key = Self::native_device_vault_key(&namespace, agent, device)?;
        let mut runtime = Self::open(config).await?;
        let row: Option<(Json<Identity>, Json<Identity>, String)> = sqlx::query_as(
            "SELECT a.profile,o.profile,d.cert FROM identities a JOIN identities o ON o.id=a.owner JOIN devices d ON d.identity=a.id WHERE a.id=$1 AND d.device=$2 AND d.revoked_at IS NULL"
        ).bind(agent).bind(device).fetch_optional(&runtime.finance.pool).await?;
        let Some((Json(profile), Json(owner), certificate)) = row else {
            return Err(RuntimeError::Denied);
        };
        // This read validates the actual registered profile, parent and device;
        // activation later needs a separate fresh directory authorization.
        runtime
            .finance
            .native_directory(&profile, &owner, device, &certificate)
            .await?
            .commit()
            .await?;
        let key = secret(&*vault, capsule_key)?;
        let credential = Credential {
            agent: profile,
            owner,
            certificate,
            secret: secret(&*vault, device_key)?,
        };
        credential.unlocked()?;
        let principal = credential.principal();
        if principal.agent != agent || principal.device != device {
            return Err(RuntimeError::Denied);
        }
        let custody = NativeCustody {
            cipher: Custody::new(*key, format!("native-capsule/1/{namespace}")),
            key_version,
            credentials: BTreeMap::from([((agent.into(), device.into()), credential)]),
        };
        // An existing capsule must authenticate before returning a configured
        // runtime. A changed vault cannot silently bootstrap over retained state.
        let trx = runtime.execution.transaction().await?;
        let exists = runtime
            .execution
            .native_root(&trx, &principal)
            .await?
            .is_some();
        drop(trx);
        if exists {
            custody.inspect(&runtime, agent, device).await?;
        }
        runtime.native = Some(custody);
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
        let result = self
            .initialize_fenced(runtime, credential, &principal, &fence)
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
        let guard = runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                &principal.device,
                &credential.certificate,
            )
            .await?;
        let admission = guard
            .authorize(staged.scope(&runtime.execution, fence))
            .await?;
        let root = runtime
            .execution
            .activate_native(staged, fence, admission)
            .await?;
        Ok(NativeInspection {
            generation: root.context.generation,
            image_bytes: root.context.image_bytes,
        })
    }
}
