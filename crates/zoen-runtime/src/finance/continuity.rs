//! Reuse the original signed-policy and native directory transactions.
#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::perf,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::*;

impl Finance {
    pub(crate) async fn continuity_transaction(
        &self,
    ) -> Result<Transaction<'static, Postgres>, RuntimeError> {
        self.begin().await
    }

    pub(crate) async fn policy_scope(
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Option<i16>, RuntimeError> {
        // Startup takes this same row exclusively BEFORE checking unused tables.
        // A legacy transaction cannot retain an absence read across bootstrap.
        sqlx::query("SELECT singleton FROM runtime_deployment_binding WHERE singleton FOR SHARE")
            .fetch_one(&mut **tx)
            .await?;
        Ok(sqlx::query_scalar(
            "SELECT profile FROM runtime_image_checkpoint WHERE singleton FOR SHARE",
        )
        .fetch_optional(&mut **tx)
        .await?)
    }

    pub(crate) async fn require_unmanaged_policy(
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<(), RuntimeError> {
        if Self::policy_scope(tx).await? == Some(3) {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(())
    }

    pub(crate) async fn install_continuous_policy_in(
        tx: &mut Transaction<'_, Postgres>,
        signed: SignedOwnerPolicy,
    ) -> Result<String, RuntimeError> {
        Self::install_policy_in(tx, signed).await
    }
}

impl NativeDirectoryGuard {
    pub(crate) fn policy_transaction(
        self,
        owner: &str,
    ) -> Result<Transaction<'static, Postgres>, RuntimeError> {
        if self.owner != owner {
            return Err(RuntimeError::Denied);
        }
        Ok(self.tx)
    }
}
