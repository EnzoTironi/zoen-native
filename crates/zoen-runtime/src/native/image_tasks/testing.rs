//! Controlled native Image journey; never a production output capability.
pub(crate) use super::output::testing::{bytes, remove_chunk, restore_chunk};
use super::*;
use crate::{image_tasks::ImageAttemptAdapter, FinancialState, VerifiedStep};
use client_core::image_tasks::ImageTaskInput;

pub(crate) async fn stage(
    runtime: &RuntimeAuthority,
    agent: &str,
    device: &str,
    space: &str,
    source: &str,
    input: ImageTaskInput,
) -> (String, String, protocol::Envelope) {
    let custody = runtime.native.as_ref().unwrap();
    let credential = custody.credential(agent, device).unwrap();
    let principal = credential.principal();
    let fence = runtime
        .execution
        .acquire_native_device(&principal)
        .await
        .unwrap();
    let capsule = runtime
        .execution
        .read_native(&principal, Some(&fence))
        .await
        .unwrap();
    let previous = capsule.root.clone();
    let mut core = Box::new(
        DeviceCore::restore(
            custody.open_image(&principal, capsule).unwrap(),
            credential.unlocked().unwrap(),
        )
        .unwrap(),
    );
    let (request, envelope, image) = core.stage_image_task(space, source, input).unwrap();
    let candidate = custody
        .seal_image(&principal, Some(&previous), &image)
        .unwrap();
    let staged = runtime
        .execution
        .stage_native(&principal, Some(previous), &fence, candidate)
        .await
        .unwrap();
    let admission = runtime
        .finance
        .native_directory(
            &credential.agent,
            &credential.owner,
            device,
            &credential.certificate,
        )
        .await
        .unwrap()
        .authorize(staged.scope(&runtime.execution, &fence))
        .await
        .unwrap();
    runtime
        .execution
        .activate_native(staged, &fence, admission)
        .await
        .unwrap();
    runtime
        .execution
        .release_native_device(&principal, &fence)
        .await
        .unwrap();
    (
        request.id,
        request.tool_approval.unwrap().content_hash,
        envelope,
    )
}

pub(crate) async fn run(
    runtime: &RuntimeAuthority,
    run: &str,
    gateway: &ModelGateway,
    adapter: &dyn ImageAttemptAdapter,
) -> FinancialState {
    // Exact real native loader and common Session. Only the cfg(test) existing
    // empty-namespace capability substitutes for the CLOSED production barrier.
    assert!(runtime.output_fixture.is_some());
    let image = runtime
        .native
        .as_ref()
        .unwrap()
        .verified_image(runtime, run, gateway)
        .await
        .unwrap();
    let task = image.task().clone();
    let step = VerifiedStep::NativeImage(Box::new(image));
    let result = runtime.complete_image_task(&step, &task, adapter).await;
    step.release(&runtime.execution).await.unwrap();
    result.unwrap()
}

pub(crate) async fn task(runtime: &RuntimeAuthority, run: &str) -> ImageTask {
    runtime
        .execution
        .read_image_task(run, &runtime.custody)
        .await
        .unwrap()
        .0
        .task
}

pub(crate) async fn output(
    runtime: &RuntimeAuthority,
    run: &str,
    gateway: &ModelGateway,
) -> ImageOutputInspection {
    try_output(runtime, run, gateway).await.unwrap()
}

pub(crate) async fn try_output(
    runtime: &RuntimeAuthority,
    run: &str,
    gateway: &ModelGateway,
) -> Result<ImageOutputInspection, RuntimeError> {
    runtime
        .native
        .as_ref()
        .unwrap()
        .prepare_image_output(
            runtime,
            run,
            gateway,
            runtime.output_fixture.as_ref().unwrap(),
        )
        .await
}
