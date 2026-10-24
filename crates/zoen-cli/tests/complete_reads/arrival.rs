//! A local empty-work predicate cannot acknowledge an unseen remote membership.
use super::*;
use common::claim_proxy::{Boundary, ClaimProxy, Observed};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_settlement_does_not_prove_new_group_message_reception() {
    let w = World::new("complete_reads_arrival").await;
    let mut proxy = ClaimProxy::new(w.port, Boundary::Joined).await;
    let port = proxy.url.rsplit_once(':').unwrap().1.parse().unwrap();
    w.init("owner", "Owner");
    w.zoen_at(
        port,
        "peer",
        &["init", "--name", "Peer", "--handle", "peer"],
    );
    let owner = Online::open(&w, "owner").await;
    let peer = Online::open(&w, "peer").await;
    let space = owner
        .0
        .create_group_with(
            "Delayed membership".into(),
            vec![peer.0.account().unwrap().identity_id],
            PrivacyDto::EndToEnd,
        )
        .unwrap();
    let text = "Actual encrypted source";
    owner.0.send_message(space.clone(), text.into()).unwrap();
    let Observed::Joined(joined) = proxy.next().await else {
        panic!("expected the real new-space notification");
    };
    assert_eq!(joined, space);
    owner.0.wait_until_settled(20_000).await.unwrap();
    peer.0.wait_until_settled(20_000).await.unwrap();
    assert!(peer.0.space(space.clone()).is_err());
    assert!(peer.0.timeline(space.clone()).is_err());
    // Both original local waits succeeded while the actual membership notice
    // (and the ordered Welcome/message behind it) still cannot reach the peer.
    proxy.release.notify_one();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        if peer.0.timeline(space.clone()).is_ok_and(|entries| {
            entries.iter().any(|entry| {
                matches!(&entry.kind,
                client_core::EntryKind::Message { text: actual, .. } if actual == text)
            })
        }) {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "released message missing"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
