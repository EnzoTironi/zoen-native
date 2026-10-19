use base64::{engine::general_purpose::STANDARD, Engine};
use roda_log::{Author, Signer};
use roda_proto::{ClientFrame, Op, Reply, ServerFrame};
use roda_relay_admission::directory::{self, reads, Device, Facts, Session};
use roda_types::{Identity, IdentityKind};
use serde_json::{json, Value};

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
fn session() -> Session {
    let root = Signer::from_secret(&[1; 32]);
    let author = Author::device(&root, Signer::from_secret(&[2; 32]));
    Session {
        identity: author.identity,
        device: author.device.unwrap(),
        cert: author.cert.unwrap(),
        registered: true,
    }
}
fn row(p: &Identity) -> reads::ProfileRow {
    reads::ProfileRow {
        id: p.id.clone(),
        kind: "Person".into(),
        owner: None,
        profile: json!(p),
    }
}
fn selected(
    op: &Op,
    profiles: Vec<reads::ProfileRow>,
    devices: Vec<reads::DeviceRow>,
) -> reads::Facts {
    reads::Facts {
        query: reads::query(op).unwrap(),
        profiles,
        devices,
    }
}
fn facts(s: &Session, read: reads::Facts) -> Facts {
    Facts {
        registered: true,
        device: Some(Device {
            identity: s.identity.clone(),
            cert: s.cert.clone(),
            revoked: false,
        }),
        reads: Some(read),
        ..Default::default()
    }
}

#[test]
fn native_lookup_normalization_and_literal_prefix_pattern_are_preserved() {
    assert_eq!(reads::normalize_lookup("  @@@ZoEn_Name  "), "zoen_name");
    assert_eq!(reads::normalize_lookup("  @İ  "), "i\u{307}");
    assert_eq!(reads::prefix_pattern("zo%_\\en"), "zo\\_en%");
    assert_eq!(
        reads::query(&Op::Lookup {
            handle: " @@@Zo%_\\EN ".into(),
            prefix: true
        }),
        Some(reads::Query::Lookup {
            handle: "zo%_\\en".into(),
            prefix: true,
            pattern: "zo\\_en%".into(),
        })
    );
    assert_eq!(reads::normalize_lookup(" @@ "), "");
}

