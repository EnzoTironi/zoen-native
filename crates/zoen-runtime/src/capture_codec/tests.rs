//! Inert codec fixtures; these create no grant, claim or output authority.
use super::*;
use crate::{hash, PriceProfile};
use zoen_models::{AttemptContext, UsageEvidence};
fn fixture() -> Capture {
    let context = AttemptContext {
        attempt_id: "attempt-codec".into(),
        run_id: "run-codec".into(),
        owner: "owner".into(),
        agent: "agent".into(),
        device: "device".into(),
        space: "space".into(),
        authority_version: "authority-v1".into(),
        definition_version: "definition-v1".into(),
        price_version: "price-v1".into(),
    };
    let price = PriceProfile {
        version: "price-v1".into(),
        endpoint: "https://example.com/v1/chat/completions".into(),
        model: "fixture".into(),
        credential_ref: "fixture".into(),
        currency: "USD".into(),
        scale: 6,
        input_units_per_million: 1_000_000,
        output_units_per_million: 1_000_000,
        max_billable_input_tokens: 10,
        max_output_tokens: 8,
        max_request_bytes: 32768,
        max_response_bytes: 16_777_216,
    };
    Capture {
        binding: Binding {
            context: context.clone(),
            device_cert: "certificate".into(),
            frontier: "frontier".into(),
            period_start: 0,
            policy_digest: "a".repeat(64),
            request_digest: "b".repeat(64),
            requested_output_tokens: 3,
            hold_units: 13,
            price: price.into(),
        },
        result: ModelAttemptResult {
            context,
            request_digest: "b".repeat(64),
            output: Err(OutputFailure::MalformedResponse),
            receipt: ProviderReceipt {
                http_status: Some(200),
                request_id: None,
                response_id: None,
                reported_model: Some("fixture".into()),
                usage: UsageEvidence::Missing,
            },
            evidence: ProviderEvidence {
                body: b"malformed original response".to_vec(),
                complete: true,
            },
        },
    }
}
#[test]
fn legacy_json_bytes_digest_and_custody_binding_remain_exact() -> Result<(), RuntimeError> {
    let capture = fixture();
    let plain = serde_json::to_vec(&capture).map_err(|_| RuntimeError::InvalidBinding)?;
    assert_eq!(encode(&capture)?.as_slice(), plain);
    assert_eq!(digest(&capture)?, hash(&plain));
    let custody = Custody::new([7; 32], "codec-test".into());
    let (old_digest, old_sealed) = custody.seal("evidence/attempt-codec", &capture)?;
    let decoded = open(&custody, "attempt-codec", &old_digest, &old_sealed)?;
    assert!(decoded.binding == capture.binding && decoded.result == capture.result);
    let (current, sealed) = seal(&custody, &capture)?;
    assert_eq!(current, old_digest);
    let verified = authenticate(&custody, "attempt-codec", &current, &sealed)?;
    assert!(verified.matches(&current, &hash(&sealed)));
    assert!(!verified.matches("changed-digest", &hash(&sealed)));
    assert!(!verified.matches(&current, "changed-ciphertext"));
    assert_eq!(verified.capture().result, capture.result);
    assert!(open(
        &Custody::new([7; 32], "other".into()),
        "attempt-codec",
        &current,
        &sealed
    )
    .is_err());
    assert!(open(&custody, "other-attempt", &current, &sealed).is_err());
    let mut changed = sealed;
    let last = changed.last_mut().ok_or(RuntimeError::InvalidBinding)?;
    *last ^= 1;
    assert!(open(&custody, "attempt-codec", &current, &changed).is_err());
    Ok(())
}
#[test]
fn maximum_gateway_body_and_output_round_trip_above_old_storage_bound() -> Result<(), RuntimeError>
{
    let mut capture = fixture();
    capture.result.evidence.body = vec![0xfe; MAX_RESPONSE];
    capture.result.output = Ok(vec![OutputBlock::Text {
        text: "x".repeat(MAX_RESPONSE),
    }]);
    let plain = encode(&capture)?;
    assert!(plain.starts_with(MAGIC));
    assert!(plain.len() > 32 * 1024 * 1024);
    assert!(plain.len() < MAXIMUM - 40);
    let decoded = decode(&plain)?;
    assert!(decoded.result == capture.result && decoded.binding == capture.binding);
    let custody = Custody::new([9; 32], "codec-maximum".into());
    let (digest, sealed) = seal(&custody, &capture)?;
    let decoded = open(&custody, "attempt-codec", &digest, &sealed)?;
    assert_eq!(decoded.result, capture.result);
    for cut in [MAGIC.len() - 1, MAGIC.len() + 3, plain.len() - 1] {
        assert!(decode(&plain[..cut]).is_err());
    }
    let mut trailing = plain.to_vec();
    trailing.push(0);
    assert!(decode(&trailing).is_err());
    let mut bad_header = plain.to_vec();
    bad_header[MAGIC.len()..MAGIC.len() + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(decode(&bad_header).is_err());
    Ok(())
}
#[test]
fn declared_body_profile_and_header_limits_refuse_before_sealing() -> Result<(), RuntimeError> {
    let mut capture = fixture();
    capture.result.context.run_id = "different".into();
    assert!(encode(&capture).is_err());
    capture.result.context = capture.binding.context.clone();
    capture.result.evidence.body = vec![0; MAX_RESPONSE + 1];
    assert!(encode(&capture).is_err());
    capture.result.evidence.body.clear();
    capture.result.output = Ok(vec![OutputBlock::Text {
        text: "x".repeat(MAX_HEADER + 1),
    }]);
    assert!(encode(&capture).is_err());
    let mut price = capture.binding.price.chat()?.clone();
    price.max_response_bytes = 16_777_217;
    assert!(price.digest().is_err());
    price.max_response_bytes = 16_777_216;
    price.digest()?;
    Ok(())
}
