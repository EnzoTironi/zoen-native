use super::*;

#[test]
fn exhausted_counter_does_not_panic_or_reuse_a_nonce() {
    let keys = derive(&[3; 32], &[4; 32], &[5; 32], [6; SESSION_ID_LEN]);
    let mut sealer = keys.sealer(Direction::Frames);
    sealer.next = u64::MAX;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sealer.seal(b"last")));
    assert!(result.is_ok(), "counter exhaustion must be a typed refusal");
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
    let known_keys = derive(&[0; 32], &offer.vm_pub, &device.public(), offer.session);
    let frame = known_keys.sealer(Direction::Frames).seal(b"forged frame");
    let received = device.accept(&offer).opener(Direction::Frames).open(&frame);
    assert_eq!(received, Err(Error::Auth));
}

#[test]
fn captures_legacy_session_and_frame_bytes() {
    let keys = derive(&[3; 32], &[4; 32], &[5; 32], [6; SESSION_ID_LEN]);
    let frame = keys.sealer(Direction::Frames).seal(b"legacy frame");
    println!(
        "frames={:?} input={:?} frame={frame:?}",
        keys.frames, keys.input
    );
    assert_eq!(
        keys.opener(Direction::Frames).open(&frame).unwrap(),
        b"legacy frame"
    );
}
