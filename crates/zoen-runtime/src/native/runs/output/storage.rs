use super::*;

impl Execution {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn associate_native_capture(
        &self,
        capability: &dyn NativeOutputAuthority,
        loaded: &LoadedReply,
        capture: &Capture,
        nonce: &str,
        evidence: &str,
        sealed: &[u8],
        custody: &Custody,
    ) -> Result<(), RuntimeError> {
        let record = &loaded.record;
        if capture.binding != record.binding
            || capture.result.context != record.binding.context
            || capture.result.request_digest != record.binding.request_digest
            || !id(nonce)
            || !digest(evidence)
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let decoded =
            crate::capture_codec::open(custody, &record.locator.attempt, evidence, sealed)?;
        if decoded.binding != capture.binding
            || decoded.result != capture.result
            || crate::capture_codec::digest(capture)? != evidence
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let payload = self
            .evidence_payload(&record.locator.attempt, evidence, sealed)
            .await?;
        let trx = self.transaction().await?;
        capability.check_in(self, &trx).await?;
        let principal = &loaded.record.original.context.principal;
        capability.check_device(&principal.agent, &principal.device)?;
        self.reply_original_scope(&trx, record, &loaded.record_digest)
            .await?;
        self.reply_binding_in(&trx, record, custody).await?;
        if trx
            .get(&self.attempt(&record.locator.attempt, "admitted"), false)
            .await?
            .as_deref()
            != Some(nonce.as_bytes())
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let key = self.attempt(&record.locator.attempt, "native-capture/1");
        if let Some(prior) = trx.get(&key, false).await? {
            let prior: SealedRecord<NativeCaptureRecord> = open_metadata(
                custody,
                &format!("native-capture/1/{}", record.locator.attempt),
                &prior,
            )?;
            prior.value.validate(record, &loaded.record_digest)?;
            if prior.value.evidence != evidence || prior.value.admitted_nonce != nonce {
                return Err(RuntimeError::InvalidBinding);
            }
        } else {
            let association = seal_metadata(
                custody,
                &format!("native-capture/1/{}", record.locator.attempt),
                NativeCaptureRecord {
                    format: 1,
                    source: record.source.clone(),
                    run: record.locator.run.clone(),
                    attempt: record.locator.attempt.clone(),
                    original: loaded.record_digest.clone(),
                    binding: record.binding.digest()?,
                    original_capsule: record.original.manifest_digest.clone(),
                    admitted_nonce: nonce.into(),
                    evidence: evidence.into(),
                },
            )?;
            trx.set(&key, &association.bytes);
        }
        let key = self
            .root
            .pack(&("evidence", record.locator.attempt.as_str(), evidence));
        if let Some(prior) = trx.get(&key, false).await? {
            let original = self
                .read_evidence_payload_in(&trx, &record.locator.attempt, evidence, &prior)
                .await?;
            let prior =
                crate::capture_codec::open(custody, &record.locator.attempt, evidence, &original)?;
            if prior.binding != capture.binding || prior.result != capture.result {
                return Err(RuntimeError::InvalidBinding);
            }
        } else {
            trx.set(&key, &payload);
            trx.set(
                &self
                    .root
                    .pack(&("transfer", record.locator.attempt.as_str(), evidence)),
                &payload,
            );
        }
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(())
    }

    pub(in crate::native::runs) async fn output_capture_in(
        &self,
        trx: &Transaction,
        record: &RunRecord,
        original: &str,
        custody: &Custody,
    ) -> Result<RetainedCapture, RuntimeError> {
        self.reply_original_scope(trx, record, original).await?;
        self.reply_binding_in(trx, record, custody).await?;
        let bytes = trx
            .get(
                &self.attempt(&record.locator.attempt, "native-capture/1"),
                false,
            )
            .await?
            .ok_or(RuntimeError::Denied)?;
        let association: SealedRecord<NativeCaptureRecord> = open_metadata(
            custody,
            &format!("native-capture/1/{}", record.locator.attempt),
            &bytes,
        )?;
        association.value.validate(record, original)?;
        if trx
            .get(&self.attempt(&record.locator.attempt, "admitted"), false)
            .await?
            .as_deref()
            != Some(association.value.admitted_nonce.as_bytes())
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let bytes = trx
            .get(
                &self.root.pack(&(
                    "evidence",
                    record.locator.attempt.as_str(),
                    association.value.evidence.as_str(),
                )),
                false,
            )
            .await?
            .ok_or(RuntimeError::Denied)?;
        let sealed = self
            .read_evidence_payload_in(
                trx,
                &record.locator.attempt,
                &association.value.evidence,
                &bytes,
            )
            .await?;
        let capture = crate::capture_codec::open(
            custody,
            &record.locator.attempt,
            &association.value.evidence,
            &sealed,
        )?;
        if capture.binding != record.binding
            || capture.result.context != record.binding.context
            || capture.result.request_digest != record.binding.request_digest
            || crate::capture_codec::digest(&capture)? != association.value.evidence
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(RetainedCapture {
            association,
            capture,
        })
    }

