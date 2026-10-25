//! A package claimed around reconnect must still trigger refill for the attaching device.
//! Login no longer awaits a NATS presence self-ping, so this journey synchronizes on
//! live attachment (hub online) rather than holding that ping between handshake and add.

mod common;
use common::*;
use event_log::{Author, Signer};
use futures_util::StreamExt;
use protocol::{Op, ServerFrame};
use sha2::{Digest, Sha256};
use std::time::Duration;

#[tokio::test]
async fn a_connecting_device_does_not_miss_a_low_stock_notice(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let w = World::new_with(
        "kp_connect",
        Some(std::env::var("ZOEN_NATS_URL").expect("set ZOEN_NATS_URL")),
        &[("ZOEN_LIMITS", "lookup_account=1000/m:1000")],
    )
    .await;
    w.init("bruno", "Bruno");
    let read_key = |key: &str| {
        let secret: [u8; 32] = std::fs::read(w.dir.join("bruno/vault").join(key))
            .unwrap()
            .try_into()
            .expect("device vault key");
        Signer::from_secret(&secret)
    };
    let bruno = Author::device(&read_key("zoen.identity.v1"), read_key("zoen.device.v1"));
    let sql = format!(
        "SELECT count(*) FROM key_packages WHERE identity = '{}' AND NOT last_resort",
        bruno.identity
    );
    assert_eq!(w.count(&sql).await, 32);
    let mut ana = RawClient::connect(&w.relay_url(), "ana").await?;
    for _ in 0..24 {
        ana.request(Op::ClaimKeyPackages {
            ids: vec![bruno.identity.clone()],
            operation_id: None,
        })
        .await
        .expect("claim package");
    }
    assert_eq!(w.count(&sql).await, 8);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while w.sessions_online() != 1 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "Bruno's init session stayed online"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Subscribe before reconnect so we can observe the low-stock bus publish.
    let bus = async_nats::connect(w.nats.as_ref().unwrap()).await.unwrap();
    let hash =
        Sha256::digest([b"zoen-subject-v1\0".as_slice(), bruno.identity.as_bytes()].concat());
    let subject = format!("zoen.{}.to.{}", w.cell, hex::encode(&hash[..16]));
    let mut peer = bus.subscribe(subject).await.unwrap();
    bus.flush().await.unwrap();

    let mut connecting = RawClient::reconnect(&w.relay_url(), bruno.clone()).await;
    let attached = tokio::time::Instant::now() + Duration::from_secs(3);
    while w.sessions_online() < 2 {
        assert!(
            tokio::time::Instant::now() < attached,
            "connecting device never attached"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    ana.request(Op::ClaimKeyPackages {
        ids: vec![bruno.identity.clone()],
        operation_id: None,
    })
    .await
    .expect("claim the eighth package");
    assert_eq!(w.count(&sql).await, 7);

    let low = tokio::time::timeout(Duration::from_secs(3), peer.next())
        .await
        .expect("low-stock publish timeout")
        .expect("low-stock publish");
    assert!(matches!(
        ServerFrame::decode(&low.payload).unwrap(),
        ServerFrame::KeyPackagesLow { remaining: 7, .. }
    ));

    let notice = tokio::time::timeout(Duration::from_secs(3), connecting.recv()).await;
    assert!(
        matches!(notice, Ok(ServerFrame::KeyPackagesLow { device, remaining: 7 }) if Some(&device) == bruno.device.as_ref()),
        "the connecting device missed refill after the concurrent claim"
    );
    Ok(())
}
