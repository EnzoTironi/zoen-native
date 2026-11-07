//! The existing custody/source/actor proofs guard every bounded history read.
use super::*;

impl NativeCustody {
    pub(crate) async fn search_reply_history(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
        arguments: &serde_json::Value,
        scope: &dyn NativeOutputAuthority,
    ) -> Result<client_core::MessageSearchPage, RuntimeError> {
        let loaded = self.load_reply(runtime, run).await?;
        let result = self.search_loaded(runtime, &loaded, arguments, scope).await;
        let released = runtime
            .execution
            .release_reply(&loaded.record, &loaded.fences)
            .await;
        let page = result?;
        released?;
        Ok(page)
    }
    pub(in crate::native::runs) async fn search_loaded(
        &self,
        runtime: &RuntimeAuthority,
        loaded: &LoadedReply,
        arguments: &serde_json::Value,
        scope: &dyn NativeOutputAuthority,
    ) -> Result<client_core::MessageSearchPage, RuntimeError> {
        Box::pin(self.search_loaded_with_history(runtime, loaded, arguments, scope, None)).await
    }

    pub(in crate::native::runs) async fn search_loaded_with_history(
        &self,
        runtime: &RuntimeAuthority,
        loaded: &LoadedReply,
        arguments: &serde_json::Value,
        scope: &dyn NativeOutputAuthority,
        reads: Option<&[client_core::runtime_core::ReplyHistoryRead]>,
    ) -> Result<client_core::MessageSearchPage, RuntimeError> {
        let _snapshot = loaded.policy_snapshot(runtime, scope).await?;
        let mut definition = runtime.finance.continuity_transaction().await?;
        loaded.record.current_definition_in(&mut definition).await?;
        definition.commit().await?;
        let principal = &loaded.record.original.context.principal;
        let credential = self.credential(&principal.agent, &principal.device)?;
        let capsule = runtime
            .execution
            .read_native(principal, Some(&loaded.fences.device))
            .await?;
        if capsule.root != loaded.current {
            return Err(RuntimeError::Denied);
        }
        let observed = runtime.finance.reply_clock().await?;
        let core =
            DeviceCore::restore(self.open_image(principal, capsule)?, credential.unlocked()?)
                .map_err(native_error)?;
        let page = match reads {
            Some(reads) => {
                core.continue_reply_history(&loaded.record.intent, reads, arguments, observed)
            }
            None => core.search_reply_history(&loaded.record.intent, arguments, observed),
        }
        .map_err(native_error)?;
        if serde_json::to_vec(&page)
            .map_err(|_| RuntimeError::InvalidBinding)?
            .len()
            > 32 * 1024
        {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 80 {
            runtime.native_cut_entered.notify_one();
            runtime.native_cut_resume.notified().await;
        }
        // Do not release data using a cached read grant, an earlier source
        // head or a definition/actor that changed during index computation.
        let _snapshot = loaded.policy_snapshot(runtime, scope).await?;
        let mut definition = runtime.finance.continuity_transaction().await?;
        loaded.record.current_definition_in(&mut definition).await?;
        definition.commit().await?;
        if let Some(expires) = loaded.facts.expires_at_ms() {
            if runtime.finance.reply_clock().await? >= expires {
                return Err(RuntimeError::Denied);
            }
        }
        Ok(page)
    }
}
