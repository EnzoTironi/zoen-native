use super::*;
use crate::{Client, PrivacyDto, SecretVault};
use std::sync::{Arc, Mutex};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
#[derive(Default)]
struct Vault(Mutex<std::collections::HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().ok()?.get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        if let Ok(mut data) = self.0.lock() {
            data.insert(key, value);
            true
        } else {
            false
        }
    }
    fn delete(&self, key: String) {
        if let Ok(mut data) = self.0.lock() {
            data.remove(&key);
        }
    }
}
fn fixture(path: &str) -> TestResult<(Arc<Client>, Arc<Vault>, MediaPlaybackScope)> {
    let client = Client::open(path.into(), "en".into())?;
    let vault = Arc::new(Vault::default());
    let account = client.create_account(
        "Playback".into(),
        "playback_checkpoint".into(),
        "http://127.0.0.1:1".into(),
        vault.clone(),
    )?;
    let space =
        client.create_group_with("Private listening room".into(), vec![], PrivacyDto::Closed)?;
    let scope = post(
        &client,
        &space,
        &account.identity_id,
        &account.device_id,
        "first",
    )?;
    Ok((client, vault, scope))
}
fn post(
    client: &Client,
    space: &str,
    identity: &str,
    device: &str,
    mutation: &str,
) -> TestResult<MediaPlaybackScope> {
    let message = client.send_file_message(
        space.into(),
        "private.mp4".into(),
        "video/mp4".into(),
        vec![1, 2, 3, 4],
        None,
        None,
        false,
        mutation.into(),
    )?;
    let EntryKind::Message {
        card: Some(card), ..
    } = message.kind
    else {
        return Err("missing card".into());
    };
    let file = client.file_info_at(card.item_id.clone(), Some(card.version))?;
    Ok(MediaPlaybackScope {
        expected_identity_id: identity.into(),
        expected_device_id: device.into(),
        space_id: space.into(),
        message_id: message.id,
        item_id: card.item_id,
        version: card.version,
        sha256: file.sha256,
    })
}
fn position(ms: u32) -> MediaPlaybackPosition {
    MediaPlaybackPosition {
        position_ms: ms,
        duration_ms: 60000,
        rate_milli: 1500,
    }
}
#[test]
fn encrypted_checkpoint_cold_reopens_and_is_not_a_message_or_read_receipt() -> TestResult {
    let dir = std::env::temp_dir().join(types::new_id("playback-test")?);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("state.sqlite").to_string_lossy().into_owned();
    let (client, vault, scope) = fixture(&path)?;
    let before = client.lock().store.outbox()?.len();
    let saved = client
        .lock()
        .media_playback_save(&scope, "0", position(12345))?;
    assert_eq!(client.lock().store.outbox()?.len(), before);
    let engine = client.lock();
    let key = engine.playback_record(&scope)?.id;
    let ciphertext = engine.store.meta(&key)?.ok_or("record")?;
    for private in [
        &scope.message_id,
        &scope.item_id,
        &scope.space_id,
        &scope.sha256,
    ] {
        assert!(!ciphertext.contains(private));
    }
    drop(engine);
    drop(client);
    let reopened = Client::open(path, "en".into())?;
    assert!(reopened.unlock(vault)?);
    let loaded = reopened.lock().media_playback_read(&scope)?;
    assert_eq!(loaded.revision, saved.revision);
    assert_eq!(loaded.position, Some(position(12345)));
    drop(reopened);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn stale_player_invalid_values_scope_drift_and_removed_message_cannot_overwrite_progress(
) -> TestResult {
    let (client, _, scope) = fixture(":memory:")?;
    let saved = client
        .lock()
        .media_playback_save(&scope, "0", position(12000))?;
    assert!(matches!(
        client
            .lock()
            .media_playback_save(&scope, "0", position(1000)),
        Err(CoreError::Stale { .. })
    ));
    for value in [
        MediaPlaybackPosition {
            duration_ms: 0,
            ..position(0)
        },
        MediaPlaybackPosition {
            position_ms: 60001,
            ..position(0)
        },
        MediaPlaybackPosition {
            rate_milli: 999,
            ..position(0)
        },
        MediaPlaybackPosition {
            duration_ms: u32::MAX,
            ..position(0)
        },
    ] {
        assert!(client
            .lock()
            .media_playback_save(&scope, &saved.revision, value)
            .is_err());
    }
    for field in ["device", "hash", "message", "version"] {
        let mut other = scope.clone();
        match field {
            "device" => other.expected_device_id = "other".into(),
            "hash" => other.sha256 = "a".repeat(64),
            "message" => other.message_id = "other".into(),
            _ => other.version = 2,
        };
        assert!(client.lock().media_playback_read(&other).is_err());
    }
    assert_eq!(
        client.lock().media_playback_read(&scope)?.position,
        Some(position(12000))
    );
    client.delete_message(scope.space_id.clone(), scope.message_id.clone())?;
    assert!(client
        .lock()
        .media_playback_save(&scope, &saved.revision, position(3000))
        .is_err());
    Ok(())
}
#[test]
fn storage_failure_preserves_the_previous_checkpoint_and_retention_is_bounded() -> TestResult {
    let (client, _, scope) = fixture(":memory:")?;
    let saved = client
        .lock()
        .media_playback_save(&scope, "0", position(1000))?;
    client.lock().store.conn().execute_batch("CREATE TRIGGER reject_progress BEFORE UPDATE ON meta WHEN NEW.key LIKE 'media.playback.%' BEGIN SELECT RAISE(ABORT,'disk full'); END;")?;
    assert!(client
        .lock()
        .media_playback_save(&scope, &saved.revision, position(2000))
        .is_err());
    assert_eq!(
        client.lock().media_playback_read(&scope)?.position,
        Some(position(1000))
    );
    client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER reject_progress")?;
    for index in 0..128 {
        let other = post(
            &client,
            &scope.space_id,
            &scope.expected_identity_id,
            &scope.expected_device_id,
            &format!("other-{index}"),
        )?;
        client
            .lock()
            .media_playback_save(&other, "0", position(index))?;
    }
    assert_eq!(client.lock().media_playback_read(&scope)?.revision, "0");
    assert_eq!(
        client.lock().playback_record(&scope)?.record.entries.len(),
        128
    );
    Ok(())
}
#[tokio::test]
async fn json_contract_uses_exact_scope_and_rejects_unknown_fields() -> TestResult {
    let (client, _, scope) = fixture(":memory:")?;
    let mut request = serde_json::json!({"op":"media_playback_save","scope":scope,"expected_revision":"0","position":position(2500)});
    let response: MediaPlaybackBookmark =
        serde_json::from_str(&client.client_request(request.to_string()).await?)?;
    assert_eq!(response.position, Some(position(2500)));
    request["extra"] = true.into();
    assert!(client.client_request(request.to_string()).await.is_err());
    Ok(())
}

#[test]
fn portable_backup_excludes_listening_history_and_revoked_device_cannot_read_it() -> TestResult {
    let (client, _, scope) = fixture(":memory:")?;
    client
        .lock()
        .media_playback_save(&scope, "0", position(12000))?;
    let snapshot = client.lock().backup_snapshot()?;
    let path = std::env::temp_dir().join(types::new_id("playback-backup")?);
    std::fs::write(&path, snapshot)?;
    let portable = rusqlite::Connection::open(&path)?;
    let count: u32 = portable.query_row(
        "SELECT COUNT(*) FROM meta WHERE key LIKE 'media.playback.%'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(count, 0);
    drop(portable);
    std::fs::remove_file(path)?;
    assert_eq!(
        client.lock().media_playback_read(&scope)?.position,
        Some(position(12000))
    );
    client.lock().store.set_meta(
        "devices.revoked",
        &serde_json::to_string(&[&scope.expected_device_id])?,
    )?;
    assert!(client.lock().media_playback_read(&scope).is_err());
    Ok(())
}
