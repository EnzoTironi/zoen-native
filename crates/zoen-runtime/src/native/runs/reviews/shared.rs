//! Review/decision metadata and the native capsule activate in one shared journal.
use super::*;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct SharedToolDecision {
    run: String,
    index: u8,
    record: String,
    effect: String,
    source: SharedSource,
    expires: Option<i64>,
    update: SubjectUpdate,
}
impl SharedToolDecision {
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
        if self.record.len() > 24 * 1024 {
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
        index: u8,
        bytes: &[u8],
        sequence: u64,
    ) -> Result<(), RuntimeError> {
        if self.run != run || self.index != index || self.bytes()? != bytes || sequence == 0 {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(())
    }
    pub(in crate::native) async fn apply(
        &self,
        runtime: &RuntimeAuthority,
        tx: &Transaction,
        target: &Root,
        core: &mut DeviceCore,
        namespace: &str,
        prepared: &Prepared,
    ) -> Result<(), RuntimeError> {
        let bytes = self.bytes()?;
        let stored: SealedRecord<DecisionRecord> =
            open_metadata(&runtime.custody, &subject(&self.run, self.index), &bytes)?;
        let (record, original) = runtime
            .execution
            .reply_in(tx, &self.run, &runtime.custody)
            .await?;
        let capture = runtime
            .execution
            .output_capture_in(tx, &record, &original, &runtime.custody)
            .await?;
        let (_, blocks) =
            output::search_proposal(&record, &capture)?.ok_or(RuntimeError::InvalidBinding)?;
        stored
            .value
            .validate(&record, &original, &capture, &blocks, self.index)?;
        stored.value.verify_core(&record, core)?;
        if stored.value.root != *target
            || stored.value.sequence != prepared.next.sequence
            || stored.value.source != self.source
            || stored.value.expires != self.expires
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let effect = match stored.value.envelope(&record, core)? {
            Some(wire) => relay_admission::managed_images::publish_digest(namespace, &wire)
                .map_err(crate::managed_images::error)?,
            None => hash(
                &serde_json::to_vec(&("runtime-tool-decision/1", &stored.digest))
                    .map_err(|_| RuntimeError::InvalidBinding)?,
            ),
        };
        if effect != self.effect
            || effect != prepared.effect_digest
            || self.update.subject != capture_subject(&record, &capture)?
            || self.update.next
                != commitment(
                    self.update
                        .previous
                        .as_deref()
                        .ok_or(RuntimeError::RestoreUnreconciled)?,
                    &stored.digest,
                )?
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let key = key(&runtime.execution, &self.run, self.index);
        if let Some(prior) = tx.get(&key, false).await? {
            if prior.as_ref() != bytes.as_slice() {
                return Err(RuntimeError::RestoreUnreconciled);
            }
        } else {
            tx.set(&key, &bytes);
        }
        tx.set(
            &runtime
                .execution
                .native_references(target)
                .pack(&(format!("tool/1/{}", self.index), self.run.as_str())),
            target.manifest_digest.as_bytes(),
        );
        Ok(())
    }
}

fn capture_subject(record: &RunRecord, capture: &RetainedCapture) -> Result<String, RuntimeError> {
    Ok(hash(
        &serde_json::to_vec(&(
            "runtime-capture/1",
            &record.locator.attempt,
            &capture.association.value.evidence,
        ))
        .map_err(|_| RuntimeError::InvalidBinding)?,
    ))
}

pub(in crate::native) struct LiveToolDecision<'a> {
    pub(super) loaded: &'a LoadedReply,
    pub(super) capture: &'a RetainedCapture,
    pub(super) blocks: &'a [OutputBlock],
    pub(super) index: u8,
    pub(super) policy: &'a str,
    pub(super) audit: &'a gate::AuditRecord,
    pub(super) image: &'a client_core::runtime_core::NativeImage,
    pub(super) review: Option<Review>,
    pub(super) capability: &'a dyn NativeOutputAuthority,
}
impl LiveToolDecision<'_> {
    fn core(&self, runtime: &RuntimeAuthority) -> Result<DeviceCore, RuntimeError> {
        let principal = &self.loaded.record.original.context.principal;
        let native = runtime
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
        let image =
            client_core::runtime_core::NativeImage::decode_bounded(self.image.as_bytes().to_vec())
                .map_err(native_error)?;
        DeviceCore::restore(
            image,
            native
                .credential(&principal.agent, &principal.device)?
                .unlocked()?,
        )
        .map_err(native_error)
    }
    pub(in crate::native) fn source(
        &self,
        runtime: &RuntimeAuthority,
    ) -> Result<SharedSource, RuntimeError> {
        let head = Seen {
            seq: self.loaded.facts.frontier().seq(),
            hash: self.loaded.facts.frontier().hash().into(),
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
        let loaded = self.loaded;
        runtime
            .execution
            .verify_reply_in(
                tx,
                &loaded.record,
                &loaded.record_digest,
                &loaded.current,
                &loaded.facts,
                &loaded.fences,
                &runtime.custody,
            )
            .await?;
        let actual = runtime
            .execution
            .output_capture_in(tx, &loaded.record, &loaded.record_digest, &runtime.custody)
            .await?;
        if actual.association.digest != self.capture.association.digest
            || tx
                .get(
                    &key(&runtime.execution, &loaded.record.locator.run, self.index),
                    false,
                )
                .await?
                .is_some()
        {
            return Err(RuntimeError::Denied);
        }
        if let Some(review) = &self.review {
            let core = self.core(runtime)?;
            let wire = core
                .retained_reply_search_review(
                    &loaded.record.intent,
                    &review.request,
                    &review.content_hash,
                    &review.client_id,
                )
                .map_err(native_error)?;
            let now = runtime.finance.reply_clock().await?;
            core.reply_search_review_ready(
                &loaded.record.intent,
                &review.request,
                &review.content_hash,
                &wire,
                now,
            )
            .map_err(native_error)?;
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
    ) -> Result<SharedToolDecision, RuntimeError> {
        let record = &self.loaded.record;
        let review = self.review.as_ref().map(|r| Review {
            request: r.request.clone(),
            content_hash: r.content_hash.clone(),
            client_id: r.client_id.clone(),
            wire: r.wire.clone(),
        });
        let expires = if let Some(review) = &review {
            let status = self
                .core(runtime)?
                .reply_search_review_status(&record.intent, &review.request, &review.content_hash)
                .map_err(native_error)?;
            let expiry = status
                .tool_approval
                .ok_or(RuntimeError::InvalidBinding)?
                .expires_ms;
            Some(
                self.loaded
                    .deadline()
                    .map_or(expiry, |grant| grant.min(expiry)),
            )
        } else {
            self.loaded.deadline()
        };
        let value = DecisionRecord {
            format: 1,
            run: record.locator.run.clone(),
            original: self.loaded.record_digest.clone(),
            capture: self.capture.association.digest.clone(),
            index: self.index,
            policy: self.policy.into(),
            audit: self.audit.clone(),
            review,
            source: self.source(runtime)?,
            root: target.clone(),
            sequence,
            expires,
        };
        value.validate(
            record,
            &self.loaded.record_digest,
            self.capture,
            self.blocks,
            self.index,
        )?;
        let stored = seal_metadata(
            &runtime.custody,
            &subject(&record.locator.run, self.index),
            value,
        )?;
        let effect = match stored.value.envelope(record, &self.core(runtime)?)? {
            Some(wire) => relay_admission::managed_images::publish_digest(namespace, &wire)
                .map_err(crate::managed_images::error)?,
            None => hash(
                &serde_json::to_vec(&("runtime-tool-decision/1", &stored.digest))
                    .map_err(|_| RuntimeError::InvalidBinding)?,
            ),
        };
        let subject = capture_subject(record, self.capture)?;
        let previous = before
            .get(&subject)
            .cloned()
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        Ok(SharedToolDecision {
            run: record.locator.run.clone(),
            index: self.index,
            record: STANDARD.encode(&stored.bytes),
            effect,
            source: stored.value.source,
            expires,
            update: SubjectUpdate {
                subject,
                next: commitment(&previous, &stored.digest)?,
                previous: Some(previous),
            },
        })
    }
}
