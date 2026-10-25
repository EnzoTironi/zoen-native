use super::*;

#[test]
fn exhausted_counter_does_not_panic_or_reuse_a_nonce() {
    let keys = derive(&[3; 32], &[4; 32], &[5; 32], [6; SESSION_ID_LEN]).unwrap();
    let mut sealer = keys.sealer(Direction::Frames);
    sealer.next = u64::MAX;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sealer.seal(b"last")));
    assert!(result.is_ok(), "counter exhaustion must be a typed refusal");
    assert_eq!(result.unwrap(), Err(Error::Exhausted));
    assert_eq!(
        sealer.next,
        u64::MAX,
        "failed sealing cannot wrap the nonce"
    );
}

#[test]
fn noncontributory_offer_cannot_authenticate_known_zero_secret_frames() {
    let device = DeviceKey::from_secret([7; 32]);
    let offer = Offer {
        vm_pub: [0; 32],
        session: [8; SESSION_ID_LEN],
    };
    let known_keys = derive(&[0; 32], &offer.vm_pub, &device.public(), offer.session).unwrap();
    let frame = known_keys
        .sealer(Direction::Frames)
        .seal(b"forged frame")
        .unwrap();
    let received = device
        .accept(&offer)
        .and_then(|keys| keys.opener(Direction::Frames).open(&frame));
    assert_eq!(received, Err(Error::Auth));
}

#[test]
fn captures_legacy_session_and_frame_bytes() {
    let keys = derive(&[3; 32], &[4; 32], &[5; 32], [6; SESSION_ID_LEN]).unwrap();
    let frame = keys
        .sealer(Direction::Frames)
        .seal(b"legacy frame")
        .unwrap();
    assert_eq!(
        keys.frames,
        [
            71, 102, 189, 193, 67, 246, 252, 113, 65, 173, 229, 48, 90, 150, 141, 142, 23, 46, 229,
            166, 3, 110, 145, 246, 244, 145, 193, 208, 138, 42, 52, 43
        ]
    );
    assert_eq!(
        keys.input,
        [
            5, 249, 17, 174, 17, 173, 65, 255, 139, 24, 28, 137, 134, 40, 90, 200, 204, 63, 62, 54,
            58, 155, 255, 70, 143, 210, 80, 155, 68, 140, 13, 255
        ]
    );
    assert_eq!(
        frame,
        [
            0, 0, 0, 0, 0, 0, 0, 1, 85, 143, 61, 231, 52, 151, 67, 195, 77, 28, 137, 70, 237, 92,
            248, 210, 210, 176, 4, 13, 202, 4, 82, 133, 43, 243, 165, 141
        ]
    );
    assert_eq!(
        keys.opener(Direction::Frames).open(&frame).unwrap(),
        b"legacy frame"
    );
}

#[test]
fn entropy_refusal_never_returns_key_or_offer() {
    assert!(matches!(
        DeviceKey::generate_with(|bytes| {
            bytes[0] = 9;
            Err(getrandom::Error::UNSUPPORTED)
        }),
        Err(Error::Entropy)
    ));
    let device = DeviceKey::from_secret([7; 32]);
    for failed_call in [1, 2] {
        let mut calls = 0;
        let result = vm_start_with(&device.public(), |bytes| {
            calls += 1;
            bytes.fill(9);
            if calls == failed_call {
                Err(getrandom::Error::UNSUPPORTED)
            } else {
                Ok(())
            }
        });
        assert!(matches!(result, Err(Error::Entropy)));
        assert_eq!(calls, failed_call);
    }
    assert!(matches!(
        vm_start_with(&[0; 32], |bytes| {
            bytes.fill(9);
            Ok(())
        }),
        Err(Error::Auth)
    ));
}

#[test]
fn last_admitted_nonce_and_failed_auth_keep_monotonic_state() {
    let keys = derive(&[3; 32], &[4; 32], &[5; 32], [6; SESSION_ID_LEN]).unwrap();
    let mut sealer = keys.sealer(Direction::Frames);
    let mut opener = keys.opener(Direction::Frames);
    sealer.next = u64::MAX - 1;
    let frame = sealer.seal(b"last admitted").unwrap();
    let mut tampered = frame.clone();
    tampered[8] ^= 1;
    assert_eq!(opener.open(&tampered), Err(Error::Auth));
    assert_eq!(opener.last_seq(), 0);
    assert_eq!(opener.open(&frame).unwrap(), b"last admitted");
    assert_eq!(opener.last_seq(), u64::MAX - 1);
    assert_eq!(sealer.seal(b"refused"), Err(Error::Exhausted));
    assert_eq!(sealer.next, u64::MAX);
    assert_eq!(opener.open(&frame), Err(Error::Replay));
}
