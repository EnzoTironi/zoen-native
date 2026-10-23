use super::*;
use crate::{
    ImageBackground, ImageQuality, ImageRequest, ImageSize, ImageUsageEvidence, IMAGE_PROFILE,
};
use rig_core::providers::openai::image_generation::GPT_IMAGE_1_5;
use sha2::Digest;

pub(super) fn input() -> ImageRequest {
    ImageRequest {
        context: request().context,
        prompt: PROMPT.into(),
        size: ImageSize::Square,
        quality: ImageQuality::Low,
        background: ImageBackground::Opaque,
        max_image_bytes: 60_000,
    }
}
pub(super) fn image_config(base: &str) -> GatewayConfig {
    let mut c = config(base);
    c.model = GPT_IMAGE_1_5.into();
    c.limits.max_response_bytes = 262_144;
    c
}
pub(super) fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&vec![0; (width * height * 4) as usize])
            .unwrap();
    }
    bytes
}
fn encoded(bytes: &[u8]) -> String {
    // Rig's same canonical base64 codec; no test-only provider implementation.
    use base64::Engine;
    base64::prelude::BASE64_STANDARD.encode(bytes)
}
pub(super) fn response(bytes: &[u8]) -> Value {
    json!({"created":1,"data":[{"b64_json":encoded(bytes)}],"output_format":"png",
        "size":"1024x1024","quality":"low","background":"opaque",
        "usage":{"input_tokens":5,"output_tokens":7,"total_tokens":12,
            "input_tokens_details":{"text_tokens":5,"image_tokens":0},
            "output_tokens_details":{"image_tokens":7,"text_tokens":0}}})
}

#[tokio::test]
async fn image_wire_preflight_and_verified_png_preserve_exact_bytes_and_reported_usage() {
    let bytes = png(1024, 1024);
    let f = Fixture::json(response(&bytes)).await;
    let gateway = ModelGateway::new(image_config(&f.base)).unwrap();
    let authority = TestAuthority::default();
    assert!(gateway.supports(Operation::ImageGeneration));
    let preflight = gateway.preflight_image(&input()).unwrap();
    assert_eq!(authority.calls.load(Ordering::SeqCst), 0);
    assert_eq!(preflight.profile, IMAGE_PROFILE);
    assert_eq!(preflight.descriptor.operation, Operation::ImageGeneration);
    assert_eq!(preflight.descriptor.requested_output_tokens, 0);
    let spec = preflight.descriptor.image.as_ref().unwrap();
    assert_eq!(
        (spec.count, spec.format.as_str(), spec.size),
        (1, "png", ImageSize::Square)
    );
    let actual = gateway.generate_image(input(), &authority).await.unwrap();
    assert_eq!(actual.request_digest, preflight.request_digest);
    let image = actual.output.unwrap();
    assert_eq!(image.bytes, bytes);
    assert_eq!(
        (image.width, image.height, image.mime),
        (1024, 1024, "image/png")
    );
    assert_eq!(image.sha256, hex::encode(sha2::Sha256::digest(&bytes)));
    assert_eq!(actual.receipt.http_status, Some(200));
    assert_eq!(actual.receipt.request_id.as_deref(), Some("transport-17"));
    assert_eq!(
        actual.receipt.reported_model, None,
        "configured model is not a provider-reported fact"
    );
    let UsageEvidence::Reported(c) = actual.receipt.usage else {
        panic!("missing reported usage");
    };
    assert_eq!(
        (c.input_tokens, c.output_tokens, c.total_tokens),
        (Some(5), Some(7), Some(12))
    );
    let ImageUsageEvidence::Reported(c) = actual.image_usage else {
        panic!("missing image token details");
    };
    assert_eq!(
        (c.input_image_tokens, c.output_image_tokens),
        (Some(0), Some(7))
    );
    assert!(actual.evidence.complete);
    let seen = f.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].0.starts_with("POST /v1/images/generations "));
    assert_eq!(
        seen[0].1,
        json!({"model":GPT_IMAGE_1_5,"prompt":PROMPT,"size":"1024x1024",
        "n":1,"output_format":"png","stream":false,"quality":"low","background":"opaque",
        "safety_identifier":hex::encode(sha2::Sha256::digest(format!("zoen-image-user-v1\0{}",input().context.owner)))})
    );
    assert!(!serde_json::to_string(&preflight.descriptor)
        .unwrap()
        .contains(PROMPT));
}

