use event_log::{Author, Signer};
use relay_admission::closed::{
    reads::{co_members, co_members_json, plan, plan_json, visibility},
    ClosedProjection,
};
use relay_admission::portable::{ObservedHash, SessionContext};
use types::{Privacy, Role, SpaceKind};

fn author(seed: u8) -> Author {
    Author::device(
        &Signer::from_secret(&[seed; 32]),
        Signer::from_secret(&[seed + 1; 32]),
    )
}
fn session(author: &Author) -> SessionContext {
    SessionContext {
        authenticated_identity: author.identity.clone(),
        authenticated_device: author.device.clone().unwrap(),
    }
}
fn state() -> (ClosedProjection, Author, Author, Author) {
    let (owner, peer, stranger) = (author(7), author(17), author(27));
    let mut state = ClosedProjection::empty("sp_read_policy");
    state.head = Some(ObservedHash {
        seq: "100".into(),
        hash: "a".repeat(64),
    });
    state.kind = Some(SpaceKind::Group);
    state.privacy = Some(Privacy::Closed);
    state.creator = owner.identity.clone();
    state.members.insert(owner.identity.clone(), Role::Owner);
    state.members.insert(peer.identity.clone(), Role::Member);
    (state, owner, peer, stranger)
}

#[test]
fn current_member_reads_only_through_the_authoritative_head_and_empty_space_is_read_only() {
    let (state, owner, _, _) = state();
    let read = plan(&state, &session(&owner), 90, 256).unwrap();
    assert_eq!(read.through.as_deref(), Some("100"));
    assert!(read.visibility.member);
    assert!(plan(&state, &session(&owner), 101, 256)
        .unwrap()
        .through
        .is_none());
    let empty = ClosedProjection::empty(&state.space);
    assert!(plan(&empty, &session(&owner), 0, 256)
        .unwrap()
        .through
        .is_none());
}

#[test]
fn removal_is_inclusive_and_never_leaks_a_later_row() {
    let (mut state, _, peer, stranger) = state();
    state.members.remove(&peer.identity);
    state.removed_at.insert(peer.identity.clone(), "37".into());
    assert_eq!(
        plan(&state, &session(&peer), 0, 256)
            .unwrap()
            .through
            .as_deref(),
        Some("37")
    );
    assert_eq!(
        plan(&state, &session(&peer), 37, 256)
            .unwrap()
            .through
            .as_deref(),
        Some("37")
    );
    assert!(plan(&state, &session(&peer), 38, 256)
        .unwrap()
        .through
        .is_none());
    assert!(plan(&state, &session(&stranger), 0, 256)
        .unwrap()
        .through
        .is_none());
    assert_eq!(
        visibility(&state, &session(&peer))
            .unwrap()
            .removed_at
            .as_deref(),
        Some("37")
    );
}

#[test]
fn rejoin_uses_current_membership_and_clears_the_prior_removal_boundary() {
    let (mut state, _, peer, _) = state();
    state.members.remove(&peer.identity);
    state.removed_at.insert(peer.identity.clone(), "37".into());
    // The actual signed Remove/Add integration test below proves these effects;
    // here the pure read planner checks the resulting authoritative state.
    state.removed_at.remove(&peer.identity);
    state.members.insert(peer.identity.clone(), Role::Member);
    assert_eq!(
        plan(&state, &session(&peer), 38, 256)
            .unwrap()
            .through
            .as_deref(),
        Some("100")
    );
    assert!(visibility(&state, &session(&peer))
        .unwrap()
        .removed_at
        .is_none());
}

#[test]
fn legacy_v1_projection_without_removal_field_is_compatible_and_missing_facts_never_grant() {
    let (state, owner, _, stranger) = state();
    let json = serde_json::to_value(&state).unwrap();
    assert!(json.get("removed_at").is_none());
    let restored: ClosedProjection = serde_json::from_value(json).unwrap();
    assert!(plan(&restored, &session(&owner), 0, 1)
        .unwrap()
        .through
        .is_some());
    assert!(plan(&restored, &session(&stranger), 0, 1)
        .unwrap()
        .through
        .is_none());
}

