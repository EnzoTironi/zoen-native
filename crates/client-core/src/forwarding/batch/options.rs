//! Accepted records are immutable; only still-pending output can be reconfigured.
use super::*;

impl Engine {
    pub(super) fn pending_forward_sources_current(
        &self,
        source: &ForwardBatchSource,
        batch: &Batch,
    ) -> bool {
        batch
            .records
            .iter()
            .filter(|record| record.accepted_message.is_none())
            .all(|record| {
                let scope = source
                    .item(&record.message.id)
                    .target(batch.target_space.clone());
                self.forward_source(&scope)
                    .is_ok_and(|(_, revision)| revision == record.source_revision)
            })
    }

    pub(crate) fn set_forward_batch_options(
        &mut self,
        source: &ForwardBatchSource,
        batch_id: &str,
        revision: &str,
        options: ForwardOptions,
    ) -> R<ForwardBatchReview> {
        self.batch_access(source)?;
        let mut batch = self.load_forward_batch(source)?.ok_or_else(stale)?;
        if batch.batch_id != batch_id || batch.review_revision != revision || batch.completed {
            return Err(stale());
        }
        self.batch_target_access(source, &batch.target_space)?;
        if !batch.records.iter().any(|r| r.accepted_message.is_none())
            || !self.pending_forward_sources_current(source, &batch)
        {
            return Err(stale());
        }
        if batch
            .records
            .iter()
            .any(|r| r.accepted_message.is_none() && r.options != options)
        {
            for record in &mut batch.records {
                if record.accepted_message.is_none() {
                    record.options = options;
                    record.operation_id = types::new_id("forward")?;
                }
            }
            batch.review_revision = types::new_id("forwardreview")?;
            // An interrupted batch still needs an explicit re-review. Changing
            // private options never arms Send or rewrites accepted receipts.
            self.persist_forward_batch(source, &batch)?;
        }
        self.batch_result(source, batch)
    }
}