#[tokio::test]
async fn image_replay_binding_changes_concurrent_duplicates_and_denial_make_no_extra_effect() {
    let f = Fixture::json(response(&png(1024, 1024))).await;
    let gateway = ModelGateway::new(image_config(&f.base)).unwrap();
    for e in [AdmissionError::Denied, AdmissionError::Unavailable] {
        assert_eq!(
            gateway.generate_image(input(), &Refuse(e)).await,
            Err(GatewayError::Admission(e))
        );
    }
    assert_eq!(f.connections.load(Ordering::SeqCst), 0);
    let authority = TestAuthority::default();
    let (a, b) = tokio::join!(
        gateway.generate_image(input(), &authority),
        gateway.generate_image(input(), &authority)
    );
    assert_eq!([a, b].iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        gateway.generate_image(input(), &authority).await,
        Err(GatewayError::NotFresh)
    );
    let mut changed = input();
    changed.quality = ImageQuality::High;
    assert_eq!(
        gateway.generate_image(changed, &authority).await,
        Err(GatewayError::Admission(AdmissionError::Denied))
    );
    assert_eq!(f.connections.load(Ordering::SeqCst), 1);
    for changed in [
        ImageRequest {
            size: ImageSize::Portrait,
            ..input()
        },
        ImageRequest {
            quality: ImageQuality::Medium,
            ..input()
        },
        ImageRequest {
            background: ImageBackground::Transparent,
            ..input()
        },
        ImageRequest {
            max_image_bytes: 50_000,
            ..input()
        },
        ImageRequest {
            prompt: "different".into(),
            ..input()
        },
    ] {
        assert_ne!(
            gateway.preflight_image(&changed).unwrap().request_digest,
            gateway.preflight_image(&input()).unwrap().request_digest
        );
    }
}

