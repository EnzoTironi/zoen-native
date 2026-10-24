//! Startup-only checkpoint initialization. Ordinary finance retains its
//! existing transaction boundaries and never takes these table locks.
use super::*;

impl Finance {
    pub(crate) async fn empty_image_checkpoint_transaction(
        &self,
    ) -> Result<Transaction<'static, Postgres>, RuntimeError> {
        let mut transaction = self.begin().await?;
        sqlx::query("LOCK TABLE runtime_attempts IN SHARE MODE")
            .execute(&mut *transaction)
            .await?;
        sqlx::query("LOCK TABLE runtime_image_checkpoint IN EXCLUSIVE MODE")
            .execute(&mut *transaction)
            .await?;
        let retained: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runtime_attempts)")
            .fetch_one(&mut *transaction)
            .await?;
        if retained {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(transaction)
    }
}
