//! Pure receipt policy tests; SQL races and public Client journeys are separate.
use relay_admission::receipts::{
    raise, recipients, validate_page, Watermark, WatermarkDto, PAGE_ROWS,
};

#[test]
fn exact_unsigned_maxima_and_read_implies_delivered_without_regression() {
    let first = raise(None, 5, 0);
    assert!(first.changed);
    let read = raise(Some(first.watermark), 3, 9);
    assert_eq!(read.watermark.pair(), (9, 9));
    assert_eq!(raise(Some(read.watermark), 1, 1).watermark.pair(), (9, 9));
    assert!(!raise(Some(read.watermark), 1, 1).changed);
    let highest = raise(Some(read.watermark), u64::MAX, u64::MAX - 1);
    assert_eq!(highest.watermark.pair(), (u64::MAX, u64::MAX - 1));
    assert_eq!(
        raise(Some(highest.watermark), 0, u64::MAX).watermark.pair(),
        (u64::MAX, u64::MAX)
    );
    assert!(Watermark::stored(1, 2).is_err());
}

#[test]
fn first_zero_is_a_real_row_and_repeating_it_is_idempotent() {
    let first = raise(None, 0, 0);
    assert!(
        first.changed,
        "Initial zero still participates in complete replay"
    );
    assert_eq!(first.watermark.pair(), (0, 0));
    assert!(!raise(Some(first.watermark), 0, 0).changed);
    let restored = Watermark::from_dto(&first.watermark.dto()).unwrap();
    assert_eq!(restored, first.watermark);
    assert!(!raise(Some(restored), 0, 0).changed);
}

#[test]
fn sql_decimal_boundary_preserves_every_unsigned_bit_and_refuses_corrupt_state() {
    for delivered in [0, 1, 9_007_199_254_740_993, i64::MAX as u64 + 1, u64::MAX] {
        let mark = Watermark::stored(delivered, delivered.saturating_sub(1)).unwrap();
        let dto = mark.dto();
        let json = serde_json::to_value(&dto).unwrap();
        assert!(json["delivered"].is_string() && json["read"].is_string());
        assert_eq!(Watermark::from_dto(&dto).unwrap(), mark);
    }
    for invalid in ["01", "-1", "18446744073709551616", "1.5", "", "+1", " 1"] {
        assert!(Watermark::from_dto(&WatermarkDto {
            delivered: invalid.into(),
            read: "0".into()
        })
        .is_err());
    }
    assert!(Watermark::from_dto(&WatermarkDto {
        delivered: "18446744073709551614".into(),
        read: "18446744073709551615".into()
    })
    .is_err());
}

#[test]
fn keyset_cursor_is_bounded_and_never_crosses_its_initial_identity_ceiling() {
    let low = format!("{:064x}", 1);
    let high = format!("{:064x}", 999);
    assert!(validate_page("", &high, PAGE_ROWS as u32).is_ok());
    assert!(validate_page(&low, &high, 1).is_ok());
    assert!(
        validate_page(&high, &high, 1).is_ok(),
        "An empty final page is valid"
    );
    assert!(validate_page(&high, &low, 1).is_err());
    assert!(validate_page("", &high, 0).is_err());
    assert!(validate_page("", &high, PAGE_ROWS as u32 + 1).is_err());
    for invalid in ["bad", "", "fF", "0123"] {
        assert!(validate_page(&low, invalid, 1).is_err());
    }
    assert!(validate_page("BAD", &high, 1).is_err());
}

#[test]
fn former_authors_never_receive_receipt_metadata_after_signed_membership_removal() {
    let current: Vec<_> = (1..=18).map(|n| format!("{n:064x}")).collect();
    let former = format!("{:064x}", 99);
    let from = current[0].clone();
    let audience = recipients(&from, &current, true, &[former.clone(), current[3].clone()])
        .unwrap()
        .unwrap();
    assert_eq!(audience, vec![from, current[3].clone()]);
    assert!(!audience.contains(&former));
    assert!(recipients(&former, &current, true, &[]).unwrap().is_none());
}

#[test]
fn receipt_and_transient_audiences_preserve_own_device_scope_and_refuse_invalid_rosters() {
    let first = format!("{:064x}", 1);
    let second = format!("{:064x}", 2);
    let members = vec![second.clone(), first.clone(), second.clone()];
    assert_eq!(
        recipients(&first, &members, true, &[]).unwrap(),
        Some(vec![first.clone(), second.clone()])
    );
    assert_eq!(
        recipients(&first, &members, false, &[]).unwrap(),
        Some(vec![second])
    );
    assert!(recipients(&first, &["invalid".into()], true, &[]).is_err());
    assert!(recipients("invalid", &members, true, &[]).is_err());
    assert!(recipients(&first, &members, true, &["invalid".into()]).is_err());
    assert!(recipients(&first, &members, true, &vec![first.clone(); 17]).is_err());
}
