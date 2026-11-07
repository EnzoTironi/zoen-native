//! Consent remains a constraint on every descendant model call and publication.
use super::*;

fn earlier(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}
impl Execution {
    pub(in crate::native::runs) async fn approved_tools_in(
        &self,
        runtime: &RuntimeAuthority,
        tx: &Transaction,
        record: &RunRecord,
        core: &mut DeviceCore,
        required: bool,
        policy: Option<&str>,
    ) -> Result<(bool, Option<i64>), RuntimeError> {
        let present = tx
            .get(&key(self, &record.locator.run, 0), false)
            .await?
            .is_some();
        if !present {
            return if required {
                Err(RuntimeError::Denied)
            } else {
                Ok((false, None))
            };
        }
        let (_, original) = self
            .reply_in(tx, &record.locator.run, &runtime.custody)
            .await?;
        let capture = self
            .output_capture_in(tx, record, &original, &runtime.custody)
            .await?;
        let (_, blocks) =
            output::search_proposal(record, &capture)?.ok_or(RuntimeError::InvalidBinding)?;
        let now = runtime.finance.reply_clock().await?;
        let mut expires = None;
        for (index, call) in search_calls(&blocks).iter().enumerate() {
            let index = u8::try_from(index).map_err(|_| RuntimeError::InvalidBinding)?;
            let stored = self
                .tool_decision_in(
                    runtime,
                    tx,
                    ToolOriginal {
                        record,
                        original: &original,
                        capture: &capture,
                        blocks: &blocks,
                    },
                    index,
                )
                .await?
                .ok_or(RuntimeError::Denied)?;
            if policy.is_some_and(|policy| policy != stored.value.policy)
                || stored.value.audit.outcome == gate::Outcome::Block.id()
                || stored.value.expires.is_some_and(|expiry| now >= expiry)
            {
                return Err(RuntimeError::Denied);
            }
            stored.value.verify_core(record, core)?;
            if let Some(review) = &stored.value.review {
                core.reply_search_review_approved(
                    &record.intent,
                    &review.request,
                    &review.content_hash,
                    call.arguments,
                    now,
                )
                .map_err(native_error)?;
            }
            expires = earlier(expires, stored.value.expires);
        }
        Ok((true, expires))
    }

    pub(in crate::native::runs) async fn tool_ancestor_deadline_in(
        &self,
        runtime: &RuntimeAuthority,
        tx: &Transaction,
        record: &RunRecord,
        core: &mut DeviceCore,
        policy: Option<&str>,
    ) -> Result<Option<i64>, RuntimeError> {
        let mut next = record
            .continuation
            .as_ref()
            .map(|step| (step.parent.clone(), step.gated));
        let mut expires = None;
        for _ in 0..3 {
            let Some((run, gated)) = next.take() else {
                return Ok(expires);
            };
            let (parent, _) = self.reply_in(tx, &run, &runtime.custody).await?;
            if gated {
                expires = earlier(
                    expires,
                    self.approved_tools_in(runtime, tx, &parent, core, true, policy)
                        .await?
                        .1,
                );
            }
            next = parent
                .continuation
                .as_ref()
                .map(|step| (step.parent.clone(), step.gated));
        }
        if next.is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(expires)
    }
}
impl NativeCustody {
    pub(crate) async fn verify_reply_tool_policy(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
        policy: &str,
    ) -> Result<(), RuntimeError> {
        let loaded = self.load_reply(runtime, run).await?;
        let result = async {
            let principal = &loaded.record.original.context.principal;
            let capsule = runtime
                .execution
                .read_native(principal, Some(&loaded.fences.device))
                .await?;
            if capsule.root != loaded.current {
                return Err(RuntimeError::Denied);
            }
            let mut core = DeviceCore::restore(
                self.open_image(principal, capsule)?,
                self.credential(&principal.agent, &principal.device)?
                    .unlocked()?,
            )
            .map_err(native_error)?;
            let tx = runtime.execution.transaction().await?;
            runtime
                .execution
                .tool_ancestor_deadline_in(runtime, &tx, &loaded.record, &mut core, Some(policy))
                .await?;
            Ok(())
        }
        .await;
        let released = runtime
            .execution
            .release_reply(&loaded.record, &loaded.fences)
            .await;
        result?;
        released
    }
}
