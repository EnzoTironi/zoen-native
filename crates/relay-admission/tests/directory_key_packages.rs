use relay_admission::directory::key_packages as kp;
const NOW: i64 = 1_800_000_000_000;
#[test]
fn native_claim_targets_are_sorted_sets_and_operation_clock_bounds_are_exact(
) -> Result<(), Box<dyn std::error::Error>> {
    let op = types::new_ulid(NOW)?;
    let a = "a".repeat(64);
    let b = "b".repeat(64);
    let claim = kp::claim_input(&op, &[b.clone(), a.clone(), b]).unwrap();
    assert_eq!(claim.targets, vec![a, "b".repeat(64)]);
    assert!(kp::claim_time(&claim, NOW).is_ok());
    assert_eq!(
        kp::claim_time(&claim, NOW - 1).unwrap_err(),
        "key-package claim operation is in the future"
    );
    assert_eq!(
        kp::claim_time(&claim, NOW + protocol::KEY_PACKAGE_CLAIM_TTL_MS).unwrap_err(),
        protocol::KEY_PACKAGE_CLAIM_EXPIRED
    );
    assert!(kp::claim_input(&op.to_lowercase(), &["a".repeat(64)]).is_err());
    Ok(())
}
#[test]
fn native_claim_input_refuses_invalid_id_sets_before_any_sql_selection(
) -> Result<(), Box<dyn std::error::Error>> {
    let op = types::new_ulid(NOW)?;
    assert!(kp::claim_input("bad", &[]).is_err());
    assert!(kp::claim_input(&op, &[]).is_err());
    for id in ["A".repeat(64), "g".repeat(64), "a".repeat(63)] {
        assert_eq!(
            kp::claim_input(&op, &[id]).unwrap_err(),
            "claim targets must be canonical identity keys"
        );
    }
    let ids = (0..51).map(|i| format!("{i:064x}")).collect::<Vec<_>>();
    assert_eq!(
        kp::claim_input(&op, &ids).unwrap_err(),
        "claim one to fifty identities"
    );
    assert_eq!(
        kp::claim_input(&op, &vec!["a".repeat(64); 100])
            .unwrap()
            .targets
            .len(),
        1
    );
    Ok(())
}
#[test]
fn native_publication_bounds_precede_real_mls_parsing_and_no_malformed_key_is_accepted(
) -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        kp::validate_publications("id", "device", &vec![vec![]; 101], None).unwrap_err(),
        "at most 100 key packages at a time"
    );
    assert_eq!(
        kp::validate_publications("id", "device", &[vec![0; 4097]], None).unwrap_err(),
        "key package too large"
    );
    assert_eq!(
        kp::validate_publications("id", "device", &[vec![0; 4]], None).unwrap_err(),
        "not a valid key package"
    );
    assert_eq!(
        kp::validate_publications("id", "device", &[], Some(&[0; 4097])).unwrap_err(),
        "key package too large"
    );
    assert_eq!(
        kp::validate_publications("id", "device", &[], Some(&[0; 4])).unwrap_err(),
        "not a valid key package"
    );
    assert!(kp::validate_publications("id", "device", &[], None)
        .unwrap()
        .is_empty());
    assert_eq!(
        (kp::MAX_STOCK, kp::MAX_RECEIPTS, kp::MAX_RECEIPT_BYTES),
        (200, 4096, 8 * 1024 * 1024)
    );
    Ok(())
}
