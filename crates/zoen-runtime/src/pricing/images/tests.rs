use super::*;
use serde_json::{json, Value};
use zoen_models::{
    ImageBackground, ImageQuality, ImageSize, ImageTokenDetails, ImageUsageEvidence, OutputFailure,
    ProviderEvidence, ProviderReceipt, ReportedUsage, UsageEvidence,
};
fn profile() -> ImagePriceProfile {
    ImagePriceProfile {
        version: "image-v1".into(),
        endpoint: "https://api.openai.com/v1/images/generations".into(),
        model: "gpt-image-fixture".into(),
        credential_ref: "host-image-fixture".into(),
        currency: "USD".into(),
        scale: 6,
        image: ImageDispatch {
            count: 1,
            size: ImageSize::Square,
            quality: ImageQuality::Low,
            background: ImageBackground::Opaque,
            format: "png".into(),
            max_image_bytes: 60_000,
        },
        input_text_units_per_million: 1_000_000,
        input_image_units_per_million: 2_000_000,
        output_text_units_per_million: 3_000_000,
        output_image_units_per_million: 4_000_000,
        reservation_units: 10,
        max_request_bytes: 32768,
        max_response_bytes: 262144,
    }
}
fn dispatch(p: &ImagePriceProfile) -> DispatchRequest {
    DispatchRequest {
        context: AttemptContext {
            attempt_id: "attempt-image".into(),
            run_id: "run-image".into(),
            owner: "owner".into(),
            agent: "agent".into(),
            device: "device".into(),
            space: "space".into(),
            authority_version: "authority-v1".into(),
            definition_version: "definition-v1".into(),
            price_version: p.version.clone(),
        },
        request_digest: "a".repeat(64),
        profile: IMAGE_PROFILE,
        descriptor: DispatchDescriptor {
            operation: Operation::ImageGeneration,
            endpoint: p.endpoint.clone(),
            model: p.model.clone(),
            credential_ref: p.credential_ref.clone(),
            encoded_request_bytes: 200,
            max_request_bytes: 32768,
            max_response_bytes: 262144,
            requested_output_tokens: 0,
            image: Some(p.image.clone()),
        },
    }
}
fn result(q: &ImagePriceQuote) -> ImageAttemptResult {
    let usage = ReportedUsage {
        input_tokens: Some(5),
        output_tokens: Some(2),
        total_tokens: Some(7),
        cache_read_tokens: None,
        reasoning_tokens: None,
    };
    let details = ImageTokenDetails {
        input_text_tokens: Some(2),
        input_image_tokens: Some(3),
        output_text_tokens: Some(1),
        output_image_tokens: Some(1),
    };
    let mut result = ImageAttemptResult {
        context: q.context().clone(),
        request_digest: q.request_digest().into(),
        output: Err(OutputFailure::MalformedResponse),
        receipt: ProviderReceipt {
            http_status: Some(200),
            request_id: Some("request".into()),
            response_id: None,
            reported_model: None,
            usage: UsageEvidence::Reported(usage),
        },
        image_usage: ImageUsageEvidence::Reported(details),
        evidence: ProviderEvidence {
            complete: true,
            body: b"{}".to_vec(),
        },
    };
    reflect_usage(&mut result);
    result
}
// Fixtures retain the same raw provider counters as their normalized receipt.
fn reflect_usage(result: &mut ImageAttemptResult) {
    let UsageEvidence::Reported(u) = &result.receipt.usage else {
        panic!("reported fixture");
    };
    let ImageUsageEvidence::Reported(d) = &result.image_usage else {
        panic!("reported fixture");
    };
    let usage = json!({"input_tokens":u.input_tokens,"output_tokens":u.output_tokens,
        "total_tokens":u.total_tokens,"input_tokens_details":{
            "text_tokens":d.input_text_tokens,"image_tokens":d.input_image_tokens},
        "output_tokens_details":{"text_tokens":d.output_text_tokens,"image_tokens":d.output_image_tokens}});
    raw(result, |value| value["usage"] = usage);
}

