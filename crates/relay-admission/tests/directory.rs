use event_log::{device_cert_message, Signer};
use protocol::auth_message;
use relay_admission::directory;
use types::{Identity, IdentityKind};

fn profile(id: String) -> Identity {
    Identity {
        id,
        kind: IdentityKind::Person,
        name: "private name".into(),
        handle: "@Zoen.Test".into(),
        tint_hex: "#123456".into(),
        glyph: None,
        owner: None,
        bio: "private bio".into(),
        owner_proof: None,
    }
}
#[test]
fn native_root_and_certified_device_proofs_bind_identity_device_nonce_and_relay() {
    let root = Signer::from_secret(&[41; 32]);
    let device = Signer::from_secret(&[42; 32]);
    let cert = root.sign(&device_cert_message(&device.id()));
    assert!(directory::certificate_valid(
        &root.id(),
        &device.id(),
        &cert
    ));
    assert!(directory::enrollment_certificate_valid(
        &root.id(),
        &device.id(),
        &cert
    ));
    assert!(directory::certificate_valid(
        &root.id(),
        &root.id(),
        &root.sign(&device_cert_message(&root.id()))
    ));
    assert!(!directory::certificate_valid(
        &device.id(),
        &device.id(),
        &cert
    ));
    assert!(!directory::certificate_valid(&root.id(), &root.id(), &cert));
    assert!(!directory::enrollment_certificate_valid(
        &root.id(),
        "not-a-device",
        &cert
    ));
    assert!(!directory::certificate_valid(
        &root.id(),
        &device.id(),
        &"00".repeat(64)
    ));
    let sig = device.sign(&auth_message("nonce", "relay"));
    assert!(directory::auth_valid(&device.id(), "nonce", "relay", &sig));
    assert!(!directory::auth_valid(
        &device.id(),
        "nonce",
        "other-relay",
        &sig
    ));
    assert!(!directory::auth_valid(
        &device.id(),
        "other-nonce",
        "relay",
        &sig
    ));
    assert!(!directory::auth_valid(&root.id(), "nonce", "relay", &sig));
}
#[test]
fn exact_native_provisional_and_revocation_truth_table_is_preserved() {
    for enrolled in [
        None,
        Some((false, false)),
        Some((true, false)),
        Some((false, true)),
        Some((true, true)),
    ] {
        for same_key in [false, true] {
            for required in [false, true] {
                let (identity, device) = if same_key {
                    ("same", "same")
                } else {
                    ("root", "device")
                };
                let expected = !required
                    && enrolled.is_none_or(|(_, revoked)| !revoked)
                    && (enrolled.is_none() || same_key);
                let provisional =
                    directory::provisional_allowed(enrolled, identity, device, required);
                assert_eq!(provisional, expected);
                assert_eq!(
                    directory::request_authorized(enrolled, provisional),
                    enrolled == Some((true, false)) || provisional
                );
            }
        }
    }
}
#[test]
fn native_person_normalization_hides_private_profile_fields_and_refuses_other_identity() {
    let id = Signer::from_secret(&[41; 32]).id();
    let p = profile(id.clone());
    let normalized = directory::prepare_profile(p.clone(), &id).unwrap();
    assert_eq!(normalized.handle, "zoen.test");
    assert!(normalized.name.is_empty());
    assert!(normalized.bio.is_empty());
    assert_eq!(normalized.tint_hex, p.tint_hex);
    assert!(directory::prepare_profile(p, "other").is_err());
    let mut bad = profile(id.clone());
    bad.handle = "!".into();
    assert!(directory::prepare_profile(bad, &id).is_err());
}
#[test]
fn native_agent_profile_requires_owner_and_name_and_keeps_character_bounds() {
    let id = Signer::from_secret(&[41; 32]).id();
    let mut p = profile(id.clone());
    p.kind = IdentityKind::Agent;
    p.owner = Some("owner".into());
    p.name = format!("  {}  ", "é".repeat(70));
    p.bio = "á".repeat(300);
    let normalized = directory::prepare_profile(p.clone(), &id).unwrap();
    assert_eq!(normalized.name.chars().count(), 64);
    assert_eq!(normalized.bio.chars().count(), 280);
    p.owner = None;
    assert!(directory::prepare_profile(p.clone(), &id).is_err());
    p.owner = Some("owner".into());
    p.name = " ".into();
    assert!(directory::prepare_profile(p, &id).is_err());
}
