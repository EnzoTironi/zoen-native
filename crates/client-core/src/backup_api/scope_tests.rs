//! All values are synthetic. Review/gate checks must precede vault or network effects.
use super::*;
use crate::client_api::backup_command::BackupScope;
use std::{collections::HashMap, sync::Mutex};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
#[derive(Default)]
struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, name: String) -> Option<Vec<u8>> {
        self.0.lock().ok()?.get(&name).cloned()
    }
    fn save(&self, name: String, value: Vec<u8>) -> bool {
        let Ok(mut map) = self.0.lock() else {
            return false;
        };
        map.insert(name, value);
        true
    }
    fn delete(&self, name: String) {
        if let Ok(mut map) = self.0.lock() {
            map.remove(&name);
        }
    }
}
fn account() -> TestResult<(Arc<Client>, Arc<Vault>, BackupScope)> {
    let client = Client::open(":memory:".into(), "en".into())?;
    let vault = Arc::new(Vault::default());
    client.create_account(
        "Backup owner".into(),
        "backup_owner".into(),
        "http://127.0.0.1:1".into(),
        vault.clone(),
    )?;
    {
        let mut engine = client.lock();
        engine
            .net
            .account
            .as_mut()
            .ok_or("native account")?
            .registered = true;
    }
    let account = client.account().ok_or("account DTO")?;
    let scope = BackupScope {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        expected_relay_url: account.relay_url,
    };
    Ok((client, vault, scope))
}
#[test]
fn backup_scope_checks_native_owner_device_relay_and_revocation() -> TestResult {
    let (client, _, scope) = account()?;
    scope.check(&client)?;
    let mut foreign = scope.clone();
    foreign.expected_identity_id = "foreign".into();
    assert!(foreign.check(&client).is_err());
    foreign = scope.clone();
    foreign.expected_device_id = "foreign-device".into();
    assert!(foreign.check(&client).is_err());
    foreign = scope.clone();
    foreign.expected_relay_url = "https://another.invalid".into();
    assert!(foreign.check(&client).is_err());
    client.lock().store.set_meta(
        "devices.revoked",
        &serde_json::to_string(&vec![scope.expected_device_id.clone()])?,
    )?;
    assert!(scope.check(&client).is_err());
    client.lock().store.set_meta("devices.revoked", "corrupt")?;
    assert!(scope.check(&client).is_err());
    Ok(())
}
#[tokio::test]
async fn native_backup_writer_refuses_concurrent_mutations_and_does_not_persist_activity(
) -> TestResult {
    let (client, vault, _) = account()?;
    assert!(!client.backup_status().in_progress);
    let writer = client.backup_write_guard()?;
    assert!(client.backup_status().in_progress);
    let stored = BackupStatusDto {
        enabled: true,
        in_progress: true,
        ..Default::default()
    };
    client.save_status(&stored)?;
    assert!(!client
        .lock()
        .store
        .meta(STATUS_META)?
        .ok_or("receipt")?
        .contains("in_progress"));
    let before = vault.0.lock().map_err(|e| e.to_string())?.clone();
    for result in [
        client.backup_now(vault.clone()).await.map(|_| ()),
        client.backup_turn_off(vault.clone()).await,
        client
            .backup_enable_recovery_key("00".repeat(32), vault.clone())
            .await
            .map(|_| ()),
        client
            .backup_turn_on_password("a strong synthetic password".into(), vault.clone())
            .await
            .map(|_| ()),
    ] {
        let error = result.err().ok_or("concurrent write was admitted")?;
        assert!(error.to_string().contains("already in progress"));
    }
    assert_eq!(*vault.0.lock().map_err(|e| e.to_string())?, before);
    drop(writer);
    assert!(!client.backup_status().in_progress);
    let _next = client.backup_write_guard()?;
    Ok(())
}
#[tokio::test]
async fn scoped_backup_json_retains_legacy_shape_but_refuses_foreign_actor_before_generation(
) -> TestResult {
    let (client, vault, scope) = account()?;
    let legacy: String = serde_json::from_str(
        &client
            .client_vault_request(
                "{\"op\":\"backup_create_recovery_key\"}".into(),
                vault.clone(),
            )
            .await?,
    )?;
    backup::recovery_keys(&legacy)?;
    let review = serde_json::json!({ "expected_identity_id":scope.expected_identity_id, "expected_device_id":scope.expected_device_id,"expected_relay_url":scope.expected_relay_url });
    let command = serde_json::json!({ "op":"backup_create_recovery_key", "scope":review });
    let generated: String = serde_json::from_str(
        &client
            .client_vault_request(command.to_string(), vault.clone())
            .await?,
    )?;
    backup::recovery_keys(&generated)?;
    let mut foreign = command;
    foreign["scope"]["expected_device_id"] = "other-device".into();
    assert!(client
        .client_vault_request(foreign.to_string(), vault)
        .await
        .is_err());
    Ok(())
}
