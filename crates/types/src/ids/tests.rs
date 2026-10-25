use super::*;

#[test]
fn entropy_failure_returns_no_identifier_or_sequence_mutation() {
    let sequence = Mutex::new(7_u128);
    let failed = ulid_with(&sequence, 1000, |_| Err(getrandom::Error::UNSUPPORTED));
    assert!(matches!(failed, Err(IdError::Entropy(_))));
    assert_eq!(*sequence.lock().unwrap(), 7);
    assert!(matches!(
        secret_id_with("invite", |_| Err(getrandom::Error::UNSUPPORTED)),
        Err(IdError::Entropy(_))
    ));
}

#[test]
fn unsupported_clock_and_poisoned_or_exhausted_sequence_refuse_ids() {
    let before_epoch = std::time::UNIX_EPOCH - std::time::Duration::from_millis(1);
    assert!(matches!(clock_ms(before_epoch), Err(IdError::Clock(_))));
    let exhausted = Mutex::new(u128::MAX);
    assert!(matches!(
        ulid_with(&exhausted, 0, |_| Ok(())),
        Err(IdError::SequenceExhausted)
    ));
    assert_eq!(*exhausted.lock().unwrap(), u128::MAX);
    let poisoned = Mutex::new(7_u128);
    let panic = std::panic::catch_unwind(|| {
        let _held = poisoned.lock().unwrap();
        panic!("poison the actual sequence mutex");
    });
    assert!(panic.is_err());
    assert!(matches!(
        ulid_with(&poisoned, 1000, |_| Ok(())),
        Err(IdError::SequencePoisoned)
    ));
}

#[test]
fn successful_bytes_and_monotonic_rollback_preserve_the_id_wire_format() {
    let sequence = Mutex::new(0);
    let first = ulid_with(&sequence, 1_700_000_000_123, |bytes| {
        bytes.copy_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        Ok(())
    })
    .unwrap();
    assert_eq!(first, "01HF7YAT3V000G40R40M30E209");
    assert_eq!(id_time_ms(&first), Some(1_700_000_000_123));
    assert!(ulid_with(&sequence, 1, |_| Ok(())).unwrap() > first);
    assert_eq!(
        secret_id_with("invite", |bytes| {
            bytes.fill(0xab);
            Ok(())
        })
        .unwrap(),
        "invite_abababababababababababababababab"
    );
}

#[test]
fn noncanonical_130_bit_ulid_never_wraps_to_an_accepted_timestamp() {
    assert_eq!(id_time_ms("ZZZZZZZZZZZZZZZZZZZZZZZZZZ"), None);
    assert_eq!(id_time_ms("80000000000000000000000000"), None);
    assert_eq!(id_time_ms("00000000000000000000000000"), Some(0));
    assert_eq!(
        id_time_ms("7ZZZZZZZZZZZZZZZZZZZZZZZZZ"),
        Some(0xFFFF_FFFF_FFFF)
    );
}
