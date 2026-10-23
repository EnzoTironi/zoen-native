use super::*;
use event_log::Signer;
use protocol::sites::{Mutation, Receipt, SourceVersion};

const NOW: i64 = 1_800_000_000_000;
const AUDIENCE: &str = "relay.example.test";
const HTML: &[u8] = b"<!doctype html><title>Private draft chosen explicitly</title>";

fn fixture() -> (Signer, SignedMutation) {
    let device = Signer::from_secret(&[23; 32]);
    let mut signed = SignedMutation {
        owner_id: Signer::from_secret(&[24; 32]).id(),
        device_id: device.id(),
        at_ms: NOW,
        mutation: Mutation {
            schema: SCHEMA,
            public_origin: "https://sites.example.test".into(),
            site_id: format!("site_{}", "a".repeat(32)),
            request_id: format!("pub_{}", "b".repeat(32)),
            expected_head: None,
            change: Change::Publish {
                source: SourceVersion {
                    space_id: "space_selected".into(),
                    item_id: "item_selected".into(),
                    version: 3,
                    sha256: event_log::sha256_hex(HTML),
                    byte_length: HTML.len() as u64,
                },
            },
        },
        signature: String::new(),
    };
    sign(&device, &mut signed);
    (device, signed)
}

fn sign(device: &Signer, signed: &mut SignedMutation) {
    signed.signature = device.sign(&signed.signing_bytes(AUDIENCE).unwrap());
}

#[test]
fn selected_exact_bytes_and_receipt_are_deterministic() {
    let (_, signed) = fixture();
    assert_eq!(authorize(&signed, AUDIENCE, NOW), Ok(()));
    let key = verify_content(&signed, Some(HTML)).unwrap().unwrap();
    assert_eq!(
        key,
        format!(
            "public-sites/v1/{}/{}",
            signed.mutation.site_id,
            event_log::sha256_hex(HTML)
        )
    );
    let receipt = Receipt::for_mutation(&signed, AUDIENCE).unwrap();
    assert!(receipt.active);
    assert_eq!(receipt.source.as_ref().unwrap().version, 3);
    assert_eq!(receipt.head, signed.intent_digest(AUDIENCE).unwrap());
    assert_eq!(
        serde_json::from_slice::<Receipt>(&serde_json::to_vec(&receipt).unwrap()).unwrap(),
        receipt
    );
}

#[test]
fn signature_binds_audience_owner_device_destination_version_and_cas() {
    let (_, signed) = fixture();
    assert_eq!(
        authorize(&signed, "another-relay", NOW),
        Err(Failure::Signature)
    );
    for field in 0..9 {
        let mut altered = signed.clone();
        match field {
            0 => altered.owner_id = "c".repeat(64),
            1 => altered.device_id = Signer::from_secret(&[25; 32]).id(),
            2 => altered.mutation.site_id = format!("site_{}", "c".repeat(32)),
            3 => altered.mutation.request_id = format!("pub_{}", "c".repeat(32)),
            4 => altered.mutation.expected_head = Some("c".repeat(64)),
            5 => altered.at_ms += 1,
            6 => {
                let Change::Publish { source } = &mut altered.mutation.change else {
                    unreachable!()
                };
                source.version += 1;
            }
            7 => altered.mutation.change = Change::Unpublish,
            _ => altered.mutation.public_origin = "https://another.example.test".into(),
        }
        assert!(authorize(&altered, AUDIENCE, NOW).is_err(), "field {field}");
    }
}

#[test]
fn renewal_and_linked_device_recover_same_intent_but_new_action_changes_head() {
    let (device, signed) = fixture();
    let mut renewed = signed.clone();
    renewed.at_ms += 300_000;
    sign(&device, &mut renewed);
    assert_eq!(authorize(&renewed, AUDIENCE, renewed.at_ms), Ok(()));
    assert_eq!(
        renewed.intent_digest(AUDIENCE).unwrap(),
        signed.intent_digest(AUDIENCE).unwrap()
    );
    let linked = Signer::from_secret(&[25; 32]);
    renewed.device_id = linked.id();
    sign(&linked, &mut renewed);
    assert_eq!(authorize(&renewed, AUDIENCE, renewed.at_ms), Ok(()));
    assert_eq!(
        renewed.intent_digest(AUDIENCE).unwrap(),
        signed.intent_digest(AUDIENCE).unwrap()
    );
    renewed.mutation.request_id = format!("pub_{}", "d".repeat(32));
    assert_ne!(
        renewed.intent_digest(AUDIENCE).unwrap(),
        signed.intent_digest(AUDIENCE).unwrap()
    );
}

