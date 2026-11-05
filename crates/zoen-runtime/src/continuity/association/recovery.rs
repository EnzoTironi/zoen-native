//! Recover correspondence only from its authenticated original and checkpoint.
use super::Intent;
use crate::continuity::{error, ManagedRuntime, Prepared, Recovery};
use crate::{RuntimeAuthority, RuntimeError};
impl ManagedRuntime {
    pub(in crate::continuity) async fn recover_capture_association(
        &mut self,
        runtime: &RuntimeAuthority,
        prepared: &Prepared,
    ) -> Result<(), RuntimeError> {
        let intent = Intent::decode(runtime, prepared)?;
        let mut sql = runtime.finance.continuity_transaction().await?;
        let sql_cp = self.sql_checkpoint(&mut sql).await?;
        let tx = runtime.execution.retention_transaction().await?;
        let fdb_cp = self.fdb_checkpoint(runtime, &tx).await?;
        let actual = super::super::inventory(runtime, &mut sql, &tx).await?;
        let expected = self
            .anchor
            .reader()
            .pending_subject_commitments(prepared)
            .map_err(error)?;
        if sql_cp == prepared.next && fdb_cp == prepared.next {
            let bytes = tx
                .get(
                    &super::super::history(&runtime.execution, prepared.next.sequence),
                    false,
                )
                .await?
                .ok_or(RuntimeError::RestoreUnreconciled)?;
            if serde_json::from_slice::<Prepared>(&bytes)
                .map_err(|_| RuntimeError::InvalidBinding)?
                != *prepared
                || actual != expected
            {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            // Also authenticate the exact intent against the actual original.
            intent.stage(runtime, &mut sql, &tx).await?;
            sql.commit().await?;
        } else if fdb_cp == prepared.previous
            && (sql_cp == prepared.previous || sql_cp == prepared.next)
        {
            let mut before = expected.clone();
            before.insert(
                intent.update.subject.clone(),
                intent
                    .update
                    .previous
                    .clone()
                    .ok_or(RuntimeError::RestoreUnreconciled)?,
            );
            if actual != before {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            intent.stage(runtime, &mut sql, &tx).await?;
            if super::super::inventory(runtime, &mut sql, &tx).await? != expected {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            if sql_cp == prepared.previous {
                self.write_sql(&mut sql, prepared).await?;
            }
            sql.commit().await?;
            self.write_fdb(runtime, &tx, prepared).await?;
            tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        } else {
            return Err(RuntimeError::RestoreUnreconciled);
        }
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
