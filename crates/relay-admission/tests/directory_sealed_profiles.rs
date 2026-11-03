use event_log::{profile::*, Author, Signer};
use protocol::{Reply, SealedProfile, ServerFrame};
use relay_admission::directory::sealed_profiles as policy;

fn author(device: u8) -> Author {
    Author::device(
        &Signer::from_secret(&[31; 32]),
        Signer::from_secret(&[device; 32]),
    )
}
fn sealed(
    who: &Author,
    version: u64,
    ciphertext: Vec<u8>,
) -> Result<SealedProfile, Box<dyn std::error::Error>> {
    Ok(SealedProfile {
        identity: who.identity.clone(),
        version,
        signed: sign_upload(who, version, &ciphertext)
            .ok_or("fixture upload signature unavailable")?,
        ciphertext,
    })
}
#[test]
fn real_profile_ciphertext_keeps_native_crypto_and_wire_without_host_dtos(
) -> Result<(), Box<dyn std::error::Error>> {
    let who = author(32);
    let fields = ProfileFields {
        name: "Synthetic Zoen".into(),
        bio: "private fixture".into(),
        photo: None,
    };
    let key = ProfileKey::generate()?;
    let version = 9_007_199_254_740_993;
    let ciphertext = seal_profile(&key, &who.identity, version, &fields)?;
    let p = sealed(&who, version, ciphertext)?;
    policy::validate_profile(&who.identity, who.device.as_deref().unwrap(), &p).unwrap();
    let frame = ServerFrame::Res {
        id: 9_007_199_254_740_993,
        result: Ok(Reply::SealedProfiles(vec![p.clone()])),
    };
    let ServerFrame::Res {
        id,
        result: Ok(Reply::SealedProfiles(rows)),
    } = ServerFrame::decode(&frame.encode()).unwrap()
    else {
        panic!("wrong reply")
    };
    assert_eq!(id, 9_007_199_254_740_993);
    assert_eq!(rows, vec![p.clone()]);
    assert_eq!(
        open_profile(&key, &who.identity, version, &p.ciphertext),
        Some(fields)
    );
    assert!(open_profile(&key, &who.identity, version + 1, &p.ciphertext).is_none());
    assert!(open_profile(
        &ProfileKey::generate()?,
        &who.identity,
        version,
        &p.ciphertext
    )
    .is_none());
    Ok(())
}
#[test]
fn upload_identity_device_signature_and_size_are_independent_fences(
) -> Result<(), Box<dyn std::error::Error>> {
    let who = author(32);
    let valid = sealed(&who, 1, vec![1])?;
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
            sealed(&author(33), 1, vec![1])?,
            "profile is for another device",
        ),
        (
            {
                let mut p = valid;
                p.signed.sig = "00".repeat(64);
                p
            },
            "profile isn't signed by this identity",
        ),
        (
            sealed(&who, 1, vec![1; MAX_CIPHERTEXT + 1])?,
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
            &sealed(&who, version, vec![0; MAX_CIPHERTEXT])?
        )
        .is_ok());
    }
    for version in [i64::MAX as u64 + 1, u64::MAX] {
        assert_eq!(
            policy::validate_profile(
                &who.identity,
                who.device.as_ref().unwrap(),
                &sealed(&who, version, vec![1])?
            )
            .unwrap_err(),
            "profile version out of range"
        );
    }
    Ok(())
}
#[test]
fn native_agreement_signatures_bind_current_device_identity_key_and_certificate() {
    let who = author(32);
    let public = AgreementKey::from_secret([44; 32]).public_hex();
    let signed = sign_agreement(&who, &public).unwrap();
    policy::validate_agreement(
        &who.identity,
        who.device.as_deref().unwrap(),
        &public,
        &signed,
    )
    .unwrap();
    assert_eq!(
        policy::validate_agreement(
            &who.identity,
            author(33).device.as_deref().unwrap(),
            &public,
            &signed
        )
        .unwrap_err(),
        "agreement key is for another device"
    );
    assert!(policy::validate_agreement(
        &who.identity,
        who.device.as_deref().unwrap(),
        &"ff".repeat(32),
        &signed
    )
    .is_err());
    assert!(policy::validate_agreement(
        &Signer::from_secret(&[91; 32]).id(),
        who.device.as_deref().unwrap(),
        &public,
        &signed
    )
    .is_err());
    let mut invalid = signed;
    invalid.cert = "00".repeat(64);
    assert!(policy::validate_agreement(
        &who.identity,
        who.device.as_deref().unwrap(),
        &public,
        &invalid
    )
    .is_err());
}
