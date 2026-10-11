use roda_proto::{ClientFrame, Sequenced};

use super::*;

#[cfg(test)]
macro_rules! receipt_stage {
    ($stage:literal, $result:expr) => {{
        let started = std::time::Instant::now();
        let result = $result;
        match &result {
            Ok(_) => eprintln!(
                "native receipt stage {}: ok ({} us)",
                $stage,
                started.elapsed().as_micros()
            ),
            Err(error) => eprintln!(
                "native receipt stage {}: error {:?} ({} us)",
                $stage,
                error,
                started.elapsed().as_micros()
            ),
        }
        result
    }};
}

#[cfg(not(test))]
macro_rules! receipt_stage {
    ($stage:literal, $result:expr) => {
        $result
    };
}

pub(super) struct LoadedReceipt {
    pub(super) record: RunRecord,
    pub(super) original: String,
    pub(super) capture: RetainedCapture,
    pub(super) publication: SealedRecord<PublicationRecord>,
    pub(super) receipt: Option<SealedRecord<ReceiptRecord>>,
    pub(super) current: Root,
    pub(super) fences: ReplyFences,
}

// An actual native proof and its exact transaction inputs, never a record read.
pub(super) struct ReceiptActivation<'a> {
    pub(super) loaded: &'a LoadedReceipt,
    pub(super) proof: ReplyReceipt,
    pub(super) source: Seen,
    pub(super) position: Sequenced,
    pub(super) staged: Option<super::super::super::storage::Staged>,
    pub(super) receipt: Option<SealedRecord<ReceiptRecord>>,
}

impl ReceiptActivation<'_> {
    pub(super) fn target(&self) -> &Root {
        self.staged
            .as_ref()
            .map(|s| &s.root)
            .unwrap_or(&self.loaded.current)
    }
    pub(super) fn image(&self) -> &NativeImage {
        match &self.proof {
            ReplyReceipt::Accepted(p) => p.image(),
            ReplyReceipt::Prefix(p) => p.image(),
        }
    }
}

pub(super) fn verify_position(
    publication: &PublicationRecord,
    proof: &ReplyReceipt,
    position: &Sequenced,
) -> Result<(), RuntimeError> {
    if !publication.matches_receipt(proof) {
        return Err(RuntimeError::InvalidBinding);
    }
    let through = match proof {
        ReplyReceipt::Accepted(p) => p.accepted(),
        ReplyReceipt::Prefix(p) => p.frontier(),
    };
    if through.seq != position.seq || through.hash != position.hash {
        return Err(RuntimeError::InvalidBinding);
    }
    match proof {
        ReplyReceipt::Accepted(_)
            if position.env.client_id() == publication.client_id
                && hash(
                    &ClientFrame::Publish {
                        env: position.env.clone(),
                    }
                    .encode(),
                ) == publication.publish =>
        {
            Ok(())
        }
        ReplyReceipt::Prefix(_) if position.env.client_id() != publication.client_id => Ok(()),
        _ => Err(RuntimeError::InvalidBinding),
    }
}

impl NativeCustody {
    async fn load_receipt(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
        capability: &FreshOutputNamespace,
    ) -> Result<LoadedReceipt, RuntimeError> {
        let trx = receipt_stage!("transaction", runtime.execution.transaction().await)?;
        capability.check_in(&runtime.execution, &trx).await?;
        let (record, original) = receipt_stage!(
            "original_record",
            runtime
                .execution
                .reply_in(&trx, run, &runtime.custody)
                .await
        )?;
        let capture = receipt_stage!(
            "retained_capture",
            runtime
                .execution
                .output_capture_in(&trx, &record, &original, &runtime.custody)
                .await
        )?;
        let publication = receipt_stage!(
            "original_publication",
            runtime
                .execution
                .publication_in(
                    &trx,
                    &record,
                    &original,
                    &capture.association.digest,
                    &runtime.custody,
                )
                .await
        )?
        .ok_or(RuntimeError::Denied)?;
        let receipt = receipt_stage!(
            "prior_receipt",
            runtime
                .execution
                .receipt_in(&trx, &record, &publication, &runtime.custody)
                .await
        )?;
        let principal = &record.original.context.principal;
        if self
            .credential(&principal.agent, &principal.device)?
            .principal()
            != *principal
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let current = receipt_stage!(
            "current_native_root",
            runtime.execution.native_root(&trx, principal).await
        )?
        .ok_or(RuntimeError::Denied)?;
        // Historical acquisition omits current grant, directory, cancellation
        // and model closure. Its original/capture/root and real pair remain exact.
        let fences = receipt_stage!(
            "historical_pair_acquisition",
            runtime.execution.acquire_reply_in(&trx, &record).await
        )?
        .ok_or(RuntimeError::Denied)?;
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(LoadedReceipt {
            record,
            original,
            capture,
            publication,
            receipt,
            current,
            fences,
        })
    }

    pub(crate) async fn reconcile_output(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
        capability: &FreshOutputNamespace,
    ) -> Result<ReplyReceiptProgress, RuntimeError> {
        runtime.continuity()?;
        let loaded = receipt_stage!(
            "load_historical_receipt",
            self.load_receipt(runtime, run, capability).await
        )?;
        let result = receipt_stage!(
            "receive_fenced_receipt",
            self.receive_output_fenced(runtime, &loaded, capability)
                .await
        );
        let _ = runtime
            .execution
            .release_reply(&loaded.record, &loaded.fences)
            .await;
        result
    }

