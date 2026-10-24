use super::*;
use event_log::{Author, Signer};
use protocol::{Envelope, Sealed};

fn entry(kind: SealedKind) -> Sequenced {
    let root = Signer::from_secret(&[11; 32]);
    let device = Signer::from_secret(&[12; 32]);
    let author = Author::device(&root, device);
    let env = Envelope::sealed(
        &author,
        "space",
        "private-client-id",
        1,
        None,
        Sealed::new(kind, 1, b"private-ciphertext".to_vec()),
    );
    Sequenced {
        seq: 0,
        prev: types::GENESIS_PREV.into(),
        hash: event_log::chain_hash("space", 0, types::GENESIS_PREV, &env.wire_hash()),
        env,
    }
}

fn wire(entry: &Sequenced) -> String {
    format!("{}\n", hex::encode(entry.encode()))
}

#[test]
fn actual_welcome_and_pruned_stub_expose_only_closed_metadata() {
    let mut entry = entry(SealedKind::Welcome);
    for pruned in [false, true] {
        if pruned {
            entry.env = entry.env.pruned().expect("actual signed pruning stub");
        }
        let projected = project(wire(&entry).as_bytes(), "space").expect("valid metadata");
        assert_eq!(projected.rows[0].kind, "sealed_welcome");
        assert_eq!(projected.rows[0].is_pruned, pruned);
        assert_eq!(projected.identity_binding, "not_checked");
        let json = serde_json::to_string(&projected).expect("serialize projection");
        for private in [
            "private-client-id",
            "private-ciphertext",
            entry.env.device().unwrap(),
        ] {
            assert!(!json.contains(private));
        }
        assert_eq!(
            projected.rows[0].device_fingerprint,
            Some(hex::encode(Sha256::digest(
                entry.env.device().unwrap().as_bytes()
            )))
        );
    }
}

#[test]
fn unknown_sealed_kind_is_not_inferred_to_be_a_welcome() {
    let e = entry(SealedKind::Unspecified);
    assert_eq!(
        project(wire(&e).as_bytes(), "space").unwrap().rows[0].kind,
        "sealed_unknown"
    );
}

#[test]
fn malformed_partial_empty_and_oversized_inputs_are_closed() {
    for input in ["", "00", "00\n", "not-hex\n"] {
        assert!(project(input.as_bytes(), "space").is_err());
    }
    assert_eq!(
        project(vec![b'a'; MAX_WIRE_BYTES * 2 + 2].as_slice(), "space").err(),
        Some(Rejected::Bounds)
    );
    let original = wire(&entry(SealedKind::Welcome));
    assert_eq!(
        project(original.trim_end().as_bytes(), "space").err(),
        Some(Rejected::Truncated)
    );
    assert_eq!(
        project(original.as_bytes(), "other-space").err(),
        Some(Rejected::Chain)
    );
}

#[test]
fn row_overflow_duplicates_and_chain_or_signature_corruption_are_refused() {
    let original = entry(SealedKind::Welcome);
    let mut previous = types::GENESIS_PREV.to_owned();
    let mut input = String::new();
    for seq in 0..=MAX_ROWS {
        let mut e = original.clone();
        e.seq = seq as u64;
        e.prev = previous;
        e.hash = event_log::chain_hash("space", e.seq, &e.prev, &e.env.wire_hash());
        previous = e.hash.clone();
        input.push_str(&wire(&e));
    }
    assert_eq!(
        project(input.as_bytes(), "space").err(),
        Some(Rejected::Bounds)
    );
    assert_eq!(
        project(wire(&original).repeat(2).as_bytes(), "space").err(),
        Some(Rejected::Chain)
    );
    let mut bad = original.clone();
    bad.hash = "00".repeat(32);
    assert_eq!(
        project(wire(&bad).as_bytes(), "space").err(),
        Some(Rejected::Chain)
    );
    bad = original;
    bad.env.sig = "00".repeat(64);
    bad.hash = event_log::chain_hash("space", bad.seq, &bad.prev, &bad.env.wire_hash());
    assert_eq!(
        project(wire(&bad).as_bytes(), "space").err(),
        Some(Rejected::Signature)
    );
}
