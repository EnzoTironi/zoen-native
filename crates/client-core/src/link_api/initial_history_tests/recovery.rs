//! Complete recovery scenarios; registration remains in the parent.

use super::*;

pub(super) async fn link_then_cold_reopen_before_history_retains_pending_and_same_keys(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let file = path()?;
    let core = Client::open(file.to_string_lossy().into(), "en-US".into()).unwrap();
    let vault = install_link(&core);
    let account = core.account().unwrap();
    let identity_key = vault.load(VAULT_IDENTITY.into());
    let device_key = vault.load(VAULT_DEVICE.into());
    assert!(core.history_status().unwrap().initial_pending);
    drop(core);
    let reopened = Client::open(file.to_string_lossy().into(), "en-US".into()).unwrap();
    assert!(reopened.unlock(vault.clone()).unwrap());
    assert_eq!(reopened.account().unwrap().identity_id, account.identity_id);
    let status = reopened.history_status().unwrap();
    assert!(status.initial_pending);
    assert!(!status.manifest_received);
    assert!(reopened.receive_history(20, None).await.is_err());
    assert!(reopened.history_status().unwrap().initial_pending);
    assert_eq!(vault.load(VAULT_IDENTITY.into()), identity_key);
    assert_eq!(vault.load(VAULT_DEVICE.into()), device_key);
    drop(reopened);
    let _ = std::fs::remove_file(file);

    Ok(())
}

pub(super) async fn missing_or_forged_history_cannot_clear_pending_or_partially_import(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let core = Client::open(":memory:".into(), "en-US".into()).unwrap();
    install_link(&core);
    let opened = encrypted_record(&core)?;
    let receive = core.history_receive.begin(5_000).unwrap();
    let mut missing = opened.clone();
    missing.space = "sp_not_synced".into();
    assert!(core
        .commit_initial_history(vec![opened.clone(), missing], 1, 0, &receive)
        .is_err());
    assert!(core.history_status().unwrap().initial_pending);
    assert!(matches!(
        core.lock().logs["sp_initial_history"].events()[1].body,
        EventBody::Sealed { .. }
    ));
    assert!(core
        .lock()
        .store
        .meta(META_HISTORY_COMPLETE)
        .unwrap()
        .is_none());
    let mut forged = opened.clone();
    forged.sig = "00".repeat(64);
    assert!(core
        .commit_initial_history(vec![forged], 1, 0, &receive)
        .is_err());
    assert!(core.history_status().unwrap().initial_pending);
    assert_eq!(
        core.commit_initial_history(vec![opened], 1, 0, &receive)
            .unwrap()
            .messages,
        1
    );
    assert!(!core.history_status().unwrap().initial_pending);
    assert!(matches!(
        core.lock().logs["sp_initial_history"].events()[1].body,
        EventBody::MessagePosted { .. }
    ));

    Ok(())
}

pub(super) async fn durable_import_and_receipt_survive_reopen_before_relay_cleanup_or_ui_ack(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let file = path()?;
    let core = Client::open(file.to_string_lossy().into(), "en-US".into()).unwrap();
    let vault = install_link(&core);
    let opened = encrypted_record(&core)?;
    let receive = core.history_receive.begin(5_000).unwrap();
    core.lock()
        .store
        .set_meta(META_HISTORY, r#"{"transfer":"old"}"#)
        .unwrap();
    core.lock()
        .store
        .set_meta(&chunk_meta(0), "retained sealed bytes")
        .unwrap();
    let receipt = core
        .commit_initial_history(vec![opened], 1, 1, &receive)
        .unwrap();
    drop(receive);
    drop(core);
    let reopened = Client::open(file.to_string_lossy().into(), "en-US".into()).unwrap();
    assert!(reopened.unlock(vault).unwrap());
    assert!(!reopened.history_status().unwrap().initial_pending);
    assert!(reopened.lock().store.meta(META_HISTORY).unwrap().is_none());
    assert!(reopened
        .lock()
        .store
        .meta(&chunk_meta(0))
        .unwrap()
        .is_none());
    // No relay connection exists: a successful replay must use the committed receipt.
    assert_eq!(
        reopened.receive_history(5_000, None).await.unwrap(),
        receipt
    );
    assert!(matches!(
        reopened.lock().logs["sp_initial_history"].events()[1].body,
        EventBody::MessagePosted { .. }
    ));
    drop(reopened);
    let _ = std::fs::remove_file(file);

    Ok(())
}