#[test]
fn exact_uint64_bounds_and_scoped_server_context_are_preserved() {
    let (mut state, owner, peer, _) = state();
    state.head.as_mut().unwrap().seq = "9007199254741001".into();
    state.members.remove(&peer.identity);
    state
        .removed_at
        .insert(peer.identity.clone(), "9007199254740993".into());
    let context =
        serde_json::json!({"space_id":state.space,"projection":state,"session":session(&peer)});
    let output: serde_json::Value =
        serde_json::from_str(&plan_json(&context.to_string(), "9007199254740993", 1).unwrap())
            .unwrap();
    assert_eq!(output["through"], "9007199254740993");
    assert!(plan_json(&context.to_string(), "9007199254740994", 1)
        .unwrap()
        .contains("\"through\":null"));
    let mut wrong = context.clone();
    wrong["space_id"] = "sp_other".into();
    assert!(plan_json(&wrong.to_string(), "0", 1).is_err());
    let mut forged = context;
    forged["member"] = true.into();
    assert!(plan_json(&forged.to_string(), "0", 1).is_err());
    assert!(plan(&state, &session(&owner), 0, 0).is_err());
    assert!(plan(&state, &session(&owner), 0, 257).is_err());
    assert!(plan_json("{}", "0", 1).is_err());
}

#[test]
fn inconsistent_or_invalid_retention_state_and_invalid_session_are_refused() {
    let (mut state, owner, peer, _) = state();
    state.removed_at.insert(peer.identity.clone(), "37".into());
    assert!(plan(&state, &session(&owner), 0, 1).is_err());
    state.members.remove(&peer.identity);
    state.removed_at.insert(peer.identity.clone(), "101".into());
    assert!(plan(&state, &session(&owner), 0, 1).is_err());
    state.removed_at.clear();
    let bad = SessionContext {
        authenticated_identity: owner.identity,
        authenticated_device: "not-a-device".into(),
    };
    assert!(plan(&state, &bad, 0, 1).is_err());
}

#[test]
fn profile_co_members_only_current_peers_excludes_self_and_removed_or_unrelated_callers() {
    let (mut state, owner, peer, stranger) = state();
    assert_eq!(
        co_members(&state, &owner.identity).unwrap(),
        vec![peer.identity.clone()]
    );
    assert_eq!(
        co_members(&state, &peer.identity).unwrap(),
        vec![owner.identity.clone()]
    );
    assert!(co_members(&state, &stranger.identity).unwrap().is_empty());
    state.members.remove(&peer.identity);
    state.removed_at.insert(peer.identity.clone(), "37".into());
    assert!(co_members(&state, &peer.identity).unwrap().is_empty());
    assert!(co_members(&state, &owner.identity).unwrap().is_empty());
    state.removed_at.remove(&peer.identity);
    state.members.insert(peer.identity.clone(), Role::Member);
    assert_eq!(
        co_members(&state, &peer.identity).unwrap(),
        vec![owner.identity]
    );
}

#[test]
fn unknown_co_member_lookup_is_empty_and_invalid_or_forged_context_never_grants() {
    let (state, owner, _, _) = state();
    let unknown =
        serde_json::json!({"space_id":state.space,"projection":null,"identity":owner.identity});
    assert_eq!(co_members_json(&unknown.to_string()).unwrap(), "[]");
    let mut forged = unknown;
    forged["member"] = true.into();
    assert!(co_members_json(&forged.to_string()).is_err());
    forged.as_object_mut().unwrap().remove("member");
    forged["identity"] = "bad-identity".into();
    assert!(co_members_json(&forged.to_string()).is_err());
    forged["identity"] = owner.identity.clone().into();
    forged["projection"] = serde_json::to_value(state).unwrap();
    forged["space_id"] = "sp_other".into();
    assert!(co_members_json(&forged.to_string()).is_err());
}

#[test]
fn notification_subject_never_accepts_fabricated_session_or_registration_facts() {
    let (state, owner, peer, _) = state();
    let subject =
        serde_json::json!({"space_id":state.space,"projection":state,"identity":owner.identity});
    let peers: Vec<String> =
        serde_json::from_str(&co_members_json(&subject.to_string()).unwrap()).unwrap();
    assert_eq!(peers, vec![peer.identity]);
    for (key, value) in [
        ("session", serde_json::to_value(session(&owner)).unwrap()),
        ("registered", serde_json::json!(true)),
        ("member", serde_json::json!(true)),
    ] {
        let mut forged = subject.clone();
        forged[key] = value;
        assert!(co_members_json(&forged.to_string()).is_err());
    }
}
