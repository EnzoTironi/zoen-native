//! Acknowledgements of already retained expenses, never another spend or refund.
#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::perf,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod commit;
mod inventory;
mod record;
mod recovery;
use super::{error, Finance, RuntimeAuthority, RuntimeError, SubjectUpdate};
pub(super) use inventory::commitments;
pub(super) use record::ack_intent;
use record::{Intent, Record};

impl RuntimeAuthority {
    #[cfg(test)]
    pub(crate) fn test_ack_intent(&self, prepared: &super::Prepared) -> Result<(), RuntimeError> {
        Intent::decode(self, prepared).map(|_| ())
    }
    pub(crate) async fn ack_shared_outbox(
        &self,
        attempt: &str,
        kind: &str,
        digest: &str,
    ) -> Result<(), RuntimeError> {
        let record = Record {
            attempt: attempt.into(),
            kind: kind.into(),
            digest: digest.into(),
        };
        record.validate(self)?;
        let mut managed = self
            .runtime_continuity
            .as_ref()
            .ok_or(RuntimeError::RestoreUnreconciled)?
            .lock()
            .await;
        Box::pin(managed.reconcile(self)).await?;
        let mut sql = self.finance.continuity_transaction().await?;
        let previous = managed.sql_checkpoint(&mut sql).await?;
        let tx = self.execution.retention_transaction().await?;
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
        let binding = record.observe(self, &mut sql, &tx).await?;
        if record.acknowledged(&mut sql).await? {
            if tx.get(&record.key(self), false).await?.as_deref() != Some(b"1".as_slice()) {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            sql.commit().await?;
            return Ok(());
        }
        Finance::ack_outbox_in(&mut sql, attempt, kind, digest).await?;
        record.stage(self, &tx).await?;
        let after = super::inventory(self, &mut sql, &tx).await?;
        let subject =
            crate::finance::snapshot::subject(&binding.context.owner, binding.period_start)?;
        let intent = Intent {
            capture: SubjectUpdate {
                subject: record.subject()?,
                previous: before.get(&record.subject()?).cloned(),
                next: after
                    .get(&record.subject()?)
                    .cloned()
                    .ok_or(RuntimeError::InvalidBinding)?,
            },
            record,
            financial: SubjectUpdate {
                previous: before.get(&subject).cloned(),
                next: after
                    .get(&subject)
                    .cloned()
                    .ok_or(RuntimeError::InvalidBinding)?,
                subject,
            },
        };
        let mut expected = before;
        for update in intent.updates() {
            if expected.get(&update.subject) != update.previous.as_ref() {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            expected.insert(update.subject, update.next);
        }
        if expected != after {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let prepared = managed
            .anchor
            .prepare_subjects(
                &intent.effect()?,
                &previous,
                &previous,
                false,
                &intent.updates(),
                Some(&intent.seal(self)?),
            )
            .map_err(error)?;
        managed.commit_financial_ack(self, sql, tx, &prepared).await
    }
    pub(crate) async fn reconcile_shared(
        &self,
        limit: u16,
    ) -> Result<crate::ReconcileReport, RuntimeError> {
        let mut report = self.reconcile_shared_captures(limit).await?;
        for (attempt, kind, digest) in self
            .finance
            .pending_outbox_keys(limit.saturating_sub(report.inspected))
            .await?
        {
            report.inspected += 1;
            if Box::pin(self.ack_shared_outbox(&attempt, &kind, &digest))
                .await
                .is_ok()
            {
                report.transferred += 1;
            } else {
                report.parked += 1;
            }
        }
        Ok(report)
    }
}