fn raw(r: &mut ImageAttemptResult, f: impl FnOnce(&mut Value)) {
    let mut value: Value = serde_json::from_slice(&r.evidence.body).unwrap();
    f(&mut value);
    r.evidence.body = serde_json::to_vec(&value).unwrap();
}
#[test]
fn image_configured_reserve_is_not_a_cap_and_invalid_output_can_still_incur_full_charge() {
    let p = profile();
    let d = dispatch(&p);
    let q = p.quote(&d).unwrap();
    assert_eq!(q.reservation_units(), 10);
    assert_eq!(q.charge(&result(&q)).unwrap(), Some(15));
    assert_eq!(
        q.reservation_units(),
        10,
        "quote cannot lower its configured reservation"
    );
    assert_eq!(
        d.descriptor.requested_output_tokens, 0,
        "absence of provider cap is not zero price"
    );
    let mut r = result(&q);
    r.receipt.reported_model = Some(p.model.clone());
    raw(&mut r, |value| value["model"] = p.model.clone().into());
    assert_eq!(q.charge(&r).unwrap(), Some(15));
    r.context.device = "replacement".into();
    assert_eq!(q.charge(&r), Err(RuntimeError::InvalidBinding));
    r = result(&q);
    r.request_digest = "b".repeat(64);
    assert_eq!(q.charge(&r), Err(RuntimeError::InvalidBinding));
}
#[test]
fn image_profile_and_quote_digests_bind_price_request_scope_and_exact_specification() {
    let p = profile();
    let d = dispatch(&p);
    let q = p.quote(&d).unwrap();
    let mut alternatives = vec![];
    macro_rules! changed {
        ($field:ident,$value:expr) => {{
            let mut x = p.clone();
            x.$field = $value;
            alternatives.push(x);
        }};
    }
    changed!(version, "image-v2".into());
    changed!(
        endpoint,
        "https://example.test/v1/images/generations".into()
    );
    changed!(model, "gpt-image-other".into());
    changed!(credential_ref, "other-reference".into());
    changed!(reservation_units, 11);
    changed!(input_text_units_per_million, 17);
    changed!(input_image_units_per_million, 19);
    changed!(output_text_units_per_million, 23);
    changed!(output_image_units_per_million, 29);
    changed!(max_request_bytes, 32767);
    changed!(max_response_bytes, 65535);
    for i in 0..5 {
        let mut x = p.clone();
        match i {
            0 => x.image.size = ImageSize::Portrait,
            1 => x.image.quality = ImageQuality::High,
            2 => x.image.background = ImageBackground::Transparent,
            3 => x.image.max_image_bytes -= 1,
            _ => x.image.size = ImageSize::Landscape,
        };
        alternatives.push(x);
    }
    for x in alternatives {
        assert_ne!(x.digest().unwrap(), q.profile_digest());
        let mut dx = dispatch(&x);
        dx.descriptor.max_request_bytes = x.max_request_bytes;
        dx.descriptor.max_response_bytes = x.max_response_bytes;
        assert_ne!(x.quote(&dx).unwrap().digest(), q.digest());
        assert!(
            x.quote(&d).is_err()
                || x.reservation_units != p.reservation_units
                || x.input_text_units_per_million != p.input_text_units_per_million
                || x.input_image_units_per_million != p.input_image_units_per_million
                || x.output_text_units_per_million != p.output_text_units_per_million
                || x.output_image_units_per_million != p.output_image_units_per_million,
            "wire/price disagreement cannot be silently admitted"
        );
    }
    for i in 0..5 {
        let mut x = dispatch(&p);
        match i {
            0 => x.context.attempt_id = "other-attempt".into(),
            1 => x.context.owner = "other-owner".into(),
            2 => x.context.authority_version = "new-frontier".into(),
            3 => x.request_digest = "c".repeat(64),
            _ => x.descriptor.encoded_request_bytes += 1,
        };
        assert_ne!(p.quote(&x).unwrap().digest(), q.digest());
    }
    let encoded = serde_json::to_vec(&p).unwrap();
    let restored: ImagePriceProfile = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(restored.quote(&d).unwrap(), q);
    let debug = format!("{q:?}");
    assert!(!debug.contains("owner"));
    let mut unknown = serde_json::to_value(&p).unwrap();
    unknown["max_output_tokens"] = 10.into();
    assert!(serde_json::from_value::<ImagePriceProfile>(unknown).is_err());
}
#[test]
fn incompatible_image_profiles_and_dispatches_fail_closed() {
    let p = profile();
    for i in 0..15 {
        let mut x = p.clone();
        match i {
            0 => x.reservation_units = 0,
            1 => x.reservation_units = -1,
            2 => x.currency = "EUR".into(),
            3 => x.scale = 5,
            4 => x.image.count = 2,
            5 => x.image.format = "jpeg".into(),
            6 => x.image.max_image_bytes = MAX_IMAGE_BYTES + 1,
            7 => x.max_request_bytes = 0,
            8 => x.max_response_bytes = 16_777_217,
            9 => x.endpoint = "http://api.openai.com/v1/images/generations".into(),
            10 => x.endpoint = "https://user:pass@example.test/v1/images/generations".into(),
            11 => x.endpoint = "https://example.test/v1/images/generations?other=1".into(),
            12 => x.endpoint = "https://example.test/v1/chat/completions".into(),
            13 => x.model = "chat-only".into(),
            _ => {
                x.input_text_units_per_million = 0;
                x.input_image_units_per_million = 0;
                x.output_text_units_per_million = 0;
                x.output_image_units_per_million = 0;
            }
        }
        assert_eq!(x.digest(), Err(RuntimeError::InvalidProfile));
    }
    for i in 0..13 {
        let mut d = dispatch(&p);
        match i {
            0 => d.profile = zoen_models::CHAT_PROFILE,
            1 => d.descriptor.operation = Operation::ChatCompletion,
            2 => d.descriptor.requested_output_tokens = 1,
            3 => d.context.price_version = "other".into(),
            4 => d.descriptor.image = None,
            5 => d.descriptor.image.as_mut().unwrap().count = 2,
            6 => d.descriptor.image.as_mut().unwrap().quality = ImageQuality::High,
            7 => d.descriptor.image.as_mut().unwrap().max_image_bytes -= 1,
            8 => d.descriptor.encoded_request_bytes = 0,
            9 => d.descriptor.encoded_request_bytes = d.descriptor.max_request_bytes + 1,
            10 => d.descriptor.max_response_bytes = p.max_response_bytes + 1,
            11 => d.context.owner = d.context.agent.clone(),
            _ => d.request_digest = "A".repeat(64),
        };
        assert_eq!(p.quote(&d), Err(RuntimeError::InvalidProfile));
    }
}
#[test]
fn absent_invalid_incomplete_or_differential_usage_keeps_cost_unknown_and_reserve_intact() {
    let p = profile();
    let q = p.quote(&dispatch(&p)).unwrap();
    for i in 0..22 {
        let mut r = result(&q);
        match i {
            0 => r.evidence.complete = false,
            1 => r.receipt.http_status = Some(500),
            2 => r.receipt.reported_model = Some("gpt-image-other".into()),
            3 => r.receipt.usage = UsageEvidence::Missing,
            4 => r.receipt.usage = UsageEvidence::Invalid,
            5 => r.image_usage = ImageUsageEvidence::Missing,
            6 => r.image_usage = ImageUsageEvidence::Invalid,
            7 => r.evidence.body = b"broken".to_vec(),
            8 => raw(&mut r, |v| v["quality"] = "high".into()),
            9 => raw(&mut r, |v| {
                v["usage"]["input_tokens_details"]["cached_tokens"] = 1.into()
            }),
            10 => raw(&mut r, |v| {
                v["usage"]["input_tokens_details"]["cached_tokens_details"] =
                    json!({"image_tokens":1})
            }),
            11 => r.evidence.body = vec![b'x'; 65537],
            20 => raw(&mut r, |v| v["usage"] = Value::Null),
            21 => raw(&mut r, |v| v["usage"]["input_tokens"] = 6.into()),
            12..=15 => {
                let UsageEvidence::Reported(ref mut u) = r.receipt.usage else {
                    unreachable!()
                };
                match i {
                    12 => u.input_tokens = None,
                    13 => u.total_tokens = Some(8),
                    14 => u.cache_read_tokens = Some(1),
                    _ => u.reasoning_tokens = Some(1),
                }
            }
            _ => {
                let ImageUsageEvidence::Reported(ref mut u) = r.image_usage else {
                    unreachable!()
                };
                match i {
                    16 => u.input_text_tokens = None,
                    17 => u.output_image_tokens = None,
                    18 => u.input_text_tokens = Some(6),
                    _ => u.output_text_tokens = Some(2),
                }
            }
        }
        assert_eq!(q.charge(&r).unwrap(), None, "case{i}");
        assert_eq!(q.reservation_units(), 10);
    }
}
#[test]
fn reported_zero_and_homogeneous_tariffs_do_not_invent_missing_counters() {
    let mut p = profile();
    p.input_image_units_per_million = p.input_text_units_per_million;
    p.output_image_units_per_million = p.output_text_units_per_million;
    let q = p.quote(&dispatch(&p)).unwrap();
    let mut r = result(&q);
    r.image_usage = ImageUsageEvidence::Reported(ImageTokenDetails::default());
    reflect_usage(&mut r);
    assert_eq!(
        q.charge(&r).unwrap(),
        Some(11),
        "known totals suffice for equal configured tariffs"
    );
    let UsageEvidence::Reported(ref mut usage) = r.receipt.usage else {
        unreachable!()
    };
    usage.input_tokens = Some(0);
    usage.output_tokens = Some(0);
    usage.total_tokens = Some(0);
    reflect_usage(&mut r);
    assert_eq!(
        q.charge(&r).unwrap(),
        Some(0),
        "reported zero is distinct from Missing"
    );
    r.receipt.usage = UsageEvidence::Missing;
    assert_eq!(q.charge(&r).unwrap(), None);
    let mut r = result(&q);
    let ImageUsageEvidence::Reported(ref mut details) = r.image_usage else {
        unreachable!()
    };
    details.input_text_tokens = Some(9);
    assert_eq!(
        q.charge(&r).unwrap(),
        None,
        "even equal rates do not excuse contradictory details"
    );
}
#[test]
fn integer_rounding_boundaries_and_overflows_never_cap_or_wrap_incurred_cost() {
    assert_eq!(
        super::super::arithmetic::million_token_cost(&[(1, 300_000); 4]).unwrap(),
        2,
        "round once after summing tariffs"
    );
    assert_eq!(
        super::super::arithmetic::million_token_cost(&[(u64::MAX, u64::MAX); 2]),
        Err(RuntimeError::InvalidProfile)
    );
    let mut p = profile();
    p.input_image_units_per_million = p.input_text_units_per_million;
    let q = p.quote(&dispatch(&p)).unwrap();
    let mut r = result(&q);
    let UsageEvidence::Reported(ref mut u) = r.receipt.usage else {
        unreachable!()
    };
    u.input_tokens = Some(i64::MAX as u64);
    u.output_tokens = Some(0);
    u.total_tokens = Some(i64::MAX as u64);
    r.image_usage = ImageUsageEvidence::Reported(ImageTokenDetails {
        output_text_tokens: Some(0),
        output_image_tokens: Some(0),
        ..Default::default()
    });
    reflect_usage(&mut r);
    assert_eq!(q.charge(&r).unwrap(), Some(i64::MAX));
    let UsageEvidence::Reported(ref mut u) = r.receipt.usage else {
        unreachable!()
    };
    u.input_tokens = Some(i64::MAX as u64 + 1);
    u.total_tokens = u.input_tokens;
    reflect_usage(&mut r);
    assert_eq!(q.charge(&r), Err(RuntimeError::InvalidProfile));
    assert_eq!(q.reservation_units(), 10);
}

#[test]
fn image_price_refuses_a_spec_that_its_response_budget_cannot_represent() {
    let mut p = profile();
    p.max_response_bytes = zoen_models::image_response_budget(p.image.max_image_bytes).unwrap();
    assert!(p.validate().is_ok());
    let mut d = dispatch(&p);
    d.descriptor.max_response_bytes = p.max_response_bytes;
    assert!(p.quote(&d).is_ok());
    let digest = p.digest().unwrap();
    let mut incompatible = d.clone();
    incompatible.descriptor.max_response_bytes -= 1;
    assert!(p.quote(&incompatible).is_err());
    p.max_response_bytes -= 1;
    assert!(p.validate().is_err());
    assert!(p.digest().is_err());
    p.max_response_bytes += 2;
    assert_ne!(p.digest().unwrap(), digest);
}
