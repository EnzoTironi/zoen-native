//! Reuse the original signed-policy and native directory transactions.
#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::perf,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::{
    Finance, NativeDirectoryGuard, Postgres, RuntimeError, SignedOwnerPolicy, Transaction,
};

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

impl NativeDirectoryGuard {
    pub(crate) async fn shared_native_checkpoint(
        mut self,
        managed: &crate::continuity::ManagedRuntime,
        prepared: &relay_admission::managed_images::Prepared,
        scope: &crate::native::ActivationScope,
        recovery: bool,
    ) -> Result<Self, RuntimeError> {
        if !scope.matches_directory(
            &self.owner,
            &self.agent,
            &self.device,
            &self.certificate_digest,
        ) {
            return Err(RuntimeError::InvalidBinding);
        }
        let observed = managed.sql_checkpoint(&mut self.tx).await?;
        if observed == prepared.previous {
            managed.write_sql(&mut self.tx, prepared).await?;
        } else if !recovery || observed != prepared.next {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        self.shared_native_checkpoint = true;
        Ok(self)
    }
}
