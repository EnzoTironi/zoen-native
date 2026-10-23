//! Complete controls scenarios; registration remains in the parent.

use super::*;

pub(super) async fn cancellation_deadline_and_single_receiver_guard_leave_pending_retryable() {
    let control = Arc::new(HistoryControl::default());
    let mut receive = control.begin(5_000).unwrap();
    assert!(control.begin(5_000).is_err());
    let cancel = control.clone();
    let cancellation = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        cancel.cancel();
    });
    assert!(receive
        .wait(std::future::pending::<Result<(), CoreError>>())
        .await
        .is_err());
    cancellation.await.unwrap();
    drop(receive);
    let mut retry = control.begin(10).unwrap();
    assert!(retry
        .wait(std::future::pending::<Result<(), CoreError>>())
        .await
        .is_err());
    drop(retry);
    assert!(control.begin(5_000).is_ok());
    assert!(!valid_history_manifest(&HistoryBox {
        transfer: "a".repeat(64),
        key: "b".repeat(64),
        chunks: vec![]
    }));
}

pub(super) fn history_ready_is_a_content_free_authenticated_primary_hint_only_while_pending() {
    let core = Client::open(":memory:".into(), "en-US".into()).unwrap();
    install_link(&core);
    let primary = core.lock().store.meta(META_PRIMARY).unwrap().unwrap();
    let recipient = core.account().unwrap().device_id;
    let (recipient_secret, recipient_public) = link::key_pair();
    let (sender_secret, sender_public) = link::key_pair();
    let secret = hex::encode([41; 32]);
    core.lock().add_peer(Peer {
        device: primary.clone(),
        their_hpke: hex::encode(sender_public),
        my_hpke_secret: hex::encode(recipient_secret),
        secret: secret.clone(),
    });
    let mut sender = crate::Engine::open(":memory:").unwrap();
    sender.add_peer(Peer {
        device: recipient.clone(),
        their_hpke: hex::encode(recipient_public),
        my_hpke_secret: hex::encode(sender_secret),
        secret,
    });
    sender.announce_initial_history(&recipient);
    let queued = sender.take_device_outbox();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].0, recipient);
    let sealed = &queued[0].1;
    assert!(!String::from_utf8_lossy(sealed).contains("HistoryReady"));
    {
        let mut e = core.lock();
        e.device_message("unknown", &recipient, sealed);
        assert!(!e.take_initial_history_ready());
        e.device_message(&primary, "another recipient", sealed);
        assert!(!e.take_initial_history_ready());
        let mut corrupt = sealed.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        e.device_message(&primary, &recipient, &corrupt);
        assert!(!e.take_initial_history_ready());
        e.device_message(&primary, &recipient, sealed);
        assert!(e.take_initial_history_ready());
        assert!(!e.take_initial_history_ready());
        e.store
            .set_meta("devices.revoked", &serde_json::json!([primary]).to_string())
            .unwrap();
        e.device_message(&primary, &recipient, sealed);
        assert!(!e.take_initial_history_ready());
        e.store.meta_delete("devices.revoked").unwrap();
        e.store.meta_delete(META_INITIAL_HISTORY_PENDING).unwrap();
        e.device_message(&primary, &recipient, sealed);
        assert!(!e.take_initial_history_ready());
    }
}
