//! Controlled native Image journey; never a production output capability.
pub(crate) use super::output::testing::{bytes, remove_chunk, restore_chunk, transport_original};
pub(crate) use super::review::testing as review;
use super::*;
use crate::{image_tasks::ImageAttemptAdapter, FinancialState, VerifiedStep};

pub(crate) async fn deliver(
    runtime: &RuntimeAuthority,
    run: &str,
    target: &crate::ImageDeliveryTarget,
) -> Result<crate::ImageDeliveryProgress, RuntimeError> {
    let scope = runtime.output_fixture.as_ref().unwrap();
    runtime
        .native
        .as_ref()
        .unwrap()
        .deliver_image_output(runtime, run, target, scope)
        .await
}

pub(crate) async fn uploads_pending(runtime: &RuntimeAuthority, run: &str) -> u64 {
    let (record, _) = runtime
        .execution
        .read_image_task(run, &runtime.custody)
        .await
        .unwrap();
    let principal = &record.original.context.principal;
    let custody = runtime.native.as_ref().unwrap();
    let credential = custody
        .credential(&principal.agent, &principal.device)
        .unwrap();
    let capsule = runtime
        .execution
        .read_native(principal, None)
        .await
        .unwrap();
    let core = DeviceCore::restore(
        custody.open_image(principal, capsule).unwrap(),
        credential.unlocked().unwrap(),
    )
    .unwrap();
    let bytes = runtime
        .execution
        .transaction()
        .await
        .unwrap()
        .get(&runtime.execution.run(run, "image-output/1"), false)
        .await
        .unwrap()
        .unwrap();
    let publication: output::Publication = runtime
        .custody
        .open(&format!("native-image-output/1/{run}"), None, &bytes)
        .unwrap();
    publication
        .blobs
        .iter()
        .filter(|reference| {
            core.image_output_upload_pending(&reference.chunk.blob)
                .unwrap()
        })
        .count() as u64
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
