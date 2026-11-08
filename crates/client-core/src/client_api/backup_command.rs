//! Scoped backup controls share the original vault and public backup implementation.
use super::{account_vault, encode, Arc, Client, CoreError, SecretVault};
use serde::Deserialize;
#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub(crate) struct BackupScope {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub expected_relay_url: String,
}
impl BackupScope {
    pub(crate) fn check(&self, client: &Client) -> Result<(), CoreError> {
        let engine = client.lock();
        engine.native_surface_owner(&self.expected_identity_id, &self.expected_device_id)?;
        let account = engine
            .account()
            .ok_or_else(|| super::invalid("backup account unavailable"))?;
        if !account.registered || account.relay_url != self.expected_relay_url {
            return Err(super::invalid("backup account changed"));
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum BackupCommand {
    #[serde(rename = "backup_status")]
    Status {
        #[cfg_attr(feature = "client-types", ts(optional))]
        scope: Option<BackupScope>,
    },
    #[serde(rename = "backup_create_recovery_key")]
    CreateRecoveryKey {
        #[cfg_attr(feature = "client-types", ts(optional))]
        scope: Option<BackupScope>,
    },
    #[serde(rename = "backup_enable_recovery_key")]
    EnableRecoveryKey {
        recovery_key: String,
        #[cfg_attr(feature = "client-types", ts(optional))]
        scope: Option<BackupScope>,
    },
    #[serde(rename = "backup_password")]
    Password {
        password: String,
        #[cfg_attr(feature = "client-types", ts(optional))]
        scope: Option<BackupScope>,
    },
    #[serde(rename = "backup_now")]
    Now {
        #[cfg_attr(feature = "client-types", ts(optional))]
        scope: Option<BackupScope>,
    },
    #[serde(rename = "backup_turn_off")]
    TurnOff {
        #[cfg_attr(feature = "client-types", ts(optional))]
        scope: Option<BackupScope>,
    },
}
impl std::fmt::Debug for BackupCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackupCommand").finish_non_exhaustive()
    }
}
impl BackupCommand {
    fn scope(&self) -> Option<&BackupScope> {
        match self {
            Self::Status { scope }
            | Self::CreateRecoveryKey { scope }
            | Self::EnableRecoveryKey { scope, .. }
            | Self::Password { scope, .. }
            | Self::Now { scope }
            | Self::TurnOff { scope } => scope.as_ref(),
        }
    }
    pub(super) async fn execute(
        self,
        client: &Client,
        vault: Option<Arc<dyn SecretVault>>,
    ) -> Result<String, CoreError> {
        let scope = self.scope().cloned();
        if let Some(scope) = &scope {
            scope.check(client)?;
        }
        let result = match self {
            Self::Status { .. } => encode(&client.backup_status()),
            Self::CreateRecoveryKey { .. } => {
                let _ = account_vault(&vault)?;
                encode(&client.backup_create_recovery_key()?)
            }
            Self::EnableRecoveryKey { recovery_key, .. } => encode(
                &client
                    .backup_enable_recovery_key(recovery_key, account_vault(&vault)?)
                    .await?,
            ),
            Self::Password { password, .. } => encode(
                &client
                    .backup_turn_on_password(password, account_vault(&vault)?)
                    .await?,
            ),
            Self::Now { .. } => encode(&client.backup_now(account_vault(&vault)?).await?),
            Self::TurnOff { .. } => encode(&client.backup_turn_off(account_vault(&vault)?).await?),
        }?;
        if let Some(scope) = &scope {
            scope.check(client)?;
        }
        Ok(result)
    }
}
