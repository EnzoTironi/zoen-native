//! The FDB admission observation and SQL send witness are distinct durable
//! transitions. Recovery never fabricates a witness or reconstructs permission.
mod inventory;
mod recovery;
use super::*;
use crate::{
    finance::{DispatchPermit, FreshClaim},
    Binding, VerifiedStep,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
pub(super) use inventory::commitments;
use reservation::scope;
use serde::{Deserialize, Serialize};
const PREFIX: &str = "dispatch_v1_";

// Only the known live FDB+SQL checkpoint path constructs this receipt. No Clone,
// Deserialize or constructor from retained metadata; it is consumed once.
pub(crate) struct SharedAdmission {
    original: crate::managed_images::DispatchReference,
}
impl SharedAdmission {
    pub(crate) fn reference(&self) -> crate::managed_images::DispatchReference {
        self.original.clone()
    }
    pub(crate) fn consume(self) -> crate::managed_images::DispatchReference {
        self.original
    }
}
pub(crate) struct CompletedDispatch {
    pub(crate) permit: DispatchPermit,
    pub(crate) generation: relay_admission::managed_images::GenerationGuard,
    pub(crate) nonce: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pointer {
    run: String,
    attempt: String,
    binding_digest: String,
    nonce_digest: String,
}
impl Pointer {
    fn subject(&self) -> String {
        crate::hash(format!("runtime-dispatch/1/{}", self.attempt).as_bytes())
    }
    fn key(&self, runtime: &RuntimeAuthority) -> Vec<u8> {
        runtime
            .execution
            .root
            .pack(&("shared-native-dispatch/1", self.attempt.as_str()))
    }
    fn bytes(&self) -> Result<Vec<u8>, RuntimeError> {
        serde_json::to_vec(self).map_err(|_| RuntimeError::InvalidBinding)
    }
    fn update(&self) -> Result<SubjectUpdate, RuntimeError> {
        Ok(SubjectUpdate {
            subject: self.subject(),
            previous: None,
            next: crate::hash(&self.bytes()?),
        })
    }
    async fn observe(
        &self,
        runtime: &RuntimeAuthority,
        tx: &foundationdb::Transaction,
    ) -> Result<(), RuntimeError> {
        let nonce = tx
            .get(&runtime.execution.attempt(&self.attempt, "admitted"), false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let sealed = tx
            .get(&runtime.execution.attempt(&self.attempt, "prepared"), false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let binding: Binding =
            runtime
                .custody
                .open(&format!("prepared/{}", self.attempt), None, &sealed)?;
        if binding.context.run_id != self.run
            || binding.context.attempt_id != self.attempt
            || binding.digest()? != self.binding_digest
            || crate::hash(&nonce) != self.nonce_digest
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    binding: Binding,
    nonce: String,
}
impl Intent {
    fn pointer(&self) -> Result<Pointer, RuntimeError> {
        Ok(Pointer {
            run: self.binding.context.run_id.clone(),
            attempt: self.binding.context.attempt_id.clone(),
            binding_digest: self.binding.digest()?,
            nonce_digest: crate::hash(self.nonce.as_bytes()),
        })
    }
    fn effect(&self) -> Result<String, RuntimeError> {
        Ok(crate::hash(
            &serde_json::to_vec(&("runtime-dispatch/1", self))
                .map_err(|_| RuntimeError::InvalidBinding)?,
        ))
    }
    fn seal(&self, runtime: &RuntimeAuthority) -> Result<String, RuntimeError> {
        let (_, bytes) = runtime.custody.seal("runtime-dispatch/1", self)?;
        let encoded = format!("{PREFIX}{}", STANDARD.encode(bytes));
        if encoded.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        Ok(encoded)
    }
}
pub(super) fn dispatch_intent(prepared: &Prepared) -> bool {
    prepared
        .intent
        .as_deref()
        .is_some_and(|v| v.starts_with(PREFIX))
}
impl RuntimeAuthority {
    pub(crate) async fn dispatch_verified_budget(
        &self,
        step: &VerifiedStep,
        claim: FreshClaim,
        binding: &Binding,
    ) -> Result<CompletedDispatch, RuntimeError> {
        let admission = self.begin_shared_dispatch(step, claim, binding).await?;
        self.witness_verified_budget(step, binding, admission).await
    }
    async fn begin_shared_dispatch(
        &self,
        step: &VerifiedStep,
        claim: FreshClaim,
        binding: &Binding,
    ) -> Result<SharedAdmission, RuntimeError> {
        let managed = self
            .runtime_continuity
            .as_ref()
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let mut managed = managed.lock().await;
        Box::pin(managed.reconcile(self)).await?;
        let mut sql = self.finance.continuity_transaction().await?;
        let previous = managed.sql_checkpoint(&mut sql).await?;
        let tx = self.execution.transaction().await?;
        let before = super::inventory(self, &mut sql, &tx).await?;
        if managed.fdb_checkpoint(self, &tx).await? != previous
            || managed.anchor.reader().checkpoint().map_err(error)? != previous
            || managed
                .anchor
                .reader()
                .subject_commitments()
                .map_err(error)?
                != before
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let reference = claim.checkpoint_reference();
        Finance::observe_claim(&mut sql, binding, &reference.nonce).await?;
        Finance::guard_deadline_in(&mut sql, binding, step).await?;
        let staged = self
            .execution
            .stage_admission_in(&tx, step, claim, binding, &self.custody)
            .await?;
        if staged.checkpoint.is_some()
            || staged.attempt != reference.attempt
            || staged.nonce != reference.nonce
            || reference.binding_digest != binding.digest()?
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let intent = Intent {
            binding: binding.clone(),
            nonce: staged.nonce,
        };
        let pointer = intent.pointer()?;
        let key = pointer.key(self);
        if tx.get(&key, false).await?.is_some() {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        tx.set(&key, &pointer.bytes()?);
        let mut expected = before.clone();
        if expected
            .insert(pointer.subject(), crate::hash(&pointer.bytes()?))
            .is_some()
            || super::inventory(self, &mut sql, &tx).await? != expected
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let prepared = managed
            .anchor
            .prepare_subjects(
                &intent.effect()?,
                &previous,
                &previous,
                false,
                &[pointer.update()?],
                Some(&intent.seal(self)?),
            )
            .map_err(error)?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 110 {
            return Err(RuntimeError::Unavailable);
        }
        scope(self, step, binding, &tx).await?;
        Finance::check_financial_deadline(&mut sql, binding, step.grant_deadline()).await?;
        managed.write_fdb(self, &tx, &prepared).await?;
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 111 {
            return Err(RuntimeError::Unavailable);
        }
        Finance::check_financial_deadline(&mut sql, binding, step.grant_deadline()).await?;
        managed.write_sql(&mut sql, &prepared).await?;
        sql.commit().await?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 112 {
            return Err(RuntimeError::Unavailable);
        }
        Box::pin(managed.reconcile(self)).await?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 113 {
            return Err(RuntimeError::Unavailable);
        }
        Ok(SharedAdmission {
            original: reference,
        })
    }
}
