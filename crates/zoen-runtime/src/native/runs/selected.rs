//! Exact selected originals reuse the trigger index without advancing scan cursors.
use super::*;

pub(super) struct Selected<'a> {
    pub(super) credential: &'a Credential,
    pub(super) principal: &'a Principal,
    pub(super) fence: &'a Fence,
    pub(super) core: &'a mut DeviceCore,
    pub(super) previous: &'a Root,
    pub(super) selected: &'a crate::ReplySource,
    pub(super) head: &'a Seen,
    pub(super) applied: bool,
}
impl Selected<'_> {
    pub(super) async fn reuse(
        &mut self,
        native: &NativeCustody,
        runtime: &RuntimeAuthority,
    ) -> Result<Option<ReplySync>, RuntimeError> {
        let now = runtime.finance.reply_clock().await?;
        let facts = self
            .core
            .reply_for_message(
                &self.selected.space,
                &self.selected.message,
                &self.selected.author,
                &self.selected.text_sha256,
                now,
            )
            .map_err(native_error)?;
        let tx = runtime.execution.transaction().await?;
        let Some(run) = tx
            .get(
                &runtime.execution.reply_trigger(
                    &self.principal.agent,
                    &self.selected.space,
                    facts.intent().trigger(),
                ),
                false,
            )
            .await?
        else {
            return Ok(None);
        };
        let run = std::str::from_utf8(&run).map_err(|_| RuntimeError::InvalidBinding)?;
        let (record, original) = runtime
            .execution
            .reply_in(&tx, run, &runtime.custody)
            .await?;
        if record.original.context.principal != *self.principal
            || record.intent != *facts.intent()
            || record.continuation.is_some()
        {
            return Err(RuntimeError::Denied);
        }
        runtime
            .execution
            .reply_original_scope(&tx, &record, &original)
            .await?;
        let actual = runtime
            .execution
            .reply_source()?
            .head_in(&tx, &self.selected.space)
            .await
            .map_err(|_| RuntimeError::Denied)?
            .ok_or(RuntimeError::Denied)?;
        if actual.seq() != self.head.seq || actual.hash() != self.head.hash {
            return Err(RuntimeError::Denied);
        }
        drop(tx);
        let root = if self.applied {
            let image = self.core.image().map_err(native_error)?;
            let next = native.seal_image(self.principal, Some(self.previous), &image)?;
            let staged = runtime
                .execution
                .stage_native(
                    self.principal,
                    Some(self.previous.clone()),
                    self.fence,
                    next,
                )
                .await?;
            native
                .activate_maintenance(
                    runtime,
                    self.credential,
                    staged,
                    self.fence,
                    Some(shared::SharedSource {
                        cell: record.source.clone(),
                        space: self.selected.space.clone(),
                        expected: self.head.clone(),
                        through: self.head.clone(),
                    }),
                )
                .await?
        } else {
            runtime
                .finance
                .native_directory(
                    &self.credential.agent,
                    &self.credential.owner,
                    &self.principal.device,
                    &self.credential.certificate,
                )
                .await?
                .commit()
                .await?;
            self.previous.clone()
        };
        Ok(Some(ReplySync {
            native: NativeInspection {
                generation: root.context.generation,
                image_bytes: root.context.image_bytes,
            },
            caught_up: true,
            scanned: 1,
            discovery_complete: true,
            runs: vec![record.locator],
        }))
    }
}
impl NativeCustody {
    pub(crate) async fn verify_selected_run(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
        source: &crate::ReplySource,
    ) -> Result<(), RuntimeError> {
        source.validate()?;
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
            let core = DeviceCore::restore(
                self.open_image(principal, capsule)?,
                self.credential(&principal.agent, &principal.device)?
                    .unlocked()?,
            )
            .map_err(native_error)?;
            let now = runtime.finance.reply_clock().await?;
            let facts = core
                .reply_for_message(
                    &source.space,
                    &source.message,
                    &source.author,
                    &source.text_sha256,
                    now,
                )
                .map_err(native_error)?;
            if *facts.intent() != loaded.record.intent {
                return Err(RuntimeError::Denied);
            }
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
