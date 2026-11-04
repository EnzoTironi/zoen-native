//! Exact retained text transport. No provider call or resealing is reachable.
use super::*;
use client_core::{
    relay_output_transport::{AuthorizedPublishError, RelayOutputError},
    runtime_core::PendingReply,
};

fn transport_error(error: RelayOutputError) -> RuntimeError {
    match error {
        RelayOutputError::Unknown => RuntimeError::RestoreUnreconciled,
        RelayOutputError::Invalid => RuntimeError::InvalidBinding,
        RelayOutputError::Refused => RuntimeError::Denied,
        _ => RuntimeError::Unavailable,
    }
}

impl PublicationRecord {
    fn matches_pending(&self, pending: &PendingReply) -> bool {
        self.intent == *pending.facts().intent()
            && self.client_id == pending.envelope().client_id()
            && self.message == pending.message()
            && self.inner == pending.event_digest()
            && self.reference == pending.reference_digest()
            && self.publish == pending.publish_digest()
    }
}

impl NativeCustody {
    pub(crate) async fn deliver_output(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
        target: &ImageDeliveryTarget,
        capability: &dyn NativeOutputAuthority,
    ) -> Result<ReplyReceiptProgress, RuntimeError> {
        runtime.continuity()?;
        capability.check_context(runtime)?;
        capability.check_target(target)?;
        if runtime.execution.reply_source()?.cell() != target.source {
            return Err(RuntimeError::InvalidBinding);
        }
        // First reconcile the actual source, even after grant/device expiry.
        // Historical acceptance cannot authorize any further network effect.
        match self.reconcile_output(runtime, run, capability).await? {
            ReplyReceiptProgress::Waiting => {}
            progress => return Ok(progress),
        }
        let loaded = self.load_reply(runtime, run).await?;
        let result = self
            .deliver_output_fenced(runtime, &loaded, target, capability)
            .await;
        let released = runtime
            .execution
            .release_reply(&loaded.record, &loaded.fences)
            .await;
        result?;
        released?;
        // Genuine ACK is not a receipt; the original ordered source is.
        self.reconcile_output(runtime, run, capability).await
    }

    async fn deliver_output_fenced(
        &self,
        runtime: &RuntimeAuthority,
        loaded: &LoadedReply,
        target: &ImageDeliveryTarget,
        capability: &dyn NativeOutputAuthority,
    ) -> Result<(), RuntimeError> {
        let principal = &loaded.record.original.context.principal;
        capability.check_device(&principal.agent, &principal.device)?;
        let credential = self.credential(&principal.agent, &principal.device)?;
        let capsule = runtime
            .execution
            .read_native(principal, Some(&loaded.fences.device))
            .await?;
        if capsule.root != loaded.current {
            return Err(RuntimeError::Denied);
        }
        let core =
            DeviceCore::restore(self.open_image(principal, capsule)?, credential.unlocked()?)
                .map_err(native_error)?;
        let trx = runtime.execution.transaction().await?;
        capability.check_in(&runtime.execution, &trx).await?;
        let capture = runtime
            .execution
            .output_capture_in(
                &trx,
                &loaded.record,
                &loaded.record_digest,
                &runtime.custody,
            )
            .await?;
        let publication = runtime
            .execution
            .publication_in(
                &trx,
                &loaded.record,
                &loaded.record_digest,
                &capture.association.digest,
                &runtime.custody,
            )
            .await?
            .ok_or(RuntimeError::Denied)?;
        publication.value.checkpoint_context(runtime)?;
        let pending = core
            .pending_reply(
                &loaded.record.intent,
                &publication.value.client_id,
                runtime.finance.reply_clock().await?,
            )
            .map_err(native_error)?;
        if !publication.value.matches_pending(&pending) {
            return Err(RuntimeError::InvalidBinding);
        }
        drop(trx);
        let actor = event_log::Author::certified(
            credential.agent.id.clone(),
            event_log::Signer::from_secret(&credential.secret),
            credential.certificate.clone(),
        );
        let mut transport = Box::pin(runtime.connect_bound_image_output(
            &target.url,
            &target.relay,
            actor,
            pending.envelope(),
            publication.value.checkpoint.as_ref(),
        ))
        .await?;
        let expected = runtime
            .reply_publish_expectation(
                &target.relay,
                pending.envelope(),
                publication.value.checkpoint.as_ref(),
                protocol::runtime_binding::SourceFence {
                    space: loaded.record.intent.space().into(),
                    head: Seen {
                        seq: loaded.facts.frontier().seq(),
                        hash: loaded.facts.frontier().hash().into(),
                    },
                    run: loaded.record.locator.run.clone(),
                    attempt: loaded.record.locator.attempt.clone(),
                    expires_at_ms: loaded.facts.expires_at_ms(),
                },
            )
            .await?;
        match transport
            .publish_authorized(pending.envelope(), expected, || {
                self.authorize_delivery(runtime, loaded, &core, &publication, &capture, capability)
            })
            .await
        {
            Ok(_) => Ok(()),
            Err(AuthorizedPublishError::Transport(error)) => Err(transport_error(error)),
            Err(AuthorizedPublishError::Authorization(error)) => Err(error),
        }
    }

    async fn authorize_delivery(
        &self,
        runtime: &RuntimeAuthority,
        loaded: &LoadedReply,
        core: &DeviceCore,
        publication: &SealedRecord<PublicationRecord>,
        capture: &RetainedCapture,
        capability: &dyn NativeOutputAuthority,
    ) -> Result<(), RuntimeError> {
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 72 {
            runtime.native_cut_entered.notify_one();
            runtime.native_cut_resume.notified().await;
        }
        let principal = &loaded.record.original.context.principal;
        let credential = self.credential(&principal.agent, &principal.device)?;
        // Connecting/Challenge may have waited. Refresh time, directory, source,
        // run cancellation, native root and both leases again before any send.
        let fresh = core
            .pending_reply(
                &loaded.record.intent,
                &publication.value.client_id,
                runtime.finance.reply_clock().await?,
            )
            .map_err(native_error)?;
        if !publication.value.matches_pending(&fresh) {
            return Err(RuntimeError::InvalidBinding);
        }
        runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                &principal.device,
                &credential.certificate,
            )
            .await?
            .commit()
            .await?;
        let trx = runtime.execution.transaction().await?;
        capability.check_in(&runtime.execution, &trx).await?;
        runtime
            .execution
            .verify_reply_in(
                &trx,
                &loaded.record,
                &loaded.record_digest,
                &loaded.current,
                fresh.facts(),
                &loaded.fences,
                &runtime.custody,
            )
            .await?;
        let current = runtime
            .execution
            .publication_in(
                &trx,
                &loaded.record,
                &loaded.record_digest,
                &capture.association.digest,
                &runtime.custody,
            )
            .await?
            .ok_or(RuntimeError::Denied)?;
        if current.digest != publication.digest {
            return Err(RuntimeError::InvalidBinding);
        }
        runtime.continuity()?;
        drop(trx);
        // Directory/FDB work can also wait; observe the SQL clock once more
        // after it, so that work cannot consume the last grant interval.
        core.pending_reply(
            &loaded.record.intent,
            &publication.value.client_id,
            runtime.finance.reply_clock().await?,
        )
        .map_err(native_error)?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 73 {
            runtime.native_cut_entered.notify_one();
            runtime.native_cut_resume.notified().await;
        }
        Ok(())
    }
}
