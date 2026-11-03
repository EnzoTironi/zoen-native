//! Native signed media admission, digest and weighted quota regressions.
use event_log::{Author, Signer};
use relay_admission::{
    blobs::*,
    gcra::{self, Decision},
};

const NOW: i64 = 1_700_000_000_000;
const RELAY: &str = "test-relay";
const SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn who() -> Author {
    Author::device(
        &Signer::from_secret(&[37; 32]),
        Signer::from_secret(&[38; 32]),
    )
}
fn policy(timestamp: &str, signed_ts: i64, now: i64) -> Result<PutAdmission, Failure> {
    let a = who();
    put_policy(
        SHA,
        &a.key.id(),
        timestamp,
        &a.key
            .sign(&protocol::blob_put_message(SHA, signed_ts, RELAY)),
        RELAY,
        now,
    )
}
fn failure<T>(result: Result<T, Failure>, status: u16, body: &str) {
    assert!(
        matches!(result, Err(error) if (error.status, error.body) == (status, body)),
        "expected refusal {status} {body}"
    );
}

#[test]
fn signed_prefix_preserves_raw_integer_parsing_and_relay_binding() {
    assert!(policy(&format!("+0{NOW}"), NOW, NOW).is_ok());
    let a = who();
    let sig = a
        .key
        .sign(&protocol::blob_put_message(SHA, NOW, "other-relay"));
    failure(
        put_policy(
            SHA,
            a.device.as_deref().unwrap(),
            &NOW.to_string(),
            &sig,
            RELAY,
            NOW,
        ),
        401,
        "bad signature",
    );
}

#[test]
fn skew_is_inclusive_and_extreme_timestamps_are_stale_without_overflow() {
    for offset in [-300_000, 300_000] {
        assert!(policy(&(NOW + offset).to_string(), NOW + offset, NOW).is_ok());
    }
    for offset in [-300_001, 300_001] {
        failure(
            policy(&(NOW + offset).to_string(), NOW + offset, NOW),
            401,
            "stale signature",
        );
    }
    for (ts, now) in [
        (i64::MIN, NOW),
        (i64::MAX, NOW),
        (i64::MIN, i64::MAX),
        (i64::MAX, i64::MIN),
    ] {
        failure(policy(&ts.to_string(), ts, now), 401, "stale signature");
    }
}

#[test]
fn prefix_error_precedence_is_hash_then_timestamp_then_skew_then_signature() {
    failure(put_policy("BAD", "", "", "", RELAY, NOW), 400, "bad hash");
    for ts in ["", " 1700000000000", "9223372036854775808"] {
        failure(
            put_policy(SHA, "", ts, "", RELAY, NOW),
            401,
            "missing signature",
        );
    }
    failure(
        put_policy(SHA, "", "0", "", RELAY, NOW),
        401,
        "stale signature",
    );
    failure(
        put_policy(SHA, "", &NOW.to_string(), "", RELAY, NOW),
        401,
        "bad signature",
    );
}

#[test]
fn get_capability_is_strict_lowercase_hash_and_canonical_key_only() {
    assert_eq!(get_key(SHA).unwrap(), format!("blobs/ba/{SHA}"));
    for sha in [
        SHA.to_uppercase(),
        "a".repeat(63),
        "g".repeat(64),
        "../x".into(),
    ] {
        failure(get_key(&sha), 400, "bad hash");
    }
}

#[test]
fn long_public_fields_keep_native_rejections_without_a_context_size_gate() {
    let a = who();
    let device = a.device.as_deref().unwrap();
    let now = NOW.to_string();
    let signature = a.key.sign(&protocol::blob_put_message(SHA, NOW, RELAY));
    failure(
        put_policy(&"a".repeat(17 * 1024), device, &now, &signature, RELAY, NOW),
        400,
        "bad hash",
    );
    failure(
        put_policy(SHA, &"a".repeat(32 * 1024), &now, &signature, RELAY, NOW),
        401,
        "bad signature",
    );
    failure(
        put_policy(SHA, device, &"1".repeat(32 * 1024), &signature, RELAY, NOW),
        401,
        "missing signature",
    );
    failure(
        put_policy(SHA, device, &now, &"a".repeat(32 * 1024), RELAY, NOW),
        401,
        "bad signature",
    );
}

#[test]
fn body_bounds_and_weight_rounding_match_native() {
    for (len, cost) in [
        (1, 1),
        (1024, 1),
        (1025, 2),
        (MAX_BLOB_BYTES as u64, 25 * 1024),
    ] {
        assert_eq!(body_cost(len).unwrap(), cost);
    }
    for len in [0, MAX_BLOB_BYTES as u64 + 1, u64::MAX] {
        failure(body_cost(len), 413, "empty or too large");
    }
}

#[test]
fn incremental_digest_is_exact_and_bad_hash_does_not_become_storage_success() {
    let mut digest = BlobDigest::new();
    for part in [b"a".as_slice(), b"", b"bc"] {
        digest.update(part).unwrap();
    }
    assert_eq!(digest.finish(SHA).unwrap(), SHA);
    failure(verify_body(b"abd", SHA), 400, "hash mismatch");
    let mut digest = BlobDigest::new();
    failure(
        digest.update(&vec![0; MAX_DIGEST_CHUNK + 1]),
        500,
        "storage unavailable",
    );
}

#[test]
fn multi_chunk_digest_matches_independent_native_sha256() {
    let bytes = vec![0x8a; MAX_DIGEST_CHUNK * 3 + 17];
    assert!(verify_body(&bytes, &event_log::sha256_hex(&bytes)).is_ok());
}

#[test]
fn exact_native_gcra_arithmetic_keeps_large_integer_credit() {
    assert_eq!(DEFAULT_INTERVAL_NS, 585_937);
    assert_eq!(DEFAULT_BURST, 204_800);
    let now = 9_007_199_254_740_993;
    assert_eq!(
        gcra::decide(now, Some(now), 2, 1_000_000_000, 1),
        Decision::Rejected {
            wait_ns: 1_000_000_000
        }
    );
    assert_eq!(
        gcra::decide(now, None, 1, 1, 1),
        Decision::Accepted { next_ns: now + 1 }
    );
}

#[test]
fn gcra_refusal_does_not_spend_and_idle_refills_without_extra_credit() {
    let mut tat = None;
    for _ in 0..5 {
        let Decision::Accepted { next_ns } = gcra::decide(0, tat, 1, 100, 5) else {
            panic!("burst");
        };
        tat = Some(next_ns);
    }
    assert_eq!(
        gcra::decide(0, tat, 1, 100, 5),
        Decision::Rejected { wait_ns: 100 }
    );
    assert_eq!(
        gcra::decide(99, tat, 1, 100, 5),
        Decision::Rejected { wait_ns: 1 }
    );
    assert_eq!(
        gcra::decide(100, tat, 1, 100, 5),
        Decision::Accepted { next_ns: 600 }
    );
    assert_eq!(
        gcra::decide(10_000, tat, 5, 100, 5),
        Decision::Accepted { next_ns: 10_500 }
    );
}

#[test]
fn gcra_preserves_saturation_and_zero_cost_behavior() {
    assert_eq!(
        gcra::decide(u64::MAX - 1, None, u32::MAX, u64::MAX, u32::MAX),
        Decision::Accepted { next_ns: u64::MAX }
    );
    assert_eq!(
        gcra::decide(5, Some(8), 0, 10, 1),
        Decision::Accepted { next_ns: 8 }
    );
    assert_eq!(
        gcra::decide(0, None, 6, 100, 5),
        Decision::Rejected { wait_ns: 100 }
    );
}
