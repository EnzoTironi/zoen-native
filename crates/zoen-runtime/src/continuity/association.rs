//! Connect incurred results to their original replies, without a publication grant.
#![deny(clippy::pedantic)]
mod record;
mod recovery;
use super::{error, Finance, RuntimeAuthority, RuntimeError, SubjectUpdate};
use base64::{engine::general_purpose::STANDARD, Engine as _};
pub(super) use record::association_intent;
use record::Intent;

impl RuntimeAuthority {
    #[cfg(test)]
    pub(crate) fn test_association_intent(
        &self,
        prepared: &super::Prepared,
    ) -> Result<(), RuntimeError> {
        Intent::decode(self, prepared).map(|_| ())
    }

    pub(crate) async fn associate_shared_capture(
        &self,
        attempt: &str,
        digest: &str,
    ) -> Result<(), RuntimeError> {
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
        let (binding, _, nonce) = super::capture::reference(self, &tx, attempt, digest).await?;
        Finance::observe_witness(&mut sql, &binding, &nonce).await?;
        if self
            .execution
            .retained_capture_association_in(&tx, &binding, (digest, &nonce), &self.custody)
            .await?
            .is_some()
        {
            sql.commit().await?;
            return Ok(());
        }
        let bytes = self
            .execution
            .prepare_capture_association_in(&tx, &binding, (digest, &nonce), &self.custody)
            .await?;
        let subject = crate::hash(
            &serde_json::to_vec(&("runtime-capture/1", attempt, digest))
                .map_err(|_| RuntimeError::InvalidBinding)?,
        );
        let mut intent = Intent {
            attempt: attempt.into(),
            digest: digest.into(),
            association: STANDARD.encode(bytes),
            update: SubjectUpdate {
                subject: subject.clone(),
                previous: before.get(&subject).cloned(),
                next: String::new(),
            },
        };
        if intent.update.previous.is_none() {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        intent.stage(self, &mut sql, &tx).await?;
        let after = super::inventory(self, &mut sql, &tx).await?;
        intent.update.next = after
            .get(&subject)
            .cloned()
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let mut expected = before;
        expected.insert(subject, intent.update.next.clone());
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
                &[intent.update.clone()],
                Some(&intent.seal(self)?),
            )
            .map_err(error)?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 140 {
            return Err(RuntimeError::Unavailable);
        }
        managed.write_sql(&mut sql, &prepared).await?;
        sql.commit().await?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 141 {
            return Err(RuntimeError::Unavailable);
        }
        managed.write_fdb(self, &tx, &prepared).await?;
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 142 {
            return Err(RuntimeError::Unavailable);
        }
        Box::pin(managed.reconcile(self)).await?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 143 {
            return Err(RuntimeError::Unavailable);
        }
        Ok(())
    }
    pub(super) async fn reconcile_shared_associations(
        &self,
        limit: u16,
    ) -> Result<crate::ReconcileReport, RuntimeError> {
        let mut report = crate::ReconcileReport::default();
        for (attempt, digest) in super::capture::pending_associations(self, limit).await? {
            report.inspected += 1;
            if Box::pin(self.associate_shared_capture(&attempt, &digest))
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
