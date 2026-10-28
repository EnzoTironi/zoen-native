//! Direct native boundary and signed-envelope invariants; no host facts/planner.
use event_log::{chain_hash, Author, Signer};
use protocol::{ClientFrame, Envelope, Sequenced};
use relay_admission::portable::{parse_u64, MAX_ENVELOPE_BYTES, MAX_FRAME_BYTES};
use types::{EventBody, Privacy, SpaceKind, GENESIS_PREV};
fn event(author: &Author) -> Result<types::Event, Box<dyn std::error::Error>> {
    Ok(author.sign_event(
        "sp_native_contract",
        "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        1000,
        None,
        EventBody::SpaceCreated {
            title: "Native contract".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
    )?)
}
#[test]
fn native_decimal_boundary_is_exact_and_refuses_ambiguous_or_overflowing_integers() {
    for value in [0, 9_007_199_254_740_993, u64::MAX] {
        assert_eq!(parse_u64(&value.to_string()).unwrap(), value);
    }
    for value in ["", "01", "+1", "-1", " 1", "1.0", "18446744073709551616"] {
        let err = parse_u64(value).unwrap_err();
        assert_eq!(err.code, "invalid_integer");
        assert!(!err.permanent);
    }
    assert_eq!(MAX_FRAME_BYTES, 1024 * 1024);
    assert_eq!(MAX_ENVELOPE_BYTES, 90 * 1024);
}
#[test]
fn native_signed_event_bytes_genesis_chain_and_unknown_wire_fields_remain_compatible(
) -> Result<(), Box<dyn std::error::Error>> {
    let author = Author::device(
        &Signer::from_secret(&[7; 32]),
        Signer::from_secret(&[9; 32]),
    );
    let e = event(&author)?;
    let env = Envelope::plain(&e)?;
    env.verify().unwrap();
    let content = env.content().to_vec();
    let hash = chain_hash("sp_native_contract", 0, GENESIS_PREV, &env.wire_hash())?;
    let sequenced = Sequenced {
        seq: 0,
        prev: GENESIS_PREV.into(),
        hash,
        env,
    };
    let decoded = Sequenced::decode(&sequenced.encode()).unwrap();
    assert_eq!(decoded.env.content(), content);
    decoded.env.verify().unwrap();
    let mut bytes = ClientFrame::Publish { env: decoded.env }.encode();
    bytes.extend([0xf8, 0x07, 0x01]);
    let ClientFrame::Publish { env } = ClientFrame::decode(&bytes).unwrap() else {
        panic!("wrong frame")
    };
    assert_eq!(env.content(), content);
    env.verify().unwrap();
    Ok(())
}
#[test]
fn native_envelopes_refuse_bad_device_certificates_signatures_and_mutated_content(
) -> Result<(), Box<dyn std::error::Error>> {
    let author = Author::device(
        &Signer::from_secret(&[7; 32]),
        Signer::from_secret(&[9; 32]),
    );
    let e = event(&author)?;
    for variant in [0, 1, 2] {
        let mut bad = e.clone();
        match variant {
            0 => bad.cert = Some("00".repeat(64)),
            1 => bad.sig = "00".repeat(64),
            _ => bad.content[0] ^= 1,
        }
        let result = Envelope::new(bad.content, bad.sig, bad.cert, None);
        assert!(result.is_none_or(|env| env.verify().is_err()));
    }
    Envelope::plain(&event(&Author::root(Signer::from_secret(&[12; 32])))?)?
        .verify()
        .unwrap();
    Ok(())
}
