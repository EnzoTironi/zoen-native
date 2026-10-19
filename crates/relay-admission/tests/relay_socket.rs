use base64::{engine::general_purpose::STANDARD, Engine};
use protocol::{ClientFrame, Cursor, ServerFrame};
use relay_admission::relay_socket;
use serde_json::{json, Value};
fn wire(value: Value) -> Vec<u8> {
    let json: Value =
        serde_json::from_str(&relay_socket::control_json(&value.to_string()).unwrap()).unwrap();
    STANDARD
        .decode(json["frame_base64"].as_str().unwrap())
        .unwrap()
}
#[test]
fn actual_native_ack_reject_joined_and_done_preserve_complete_uint64() {
    for seq in [0, 9_007_199_254_740_993, u64::MAX] {
        assert_eq!(
            wire(
                json!({"kind":"accepted","space":"space","client_id":"cid","seq":seq.to_string()})
            ),
            ServerFrame::Accepted {
                space: "space".into(),
                client_id: "cid".into(),
                seq
            }
            .encode()
        );
    }
    assert_eq!(
        wire(
            json!({"kind":"rejected","space":"space","client_id":"cid","reason":"stale_epoch: another commit took this epoch","permanent":true})
        ),
        ServerFrame::Rejected {
            space: "space".into(),
            client_id: "cid".into(),
            reason: "stale_epoch: another commit took this epoch".into(),
            permanent: true
        }
        .encode()
    );
    assert_eq!(
        wire(json!({"kind":"joined","space":"space"})),
        ServerFrame::Joined {
            space: "space".into()
        }
        .encode()
    );
    assert_eq!(
        wire(json!({"kind":"sync_done"})),
        ServerFrame::SyncDone.encode()
    );
    for invalid in ["01", "-1", "18446744073709551616"] {
        assert!(relay_socket::control_json(
            &json!({"kind":"accepted","space":"space","client_id":"cid","seq":invalid}).to_string()
        )
        .is_err());
    }
}
fn unhex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn frozen_actual_native_signed_ciphertext_and_pruned_stubs_keep_exact_server_wire() {
    // Exact existing native protobuf/signature fixtures, including legacy pruning.
    let data: Value = serde_json::from_str(include_str!(
        "../../event-log/testdata/legacy-sealed-v3.json"
    ))
    .unwrap();
    for name in [
        "genesis",
        "legacy_full",
        "legacy_stub",
        "unversioned_full",
        "unversioned_stub",
    ] {
        let bytes = unhex(data[name].as_str().unwrap());
        let ev = protocol::Sequenced::decode(&bytes).unwrap();
        ev.env.verify().unwrap();
        let result: Value =
            serde_json::from_str(&relay_socket::event_json(&bytes).unwrap()).unwrap();
        assert_eq!(result["space"], ev.env.space());
        assert_eq!(result["seq"], ev.seq.to_string());
        assert_eq!(result["author"], ev.env.author());
        assert_eq!(result["client_id"], ev.env.client_id());
        assert_eq!(
            STANDARD
                .decode(result["frame_base64"].as_str().unwrap())
                .unwrap(),
            ServerFrame::Event { ev: ev.clone() }.encode()
        );
        let mut wrong = ev.clone();
        wrong.hash = "00".repeat(32);
        assert!(relay_socket::event_json(&wrong.encode()).is_err());
        let mut wrong = ev;
        wrong.env.sig = "00".repeat(64);
        assert!(relay_socket::event_json(&wrong.encode()).is_err());
    }
}
#[test]
fn canonical_only_private_input_bound_is_explicitly_narrower_than_native_protobuf() {
    let canonical = ClientFrame::Ping.encode();
    let mut native_accepted = canonical.clone();
    native_accepted.extend_from_slice(&[0x98, 0x06, 0x01]); // Unknown protobuf field99.
    assert_eq!(
        ClientFrame::decode(&native_accepted).unwrap(),
        ClientFrame::Ping
    );
    assert!(relay_socket::incoming_json(&canonical).is_ok());
    assert!(relay_socket::incoming_json(&native_accepted)
        .unwrap_err()
        .reason
        .contains("canonical"));
    let input = ClientFrame::Sync {
        cursors: vec![
            Cursor {
                space: "z".into(),
                next_seq: u64::MAX,
            },
            Cursor {
                space: "z".into(),
                next_seq: 9_007_199_254_740_993,
            },
        ],
        all: true,
    }
    .encode();
    let result: Value =
        serde_json::from_str(&relay_socket::incoming_json(&input).unwrap()).unwrap();
    assert_eq!(
        result,
        json!({"kind":"sync","all":true,"cursors":[{"space":"z","next_seq":u64::MAX.to_string()},{"space":"z","next_seq":"9007199254740993"}]})
    );
}
#[test]
fn native_publish_and_request_default_bursts_use_exact_gcra_decimals() {
    for (scope, burst) in [
        ("publish_device", 60),
        ("publish_account", 120),
        ("request_device", 200),
    ] {
        let mut tat = None;
        for _ in 0..burst {
            let value: Value = serde_json::from_str(
                &relay_socket::rate_json(
                    &json!({"scope":scope,"now_ns":"9007199254740993","tat_ns":tat}).to_string(),
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(value["accepted"], true);
            tat = Some(value["next_ns"].as_str().unwrap().to_string());
        }
        let value: Value = serde_json::from_str(
            &relay_socket::rate_json(
                &json!({"scope":scope,"now_ns":"9007199254740993","tat_ns":tat}).to_string(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(value["accepted"], false);
        assert!(value["reason"]
            .as_str()
            .unwrap()
            .starts_with("slow down, retry in "));
    }
    assert!(relay_socket::rate_json(
        &json!({"scope":"invented","now_ns":"0","tat_ns":null}).to_string()
    )
    .is_err());
}

#[test]
fn challenge_advertises_only_features_implemented_by_this_composition() {
    let author = event_log::Author::device(
        &event_log::Signer::from_secret(&[25; 32]),
        event_log::Signer::from_secret(&[26; 32]),
    );
    let frame = ClientFrame::Hello {
        protocol: protocol::PROTOCOL_VERSION,
        capabilities: protocol::CAPABILITIES
            .iter()
            .map(|value| value.to_string())
            .collect(),
        identity: author.identity,
        device: author.device.unwrap(),
        cert: author.cert.unwrap(),
    }
    .encode();
    let result: Value = serde_json::from_str(
        &relay_socket::hello_json(
            &frame,
            &json!({"nonce":"aa".repeat(32),"relay":"zoen-local-test","now_ms":"1800000000000"})
                .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    let expected = json!(["profiles", protocol::KEY_PACKAGE_CLAIM_CAPABILITY]);
    assert_eq!(result["hello"]["capabilities"], expected);
    let ServerFrame::Challenge {
        capabilities,
        server_time_ms,
        ..
    } = ServerFrame::decode(
        &STANDARD
            .decode(result["challenge_base64"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap()
    else {
        panic!("challenge expected")
    };
    assert_eq!(json!(capabilities), expected);
    assert_eq!(server_time_ms, Some(1800000000000));
}

#[test]
fn native_key_package_notices_and_request_effects_preserve_reply_order_metadata() {
    for remaining in 0..8 {
        assert_eq!(
            wire(json!({"kind":"key_packages_low","device":"ab".repeat(32),"remaining":remaining})),
            ServerFrame::KeyPackagesLow {
                device: "ab".repeat(32),
                remaining
            }
            .encode()
        );
    }
    let refill = ClientFrame::Req {
        id: u64::MAX,
        op: protocol::Op::PublishKeyPackages {
            packages: vec![],
            last_resort: None,
        },
    }
    .encode();
    let value: Value =
        serde_json::from_str(&relay_socket::incoming_json(&refill).unwrap()).unwrap();
    assert_eq!(
        value,
        json!({"kind":"request","id":u64::MAX.to_string(),"register":false,"refill":true,"profile_changed":null})
    );
    let reply = ServerFrame::Res {
        id: u64::MAX,
        result: Ok(protocol::Reply::KeyPackages(vec![
            protocol::KeyPackageRecord {
                identity: "ab".repeat(32),
                device: "cd".repeat(32),
                data: vec![1, 2, 3],
            },
        ])),
    }
    .encode();
    let effects: Value =
        serde_json::from_str(&relay_socket::request_effects_json(&reply).unwrap()).unwrap();
    assert_eq!(
        effects,
        json!({"ok":true,"claimed":[{"identity":"ab".repeat(32),"device":"cd".repeat(32)}]})
    );
    assert!(effects.to_string().find("data").is_none());
    let failed = ServerFrame::Res {
        id: 1,
        result: Err("refused".into()),
    }
    .encode();
    assert_eq!(
        serde_json::from_str::<Value>(&relay_socket::request_effects_json(&failed).unwrap())
            .unwrap(),
        json!({"ok":false,"claimed":[]})
    );
}

#[test]
fn actual_signed_profile_metadata_and_notice_keep_exact_native_version() {
    use event_log::{profile::sign_upload, Author, Signer};
    use protocol::{Op, SealedProfile};
    let who = Author::device(
        &Signer::from_secret(&[83; 32]),
        Signer::from_secret(&[84; 32]),
    );
    for version in [0, 9_007_199_254_740_993, u64::MAX] {
        let ciphertext = vec![1, 2, 3, 4];
        let frame = ClientFrame::Req {
            id: u64::MAX,
            op: Op::PutProfile {
                profile: SealedProfile {
                    identity: who.identity.clone(),
                    version,
                    signed: sign_upload(&who, version, &ciphertext).unwrap(),
                    ciphertext,
                },
            },
        }
        .encode();
        let metadata: Value =
            serde_json::from_str(&relay_socket::incoming_json(&frame).unwrap()).unwrap();
        assert_eq!(
            metadata["profile_changed"],
            json!({"identity":who.identity,"version":version.to_string()})
        );
        assert!(!metadata.to_string().contains("ciphertext"));
        assert_eq!(
            wire(
                json!({"kind":"profile_changed","identity":who.identity,"version":version.to_string()})
            ),
            ServerFrame::ProfileChanged {
                identity: who.identity.clone(),
                version
            }
            .encode()
        );
    }
    for invalid in ["01", "-1", "18446744073709551616"] {
        assert!(relay_socket::control_json(
            &json!({"kind":"profile_changed","identity":who.identity,"version":invalid})
                .to_string()
        )
        .is_err());
    }
}
