//! Explicitly reviewed Image tasks share native custody, fences and finance.
mod blobs;
mod delivery;
mod host;
mod output;
mod review;
mod storage;
#[cfg(test)]
pub(crate) mod testing;
use super::*;
use crate::{image_tasks::ImageTask, pricing::BoundPrice, Binding, ImagePriceProfile};
use client_core::runtime_core::ImageTaskFacts;
pub use delivery::{ImageDeliveryProgress, ImageDeliveryTarget};
pub use output::ImageOutputInspection;
pub use review::{ImageReviewIntent, ImageReviewProgress};
use zoen_models::{
    AttemptContext, DispatchRequest, ImageRequest, InputMessage, ModelGateway, ModelRequest,
    Operation,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    source: String,
    task: ImageTask,
    original: Root,
}
impl Record {
    fn validate(&self, run: &str, source: &str) -> Result<(), RuntimeError> {
        self.task.validate()?;
        let b = &self.task.binding;
        let principal = &self.original.context.principal;
        self.original
            .context
            .validate(principal, self.original.context.key_version)?;
        if self.source != source
            || b.context.run_id != run
            || b.context.owner != principal.owner
            || b.context.agent != principal.agent
            || b.context.device != principal.device
            || hash(b.device_cert.as_bytes()) != principal.certificate_digest
            || b.frontier != b.context.authority_version
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
}
pub(crate) struct ImageStep {
    record: Record,
    record_digest: String,
    current: Root,
    facts: ImageTaskFacts,
    run_fence: crate::execution::Fence,
    device_fence: crate::execution::Fence,
    request: ModelRequest,
}
impl ImageStep {
    pub(crate) fn request(&self) -> &ModelRequest {
        &self.request
    }
    pub(crate) fn task(&self) -> &ImageTask {
        &self.record.task
    }
    pub(crate) fn deadline(&self) -> Option<i64> {
        Some(self.facts.expires_at_ms())
    }
    pub(crate) fn binding(&self, request: &DispatchRequest) -> Result<Binding, RuntimeError> {
        self.record.task.validate()?;
        if request.context != self.record.task.binding.context
            || request.request_digest != self.record.task.binding.request_digest
            || request.descriptor != self.record.task.dispatch
            || request.profile != zoen_models::IMAGE_PROFILE
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(self.record.task.binding.clone())
    }
    pub(crate) async fn scope_in(
        &self,
        execution: &crate::execution::Execution,
        trx: &foundationdb::Transaction,
        custody: &Custody,
    ) -> Result<(), RuntimeError> {
        execution.verify_image_task(trx, self, custody).await
    }
    pub(crate) async fn release(
        &self,
        execution: &crate::execution::Execution,
    ) -> Result<(), RuntimeError> {
        execution.release_image_task(self).await
    }
}

impl NativeCustody {
    pub(crate) async fn prepare_image_run(
        &self,
        runtime: &RuntimeAuthority,
        agent: &str,
        device: &str,
        request: &str,
        price: &ImagePriceProfile,
        gateway: &ModelGateway,
    ) -> Result<String, RuntimeError> {
        let credential = self.credential(agent, device)?;
        let principal = credential.principal();
        runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                device,
                &credential.certificate,
            )
            .await?
            .commit()
            .await?;
        let fence = runtime.execution.acquire_native_device(&principal).await?;
        let result = async {
            let capsule = runtime
                .execution
                .read_native(&principal, Some(&fence))
                .await?;
            let original = capsule.root.clone();
            let core = Box::new(
                DeviceCore::restore(
                    self.open_image(&principal, capsule)?,
                    credential.unlocked()?,
                )
                .map_err(|_| RuntimeError::Denied)?,
            );
            let now = runtime.finance.reply_clock().await?;
            let facts = core
                .image_task(request, now)
                .map_err(|_| RuntimeError::Denied)?;
            let input = facts.input();
            if input.definition.price_digest != price.digest()?
                || input.definition.version.is_empty()
            {
                return Err(RuntimeError::InvalidBinding);
            }
            let seed = hash(
                &serde_json::to_vec(&("native-image-run/1", agent, facts.space(), request))
                    .map_err(|_| RuntimeError::InvalidBinding)?,
            );
            let run = host::run_id(agent, facts.space(), request)?;
            let request = ImageRequest {
                context: AttemptContext {
                    run_id: run.clone(),
                    attempt_id: format!("image_attempt_{seed}"),
                    owner: principal.owner.clone(),
                    agent: agent.into(),
                    device: device.into(),
                    space: facts.space().into(),
                    authority_version: facts.frontier().hash().into(),
                    definition_version: input.definition.version.clone(),
                    price_version: price.version.clone(),
                },
                prompt: input.prompt.clone(),
                size: price.image.size,
                quality: price.image.quality,
                background: price.image.background,
                max_image_bytes: price.image.max_image_bytes,
            };
            if serde_json::to_value(request.size)
                .ok()
                .as_ref()
                .and_then(|v| v.as_str())
                != Some(input.definition.size.as_str())
                || serde_json::to_value(request.quality)
                    .ok()
                    .as_ref()
                    .and_then(|v| v.as_str())
                    != Some(input.definition.quality.as_str())
                || serde_json::to_value(request.background)
                    .ok()
                    .as_ref()
                    .and_then(|v| v.as_str())
                    != Some(input.definition.background.as_str())
                || request.max_image_bytes != input.definition.max_image_bytes
            {
                return Err(RuntimeError::InvalidBinding);
            }
            let dispatch = gateway
                .preflight_image(&request)
                .map_err(|_| RuntimeError::InvalidProfile)?;
            let quote = price.quote(&dispatch)?;
            let obligation = BoundPrice::Image(price.clone());
            let (period_start, policy_digest) = runtime
                .finance
                .operation_budget(&principal.owner, &obligation, 0)
                .await?;
            let mut task = ImageTask {
                schema: 1,
                request,
                dispatch: dispatch.descriptor,
                binding: Binding {
                    context: dispatch.context,
                    device_cert: credential.certificate.clone(),
                    frontier: facts.frontier().hash().into(),
                    period_start,
                    policy_digest,
                    request_digest: dispatch.request_digest,
                    requested_output_tokens: 0,
                    hold_units: quote.reservation_units(),
                    price: obligation,
                },
                source_digest: facts.proposal().source_sha256.clone(),
                task_digest: String::new(),
                request_id: facts.request().into(),
                approval_hash: facts.content_hash().into(),
            };
            task.task_digest = task.expected_digest()?;
            task.validate()?;
            let record = Record {
                source: runtime
                    .execution
                    .source
                    .as_ref()
                    .ok_or(RuntimeError::CoreAuthorityUnavailable)?
                    .cell()
                    .into(),
                task,
                original,
            };
            runtime
                .execution
                .create_image_task(
                    runtime,
                    &record,
                    &facts,
                    &fence,
                    &credential.agent,
                    &credential.owner,
                    &credential.certificate,
                )
                .await?;
            Ok(run)
        }
        .await;
        let released = runtime
            .execution
            .release_native_device(&principal, &fence)
            .await;
        match result {
            Err(error) => Err(error),
            Ok(run) => {
                released?;
                Ok(run)
            }
        }
    }
    pub(crate) async fn verified_image(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
        gateway: &ModelGateway,
    ) -> Result<ImageStep, RuntimeError> {
        let (record, record_digest) = runtime
            .execution
            .read_image_task(run, &runtime.custody)
            .await?;
        let ctx = &record.task.request.context;
        let credential = self.credential(&ctx.agent, &ctx.device)?;
        let principal = credential.principal();
        if record.original.context.principal != principal {
            return Err(RuntimeError::InvalidBinding);
        }
        runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                &ctx.device,
                &credential.certificate,
            )
            .await?
            .commit()
            .await?;
        let (run_fence, device_fence) = runtime
            .execution
            .acquire_image_task(&record, &record_digest)
            .await?;
        let result = async {
            let capsule = runtime
                .execution
                .read_native(&principal, Some(&device_fence))
                .await?;
            let current = capsule.root.clone();
            let core = Box::new(
                DeviceCore::restore(
                    self.open_image(&principal, capsule)?,
                    credential.unlocked()?,
                )
                .map_err(|_| RuntimeError::Denied)?,
            );
            let now = runtime.finance.reply_clock().await?;
            let facts = core
                .image_task(&record.task.request_id, now)
                .map_err(|_| RuntimeError::Denied)?;
            let dispatch = gateway
                .preflight_image(&record.task.request)
                .map_err(|_| RuntimeError::InvalidProfile)?;
            if dispatch.request_digest != record.task.binding.request_digest
                || dispatch.descriptor != record.task.dispatch
            {
                return Err(RuntimeError::InvalidBinding);
            }
            let request = ModelRequest {
                context: ctx.clone(),
                operation: Operation::ImageGeneration,
                messages: vec![InputMessage::User {
                    text: record.task.request.prompt.clone(),
                }],
                tools: Vec::new(),
                max_output_tokens: 0,
            };
            let step = ImageStep {
                record: record.clone(),
                record_digest: record_digest.clone(),
                current,
                facts,
                run_fence: run_fence.clone(),
                device_fence: device_fence.clone(),
                request,
            };
            step.scope_in(
                &runtime.execution,
                &runtime.execution.transaction().await?,
                &runtime.custody,
            )
            .await?;
            Ok(step)
        }
        .await;
        if result.is_err() {
            let _ = runtime
                .execution
                .release_image_fences(&record, &run_fence, &device_fence)
                .await;
        }
        result
    }
}

