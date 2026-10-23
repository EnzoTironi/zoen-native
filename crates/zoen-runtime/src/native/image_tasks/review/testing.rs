//! The same staging implementation under the existing isolated fixture scope.
use super::*;

pub(crate) async fn stage(
    runtime: &RuntimeAuthority,
    intent: &ImageReviewIntent,
    target: &super::super::delivery::ImageDeliveryTarget,
) -> Result<ImageReviewProgress, RuntimeError> {
    runtime
        .native
        .as_ref()
        .unwrap()
        .stage_image_review(
            runtime,
            intent,
            target,
            runtime.output_fixture.as_ref().unwrap(),
        )
        .await
}

pub(crate) async fn original(
    runtime: &RuntimeAuthority,
    intent: &ImageReviewIntent,
) -> (String, String, Envelope) {
    let key = runtime.execution.root.pack(&(
        "native-image-review/1",
        &intent.agent,
        &intent.space,
        &intent.source,
    ));
    let bytes = runtime
        .execution
        .transaction()
        .await
        .unwrap()
        .get(&key, false)
        .await
        .unwrap()
        .unwrap();
    let review: Review = runtime.custody.open(&subject(&key), None, &bytes).unwrap();
    let credential = runtime
        .native
        .as_ref()
        .unwrap()
        .credential(&intent.agent, &intent.device)
        .unwrap();
    let envelope = review
        .envelope(
            runtime.execution.reply_source().unwrap().cell(),
            &hash(&serde_json::to_vec(&intent.input).unwrap()),
            credential,
            &intent.space,
        )
        .unwrap();
    (review.request, review.content_hash, envelope)
}
