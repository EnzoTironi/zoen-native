use base64::{engine::general_purpose::STANDARD, Engine};
use event_log::{profile::*, Author, Signer};
use protocol::{ClientFrame, Op, Reply, SealedProfile, ServerFrame};
use relay_admission::directory::{self, sealed_profiles as policy, Device, Facts, Session};
use serde_json::{json, Value};

fn author(device: u8) -> Author {
    Author::device(
        &Signer::from_secret(&[31; 32]),
        Signer::from_secret(&[device; 32]),
    )
}
fn session(who: &Author) -> Session {
    Session {
        identity: who.identity.clone(),
        device: who.device.clone().unwrap(),
        cert: who.cert.clone().unwrap(),
        registered: true,
    }
}
fn sealed(who: &Author, version: u64, ciphertext: Vec<u8>) -> SealedProfile {
    SealedProfile {
        identity: who.identity.clone(),
        version,
        signed: sign_upload(who, version, &ciphertext).unwrap(),
        ciphertext,
    }
}
fn selected(op: &Op) -> policy::Facts {
    policy::Facts {
        query: policy::query(op).unwrap(),
        agreements: vec![],
        profiles: vec![],
    }
}
#[test]
fn real_ciphertext_and_device_proof_survive_portable_plan_and_binary_reply() {
    let who = author(32);
    let fields = ProfileFields {
        name: "Synthetic Zoen".into(),
        bio: "Private fixture".into(),
        photo: None,
    };
    let key = ProfileKey::generate();
    let version = 9_007_199_254_740_993;
    let ciphertext = seal_profile(&key, &who.identity, version, &fields);
    let profile = sealed(&who, version, ciphertext.clone());
    let op = Op::PutProfile {
        profile: profile.clone(),
    };
    let plan = run(op.clone(), &who, selected(&op), false).unwrap();
    assert_eq!(plan["profile"]["version"], version.to_string());
    assert_eq!(
        plan["profile"]["ciphertext_base64"],
        STANDARD.encode(&ciphertext)
    );
    assert!(!plan.to_string().contains(&fields.name));
    let op = Op::GetProfiles {
        ids: vec![who.identity.clone()],
    };
    let mut facts = selected(&op);
    facts.profiles.push((&profile).into());
    let plan = run(op, &who, facts, false).unwrap();
    let bytes = STANDARD
        .decode(plan["reply_base64"].as_str().unwrap())
        .unwrap();
    let ServerFrame::Res {
        result: Ok(Reply::SealedProfiles(rows)),
        ..
    } = ServerFrame::decode(&bytes).unwrap()
    else {
        panic!("expected encrypted profile reply")
    };
    assert_eq!(rows, vec![profile]);
    assert_eq!(
        open_profile(&key, &who.identity, version, &rows[0].ciphertext),
        Some(fields)
    );
    assert!(open_profile(&key, &who.identity, version + 1, &rows[0].ciphertext).is_none());
    let decoded: Value = serde_json::from_str(&policy::reply_json(&bytes).unwrap()).unwrap();
    assert_eq!(decoded["id"], "9007199254740993");
    assert_eq!(decoded["rows"][0]["version"], version.to_string());
}

#[test]
fn equal_bytes_are_retryable_but_equal_or_lower_versions_cannot_replace_ciphertext() {
    let who = author(32);
    let previous = sealed(&who, 10, vec![1, 2]);
    for (version, bytes, expected) in [
        (9, vec![1, 2], false),
        (10, vec![9], false),
        (10, vec![1, 2], true),
        (11, vec![9], true),
    ] {
        let op = Op::PutProfile {
            profile: sealed(&who, version, bytes),
        };
        let mut facts = selected(&op);
        facts.profiles.push((&previous).into());
        let result = run(op, &who, facts, false);
        assert_eq!(result.is_ok(), expected);
        if !expected {
            assert_eq!(result.unwrap_err().reason, "stale profile version");
        }
    }
    // The same account's second certified device may re-sign an exact retry.
    let second = author(33);
    let op = Op::PutProfile {
        profile: sealed(&second, 10, vec![1, 2]),
    };
    let mut facts = selected(&op);
    facts.profiles.push((&previous).into());
    assert!(run(op, &second, facts, false).is_ok());
}

#[test]
fn upload_identity_device_signature_and_size_are_independent_fences() {
    let who = author(32);
    let valid = sealed(&who, 1, vec![1]);
    for (profile, reason) in [
        (
            {
                let mut p = valid.clone();
                p.identity = "other".into();
                p
            },
            "profile is for another identity",
        ),
        (
            sealed(&author(33), 1, vec![1]),
            "profile is for another device",
        ),
        (
            {
                let mut p = valid.clone();
                p.signed.sig = "00".repeat(64);
                p
            },
            "profile isn't signed by this identity",
        ),
        (
            sealed(&who, 1, vec![1; MAX_CIPHERTEXT + 1]),
            "profile too large",
        ),
    ] {
        assert_eq!(
            policy::validate_profile(&who.identity, who.device.as_ref().unwrap(), &profile)
                .unwrap_err(),
            reason
        );
    }
    for version in [0, i64::MAX as u64] {
        assert!(policy::validate_profile(
            &who.identity,
            who.device.as_ref().unwrap(),
            &sealed(&who, version, vec![0; MAX_CIPHERTEXT])
        )
        .is_ok());
    }
    for version in [i64::MAX as u64 + 1, u64::MAX] {
        assert_eq!(
            policy::validate_profile(
                &who.identity,
                who.device.as_ref().unwrap(),
                &sealed(&who, version, vec![1])
            )
            .unwrap_err(),
            "profile version out of range"
        );
    }
}