#[cfg(test)]
pub(crate) async fn seed_association_fixture(runtime: &RuntimeAuthority, task: &ImageTask) {
    // Authenticated metadata protocol only: this contains no native image/MLS
    // facts and cannot load as a production execution step.
    let b = &task.binding;
    let original = Root {
        context: Context {
            format: 1,
            key_version: 1,
            principal: Principal {
                owner: b.context.owner.clone(),
                agent: b.context.agent.clone(),
                device: b.context.device.clone(),
                certificate_digest: hash(b.device_cert.as_bytes()),
            },
            previous: None,
            generation: 1,
            capsule: "controlled-image-capsule".into(),
            image_bytes: 512,
            image_digest: "a".repeat(64),
            chunks: 1,
        },
        manifest_digest: "b".repeat(64),
    };
    let record = Record {
        source: runtime.execution.source.as_ref().unwrap().cell().into(),
        task: task.clone(),
        original,
    };
    let run = &b.context.run_id;
    record.validate(run, &record.source).unwrap();
    let (_, bytes) = runtime
        .custody
        .seal(&format!("native-image-run/1/{run}"), &record)
        .unwrap();
    let trx = runtime.execution.transaction().await.unwrap();
    trx.set(&runtime.execution.run(run, "image-record/1"), &bytes);
    trx.set(
        &runtime
            .execution
            .native_references(&record.original)
            .pack(&(run.as_str(),)),
        record.original.manifest_digest.as_bytes(),
    );
    trx.commit().await.unwrap();
}
