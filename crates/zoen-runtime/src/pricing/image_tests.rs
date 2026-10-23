use super::*;
use zoen_models::{
    AttemptContext, DispatchDescriptor, ImageBackground, ImageDispatch, ImageQuality, ImageSize,
    IMAGE_PROFILE,
};
#[test]
fn chat_price_never_quotes_image_generation_or_a_forged_chat_image_descriptor() {
    let price = PriceProfile {
        version: "v1".into(),
        endpoint: "https://api.openai.com/v1/chat/completions".into(),
        model: "fixture".into(),
        credential_ref: "fixture".into(),
        currency: "USD".into(),
        scale: 6,
        input_units_per_million: 100,
        output_units_per_million: 200,
        max_billable_input_tokens: 100,
        max_output_tokens: 100,
        max_request_bytes: 32768,
        max_response_bytes: 16384,
    };
    let mut request = DispatchRequest {
        context: AttemptContext {
            attempt_id: "a".into(),
            run_id: "r".into(),
            owner: "o".into(),
            agent: "g".into(),
            device: "d".into(),
            space: "s".into(),
            authority_version: "v".into(),
            definition_version: "v".into(),
            price_version: "v1".into(),
        },
        request_digest: "digest".into(),
        profile: CHAT_PROFILE,
        descriptor: DispatchDescriptor {
            operation: Operation::ChatCompletion,
            endpoint: price.endpoint.clone(),
            model: price.model.clone(),
            credential_ref: price.credential_ref.clone(),
            encoded_request_bytes: 100,
            max_request_bytes: 32768,
            max_response_bytes: 16384,
            requested_output_tokens: 10,
            image: None,
        },
    };
    let chat = price.quote(&request).unwrap();
    request.descriptor.image = Some(ImageDispatch {
        count: 1,
        size: ImageSize::Square,
        quality: ImageQuality::Low,
        background: ImageBackground::Opaque,
        format: "png".into(),
        max_image_bytes: 1000,
    });
    assert!(matches!(
        price.quote(&request),
        Err(RuntimeError::InvalidProfile)
    ));
    request.profile = IMAGE_PROFILE;
    request.descriptor.operation = Operation::ImageGeneration;
    request.descriptor.requested_output_tokens = 0;
    assert!(matches!(
        price.quote(&request),
        Err(RuntimeError::InvalidProfile)
    ));
    request.profile = CHAT_PROFILE;
    request.descriptor.operation = Operation::ChatCompletion;
    request.descriptor.requested_output_tokens = 10;
    request.descriptor.image = None;
    assert_eq!(
        price.quote(&request).unwrap(),
        chat,
        "existing chat quote is unchanged"
    );
}
