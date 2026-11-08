// Exercise the same scoped JSON commands as Expo against the actual relay and vault.
#[tokio::test]
async fn scoped_native_backup_recovers_history_and_rejects_foreign_controls(
) -> Result<(), Box<dyn std::error::Error>> {
    use client_core::Client;
    use std::sync::Arc;
    let w = world("backup_scoped_native").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    let before = "Scoped recovery retains this encrypted conversation";
    w.zoen("bruno", &["dm", "@ana", before]);
    assert!(w.zoen("ana", &["read", "@bruno"]).contains(before));
    let home = w.dir.join("ana");
    let client = Client::open(home.join("zoen.sqlite").to_string_lossy().into_owned(), "en".into())?;
    let vault = Arc::new(file_vault::FileVault { dir: home.join("vault") });
    assert!(client.unlock(vault.clone())?);
    let account = client.account().ok_or("account missing")?;
    let scope = serde_json::json!({
        "expected_identity_id": account.identity_id,
        "expected_device_id": account.device_id,
        "expected_relay_url": account.relay_url,
    });
    let generated = client.client_vault_request(serde_json::json!({
        "op": "backup_create_recovery_key", "scope": scope,
    }).to_string(), vault.clone()).await?;
    let key: String = serde_json::from_str(&generated)?;
    let enabled = client.client_vault_request(serde_json::json!({
        "op": "backup_enable_recovery_key", "scope": scope, "recovery_key": key,
    }).to_string(), vault.clone()).await?;
    let verify_receipt = |reply: &str| -> Result<(), Box<dyn std::error::Error>> {
        let receipt: serde_json::Value = serde_json::from_str(reply)?;
        assert_eq!(receipt["enabled"], true);
        assert_eq!(receipt["in_progress"], false);
        // The native JSON codec represents u64/i64 as decimal strings for JS.
        assert!(receipt["bytes"].as_str().ok_or("wire bytes")?.parse::<u64>()? > 0);
        Ok(())
    };
    verify_receipt(&enabled)?;
    let mut foreign = scope.clone();
    foreign["expected_device_id"] = "another-device".into();
    for op in ["backup_now", "backup_turn_off"] {
        assert!(client.client_vault_request(serde_json::json!({
            "op": op, "scope": foreign,
        }).to_string(), vault.clone()).await.is_err());
        assert!(client.backup_status().enabled);
        assert_eq!(w.count("SELECT count(*) FROM backup_vaults").await, 1);
    }
    let uploaded = client.client_vault_request(serde_json::json!({
        "op": "backup_now", "scope": scope,
    }).to_string(), vault.clone()).await?;
    verify_receipt(&uploaded)?;
    drop(client);
    let restored = w.zoen("ana-restored", &["recover", "@ana", "--recovery-key", &key]);
    assert!(restored.contains("connection=online"), "{restored}");
    assert!(w.zoen("ana-restored", &["read", "@bruno"]).contains(before));
    let blobs = backup_files(&w);
    assert!(!blobs.is_empty());
    assert!(!blobs.iter().any(|blob| contains(blob, before) || contains(blob, &key)));
    let client = Client::open(home.join("zoen.sqlite").to_string_lossy().into_owned(), "en".into())?;
    assert!(client.unlock(vault.clone())?);
    client.client_vault_request(serde_json::json!({
        "op": "backup_turn_off", "scope": scope,
    }).to_string(), vault).await?;
    assert!(!client.backup_status().enabled);
    assert_eq!(w.count("SELECT count(*) FROM backup_vaults").await, 0);
    Ok(())
}
