//! Retained policy observations are not native dispatch or output authority.
use super::*;
use std::sync::Arc;

pub(crate) struct ReplyHostState {
    pub(crate) space: String,
    pub(crate) attempt: String,
    pub(crate) captured: bool,
    pub(crate) output: Option<ReplyOutputInspection>,
}

/// Constructed only by evaluating the actual retained output with the Gate.
/// No Clone, Deserialize, caller boolean or public constructor exists.
pub(crate) struct ReplyOutputReview {
    original: String,
    capture: String,
    pub(super) audit: gate::AuditRecord,
}

impl ReplyOutputReview {
    pub(crate) fn audit(&self) -> &gate::AuditRecord {
        &self.audit
    }

    pub(super) fn validate(
        &self,
        original: &str,
        retained: &RetainedCapture,
        record: &RunRecord,
    ) -> Result<(), RuntimeError> {
        if self.original != original
            || self.capture != retained.association.digest
            || self.audit.point != gate::Point::Outbound.id()
            || self.audit.outcome != gate::Outcome::Allow.id()
            || self.audit.input_hash != hash(retained.text(record)?.as_bytes())
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
}

pub(crate) enum ReplyOutputCheck {
    Allowed(ReplyOutputReview),
    Held(gate::AuditRecord),
    Unusable,
}

impl NativeCustody {
    pub(crate) async fn reply_host_state(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
        scope: &dyn NativeOutputAuthority,
        expected: (&str, &str),
    ) -> Result<ReplyHostState, RuntimeError> {
        runtime.check_reply_continuity().await?;
        let tx = runtime.execution.transaction().await?;
        scope.check_in(&runtime.execution, &tx).await?;
        let (record, original) = runtime
            .execution
            .reply_in(&tx, run, &runtime.custody)
            .await?;
        let principal = &record.original.context.principal;
        if principal.agent != expected.0 || principal.device != expected.1 {
            return Err(RuntimeError::Denied);
        }
        scope.check_device(&principal.agent, &principal.device)?;
        let captured = tx
            .get(
                &runtime
                    .execution
                    .attempt(&record.locator.attempt, "native-capture/1"),
                false,
            )
            .await?
            .is_some();
        let output = if captured {
            let retained = runtime
                .execution
                .output_capture_in(&tx, &record, &original, &runtime.custody)
                .await?;
            runtime
                .execution
                .publication_in(
                    &tx,
                    &record,
                    &original,
                    &retained.association.digest,
                    &runtime.custody,
                )
                .await?
                .map(|publication| publication.value.inspection())
        } else {
            None
        };
        Ok(ReplyHostState {
            space: record.intent.space().into(),
            attempt: record.locator.attempt,
            captured,
            output,
        })
    }

    pub(crate) async fn review_reply_output(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
        scope: &dyn NativeOutputAuthority,
        gate: Arc<gate::Gate>,
    ) -> Result<ReplyOutputCheck, RuntimeError> {
        let loaded = self.load_reply(runtime, run).await?;
        let observed = async {
            let tx = loaded.policy_snapshot(runtime, scope).await?;
            let retained = runtime
                .execution
                .output_capture_in(&tx, &loaded.record, &loaded.record_digest, &runtime.custody)
                .await?;
            let text = match retained.text(&loaded.record) {
                Ok(text) => Zeroizing::new(text.to_owned()),
                Err(RuntimeError::Denied) => return Ok(None),
                Err(error) => return Err(error),
            };
            Ok(Some((text, retained.association.digest)))
        }
        .await;
        let released = runtime
            .execution
            .release_reply(&loaded.record, &loaded.fences)
            .await;
        let observed = observed?;
        released?;
        let Some((text, capture)) = observed else {
            return Ok(ReplyOutputCheck::Unusable);
        };
        let original = loaded.record_digest;
        // No FDB transaction or native lease survives a decision-provider call.
        let decision = tokio::task::spawn_blocking(move || {
            gate.check(&gate::Check::new(gate::Point::Outbound, text.as_str()))
        })
        .await
        .map_err(|_| RuntimeError::Unavailable)?;
        Ok(if decision.outcome == gate::Outcome::Allow {
            ReplyOutputCheck::Allowed(ReplyOutputReview {
                original,
                capture,
                audit: decision.audit,
            })
        } else {
            ReplyOutputCheck::Held(decision.audit)
        })
    }
}
