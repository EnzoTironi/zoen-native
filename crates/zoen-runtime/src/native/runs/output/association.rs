//! Historical correspondence only. These bytes cannot grant publication rights.
#![deny(clippy::pedantic)]
use super::{
    digest, id, open_metadata, seal_metadata, Binding, Custody, Execution, NativeCaptureRecord,
    RunRecord, RuntimeError, SealedRecord, Transaction,
};

impl Execution {
    async fn capture_record_in(
        &self,
        tx: &Transaction,
        binding: &Binding,
        reference: (&str, &str),
        custody: &Custody,
    ) -> Result<(RunRecord, String), RuntimeError> {
        let (evidence, nonce) = reference;
        if !digest(evidence) || !id(nonce) {
            return Err(RuntimeError::InvalidBinding);
        }
        let (record, original) = self.reply_in(tx, &binding.context.run_id, custody).await?;
        if record.binding != *binding {
            return Err(RuntimeError::InvalidBinding);
        }
        self.reply_original_scope(tx, &record, &original).await?;
        self.reply_binding_in(tx, &record, custody).await?;
        if tx
            .get(&self.attempt(&record.locator.attempt, "admitted"), false)
            .await?
            .as_deref()
            != Some(nonce.as_bytes())
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok((record, original))
    }

    pub(crate) async fn retained_capture_association_in(
        &self,
        tx: &Transaction,
        binding: &Binding,
        reference: (&str, &str),
        custody: &Custody,
    ) -> Result<Option<Vec<u8>>, RuntimeError> {
        let Some(bytes) = tx
            .get(
                &self.attempt(&binding.context.attempt_id, "native-capture/1"),
                false,
            )
            .await?
        else {
            return Ok(None);
        };
        self.validate_capture_association_in(tx, binding, reference, &bytes, custody)
            .await?;
        Ok(Some(bytes.to_vec()))
    }

    pub(crate) async fn prepare_capture_association_in(
        &self,
        tx: &Transaction,
        binding: &Binding,
        reference: (&str, &str),
        custody: &Custody,
    ) -> Result<Vec<u8>, RuntimeError> {
        if let Some(bytes) = self
            .retained_capture_association_in(tx, binding, reference, custody)
            .await?
        {
            return Ok(bytes);
        }
        let (record, original) = self
            .capture_record_in(tx, binding, reference, custody)
            .await?;
        let association = seal_metadata(
            custody,
            &format!("native-capture/1/{}", record.locator.attempt),
            NativeCaptureRecord {
                format: 1,
                source: record.source,
                run: record.locator.run,
                attempt: record.locator.attempt,
                original,
                binding: binding.digest()?,
                original_capsule: record.original.manifest_digest,
                admitted_nonce: reference.1.into(),
                evidence: reference.0.into(),
            },
        )?;
        Ok(association.bytes)
    }

    pub(crate) async fn validate_capture_association_in(
        &self,
        tx: &Transaction,
        binding: &Binding,
        reference: (&str, &str),
        bytes: &[u8],
        custody: &Custody,
    ) -> Result<(), RuntimeError> {
        let (record, original) = self
            .capture_record_in(tx, binding, reference, custody)
            .await?;
        let association: SealedRecord<NativeCaptureRecord> = open_metadata(
            custody,
            &format!("native-capture/1/{}", record.locator.attempt),
            bytes,
        )?;
        association.value.validate(&record, &original)?;
        if association.value.evidence != reference.0
            || association.value.admitted_nonce != reference.1
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
}
