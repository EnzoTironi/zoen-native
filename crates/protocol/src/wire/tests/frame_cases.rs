use super::*;

#[must_use]
pub(super) fn server_handshake_and_invite_cases(profile: Identity) -> Vec<ServerFrame> {
    vec![
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
    ]
}

#[must_use]
pub(super) fn server_event_and_directory_cases(
    env: Envelope,
    first: &types::Event,
) -> Vec<ServerFrame> {
    vec![
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
    ]
}

#[must_use]
pub(super) fn client_discovery_cases() -> Vec<ClientFrame> {
    vec![
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
    ]
}

#[must_use]
pub(super) fn client_agreement_and_device_cases() -> Vec<ClientFrame> {
    vec![
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
    ]
}
