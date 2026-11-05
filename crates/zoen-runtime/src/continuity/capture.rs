//! Retain incurred chat results and cost without minting execution/output rights.
mod inbox;
mod inventory;
mod record;
mod recovery;
mod reference;
use super::*;
use crate::FinancialState;
use base64::{engine::general_purpose::STANDARD, Engine as _};
pub(super) use inventory::commitments;
pub(super) use record::capture_intent;
use record::{Intent, Original, Pointer, Source};
pub(super) use reference::{reference, pending_associations};
use serde::{Deserialize, Serialize};

async fn settle(
    sql: &mut Transaction<'_, Postgres>,
    original: &Original<'_>,
    pointer: &Pointer,
) -> Result<FinancialState, RuntimeError> {
    let capture = original.capture;
    Finance::observe_witness(sql, &capture.binding, &original.nonce).await?;
    let prior: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT sealed FROM runtime_evidence WHERE attempt=$1 AND digest=$2")
            .bind(&pointer.attempt)
            .bind(&pointer.digest)
            .fetch_optional(&mut **sql)
            .await?;
    if prior.is_some_and(|bytes| bytes != original.sealed) {
        return Err(RuntimeError::RestoreUnreconciled);
    }
    let charge = capture.binding.price.chat()?.charge(&capture.result)?;
    Finance::settle_capture_in(
        sql,
        &capture.binding,
        charge,
        &pointer.digest,
        &original.sealed,
    )
    .await
}
impl RuntimeAuthority {
    pub(crate) async fn capture_shared(
        &self,
        attempt: &str,
        digest: &str,
        sealed: &[u8],
    ) -> Result<FinancialState, RuntimeError> {
        let verified = crate::capture_codec::authenticate(&self.custody, attempt, digest, sealed)?;
        let capture = verified.capture();
        if capture.binding.context.attempt_id != attempt
            || capture.result.context != capture.binding.context
            || capture.result.request_digest != capture.binding.request_digest
        {
            return Err(RuntimeError::InvalidBinding);
        }
        capture.binding.price.chat()?.charge(&capture.result)?;
        let managed = self
            .runtime_continuity
            .as_ref()
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let mut managed = managed.lock().await;
        let source = Source {
            attempt: attempt.into(),
            digest: digest.into(),
            sealed: sealed.to_vec(),
        };
        let original_key = match managed
            .anchor
            .retain_bounded_original(&source.bytes()?, record::SOURCE_MAXIMUM)
        {
            Ok(key) => key,
            Err(failure) => {
                // Preserve the existing independent FDB fallback if the host
                // cannot acknowledge its original. It grants no output right.
                drop(managed);
                let _ = self
                    .execution
                    .retain_evidence(attempt, digest, sealed)
                    .await;
                return Err(error(failure));
            }
        };
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 122 {
            return Err(RuntimeError::Unavailable);
        }
        Box::pin(managed.reconcile(self)).await?;
        // Upload immutable chunks before acquiring the transaction read version.
        // A snapshot predating those commits cannot verify its new payload.
        let root = self
            .execution
            .evidence_payload(attempt, digest, sealed)
            .await?;

        #[cfg(test)]
        let observed = std::time::Instant::now();
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
        #[cfg(test)]
        if sealed.len() > 32 * 1024 * 1024 {
            eprintln!(
                "large capture stage=before-inventory elapsed_ms={}",
                observed.elapsed().as_millis()
            );
        }
        let nonce: String =
            sqlx::query_scalar("SELECT nonce FROM runtime_admission_witnesses WHERE attempt=$1")
                .bind(attempt)
                .fetch_optional(&mut *sql)
                .await?
                .ok_or(RuntimeError::RestoreUnreconciled)?;
        Finance::observe_witness(&mut sql, &capture.binding, &nonce).await?;
        let pointer = Pointer::new(capture, digest, sealed, &root, &nonce)?;
        let original = pointer.original(self, &tx, &root, &verified).await?;
        #[cfg(test)]
        if sealed.len() > 32 * 1024 * 1024 {
            eprintln!(
                "large capture stage=verified-original elapsed_ms={}",
                observed.elapsed().as_millis()
            );
        }
        let existing = tx.get(&pointer.key(self), false).await?.is_some();
        let state = settle(&mut sql, &original, &pointer).await?;
        #[cfg(test)]
        if sealed.len() > 32 * 1024 * 1024 {
            eprintln!(
                "large capture stage=settled elapsed_ms={}",
                observed.elapsed().as_millis()
            );
        }
        pointer.stage(self, &tx, &root).await?;
        let after = super::inventory(self, &mut sql, &tx).await?;
        #[cfg(test)]
        if sealed.len() > 32 * 1024 * 1024 {
            eprintln!(
                "large capture stage=after-inventory elapsed_ms={}",
                observed.elapsed().as_millis()
            );
        }
        if existing {
            if before != after {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            sql.commit().await?;
            return Ok(state);
        }
        let subject = crate::finance::snapshot::subject(
            &capture.binding.context.owner,
            capture.binding.period_start,
        )?;
        let intent = Intent {
            pointer,
            original: original_key,
            financial: SubjectUpdate {
                previous: before.get(&subject).cloned(),
                next: after
                    .get(&subject)
                    .cloned()
                    .ok_or(RuntimeError::InvalidBinding)?,
                subject,
            },
        };
        let mut expected = before.clone();
        for update in intent.updates()? {
            if expected.get(&update.subject) != update.previous.as_ref() {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            expected.insert(update.subject, update.next);
        }
        if expected != after {
            return Err(RuntimeError::InvalidBinding);
        }
        let prepared = managed
            .anchor
            .prepare_subjects(
                &intent.effect()?,
                &previous,
                &previous,
                false,
                &intent.updates()?,
                Some(&intent.seal(self)?),
            )
            .map_err(error)?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 118 {
            return Err(RuntimeError::Unavailable);
        }
        #[cfg(test)]
        if sealed.len() > 32 * 1024 * 1024 {
            eprintln!(
                "large capture stage=prepared elapsed_ms={}",
                observed.elapsed().as_millis()
            );
        }
        managed.write_sql(&mut sql, &prepared).await?;
        sql.commit().await?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 119 {
            return Err(RuntimeError::Unavailable);
        }
        managed.write_fdb(self, &tx, &prepared).await?;
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 120 {
            return Err(RuntimeError::Unavailable);
        }
        Box::pin(managed.reconcile(self)).await?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 121 {
            return Err(RuntimeError::Unavailable);
        }
        Ok(state)
    }
}