    async fn receive_output_fenced(
        &self,
        runtime: &RuntimeAuthority,
        loaded: &LoadedReceipt,
        capability: &FreshOutputNamespace,
    ) -> Result<ReplyReceiptProgress, RuntimeError> {
        let principal = &loaded.record.original.context.principal;
        let credential = self.credential(&principal.agent, &principal.device)?;
        let capsule = receipt_stage!(
            "fenced_native_image",
            runtime
                .execution
                .read_native(principal, Some(&loaded.fences.device))
                .await
        )?;
        if capsule.root != loaded.current {
            return Err(RuntimeError::Denied);
        }
        let mut core =
            DeviceCore::restore(self.open_image(principal, capsule)?, credential.unlocked()?)
                .map_err(native_error)?;
        let trx = receipt_stage!("transaction", runtime.execution.transaction().await)?;
        capability.check_in(&runtime.execution, &trx).await?;
        receipt_stage!(
            "historical_scope",
            runtime
                .execution
                .receipt_scope_in(&trx, loaded, &runtime.custody)
                .await
        )?;
        let source = runtime.execution.reply_source()?;
        if let Some(prior) = &loaded.receipt {
            let proof = ReplyReceipt::Accepted(
                core.receive_reply(
                    &loaded.record.intent,
                    &loaded.publication.value.client_id,
                    vec![],
                )
                .map_err(native_error)?,
            );
            let ReplyReceipt::Accepted(accepted) = &proof else {
                unreachable!();
            };
            let position = receipt_stage!(
                "actual_source_position",
                source
                    .position_in(&trx, loaded.record.intent.space(), accepted.accepted().seq)
                    .await
            )
            .map_err(|_| RuntimeError::Denied)?;
            let head = receipt_stage!(
                "actual_source_head",
                source.head_in(&trx, loaded.record.intent.space()).await
            )
            .map_err(|_| RuntimeError::Denied)?
            .ok_or(RuntimeError::Denied)?;
            verify_position(&loaded.publication.value, &proof, &position)?;
            if head.seq() < accepted.accepted().seq
                || prior.value.accepted != *accepted.accepted()
                || hash(accepted.image().as_bytes()) != loaded.current.context.image_digest
            {
                return Err(RuntimeError::InvalidBinding);
            }
            return Ok(ReplyReceiptProgress::Accepted {
                accepted: accepted.accepted().clone(),
                generation: loaded.current.context.generation,
            });
        }
        let frontier = core
            .retained_head(loaded.record.intent.space())
            .map_err(native_error)?
            .ok_or(RuntimeError::Denied)?;
        let batch = receipt_stage!(
            "actual_source_batch",
            source
                .read_in(
                    &trx,
                    loaded.record.intent.space(),
                    Some((frontier.seq, &frontier.hash)),
                )
                .await
        )
        .map_err(|_| RuntimeError::Denied)?;
        let source_head = Seen {
            seq: batch.head.seq(),
            hash: batch.head.hash().into(),
        };
        let empty = batch.entries.is_empty();
        let proof = match receipt_stage!(
            "native_historical_prefix",
            core.receive_reply_progress(
                &loaded.record.intent,
                &loaded.publication.value.client_id,
                batch.entries,
            )
        ) {
            Ok(proof) => proof,
            Err(NativeError::NotReady) if empty => return Ok(ReplyReceiptProgress::Waiting),
            Err(error) => return Err(native_error(error)),
        };
        let through = match &proof {
            ReplyReceipt::Accepted(p) => p.accepted(),
            ReplyReceipt::Prefix(p) => p.frontier(),
        };
        let position = receipt_stage!(
            "actual_source_position",
            source
                .position_in(&trx, loaded.record.intent.space(), through.seq)
                .await
        )
        .map_err(|_| RuntimeError::Denied)?;
        verify_position(&loaded.publication.value, &proof, &position)?;
        let image = match &proof {
            ReplyReceipt::Accepted(p) => p.image(),
            ReplyReceipt::Prefix(p) => p.image(),
        };
        drop(trx);
        let staged = if hash(image.as_bytes()) == loaded.current.context.image_digest {
            None
        } else {
            let image = self.seal_image(principal, Some(&loaded.current), image)?;
            Some(receipt_stage!(
                "stage_prefix_image",
                runtime
                    .execution
                    .stage_native(
                        principal,
                        Some(loaded.current.clone()),
                        &loaded.fences.device,
                        image,
                    )
                    .await
            )?)
        };
        let target = staged.as_ref().map(|s| &s.root).unwrap_or(&loaded.current);
        let receipt = match &proof {
            ReplyReceipt::Accepted(p) => Some(seal_metadata(
                &runtime.custody,
                &format!("native-receipt/1/{}", loaded.record.locator.run),
                ReceiptRecord {
                    format: 1,
                    original: loaded.original.clone(),
                    capture: loaded.capture.association.digest.clone(),
                    publication: loaded.publication.digest.clone(),
                    reference: p.reference_digest().into(),
                    publish: p.publish_digest().into(),
                    accepted: p.accepted().clone(),
                    source: source_head.clone(),
                    root: target.clone(),
                },
            )?),
            ReplyReceipt::Prefix(_) => None,
        };
        let activation = ReceiptActivation {
            loaded,
            proof,
            source: source_head,
            position,
            staged,
            receipt,
        };
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 47 {
            runtime.native_cut_entered.notify_one();
            runtime.native_cut_resume.notified().await;
        }
        runtime.continuity()?;
        receipt_stage!(
            "activate_receipt",
            runtime
                .execution
                .activate_receipt(capability, activation, runtime)
                .await
        )
    }
}
