use super::*;
use test_transport::{next_call, Call, Fixture};

pub(super) fn code() -> (LinkCode, Vec<u8>) {
    let device = Signer::generate();
    let (secret, public) = link::key_pair();
    (
        LinkCode {
            device: device.id(),
            hpke: public,
            secret: [77; 32],
        },
        secret,
    )
}
pub(super) fn opened(call: &Call, code: &LinkCode, recipient: &[u8]) -> IdentityBox {
    let Op::DeliverLink {
        id,
        sealed,
        device: Some(certificate),
    } = &call.op
    else {
        panic!("enrollment delivery");
    };
    assert_eq!(id, &link::box_id(&code.secret, "identity"));
    assert_eq!(certificate.device, code.device);
    let plain = link::open(recipient, &code.secret, "identity", sealed).unwrap();
    let boxed: IdentityBox = serde_json::from_slice(&plain).unwrap();
    assert_eq!(boxed.cert, certificate.cert);
    let identity =
        Signer::from_secret(&secret32(hex::decode(&boxed.identity_secret).ok()).unwrap());
    assert!(event_log::verify_sig(
        &identity.id(),
        &event_log::device_cert_message(&code.device),
        &boxed.cert
    ));
    boxed
}
pub(super) fn enroll(
    f: &Fixture,
    code: &LinkCode,
) -> tokio::task::JoinHandle<Result<LinkedDto, CoreError>> {
    let client = f.client().clone();
    let vault: Arc<dyn SecretVault> = f.vault.clone();
    let encoded = code.encode();
    tokio::spawn(async move { client.link_device_impl(encoded, vault).await })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retired_preparation_cannot_read_or_write_vault_or_add_peer_to_new_session() {
    let f = Fixture::new().await;
    f.online().await;
    let owner = owner::LinkOwner::capture(f.client()).unwrap();
    f.client().stop_sync();
    f.online().await;
    let reads = f.vault.reads.load(Ordering::SeqCst);
    let writes = f.vault.writes.load(Ordering::SeqCst);
    let before = f.vault.values.lock().unwrap().clone();
    let vault: Arc<dyn SecretVault> = f.vault.clone();
    assert!(matches!(
        enroll_prepare::prepare(f.client(), &owner, &code().0, &vault),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(f.vault.reads.load(Ordering::SeqCst), reads);
    assert_eq!(f.vault.writes.load(Ordering::SeqCst), writes);
    assert_eq!(*f.vault.values.lock().unwrap(), before);
    assert!(f.client().lock().net.link.peers.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ambiguous_delivery_reuses_same_peer_key_across_cold_reopen_and_explicit_retry() {
    let mut f = Fixture::new().await;
    f.online().await;
    let account = f.client().account().unwrap();
    let (code, recipient) = code();
    let first = enroll(&f, &code);
    let accepted = next_call(&mut f.relay.calls).await;
    let original = opened(&accepted, &code, &recipient);
    let original_wire = serde_json::to_vec(&accepted.op).unwrap();
    let retained = f.vault.load(VAULT_PEERS.into()).unwrap();
    // The fixture accepts delivery, then loses its response by closing the socket.
    drop(accepted.result);
    assert!(first.await.unwrap().is_err());
    f.reopen();
    f.online().await;
    assert_eq!(
        f.client().account().unwrap().identity_id,
        account.identity_id
    );
    assert_eq!(f.client().account().unwrap().device_id, account.device_id);
    let second = enroll(&f, &code);
    let still_pending = next_call(&mut f.relay.calls).await;
    let retried = opened(&still_pending, &code, &recipient);
    assert_eq!(
        serde_json::to_vec(&still_pending.op).unwrap(),
        original_wire
    );
    assert_eq!(retried.primary_hpke, original.primary_hpke);
    assert_eq!(retried.cert, original.cert);
    // An unreconciled relay conflict remains an error, never fabricated success.
    still_pending
        .result
        .send(Err("a link box is already there".into()))
        .unwrap();
    assert!(second.await.unwrap().is_err());
    assert_eq!(f.vault.load(VAULT_PEERS.into()).unwrap(), retained);
    let third = enroll(&f, &code);
    let redelivery = next_call(&mut f.relay.calls).await;
    assert_eq!(serde_json::to_vec(&redelivery.op).unwrap(), original_wire);
    assert_eq!(
        opened(&redelivery, &code, &recipient).primary_hpke,
        original.primary_hpke
    );
    redelivery.result.send(Ok(Reply::Done)).unwrap();
    assert_eq!(third.await.unwrap().unwrap().device_id, code.device);
    let peers: Vec<Peer> =
        serde_json::from_slice(&f.vault.load(VAULT_PEERS.into()).unwrap()).unwrap();
    assert_eq!(peers.len(), 1);
    assert_eq!(f.vault.load(VAULT_PEERS.into()).unwrap(), retained);
    // Independent crypto check: the announced public key opens with the retained private key.
    let device_message = link::seal(
        &hex::decode(original.primary_hpke).unwrap(),
        &code.secret,
        "device-message",
        &serde_json::to_vec(&link::DeviceMsg::HistoryReady).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        peers[0].open(&device_message),
        Some(link::DeviceMsg::HistoryReady)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ambiguous_vault_save_does_not_deliver_or_claim_success_and_retains_retry_material() {
    let mut f = Fixture::new().await;
    f.online().await;
    let (code, recipient) = code();
    f.vault.ambiguous_peer_save.store(true, Ordering::SeqCst);
    assert!(enroll(&f, &code).await.unwrap().is_err());
    assert!(
        f.relay.calls.try_recv().is_err(),
        "no DeliverLink was admitted"
    );
    assert!(f.client().lock().net.link.peers.is_empty());
    let retained = f.vault.load(VAULT_PEERS.into()).unwrap();
    f.vault.ambiguous_peer_save.store(false, Ordering::SeqCst);
    let retry = enroll(&f, &code);
    let delivery = next_call(&mut f.relay.calls).await;
    let boxed = opened(&delivery, &code, &recipient);
    let peers: Vec<Peer> = serde_json::from_slice(&retained).unwrap();
    let sealed = link::seal(
        &hex::decode(boxed.primary_hpke).unwrap(),
        &code.secret,
        "device-message",
        &serde_json::to_vec(&link::DeviceMsg::HistoryReady).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        peers[0].open(&sealed),
        Some(link::DeviceMsg::HistoryReady)
    ));
    delivery.result.send(Ok(Reply::Done)).unwrap();
    assert!(retry.await.unwrap().is_ok());
    assert_eq!(f.vault.load(VAULT_PEERS.into()).unwrap(), retained);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn corrupt_peer_vault_is_not_replaced_by_a_successful_empty_directory() {
    let f = Fixture::new().await;
    f.online().await;
    f.vault
        .values
        .lock()
        .unwrap()
        .insert(VAULT_PEERS.into(), b"not-json".to_vec());
    let before = f.vault.values.lock().unwrap().clone();
    assert!(enroll(&f, &code().0).await.unwrap().is_err());
    assert_eq!(*f.vault.values.lock().unwrap(), before);
    assert!(f.client().lock().net.link.peers.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_corrupt_or_foreign_agreement_cannot_persist_peer_or_deliver_enrollment() {
    for agreement in [
        None,
        Some(vec![]),
        Some(vec![1; 31]),
        Some(vec![1; 33]),
        Some(vec![79; 32]),
    ] {
        let mut f = Fixture::new().await;
        f.online().await;
        match agreement {
            None => {
                f.vault.values.lock().unwrap().remove(VAULT_AGREEMENT);
            }
            Some(bytes) => {
                f.vault
                    .values
                    .lock()
                    .unwrap()
                    .insert(VAULT_AGREEMENT.into(), bytes);
            }
        }
        let before = f.vault.values.lock().unwrap().clone();
        let writes = f.vault.writes.load(Ordering::SeqCst);
        assert!(enroll(&f, &code().0).await.unwrap().is_err());
        assert!(
            f.relay.calls.try_recv().is_err(),
            "no DeliverLink for an unusable account key"
        );
        assert_eq!(f.vault.writes.load(Ordering::SeqCst), writes);
        assert_eq!(*f.vault.values.lock().unwrap(), before);
        assert!(f.client().lock().net.link.peers.is_empty());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn corrupt_profile_key_cannot_be_sealed_as_successful_enrollment() {
    let mut f = Fixture::new().await;
    f.online().await;
    let me = f.client().account().unwrap().identity_id;
    f.client()
        .lock()
        .store
        .put_profile_key(&me, "not-a-profile-key", 1)
        .unwrap();
    let before = f.vault.values.lock().unwrap().clone();
    let writes = f.vault.writes.load(Ordering::SeqCst);
    assert!(enroll(&f, &code().0).await.unwrap().is_err());
    assert!(f.relay.calls.try_recv().is_err());
    assert_eq!(f.vault.writes.load(Ordering::SeqCst), writes);
    assert_eq!(*f.vault.values.lock().unwrap(), before);
    assert!(f.client().lock().net.link.peers.is_empty());
}
