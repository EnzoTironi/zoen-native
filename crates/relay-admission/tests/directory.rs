use base64::{engine::general_purpose::STANDARD, Engine};
use event_log::{device_cert_message, Signer};
use protocol::{auth_message, ClientFrame, DeviceCertificate, Op, Reply, ServerFrame};
use relay_admission::directory::{self, Device, Facts, Session};
use types::{Identity, IdentityKind};

fn keys() -> (Signer, Signer) {
    (
        Signer::from_secret(&[41; 32]),
        Signer::from_secret(&[42; 32]),
    )
}
fn session() -> Session {
    let (root, device) = keys();
    Session {
        identity: root.id(),
        device: device.id(),
        cert: root.sign(&device_cert_message(&device.id())),
        registered: false,
    }
}
fn profile() -> Identity {
    Identity {
        id: session().identity,
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
fn enrolled(session: &Session) -> Device {
    Device {
        identity: session.identity.clone(),
        cert: session.cert.clone(),
        revoked: false,
    }
}
fn active(session: &Session) -> Facts {
    Facts {
        registered: true,
        device: Some(enrolled(session)),
        profile: Some(profile()),
        ..Default::default()
    }
}
fn req(op: Op) -> Vec<u8> {
    ClientFrame::Req {
        id: 9_007_199_254_740_993,
        op,
    }
    .encode()
}

#[test]
fn protocol_and_domain_match_the_native_handshake_including_root_devices() {
    assert_eq!(protocol::PROTOCOL, "zoen-sync/2");
    let (root, device) = keys();
    let hello = ClientFrame::Hello {
        protocol: 7,
        capabilities: vec![],
        identity: root.id(),
        device: device.id(),
        cert: root.sign(&device_cert_message(&device.id())),
    }
    .encode();
    let accepted = directory::hello(&hello).unwrap();
    let nonce = "a".repeat(64);
    let auth = ClientFrame::Auth {
        sig: device.sign(&auth_message(&nonce, "relay")),
    }
    .encode();
    assert!(!directory::authenticate(&auth, &accepted, &nonce, "other", &Facts::default()).is_ok());
    assert!(directory::authenticate(&auth, &accepted, &nonce, "relay", &Facts::default()).is_ok());
    let context = serde_json::json!({"nonce":nonce,"relay":"relay","now_ms":"1700000000000"});
    let output: serde_json::Value =
        serde_json::from_str(&directory::hello_json(&hello, &context.to_string()).unwrap())
            .unwrap();
    let challenge = STANDARD
        .decode(output["challenge_base64"].as_str().unwrap())
        .unwrap();
    assert!(matches!(
        ServerFrame::decode(&challenge).unwrap(),
        ServerFrame::Challenge { protocol: 4, .. }
    ));
    let own = root.id();
    assert!(directory::certificate_valid(
        &own,
        &own,
        &root.sign(&device_cert_message(&own))
    ));
}

#[test]
fn malformed_old_noncanonical_and_oversized_frames_are_rejected() {
    let s = session();
    let hello = |protocol| {
        ClientFrame::Hello {
            protocol,
            capabilities: vec![],
            identity: s.identity.clone(),
            device: s.device.clone(),
            cert: s.cert.clone(),
        }
        .encode()
    };
    assert!(directory::hello(&hello(3))
        .unwrap_err()
        .reason
        .contains("too old"));
    let mut noncanonical = hello(4);
    noncanonical.extend_from_slice(&[0x78, 0]);
    assert!(directory::hello(&noncanonical).is_err());
    assert!(directory::hello(&vec![0; directory_frame_bound() + 1]).is_err());
    assert!(directory::hello(&[]).is_err());
}
fn directory_frame_bound() -> usize {
    relay_admission::portable::MAX_FRAME_BYTES
}

#[test]
fn root_certificate_cannot_enroll_an_existing_account_and_revoked_login_stays_revoked() {
    let (_, device) = keys();
    let s = session();
    let hello = directory::Hello {
        protocol: 4,
        capabilities: vec![],
        identity: s.identity.clone(),
        device: s.device.clone(),
        cert: s.cert.clone(),
    };
    let auth = ClientFrame::Auth {
        sig: device.sign(&auth_message("nonce", "relay")),
    }
    .encode();
    assert_eq!(
        directory::authenticate(
            &auth,
            &hello,
            "nonce",
            "relay",
            &Facts {
                registered: true,
                ..Default::default()
            }
        )
        .unwrap_err()
        .reason,
        "link this device first"
    );
    let mut facts = active(&s);
    facts.device.as_mut().unwrap().revoked = true;
    assert_eq!(
        directory::authenticate(&auth, &hello, "nonce", "relay", &facts)
            .unwrap_err()
            .reason,
        "this device was unlinked"
    );
}

#[test]
fn native_post_handshake_touch_requires_exact_certificate_and_initial_authorization() {
    let (_, device) = keys();
    let s = session();
    let hello = directory::Hello {
        protocol: 4,
        capabilities: vec![],
        identity: s.identity.clone(),
        device: s.device.clone(),
        cert: s.cert.to_uppercase(),
    };
    assert!(directory::certificate_valid(
        &hello.identity,
        &hello.device,
        &hello.cert
    ));
    let auth = ClientFrame::Auth {
        sig: device.sign(&auth_message("nonce", "relay")),
    }
    .encode();
    assert_eq!(
        directory::authenticate(&auth, &hello, "nonce", "relay", &active(&s))
            .unwrap_err()
            .reason,
        "link this device first"
    );
    let mut facts = Facts {
        device: Some(enrolled(&s)),
        ..Default::default()
    };
    facts.device.as_mut().unwrap().identity = "another-account".into();
    assert_eq!(
        directory::authenticate(&auth, &hello, "nonce", "relay", &facts)
            .unwrap_err()
            .reason,
        "this device was unlinked"
    );
}

#[test]
fn authorization_exactly_retains_native_pending_self_key_exception() {
    for same in [false, true] {
        for revoked in [false, true] {
            for required in [false, true] {
                let row = Some((same, revoked));
                let provisional = directory::provisional_allowed(row, "root", "device", required);
                assert!(!provisional);
                assert_eq!(
                    directory::request_authorized(row, provisional),
                    same && !revoked
                );
            }
        }
    }
    let mut s = session();
    s.identity = s.device.clone();
    let mut facts = Facts {
        device: Some(enrolled(&session())),
        ..Default::default()
    };
    assert!(directory::authorize(&s, &facts).is_ok());
    facts.registered = true;
    assert!(directory::authorize(&s, &facts).is_err());
    facts.registered = false;
    s.registered = true;
    assert!(directory::authorize(&s, &facts).is_err());
}

#[test]
fn first_person_registration_normalizes_without_leaking_clear_profile_and_preserves_u64() {
    let plan = directory::request(
        &req(Op::Register { profile: profile() }),
        &session(),
        &Facts::default(),
    )
    .unwrap();
    let stored: Identity = serde_json::from_value(plan["profile"].clone()).unwrap();
    assert_eq!(stored.handle, "zoen.test");
    assert_eq!(stored.name, "");
    assert_eq!(stored.bio, "");
    let wire = STANDARD
        .decode(plan["reply_base64"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        ServerFrame::decode(&wire).unwrap(),
        ServerFrame::Res {
            id: 9_007_199_254_740_993,
            result: Ok(Reply::Registered(stored))
        }
    );
    let mut bad = profile();
    bad.owner = Some(session().device);
    assert_eq!(
        directory::request(
            &req(Op::Register { profile: bad }),
            &session(),
            &Facts::default()
        )
        .unwrap_err()
        .reason,
        "profile ownership authorization invalid"
    );
}

#[test]
fn stale_provisional_registration_and_handle_collision_cannot_produce_a_write_plan() {
    let s = session();
    let frame = req(Op::Register { profile: profile() });
    assert_eq!(
        directory::request(&frame, &s, &active(&s))
            .unwrap_err()
            .reason,
        "account already registered; link this device first"
    );
    let facts = Facts {
        handle_owner: Some("other".into()),
        ..Default::default()
    };
    assert_eq!(
        directory::request(&frame, &s, &facts).unwrap_err().reason,
        "handle_taken"
    );
}

#[test]
fn sponsor_authorization_and_exact_active_target_retry_are_both_required() {
    let mut s = session();
    s.registered = true;
    let mut facts = active(&s);
    let (root, _) = keys();
    let target = Signer::from_secret(&[43; 32]);
    let cert = root.sign(&device_cert_message(&target.id()));
    let frame = req(Op::DeliverLink {
        id: "a".repeat(64),
        sealed: vec![1, 2],
        device: Some(DeviceCertificate {
            device: target.id(),
            cert: cert.clone(),
        }),
    });
    facts.target = Some(Device {
        identity: s.identity.clone(),
        cert,
        revoked: false,
    });
    assert_eq!(
        directory::request(&frame, &s, &facts).unwrap()["action"],
        "deliver_link"
    );
    facts.target.as_mut().unwrap().revoked = true;
    assert_eq!(
        directory::request(&frame, &s, &facts).unwrap_err().reason,
        "device is revoked or belongs to another account"
    );
    facts.target = None;
    facts.device.as_mut().unwrap().revoked = true;
    assert_eq!(
        directory::request(&frame, &s, &facts).unwrap_err().reason,
        "this device was unlinked"
    );
}

#[test]
fn revoked_devices_cannot_unlink_or_fetch_or_replay_requests() {
    let mut s = session();
    s.registered = true;
    let mut facts = active(&s);
    facts.device.as_mut().unwrap().revoked = true;
    for op in [
        Op::FetchLink { id: "a".repeat(64) },
        Op::Unlink {
            device: s.device.clone(),
        },
        Op::Devices,
    ] {
        assert_eq!(
            directory::request(&req(op), &s, &facts).unwrap_err().reason,
            "this device was unlinked"
        );
    }
}

#[test]
fn pending_self_key_fetch_survives_assignment_but_registration_cannot_claim_that_device() {
    let (root, device) = keys();
    let s = Session {
        identity: device.id(),
        device: device.id(),
        cert: device.sign(&device_cert_message(&device.id())),
        registered: false,
    };
    let facts = Facts {
        device: Some(Device {
            identity: root.id(),
            cert: root.sign(&device_cert_message(&device.id())),
            revoked: false,
        }),
        link_box: Some(vec![3]),
        ..Default::default()
    };
    assert_eq!(
        directory::request(&req(Op::FetchLink { id: "b".repeat(64) }), &s, &facts).unwrap()
            ["action"],
        "fetch_link"
    );
    let mut person = profile();
    person.id = s.identity.clone();
    assert_eq!(
        directory::request(&req(Op::Register { profile: person }), &s, &facts)
            .unwrap_err()
            .reason,
        "device is revoked or belongs to another account"
    );
}

#[test]
fn unsupported_operations_are_explicit_and_malformed_trusted_context_is_retryable() {
    assert_eq!(
        directory::lookup(&req(Op::RegisterPush {
            platform: "apns".into(),
            token: String::new()
        }))
        .unwrap_err()
        .code,
        "unsupported_directory"
    );
    let error =
        directory::request_json(&req(Op::FetchLink { id: "b".repeat(64) }), "{}").unwrap_err();
    assert!(!error.permanent);
    assert!(!directory::enrollment_certificate_valid(
        "bad", "bad", "bad"
    ));
}