    pub(super) async fn publication_in(
        &self,
        trx: &Transaction,
        record: &RunRecord,
        original: &str,
        capture: &str,
        custody: &Custody,
    ) -> Result<Option<SealedRecord<PublicationRecord>>, RuntimeError> {
        let Some(bytes) = trx
            .get(
                &self.run(&record.locator.run, "native-publication/1"),
                false,
            )
            .await?
        else {
            return Ok(None);
        };
        let mut publication: SealedRecord<PublicationRecord> = open_metadata(
            custody,
            &format!("native-publication/1/{}", record.locator.run),
            &bytes,
        )?;
        publication.value.validate(record, original, capture)?;
        let reference = self
            .native_references(&publication.value.root)
            .pack(&("publication/1", record.locator.run.as_str()));
        if trx.get(&reference, false).await?.as_deref()
            != Some(publication.value.root.manifest_digest.as_bytes())
        {
            return Err(RuntimeError::InvalidBinding);
        }
        self.resolve_shared_publication(trx, &mut publication, custody)
            .await?;
        Ok(Some(publication))
    }

    pub(super) async fn receipt_in(
        &self,
        trx: &Transaction,
        record: &RunRecord,
        publication: &SealedRecord<PublicationRecord>,
        custody: &Custody,
    ) -> Result<Option<SealedRecord<ReceiptRecord>>, RuntimeError> {
        let Some(bytes) = trx
            .get(&self.run(&record.locator.run, "native-receipt/1"), false)
            .await?
        else {
            return Ok(None);
        };
        let receipt: SealedRecord<ReceiptRecord> = open_metadata(
            custody,
            &format!("native-receipt/1/{}", record.locator.run),
            &bytes,
        )?;
        receipt.value.validate(publication)?;
        Ok(Some(receipt))
    }

    pub(super) async fn activate_output(
        &self,
        capability: &dyn NativeOutputAuthority,
        output: prepare::PreparedOutput<'_>,
        admission: crate::finance::ReplyOutputAdmission,
        checkpoint: Option<&crate::managed_images::PublishCheckpoint>,
        runtime: &RuntimeAuthority,
    ) -> Result<ReplyOutputInspection, RuntimeError> {
        let scope = output.scope(self)?;
        admission.check(&scope)?;
        let trx = self.transaction().await?;
        capability.check_in(self, &trx).await?;
        let loaded = output.loaded;
        self.verify_reply_in(
            &trx,
            &loaded.record,
            &loaded.record_digest,
            &loaded.current,
            output.prepared.facts(),
            &loaded.fences,
            &runtime.custody,
        )
        .await?;
        let retained = self
            .output_capture_in(
                &trx,
                &loaded.record,
                &loaded.record_digest,
                &runtime.custody,
            )
            .await?;
        if retained.association.digest != output.retained.association.digest
            || retained.capture.binding != output.retained.capture.binding
            || retained.capture.result != output.retained.capture.result
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let p = &output.publication.value;
        if p.checkpoint.as_ref() != checkpoint.map(|value| &value.prepared().next) {
            return Err(RuntimeError::InvalidBinding);
        }
        p.validate(
            &loaded.record,
            &loaded.record_digest,
            &retained.association.digest,
        )?;
        if p.root != output.staged.root
            || p.intent != *output.prepared.facts().intent()
            || p.client_id != output.prepared.envelope().client_id()
            || p.message != output.prepared.message()
            || p.inner != output.prepared.event_digest()
            || p.reference != output.prepared.reference_digest()
            || p.publish != output.prepared.publish_digest()
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let key = self.run(&loaded.record.locator.run, "native-publication/1");
        if trx.get(&key, false).await?.is_some()
            || trx
                .get(
                    &self.run(&loaded.record.locator.run, "native-receipt/1"),
                    false,
                )
                .await?
                .is_some()
        {
            return Err(RuntimeError::Denied);
        }
        self.native_activation(&trx, &output.staged, &loaded.fences.device)
            .await?;
        let reference = self
            .native_references(&p.root)
            .pack(&("publication/1", loaded.record.locator.run.as_str()));
        if trx.get(&reference, false).await?.is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
        trx.set(&key, &output.publication.bytes);
        trx.set(&reference, p.root.manifest_digest.as_bytes());
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 42 {
            return Err(RuntimeError::Unavailable);
        }
        if let Some(checkpoint) = checkpoint {
            checkpoint.write_fdb(self, &trx).await?;
            runtime.clear_image_activation(&trx, checkpoint);
        }
        admission.consume(&scope)?;
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 43 {
            return Err(RuntimeError::Unavailable);
        }
        Ok(p.inspection())
    }

    pub(super) async fn receipt_scope_in(
        &self,
        trx: &Transaction,
        loaded: &receipt::LoadedReceipt,
        custody: &Custody,
    ) -> Result<(), RuntimeError> {
        let record = &loaded.record;
        let capture = self
            .output_capture_in(trx, record, &loaded.original, custody)
            .await?;
        let publication = self
            .publication_in(
                trx,
                record,
                &loaded.original,
                &capture.association.digest,
                custody,
            )
            .await?
            .ok_or(RuntimeError::Denied)?;
        let prior = self.receipt_in(trx, record, &publication, custody).await?;
        if capture.association.digest != loaded.capture.association.digest
            || publication.digest != loaded.publication.digest
            || prior.as_ref().map(|r| &r.digest) != loaded.receipt.as_ref().map(|r| &r.digest)
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let version = trx.get_read_version().await?;
        Self::lease(
            trx,
            &self.run(&record.locator.run, "lease"),
            &loaded.fences.run,
            version,
        )
        .await?;
        let principal = &record.original.context.principal;
        Self::lease(
            trx,
            &self.native_lease(principal),
            &loaded.fences.device,
            version,
        )
        .await?;
        if self.native_root(trx, principal).await?.as_ref() != Some(&loaded.current) {
            return Err(RuntimeError::Denied);
        }
        Ok(())
    }
}
