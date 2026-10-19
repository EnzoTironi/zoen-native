//! Real native SQL/encryption oracle; never exports account signing or profile keys.
use super::*;
use roda_log::profile::{self as crypto, AgreementKey, ProfileFields, ProfileKey};
use roda_proto::SealedProfile;
use roda_relay_admission::directory::sealed_profiles as policy;

async fn encrypted_state(pool: &PgPool) -> Result<Value> {
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM identities ORDER BY id")
        .fetch_all(pool)
        .await?;
    let mut agreements: Vec<_> = db::agreement_keys(pool, &ids)
        .await?
        .into_iter()
        .map(|row| policy::Agreement {
            identity: row.identity,
            public: row.public,
            signed: (&row.signed).into(),
        })
        .collect();
    agreements.sort_by(|a, b| a.identity.cmp(&b.identity));
    let mut profiles: Vec<_> = db::sealed_profiles(pool, &ids)
        .await?
        .iter()
        .map(policy::Profile::from)
        .collect();
    profiles.sort_by(|a, b| a.identity.cmp(&b.identity));
    Ok(json!({"agreements":agreements,"profiles":profiles}))
}

async fn attach(f: &mut Fixture<'_>, tag: Option<&str>, unordered: bool) -> Result<()> {
    let state = encrypted_state(f.pool).await?;
    let step = f
        .steps
        .last_mut()
        .context("encrypted fixture step missing")?;
    step["encrypted_state_after"] = state;
    if let Some(tag) = tag {
        step["encrypted_test_case"] = json!(tag);
    }
    if unordered {
        step["unordered_encrypted_rows"] = json!(true);
    }
    Ok(())
}

async fn request(
    f: &mut Fixture<'_>,
    label: &str,
    actor: &str,
    op: Op,
    refused: Option<&str>,
    tag: Option<&str>,
) -> Result<()> {
    let read = matches!(&op, Op::AgreementKeys { .. } | Op::GetProfiles { .. });
    let before = encrypted_state(f.pool).await?;
    f.request(label, actor, op, refused).await?;
    if refused.is_some() || read {
        ensure!(
            before == encrypted_state(f.pool).await?,
            "{label}: rejected/read operation mutated encrypted rows"
        );
    }
    attach(f, tag, read).await
}

async fn login(
    f: &mut Fixture<'_>,
    label: &str,
    actor: &str,
    who: &Author,
    refused: Option<&str>,
) -> Result<()> {
    f.login(label, actor, who, refused).await?;
    attach(f, None, false).await
}

fn upload(who: &Author, version: u64, ciphertext: Vec<u8>) -> SealedProfile {
    SealedProfile {
        identity: who.identity.clone(),
        version,
        signed: crypto::sign_upload(who, version, &ciphertext).expect("certified synthetic device"),
        ciphertext,
    }
}

fn agreement(who: &Author, seed: u8) -> Op {
    let public = AgreementKey::from_secret([seed; 32]).public_hex();
    Op::PublishAgreementKey {
        signed: crypto::sign_agreement(who, &public).expect("certified synthetic device"),
        public,
    }
}

