//! Review requests use their own retained wire and shared checkpoint.
use super::*;
use client_core::relay_output_transport::{AuthorizedPublishError, RelayOutputError};

impl NativeCustody {
    pub(super) async fn deliver_tool_review(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
        target: &ImageDeliveryTarget,
        capability: &dyn NativeOutputAuthority,
    ) -> Result<(), RuntimeError> {
        capability.check_context(runtime)?;
        capability.check_target(target)?;
        if runtime.execution.reply_source()?.cell() != target.source {
            return Err(RuntimeError::InvalidBinding);
        }
        let loaded = self.load_reply(runtime, run).await?;
        let result = self
            .deliver_review_fenced(runtime, &loaded, target, capability)
            .await;
        let released = runtime
            .execution
            .release_reply(&loaded.record, &loaded.fences)
            .await;
        result?;
        released?;
        let principal = &loaded.record.original.context.principal;
        // ACK is not acceptance. Hydrate the actual ordered source instead.
        self.refresh_source(
            runtime,
            &principal.agent,
            &principal.device,
            loaded.record.intent.space(),
        )
        .await?;
        Ok(())
    }

    async fn deliver_review_fenced(
        &self,
        runtime: &RuntimeAuthority,
        loaded: &LoadedReply,
        target: &ImageDeliveryTarget,
        capability: &dyn NativeOutputAuthority,
    ) -> Result<(), RuntimeError> {
        let tx = loaded.policy_snapshot(runtime, capability).await?;
        let capture = runtime
            .execution
            .output_capture_in(&tx, &loaded.record, &loaded.record_digest, &runtime.custody)
            .await?;
        let (_, blocks) = output::search_proposal(&loaded.record, &capture)?
            .ok_or(RuntimeError::InvalidBinding)?;
        let principal = &loaded.record.original.context.principal;
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
        for index in 0..search_calls(&blocks).len() {
            let index = u8::try_from(index).map_err(|_| RuntimeError::InvalidBinding)?;
            let Some(stored) = runtime
                .execution
                .tool_decision_in(
                    runtime,
                    &tx,
                    ToolOriginal {
                        record: &loaded.record,
                        original: &loaded.record_digest,
                        capture: &capture,
                        blocks: &blocks,
                    },
                    index,
                )
                .await?
            else {
                break;
            };
            let Some(review) = &stored.value.review else {
                continue;
            };
            stored.value.verify_core(&loaded.record, &core)?;
            if core
                .reply_search_review_status(
                    &loaded.record.intent,
                    &review.request,
                    &review.content_hash,
                )
                .map_err(native_error)?
                .status
                != client_core::RequestStatus::Pending
            {
                continue;
            }
            let wire = stored
                .value
                .envelope(&loaded.record, &core)?
                .ok_or(RuntimeError::InvalidBinding)?;
            if let Some(accepted) = core
                .accepted_reply_search_review(
                    &loaded.record.intent,
                    &review.request,
                    &review.content_hash,
                    &review.client_id,
                )
                .map_err(native_error)?
            {
                let position = runtime
                    .execution
                    .reply_source()?
                    .position_in(&tx, loaded.record.intent.space(), accepted.seq)
                    .await
                    .map_err(|_| RuntimeError::Denied)?;
                if position.hash != accepted.hash || position.env != wire {
                    return Err(RuntimeError::InvalidBinding);
                }
                continue;
            }
            let bytes = tx
                .get(
                    &crate::continuity::history(&runtime.execution, stored.value.sequence),
                    false,
                )
                .await?
                .ok_or(RuntimeError::RestoreUnreconciled)?;
            let prepared: Prepared =
                serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
            crate::native::shared::tool_transition(
                &runtime.custody,
                &prepared,
                &loaded.record.locator.run,
                index,
                &stored.bytes,
            )?;
            let actor = event_log::Author::certified(
                credential.agent.id.clone(),
                event_log::Signer::from_secret(&credential.secret),
                credential.certificate.clone(),
            );
            let mut transport = Box::pin(runtime.connect_bound_image_output(
                &target.url,
                &target.relay,
                actor,
                &wire,
                Some(&prepared.next),
            ))
            .await?;
            let expected = runtime
                .reply_publish_expectation(
                    &target.relay,
                    &wire,
                    Some(&prepared.next),
                    protocol::runtime_binding::SourceFence {
                        space: loaded.record.intent.space().into(),
                        head: Seen {
                            seq: loaded.facts.frontier().seq(),
                            hash: loaded.facts.frontier().hash().into(),
                        },
                        run: loaded.record.locator.run.clone(),
                        attempt: loaded.record.locator.attempt.clone(),
                        expires_at_ms: stored.value.expires,
                    },
                )
                .await?;
            transport
                .publish_authorized(&wire, expected, || async {
                    let fresh = loaded.policy_snapshot(runtime, capability).await?;
                    let actual = runtime
                        .execution
                        .tool_decision_in(
                            runtime,
                            &fresh,
                            ToolOriginal {
                                record: &loaded.record,
                                original: &loaded.record_digest,
                                capture: &capture,
                                blocks: &blocks,
                            },
                            index,
                        )
                        .await?
                        .ok_or(RuntimeError::Denied)?;
                    if actual.digest != stored.digest {
                        return Err(RuntimeError::InvalidBinding);
                    }
                    let now = runtime.finance.reply_clock().await?;
                    if stored.value.expires.is_some_and(|expiry| now >= expiry) {
                        return Err(RuntimeError::Denied);
                    }
                    core.reply_search_review_ready(
                        &loaded.record.intent,
                        &review.request,
                        &review.content_hash,
                        &wire,
                        now,
                    )
                    .map_err(native_error)
                })
                .await
                .map_err(|error| match error {
                    AuthorizedPublishError::Authorization(error) => error,
                    AuthorizedPublishError::Transport(RelayOutputError::Unknown) => {
                        RuntimeError::RestoreUnreconciled
                    }
                    AuthorizedPublishError::Transport(RelayOutputError::Invalid) => {
                        RuntimeError::InvalidBinding
                    }
                    AuthorizedPublishError::Transport(RelayOutputError::Refused) => {
                        RuntimeError::Denied
                    }
                    _ => RuntimeError::Unavailable,
                })?;
            break;
        }
        Ok(())
    }
}
