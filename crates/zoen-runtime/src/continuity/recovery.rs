//! Only the original signed policy can advance an interrupted detector. No
//! model admission, reserve, settlement or refund is constructed by recovery.
use super::*;

impl ManagedRuntime {
    pub(crate) async fn reconcile(
        &mut self,
        runtime: &RuntimeAuthority,
    ) -> Result<(), RuntimeError> {
        if let Some(prepared) = self.anchor.pending().map_err(error)? {
            if crate::native::shared_receipt_intent(&prepared) {
                return Box::pin(crate::native::recover_shared_receipt(
                    runtime, self, &prepared,
                ))
                .await;
            }
            if super::association::association_intent(&prepared) {
                return Box::pin(self.recover_capture_association(runtime, &prepared)).await;
            }
            if super::ack::ack_intent(&prepared) {
                return Box::pin(self.recover_financial_ack(runtime, &prepared)).await;
            }
            if super::capture::capture_intent(&prepared) {
                return Box::pin(self.recover_capture(runtime, &prepared)).await;
            }
            if super::dispatch::dispatch_intent(&prepared) {
                return Box::pin(self.recover_dispatch(runtime, &prepared)).await;
            }
            if super::reservation::financial_intent(&prepared) {
                return Box::pin(self.recover_financial_phase(runtime, &prepared)).await;
            }
            if crate::native::cancel_intent(&prepared) {
                return Box::pin(crate::native::recover_shared_cancel(
                    runtime, self, &prepared,
                ))
                .await;
            }
            if crate::native::shared_intent(&prepared) {
                return Box::pin(crate::native::recover_shared_native(
                    runtime, self, &prepared,
                ))
                .await;
            }
        }
        let mut tx = runtime.finance.continuity_transaction().await?;
        if Finance::policy_scope(&mut tx).await? != Some(3) {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let sql = self.sql_checkpoint(&mut tx).await?;
        let fdb = runtime.execution.transaction().await?;
        let observed = self.fdb_checkpoint(runtime, &fdb).await?;
        let actual = inventory(runtime, &mut tx, &fdb).await?;
        let Some(prepared) = self.anchor.pending().map_err(error)? else {
            if self.anchor.reconcile(&sql, &observed).map_err(error)? != Recovery::Clean
                || actual != self.anchor.reader().subject_commitments().map_err(error)?
            {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            tx.commit().await?;
            return Ok(());
        };
        let signed = self.retained_policy(runtime, &prepared)?;
        let expected = self
            .anchor
            .reader()
            .pending_subject_commitments(&prepared)
            .map_err(error)?;
        if sql == prepared.next && observed == prepared.next {
            let bytes = fdb
                .get(&history(&runtime.execution, prepared.next.sequence), false)
                .await?
                .ok_or(RuntimeError::RestoreUnreconciled)?;
            let original: Prepared =
                serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
            if original != prepared || actual != expected {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            tx.commit().await?;
            if self.anchor.reconcile(&sql, &observed).map_err(error)? != Recovery::BothAfter {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            return Ok(());
        }
        // SQL-next/FDB-before contradicts this operation's original order.
        // Never repair it by inventing the missing FDB effect history.
        if sql != prepared.previous || (observed != prepared.previous && observed != prepared.next)
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let [update] = prepared.subject_updates.as_slice() else {
            return Err(RuntimeError::RestoreUnreconciled);
        };
        if actual.get(&update.subject) != update.previous.as_ref() {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let mut prior = expected.clone();
        match &update.previous {
            Some(value) => {
                prior.insert(update.subject.clone(), value.clone());
            }
            None => {
                prior.remove(&update.subject);
            }
        }
        if actual != prior {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        // Fresh signature/device/expiry/version checks, still using the exact
        // original policy and the original SQL transaction implementation.
        Finance::install_continuous_policy_in(&mut tx, signed).await?;
        if inventory(runtime, &mut tx, &fdb).await? != expected {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        if observed == prepared.previous {
            self.write_fdb(runtime, &fdb, &prepared).await?;
            fdb.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        } else {
            let bytes = fdb
                .get(&history(&runtime.execution, prepared.next.sequence), false)
                .await?
                .ok_or(RuntimeError::RestoreUnreconciled)?;
            let original: Prepared =
                serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
            if original != prepared {
                return Err(RuntimeError::RestoreUnreconciled);
            }
        }
        self.write_sql(&mut tx, &prepared).await?;
        tx.commit().await?;
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
