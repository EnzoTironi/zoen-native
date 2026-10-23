//! Negative one-shot evidence, not a new financial admission. No current grant
//! or read of a nonce becomes a live provider capability here.
use super::*;
use crate::managed_images::DispatchCheckpoint;

impl Finance {
    pub(crate) async fn reconcile_image_claim_checkpoint(
        &self,
        checkpoint: &DispatchCheckpoint,
    ) -> Result<(), RuntimeError> {
        let reference = &checkpoint.reference;
        let mut transaction = self.begin().await?;
        let row: Option<(String, String, Json<Binding>)> = sqlx::query_as(
            "SELECT c.nonce,a.binding_digest,a.binding FROM runtime_claims c JOIN runtime_attempts a ON a.attempt=c.attempt WHERE c.attempt=$1 FOR SHARE OF c,a",
        ).bind(&reference.attempt).fetch_optional(&mut *transaction).await?;
        let Some((nonce, digest, Json(binding))) = row else {
            return Err(RuntimeError::RestoreUnreconciled);
        };
        if nonce != reference.nonce
            || digest != reference.binding_digest
            || binding.digest()? != digest
            || binding.context.attempt_id != reference.attempt
            || binding.context.agent != checkpoint.deployment.agent
            || binding.context.device != checkpoint.deployment.device
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        binding.price.image()?;
        let witness: Option<String> =
            sqlx::query_scalar("SELECT nonce FROM runtime_admission_witnesses WHERE attempt=$1")
                .bind(&reference.attempt)
                .fetch_optional(&mut *transaction)
                .await?;
        if witness
            .as_ref()
            .is_some_and(|nonce| nonce != &reference.nonce)
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        // Only the detector's CAS moves. The original claim, hold, witness
        // absence and every financial posting remain exactly as observed.
        checkpoint.write_sql(&mut transaction).await?;
        transaction.commit().await?;
        Ok(())
    }
}