#[tokio::test]
async fn image_validation_fails_before_admission_and_chat_stays_unchanged() {
    let f = Fixture::json(reply()).await;
    let authority = TestAuthority::default();
    let gateway = ModelGateway::new(image_config(&f.base)).unwrap();
    for request in [
        ImageRequest {
            prompt: " ".into(),
            ..input()
        },
        ImageRequest {
            prompt: "x".repeat(32_001),
            ..input()
        },
        ImageRequest {
            max_image_bytes: 0,
            ..input()
        },
        ImageRequest {
            max_image_bytes: crate::MAX_IMAGE_BYTES + 1,
            ..input()
        },
    ] {
        assert_eq!(
            gateway.generate_image(request, &authority).await,
            Err(GatewayError::InvalidRequest)
        );
    }
    let mut bad = input();
    bad.context.owner = bad.context.agent.clone();
    assert_eq!(
        gateway.generate_image(bad, &authority).await,
        Err(GatewayError::InvalidRequest)
    );
    let chat = ModelGateway::new(config(&f.base)).unwrap();
    assert!(!chat.supports(Operation::ImageGeneration));
    assert_eq!(
        chat.generate_image(input(), &authority).await,
        Err(GatewayError::UnsupportedOperation)
    );
    let descriptor = chat.preflight(&request()).unwrap().descriptor;
    assert!(descriptor.image.is_none());
    assert!(!serde_json::to_string(&descriptor)
        .unwrap()
        .contains("image"));
    assert_eq!(authority.calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.connections.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn malformed_truncated_substituted_and_oversized_images_never_become_output() {
    let bytes = png(1024, 1024);
    let mut extra = response(&bytes);
    extra["data"]
        .as_array_mut()
        .unwrap()
        .push(json!({"b64_json":encoded(&bytes)}));
    let mut url = response(&bytes);
    url["data"][0]["url"] = "http://127.0.0.1:1/private".into();
    let mut format = response(&bytes);
    format["output_format"] = "jpeg".into();
    let mut corrupted = bytes.clone();
    corrupted[32] ^= 1; // IHDR checksum: valid dimensions do not excuse corrupt PNG.
    let mut bad_base64 = response(&bytes);
    bad_base64["data"][0]["b64_json"] = "not base64".into();
    for value in [
        extra,
        url,
        format,
        bad_base64,
        response(b"not an image"),
        response(&corrupted),
        response(&bytes[..bytes.len() - 10]),
        response(&png(1, 1)),
    ] {
        let f = Fixture::json(value).await;
        let g = ModelGateway::new(image_config(&f.base)).unwrap();
        let result = g
            .generate_image(input(), &TestAuthority::default())
            .await
            .unwrap();
        assert!(result.output.is_err());
        assert!(result.evidence.complete);
        assert_eq!(
            f.connections.load(Ordering::SeqCst),
            1,
            "no retry or URL fetch"
        );
    }
    let f = Fixture::json(response(&bytes)).await;
    let g = ModelGateway::new(image_config(&f.base)).unwrap();
    let result = g
        .generate_image(
            ImageRequest {
                max_image_bytes: 8,
                ..input()
            },
            &TestAuthority::default(),
        )
        .await
        .unwrap();
    assert_eq!(result.output, Err(OutputFailure::ResponseTooLarge));
}

#[tokio::test]
async fn image_provider_failures_and_usage_missing_invalid_or_zero_remain_distinct() {
    let bytes = png(1024, 1024);
    for (usage, kind) in [
        (Value::Null, 0),
        (
            json!({"input_tokens":0,"output_tokens":0,"total_tokens":0}),
            1,
        ),
        (
            json!({"input_tokens":2,"output_tokens":3,"total_tokens":4}),
            2,
        ),
        (
            json!({"input_tokens":2,"input_tokens_details":{"image_tokens":3}}),
            2,
        ),
        (json!({"input_tokens":-1}), 2),
        (json!({"input_tokens":u64::MAX,"output_tokens":1}), 2),
        (
            json!({"input_tokens_details":{"image_tokens":u64::MAX,"text_tokens":1}}),
            2,
        ),
    ] {
        let mut value = response(&bytes);
        value["usage"] = usage;
        let f = Fixture::json(value).await;
        let g = ModelGateway::new(image_config(&f.base)).unwrap();
        let actual = g
            .generate_image(input(), &TestAuthority::default())
            .await
            .unwrap();
        assert!(actual.output.is_ok());
        match (actual.receipt.usage, kind) {
            (UsageEvidence::Missing, 0) | (UsageEvidence::Invalid, 2) => {}
            (UsageEvidence::Reported(c), 1) => assert_eq!(c.total_tokens, Some(0)),
            _ => panic!("usage state was invented"),
        }
    }
    let f = Fixture::serve(
        429,
        "private provider error".into(),
        "",
        false,
        Duration::ZERO,
        false,
    )
    .await;
    let g = ModelGateway::new(image_config(&f.base)).unwrap();
    let result = g
        .generate_image(input(), &TestAuthority::default())
        .await
        .unwrap();
    assert_eq!(result.output, Err(OutputFailure::ProviderRejected));
    assert_eq!(result.receipt.http_status, Some(429));
    assert_eq!(result.receipt.usage, UsageEvidence::Missing);
    assert_eq!(f.connections.load(Ordering::SeqCst), 1);
    let debug = format!("{result:?} {:?}", input());
    for private in [PROMPT, "private provider error", KEY] {
        assert!(!debug.contains(private));
    }
}

#[tokio::test]
async fn image_cancellation_or_incomplete_response_never_releases_the_attempt() {
    let f = Fixture::serve(
        200,
        response(&png(1024, 1024)).to_string(),
        "",
        false,
        Duration::from_secs(1),
        false,
    )
    .await;
    let g = ModelGateway::new(image_config(&f.base)).unwrap();
    let a = TestAuthority::default();
    let mut pending = Box::pin(g.generate_image(input(), &a));
    tokio::select! {_=pending.as_mut()=>panic!("response must be held"),_=f.received.notified()=>{},
    _=tokio::time::sleep(Duration::from_secs(2))=>panic!("no request observed")}
    drop(pending);
    assert_eq!(
        g.generate_image(input(), &a).await,
        Err(GatewayError::NotFresh)
    );
    assert_eq!(f.connections.load(Ordering::SeqCst), 1);
    let f = Fixture::serve(200, "{\"data\":".into(), "", false, Duration::ZERO, true).await;
    let g = ModelGateway::new(image_config(&f.base)).unwrap();
    let a = TestAuthority::default();
    let result = g.generate_image(input(), &a).await.unwrap();
    assert!(result.output.is_err());
    assert!(!result.evidence.complete);
    assert_eq!(result.receipt.usage, UsageEvidence::Missing);
    assert_eq!(
        g.generate_image(input(), &a).await,
        Err(GatewayError::NotFresh)
    );
}

#[test]
fn image_response_domain_has_exact_base64_padding_and_envelope_boundary() {
    use crate::{image_response_budget, IMAGE_JSON_ENVELOPE_BYTES};
    assert_eq!(image_response_budget(0), None);
    assert_eq!(
        image_response_budget(1),
        Some(IMAGE_JSON_ENVELOPE_BYTES + 4)
    );
    assert_eq!(image_response_budget(2), image_response_budget(3));
    assert_eq!(
        image_response_budget(4),
        Some(IMAGE_JSON_ENVELOPE_BYTES + 8)
    );
    let maximum = ((16 * 1024 * 1024 - IMAGE_JSON_ENVELOPE_BYTES) / 4) * 3;
    assert_eq!(maximum, 12_533_760);
    assert_eq!(image_response_budget(maximum), Some(16 * 1024 * 1024));
    assert_eq!(
        image_response_budget(maximum + 1),
        Some(16 * 1024 * 1024 + 4)
    );
    assert!(image_response_budget(crate::MAX_IMAGE_BYTES).unwrap() > 16 * 1024 * 1024);
    assert_eq!(image_response_budget(u64::MAX), None);
    let mut config = image_config("http://127.0.0.1:1");
    let mut request = input();
    request.max_image_bytes = maximum;
    config.limits.max_response_bytes = 16 * 1024 * 1024;
    let gateway = ModelGateway::new(config).unwrap();
    assert!(gateway.preflight_image(&request).is_ok());
    request.max_image_bytes += 1;
    assert_eq!(
        gateway.preflight_image(&request),
        Err(GatewayError::InvalidRequest)
    );
}

#[tokio::test]
async fn image_json_envelope_domain_is_enforced_without_losing_usage() {
    for extra in [0, 1] {
        let mut body = response(&png(1024, 1024));
        body["metadata"] = json!("");
        let encoded = body["data"][0]["b64_json"].as_str().unwrap().len();
        let base = serde_json::to_vec(&body).unwrap().len() - encoded;
        body["metadata"] =
            json!("m".repeat(crate::IMAGE_JSON_ENVELOPE_BYTES as usize - base + extra));
        assert_eq!(
            serde_json::to_vec(&body).unwrap().len() - encoded,
            crate::IMAGE_JSON_ENVELOPE_BYTES as usize + extra
        );
        let f = Fixture::json(body).await;
        let gateway = ModelGateway::new(image_config(&f.base)).unwrap();
        let result = gateway
            .generate_image(input(), &TestAuthority::default())
            .await
            .unwrap();
        if extra == 0 {
            assert!(result.output.is_ok());
        } else {
            assert_eq!(result.output, Err(OutputFailure::ResponseTooLarge));
        }
        assert!(matches!(
            result.image_usage,
            ImageUsageEvidence::Reported(_)
        ));
        assert_eq!(f.connections.load(Ordering::SeqCst), 1);
    }
}
