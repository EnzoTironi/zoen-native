//! Sender recovery uses one exact retained delivery, not a fresh HPKE seal.
use super::*;
use enrollment_owner_tests::{code, enroll, opened, wire};
use serde_json::{json, Value};
use test_transport::{next_call, Fixture};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cold_retry_after_profile_edit_replays_original_signed_ciphertext() {
    let mut f = Fixture::new().await;
    f.online().await;
    let (code, recipient) = code();
    let first = enroll(&f, &code);
    let accepted = next_call(&mut f.relay.calls).await;
    let original = opened(&accepted, &code, &recipient);
    let original_wire = wire(&accepted);
    let retained = f.vault.load(VAULT_PEERS.into()).unwrap();
    drop(accepted.result);
    assert!(first.await.unwrap().is_err());
    f.reopen();
    f.online().await;
    {
        let engine = f.client().lock();
        let identity = engine.me_id().unwrap();
        let fields = event_log::profile::ProfileFields {
            name: "Edited after ambiguous delivery".into(),
            ..Default::default()
        };
        engine
            .store
            .set_profile_fields(
                &identity,
                original.profile_seen_version + 1,
                &fields.to_bytes(),
            )
            .unwrap();
    }
    let second = enroll(&f, &code);
    let replay = next_call(&mut f.relay.calls).await;
    assert_eq!(wire(&replay), original_wire);
    let retried = opened(&replay, &code, &recipient);
    assert_eq!(retried.profile_fields, original.profile_fields);
    replay.result.send(Ok(Reply::Done)).unwrap();
    assert_eq!(second.await.unwrap().unwrap().device_id, code.device);
    assert_eq!(f.vault.load(VAULT_PEERS.into()).unwrap(), retained);
    let peers: Vec<Peer> = serde_json::from_slice(&retained).unwrap();
    assert_eq!(
        peers.len(),
        1,
        "the ordinary peer projection remains compatible"
    );
    let detailed = enroll_prepare::peers(&(f.vault.clone() as Arc<dyn SecretVault>)).unwrap();
    let roundtrip: Value = serde_json::to_value(detailed).unwrap();
    assert_eq!(
        roundtrip,
        serde_json::from_slice::<Value>(&retained).unwrap(),
        "all retained delivery fields survive the shared writer codec"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn altered_or_legacy_retained_delivery_never_reseals_or_sends() {
    let f = Fixture::new().await;
    f.online().await;
    let (code, _) = code();
    let owner = owner::LinkOwner::capture(f.client()).unwrap();
    let vault: Arc<dyn SecretVault> = f.vault.clone();
    enroll_prepare::prepare(f.client(), &owner, &code, &vault).unwrap();
    let original: Value =
        serde_json::from_slice(&f.vault.load(VAULT_PEERS.into()).unwrap()).unwrap();
    let writes = f.vault.writes.load(Ordering::SeqCst);
    for case in 0..12 {
        let mut changed = original.clone();
        let row = &mut changed[0];
        match case {
            0 => {
                row.as_object_mut().unwrap().remove("enrollment");
            }
            1 => row["enrollment"]["signature"] = json!("invalid"),
            2 => {
                let original = row["enrollment"]["sealed"].as_str().unwrap();
                let first = if original.starts_with('A') { 'B' } else { 'A' };
                row["enrollment"]["sealed"] = json!(format!("{first}{}", &original[1..]));
            }
            3 => row["enrollment"]["identity"] = json!(Signer::generate().id()),
            4 => row["enrollment"]["primary"] = json!(Signer::generate().id()),
            5 => row["enrollment"]["primary_certificate"] = json!("invalid"),
            6 => row["enrollment"]["primary_hpke"] = json!(hex::encode([9; 32])),
            7 => row["enrollment"]["relay"] = json!("http://127.0.0.1:2"),
            8 => row["enrollment"]["code_digest"] = json!("00".repeat(32)),
            9 => row["enrollment"]["certificate"] = json!("invalid"),
            10 => row["my_hpke_secret"] = json!(hex::encode([8; 32])),
            11 => row["enrollment"]["sealed"] = json!("A".repeat(90 * 1024 + 1)),
            _ => unreachable!(),
        }
        let bytes = serde_json::to_vec(&changed).unwrap();
        f.vault
            .values
            .lock()
            .unwrap()
            .insert(VAULT_PEERS.into(), bytes.clone());
        assert!(
            enroll_prepare::prepare(f.client(), &owner, &code, &vault).is_err(),
            "alteration {case}"
        );
        assert_eq!(f.vault.load(VAULT_PEERS.into()).unwrap(), bytes);
        assert_eq!(f.vault.writes.load(Ordering::SeqCst), writes);
    }
    assert!(
        f.relay.calls.is_empty(),
        "preparation never opens another relay delivery"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retained_original_does_not_bypass_invalid_current_profile() {
    let f = Fixture::new().await;
    f.online().await;
    let (code, _) = code();
    let owner = owner::LinkOwner::capture(f.client()).unwrap();
    let vault: Arc<dyn SecretVault> = f.vault.clone();
    enroll_prepare::prepare(f.client(), &owner, &code, &vault).unwrap();
    let before = f.vault.load(VAULT_PEERS.into()).unwrap();
    let identity = f.client().account().unwrap().identity_id;
    f.client()
        .lock()
        .store
        .put_profile_key(&identity, "invalid", 1)
        .unwrap();
    let writes = f.vault.writes.load(Ordering::SeqCst);
    assert!(enroll_prepare::prepare(f.client(), &owner, &code, &vault).is_err());
    assert_eq!(f.vault.writes.load(Ordering::SeqCst), writes);
    assert_eq!(f.vault.load(VAULT_PEERS.into()).unwrap(), before);
    assert!(f.relay.calls.is_empty());
}
