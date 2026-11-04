//! One-shot FDB admission; planning data alone is not a live permission.
use super::*;
impl Execution {
    pub async fn admit_once(
        &self,
        step: &VerifiedStep,
        claim: FreshClaim,
        binding: &Binding,
        custody: &Custody,
        runtime: &crate::RuntimeAuthority,
    ) -> Result<FreshAdmission, RuntimeError> {
        runtime.step_continuity(step)?;
        let trx = self.transaction().await?;
        let staged = self
            .stage_admission_in(&trx, step, claim, binding, custody)
            .await?;
        if let Some(checkpoint) = &staged.checkpoint {
            checkpoint.write_fdb(self, &trx).await?;
        }
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 22 {
            runtime.native_cut_entered.notify_one();
            runtime.native_cut_resume.notified().await;
        }
        runtime.step_continuity(step)?;
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        runtime.step_continuity(step)?;
        Ok(FreshAdmission {
            attempt: staged.attempt,
            nonce: staged.nonce,
            checkpoint: staged.checkpoint,
        })
    }
    pub(crate) async fn stage_admission_in(
        &self,
        trx: &Transaction,
        step: &VerifiedStep,
        claim: FreshClaim,
        binding: &Binding,
        custody: &Custody,
    ) -> Result<StagedAdmission, RuntimeError> {
        if binding.context != step.request().context {
            return Err(RuntimeError::InvalidBinding);
        }
        let (attempt, nonce, digest, checkpoint) = claim.consume();
        if attempt != binding.context.attempt_id || digest != binding.digest()? {
            return Err(RuntimeError::InvalidBinding);
        }
        self.scope(trx, step, custody, false).await?;
        let prepared = trx
            .get(&self.attempt(&attempt, "prepared"), false)
            .await?
            .ok_or(RuntimeError::Denied)?;
        let prior: Binding = custody.open(&format!("prepared/{attempt}"), None, &prepared)?;
        if prior != *binding {
            return Err(RuntimeError::InvalidBinding);
        }
        if trx
            .get(&self.attempt(&attempt, "closed"), false)
            .await?
            .is_some()
            || trx
                .get(&self.attempt(&attempt, "admitted"), false)
                .await?
                .is_some()
        {
            return Err(RuntimeError::Denied);
        }
        // The unique SQL nonce is evidence of the consumed claim, never a read API.
        trx.set(&self.attempt(&attempt, "admitted"), nonce.as_bytes());
        Ok(StagedAdmission {
            attempt,
            nonce,
            checkpoint,
        })
    }
}
