//! One custody/financial deployment can attach independently certified devices.
use super::*;
use client_core::SecretVault;
use sqlx::types::Json;

impl RuntimeAuthority {
    /// Open the deployment's capsule custody without attaching any device. This
    /// neither initializes continuity nor admits a model/output operation.
    ///
    /// # Errors
    /// Refuses missing custody material or a conflicting SQL/FDB deployment.
    pub async fn open_native_service(
        config: crate::RuntimeConfig,
        vault: Arc<dyn SecretVault>,
        key_version: u32,
    ) -> Result<Self, RuntimeError> {
        let namespace = config.namespace.clone();
        let capsule_key = Self::native_capsule_vault_key(&namespace, key_version)?;
        let mut runtime = Self::open(config).await?;
        let key = managed::secret(&*vault, capsule_key)?;
        runtime.native = Some(NativeCustody {
            cipher: Custody::new(*key, format!("native-capsule/1/{namespace}")),
            key_version,
            vault_namespace: namespace,
            credentials: RwLock::new(BTreeMap::new()),
        });
        Ok(runtime)
    }

    /// Resolve a certified device from the canonical directory and its existing
    /// private vault. Cache only that verified credential in this same runtime;
    /// do not create another deployment, anchor, capsule or permission.
    ///
    /// # Errors
    /// Refuses foreign/revoked devices, missing/changed keys, unreadable retained
    /// capsules or a conflicting cached principal. Never replaces a native root.
    pub async fn attach_native_device(
        &self,
        vault: Arc<dyn SecretVault>,
        agent: &str,
        device: &str,
    ) -> Result<(), RuntimeError> {
        let native = self
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
        let device_key = Self::native_device_vault_key(&native.vault_namespace, agent, device)?;
        let row:Option<(Json<Identity>,Json<Identity>,String)>=sqlx::query_as(
            "SELECT a.profile,o.profile,d.cert FROM identities a JOIN identities o ON o.id=a.owner
            JOIN devices d ON d.identity=a.id WHERE a.id=$1 AND d.device=$2 AND d.revoked_at IS NULL"
        ).bind(agent).bind(device).fetch_optional(&self.finance.pool).await?;
        let Some((Json(profile), Json(owner), certificate)) = row else {
            return Err(RuntimeError::Denied);
        };
        self.finance
            .native_directory(&profile, &owner, device, &certificate)
            .await?
            .commit()
            .await?;
        let credential = Credential {
            agent: profile,
            owner,
            certificate,
            secret: managed::secret(&*vault, device_key)?,
        };
        credential.unlocked()?;
        let principal = credential.principal();
        if principal.agent != agent || principal.device != device {
            return Err(RuntimeError::Denied);
        };
        let tx = self.execution.transaction().await?;
        let exists = self.execution.native_root(&tx, &principal).await?.is_some();
        drop(tx);
        if exists {
            native.inspect_credential(self, &credential).await?;
        }
        // No lock is held across directory/native IO or by an executing model.
        let mut credentials = native
            .credentials
            .write()
            .map_err(|_| RuntimeError::Unavailable)?;
        let key = (agent.to_owned(), device.to_owned());
        if let Some(retained) = credentials.get(&key) {
            if retained.principal() != principal {
                return Err(RuntimeError::Denied);
            };
        } else {
            if credentials.len() >= MAX_CONFIGURED_DEVICES {
                return Err(RuntimeError::InvalidBinding);
            };
            credentials.insert(key, Arc::new(credential));
        }
        Ok(())
    }
}
