//! Focused recovery UI API boundaries. No secrets are printed, even on assertion failure.
use crate::{api::SecretVault, backup, RodaEngine};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Default)]
struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn save(&self, name: String, value: Vec<u8>) -> bool {
        self.0.lock().unwrap().insert(name, value);
        true
    }
    fn load(&self, name: String) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&name).cloned()
    }
    fn delete(&self, name: String) {
        self.0.lock().unwrap().remove(&name);
    }
}

async fn server(
    responses: Vec<(&'static str, &'static str)>,
) -> (String, tokio::task::JoinHandle<Vec<Vec<u8>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut bodies = Vec::new();
        for (status, body) in responses {
            let (mut socket, _) = tokio::time::timeout(Duration::from_secs(10), listener.accept())
                .await
                .unwrap()
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 4096];
            let (header_end, length) = loop {
                let n = tokio::time::timeout(Duration::from_secs(10), socket.read(&mut buffer))
                    .await
                    .unwrap()
                    .unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(pos) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..pos]);
                    let length: usize = header
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|n| n.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    break (pos + 4, length);
                }
            };
            while bytes.len() < header_end + length {
                let n = tokio::time::timeout(Duration::from_secs(10), socket.read(&mut buffer))
                    .await
                    .unwrap()
                    .unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
            }
            bodies.push(bytes[header_end..header_end + length].to_vec());
            let response = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}", body.len());
            socket.write_all(response.as_bytes()).await.unwrap();
        }
        bodies
    });
    (address, task)
}

#[tokio::test]
async fn recovery_ui_retains_confirmed_key_after_upload_failure_and_retry() {
    let (relay, server) = server(vec![
        ("200 OK", "{}"),
        ("503 Service Unavailable", "offline"),
        ("200 OK", "{}"),
        ("200 OK", "{}"),
    ])
    .await;
    let engine = RodaEngine::open(":memory:".into(), "pt-BR".into()).unwrap();
    let vault: Arc<dyn SecretVault> = Arc::new(Vault::default());
    engine
        .create_account("Ana".into(), "ana".into(), relay, vault.clone())
        .unwrap();
    // Enrollment is a fixture; the mock tests backup HTTP failures, not relay admission.
    engine
        .lock()
        .store
        .set_meta("relay_name", "test-relay")
        .unwrap();
    let key = engine.backup_create_recovery_key();
    assert!(!engine.backup_status().enabled); // Merely showing/confirming a key has no side effect.
    assert!(engine
        .backup_enable_recovery_key(key.clone(), vault.clone())
        .await
        .is_err());
    assert!(engine.backup_status().last_backup_ms == 0);
    let status = engine
        .backup_enable_recovery_key(key.clone(), vault.clone())
        .await
        .unwrap();
    assert!(status.enabled && status.last_backup_ms > 0);
    let bodies = server.await.unwrap();
    let keys = backup::recovery_keys(&key).unwrap();
    let identity = engine.account().unwrap().identity_id;
    let original = vault.load(backup::VAULT_BACKUP.into()).unwrap();
    for index in [0, 2] {
        let body: serde_json::Value = serde_json::from_slice(&bodies[index]).unwrap();
        let wrapped = hex::decode(body["wrapped_key"].as_str().unwrap()).unwrap();
        let opened = backup::unwrap_key(&keys, &identity, &wrapped).unwrap();
        assert!(opened.as_slice() == original.as_slice());
        let wrong = backup::recovery_keys(&"0".repeat(64)).unwrap();
        assert!(backup::unwrap_key(&wrong, &identity, &wrapped).is_err());
    }
}

#[tokio::test]
async fn recovery_ui_rejects_malformed_keys_before_network() {
    let engine = RodaEngine::open(":memory:".into(), "pt-BR".into()).unwrap();
    let vault: Arc<dyn SecretVault> = Arc::new(Vault::default());
    for key in [
        "1".repeat(63),
        "1".repeat(65),
        format!("{}x", "1".repeat(64)),
        "١".repeat(64),
    ] {
        assert!(backup::recovery_keys(&key).is_err());
        assert!(engine
            .restore_backup_with_recovery_key(
                "invalid origin".into(),
                "ana".into(),
                key,
                vault.clone()
            )
            .await
            .is_err());
    }
}

#[tokio::test]
async fn recovery_ui_refuses_password_mode_before_spending_a_guess() {
    let response = r#"{"mode":"passphrase","identity":"fixture","kdf":{},"generation":null}"#;
    let (relay, server) = server(vec![("200 OK", response)]).await;
    let engine = RodaEngine::open(":memory:".into(), "pt-BR".into()).unwrap();
    let vault: Arc<dyn SecretVault> = Arc::new(Vault::default());
    let key = engine.backup_create_recovery_key();
    let result = engine
        .restore_backup_with_recovery_key(relay, "ana".into(), key, vault)
        .await;
    assert!(
        matches!(result, Err(crate::CoreError::Invalid { reason }) if reason.contains("Esta cópia usa senha"))
    );
    assert!(engine.account().is_none());
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 1);
    let start: serde_json::Value = serde_json::from_slice(&requests[0]).unwrap();
    assert!(
        start["blinded"].is_null(),
        "key-only restore must not send a password OPRF point"
    );
    assert_eq!(start["recovery_only"], true);
}
