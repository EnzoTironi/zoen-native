//! Only an original acknowledgement can finish its SQL-first cut.
use super::Intent;
use crate::continuity::{error, history, Checkpoint, ManagedRuntime, Prepared, Recovery};
use crate::finance::Finance;
use crate::{RuntimeAuthority, RuntimeError};
use sqlx::{Postgres, Transaction};
use std::collections::BTreeMap;
struct View {
    fdb: Checkpoint,
    actual: BTreeMap<String, String>,
    expected: BTreeMap<String, String>,
}
impl ManagedRuntime {
    pub(in crate::continuity) async fn recover_financial_ack(
        &mut self,
        runtime: &RuntimeAuthority,
        prepared: &Prepared,
    ) -> Result<(), RuntimeError> {
        let intent = Intent::decode(runtime, prepared)?;
        let mut sql = runtime.finance.continuity_transaction().await?;
        let sql_cp = self.sql_checkpoint(&mut sql).await?;
        let tx = runtime.execution.retention_transaction().await?;
        let fdb = self.fdb_checkpoint(runtime, &tx).await?;
        let binding = intent.record.observe(runtime, &mut sql, &tx).await?;
        if intent.financial.subject
            != crate::finance::snapshot::subject(&binding.context.owner, binding.period_start)?
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let view = View {
            fdb,
            actual: super::super::inventory(runtime, &mut sql, &tx).await?,
            expected: self
                .anchor
                .reader()
                .pending_subject_commitments(prepared)
                .map_err(error)?,
        };
        if sql_cp == prepared.next {
            self.ack_after(runtime, prepared, &intent, sql, tx, view)
                .await?;
        } else if sql_cp == prepared.previous && view.fdb == prepared.previous {
            self.ack_before(runtime, prepared, &intent, sql, tx, view)
                .await?;
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
    async fn ack_after(
        &mut self,
        runtime: &RuntimeAuthority,
        prepared: &Prepared,
        intent: &Intent,
        mut sql: Transaction<'static, Postgres>,
        tx: foundationdb::Transaction,
        view: View,
    ) -> Result<(), RuntimeError> {
        if !intent.record.acknowledged(&mut sql).await? {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        if view.fdb == prepared.next {
            let bytes = tx
                .get(&history(&runtime.execution, prepared.next.sequence), false)
                .await?
                .ok_or(RuntimeError::RestoreUnreconciled)?;
            if serde_json::from_slice::<Prepared>(&bytes)
                .map_err(|_| RuntimeError::InvalidBinding)?
                != *prepared
                || view.actual != view.expected
            {
                return Err(RuntimeError::RestoreUnreconciled);
            }
        } else if view.fdb == prepared.previous {
            let mut partial = view.expected.clone();
            partial.insert(
                intent.capture.subject.clone(),
                intent
                    .capture
                    .previous
                    .clone()
                    .ok_or(RuntimeError::RestoreUnreconciled)?,
            );
            if view.actual != partial {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            intent.record.stage(runtime, &tx).await?;
            if super::super::inventory(runtime, &mut sql, &tx).await? != view.expected {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            self.write_fdb(runtime, &tx, prepared).await?;
            tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        } else {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        sql.commit().await?;
        Ok(())
    }
    async fn ack_before(
        &mut self,
        runtime: &RuntimeAuthority,
        prepared: &Prepared,
        intent: &Intent,
        mut sql: Transaction<'static, Postgres>,
        tx: foundationdb::Transaction,
        view: View,
    ) -> Result<(), RuntimeError> {
        let mut before = view.expected.clone();
        before.insert(
            intent.capture.subject.clone(),
            intent
                .capture
                .previous
                .clone()
                .ok_or(RuntimeError::RestoreUnreconciled)?,
        );
        before.insert(
            intent.financial.subject.clone(),
            intent
                .financial
                .previous
                .clone()
                .ok_or(RuntimeError::RestoreUnreconciled)?,
        );
        if view.actual != before || intent.record.acknowledged(&mut sql).await? {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Finance::ack_outbox_in(
            &mut sql,
            &intent.record.attempt,
            &intent.record.kind,
            &intent.record.digest,
        )
        .await?;
        intent.record.stage(runtime, &tx).await?;
        if super::super::inventory(runtime, &mut sql, &tx).await? != view.expected {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        self.write_sql(&mut sql, prepared).await?;
        sql.commit().await?;
        self.write_fdb(runtime, &tx, prepared).await?;
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(())
    }
}
