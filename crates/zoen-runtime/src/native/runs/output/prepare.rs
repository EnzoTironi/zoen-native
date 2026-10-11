use super::*;

pub(super) struct PreparedOutput<'a> {
    pub(super) loaded: &'a LoadedReply,
    pub(super) retained: RetainedCapture,
    pub(super) prepared: PreparedReply,
    pub(super) publication: SealedRecord<PublicationRecord>,
    pub(super) staged: super::super::super::storage::Staged,
}

impl PreparedOutput<'_> {
    pub(super) fn scope(&self, execution: &Execution) -> Result<ReplyOutputScope, RuntimeError> {
        let facts = self.prepared.facts();
        let mut image = self.staged.scope(execution, &self.loaded.fences.device);
        image.effects = self.publication.digest.clone();
        Ok(ReplyOutputScope {
            image,
            source: self.loaded.record.source.clone(),
            run: self.loaded.record.locator.run.clone(),
            original: self.loaded.record_digest.clone(),
            binding: self.loaded.record.binding.digest()?,
            capture: self.retained.association.digest.clone(),
            run_fence: self.loaded.fences.run.clone(),
            intent: facts.intent().clone(),
            frontier: Seen {
                seq: facts.frontier().seq(),
                hash: facts.frontier().hash().into(),
            },
            epoch: facts.frontier().epoch(),
            expires_at_ms: facts.expires_at_ms(),
            publication: self.publication.digest.clone(),
        })
    }
}

impl NativeCustody {
    pub(crate) async fn prepare_output(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
        capability: &FreshOutputNamespace,
    ) -> Result<ReplyOutputInspection, RuntimeError> {
        runtime.continuity()?;
        let trx = runtime.execution.transaction().await?;
        capability.check_in(&runtime.execution, &trx).await?;
        let (record, original) = runtime
            .execution
            .reply_in(&trx, run, &runtime.custody)
            .await?;
        let retained = runtime
            .execution
            .output_capture_in(&trx, &record, &original, &runtime.custody)
            .await?;
        if let Some(publication) = runtime
            .execution
            .publication_in(
                &trx,
                &record,
                &original,
                &retained.association.digest,
                &runtime.custody,
            )
            .await?
        {
            // Observation after an uncertain ACK does not sign or reseal again.
            return Ok(publication.value.inspection());
        }
        retained.text(&record)?;
        drop(trx);
        let loaded = self.load_reply(runtime, run).await?;
        if loaded.record_digest != original {
            let _ = runtime
                .execution
                .release_reply(&loaded.record, &loaded.fences)
                .await;
            return Err(RuntimeError::InvalidBinding);
        }
        let result = self
            .prepare_output_fenced(runtime, &loaded, retained, capability)
            .await;
        let _ = runtime
            .execution
            .release_reply(&loaded.record, &loaded.fences)
            .await;
        result
    }

    async fn prepare_output_fenced(
        &self,
        runtime: &RuntimeAuthority,
        loaded: &LoadedReply,
        retained: RetainedCapture,
        capability: &FreshOutputNamespace,
    ) -> Result<ReplyOutputInspection, RuntimeError> {
        let principal = &loaded.record.original.context.principal;
        let credential = self.credential(&principal.agent, &principal.device)?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 54 {
            self.repack_fenced(runtime, credential, principal, &loaded.fences.device)
                .await?;
        }
        let capsule = runtime
            .execution
            .read_native(principal, Some(&loaded.fences.device))
            .await?;
        if capsule.root != loaded.current {
            return Err(RuntimeError::Denied);
        }
        let mut core =
            DeviceCore::restore(self.open_image(principal, capsule)?, credential.unlocked()?)
                .map_err(native_error)?;
        let at = runtime.finance.reply_clock().await?;
        let prepared = core
            .prepare_reply(&loaded.record.intent, retained.text(&loaded.record)?, at)
            .map_err(native_error)?;
        #[cfg(test)]
        capability
            .observed
            .lock()
            .map_err(|_| RuntimeError::Unavailable)?
            .insert(
                prepared.publish_digest().into(),
                prepared.envelope().clone(),
            );
        if prepared.facts().frontier() != loaded.facts.frontier() {
            return Err(RuntimeError::Denied);
        }
        let image = self.seal_image(principal, Some(&loaded.current), prepared.image())?;
        let staged = runtime
            .execution
            .stage_native(
                principal,
                Some(loaded.current.clone()),
                &loaded.fences.device,
                image,
            )
            .await?;
        let publication = seal_metadata(
            &runtime.custody,
            &format!("native-publication/1/{}", loaded.record.locator.run),
            PublicationRecord {
                format: 1,
                run: loaded.record.locator.run.clone(),
                original: loaded.record_digest.clone(),
                binding: loaded.record.binding.digest()?,
                capture: retained.association.digest.clone(),
                intent: prepared.facts().intent().clone(),
                client_id: prepared.envelope().client_id().into(),
                message: prepared.message().into(),
                inner: prepared.event_digest().into(),
                reference: prepared.reference_digest().into(),
                publish: prepared.publish_digest().into(),
                root: staged.root.clone(),
            },
        )?;
        let output = PreparedOutput {
            loaded,
            retained,
            prepared,
            publication,
            staged,
        };
        let scope = output.scope(&runtime.execution)?;
        let guard = runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                &principal.device,
                &credential.certificate,
            )
            .await?;
        #[cfg(test)]
        let guard = match runtime.fault.load(std::sync::atomic::Ordering::SeqCst) {
            45 => guard.suppress_known_ack(),
            46 => guard.delay_known_ack(),
            50 => guard
                .delay_output_expiry(std::sync::Arc::clone(&capability.output_expiry_remaining)),
            _ => guard,
        };
        let admission = guard.authorize_output(scope).await?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 41 {
            runtime.native_cut_entered.notify_one();
            runtime.native_cut_resume.notified().await;
        }
        runtime.continuity()?;
        runtime
            .execution
            .activate_output(capability, output, admission, runtime)
            .await
    }
}
