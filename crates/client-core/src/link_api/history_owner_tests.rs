use super::*;
use test_transport::{next_call, Fixture};

fn manifest(ch: char) -> HistoryBox {
    HistoryBox {
        transfer: ch.to_string().repeat(64),
        key: "b".repeat(64),
        chunks: vec!["c".repeat(64)],
    }
}
fn history_state(client: &Client) -> [Option<String>; 3] {
    [
        META_HISTORY,
        META_INITIAL_HISTORY_PENDING,
        META_HISTORY_COMPLETE,
    ]
    .map(|key| client.lock().store.meta(key).unwrap())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retired_history_owner_cannot_read_a_new_manifest_or_commit_its_import() {
    let f = Fixture::new().await.unwrap();
    f.online().await;
    let owner = owner::LinkOwner::capture(f.client()).unwrap();
    f.client().stop_sync();
    f.online().await;
    {
        let engine = f.client().lock();
        engine
            .store
            .set_meta(
                META_HISTORY,
                &serde_json::to_string(&manifest('d')).unwrap(),
            )
            .unwrap();
        engine
            .store
            .set_meta(META_INITIAL_HISTORY_PENDING, "1")
            .unwrap();
    }
    let before = history_state(f.client());
    let mut receive = f.client().history_receive.begin(5_000).unwrap();
    assert!(matches!(
        f.client().history_manifest(&mut receive, &owner).await,
        Err(CoreError::Stale { .. }),
    ));
    assert!(matches!(
        f.client()
            .commit_initial_history_owned(&owner, Vec::new(), 1, 0, &receive),
        Err(CoreError::Stale { .. }),
    ));
    assert_eq!(history_state(f.client()), before);
    assert!(f
        .client()
        .lock()
        .store
        .meta(META_HISTORY_COMPLETE)
        .unwrap()
        .is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_fetch_accepted_before_retirement_cannot_install_its_late_box_in_new_session() {
    let mut f = Fixture::new().await.unwrap();
    f.online().await;
    let (recipient_secret, recipient_public) = link::key_pair();
    let (_, primary_public) = link::key_pair();
    let primary = Signer::from_secret(&[75; 32]).id();
    let secret = [76; 32];
    {
        let mut engine = f.client().lock();
        engine.store.set_meta(META_PRIMARY, &primary).unwrap();
        engine.add_peer(Peer {
            device: primary,
            their_hpke: hex::encode(primary_public),
            my_hpke_secret: hex::encode(recipient_secret),
            secret: hex::encode(secret),
        });
    }
    let owner = owner::LinkOwner::capture(f.client()).unwrap();
    let client = f.client().clone();
    let mut receive = client.history_receive.begin(5_000).unwrap();
    let pending = tokio::spawn(async move { client.history_manifest(&mut receive, &owner).await });
    let accepted = next_call(&mut f.relay.calls).await;
    assert!(
        matches!(&accepted.op, Op::FetchLink { id } if id == &link::box_id(&secret, "history"))
    );
    let old_box = link::seal(
        &recipient_public,
        &secret,
        "history",
        &serde_json::to_vec(&manifest('a')).unwrap(),
    )
    .unwrap();
    // The synthetic server has already accepted FetchLink. Retirement cancels
    // this waiter, not the remote effect; this test makes no recovery claim for
    // the protocol's destructive one-time fetch on a lost response.
    f.client().stop_sync();
    assert!(pending.await.unwrap().is_err());
    f.online().await;
    f.client()
        .lock()
        .store
        .set_meta(
            META_HISTORY,
            &serde_json::to_string(&manifest('e')).unwrap(),
        )
        .unwrap();
    let before = history_state(f.client());
    accepted
        .result
        .send(Ok(Reply::Link(Some(old_box))))
        .unwrap();
    // The old caller was already joined before the late reply. This current
    // operation only verifies that the replacement Net remains usable.
    assert!(!f.client().devices_impl().await.unwrap().is_empty());
    assert_eq!(history_state(f.client()), before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retired_history_signer_cannot_borrow_a_replacement_accounts_key() {
    let f = Fixture::new().await.unwrap();
    f.online().await;
    let owner = owner::LinkOwner::capture(f.client()).unwrap();
    let old = f.client().account().unwrap();
    f.client().erase_device(f.vault.clone()).unwrap();
    f.client()
        .create_account(
            "Replacement".into(),
            "replacement".into(),
            f.relay.url.clone(),
            f.vault.clone(),
        )
        .unwrap();
    f.client().lock().set_registered(true).unwrap();
    f.online().await;
    let current = f.client().account().unwrap();
    assert_ne!(current.identity_id, old.identity_id);
    assert_ne!(current.device_id, old.device_id);
    assert!(matches!(
        owner.signed_headers(f.client(), |ts| {
            transfer_message("put", &"a".repeat(64), "0", &"b".repeat(64), ts)
        }),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(
        f.client().account().unwrap().identity_id,
        current.identity_id
    );
    assert_eq!(f.client().account().unwrap().device_id, current.device_id);
}
