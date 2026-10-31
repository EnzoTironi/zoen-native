use super::*;

#[test]
fn retained_or_uncertain_admission_never_proves_model_non_dispatch() {
    for error in [
        zoen_models::GatewayError::NotFresh,
        zoen_models::GatewayError::Admission(AdmissionError::Unavailable),
    ] {
        assert!(matches!(
            gateway_failure(error),
            BrainError::OutcomeUnknown(_)
        ));
    }
    for error in [
        zoen_models::GatewayError::InvalidConfiguration,
        zoen_models::GatewayError::InvalidRequest,
        zoen_models::GatewayError::UnsupportedOperation,
        zoen_models::GatewayError::Admission(AdmissionError::Denied),
    ] {
        assert!(matches!(
            gateway_failure(error),
            BrainError::NotDispatched(_)
        ));
    }
}
use zoen_models::{
    image_response_budget, Admission, DispatchAuthority, ImageBackground, ImageQuality,
    ImageRequest, ImageSize,
};

#[tokio::test]
async fn cheap_chat_quota_cannot_admit_images_or_a_chat_label_with_image_output() {
    let max_image_bytes = 1000;
    let max_response_bytes = usize::try_from(
        image_response_budget(max_image_bytes)
            .expect("bounded image fixture has a response budget"),
    )
    .expect("bounded image fixture response budget fits usize");
    let gateway = ModelGateway::new(GatewayConfig {
        base_url: "http://127.0.0.1:1/v1".into(),
        endpoint_policy: EndpointPolicy::LoopbackFixture,
        model: "gpt-image-1.5".into(),
        credential_ref: "fixture-only".into(),
        api_key: "fixture-only".into(),
        limits: ModelLimits {
            max_request_bytes: 32768,
            max_response_bytes,
            max_output_tokens: 100,
            timeout: Duration::from_secs(2),
            connect_timeout: Duration::from_secs(1),
        },
    })
    .unwrap();
    let context = AttemptContext {
        attempt_id: "attempt-image".into(),
        run_id: "run".into(),
        owner: "owner".into(),
        agent: "agent".into(),
        device: "device".into(),
        space: "space".into(),
        authority_version: "authority-v1".into(),
        definition_version: "definition-v1".into(),
        price_version: "price-v1".into(),
    };
    let image = gateway
        .preflight_image(&ImageRequest {
            context: context.clone(),
            prompt: "Fixture image".into(),
            size: ImageSize::Square,
            quality: ImageQuality::Low,
            background: ImageBackground::Opaque,
            max_image_bytes,
        })
        .unwrap();
    let authority = CheapTierAuthority::new(1);
    assert_eq!(authority.admit(&image).await, Err(AdmissionError::Denied));
    let mut chat = gateway
        .preflight(&ModelRequest {
            context,
            operation: Operation::ChatCompletion,
            messages: vec![InputMessage::User {
                text: "Fixture chat".into(),
            }],
            tools: vec![],
            max_output_tokens: 10,
        })
        .unwrap();
    chat.descriptor.image = image.descriptor.image.clone();
    assert_eq!(authority.admit(&chat).await, Err(AdmissionError::Denied));
    // A rejected capability must leave the full chat quota available.
    chat.descriptor.image = None;
    assert_eq!(authority.admit(&chat).await, Ok(Admission::Fresh));
    assert_eq!(authority.admit(&chat).await, Ok(Admission::AlreadyKnown));
}
