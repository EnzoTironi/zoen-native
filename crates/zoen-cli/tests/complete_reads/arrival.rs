//! A local empty-work predicate cannot acknowledge an unseen remote membership.
use super::*;
use common::claim_proxy::{Boundary, ClaimProxy, Observed};

/// These waits settle only each client's known work. Use the same single 20s
/// budget to require the recipient's actual plaintext projection as well; reads
/// do not send Sync, retry a publication or manufacture a receive acknowledgement.
pub(super) async fn settled_message(owner: &Online, peer: &Online, space: &str, text: &str) {
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        owner.0.wait_until_settled(20_000).await.unwrap();
        peer.0.wait_until_settled(20_000).await.unwrap();
        loop {
            match peer.0.timeline(space.into()) {
                Ok(entries) => {
                    if entries.iter().any(|entry| {
                        matches!(&entry.kind,
                        client_core::EntryKind::Message { text: actual, .. } if actual == text)
                    }) {
                        return;
                    }
                }
                Err(client_core::CoreError::NotFound { .. }) => {}
                Err(error) => panic!("recipient timeline failed: {error:?}"),
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await;
    assert!(
        result.is_ok(),
        "recipient did not observe {text:?} in {space} within the shared 20s budget; {:?}",
        peer.0.connection()
    );
}

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
    let reception = settled_message(&owner, &peer, &space, text);
    let mut reception = std::pin::pin!(reception);
    assert!(matches!(
        futures_util::poll!(reception.as_mut()),
        std::task::Poll::Pending
    ));
    proxy.release.notify_one();
    reception.await;
}
