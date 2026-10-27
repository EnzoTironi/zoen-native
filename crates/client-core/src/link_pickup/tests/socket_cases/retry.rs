//! Actual Client transport retries after a synthetic peer drops its first response.
use super::*;
use crate::SecretVault;
use futures_util::SinkExt;
use protocol::{Reply, ServerFrame};
use tokio_tungstenite::tungstenite::Message;

pub(super) async fn lost_transport_response_retries_the_original_pending_and_installs_only_once(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = format!("http://{}", listener.local_addr().unwrap());
    let core = crate::Client::open(":memory:".into(), "en".into()).unwrap();
    let vault = Arc::new(RecordingVault::default());
    core.link_request(relay, vault.clone()).unwrap();
    let original_pending = vault.load(crate::link::VAULT_PENDING.into()).unwrap();
    let material: Pending = serde_json::from_slice(&original_pending).unwrap();
    let secret: [u8; 32] = hex::decode(&material.secret).unwrap().try_into().unwrap();
    let box_id = crate::link::box_id(&secret, "identity");
    let sealed = sealed_identity(&material)?;
    let plain = crate::link::open(
        &hex::decode(&material.hpke_secret).unwrap(),
        &secret,
        "identity",
        &sealed,
    )
    .unwrap();
    let boxed: crate::link::IdentityBox = serde_json::from_slice(&plain).unwrap();
    let (dropped, first) = oneshot::channel();
    let (resume, next) = oneshot::channel();
    let peer = tokio::spawn(async move {
        let (first, _) = transport::accept_fetch(&listener, &box_id).await;
        // The retained mailbox remains outside this dropped connection. Only the
        // transport response is lost; no new seal/key material is manufactured.
        drop(first);
        dropped.send(()).unwrap();
        next.await.unwrap();
        let (mut second, request_id) = transport::accept_fetch(&listener, &box_id).await;
        second
            .send(Message::Binary(
                ServerFrame::Res {
                    id: request_id,
                    result: Ok(Reply::Link(Some(sealed))),
                }
                .encode()
                .into(),
            ))
            .await
            .unwrap();
    });
    let mut wait = Box::pin(core.link_wait(vault.clone(), 8_000));
    tokio::select! {
        result = wait.as_mut() => panic!("completed before lost response: {}", result.is_ok()),
        result = first => result.unwrap(),
        _ = tokio::time::sleep(BOUND) => panic!("first real fetch did not arrive"),
    }
    assert!(core.account().is_none());
    assert_eq!(
        vault.load(crate::link::VAULT_PENDING.into()).unwrap(),
        original_pending
    );
    assert_eq!(
        *vault.saves.lock().unwrap(),
        vec![crate::link::VAULT_PENDING.to_string()]
    );
    resume.send(()).unwrap();
    let account = tokio::time::timeout(budget(), wait.as_mut())
        .await
        .unwrap()
        .unwrap();
    drop(wait);
    tokio::time::timeout(BOUND, peer).await.unwrap().unwrap();
    tokio::time::timeout(BOUND, core.drain_link_pickup())
        .await
        .unwrap();
    assert_eq!(
        account.device_id,
        Signer::from_secret(
            &hex::decode(&material.device_secret)
                .unwrap()
                .try_into()
                .unwrap()
        )
        .id()
    );
    assert_eq!(
        vault.load(crate::profile::VAULT_AGREEMENT.into()).unwrap(),
        hex::decode(boxed.agreement_secret).unwrap()
    );
    assert!(vault.load(crate::link::VAULT_PENDING.into()).is_none());
    assert_eq!(
        vault.deletes.lock().unwrap().as_slice(),
        &[crate::link::VAULT_PENDING.to_string()]
    );
    assert!(core.link_wait(vault.clone(), 1_000).await.is_err());
    assert_eq!(core.account().unwrap().device_id, account.device_id);
    assert_eq!(
        vault.saves.lock().unwrap().len(),
        5,
        "one pending write and exactly one installation"
    );

    Ok(())
}
