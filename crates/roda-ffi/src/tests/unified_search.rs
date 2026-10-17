use super::*;

/// Search may show someone from a shared group, but choosing them must create a
/// direct conversation, not route to that group. Authentication uses the public
/// account API and an in-memory vault; no demo author or transport is forged.
#[test]
fn authenticated_search_recipient_creates_and_reuses_a_real_direct_not_a_shared_group() {
    let owner = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
    let peer = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
    owner
        .create_account(
            "Owner".into(),
            "searchowner".into(),
            "http://127.0.0.1:9".into(),
            Arc::new(MemVault::default()),
        )
        .unwrap();
    peer.create_account(
        "Peer".into(),
        "searchpeer".into(),
        "http://127.0.0.1:9".into(),
        Arc::new(MemVault::default()),
    )
    .unwrap();
    let peer_id = peer.me().unwrap().id;
    let profile = peer.lock().identities[&peer_id].clone();
    // Directory replies enter this same projection. This is public profile data,
    // never the peer's signing keys or an authentication shortcut.
    owner.lock().put_profiles(vec![profile]).unwrap();
    let shared = owner
        .create_group_with(
            "Shared group".into(),
            vec![peer_id.clone()],
            PrivacyDto::Closed,
        )
        .unwrap();
    let before = owner.spaces().len();
    let direct = owner.start_direct(peer_id.clone()).unwrap();
    assert_ne!(direct, shared);
    let space = owner.spaces().into_iter().find(|s| s.id == direct).unwrap();
    assert_eq!(space.kind, SpaceKindDto::Direct);
    assert_eq!(space.privacy, PrivacyDto::EndToEnd);
    assert_eq!(space.counterpart.unwrap().id, peer_id);
    assert_eq!(owner.spaces().len(), before + 1);
    assert_eq!(owner.start_direct(peer_id).unwrap(), direct);
    assert_eq!(owner.spaces().len(), before + 1);
}

#[test]
fn demo_search_recipient_without_a_direct_requires_account_and_does_not_create_a_group() {
    let core = seeded();
    let spaces = core.spaces();
    let peer = core
        .personas()
        .into_iter()
        .find(|p| {
            !p.is_me
                && p.kind == PersonaKind::Person
                && !spaces
                    .iter()
                    .any(|s| s.counterpart.as_ref().is_some_and(|other| other.id == p.id))
        })
        .unwrap();
    assert!(matches!(
        core.start_direct(peer.id),
        Err(CoreError::Forbidden { .. })
    ));
    assert_eq!(core.spaces().len(), spaces.len());
}
