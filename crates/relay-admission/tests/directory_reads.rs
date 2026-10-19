use event_log::{Author, Signer};
use relay_admission::directory::reads;
use serde_json::json;
use types::{Identity, IdentityKind};

fn profile(seed: u8, handle: &str) -> Identity {
    Identity {
        id: Signer::from_secret(&[seed; 32]).id(),
        kind: IdentityKind::Person,
        name: String::new(),
        handle: handle.into(),
        tint_hex: "#123456".into(),
        glyph: None,
        owner: None,
        bio: String::new(),
        owner_proof: None,
    }
}

#[test]
fn native_lookup_normalization_and_literal_prefix_pattern_are_preserved() {
    assert_eq!(reads::normalize_lookup("  @@@ZoEn_Name  "), "zoen_name");
    assert_eq!(reads::normalize_lookup("  @İ  "), "i\u{307}");
    assert_eq!(reads::prefix_pattern("zo%_\\en"), "zo\\_en%");
    assert_eq!(reads::normalize_lookup(" @@ "), "");
}
#[test]
fn native_profile_bound_keeps_first_500_positions_including_duplicates() {
    let ids: Vec<_> = (0..501).map(|i| format!("identity-{i}")).collect();
    assert_eq!(reads::profile_ids(ids.clone()), ids[..500]);
    let repeated = vec!["same".to_string(); 501];
    assert_eq!(reads::profile_ids(repeated).len(), 500);
    assert_eq!(reads::PREFIX_LIMIT, 20);
    assert_eq!(
        (reads::LOOKUP_INTERVAL_NS, reads::LOOKUP_BURST),
        (1_000_000_000, 60)
    );
}
#[test]
fn authoritative_profile_kind_owner_and_id_must_match_the_payload() {
    let p = profile(3, "person");
    assert_eq!(
        reads::authorized_profile(&p.id, "Person", None, json!(p)),
        Some(p.clone())
    );
    assert!(reads::authorized_profile("wrong-id", "Person", None, json!(p)).is_none());
    assert!(reads::authorized_profile(&p.id, "Agent", None, json!(p)).is_none());
    assert!(reads::authorized_profile(&p.id, "Person", Some("wrong-owner"), json!(p)).is_none());
    let mut malformed = json!(p);
    malformed["owner"] = json!("forged-owner");
    assert!(reads::authorized_profile(&p.id, "Person", Some("forged-owner"), malformed).is_none());
    assert!(reads::authorized_profile(&p.id, "Person", None, json!("malformed")).is_none());
    let root = Signer::from_secret(&[4; 32]);
    let owner = Author::device(&root, Signer::from_secret(&[5; 32]));
    let mut agent = profile(6, "agent");
    agent.kind = IdentityKind::Agent;
    agent.owner = Some(owner.identity.clone());
    agent.owner_proof = event_log::agent_owner::authorize(&owner, &agent.id).map(Box::new);
    assert!(
        reads::authorized_profile(&agent.id, "Agent", Some(&owner.identity), json!(agent))
            .is_some()
    );
    agent.owner_proof.as_mut().unwrap().signature = "00".repeat(64);
    assert!(
        reads::authorized_profile(&agent.id, "Agent", Some(&owner.identity), json!(agent))
            .is_none()
    );
}
