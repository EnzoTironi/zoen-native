use event_log::{Author, Signer};
use relay_admission::transfer::*;

const NOW: i64 = 1_700_000_000_000;
const ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
fn author() -> Author {
    Author::device(
        &Signer::from_secret(&[61; 32]),
        Signer::from_secret(&[62; 32]),
    )
}
fn refused<T>(value: Result<T, Failure>, status: u16, body: &str) {
    assert!(
        matches!(value, Err(error) if (error.status, error.body) == (status, body)),
        "expected refusal {status} {body}"
    );
}

#[test]
fn capability_keys_are_bounded_and_deletion_has_an_exact_prefix_boundary() {
    assert_eq!(get_key(ID, 0).unwrap(), format!("transfers/{ID}/0"));
    assert_eq!(get_key(ID, 4095).unwrap(), format!("transfers/{ID}/4095"));
    assert_eq!(delete_prefix(ID).unwrap(), format!("transfers/{ID}/"));
    for id in [
        ID.to_uppercase(),
        "a".repeat(63),
        "../".into(),
        String::new(),
    ] {
        refused(get_key(&id, 0), 400, "bad transfer");
        refused(delete_prefix(&id), 400, "bad transfer");
    }
    refused(get_key(ID, 4096), 400, "bad transfer");
    refused(get_key(ID, u32::MAX), 400, "bad transfer");
}

#[test]
fn body_sizes_and_shared_weighted_cost_keep_native_bounds() {
    for (length, cost) in [(1, 1), (1024, 1), (1025, 2), (MAX_CHUNK as u64, 4096)] {
        assert_eq!(body_cost(length).unwrap(), cost);
    }
    for length in [0, MAX_CHUNK as u64 + 1, u64::MAX] {
        refused(body_cost(length), 413, "empty or too large");
    }
}

#[test]
fn signatures_bind_operation_transfer_chunk_actual_ciphertext_and_timestamp() {
    let a = author();
    let device = a.device.as_deref().unwrap();
    let sig = a
        .key
        .sign(&protocol::transfer_message("put", ID, "3", SHA, NOW));
    assert!(put_policy(ID, 3, 10, SHA, device, &NOW.to_string(), &sig, NOW).is_ok());
    for (id, chunk, sha) in [(ID, 4, SHA), (ID, 3, ID), (SHA, 3, SHA)] {
        refused(
            put_policy(id, chunk, 10, sha, device, &NOW.to_string(), &sig, NOW),
            401,
            "bad signature",
        );
    }
    refused(
        delete_policy(ID, device, &NOW.to_string(), &sig, NOW),
        401,
        "bad signature",
    );
    let delete = a
        .key
        .sign(&protocol::transfer_message("delete", ID, "", "", NOW));
    assert_eq!(
        delete_policy(ID, device, &NOW.to_string(), &delete, NOW).unwrap(),
        format!("transfers/{ID}/")
    );
    refused(
        put_policy(ID, 3, 10, SHA, device, &NOW.to_string(), &delete, NOW),
        401,
        "bad signature",
    );
}

#[test]
fn timestamp_boundaries_and_extremes_never_overflow_or_accept_stale_proof() {
    let a = author();
    let device = a.device.as_deref().unwrap();
    for ts in [NOW - 300_000, NOW + 300_000] {
        let sig = a
            .key
            .sign(&protocol::transfer_message("delete", ID, "", "", ts));
        assert!(delete_policy(ID, device, &format!("+0{ts}"), &sig, NOW).is_ok());
    }
    for (ts, now) in [
        (NOW - 300_001, NOW),
        (NOW + 300_001, NOW),
        (i64::MIN, NOW),
        (i64::MAX, NOW),
        (i64::MIN, i64::MAX),
    ] {
        refused(
            delete_policy(ID, device, &ts.to_string(), "", now),
            401,
            "stale signature",
        );
    }
    for ts in ["", " 1700000000000", "9223372036854775808"] {
        refused(
            delete_policy(ID, device, ts, "", NOW),
            401,
            "missing signature",
        );
    }
}

#[test]
fn rejection_precedence_is_path_then_body_then_signature() {
    refused(
        put_policy("BAD", 0, 0, "", "", "", "", NOW),
        400,
        "bad transfer",
    );
    refused(
        put_policy(ID, 0, 0, "", "", "", "", NOW),
        413,
        "empty or too large",
    );
    refused(
        put_policy(ID, 0, 1, "", "", "", "", NOW),
        401,
        "missing signature",
    );
    refused(
        put_policy(ID, 0, 1, "", "", "0", "", NOW),
        401,
        "stale signature",
    );
    refused(
        put_policy(ID, 0, 1, "", "", &NOW.to_string(), "", NOW),
        401,
        "bad signature",
    );
}

#[test]
fn native_policy_keeps_large_raw_fields_and_full_unsigned_body_bounds() {
    refused(
        put_policy(&"a".repeat(17000), 0, 1, SHA, "", "", "", NOW),
        400,
        "bad transfer",
    );
    refused(
        put_policy(ID, 0, u64::MAX, SHA, "", "", "", NOW),
        413,
        "empty or too large",
    );
    refused(
        put_policy(ID, 0, 1, SHA, "", &"1".repeat(32000), "", NOW),
        401,
        "missing signature",
    );
    assert_eq!(get_key(ID, 1).unwrap(), format!("transfers/{ID}/1"));
}
