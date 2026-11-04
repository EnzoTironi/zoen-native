//! Pending work is derived from original evidence and the existing commitment;
//! there is no second financial state, refund proof or provider queue.
use super::*;
impl RuntimeAuthority {
    pub(crate) async fn reconcile_shared_captures(
        &self,
        limit: u16,
    ) -> Result<crate::ReconcileReport, RuntimeError> {
        let originals = {
            let mut managed = self
                .runtime_continuity
                .as_ref()
                .ok_or(RuntimeError::RestoreUnreconciled)?
                .lock()
                .await;
            Box::pin(managed.reconcile(self)).await?;
            let subjects = managed
                .anchor
                .reader()
                .subject_commitments()
                .map_err(error)?;
            let mut originals = Vec::new();
            for key in managed
                .anchor
                .reader()
                .retained_originals()
                .map_err(error)?
            {
                let source = Source::read(&managed, &key)?;
                let capture: Capture = self.custody.open(
                    &format!("evidence/{}", source.attempt),
                    Some(&source.digest),
                    &source.sealed,
                )?;
                if capture.binding.context.attempt_id != source.attempt {
                    return Err(RuntimeError::InvalidBinding);
                }
                let subject = crate::hash(
                    &serde_json::to_vec(&("runtime-capture/1", &source.attempt, &source.digest))
                        .map_err(|_| RuntimeError::InvalidBinding)?,
                );
                if !subjects.contains_key(&subject) {
                    originals.push(source);
                    if originals.len() >= usize::from(limit) {
                        break;
                    }
                }
            }
            originals
        };
        let mut report = crate::ReconcileReport::default();
        for source in originals {
            report.inspected += 1;
            if self
                .capture_shared(&source.attempt, &source.digest, &source.sealed)
                .await
                .is_ok()
            {
                report.transferred += 1;
            } else {
                report.parked += 1;
            }
        }
        let remaining = limit.saturating_sub(report.inspected);
        if remaining > 0 {
            for (attempt, digest, sealed) in self.execution.pending_evidence(remaining).await? {
                report.inspected += 1;
                if self
                    .capture_shared(&attempt, &digest, &sealed)
                    .await
                    .is_ok()
                    && self.execution.ack_evidence(&attempt, &digest).await.is_ok()
                {
                    report.transferred += 1;
                } else {
                    report.parked += 1;
                }
            }
        }
        // Financial outbox acknowledgements need their own subsequent shared
        // transition. Never execute the unmanaged mutation path in this profile.
        let remaining = limit.saturating_sub(report.inspected);
        if remaining > 0 {
            let parked = u16::try_from(self.finance.pending_outbox(remaining).await?.len())
                .map_err(|_| RuntimeError::EvidenceTooLarge)?;
            report.inspected += parked;
            report.parked += parked;
        }
        Ok(report)
    }
}
