use super::*;
use protocol::Sequenced;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Intent {
    run: String,
    publication: String,
    pub(super) previous: Root,
    pub(super) target: Root,
    source: Seen,
    through: Seen,
    receipt: Option<String>,
    pub(super) updates: Vec<SubjectUpdate>,
}
impl Intent {
    pub(super) fn new(activation: &ReceiptActivation<'_>) -> Self {
        Self {
            run: activation.loaded.record.locator.run.clone(),
            publication: activation.loaded.publication.digest.clone(),
            previous: activation.loaded.current.clone(),
            target: activation.target().clone(),
            source: activation.source.clone(),
            through: Seen {
                seq: activation.position.seq,
                hash: activation.position.hash.clone(),
            },
            receipt: activation
                .receipt
                .as_ref()
                .map(|r| STANDARD.encode(&r.bytes)),
            updates: Vec::new(),
        }
    }
    #[cfg(test)]
    pub(super) fn receipt_digest(&self) -> Result<Option<String>, RuntimeError> {
        self.receipt
            .as_ref()
            .map(|encoded| {
                STANDARD
                    .decode(encoded)
                    .map(|bytes| hash(&bytes))
                    .map_err(|_| RuntimeError::InvalidBinding)
            })
            .transpose()
    }
    pub(super) fn effect(&self) -> Result<String, RuntimeError> {
        Ok(hash(
            &serde_json::to_vec(&("runtime-native-receipt/1", self))
                .map_err(|_| RuntimeError::InvalidBinding)?,
        ))
    }
    pub(super) fn seal(&self, runtime: &RuntimeAuthority) -> Result<String, RuntimeError> {
        let (_, bytes) = runtime.custody.seal("runtime-native-receipt/1", self)?;
        let value = format!("{PREFIX}{}", STANDARD.encode(bytes));
        if value.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        Ok(value)
    }
    pub(super) fn decode(
        runtime: &RuntimeAuthority,
        prepared: &Prepared,
    ) -> Result<Self, RuntimeError> {
        let encoded = prepared
            .intent
            .as_deref()
            .and_then(|s| s.strip_prefix(PREFIX))
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if encoded.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| RuntimeError::InvalidBinding)?;
        let value: Self = runtime
            .custody
            .open("runtime-native-receipt/1", None, &bytes)?;
        if prepared.retirement
            || prepared.effect_digest != value.effect()?
            || prepared.subject_updates != value.updates
            || value.updates.is_empty()
            || value.updates.len() > 2
            || value.source.seq < value.through.seq
            || value.previous.context.principal != value.target.context.principal
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(value)
    }
    pub(super) fn validate_updates(
        &self,
        before: &BTreeMap<String, String>,
        after: &BTreeMap<String, String>,
    ) -> Result<(), RuntimeError> {
        let mut expected = before.clone();
        if self.updates.is_empty() || self.updates.len() > 2 {
            return Err(RuntimeError::InvalidBinding);
        }
        for update in &self.updates {
            if update.previous.is_none() || before.get(&update.subject) != update.previous.as_ref()
            {
                return Err(RuntimeError::InvalidBinding);
            }
            expected.insert(update.subject.clone(), update.next.clone());
        }
        if expected != *after {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(())
    }
    pub(super) fn staged(&self) -> Option<crate::native::storage::Staged> {
        (self.target != self.previous).then(|| crate::native::storage::Staged {
            previous: Some(self.previous.clone()),
            root: self.target.clone(),
        })
    }
    pub(super) async fn original(
        &self,
        runtime: &RuntimeAuthority,
        tx: &Transaction,
    ) -> Result<(RunRecord, SealedRecord<PublicationRecord>, Sequenced), RuntimeError> {
        let (record, original) = runtime
            .execution
            .reply_in(tx, &self.run, &runtime.custody)
            .await?;
        let capture = runtime
            .execution
            .output_capture_in(tx, &record, &original, &runtime.custody)
            .await?;
        let publication = runtime
            .execution
            .publication_in(
                tx,
                &record,
                &original,
                &capture.association.digest,
                &runtime.custody,
            )
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if publication.digest != self.publication
            || record.original.context.principal != self.target.context.principal
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let source = runtime.execution.reply_source()?;
        let head = source
            .position_in(tx, record.intent.space(), self.source.seq)
            .await
            .map_err(|_| RuntimeError::Denied)?;
        let position = source
            .position_in(tx, record.intent.space(), self.through.seq)
            .await
            .map_err(|_| RuntimeError::Denied)?;
        if head.hash != self.source.hash || position.hash != self.through.hash {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok((record, publication, position))
    }
    pub(super) fn verify_native(
        &self,
        core: &mut DeviceCore,
        record: &RunRecord,
        publication: &SealedRecord<PublicationRecord>,
        position: &Sequenced,
    ) -> Result<(), RuntimeError> {
        let retained = core
            .retained_reply(&record.intent, &publication.value.client_id)
            .map_err(native_error)?;
        if !publication.value.matches_retained(&retained) {
            return Err(RuntimeError::InvalidBinding);
        }
        if self.receipt.is_some() {
            let proof = ReplyReceipt::Accepted(
                core.receive_reply(&record.intent, &publication.value.client_id, vec![])
                    .map_err(native_error)?,
            );
            super::super::super::receipt::verify_position(&publication.value, &proof, position)?;
            let ReplyReceipt::Accepted(accepted) = proof else {
                return Err(RuntimeError::InvalidBinding);
            };
            if hash(accepted.image().as_bytes()) != self.target.context.image_digest {
                return Err(RuntimeError::InvalidBinding);
            }
        } else if position.env.client_id() == publication.value.client_id
            || core
                .retained_head(record.intent.space())
                .map_err(native_error)?
                .as_ref()
                != Some(&self.through)
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
    pub(super) async fn apply_record(
        &self,
        runtime: &RuntimeAuthority,
        tx: &Transaction,
        publication: &SealedRecord<PublicationRecord>,
    ) -> Result<(), RuntimeError> {
        if let Some(encoded) = &self.receipt {
            let bytes = STANDARD
                .decode(encoded)
                .map_err(|_| RuntimeError::InvalidBinding)?;
            let receipt: SealedRecord<ReceiptRecord> = open_metadata(
                &runtime.custody,
                &format!("native-receipt/1/{}", self.run),
                &bytes,
            )?;
            receipt.value.validate(publication)?;
            if receipt.value.root != self.target
                || receipt.value.accepted != self.through
                || receipt.value.source != self.source
            {
                return Err(RuntimeError::InvalidBinding);
            }
            let key = runtime.execution.run(&self.run, "native-receipt/1");
            if tx
                .get(&key, false)
                .await?
                .as_deref()
                .is_some_and(|v| v != bytes.as_slice())
            {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            tx.set(&key, &bytes);
        }
        Ok(())
    }
}
