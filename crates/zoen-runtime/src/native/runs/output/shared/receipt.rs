//! Historical receipt activation advances the existing checkpoint, never a send permit.
mod record;
mod recovery;
use super::super::receipt::ReceiptActivation;
use super::*;
use crate::continuity::{inventory, ManagedRuntime};
use record::Intent;
pub(crate) use recovery::recover_shared_receipt;

const PREFIX: &str = "native_receipt_v1_";
pub(crate) fn shared_receipt_intent(prepared: &Prepared) -> bool {
    prepared
        .intent
        .as_deref()
        .is_some_and(|v| v.starts_with(PREFIX))
}

pub(in crate::native::runs::output) async fn activate(
    runtime: &RuntimeAuthority,
    capability: &dyn NativeOutputAuthority,
    activation: ReceiptActivation<'_>,
) -> Result<ReplyReceiptProgress, RuntimeError> {
    let mut managed = runtime
        .runtime_continuity
        .as_ref()
        .ok_or(RuntimeError::RestoreUnreconciled)?
        .lock()
        .await;
    Box::pin(managed.reconcile(runtime)).await?;
    let mut sql = runtime.finance.continuity_transaction().await?;
    let previous = managed.sql_checkpoint(&mut sql).await?;
    let tx = runtime.execution.transaction().await?;
    let before = inventory(runtime, &mut sql, &tx).await?;
    if managed.fdb_checkpoint(runtime, &tx).await? != previous
        || managed
            .anchor
            .reader()
            .checkpoint()
            .map_err(crate::managed_images::error)?
            != previous
        || managed
            .anchor
            .reader()
            .subject_commitments()
            .map_err(crate::managed_images::error)?
            != before
    {
        return Err(RuntimeError::RestoreUnreconciled);
    }
    // The same live proof and transaction checks as unmanaged/Images receipts.
    let progress = runtime
        .execution
        .apply_receipt_in(&tx, capability, &activation, runtime)
        .await?;
    let after = inventory(runtime, &mut sql, &tx).await?;
    let mut intent = Intent::new(&activation);
    for (subject, next) in &after {
        if before.get(subject) != Some(next) {
            intent.updates.push(SubjectUpdate {
                subject: subject.clone(),
                previous: before.get(subject).cloned(),
                next: next.clone(),
            });
        }
    }
    intent.validate_updates(&before, &after)?;
    let sealed = intent.seal(runtime)?;
    let prepared = managed
        .anchor
        .prepare_subjects(
            &intent.effect()?,
            &previous,
            &previous,
            false,
            &intent.updates,
            Some(&sealed),
        )
        .map_err(crate::managed_images::error)?;
    cut(runtime, 159)?;
    // Preparation was observational. The durable pin changes inventory conflict
    // ranges, so activate in a fresh transaction and recheck the same live proof.
    drop(tx);
    // Keep the staged image durable even if the process exits before either store commit.
    if let Some(staged) = &activation.staged {
        let pin = runtime.execution.transaction().await?;
        runtime
            .execution
            .pin_image_activation(
                &pin,
                staged,
                &activation.loaded.fences.device,
                sealed.as_bytes(),
            )
            .await?;
        pin.commit().await.map_err(|_| RuntimeError::Unavailable)?;
    }
    cut(runtime, 160)?;
    let tx = runtime.execution.transaction().await?;
    runtime
        .execution
        .apply_receipt_in(&tx, capability, &activation, runtime)
        .await?;
    if inventory(runtime, &mut sql, &tx).await? != after {
        return Err(RuntimeError::RestoreUnreconciled);
    }
    managed.write_sql(&mut sql, &prepared).await?;
    sql.commit().await?;
    cut(runtime, 161)?;
    managed.write_fdb(runtime, &tx, &prepared).await?;
    tx.clear(
        &runtime
            .execution
            .image_activation_pin(&intent.target.context.principal),
    );
    tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
    cut(runtime, 162)?;
    Box::pin(managed.reconcile(runtime)).await?;
    cut(runtime, 163)?;
    Ok(progress)
}
fn cut(_runtime: &RuntimeAuthority, _value: u8) -> Result<(), RuntimeError> {
    #[cfg(test)]
    if _runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == _value {
        return Err(RuntimeError::Unavailable);
    }
    Ok(())
}

#[cfg(test)]
impl ManagedRuntime {
    pub(crate) fn test_pending_receipt_digest(
        &self,
        runtime: &RuntimeAuthority,
    ) -> Result<Option<String>, RuntimeError> {
        let prepared = self
            .anchor
            .pending()
            .map_err(crate::managed_images::error)?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        Intent::decode(runtime, &prepared)?.receipt_digest()
    }
}
