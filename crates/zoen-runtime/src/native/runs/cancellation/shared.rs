//! A durable stop request is not a claim that execution stopped or money was
//! released. Retain the existing request bytes under the shared checkpoint.
mod recovery;
use super::*;
use crate::continuity::{inventory, ManagedRuntime};
use base64::{engine::general_purpose::STANDARD, Engine as _};
pub(crate) use recovery::recover_shared_cancel;
use relay_admission::managed_images::{Prepared, Recovery, SubjectUpdate};
const PREFIX: &str = "cancel_v1_";

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Pointer {
    request: CancellationRequest,
    ciphertext_digest: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    pointer: Pointer,
    ciphertext: String,
}
impl Pointer {
    fn key(&self, runtime: &RuntimeAuthority) -> Vec<u8> {
        runtime
            .execution
            .root
            .pack(&("shared-native-cancel/1", self.request.run.as_str()))
    }
    fn subject(&self) -> String {
        hash(format!("runtime-native-cancel/1/{}", self.request.run).as_bytes())
    }
    fn bytes(&self) -> Result<Vec<u8>, RuntimeError> {
        serde_json::to_vec(self).map_err(|_| RuntimeError::InvalidBinding)
    }
    fn update(&self) -> Result<SubjectUpdate, RuntimeError> {
        Ok(SubjectUpdate {
            subject: self.subject(),
            previous: None,
            next: hash(&self.bytes()?),
        })
    }
    fn effect(&self) -> Result<String, RuntimeError> {
        Ok(hash(
            &serde_json::to_vec(&("runtime-native-stop/1", self))
                .map_err(|_| RuntimeError::InvalidBinding)?,
        ))
    }
    async fn observe(
        &self,
        runtime: &RuntimeAuthority,
        tx: &Transaction,
    ) -> Result<(), RuntimeError> {
        let key = runtime.execution.run(&self.request.run, "cancel");
        let bytes = tx
            .get(&key, false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if hash(&bytes) != self.ciphertext_digest {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let request: CancellationRequest =
            runtime
                .custody
                .open(&self.request.subject(), None, &bytes)?;
        let record = tx
            .get(&runtime.execution.run(&request.run, "record/1"), false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if request != self.request || request.format != 1 || hash(&record) != request.record_digest
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(())
    }
}
impl Intent {
    async fn validate_original(
        &self,
        runtime: &RuntimeAuthority,
        tx: &Transaction,
    ) -> Result<Vec<u8>, RuntimeError> {
        let request = &self.pointer.request;
        let (record, digest) = runtime
            .execution
            .reply_in(tx, &request.run, &runtime.custody)
            .await?;
        runtime
            .execution
            .reply_original_scope(tx, &record, &digest)
            .await?;
        runtime
            .execution
            .reply_binding_in(tx, &record, &runtime.custody)
            .await?;
        if CancellationRequest::original(&record, &digest)? != *request
            || self.ciphertext.len() > 8192
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let bytes = STANDARD
            .decode(&self.ciphertext)
            .map_err(|_| RuntimeError::InvalidBinding)?;
        let decoded: CancellationRequest =
            runtime.custody.open(&request.subject(), None, &bytes)?;
        if decoded != *request || hash(&bytes) != self.pointer.ciphertext_digest {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(bytes)
    }
    async fn apply(
        &self,
        runtime: &RuntimeAuthority,
        tx: &Transaction,
    ) -> Result<(), RuntimeError> {
        let bytes = self.validate_original(runtime, tx).await?;
        for (key, value) in [
            (
                runtime.execution.run(&self.pointer.request.run, "cancel"),
                bytes,
            ),
            (self.pointer.key(runtime), self.pointer.bytes()?),
        ] {
            if tx
                .get(&key, false)
                .await?
                .as_deref()
                .is_some_and(|prior| prior != value)
            {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            tx.set(&key, &value);
        }
        Ok(())
    }
    fn seal(&self, runtime: &RuntimeAuthority) -> Result<String, RuntimeError> {
        let (_, bytes) = runtime.custody.seal("runtime-native-stop/1", self)?;
        let encoded = format!("{PREFIX}{}", STANDARD.encode(bytes));
        if encoded.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        Ok(encoded)
    }
}
pub(crate) fn cancel_intent(prepared: &Prepared) -> bool {
    prepared
        .intent
        .as_deref()
        .is_some_and(|value| value.starts_with(PREFIX))
}
pub(crate) async fn cancel_commitments(
    runtime: &RuntimeAuthority,
    tx: &Transaction,
) -> Result<BTreeMap<String, String>, RuntimeError> {
    let prefix = runtime
        .execution
        .root
        .subspace(&("shared-native-cancel/1",));
    let rows = crate::native::storage::bounded_rows(
        tx,
        foundationdb::RangeOption::from(prefix.range()),
        513,
        8192,
    )
    .await?;
    if rows.len() > 512 {
        return Err(RuntimeError::EvidenceTooLarge);
    }
    let mut result = BTreeMap::new();
    for (key, bytes) in rows {
        let pointer: Pointer =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
        if key != pointer.key(runtime) || bytes != pointer.bytes()? {
            return Err(RuntimeError::InvalidBinding);
        }
        pointer.observe(runtime, tx).await?;
        if result.insert(pointer.subject(), hash(&bytes)).is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
    }
    Ok(result)
}
impl RuntimeAuthority {
    pub(crate) async fn request_shared_reply_cancel(&self, run: &str) -> Result<(), RuntimeError> {
        let managed = self
            .runtime_continuity
            .as_ref()
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let mut managed = managed.lock().await;
        Box::pin(managed.reconcile(self)).await?;
        let mut sql = self.finance.continuity_transaction().await?;
        let previous = managed.sql_checkpoint(&mut sql).await?;
        let tx = self.execution.transaction().await?;
        let before = inventory(self, &mut sql, &tx).await?;
        if managed.fdb_checkpoint(self, &tx).await? != previous
            || managed
                .anchor
                .reader()
                .checkpoint()
                .map_err(crate::managed_images::error)?
                != previous
            || managed
                .anchor
                .reader()
                .subject_commitments()
                .map_err(crate::managed_images::error)?
                != before
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let (record, record_digest) = self.execution.reply_in(&tx, run, &self.custody).await?;
        self.execution
            .reply_original_scope(&tx, &record, &record_digest)
            .await?;
        self.execution
            .reply_binding_in(&tx, &record, &self.custody)
            .await?;
        let request = CancellationRequest::original(&record, &record_digest)?;
        let key = self.execution.root.pack(&("shared-native-cancel/1", run));
        if let Some(bytes) = tx.get(&key, false).await? {
            let pointer: Pointer =
                serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
            if pointer.request != request
                || before.get(&pointer.subject()) != Some(&hash(&pointer.bytes()?))
            {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            pointer.observe(self, &tx).await?;
            sql.commit().await?;
            return Ok(());
        }
        if tx
            .get(&self.execution.run(run, "cancel"), false)
            .await?
            .is_some()
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let (_, bytes) = self.custody.seal(&request.subject(), &request)?;
        let intent = Intent {
            pointer: Pointer {
                request,
                ciphertext_digest: hash(&bytes),
            },
            ciphertext: STANDARD.encode(bytes),
        };
        let update = intent.pointer.update()?;
        if before.contains_key(&update.subject) {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let prepared = managed
            .anchor
            .prepare_subjects(
                &intent.pointer.effect()?,
                &previous,
                &previous,
                false,
                &[update],
                Some(&intent.seal(self)?),
            )
            .map_err(crate::managed_images::error)?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 94 {
            return Err(RuntimeError::Unavailable);
        }
        intent.apply(self, &tx).await?;
        managed.write_fdb(self, &tx, &prepared).await?;
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 95 {
            return Err(RuntimeError::Unavailable);
        }
        managed.write_sql(&mut sql, &prepared).await?;
        sql.commit().await?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 96 {
            return Err(RuntimeError::Unavailable);
        }
        Box::pin(managed.reconcile(self)).await?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 97 {
            return Err(RuntimeError::Unavailable);
        }
        Ok(())
    }
}
