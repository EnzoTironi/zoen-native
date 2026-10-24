//! Existing retained reply loader and current native revalidation.
use super::*;
impl NativeCustody {
    pub(super) async fn load_reply(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
    ) -> Result<LoadedReply, RuntimeError> {
        let (record, record_digest) = runtime.execution.read_reply(run, &runtime.custody).await?;
        let context = &record.request.context;
        let credential = self.credential(&context.agent, &context.device)?;
        let principal = credential.principal();
        if principal != record.original.context.principal {
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
        let fences = runtime
            .execution
            .acquire_reply(&record, &record_digest)
            .await?;
        let result = async {
            let capsule = runtime
                .execution
                .read_native(&principal, Some(&fences.device))
                .await?;
            let current = capsule.root.clone();
            let observed = runtime.finance.reply_clock().await?;
            #[cfg(test)]
            println!("native paid load: stage=refresh_start observed_sql_ms={observed}");
            let facts = {
                let core = DeviceCore::restore(
                    self.open_image(&principal, capsule)?,
                    credential.unlocked()?,
                )
                .map_err(native_error)?;
                core.refresh(&record.intent, observed)
                    .map_err(native_error)?
            };
            #[cfg(test)]
            println!(
                "native paid load: stage=refresh_complete observed_sql_ms={observed} native_expires_at_ms={:?} remaining_ms={:?}",
                facts.expires_at_ms(),
                facts.expires_at_ms().and_then(|expiry| expiry.checked_sub(observed))
            );
            let [InputMessage::User { text }] = record.request.messages.as_slice() else {
                return Err(RuntimeError::InvalidBinding);
            };
            if text != facts.text() {
                return Err(RuntimeError::InvalidBinding);
            }
            let dispatch = runtime
                .gateway
                .preflight(&record.request)
                .map_err(|_| RuntimeError::InvalidProfile)?;
            if dispatch.request_digest != record.binding.request_digest
                || record.binding.price.chat()?.quote(&dispatch)? != record.binding.hold_units
            {
                return Err(RuntimeError::InvalidBinding);
            }
            runtime
                .execution
                .verify_reply(
                    &record,
                    &record_digest,
                    &current,
                    &facts,
                    &fences,
                    &runtime.custody,
                )
                .await?;
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
            Ok((facts, current))
        }
        .await;
        match result {
            Ok((facts, current)) => Ok(LoadedReply {
                record,
                record_digest,
                current,
                facts,
                fences,
            }),
            Err(error) => {
                let _ = runtime.execution.release_reply(&record, &fences).await;
                Err(error)
            }
        }
    }

    pub(crate) async fn verified_reply(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
    ) -> Result<ReplyStep, RuntimeError> {
        Ok(ReplyStep {
            loaded: self.load_reply(runtime, run).await?,
        })
    }

    pub(crate) async fn inspect_reply(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
    ) -> Result<ReplyRun, RuntimeError> {
        let loaded = self.load_reply(runtime, run).await?;
        // Derived current authorization is deliberately separate from the frozen
        // original binding. No VerifiedStep or dispatch permit is constructed.
        if loaded.facts.intent() != &loaded.record.intent {
            return Err(RuntimeError::InvalidBinding);
        }
        runtime
            .execution
            .release_reply(&loaded.record, &loaded.fences)
            .await?;
        Ok(loaded.record.locator)
    }
}