#[test]
fn only_the_first_five_hundred_requested_profile_ids_are_read() {
    let ids: Vec<_> = (0..501).map(|index| format!("identity-{index}")).collect();
    assert_eq!(reads::profile_ids(ids.clone()), ids[..500]);
    let op = Op::Profiles { ids };
    let reads::Query::Profiles { ids } = reads::query(&op).unwrap() else {
        unreachable!()
    };
    assert_eq!(ids.len(), 500);
    assert_eq!(ids.last().unwrap(), "identity-499");
    let mut selected = selected(&op, vec![], vec![]);
    selected.profiles.push(reads::ProfileRow {
        id: "identity-500".into(),
        kind: "Person".into(),
        owner: None,
        profile: json!({}),
    });
    assert!(reads::reply(&op, &session(), &selected).is_err());
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
    agent.owner_proof = roda_log::agent_owner::authorize(&owner, &agent.id).map(Box::new);
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

#[test]
fn devices_include_revoked_rows_in_supplied_creation_order_and_never_another_account() {
    let s = session();
    let op = Op::Devices;
    let mut read = selected(
        &op,
        vec![],
        vec![
            reads::DeviceRow {
                identity: s.identity.clone(),
                device: "first".into(),
                revoked: true,
            },
            reads::DeviceRow {
                identity: s.identity.clone(),
                device: "second".into(),
                revoked: false,
            },
        ],
    );
    let Reply::Devices(devices) = reads::reply(&op, &s, &read).unwrap() else {
        unreachable!()
    };
    assert_eq!(
        devices
            .iter()
            .map(|row| (row.device.as_str(), row.revoked))
            .collect::<Vec<_>>(),
        vec![("first", true), ("second", false)]
    );
    read.devices[1].identity = "another-account".into();
    assert!(!reads::reply(&op, &s, &read).unwrap_err().permanent);
}

#[test]
fn read_rows_are_scoped_to_the_exact_query_and_native_storage_bounds() {
    let s = session();
    let p = profile(3, "person");
    let op = Op::Profiles {
        ids: vec![p.id.clone(), p.id.clone()],
    };
    let mut read = selected(&op, vec![row(&p)], vec![]);
    assert!(
        matches!(reads::reply(&op, &s, &read).unwrap(), Reply::Profiles(profiles) if profiles == vec![p.clone()])
    );
    read.profiles.push(row(&p));
    assert!(reads::reply(&op, &s, &read).is_err());
    read.profiles.pop();
    read.query = reads::Query::Devices;
    assert!(reads::reply(&op, &s, &read).is_err());
    let exact = Op::Lookup {
        handle: "person".into(),
        prefix: false,
    };
    assert!(reads::reply(
        &exact,
        &s,
        &selected(&exact, vec![row(&p), row(&p)], vec![])
    )
    .is_err());
    let empty = Op::Lookup {
        handle: " @@ ".into(),
        prefix: true,
    };
    assert!(reads::reply(&empty, &s, &selected(&empty, vec![row(&p)], vec![])).is_err());
    let prefix = Op::Lookup {
        handle: "person".into(),
        prefix: true,
    };
    assert!(reads::reply(
        &prefix,
        &s,
        &selected(&prefix, (0..21).map(|_| row(&p)).collect(), vec![])
    )
    .is_err());
}

#[test]
fn reads_require_registration_and_current_device_authorization_before_revealing_rows() {
    let mut s = session();
    let op = Op::Devices;
    let frame = ClientFrame::Req {
        id: 9_007_199_254_740_993,
        op: op.clone(),
    }
    .encode();
    let mut f = facts(&s, selected(&op, vec![], vec![]));
    f.device.as_mut().unwrap().revoked = true;
    assert_eq!(
        directory::request(&frame, &s, &f).unwrap_err().reason,
        "this device was unlinked"
    );
    s.registered = false;
    assert_eq!(
        directory::lookup_for_session(&frame, &s)
            .unwrap_err()
            .reason,
        "register first"
    );
    s.registered = true;
    f.device.as_mut().unwrap().revoked = false;
    let plan = directory::request(&frame, &s, &f).unwrap();
    assert_eq!(plan["action"], "read");
    let decoded: Value = serde_json::from_str(
        &reads::reply_json(
            &STANDARD
                .decode(plan["reply_base64"].as_str().unwrap())
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(decoded["id"], "9007199254740993");
    assert_eq!(decoded["devices"], json!([]));
    assert!(matches!(
        ServerFrame::decode(
            &STANDARD
                .decode(plan["reply_base64"].as_str().unwrap())
                .unwrap()
        )
        .unwrap(),
        ServerFrame::Res {
            result: Ok(Reply::Devices(_)),
            ..
        }
    ));
}

#[test]
fn default_lookup_budget_is_shared_with_link_delivery_and_refills_without_reset() {
    assert!(reads::spends_lookup_budget(&Op::Lookup {
        handle: String::new(),
        prefix: false
    }));
    assert!(reads::spends_lookup_budget(&Op::DeliverLink {
        id: String::new(),
        sealed: vec![],
        device: None
    }));
    assert!(!reads::spends_lookup_budget(&Op::Profiles { ids: vec![] }));
    assert!(!reads::spends_lookup_budget(&Op::Devices));
    let mut tat = None;
    for _ in 0..60 {
        let rate: Value = serde_json::from_str(
            &reads::lookup_rate_json(&json!({"now_ns":"1000000000000","tat_ns":tat}).to_string())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(rate["accepted"], true);
        tat = Some(rate["next_ns"].as_str().unwrap().to_string());
    }
    let rate = |now| -> Value {
        serde_json::from_str(
            &reads::lookup_rate_json(&json!({"now_ns":now,"tat_ns":tat}).to_string()).unwrap(),
        )
        .unwrap()
    };
    assert_eq!(
        rate("1000000000000"),
        json!({"accepted":false,"reason":"slow down, retry in 1.0 s"})
    );
    assert_eq!(rate("1001000000000")["accepted"], true);
    assert!(reads::lookup_rate_json(&json!({"now_ns":"01","tat_ns":null}).to_string()).is_err());
}
