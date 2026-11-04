//! Observe actual FDB admission and the original SQL claim. Never create a
//! missing admission/witness or a new consumable live receipt from a read.
use super::*;
impl ManagedRuntime {
    pub(in crate::continuity) async fn recover_dispatch(
        &mut self,
        runtime: &RuntimeAuthority,
        prepared: &Prepared,
    ) -> Result<(), RuntimeError> {
        let encoded = prepared
            .intent
            .as_deref()
            .and_then(|v| v.strip_prefix(PREFIX))
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if encoded.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| RuntimeError::InvalidBinding)?;
        let intent: Intent = runtime.custody.open("runtime-dispatch/1", None, &bytes)?;
        let pointer = intent.pointer()?;
        if prepared.retirement
            || prepared.effect_digest != intent.effect()?
            || prepared.subject_updates != [pointer.update()?]
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let mut sql = runtime.finance.continuity_transaction().await?;
        let sql_cp = self.sql_checkpoint(&mut sql).await?;
        let tx = runtime.execution.transaction().await?;
        let fdb_cp = self.fdb_checkpoint(runtime, &tx).await?;
        if fdb_cp != prepared.next || (sql_cp != prepared.previous && sql_cp != prepared.next) {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let expected = self
            .anchor
            .reader()
            .pending_subject_commitments(prepared)
            .map_err(error)?;
        if super::super::inventory(runtime, &mut sql, &tx).await? != expected {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        pointer.observe(runtime, &tx).await?;
        Finance::observe_claim(&mut sql, &intent.binding, &intent.nonce).await?;
        let history = tx
            .get(&history(&runtime.execution, prepared.next.sequence), false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if serde_json::from_slice::<Prepared>(&history).map_err(|_| RuntimeError::InvalidBinding)?
            != *prepared
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        if sql_cp == prepared.previous {
            self.write_sql(&mut sql, prepared).await?;
        }
        sql.commit().await?;
        if self
            .anchor
            .reconcile(&prepared.next, &prepared.next)
            .map_err(error)?
            != Recovery::BothAfter
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(())
    }
}