#[test]
fn agreement_keys_require_current_device_proof_and_roundtrip_unchanged() {
    let who = author(32);
    let public = AgreementKey::from_secret([44; 32]).public_hex();
    let signed = sign_agreement(&who, &public).unwrap();
    let op = Op::PublishAgreementKey {
        public: public.clone(),
        signed: signed.clone(),
    };
    let published = run(op.clone(), &who, selected(&op), false).unwrap();
    assert_eq!(published["agreement"]["public"], public);
    assert_eq!(
        policy::validate_agreement(
            &who.identity,
            author(33).device.as_ref().unwrap(),
            &public,
            &signed
        )
        .unwrap_err(),
        "agreement key is for another device"
    );
    assert!(policy::validate_agreement(
        &who.identity,
        who.device.as_ref().unwrap(),
        &"ff".repeat(32),
        &signed
    )
    .is_err());
    let op = Op::AgreementKeys {
        ids: vec![who.identity.clone(), who.identity.clone(), "missing".into()],
    };
    let mut facts = selected(&op);
    facts.agreements.push(policy::Agreement {
        identity: who.identity.clone(),
        public: public.clone(),
        signed: (&signed).into(),
    });
    let plan = run(op, &who, facts, false).unwrap();
    let bytes = STANDARD
        .decode(plan["reply_base64"].as_str().unwrap())
        .unwrap();
    let ServerFrame::Res {
        result: Ok(Reply::AgreementKeys(rows)),
        ..
    } = ServerFrame::decode(&bytes).unwrap()
    else {
        panic!("expected agreement keys")
    };
    assert_eq!(rows.len(), 1);
    assert!(verify_agreement(
        &rows[0].identity,
        &rows[0].public,
        &rows[0].signed
    ));
}

#[test]
fn current_device_revocation_and_registration_precede_encrypted_operations() {
    let who = author(32);
    for op in [
        Op::GetProfiles { ids: vec![] },
        Op::AgreementKeys { ids: vec![] },
        Op::PutProfile {
            profile: sealed(&who, 1, vec![1]),
        },
    ] {
        assert_eq!(
            run(op.clone(), &who, selected(&op), true)
                .unwrap_err()
                .reason,
            "this device was unlinked"
        );
        let mut provisional = session(&who);
        provisional.registered = false;
        let frame = ClientFrame::Req { id: 1, op }.encode();
        assert_eq!(
            directory::preflight(&frame, &provisional)
                .unwrap_err()
                .reason,
            "register first"
        );
    }
}

#[test]
fn storage_context_rejects_query_substitution_duplicate_rows_and_unrequested_identity() {
    let who = author(32);
    let op = Op::GetProfiles {
        ids: vec![who.identity.clone()],
    };
    let row = policy::Profile::from(&sealed(&who, 1, vec![1]));
    let mut facts = selected(&op);
    facts.profiles = vec![row.clone(), row.clone()];
    assert!(
        !run(op.clone(), &who, facts.clone(), false)
            .unwrap_err()
            .permanent
    );
    facts.profiles.truncate(1);
    facts.profiles[0].identity = "another".into();
    assert!(run(op.clone(), &who, facts.clone(), false).is_err());
    facts.profiles = vec![row];
    facts.query = policy::Query::AgreementKeys {
        ids: vec![who.identity.clone()],
    };
    assert!(run(op, &who, facts, false).is_err());
}

#[test]
fn decimal_versions_and_base64_in_trusted_rows_must_be_exact_and_bounded() {
    let who = author(32);
    let op = Op::GetProfiles {
        ids: vec![who.identity.clone()],
    };
    for version in [
        "01",
        "-1",
        "+1",
        "9223372036854775808",
        "18446744073709551615",
    ] {
        let mut facts = selected(&op);
        let mut row = policy::Profile::from(&sealed(&who, 1, vec![1]));
        row.version = version.into();
        facts.profiles.push(row);
        assert!(!run(op.clone(), &who, facts, false).unwrap_err().permanent);
    }
    for ciphertext in [
        "not-base64".into(),
        STANDARD.encode(vec![1; MAX_CIPHERTEXT + 1]),
    ] {
        let mut facts = selected(&op);
        let mut row = policy::Profile::from(&sealed(&who, 1, vec![1]));
        row.ciphertext_base64 = ciphertext;
        facts.profiles.push(row);
        assert!(run(op.clone(), &who, facts, false).is_err());
    }
}

#[test]
fn encrypted_reads_preserve_native_first_five_hundred_id_bound() {
    for op in [
        Op::GetProfiles {
            ids: (0..501).map(|i| i.to_string()).collect(),
        },
        Op::AgreementKeys {
            ids: (0..501).map(|i| i.to_string()).collect(),
        },
    ] {
        let ids = match policy::query(&op).unwrap() {
            policy::Query::GetProfiles { ids } | policy::Query::AgreementKeys { ids } => ids,
            _ => unreachable!(),
        };
        assert_eq!(ids.len(), 500);
        assert_eq!(ids.last().unwrap(), "499");
    }
}

fn run(
    op: Op,
    who: &Author,
    selected: policy::Facts,
    revoked: bool,
) -> Result<Value, relay_admission::portable::BoundaryError> {
    let s = session(who);
    let facts = Facts {
        registered: true,
        device: Some(Device {
            identity: s.identity.clone(),
            cert: s.cert.clone(),
            revoked,
        }),
        sealed_profiles: Some(selected),
        ..Default::default()
    };
    let frame = ClientFrame::Req {
        id: 9_007_199_254_740_993,
        op,
    }
    .encode();
    let context = json!({"session":s,"facts":facts}).to_string();
    Ok(serde_json::from_str(&directory::request_json(&frame, &context)?).expect("JSON plan"))
}
