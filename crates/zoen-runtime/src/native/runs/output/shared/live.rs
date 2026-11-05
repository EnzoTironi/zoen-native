//! Live native proof is borrowed, never reconstructed from retained metadata.
use super::*;
pub(in crate::native) struct LivePublication<'a> {
    loaded: &'a LoadedReply,
    retained: &'a RetainedCapture,
    prepared: &'a PreparedReply,
    capability: &'a dyn NativeOutputAuthority,
}
impl<'a> LivePublication<'a> {
    pub(in super::super) fn new(
        loaded: &'a LoadedReply,
        retained: &'a RetainedCapture,
        prepared: &'a PreparedReply,
        capability: &'a dyn NativeOutputAuthority,
    ) -> Self {
        Self {
            loaded,
            retained,
            prepared,
            capability,
        }
    }
    pub(in crate::native) fn inspection(&self, target: &Root) -> ReplyOutputInspection {
        ReplyOutputInspection {
            client_id: self.prepared.envelope().client_id().into(),
            message: self.prepared.message().into(),
            generation: target.context.generation,
        }
    }
    pub(in crate::native) fn source(
        &self,
        runtime: &RuntimeAuthority,
    ) -> Result<SharedSource, RuntimeError> {
        let frontier = self.prepared.facts().frontier();
        let head = Seen {
            seq: frontier.seq(),
            hash: frontier.hash().into(),
        };
        Ok(SharedSource {
            cell: runtime.execution.reply_source()?.cell().into(),
            space: self.loaded.record.intent.space().into(),
            expected: head.clone(),
            through: head,
        })
    }
    pub(in crate::native) async fn verify(
        &self,
        runtime: &RuntimeAuthority,
        tx: &Transaction,
    ) -> Result<(), RuntimeError> {
        self.capability.check_context(runtime)?;
        self.capability.check_in(&runtime.execution, tx).await?;
        let principal = &self.loaded.record.original.context.principal;
        self.capability
            .check_device(&principal.agent, &principal.device)?;
        runtime
            .execution
            .verify_reply_in(
                tx,
                &self.loaded.record,
                &self.loaded.record_digest,
                &self.loaded.current,
                self.prepared.facts(),
                &self.loaded.fences,
                &runtime.custody,
            )
            .await?;
        let capture = runtime
            .execution
            .output_capture_in(
                tx,
                &self.loaded.record,
                &self.loaded.record_digest,
                &runtime.custody,
            )
            .await?;
        if capture.association.digest != self.retained.association.digest
            || capture.capture.binding != self.retained.capture.binding
            || capture.capture.result != self.retained.capture.result
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
    pub(in crate::native) fn retain(
        &self,
        runtime: &RuntimeAuthority,
        target: &Root,
        namespace: &str,
        sequence: u64,
        before: &BTreeMap<String, String>,
    ) -> Result<SharedPublication, RuntimeError> {
        let loaded = self.loaded;
        let publication = seal_metadata(
            &runtime.custody,
            &format!("native-publication/1/{}", loaded.record.locator.run),
            PublicationRecord {
                checkpoint: None,
                shared_sequence: Some(sequence),
                format: 1,
                run: loaded.record.locator.run.clone(),
                original: loaded.record_digest.clone(),
                binding: loaded.record.binding.digest()?,
                capture: self.retained.association.digest.clone(),
                intent: self.prepared.facts().intent().clone(),
                client_id: self.prepared.envelope().client_id().into(),
                message: self.prepared.message().into(),
                inner: self.prepared.event_digest().into(),
                reference: self.prepared.reference_digest().into(),
                publish: self.prepared.publish_digest().into(),
                root: target.clone(),
            },
        )?;
        let subject = hash(
            &serde_json::to_vec(&(
                "runtime-capture/1",
                &loaded.record.locator.attempt,
                &self.retained.association.value.evidence,
            ))
            .map_err(|_| RuntimeError::InvalidBinding)?,
        );
        let previous = before
            .get(&subject)
            .cloned()
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        Ok(SharedPublication {
            run: loaded.record.locator.run.clone(),
            record: STANDARD.encode(&publication.bytes),
            effect: relay_admission::managed_images::publish_digest(
                namespace,
                self.prepared.envelope(),
            )
            .map_err(crate::managed_images::error)?,
            source: self.source(runtime)?,
            expires: self.prepared.facts().expires_at_ms(),
            update: SubjectUpdate {
                subject,
                next: publication_commitment(&previous, &publication.digest)?,
                previous: Some(previous),
            },
        })
    }
}
