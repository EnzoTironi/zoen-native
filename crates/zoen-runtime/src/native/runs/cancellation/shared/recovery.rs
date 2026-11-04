//! FDB-first recovery of the exact original stop request. No spend or refund.
use super::*;
pub(crate) async fn recover_shared_cancel(
    runtime: &RuntimeAuthority,
    managed: &mut ManagedRuntime,
    prepared: &Prepared,
) -> Result<(), RuntimeError> {
    let encoded = prepared
        .intent
        .as_deref()
        .and_then(|value| value.strip_prefix(PREFIX))
        .ok_or(RuntimeError::RestoreUnreconciled)?;
    if encoded.len() > 16_384 {
        return Err(RuntimeError::EvidenceTooLarge);
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| RuntimeError::InvalidBinding)?;
    let intent: Intent = runtime
        .custody
        .open("runtime-native-stop/1", None, &bytes)?;
    if prepared.retirement
        || prepared.effect_digest != intent.pointer.effect()?
        || prepared.subject_updates != [intent.pointer.update()?]
    {
        return Err(RuntimeError::RestoreUnreconciled);
    }
    let mut sql = runtime.finance.continuity_transaction().await?;
    let sql_cp = managed.sql_checkpoint(&mut sql).await?;
    let tx = runtime.execution.transaction().await?;
    let fdb_cp = managed.fdb_checkpoint(runtime, &tx).await?;
    let actual = inventory(runtime, &mut sql, &tx).await?;
    let expected = managed
        .anchor
        .reader()
        .pending_subject_commitments(prepared)
        .map_err(crate::managed_images::error)?;
    intent.validate_original(runtime, &tx).await?;
    if sql_cp == prepared.next && fdb_cp == prepared.next {
        if actual != expected {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        verify_history(runtime, &tx, prepared).await?;
    } else {
        let mut prior = expected.clone();
        prior.remove(&intent.pointer.subject());
        if sql_cp != prepared.previous
            || (fdb_cp != prepared.previous && fdb_cp != prepared.next)
            || actual
                != if fdb_cp == prepared.previous {
                    prior
                } else {
                    expected
                }
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        if fdb_cp == prepared.previous {
            intent.apply(runtime, &tx).await?;
            managed.write_fdb(runtime, &tx, prepared).await?;
            tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        } else {
            verify_history(runtime, &tx, prepared).await?;
        }
        managed.write_sql(&mut sql, prepared).await?;
    }
    sql.commit().await?;
    if managed
        .anchor
        .reconcile(&prepared.next, &prepared.next)
        .map_err(crate::managed_images::error)?
        != Recovery::BothAfter
    {
        return Err(RuntimeError::RestoreUnreconciled);
    }
    Ok(())
}
async fn verify_history(
    runtime: &RuntimeAuthority,
    tx: &Transaction,
    prepared: &Prepared,
) -> Result<(), RuntimeError> {
    let bytes = tx
        .get(
            &crate::continuity::history(&runtime.execution, prepared.next.sequence),
            false,
        )
        .await?
        .ok_or(RuntimeError::RestoreUnreconciled)?;
    if serde_json::from_slice::<Prepared>(&bytes).map_err(|_| RuntimeError::InvalidBinding)?
        != *prepared
    {
        return Err(RuntimeError::RestoreUnreconciled);
    }
    Ok(())
}