#[test]
fn freshness_has_inclusive_boundary_and_no_signed_integer_overflow() {
    let (device, signed) = fixture();
    assert_eq!(authorize(&signed, AUDIENCE, NOW + 300_000), Ok(()));
    assert_eq!(authorize(&signed, AUDIENCE, NOW - 300_000), Ok(()));
    assert_eq!(
        authorize(&signed, AUDIENCE, NOW + 300_001),
        Err(Failure::Stale)
    );
    for timestamp in [i64::MIN, -1, 0, i64::MAX] {
        let mut altered = signed.clone();
        altered.at_ms = timestamp;
        sign(&device, &mut altered);
        assert_eq!(authorize(&altered, AUDIENCE, NOW), Err(Failure::Stale));
    }
}

#[test]
fn changed_missing_extra_or_non_utf8_content_cannot_be_published() {
    let (_, signed) = fixture();
    assert_eq!(verify_content(&signed, None), Err(Failure::Content));
    assert_eq!(
        verify_content(&signed, Some(b"newer private draft")),
        Err(Failure::Content)
    );
    for invalid in [
        vec![0],
        vec![0xff],
        vec![],
        vec![b'a'; MAX_HTML_BYTES as usize + 1],
    ] {
        let mut altered = signed.clone();
        let Change::Publish { source } = &mut altered.mutation.change else {
            unreachable!()
        };
        source.byte_length = invalid.len() as u64;
        source.sha256 = event_log::sha256_hex(&invalid);
        assert_eq!(
            verify_content(&altered, Some(&invalid)),
            Err(Failure::Content)
        );
    }
}

#[test]
fn malformed_scope_and_versions_fail_even_with_genuine_signature() {
    let (device, signed) = fixture();
    for field in 0..9 {
        let mut altered = signed.clone();
        match field {
            0 => altered.mutation.schema = 2,
            1 => altered.mutation.site_id = "../private".into(),
            2 => altered.mutation.request_id = "pub_".into(),
            3 => altered.mutation.expected_head = Some("AA".repeat(32)),
            field => {
                let Change::Publish { source } = &mut altered.mutation.change else {
                    unreachable!()
                };
                match field {
                    4 => source.item_id = "../item".into(),
                    5 => source.space_id.clear(),
                    6 => source.version = 0,
                    7 => source.byte_length = MAX_HTML_BYTES + 1,
                    _ => source.sha256 = "not a digest".into(),
                }
            }
        }
        sign(&device, &mut altered);
        assert_eq!(
            authorize(&altered, AUDIENCE, NOW),
            Err(Failure::Invalid),
            "field {field}"
        );
    }
}

#[test]
fn unpublish_and_rollback_are_new_signed_compare_and_swap_actions() {
    let (device, initial) = fixture();
    let mut removal = initial.clone();
    removal.mutation.change = Change::Unpublish;
    sign(&device, &mut removal);
    assert_eq!(authorize(&removal, AUDIENCE, NOW), Err(Failure::Invalid));
    removal.mutation.expected_head = Some(initial.intent_digest(AUDIENCE).unwrap());
    removal.mutation.request_id = format!("pub_{}", "e".repeat(32));
    sign(&device, &mut removal);
    assert_eq!(authorize(&removal, AUDIENCE, NOW), Ok(()));
    assert_eq!(verify_content(&removal, None), Ok(None));
    assert_eq!(verify_content(&removal, Some(HTML)), Err(Failure::Content));
    assert!(!Receipt::for_mutation(&removal, AUDIENCE).unwrap().active);
    let mut rollback = initial;
    rollback.mutation.expected_head = Some(removal.intent_digest(AUDIENCE).unwrap());
    rollback.mutation.request_id = format!("pub_{}", "f".repeat(32));
    sign(&device, &mut rollback);
    assert_eq!(authorize(&rollback, AUDIENCE, NOW), Ok(()));
    assert!(verify_content(&rollback, Some(HTML)).unwrap().is_some());
}

#[test]
fn inspection_is_signed_in_its_own_domain_with_destination_and_cursor() {
    let (device, signed) = fixture();
    let mut read = protocol::sites::SignedInspection {
        owner_id: signed.owner_id.clone(),
        device_id: signed.device_id.clone(),
        at_ms: NOW,
        site_id: signed.mutation.site_id.clone(),
        public_origin: signed.mutation.public_origin.clone(),
        before: None,
        signature: String::new(),
    };
    read.signature = device.sign(&read.signing_bytes(AUDIENCE).unwrap());
    assert_eq!(authorize_inspection(&read, AUDIENCE, NOW), Ok(()));
    for field in 0..4 {
        let mut altered = read.clone();
        match field {
            0 => altered.before = Some("a".repeat(64)),
            1 => altered.public_origin = "https://other.example.test".into(),
            2 => altered.signature = signed.signature.clone(),
            _ => altered.site_id = format!("site_{}", "d".repeat(32)),
        }
        assert_eq!(
            authorize_inspection(&altered, AUDIENCE, NOW),
            Err(Failure::Signature)
        );
    }
    assert_eq!(
        authorize_inspection(&read, AUDIENCE, NOW + 300_001),
        Err(Failure::Stale)
    );
}
