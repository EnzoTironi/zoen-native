use base64::{engine::general_purpose::STANDARD, Engine};
use protocol::{ClientFrame, KeyPackageRecord, Op, Reply, ServerFrame};
use relay_admission::directory::{
    self,
    key_packages::{self as kp, Candidate, Facts, Receipt},
    Device, Session,
};

const NOW: i64 = 1_800_000_000_000;
fn operation() -> String {
    types::new_ulid(NOW)
}
fn target() -> String {
    "a".repeat(64)
}
fn facts() -> Facts {
    Facts {
        now_ms: NOW.to_string(),
        receipt_bytes: "0".into(),
        ..Default::default()
    }
}
fn candidate(number: usize) -> Candidate {
    Candidate {
        id: (number + 1).to_string(),
        identity: target(),
        device: format!("{number:064x}"),
        data_base64: STANDARD.encode([7; 4096]),
        last_resort: false,
    }
}
fn record() -> KeyPackageRecord {
    KeyPackageRecord {
        identity: target(),
        device: "b".repeat(64),
        data: vec![1, 2],
    }
}
fn receipt(op: &str) -> Receipt {
    Receipt {
        targets: vec![target()],
        response_base64: STANDARD.encode(
            ServerFrame::Res {
                id: 0,
                result: Ok(Reply::KeyPackages(vec![record()])),
            }
            .encode(),
        ),
        expires_ms: (protocol::key_package_claim_time_ms(op).unwrap()
            + protocol::KEY_PACKAGE_CLAIM_TTL_MS)
            .to_string(),
    }
}

