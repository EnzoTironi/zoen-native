//! Publication sequence is an authenticated locator, not a second checkpoint.
use super::*;
impl Execution {
    pub(in crate::native::runs::output) async fn resolve_shared_publication(
        &self,
        tx: &Transaction,
        record: &mut SealedRecord<PublicationRecord>,
        custody: &Custody,
    ) -> Result<(), RuntimeError> {
        let Some(sequence) = record.value.shared_sequence else {
            return Ok(());
        };
        if sequence == 0 || record.value.checkpoint.is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
        let bytes = tx
            .get(&crate::continuity::history(self, sequence), false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if bytes.len() > 32_768 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let prepared: Prepared =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
        if prepared.next.sequence != sequence {
            return Err(RuntimeError::InvalidBinding);
        }
        crate::native::shared::publication_transition(
            custody,
            &prepared,
            &record.value.run,
            &record.bytes,
        )?;
        record.value.checkpoint = Some(prepared.next);
        Ok(())
    }
    pub(crate) async fn shared_publication_digest_in(
        &self,
        runtime: &RuntimeAuthority,
        tx: &Transaction,
        binding: &Binding,
    ) -> Result<Option<String>, RuntimeError> {
        let key = self.run(&binding.context.run_id, "native-publication/1");
        if tx.get(&key, false).await?.is_none() {
            return Ok(None);
        }
        let (record, original) = self
            .reply_in(tx, &binding.context.run_id, &runtime.custody)
            .await?;
        if record.binding != *binding {
            return Err(RuntimeError::InvalidBinding);
        }
        let capture = self
            .output_capture_in(tx, &record, &original, &runtime.custody)
            .await?;
        let publication = self
            .publication_in(
                tx,
                &record,
                &original,
                &capture.association.digest,
                &runtime.custody,
            )
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if publication.value.shared_sequence.is_none() {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let checkpoint = publication
            .value
            .checkpoint
            .as_ref()
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let actual = tx
            .get(
                &crate::continuity::history(self, checkpoint.sequence),
                false,
            )
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let retained = runtime
            .runtime_continuity
            .as_ref()
            .ok_or(RuntimeError::RestoreUnreconciled)?
            .reader()
            .observed_transition(checkpoint)
            .map_err(crate::managed_images::error)?;
        if serde_json::to_vec(&retained)
            .map_err(|_| RuntimeError::InvalidBinding)?
            .as_slice()
            != actual.as_ref()
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(Some(publication.digest))
    }
}
