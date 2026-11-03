//! A durable reviewed set over the same signed forward writer. Each item's
//! acceptance and batch receipt commit together; the set is intentionally non-atomic.
use super::*;
mod options;
mod storage;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ForwardBatchSource {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub source_space: String,
    pub message_ids: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ForwardBatchScope {
    pub source: ForwardBatchSource,
    pub target_space: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ForwardBatchReview {
    pub batch_id: String,
    pub review_revision: String,
    pub target_space: String,
    pub messages: Vec<ForwardReview>,
    pub ready: bool,
    pub completed: bool,
    pub error: Option<String>,
    pub skipped_message_ids: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Batch {
    batch_id: String,
    review_revision: String,
    target_space: String,
    records: Vec<Record>,
    ready: bool,
    completed: bool,
    error: Option<String>,
    skipped_message_ids: Vec<String>,
}
impl ForwardBatchSource {
    fn item(&self, id: &str) -> ForwardSource {
        ForwardSource {
            expected_identity_id: self.expected_identity_id.clone(),
            expected_device_id: self.expected_device_id.clone(),
            source_space: self.source_space.clone(),
            message_id: id.into(),
        }
    }
}
impl Engine {
    fn batch_access(&self, source: &ForwardBatchSource) -> R<()> {
        // Bound encrypted local state and canonical projection work, not transport batches.
        if source.message_ids.is_empty() || source.message_ids.len() > 100 {
            return Err(invalid());
        }
        let mut ids = std::collections::HashSet::new();
        for id in &source.message_ids {
            if !ids.insert(id) {
                return Err(invalid());
            }
            self.forward_source_access(&source.item(id))?;
        }
        Ok(())
    }
    fn batch_target_access(&self, source: &ForwardBatchSource, target: &str) -> R<()> {
        self.batch_access(source)?;
        self.forward_access(&source.item(&source.message_ids[0]).target(target.into()))?;
        if !self.state.writer(target, &source.expected_identity_id) {
            return Err(stale());
        }
        Ok(())
    }
    fn batch_result(&self, source: &ForwardBatchSource, batch: Batch) -> R<ForwardBatchReview> {
        let messages = batch
            .records
            .into_iter()
            .map(|record| {
                self.forward_result(
                    &source
                        .item(&record.message.id)
                        .target(record.target_space.clone()),
                    record,
                )
            })
            .collect::<R<Vec<_>>>()?;
        Ok(ForwardBatchReview {
            batch_id: batch.batch_id,
            review_revision: batch.review_revision,
            target_space: batch.target_space,
            messages,
            ready: batch.ready,
            completed: batch.completed,
            error: batch.error,
            skipped_message_ids: batch.skipped_message_ids,
        })
    }
    pub(crate) fn forward_batch_status(
        &self,
        source: &ForwardBatchSource,
    ) -> R<Option<ForwardBatchReview>> {
        self.batch_access(source)?;
        self.load_forward_batch(source)?
            .map(|batch| self.batch_result(source, batch))
            .transpose()
    }
    pub(crate) fn forward_batch_review(
        &mut self,
        scope: &ForwardBatchScope,
    ) -> R<ForwardBatchReview> {
        self.batch_target_access(&scope.source, &scope.target_space)?;
        let retained = self.load_forward_batch(&scope.source)?;
        let mut batch = match retained {
            Some(batch) if !batch.completed => batch,
            _ => Batch {
                batch_id: types::new_id("forwardbatch")?,
                review_revision: String::new(),
                target_space: scope.target_space.clone(),
                records: Vec::new(),
                ready: false,
                completed: false,
                error: None,
                skipped_message_ids: Vec::new(),
            },
        };
        let retargeted = batch.target_space != scope.target_space;
        batch.target_space = scope.target_space.clone();
        let mut records = Vec::with_capacity(scope.source.message_ids.len());
        for id in &scope.source.message_ids {
            if let Some(accepted) = batch
                .records
                .iter()
                .find(|r| r.message.id == *id && r.accepted_message.is_some())
            {
                records.push(accepted.clone());
                continue;
            }
            let (message, source_revision) =
                self.forward_source(&scope.source.item(id).target(scope.target_space.clone()))?;
            // Preserve operation identities for unchanged pending projections.
            let operation_id = batch
                .records
                .iter()
                .find(|r| {
                    r.message.id == *id
                        && r.source_revision == source_revision
                        && r.target_space == scope.target_space
                })
                .map(|r| r.operation_id.clone())
                .unwrap_or(types::new_id("forward")?);
            let options = batch
                .records
                .iter()
                .find(|record| record.message.id == *id)
                .map(|record| record.options)
                .unwrap_or_default();
            records.push(Record {
                options,
                target_space: scope.target_space.clone(),
                operation_id,
                source_revision,
                message,
                accepted_message: None,
            });
        }
        // A repeated review of the same exact set is idempotent. An explicit
        // re-review after partial failure issues a fresh confirmation token.
        let changed = retargeted
            || !batch.ready
            || serde_json::to_vec(&batch.records).map_err(|_| invalid())?
                != serde_json::to_vec(&records).map_err(|_| invalid())?;
        batch.records = records;
        if changed {
            batch.review_revision = types::new_id("forwardreview")?;
        }
        batch.ready = true;
        batch.error = None;
        self.persist_forward_batch(&scope.source, &batch)?;
        self.batch_result(&scope.source, batch)
    }
    pub(crate) fn confirm_forward_batch(
        &mut self,
        source: &ForwardBatchSource,
        batch_id: &str,
        review_revision: &str,
    ) -> R<ForwardBatchReview> {
        self.batch_access(source)?;
        let mut batch = self.load_forward_batch(source)?.ok_or_else(stale)?;
        if batch.batch_id != batch_id || batch.review_revision != review_revision {
            return Err(stale());
        }
        // Replaying an observed acceptance never creates another signed entry.
        if batch.completed || batch.records.iter().all(|r| r.accepted_message.is_some()) {
            return self.batch_result(source, batch);
        }
        if !batch.ready {
            return Err(stale());
        }
        self.batch_target_access(source, &batch.target_space)?;
        // Freeze the entire still-pending set before the first publication.
        if !self.pending_forward_sources_current(source, &batch) {
            batch.ready = false;
            batch.error = Some(stale().to_string());
            self.persist_forward_batch(source, &batch)?;
            return self.batch_result(source, batch);
        }
        // Consume this explicit Send durably before any signed output. A cold
        // interrupted batch requires a new review for only its pending items.
        batch.ready = false;
        batch.error = None;
        self.persist_forward_batch(source, &batch)?;
        for index in 0..batch.records.len() {
            if batch.records[index].accepted_message.is_some() {
                continue;
            }
            let before = batch.clone();
            let result = self.atomic_agent_action(|engine| {
                engine.batch_target_access(source, &batch.target_space)?;
                let entry = engine.forward_with_options(
                    &source.source_space,
                    &batch.records[index].message.id,
                    &batch.target_space,
                    batch.records[index].options,
                )?;
                batch.records[index].accepted_message = Some(entry.id);
                engine.persist_forward_batch(source, &batch)
            });
            if let Err(reason) = result {
                batch = before;
                batch.ready = false;
                batch.error = Some(reason.to_string());
                // If the failure is storage itself, the previously committed
                // receipts remain authoritative; return the error and recover.
                self.persist_forward_batch(source, &batch)?;
                return self.batch_result(source, batch);
            }
        }
        self.batch_result(source, batch)
    }
    pub(crate) fn abandon_forward_batch(
        &mut self,
        source: &ForwardBatchSource,
        batch_id: &str,
        revision: &str,
    ) -> R<ForwardBatchReview> {
        self.batch_access(source)?;
        let mut batch = self.load_forward_batch(source)?.ok_or_else(stale)?;
        if batch.batch_id != batch_id || batch.review_revision != revision {
            return Err(stale());
        }
        batch.skipped_message_ids = batch
            .records
            .iter()
            .filter(|record| record.accepted_message.is_none())
            .map(|record| record.message.id.clone())
            .collect();
        batch.ready = false;
        batch.completed = true;
        batch.error = None;
        self.persist_forward_batch(source, &batch)?;
        self.batch_result(source, batch)
    }
    pub(crate) fn finish_forward_batch(
        &mut self,
        source: &ForwardBatchSource,
        batch_id: &str,
    ) -> R<ForwardBatchReview> {
        self.batch_access(source)?;
        let mut batch = self.load_forward_batch(source)?.ok_or_else(stale)?;
        if batch.batch_id != batch_id
            || batch.records.iter().any(|r| {
                r.accepted_message.is_none() && !batch.skipped_message_ids.contains(&r.message.id)
            })
        {
            return Err(stale());
        }
        batch.completed = true;
        batch.ready = false;
        batch.error = None;
        self.persist_forward_batch(source, &batch)?;
        self.batch_result(source, batch)
    }
}
impl Client {
    pub fn abandon_forward_batch(
        &self,
        source: ForwardBatchSource,
        batch_id: String,
        review_revision: String,
    ) -> Result<ForwardBatchReview, CoreError> {
        self.lock()
            .abandon_forward_batch(&source, &batch_id, &review_revision)
    }
    pub fn forward_batch_status(
        &self,
        source: ForwardBatchSource,
    ) -> Result<Option<ForwardBatchReview>, CoreError> {
        self.lock().forward_batch_status(&source)
    }
    pub fn forward_batch_review(
        &self,
        scope: ForwardBatchScope,
    ) -> Result<ForwardBatchReview, CoreError> {
        self.lock().forward_batch_review(&scope)
    }
    pub fn confirm_forward_batch(
        &self,
        source: ForwardBatchSource,
        batch_id: String,
        review_revision: String,
    ) -> Result<ForwardBatchReview, CoreError> {
        self.lock()
            .confirm_forward_batch(&source, &batch_id, &review_revision)
    }
    pub fn finish_forward_batch(
        &self,
        source: ForwardBatchSource,
        batch_id: String,
    ) -> Result<ForwardBatchReview, CoreError> {
        self.lock().finish_forward_batch(&source, &batch_id)
    }
}
