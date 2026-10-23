use super::*;
use zoen_models::{ImageBackground, ImageQuality, ImageRequest, ImageSize};

#[tokio::test]
async fn cheap_chat_quota_cannot_admit_images_or_a_chat_label_with_image_output() {
    let gateway = ModelGateway::new(GatewayConfig {
        base_url: "http://127.0.0.1:1/v1".into(),
        endpoint_policy: EndpointPolicy::LoopbackFixture,
        model: "gpt-image-1.5".into(),
        credential_ref: "fixture-only".into(),
        api_key: "fixture-only".into(),
        limits: ModelLimits {
            max_request_bytes: 32768,
            max_response_bytes: 65536,
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
            max_image_bytes: 1000,
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
    assert!(authority.used.lock().unwrap().is_empty());
    assert!(
        authority.seen.lock().unwrap().is_empty(),
        "rejected capability cannot consume or poison chat quota"
    );
    chat.descriptor.image = None;
    assert_eq!(authority.admit(&chat).await, Ok(Admission::Fresh));
    assert_eq!(authority.admit(&chat).await, Ok(Admission::AlreadyKnown));
}
