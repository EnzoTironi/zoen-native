//! Exact original publication bytes and a locator into existing checkpoint history.
mod live;
mod storage;
use super::*;
use crate::native::shared::SharedSource;
use base64::{engine::general_purpose::STANDARD, Engine as _};
pub(in crate::native) use live::LivePublication;
use relay_admission::managed_images::{Prepared, SubjectUpdate};
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct SharedPublication {
    run: String,
    record: String,
    effect: String,
    source: SharedSource,
    expires: Option<i64>,
    update: SubjectUpdate,
}
impl SharedPublication {
    pub(in crate::native) fn source(&self) -> &SharedSource {
        &self.source
    }
    pub(in crate::native) fn expires(&self) -> Option<i64> {
        self.expires
    }
    pub(in crate::native) fn update(&self) -> &SubjectUpdate {
        &self.update
    }
    pub(in crate::native) fn effect(&self) -> Result<String, RuntimeError> {
        if !digest(&self.effect) {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(self.effect.clone())
    }
    fn bytes(&self) -> Result<Vec<u8>, RuntimeError> {
        if self.record.len() > (METADATA_BYTES + 40) * 4 / 3 + 4 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        STANDARD
            .decode(&self.record)
            .map_err(|_| RuntimeError::InvalidBinding)
    }
    pub(in crate::native) fn digest(&self) -> Result<String, RuntimeError> {
        Ok(hash(&self.bytes()?))
    }
    pub(in crate::native) fn matches_original(
        &self,
        run: &str,
        bytes: &[u8],
        sequence: u64,
    ) -> Result<(), RuntimeError> {
        if self.run != run || self.bytes()? != bytes || sequence == 0 {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
    pub(in crate::native) fn retained(
        &self,
        custody: &Custody,
        core: &DeviceCore,
    ) -> Result<client_core::runtime_core::RetainedReply, RuntimeError> {
        let bytes = self.bytes()?;
        let p: SealedRecord<PublicationRecord> = open_metadata(
            custody,
            &format!("native-publication/1/{}", self.run),
            &bytes,
        )?;
        core.retained_reply(&p.value.intent, &p.value.client_id)
            .map_err(native_error)
    }
    pub(in crate::native) async fn apply(
        &self,
        runtime: &RuntimeAuthority,
        tx: &Transaction,
        target: &Root,
        retained: client_core::runtime_core::RetainedReply,
        namespace: &str,
        prepared: &Prepared,
    ) -> Result<(), RuntimeError> {
        let bytes = self.bytes()?;
        let p: SealedRecord<PublicationRecord> = open_metadata(
            &runtime.custody,
            &format!("native-publication/1/{}", self.run),
            &bytes,
        )?;
        let (record, original) = runtime
            .execution
            .reply_in(tx, &self.run, &runtime.custody)
            .await?;
        let capture = runtime
            .execution
            .output_capture_in(tx, &record, &original, &runtime.custody)
            .await?;
        p.value
            .validate(&record, &original, &capture.association.digest)?;
        if p.value.root != *target
            || p.value.checkpoint.is_some()
            || p.value.shared_sequence != Some(prepared.next.sequence)
        {
            return Err(RuntimeError::InvalidBinding);
        }
        if !p.value.matches_retained(&retained)
            || relay_admission::managed_images::publish_digest(namespace, retained.envelope())
                .map_err(crate::managed_images::error)?
                != self.effect
            || prepared.effect_digest != self.effect
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let subject = hash(
            &serde_json::to_vec(&(
                "runtime-capture/1",
                &record.locator.attempt,
                &capture.association.value.evidence,
            ))
            .map_err(|_| RuntimeError::InvalidBinding)?,
        );
        if self.update.subject != subject
            || self.update.previous.is_none()
            || self.update.next
                != publication_commitment(
                    self.update
                        .previous
                        .as_deref()
                        .ok_or(RuntimeError::InvalidBinding)?,
                    &hash(&bytes),
                )?
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let key = runtime.execution.run(&self.run, "native-publication/1");
        if let Some(prior) = tx.get(&key, false).await? {
            if prior.as_ref() != bytes.as_slice() {
                return Err(RuntimeError::RestoreUnreconciled);
            }
        } else {
            tx.set(&key, &bytes);
        }
        let reference = runtime
            .execution
            .native_references(target)
            .pack(&("publication/1", self.run.as_str()));
        if tx
            .get(&reference, false)
            .await?
            .as_deref()
            .is_some_and(|prior| prior != target.manifest_digest.as_bytes())
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        tx.set(&reference, target.manifest_digest.as_bytes());
        Ok(())
    }
}
pub(crate) fn publication_commitment(
    previous: &str,
    publication: &str,
) -> Result<String, RuntimeError> {
    Ok(hash(
        &serde_json::to_vec(&("runtime-capture-publication/1", previous, publication))
            .map_err(|_| RuntimeError::InvalidBinding)?,
    ))
}