#[test]
fn receipt_targets_are_sets_but_ulid_and_clock_bounds_are_exact() {
    let op = operation();
    assert_eq!(
        kp::normalize_claim(&op, &[target(), target()], NOW)
            .unwrap()
            .targets,
        vec![target()]
    );
    assert!(kp::normalize_claim(&op.to_lowercase(), &[target()], NOW).is_err());
    assert_eq!(
        kp::normalize_claim(&op, &[target()], NOW - 1).unwrap_err(),
        "key-package claim operation is in the future"
    );
    assert_eq!(
        kp::normalize_claim(&op, &[target()], NOW + protocol::KEY_PACKAGE_CLAIM_TTL_MS)
            .unwrap_err(),
        protocol::KEY_PACKAGE_CLAIM_EXPIRED
    );
}
#[test]
fn original_receipt_precedes_quota_and_current_target_stock_but_not_time() {
    let op = operation();
    let mut f = facts();
    f.receipt = Some(receipt(&op));
    f.receipt_count = 4096;
    f.receipt_bytes = (8 * 1024 * 1024).to_string();
    f.oversized = true;
    let (plan, records) = kp::claim(&[target()], Some(&op), &f).unwrap();
    assert_eq!(records, vec![record()]);
    assert_eq!(plan["replayed"], true);
    assert_eq!(plan["delete_ids"], serde_json::json!([]));
    assert!(kp::claim(&["c".repeat(64)], Some(&op), &f)
        .unwrap_err()
        .reason
        .contains("different targets"));
    f.now_ms = (NOW + protocol::KEY_PACKAGE_CLAIM_TTL_MS).to_string();
    assert_eq!(
        kp::claim(&[target()], Some(&op), &f).unwrap_err().reason,
        protocol::KEY_PACKAGE_CLAIM_EXPIRED
    );
}
#[test]
fn private_128_record_context_preserves_real_wire_bytes_and_uint64_reply_ids() {
    let mut f = facts();
    f.selected = (0..128).map(candidate).collect();
    let session = Session {
        identity: "1".repeat(64),
        device: "2".repeat(64),
        cert: "cert".into(),
        registered: true,
    };
    let context = directory::Facts {
        registered: true,
        device: Some(Device {
            identity: session.identity.clone(),
            cert: session.cert.clone(),
            revoked: false,
        }),
        key_packages: Some(f.clone()),
        ..Default::default()
    };
    let context = serde_json::json!({"session":session,"facts":context}).to_string();
    assert!(context.len() < 1024 * 1024);
    let bytes = ClientFrame::Req {
        id: 9_007_199_254_740_993,
        op: Op::ClaimKeyPackages {
            ids: vec![target()],
            operation_id: Some(operation()),
        },
    }
    .encode();
    let result: serde_json::Value =
        serde_json::from_str(&directory::request_json(&bytes, &context).unwrap()).unwrap();
    let reply = STANDARD
        .decode(result["reply_base64"].as_str().unwrap())
        .unwrap();
    assert!(reply.len() > 512 * 1024 && reply.len() < kp::PRIVATE_MAX_RESPONSE_BYTES);
    assert!(matches!(
        ServerFrame::decode(&reply),
        Ok(ServerFrame::Res {
            id: 9_007_199_254_740_993,
            ..
        })
    ));
    f.oversized = true;
    let error = kp::claim(&[target()], Some(&operation()), &f).unwrap_err();
    assert_eq!(error.code, "private_key_package_capacity");
    assert!(!error.permanent);
}
#[test]
fn legacy_claim_truncates_before_dedup_and_does_not_create_receipts() {
    let mut ids = vec![target(); 50];
    ids.push("b".repeat(64));
    let mut f = facts();
    f.selected = vec![Candidate {
        identity: "b".repeat(64),
        ..candidate(0)
    }];
    assert!(kp::claim(&ids, None, &f).is_err());
    f.selected.clear();
    let (plan, records) = kp::claim(&[], None, &f).unwrap();
    assert!(records.is_empty());
    assert!(plan["receipt"].is_null());
}
#[test]
fn response_byte_quota_is_checked_after_encoding_before_any_mutation_plan() {
    let mut f = facts();
    f.selected = vec![candidate(0)];
    let (_, records) = kp::claim(&[target()], Some(&operation()), &f).unwrap();
    let size = ServerFrame::Res {
        id: 0,
        result: Ok(Reply::KeyPackages(records)),
    }
    .encode()
    .len();
    f.receipt_bytes = (kp::MAX_RECEIPT_BYTES - size as i64 + 1).to_string();
    assert_eq!(
        kp::claim(&[target()], Some(&operation()), &f)
            .unwrap_err()
            .reason,
        "key-package claim receipt capacity reached"
    );
    f.receipt_bytes = (kp::MAX_RECEIPT_BYTES - size as i64).to_string();
    assert!(kp::claim(&[target()], Some(&operation()), &f).is_ok());
}
#[test]
fn revoked_caller_cannot_use_an_otherwise_valid_original_receipt() {
    let session = Session {
        identity: "1".repeat(64),
        device: "2".repeat(64),
        cert: "cert".into(),
        registered: true,
    };
    let mut k = facts();
    k.receipt = Some(receipt(&operation()));
    let f = directory::Facts {
        registered: true,
        device: Some(Device {
            identity: session.identity.clone(),
            cert: session.cert.clone(),
            revoked: true,
        }),
        key_packages: Some(k),
        ..Default::default()
    };
    let bytes = ClientFrame::Req {
        id: 1,
        op: Op::ClaimKeyPackages {
            ids: vec![target()],
            operation_id: Some(operation()),
        },
    }
    .encode();
    assert_eq!(
        directory::request(&bytes, &session, &f).unwrap_err().reason,
        "this device was unlinked"
    );
}
#[test]
fn publication_batch_bounds_precede_dedup_and_malformed_receipts_are_internal() {
    assert_eq!(
        kp::validate_publications("id", "device", &vec![vec![]; 101], None).unwrap_err(),
        "at most 100 key packages at a time"
    );
    assert_eq!(
        kp::validate_publications("id", "device", &[vec![0; 4097]], None).unwrap_err(),
        "key package too large"
    );
    assert!(!kp::decode_reply(&[0]).unwrap_err().permanent);
}
