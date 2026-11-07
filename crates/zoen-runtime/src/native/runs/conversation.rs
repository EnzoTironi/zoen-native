//! Policy observes actual native input without retaining a lease over remote IO.
use super::*;
use std::sync::Arc;

impl LoadedReply {
    pub(super) async fn policy_snapshot(
        &self,
        runtime: &RuntimeAuthority,
        scope: &dyn NativeOutputAuthority,
    ) -> Result<foundationdb::Transaction, RuntimeError> {
        self.check_deadline(runtime).await?;
        runtime.check_reply_continuity().await?;
        scope.check_context(runtime)?;
        let principal = &self.record.original.context.principal;
        scope.check_device(&principal.agent, &principal.device)?;
        let native = runtime
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
        let credential = native.credential(&principal.agent, &principal.device)?;
        runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                &principal.device,
                &credential.certificate,
            )
            .await?
            .active_reply_actor()
            .await?
            .commit()
            .await?;
        let tx = runtime.execution.transaction().await?;
        scope.check_in(&runtime.execution, &tx).await?;
        runtime
            .execution
            .verify_reply_in(
                &tx,
                &self.record,
                &self.record_digest,
                &self.current,
                &self.facts,
                &self.fences,
                &runtime.custody,
            )
            .await?;
        Ok(tx)
    }
}

impl NativeCustody {
    pub(crate) async fn check_reply_input(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
        scope: &dyn NativeOutputAuthority,
        gate: Arc<gate::Gate>,
    ) -> Result<gate::Decision, RuntimeError> {
        let loaded = self.load_reply(runtime, run).await?;
        let observed = async {
            let _snapshot = loaded.policy_snapshot(runtime, scope).await?;
            let principal = &loaded.record.original.context.principal;
            scope.check_device(&principal.agent, &principal.device)?;
            let source = loaded
                .facts
                .messages()
                .last()
                .ok_or(RuntimeError::InvalidBinding)?;
            if source.source() != loaded.record.intent.trigger()
                || source.author() != principal.owner
            {
                return Err(RuntimeError::InvalidBinding);
            }
            Ok(Zeroizing::new(source.text().to_owned()))
        }
        .await;
        let released = runtime
            .execution
            .release_reply(&loaded.record, &loaded.fences)
            .await;
        let text = observed?;
        released?;
        // run_model subsequently reacquires and revalidates the same immutable
        // run/source. An Allow here is neither a model nor financial permit.
        tokio::task::spawn_blocking(move || {
            gate.check(&gate::Check::new(
                gate::Point::ConversationMessage,
                text.as_str(),
            ))
        })
        .await
        .map_err(|_| RuntimeError::Unavailable)
    }
}
