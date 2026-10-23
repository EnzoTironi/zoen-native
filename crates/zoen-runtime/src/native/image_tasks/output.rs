use super::*;
#[cfg(test)]
pub(super) mod testing;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageOutputInspection {
    pub item: String,
    pub sha256: String,
    pub generation: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Publication {
    pub(super) task: String,
    pub(super) evidence: String,
    pub(super) item: String,
    pub(super) sha256: String,
    pub(super) wire: Vec<u8>,
    pub(super) generation: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) blobs: Vec<super::blobs::BlobReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) accepted: Option<types::Seen>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) checkpoint: Option<relay_admission::managed_images::Checkpoint>,
}

impl NativeCustody {
    pub(crate) async fn prepare_image_output(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
        gateway: &ModelGateway,
        capability: &FreshOutputNamespace,
    ) -> Result<ImageOutputInspection, RuntimeError> {
        runtime.continuity()?;
        // A lost activation acknowledgement is an observation of retained
        // output, never another signing, provider attempt or current grant.
        let trx = runtime.execution.transaction().await?;
        capability.check_in(&runtime.execution, &trx).await?;
        if let Some(bytes) = trx
            .get(&runtime.execution.run(run, "image-output/1"), false)
            .await?
        {
            let (record, _) = runtime
                .execution
                .read_image_task(run, &runtime.custody)
                .await?;
            let principal = &record.original.context.principal;
            let credential = self.credential(&principal.agent, &principal.device)?;
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
            let publication: Publication =
                runtime
                    .custody
                    .open(&format!("native-image-output/1/{run}"), None, &bytes)?;
            if publication.task != record.task.task_digest
                || publication.evidence
                    != runtime
                        .execution
                        .image_capture_digest(&record.task, &runtime.custody)
                        .await?
                || !id(&publication.item)
                || !digest(&publication.sha256)
                || publication.generation <= record.original.context.generation
            {
                return Err(RuntimeError::InvalidBinding);
            }
            if !publication.blobs.is_empty() {
                super::blobs::read(
                    runtime,
                    run,
                    &publication.blobs,
                    &publication.sha256,
                    record.task.request.max_image_bytes,
                )
                .await?;
            }
            return Ok(ImageOutputInspection {
                item: publication.item,
                sha256: publication.sha256,
                generation: publication.generation,
            });
        }
        drop(trx);
        let step = self.verified_image(runtime, run, gateway).await?;
        let result = self
            .prepare_image_output_fenced(runtime, &step, capability)
            .await;
        let _ = step.release(&runtime.execution).await;
        result
    }
    async fn prepare_image_output_fenced(
        &self,
        runtime: &RuntimeAuthority,
        step: &ImageStep,
        capability: &FreshOutputNamespace,
    ) -> Result<ImageOutputInspection, RuntimeError> {
        let task = &step.record.task;
        let run = &task.binding.context.run_id;
        let (digest, capture) = runtime.retained_image_capture(task).await?;
        let image = capture.result.output.map_err(|_| RuntimeError::Denied)?;
        let principal = &step.record.original.context.principal;
        let credential = self.credential(&principal.agent, &principal.device)?;
        let capsule = runtime
            .execution
            .read_native(principal, Some(&step.device_fence))
            .await?;
        if capsule.root != step.current {
            return Err(RuntimeError::Denied);
        }
        let mut core = Box::new(
            DeviceCore::restore(self.open_image(principal, capsule)?, credential.unlocked()?)
                .map_err(|_| RuntimeError::Denied)?,
        );
        let now = runtime.finance.reply_clock().await?;
        let prepared = core
            .prepare_image_task_referenced(&task.request_id, &task.approval_hash, &image.bytes, now)
            .map_err(|_| RuntimeError::Denied)?;
        if prepared.facts().frontier() != step.facts.frontier()
            || prepared.file().sha256 != image.sha256
        {
            return Err(RuntimeError::InvalidBinding);
        }
        // Physical retention is inert until its authenticated references and
        // this exact native capsule are activated together below.
        let blobs = super::blobs::retain(runtime, run, prepared.blobs(), prepared.file()).await?;
        super::blobs::read(
            runtime,
            run,
            &blobs,
            &image.sha256,
            task.request.max_image_bytes,
        )
        .await?;
        let next = self.seal_image(principal, Some(&step.current), prepared.image())?;
        let transition = runtime
            .begin_image_publish_checkpoint(prepared.envelope())
            .await?;
        let publication = Publication {
            checkpoint: transition.as_ref().map(|value| value.checkpoint()),
            accepted: None,
            blobs,
            task: task.task_digest.clone(),
            evidence: digest,
            item: prepared.facts().proposal().artifact.clone(),
            sha256: image.sha256,
            wire: protocol::ClientFrame::Publish {
                env: prepared.envelope().clone(),
            }
            .encode(),
            generation: next.root.context.generation,
        };
        if publication.wire.len() > 98_304 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let (_, publication_bytes) = runtime
            .custody
            .seal(&format!("native-image-output/1/{run}"), &publication)?;
        let staged = runtime
            .execution
            .stage_native(
                principal,
                Some(step.current.clone()),
                &step.device_fence,
                next,
            )
            .await?;
        let mut scope = staged.scope(&runtime.execution, &step.device_fence);
        scope.effects = hash(&publication_bytes);
        let admission = runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                &principal.device,
                &credential.certificate,
            )
            .await?
            .image_checkpoint(transition.as_ref().map(|value| value.annotation()))
            .await?
            .authorize_before(scope.clone(), step.deadline())
            .await?;
        admission.check(&scope)?;
        let trx = runtime.execution.transaction().await?;
        capability.check_in(&runtime.execution, &trx).await?;
        step.scope_in(&runtime.execution, &trx, &runtime.custody)
            .await?;
        if trx
            .get(&runtime.execution.run(run, "image-output/1"), false)
            .await?
            .is_some()
        {
            return Err(RuntimeError::Denied);
        }
        runtime
            .execution
            .native_activation(&trx, &staged, &step.device_fence)
            .await?;
        trx.set(
            &runtime.execution.run(run, "image-output/1"),
            &publication_bytes,
        );
        if let Some(transition) = &transition {
            transition
                .annotation()
                .write_fdb(&runtime.execution, &trx)
                .await?;
        }
        admission.consume(&scope)?;
        runtime.continuity()?;
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        if let Some(transition) = transition {
            transition.finish(runtime).await?;
        }
        Ok(ImageOutputInspection {
            item: publication.item,
            sha256: publication.sha256,
            generation: publication.generation,
        })
    }
}
