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
use sqlx::types::Json;

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

impl Finance {
    pub(crate) async fn check_financial_deadline(
        tx: &mut Transaction<'_, Postgres>,
        binding: &crate::Binding,
        deadline: Option<i64>,
    ) -> Result<(), RuntimeError> {
        if deadline.is_some_and(|value| value <= 0) {
            return Err(RuntimeError::Denied);
        }
        Self::policy(tx, binding).await?;
        let now = Self::clock(tx).await?;
        if deadline.is_some_and(|value| now >= value) {
            return Err(RuntimeError::Denied);
        }
        Ok(())
    }
    pub(crate) async fn observe_reservation(
        tx: &mut Transaction<'_, Postgres>,
        binding: &crate::Binding,
    ) -> Result<(), RuntimeError> {
        let original: Option<(String, Json<crate::Binding>, i64)> = sqlx::query_as(
            "SELECT binding_digest,binding,hold_units FROM runtime_attempts WHERE attempt=$1",
        )
        .bind(&binding.context.attempt_id)
        .fetch_optional(&mut **tx)
        .await?;
        let Some((digest, Json(actual), hold)) = original else {
            return Err(RuntimeError::RestoreUnreconciled);
        };
        if actual != *binding || digest != binding.digest()? || hold != binding.hold_units {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(())
    }
}

impl Finance {
    pub(crate) async fn observe_claim(
        tx: &mut Transaction<'_, Postgres>,
        binding: &crate::Binding,
        nonce: &str,
    ) -> Result<(), RuntimeError> {
        Self::observe_reservation(tx, binding).await?;
        let actual: Option<String> =
            sqlx::query_scalar("SELECT nonce FROM runtime_claims WHERE attempt=$1")
                .bind(&binding.context.attempt_id)
                .fetch_optional(&mut **tx)
                .await?;
        if actual.as_deref() != Some(nonce) {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(())
    }
}
