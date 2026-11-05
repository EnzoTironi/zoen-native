//! Incurred evidence can be retained after revocation. Recovery accounts for
//! the original result; it cannot authorize another call, output or refund.
use super::*;
async fn observe(
    sql: &mut Transaction<'_, Postgres>,
    original: &Original<'_>,
    pointer: &Pointer,
) -> Result<(), RuntimeError> {
    Finance::observe_witness(sql, &original.capture.binding, &original.nonce).await?;
    let bytes: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT sealed FROM runtime_evidence WHERE attempt=$1 AND digest=$2")
            .bind(&pointer.attempt)
            .bind(&pointer.digest)
            .fetch_optional(&mut **sql)
            .await?;
    if bytes.as_deref() != Some(original.sealed.as_slice()) {
        return Err(RuntimeError::RestoreUnreconciled);
    }
    let disposition: Option<(String, i64, String)> = sqlx::query_as(
        "SELECT kind,amount_units,source_digest FROM runtime_dispositions WHERE attempt=$1",
    )
    .bind(&pointer.attempt)
    .fetch_optional(&mut **sql)
    .await?;
    let charge = original
        .capture
        .binding
        .price
        .chat()?
        .charge(&original.capture.result)?;
    match (charge, disposition) {
        (None, None) => Ok(()),
        (Some(charge), Some((kind, units, digest)))
            if kind == "settle" && units == charge && digest == pointer.digest =>
        {
            Ok(())
        }
        _ => Err(RuntimeError::RestoreUnreconciled),
    }
}
impl ManagedRuntime {
    pub(in crate::continuity) async fn recover_capture(
        &mut self,
        runtime: &RuntimeAuthority,
        prepared: &Prepared,
    ) -> Result<(), RuntimeError> {
        let intent = Intent::decode(runtime, prepared)?;
        let source = Source::read(self, &intent.original)?;
        if source.attempt != intent.pointer.attempt || source.digest != intent.pointer.digest {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let verified = crate::capture_codec::authenticate(
            &runtime.custody,
            &source.attempt,
            &source.digest,
            &source.sealed,
        )?;
        let root = runtime
            .execution
            .evidence_payload(&source.attempt, &source.digest, &source.sealed)
            .await?;

        let mut sql = runtime.finance.continuity_transaction().await?;
        let sql_cp = self.sql_checkpoint(&mut sql).await?;
        let tx = runtime.execution.transaction().await?;
        let fdb_cp = self.fdb_checkpoint(runtime, &tx).await?;
        let original = intent
            .pointer
            .original(runtime, &tx, &root, &verified)
            .await?;
        if intent.financial.subject
            != crate::finance::snapshot::subject(
                &original.capture.binding.context.owner,
                original.capture.binding.period_start,
            )?
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let actual = super::super::inventory(runtime, &mut sql, &tx).await?;
        let expected = self
            .anchor
            .reader()
            .pending_subject_commitments(prepared)
            .map_err(error)?;
        let pointer_subject = intent.pointer.subject()?;
        if sql_cp == prepared.next {
            if fdb_cp == prepared.next {
                if actual != expected {
                    return Err(RuntimeError::RestoreUnreconciled);
                }
                let bytes = tx
                    .get(&history(&runtime.execution, prepared.next.sequence), false)
                    .await?
                    .ok_or(RuntimeError::RestoreUnreconciled)?;
                if serde_json::from_slice::<Prepared>(&bytes)
                    .map_err(|_| RuntimeError::InvalidBinding)?
                    != *prepared
                {
                    return Err(RuntimeError::RestoreUnreconciled);
                }
                observe(&mut sql, &original, &intent.pointer).await?;
                sql.commit().await?;
            } else if fdb_cp == prepared.previous {
                let mut partial = expected.clone();
                partial.remove(&pointer_subject);
                if actual != partial {
                    return Err(RuntimeError::RestoreUnreconciled);
                }
                observe(&mut sql, &original, &intent.pointer).await?;
                intent.pointer.stage(runtime, &tx, &root).await?;
                if super::super::inventory(runtime, &mut sql, &tx).await? != expected {
                    return Err(RuntimeError::RestoreUnreconciled);
                }
                self.write_fdb(runtime, &tx, prepared).await?;
                tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
                sql.commit().await?;
            } else {
                return Err(RuntimeError::RestoreUnreconciled);
            }
        } else if sql_cp == prepared.previous && fdb_cp == prepared.previous {
            let mut before = expected.clone();
            before.remove(&pointer_subject);
            match &intent.financial.previous {
                Some(value) => {
                    before.insert(intent.financial.subject.clone(), value.clone());
                }
                None => return Err(RuntimeError::RestoreUnreconciled),
            }
            if actual != before {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            settle(&mut sql, &original, &intent.pointer).await?;
            intent.pointer.stage(runtime, &tx, &root).await?;
            if super::super::inventory(runtime, &mut sql, &tx).await? != expected {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            self.write_sql(&mut sql, prepared).await?;
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