pub(super) async fn timeline(f: &mut Fixture<'_>) -> Result<Value> {
    f.now_ms = NOW_MS.into();
    let owner = author(180, 220);
    let linked = author(180, 221);
    let foreign = author(181, 222);
    let provisional = author(182, 182);
    login(f, "encrypted-owner-login", "encrypted-owner", &owner, None).await?;
    request(
        f,
        "encrypted-owner-register",
        "encrypted-owner",
        Op::Register {
            profile: profile(&owner, "encrypted_owner"),
        },
        None,
        None,
    )
    .await?;
    login(
        f,
        "encrypted-foreign-login",
        "encrypted-foreign",
        &foreign,
        None,
    )
    .await?;
    request(
        f,
        "encrypted-foreign-register",
        "encrypted-foreign",
        Op::Register {
            profile: profile(&foreign, "encrypted_foreign"),
        },
        None,
        None,
    )
    .await?;
    login(
        f,
        "encrypted-provisional-login",
        "encrypted-provisional",
        &provisional,
        None,
    )
    .await?;
    request(
        f,
        "encrypted-provisional-agreement",
        "encrypted-provisional",
        agreement(&provisional, 40),
        Some("register first"),
        None,
    )
    .await?;
    request(
        f,
        "encrypted-provisional-read",
        "encrypted-provisional",
        Op::GetProfiles {
            ids: vec![owner.identity.clone()],
        },
        Some("register first"),
        None,
    )
    .await?;
    request(
        f,
        "encrypted-agreement-first",
        "encrypted-owner",
        agreement(&owner, 41),
        None,
        Some("agreement-first"),
    )
    .await?;
    request(
        f,
        "encrypted-agreement-empty",
        "encrypted-owner",
        Op::AgreementKeys { ids: vec![] },
        None,
        None,
    )
    .await?;
    request(
        f,
        "encrypted-agreement-first-read",
        "encrypted-owner",
        Op::AgreementKeys {
            ids: vec![
                owner.identity.clone(),
                owner.identity.clone(),
                "missing".into(),
            ],
        },
        None,
        None,
    )
    .await?;
    request(
        f,
        "encrypted-agreement-wrong-device",
        "encrypted-owner",
        agreement(&linked, 42),
        Some("agreement key is for another device"),
        None,
    )
    .await?;
    let mut tampered_agreement = agreement(&owner, 42);
    if let Op::PublishAgreementKey { signed, .. } = &mut tampered_agreement {
        signed.sig = "00".repeat(64);
    }
    request(
        f,
        "encrypted-agreement-tampered",
        "encrypted-owner",
        tampered_agreement,
        Some("agreement key isn't signed by this identity"),
        None,
    )
    .await?;
    request(
        f,
        "encrypted-agreement-replacement",
        "encrypted-owner",
        agreement(&owner, 43),
        None,
        Some("agreement-replacement"),
    )
    .await?;
    request(
        f,
        "encrypted-agreement-foreign",
        "encrypted-foreign",
        agreement(&foreign, 44),
        None,
        None,
    )
    .await?;
    let both = vec![
        foreign.identity.clone(),
        owner.identity.clone(),
        owner.identity.clone(),
        "missing".into(),
    ];
    request(
        f,
        "encrypted-agreement-mixed",
        "encrypted-owner",
        Op::AgreementKeys { ids: both.clone() },
        None,
        None,
    )
    .await?;

    // Only disposable synthetic profile keys live here; none enters fixture JSON.
    let key = ProfileKey([71; 32]);
    let fields = ProfileFields {
        name: "Private synthetic profile".into(),
        bio: "Native encrypted SQL oracle".into(),
        photo: None,
    };
    let version = 9_007_199_254_740_993;
    let ciphertext = crypto::seal_profile(&key, &owner.identity, version, &fields);
    let initial = upload(&owner, version, ciphertext.clone());
    request(
        f,
        "encrypted-profile-first",
        "encrypted-owner",
        Op::PutProfile {
            profile: initial.clone(),
        },
        None,
        Some("profile-first"),
    )
    .await?;
    let stored = db::sealed_profiles(f.pool, std::slice::from_ref(&owner.identity)).await?;
    ensure!(
        stored.len() == 1 && stored[0] == initial,
        "native SQL altered sealed bytes"
    );
    let roundtrip = crypto::open_profile(&key, &owner.identity, version, &stored[0].ciphertext)
        == Some(fields.clone());
    let wrong_version_rejected =
        crypto::open_profile(&key, &owner.identity, version + 1, &stored[0].ciphertext).is_none();
    let wrong_key_rejected = crypto::open_profile(
        &ProfileKey([72; 32]),
        &owner.identity,
        version,
        &stored[0].ciphertext,
    )
    .is_none();
    ensure!(
        roundtrip && wrong_version_rejected && wrong_key_rejected,
        "actual native profile AEAD proof failed"
    );
    let proof = json!({"roundtrip":roundtrip,"wrong_version_rejected":wrong_version_rejected,
        "wrong_key_rejected":wrong_key_rejected,"identity":owner.identity,"version":version.to_string(),
        "ciphertext_sha256":roda_log::sha256_hex(&stored[0].ciphertext)});
    request(
        f,
        "encrypted-profile-read",
        "encrypted-owner",
        Op::GetProfiles { ids: both.clone() },
        None,
        Some("profile-read"),
    )
    .await?;
    request(
        f,
        "encrypted-profile-identical-retry",
        "encrypted-owner",
        Op::PutProfile {
            profile: initial.clone(),
        },
        None,
        None,
    )
    .await?;
    request(
        f,
        "encrypted-profile-same-version-different",
        "encrypted-owner",
        Op::PutProfile {
            profile: upload(&owner, version, vec![9; 80]),
        },
        Some("stale profile version"),
        Some("profile-same-version-different"),
    )
    .await?;
    request(
        f,
        "encrypted-profile-older",
        "encrypted-owner",
        Op::PutProfile {
            profile: upload(&owner, version - 1, ciphertext.clone()),
        },
        Some("stale profile version"),
        None,
    )
    .await?;
    let newer = upload(
        &owner,
        version + 1,
        crypto::seal_profile(&key, &owner.identity, version + 1, &fields),
    );
    request(
        f,
        "encrypted-profile-newer",
        "encrypted-owner",
        Op::PutProfile { profile: newer },
        None,
        Some("profile-newer"),
    )
    .await?;
    request(
        f,
        "encrypted-profile-wrong-identity",
        "encrypted-owner",
        Op::PutProfile {
            profile: upload(&foreign, version + 2, vec![1]),
        },
        Some("profile is for another identity"),
        None,
    )
    .await?;
    request(
        f,
        "encrypted-profile-wrong-device",
        "encrypted-owner",
        Op::PutProfile {
            profile: upload(&linked, version + 2, vec![1]),
        },
        Some("profile is for another device"),
        None,
    )
    .await?;
    let mut bad_signature = upload(&owner, version + 2, vec![1]);
    bad_signature.signed.sig = "00".repeat(64);
    request(
        f,
        "encrypted-profile-tampered",
        "encrypted-owner",
        Op::PutProfile {
            profile: bad_signature,
        },
        Some("profile isn't signed by this identity"),
        None,
    )
    .await?;
    request(
        f,
        "encrypted-profile-too-large",
        "encrypted-owner",
        Op::PutProfile {
            profile: upload(&owner, version + 2, vec![1; crypto::MAX_CIPHERTEXT + 1]),
        },
        Some("profile too large"),
        None,
    )
    .await?;
    request(
        f,
        "encrypted-profile-overflow",
        "encrypted-owner",
        Op::PutProfile {
            profile: upload(&owner, u64::MAX, vec![1]),
        },
        Some("profile version out of range"),
        None,
    )
    .await?;
    request(
        f,
        "encrypted-profile-foreign-first",
        "encrypted-foreign",
        Op::PutProfile {
            profile: upload(
                &foreign,
                0,
                crypto::seal_profile(&ProfileKey([73; 32]), &foreign.identity, 0, &fields),
            ),
        },
        None,
        None,
    )
    .await?;
    request(
        f,
        "encrypted-profile-max-ciphertext",
        "encrypted-foreign",
        Op::PutProfile {
            profile: upload(&foreign, 1, vec![7; crypto::MAX_CIPHERTEXT]),
        },
        None,
        None,
    )
    .await?;
    request(
        f,
        "encrypted-profile-empty-read",
        "encrypted-owner",
        Op::GetProfiles { ids: vec![] },
        None,
        None,
    )
    .await?;
    request(
        f,
        "encrypted-profile-mixed-read",
        "encrypted-owner",
        Op::GetProfiles { ids: both },
        None,
        None,
    )
    .await?;
    for (label, op) in [
        (
            "encrypted-profile-first500",
            Op::GetProfiles {
                ids: vec![owner.identity.clone(); 500]
                    .into_iter()
                    .chain([foreign.identity.clone()])
                    .collect(),
            },
        ),
        (
            "encrypted-profile-500-distinct",
            Op::GetProfiles {
                ids: (0..500)
                    .map(|index| format!("missing-{index}"))
                    .chain([owner.identity.clone()])
                    .collect(),
            },
        ),
        (
            "encrypted-agreement-first500",
            Op::AgreementKeys {
                ids: vec![owner.identity.clone(); 500]
                    .into_iter()
                    .chain([foreign.identity.clone()])
                    .collect(),
            },
        ),
        (
            "encrypted-agreement-500-distinct",
            Op::AgreementKeys {
                ids: (0..500)
                    .map(|index| format!("missing-{index}"))
                    .chain([owner.identity.clone()])
                    .collect(),
            },
        ),
    ] {
        request(f, label, "encrypted-owner", op, None, None).await?;
    }
    request(
        f,
        "encrypted-link-device",
        "encrypted-owner",
        Op::DeliverLink {
            id: "ab".repeat(32),
            sealed: vec![1, 2, 3],
            device: Some(certificate(&linked)?),
        },
        None,
        None,
    )
    .await?;
    login(
        f,
        "encrypted-linked-login",
        "encrypted-linked",
        &linked,
        None,
    )
    .await?;
    request(
        f,
        "encrypted-linked-agreement",
        "encrypted-linked",
        agreement(&linked, 45),
        None,
        None,
    )
    .await?;
    request(
        f,
        "encrypted-profile-linked",
        "encrypted-linked",
        Op::PutProfile {
            profile: upload(
                &linked,
                version + 2,
                crypto::seal_profile(&key, &owner.identity, version + 2, &fields),
            ),
        },
        None,
        Some("profile-linked"),
    )
    .await?;
    let linked_stored = db::sealed_profiles(f.pool, std::slice::from_ref(&owner.identity)).await?;
    request(
        f,
        "encrypted-profile-linked-identical-resign",
        "encrypted-owner",
        Op::PutProfile {
            profile: upload(&owner, version + 2, linked_stored[0].ciphertext.clone()),
        },
        None,
        None,
    )
    .await?;
    request(
        f,
        "encrypted-profile-max-bigint",
        "encrypted-owner",
        Op::PutProfile {
            profile: upload(
                &owner,
                i64::MAX as u64,
                crypto::seal_profile(&key, &owner.identity, i64::MAX as u64, &fields),
            ),
        },
        None,
        None,
    )
    .await?;
    request(
        f,
        "encrypted-unlink-linked",
        "encrypted-owner",
        Op::Unlink {
            device: certificate(&linked)?.device,
        },
        None,
        Some("unlink-linked"),
    )
    .await?;
    request(
        f,
        "encrypted-revoked-profile",
        "encrypted-linked",
        Op::PutProfile {
            profile: upload(&linked, u64::MAX, vec![1]),
        },
        Some("this device was unlinked"),
        None,
    )
    .await?;
    request(
        f,
        "encrypted-revoked-agreement",
        "encrypted-linked",
        agreement(&linked, 46),
        Some("this device was unlinked"),
        None,
    )
    .await?;
    login(
        f,
        "encrypted-revoked-reconnect",
        "encrypted-revoked",
        &linked,
        Some("this device was unlinked"),
    )
    .await?;
    request(
        f,
        "encrypted-profile-final-read",
        "encrypted-owner",
        Op::GetProfiles {
            ids: vec![owner.identity.clone(), foreign.identity.clone()],
        },
        None,
        None,
    )
    .await?;
    ensure!(
        f.steps.len() >= 30,
        "encrypted native timeline is incomplete"
    );
    Ok(proof)
}
