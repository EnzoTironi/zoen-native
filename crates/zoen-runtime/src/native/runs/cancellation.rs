//! A request to stop is distinct from a known fenced proof of non-dispatch.
use foundationdb::Transaction;

use super::*;
use crate::custody::Custody;

#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CancellationRequest {
    format: u8,
    run: String,
    attempt: String,
    record_digest: String,
    binding_digest: String,
}

impl CancellationRequest {
    fn original(record: &RunRecord, record_digest: &str) -> Result<Self, RuntimeError> {
        Ok(Self {
            format: 1,
            run: record.locator.run.clone(),
            attempt: record.locator.attempt.clone(),
            record_digest: record_digest.into(),
            binding_digest: record.binding.digest()?,
        })
    }

    fn subject(&self) -> String {
        format!("native-cancel/1/{}", self.run)
    }
}

pub(crate) enum CleanupAcquisition {
    Busy,
    Ready(Box<CleanupStep>),
}

/// This has no current grant, native credential or gateway dependency. Its
/// real paired leases are insufficient for refund until the final closed write.
pub(crate) struct CleanupStep {
    record: RunRecord,
    record_digest: String,
    fences: ReplyFences,
}

impl CleanupStep {
    pub(crate) fn binding(&self) -> &Binding {
        &self.record.binding
    }

    pub(crate) async fn scope_in(
        &self,
        execution: &Execution,
        trx: &Transaction,
        custody: &Custody,
    ) -> Result<(), RuntimeError> {
        execution
            .reply_original_scope(trx, &self.record, &self.record_digest)
            .await?;
        execution
            .reply_binding_in(trx, &self.record, custody)
            .await?;
        execution
            .check_reply_cancel(trx, &self.record, &self.record_digest, custody)
            .await?;
        let version = trx.get_read_version().await?;
        Execution::lease(
            trx,
            &execution.run(&self.record.locator.run, "lease"),
            &self.fences.run,
            version,
        )
        .await?;
        Execution::lease(
            trx,
            &execution.native_lease(&self.record.original.context.principal),
            &self.fences.device,
            version,
        )
        .await
    }

    pub(crate) async fn release(&self, execution: &Execution) -> Result<(), RuntimeError> {
        execution.release_reply(&self.record, &self.fences).await
    }
}

#[cfg(test)]
impl ReplyStep {
    pub(crate) async fn cleanup_for_known_holder(
        &self,
        execution: &Execution,
        custody: &Custody,
    ) -> Result<CleanupStep, RuntimeError> {
        let trx = execution.transaction().await?;
        let (record, record_digest) = execution
            .reply_in(&trx, &self.loaded.record.locator.run, custody)
            .await?;
        if record_digest != self.loaded.record_digest {
            return Err(RuntimeError::InvalidBinding);
        }
        let step = CleanupStep {
            record,
            record_digest,
            fences: ReplyFences {
                run: self.loaded.fences.run.clone(),
                device: self.loaded.fences.device.clone(),
            },
        };
        step.scope_in(execution, &trx, custody).await?;
        Ok(step)
    }
}

impl Execution {
    async fn check_reply_cancel(
        &self,
        trx: &Transaction,
        record: &RunRecord,
        record_digest: &str,
        custody: &Custody,
    ) -> Result<(), RuntimeError> {
        let expected = CancellationRequest::original(record, record_digest)?;
        let sealed = trx
            .get(&self.run(&record.locator.run, "cancel"), false)
            .await?
            .ok_or(RuntimeError::Denied)?;
        let actual: CancellationRequest = custody.open(&expected.subject(), None, &sealed)?;
        if actual != expected {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }

    /// Recording the exact original stop request needs no worker lease. This
    /// transaction cannot mint a dispatch permit or financial release proof.
    pub(crate) async fn request_reply_cancel(
        &self,
        run: &str,
        custody: &Custody,
        runtime: &RuntimeAuthority,
    ) -> Result<(), RuntimeError> {
        runtime.continuity()?;
        let trx = self.transaction().await?;
        let (record, record_digest) = self.reply_in(&trx, run, custody).await?;
        self.reply_original_scope(&trx, &record, &record_digest)
            .await?;
        self.reply_binding_in(&trx, &record, custody).await?;
        let request = CancellationRequest::original(&record, &record_digest)?;
        let key = self.run(run, "cancel");
        if trx.get(&key, false).await?.is_some() {
            self.check_reply_cancel(&trx, &record, &record_digest, custody)
                .await?;
        } else {
            let (_, sealed) = custody.seal(&request.subject(), &request)?;
            trx.set(&key, &sealed);
        }
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 26 {
            return Err(RuntimeError::Unavailable);
        }
        runtime.continuity()
    }

    pub(crate) async fn cleanup_reply(
        &self,
        run: &str,
        custody: &Custody,
    ) -> Result<CleanupAcquisition, RuntimeError> {
        let trx = self.transaction().await?;
        let (record, record_digest) = self.reply_in(&trx, run, custody).await?;
        self.reply_original_scope(&trx, &record, &record_digest)
            .await?;
        self.reply_binding_in(&trx, &record, custody).await?;
        self.check_reply_cancel(&trx, &record, &record_digest, custody)
            .await?;
        let Some(fences) = self.acquire_reply_in(&trx, &record).await? else {
            return Ok(CleanupAcquisition::Busy);
        };
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(CleanupAcquisition::Ready(Box::new(CleanupStep {
            record,
            record_digest,
            fences,
        })))
    }
}
