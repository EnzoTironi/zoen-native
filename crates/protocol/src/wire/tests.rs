use super::*;
use crate::KeyPackageClaimClock;
use event_log::{Author, Signer, SpaceLog};
use types::{EventBody, Privacy, SpaceKind};

fn signed() -> (Envelope, SpaceLog) {
    let root = Signer::generate();
    let phone = Author::device(&root, Signer::generate());
    let mut log = SpaceLog::new("sp_1");
    let e = phone.sign_event(
        "sp_1",
        "01J",
        5,
        None,
        EventBody::SpaceCreated {
            title: "x".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
    );
    log.sequence(e.clone());
    (Envelope::plain(&e), log)
}

#[test]
fn a_published_envelope_crosses_the_wire_byte_for_byte_and_verifies() {
    let (env, _) = signed();
    let back = ClientFrame::decode(&ClientFrame::Publish { env: env.clone() }.encode()).unwrap();
    let ClientFrame::Publish { env: back } = back else {
        panic!("publish")
    };
    assert_eq!(back.content(), env.content());
    assert!(back.verify().is_ok());
    let mut forged = back.content().to_vec();
    let at = forged.len() - 3;
    forged[at] ^= 1;
    let forged = Envelope::new(forged, back.sig.clone(), back.cert.clone(), None);
    assert!(forged.is_none_or(|f| f.verify().is_err()));
}

fn signed_statement() -> DeviceSigned {
    DeviceSigned {
        device: "d".into(),
        sig: "s".into(),
        cert: "c".into(),
    }
}

fn sealed_profile() -> SealedProfile {
    SealedProfile {
        identity: "i".into(),
        version: 3,
        ciphertext: vec![1, 2, 3],
        signed: signed_statement(),
    }
}

#[test]
fn every_server_frame_roundtrips() {
    let (env, log) = signed();
    let first = &log.events()[0];
    let profile = Identity {
        owner_proof: None,
        id: "ab".into(),
        kind: IdentityKind::Agent,
        name: "Pousada".into(),
        handle: "pousada".into(),
        tint_hex: "#fff".into(),
        glyph: Some("bed.double".into()),
        owner: Some("cd".into()),
        bio: String::new(),
    };
    let frames = vec![
        ServerFrame::Challenge {
            nonce: "n".into(),
            relay: "r".into(),
            protocol: 2,
            capabilities: vec!["blobs".into()],
            server_time_ms: None,
        },
        ServerFrame::Ready {
            identity: "i".into(),
            registered: true,
        },
        ServerFrame::Res {
            id: 7,
            result: Ok(Reply::Profiles(vec![profile.clone()])),
        },
        ServerFrame::Res {
            id: 8,
            result: Ok(Reply::Registered(profile.clone())),
        },
        ServerFrame::Res {
            id: 9,
            result: Err("nope".into()),
        },
        ServerFrame::Res {
            id: 10,
            result: Ok(Reply::Preview(InvitePreview {
                space: "sp".into(),
                role: Role::Reader,
                title: "t".into(),
                members: 3,
                inviter: Some(profile),
                kind: "space".into(),
                agent: None,
            })),
        },
        ServerFrame::Res {
            id: 11,
            result: Ok(Reply::Invite(InviteCreated {
                code: "ABCD".into(),
                expires_at_ms: 9,
            })),
        },
        ServerFrame::Event {
            ev: Sequenced {
                seq: 0,
                prev: first.prev.clone(),
                hash: first.hash.clone(),
                env,
            },
        },
        ServerFrame::Accepted {
            space: "sp".into(),
            client_id: "c".into(),
            seq: 4,
        },
        ServerFrame::Rejected {
            space: "sp".into(),
            client_id: "c".into(),
            reason: "slow down".into(),
            permanent: false,
        },
        ServerFrame::Ephemeral {
            space: "sp".into(),
            from: "f".into(),
            kind: EphemeralKind::Read { seq: 3 },
        },
        ServerFrame::Presence {
            identity: "i".into(),
            online: false,
        },
        ServerFrame::Res {
            id: 12,
            result: Ok(Reply::Done),
        },
        ServerFrame::Res {
            id: 13,
            result: Ok(Reply::AgreementKeys(vec![AgreementKeyRecord {
                identity: "i".into(),
                public: "p".into(),
                signed: signed_statement(),
            }])),
        },
        ServerFrame::Res {
            id: 14,
            result: Ok(Reply::SealedProfiles(vec![sealed_profile()])),
        },
        ServerFrame::Res {
            id: 15,
            result: Ok(Reply::KeyPackages(vec![KeyPackageRecord {
                identity: "i".into(),
                device: "d".into(),
                data: vec![1, 2],
            }])),
        },
        ServerFrame::ProfileChanged {
            identity: "i".into(),
            version: 4,
        },
        ServerFrame::KeyPackagesLow {
            device: "d".into(),
            remaining: 3,
        },
        ServerFrame::DeviceMessage {
            from: "a".into(),
            to: "b".into(),
            sealed: vec![7],
        },
        ServerFrame::Joined { space: "sp".into() },
        ServerFrame::SyncDone,
        ServerFrame::Pong {
            server_time_ms: None,
        },
        ServerFrame::error(ErrorCode::UpgradeRequired, "update"),
    ];
    for f in frames {
        assert_eq!(ServerFrame::decode(&f.encode()).unwrap(), f);
    }
}

#[test]
fn every_client_frame_roundtrips() {
    let frames = vec![
        ClientFrame::Hello {
            protocol: 2,
            capabilities: vec!["presence".into()],
            identity: "i".into(),
            device: "d".into(),
            cert: "c".into(),
        },
        ClientFrame::Auth { sig: "s".into() },
        ClientFrame::Req {
            id: 1,
            op: Op::Lookup {
                handle: "an".into(),
                prefix: true,
            },
        },
        ClientFrame::Req {
            id: 2,
            op: Op::CreateInvite {
                space: "sp".into(),
                agent: None,
                role: Role::Admin,
                max_uses: 5,
                ttl_secs: 60,
            },
        },
        ClientFrame::Req {
            id: 3,
            op: Op::PreviewInvite { code: "X".into() },
        },
        ClientFrame::Req {
            id: 31,
            op: Op::RegisterPush {
                platform: "fcm".into(),
                token: "t".into(),
            },
        },
        ClientFrame::Req {
            id: 30,
            op: Op::RedeemAgentInvite { code: "X".into() },
        },
        ClientFrame::Req {
            id: 31,
            op: Op::RevokeAgentInvite { agent: "a".into() },
        },
        ClientFrame::Req {
            id: 4,
            op: Op::Profiles {
                ids: vec!["a".into()],
            },
        },
        ClientFrame::Req {
            id: 5,
            op: Op::PublishAgreementKey {
                public: "p".into(),
                signed: signed_statement(),
            },
        },
        ClientFrame::Req {
            id: 6,
            op: Op::AgreementKeys {
                ids: vec!["a".into()],
            },
        },
        ClientFrame::Req {
            id: 7,
            op: Op::PutProfile {
                profile: sealed_profile(),
            },
        },
        ClientFrame::Req {
            id: 8,
            op: Op::GetProfiles {
                ids: vec!["a".into()],
            },
        },
        ClientFrame::Req {
            id: 9,
            op: Op::PublishKeyPackages {
                packages: vec![vec![1], vec![2]],
                last_resort: Some(vec![3]),
            },
        },
        ClientFrame::Req {
            id: 10,
            op: Op::ClaimKeyPackages {
                ids: vec!["a".into()],
                operation_id: None,
            },
        },
        ClientFrame::Req {
            id: 11,
            op: Op::DeliverLink {
                id: "identity-box".into(),
                sealed: vec![1, 2],
                device: Some(DeviceCertificate {
                    device: "d".into(),
                    cert: "c".into(),
                }),
            },
        },
        ClientFrame::Req {
            id: 12,
            op: Op::DeliverLink {
                id: "history-box".into(),
                sealed: vec![3, 4],
                device: None,
            },
        },
        ClientFrame::Sync {
            cursors: vec![Cursor {
                space: "sp".into(),
                next_seq: 9,
            }],
            all: true,
        },
        ClientFrame::Ephemeral {
            space: "sp".into(),
            kind: EphemeralKind::Status {
                status: "building".into(),
            },
        },
        ClientFrame::Ping,
    ];
    for f in frames {
        assert_eq!(ClientFrame::decode(&f.encode()).unwrap(), f);
    }
}

#[test]
fn a_legacy_link_box_does_not_enroll_a_device() {
    // Pre-enrollment protobuf: Req{id:1, DeliverLink{id:"box", sealed:[1]}}.
    let legacy = [
        0x1a, 0x0c, 0x08, 0x01, 0x6a, 0x08, 0x0a, 0x03, b'b', b'o', b'x', 0x12, 0x01, 0x01,
    ];
    assert_eq!(
        ClientFrame::decode(&legacy).unwrap(),
        ClientFrame::Req {
            id: 1,
            op: Op::DeliverLink {
                id: "box".into(),
                sealed: vec![1],
                device: None,
            },
        }
    );
}

#[test]
fn historical_profiles_and_new_owner_proofs_roundtrip() {
    // Legacy Person profile, protobuf tags 1 and 4 only.
    let legacy = [0x0a, 0x01, b'i', 0x22, 0x01, b'h'];
    let person = profile_from(PbProfile::decode(&legacy[..]).unwrap()).unwrap();
    assert!(person.owner_proof.is_none());
    assert_eq!(profile_to(&person).encode_to_vec(), legacy);
    let mut agent = person;
    agent.kind = IdentityKind::Agent;
    agent.owner = Some("owner".into());
    agent.owner_proof = Some(Box::new(types::AgentOwnerProof {
        device: "device".into(),
        cert: "certificate".into(),
        signature: "signature".into(),
    }));
    let request = ClientFrame::Req {
        id: 1,
        op: Op::Register {
            profile: agent.clone(),
        },
    };
    assert_eq!(ClientFrame::decode(&request.encode()).unwrap(), request);
    let response = ServerFrame::Res {
        id: 1,
        result: Ok(Reply::Profiles(vec![agent])),
    };
    assert_eq!(ServerFrame::decode(&response.encode()).unwrap(), response);
}

#[test]
fn legacy_claim_is_byte_compatible_and_receipt_id_round_trips() {
    let legacy = [
        0x1a, 0x09, 0x08, 0x01, 0x62, 0x05, 0x0a, 0x03, b'b', b'o', b'b',
    ];
    let frame = ClientFrame::Req {
        id: 1,
        op: Op::ClaimKeyPackages {
            ids: vec!["bob".into()],
            operation_id: None,
        },
    };
    assert_eq!(ClientFrame::decode(&legacy).unwrap(), frame);
    assert_eq!(frame.encode(), legacy);
    let durable = ClientFrame::Req {
        id: 1,
        op: Op::ClaimKeyPackages {
            ids: vec!["bob".into()],
            operation_id: Some("01K75MYBPQ0000000000000001".into()),
        },
    };
    assert_eq!(ClientFrame::decode(&durable.encode()).unwrap(), durable);
    // Protocol 4 alone carries no promise of receipt support.
    let hello = ClientFrame::Hello {
        protocol: 4,
        capabilities: Vec::new(),
        identity: "i".into(),
        device: "d".into(),
        cert: "c".into(),
    };
    assert_eq!(ClientFrame::decode(&hello.encode()).unwrap(), hello);
}

#[test]
fn optional_relay_clock_preserves_legacy_challenge_and_pong_bytes() {
    let challenge = ServerFrame::Challenge {
        nonce: "n".into(),
        relay: "r".into(),
        protocol: 4,
        capabilities: Vec::new(),
        server_time_ms: None,
    };
    let old_challenge = [0x0a, 0x08, 0x0a, 0x01, b'n', 0x12, 0x01, b'r', 0x18, 0x04];
    assert_eq!(challenge.encode(), old_challenge);
    assert_eq!(ServerFrame::decode(&old_challenge).unwrap(), challenge);
    let old_pong = [0x5a, 0x00];
    assert_eq!(
        ServerFrame::Pong {
            server_time_ms: None
        }
        .encode(),
        old_pong
    );
    assert_eq!(
        ServerFrame::decode(&old_pong).unwrap(),
        ServerFrame::Pong {
            server_time_ms: None
        }
    );
    for time in [None, Some(-1), Some(1_800_000_000_000)] {
        let pong = ServerFrame::Pong {
            server_time_ms: time,
        };
        assert_eq!(ServerFrame::decode(&pong.encode()).unwrap(), pong);
    }
    assert!(KeyPackageClaimClock::from_server_ms(-1).is_none());
    assert!(KeyPackageClaimClock::from_server_ms(0).is_none());
    assert!(KeyPackageClaimClock::from_server_ms(1i64 << 48).is_none());
}

#[test]
fn garbage_never_panics_the_decoder() {
    for bytes in [
        &[][..],
        &[0xff, 0xff, 0xff],
        &[0x0a, 0x05, 0x01],
        &[0x22, 0x02, 0x0a, 0x00],
    ] {
        let _ = ClientFrame::decode(bytes);
        let _ = ServerFrame::decode(bytes);
    }
    assert!(ClientFrame::decode(&[]).is_err());
}

#[test]
fn agent_requests_round_trip() {
    use crate::{AgentAction, AgentRequest, AgentStateRecord};
    for op in [
        Op::Agent(AgentRequest::State),
        Op::Agent(AgentRequest::Status {
            agent: "bot".into(),
        }),
        Op::Agent(AgentRequest::Advance {
            space: "sp".into(),
            seq: 7,
        }),
        Op::Agent(AgentRequest::Define {
            agent: "a".into(),
            version: 2,
            kind: "User".into(),
            definition: b"{}".to_vec(),
        }),
        Op::Agent(AgentRequest::Control {
            agent: "a".into(),
            action: AgentAction::Revoke,
        }),
    ] {
        let f = ClientFrame::Req { id: 1, op };
        assert_eq!(ClientFrame::decode(&f.encode()).unwrap(), f);
    }
    let r = ServerFrame::Res {
        id: 1,
        result: Ok(Reply::AgentState(AgentStateRecord {
            status: "paused".into(),
            kind: "Creator".into(),
            definition_version: 3,
            definition: b"{}".to_vec(),
            cursors: vec![("sp".into(), 9)],
        })),
    };
    assert_eq!(ServerFrame::decode(&r.encode()).unwrap(), r);
}
