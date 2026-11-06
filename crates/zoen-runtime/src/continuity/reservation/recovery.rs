//! Recover original SQL-first financial state. Observing a nonce never recovers
//! a fresh one-shot claim. New financial writes need actual native extraction.
use super::*;
impl ManagedRuntime {
    pub(in crate::continuity) async fn recover_financial_phase(
        &mut self,
        runtime: &RuntimeAuthority,
        prepared: &Prepared,
    ) -> Result<(), RuntimeError> {
        let intent = Intent::decode(runtime, prepared)?;
        if prepared.retirement
            || prepared.effect_digest != intent.effect()?
            || prepared.subject_updates != [intent.update()?]
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let mut sql = runtime.finance.continuity_transaction().await?;
        let sql_cp = self.sql_checkpoint(&mut sql).await?;
        let tx = runtime.execution.transaction().await?;
        let fdb_cp = self.fdb_checkpoint(runtime, &tx).await?;
        let actual = inventory(runtime, &mut sql, &tx).await?;
        let expected = self
            .anchor
            .reader()
            .pending_subject_commitments(prepared)
            .map_err(error)?;
        if sql_cp == prepared.next {
            if actual != expected || (fdb_cp != prepared.previous && fdb_cp != prepared.next) {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            if let Some(nonce) = &intent.witness_nonce {
                Finance::observe_witness(&mut sql, &intent.binding, nonce).await?;
            } else {
                match &intent.claim_nonce {
                    Some(nonce) => Finance::observe_claim(&mut sql, &intent.binding, nonce).await?,
                    None => Finance::observe_reservation(&mut sql, &intent.binding).await?,
                }
            }
            if fdb_cp == prepared.previous {
                self.write_fdb(runtime, &tx, prepared).await?;
                tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
            } else {
                let original = tx
                    .get(&history(&runtime.execution, prepared.next.sequence), false)
                    .await?
                    .ok_or(RuntimeError::RestoreUnreconciled)?;
                if serde_json::from_slice::<Prepared>(&original)
                    .map_err(|_| RuntimeError::InvalidBinding)?
                    != *prepared
                {
                    return Err(RuntimeError::RestoreUnreconciled);
                }
            }
            sql.commit().await?;
        } else {
            if sql_cp != prepared.previous
                || fdb_cp != prepared.previous
                || projected(&actual, &intent)? != expected
            {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            if intent.witness_nonce.is_some() {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            // No original financial commit exists: derive fresh facts and real
            // paired leases, rather than constructing authority from metadata.
            drop(tx);
            drop(sql);
            let native = runtime
                .native
                .as_ref()
                .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
            let step = VerifiedStep::Native(Box::new(
                native
                    .verified_reply(runtime, &intent.binding.context.run_id)
                    .await?,
            ));
            let result: Result<(), RuntimeError> = async {
                let mut sql = runtime.finance.continuity_transaction().await?;
                let tx = runtime.execution.transaction().await?;
                if self.sql_checkpoint(&mut sql).await? != prepared.previous
                    || self.fdb_checkpoint(runtime, &tx).await? != prepared.previous
                    || projected(&inventory(runtime, &mut sql, &tx).await?, &intent)? != expected
                {
                    return Err(RuntimeError::RestoreUnreconciled);
                }
                scope(runtime, &step, &intent.binding, &tx).await?;
                if let VerifiedStep::Native(native) = &step {
                    native.current_definition_in(&mut sql).await?;
                }
                let inserted = match &intent.claim_nonce {
                    Some(nonce) => {
                        Finance::restore_claim_in(&mut sql, &intent.binding, nonce).await?
                    }
                    None => Finance::reserve_in(&mut sql, &intent.binding).await?,
                };
                if !inserted || inventory(runtime, &mut sql, &tx).await? != expected {
                    return Err(RuntimeError::RestoreUnreconciled);
                }
                Finance::check_financial_deadline(&mut sql, &intent.binding, step.grant_deadline())
                    .await?;
                scope(runtime, &step, &intent.binding, &tx).await?;
                self.write_sql(&mut sql, prepared).await?;
                sql.commit().await?;
                self.write_fdb(runtime, &tx, prepared).await?;
                tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
                Ok(())
            }
            .await;
            let released = step.release(&runtime.execution).await;
            result?;
            released?;
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
