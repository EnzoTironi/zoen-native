//! Existing SQL reservation under shared restore continuity. This phase creates
//! a hold, never a claim, provider capability, bill or release proof.
mod recovery;
use super::*;
use crate::{Binding, VerifiedStep};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
const PREFIX: &str = "reserve_v1_";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    binding: Binding,
    previous: Option<String>,
    next: String,
}
impl Intent {
    fn update(&self) -> Result<SubjectUpdate, RuntimeError> {
        Ok(SubjectUpdate {
            subject: crate::finance::snapshot::subject(
                &self.binding.context.owner,
                self.binding.period_start,
            )?,
            previous: self.previous.clone(),
            next: self.next.clone(),
        })
    }
    fn effect(&self) -> Result<String, RuntimeError> {
        Ok(crate::hash(
            &serde_json::to_vec(&("runtime-reservation/1", self))
                .map_err(|_| RuntimeError::InvalidBinding)?,
        ))
    }
    fn seal(&self, runtime: &RuntimeAuthority) -> Result<String, RuntimeError> {
        let (_, bytes) = runtime.custody.seal("runtime-reservation/1", self)?;
        let encoded = format!("{PREFIX}{}", STANDARD.encode(bytes));
        if encoded.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        Ok(encoded)
    }
}
pub(super) fn reservation_intent(prepared: &Prepared) -> bool {
    prepared
        .intent
        .as_deref()
        .is_some_and(|value| value.starts_with(PREFIX))
}
async fn scope(
    runtime: &RuntimeAuthority,
    step: &VerifiedStep,
    binding: &Binding,
    tx: &foundationdb::Transaction,
) -> Result<(), RuntimeError> {
    let dispatch = runtime
        .gateway
        .preflight(step.request())
        .map_err(|_| RuntimeError::InvalidProfile)?;
    if step.binding(&dispatch, &runtime.price)? != *binding {
        return Err(RuntimeError::InvalidBinding);
    }
    let VerifiedStep::Native(native) = step else {
        return Err(RuntimeError::CoreAuthorityUnavailable);
    };
    native
        .scope_in(&runtime.execution, tx, &runtime.custody)
        .await
}
fn projected(
    before: &BTreeMap<String, String>,
    intent: &Intent,
) -> Result<BTreeMap<String, String>, RuntimeError> {
    let update = intent.update()?;
    if before.get(&update.subject) != update.previous.as_ref() {
        return Err(RuntimeError::RestoreUnreconciled);
    }
    let mut after = before.clone();
    after.insert(update.subject, update.next);
    Ok(after)
}
impl RuntimeAuthority {
    pub(crate) async fn reserve_verified_budget(
        &self,
        step: &VerifiedStep,
        binding: &Binding,
    ) -> Result<(), RuntimeError> {
        let Some(managed) = &self.runtime_continuity else {
            return self.finance.reserve(binding).await;
        };
        let mut managed = managed.lock().await;
        Box::pin(managed.reconcile(self)).await?;
        let mut sql = self.finance.continuity_transaction().await?;
        let previous = managed.sql_checkpoint(&mut sql).await?;
        let tx = self.execution.transaction().await?;
        let before = inventory(self, &mut sql, &tx).await?;
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
        scope(self, step, binding, &tx).await?;
        let inserted = Finance::reserve_in(&mut sql, binding).await?;
        Finance::check_reservation_deadline(&mut sql, binding, step.grant_deadline()).await?;
        if !inserted {
            sql.commit().await?;
            return Ok(());
        }
        let after = inventory(self, &mut sql, &tx).await?;
        let subject =
            crate::finance::snapshot::subject(&binding.context.owner, binding.period_start)?;
        let intent = Intent {
            binding: binding.clone(),
            previous: before.get(&subject).cloned(),
            next: after
                .get(&subject)
                .cloned()
                .ok_or(RuntimeError::InvalidBinding)?,
        };
        if projected(&before, &intent)? != after {
            return Err(RuntimeError::InvalidBinding);
        }
        let prepared = managed
            .anchor
            .prepare_subjects(
                &intent.effect()?,
                &previous,
                &previous,
                false,
                &[intent.update()?],
                Some(&intent.seal(self)?),
            )
            .map_err(error)?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 102 {
            return Err(RuntimeError::Unavailable);
        }
        // The original native/source read conflicts with an advancing source in
        // the subsequent FDB commit. SQL can retain a hold, never authorize HTTP.
        scope(self, step, binding, &tx).await?;
        Finance::check_reservation_deadline(&mut sql, binding, step.grant_deadline()).await?;
        managed.write_sql(&mut sql, &prepared).await?;
        sql.commit().await?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 103 {
            return Err(RuntimeError::Unavailable);
        }
        managed.write_fdb(self, &tx, &prepared).await?;
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 104 {
            return Err(RuntimeError::Unavailable);
        }
        Box::pin(managed.reconcile(self)).await?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 105 {
            return Err(RuntimeError::Unavailable);
        }
        Ok(())
    }
}
