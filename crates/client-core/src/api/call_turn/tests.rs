use super::*;

fn fixture() -> (Arc<Client>, String, String, String, String) {
    let client = Client::open(":memory:".into(), "en".into()).unwrap();
    let mut engine = client.lock();
    engine
        .create_account("Caller", "turn_caller", "http://127.0.0.1:1")
        .unwrap();
    engine.set_registered(true).unwrap();
    let peer = engine
        .create_identity(
            Identity {
                id: String::new(),
                kind: types::IdentityKind::Person,
                name: "Callee".into(),
                handle: "turn_callee".into(),
                tint_hex: "#000000".into(),
                glyph: None,
                owner: None,
                bio: String::new(),
                owner_proof: None,
            },
            true,
        )
        .unwrap();
    let space = engine.start_direct(&peer).unwrap();
    let account = engine.account().unwrap().clone();
    drop(engine);
    (client, account.identity, account.device, space, peer)
}

#[test]
fn current_account_and_exact_encrypted_person_pair_are_required_before_transport_request() {
    let (client, identity, device, space, peer) = fixture();
    assert_eq!(
        scope(&client, &identity, &device, &space, &peer).unwrap(),
        "http://127.0.0.1:1"
    );
    for (who, key, other) in [
        ("foreign", device.as_str(), peer.as_str()),
        (identity.as_str(), "foreign", peer.as_str()),
        (identity.as_str(), device.as_str(), identity.as_str()),
        (identity.as_str(), device.as_str(), "foreign"),
    ] {
        assert!(scope(&client, who, key, &space, other).is_err());
    }
    client
        .lock()
        .append(
            &space,
            &identity,
            EventBody::MemberAdded {
                identity: peer.clone(),
                role: Role::Reader,
            },
        )
        .unwrap();
    assert!(scope(&client, &identity, &device, &space, &peer).is_err());
    client
        .lock()
        .append(
            &space,
            &identity,
            EventBody::MemberRemoved {
                identity: peer.clone(),
            },
        )
        .unwrap();
    assert!(scope(&client, &identity, &device, &space, &peer).is_err());
}

#[tokio::test]
async fn stale_native_owner_fails_before_network_and_credentials_debug_is_redacted() {
    let (client, identity, device, space, peer) = fixture();
    assert!(matches!(
        client
            .call_turn("foreign-owner".into(), device, space, peer)
            .await,
        Err(CoreError::Forbidden { .. })
    ));
    assert_eq!(client.account().unwrap().identity_id, identity);
    let credentials = CallTurnDto {
        urls: vec!["turns:turn.example.test".into()],
        username: "PRIVATE_USERNAME_CANARY".into(),
        credential: "PRIVATE_PASSWORD_CANARY".into(),
        expires_at_ms: 1000,
    };
    let debug = format!("{credentials:?}");
    assert!(!debug.contains("PRIVATE_") && !debug.contains("turn.example.test"));
}
