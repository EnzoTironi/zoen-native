use super::*;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.0.lock().unwrap().insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        self.0.lock().unwrap().remove(&key);
    }
}

struct Fixture {
    client: Arc<Client>,
    identity: String,
    device: String,
    space: String,
}
impl Fixture {
    fn at(path: String) -> Self {
        let client = Client::open(path, "en".into()).unwrap();
        client
            .create_account(
                "Mute owner".into(),
                "mute_owner".into(),
                "http://127.0.0.1:1".into(),
                Arc::new(Vault::default()),
            )
            .unwrap();
        let account = client.account().unwrap();
        let space = client
            .create_group_with("Muted group".into(), vec![], PrivacyDto::Closed)
            .unwrap();
        Self {
            client,
            identity: account.identity_id,
            device: account.device_id,
            space,
        }
    }
    fn new() -> Self {
        Self::at(":memory:".into())
    }
    fn command(&self, muted: bool) -> serde_json::Value {
        json!({
            "op":"set_chat_muted", "space_id":self.space, "muted":muted,
            "expected_identity_id":self.identity, "expected_device_id":self.device
        })
    }
    async fn mute(&self, muted: bool) -> Result<String, CoreError> {
        self.client
            .client_request(self.command(muted).to_string())
            .await
    }
    fn counts(&self) -> (u64, usize) {
        let engine = self.client.lock();
        (
            engine.store.event_count().unwrap(),
            engine.store.outbox().unwrap().len(),
        )
    }
    fn summary(&self) -> SpaceSummary {
        self.client.lock().space_summary(&self.space).unwrap()
    }
}

#[tokio::test]
async fn merges_latest_local_pin_archive_instead_of_a_stale_review_and_replays() {
    let directory = std::env::temp_dir().join(types::new_id("chat-muting-test").unwrap());
    std::fs::create_dir(&directory).unwrap();
    let path = directory
        .join("synthetic.sqlite")
        .to_string_lossy()
        .into_owned();
    let f = Fixture::at(path.clone());
    f.client
        .set_chat_prefs(f.space.clone(), false, false, Some(i64::MAX))
        .unwrap();
    let reviewed = f.summary();
    assert!(!reviewed.pinned && !reviewed.archived && reviewed.muted);
    // Another local action lands after the UI's read, before its mute-only command.
    f.client
        .set_chat_prefs(f.space.clone(), true, true, Some(i64::MAX))
        .unwrap();
    assert_eq!(f.mute(false).await.unwrap(), "null");
    let current = f.summary();
    assert!(current.pinned && current.archived && !current.muted);
    assert_eq!(current.muted_until_ms, None);
    let space = f.space.clone();
    drop(f);
    let reopened = Client::open(path, "en".into()).unwrap();
    let replayed = reopened.lock().space_summary(&space).unwrap();
    assert!(replayed.pinned && replayed.archived && !replayed.muted);
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn mute_only_command_preserves_other_chats_and_idempotent_writes() {
    let f = Fixture::new();
    let other = f
        .client
        .create_group_with("Other group".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    f.client
        .set_chat_prefs(other.clone(), true, false, Some(i64::MAX))
        .unwrap();
    let other_before = f.client.lock().space_summary(&other).unwrap();
    f.mute(true).await.unwrap();
    let muted = f.summary();
    assert!(!muted.pinned && !muted.archived && muted.muted);
    assert_eq!(muted.muted_until_ms, Some(i64::MAX));
    let count = f.counts();
    f.mute(true).await.unwrap();
    assert_eq!(
        f.counts(),
        count,
        "existing writer deduplicates the same preference"
    );
    f.mute(false).await.unwrap();
    let count = f.counts();
    f.mute(false).await.unwrap();
    assert_eq!(f.counts(), count);
    let after = f.client.lock().space_summary(&other).unwrap();
    assert_eq!(after.pinned, other_before.pinned);
    assert_eq!(after.archived, other_before.archived);
    assert_eq!(after.muted_until_ms, other_before.muted_until_ms);
}

#[tokio::test]
async fn wrong_owner_device_locked_or_setup_session_never_writes() {
    let f = Fixture::new();
    let before = f.counts();
    for field in ["expected_identity_id", "expected_device_id"] {
        let mut command = f.command(true);
        command[field] = "other".into();
        assert!(f.client.client_request(command.to_string()).await.is_err());
        assert_eq!(f.counts(), before);
    }
    let author = f.client.lock().net.author.take().unwrap();
    assert!(f.mute(true).await.is_err());
    assert_eq!(f.counts(), before);
    f.client.lock().net.author = Some(author);
    f.client
        .lock()
        .store
        .set_meta("provider_signup_pending", "true")
        .unwrap();
    assert!(f.mute(true).await.is_err());
    assert_eq!(f.counts(), before);
}

#[tokio::test]
async fn revoked_or_unreadable_device_admission_and_lost_membership_never_write() {
    let f = Fixture::new();
    let before = f.counts();
    for value in [json!([f.device]).to_string(), "not JSON".into()] {
        f.client
            .lock()
            .store
            .set_meta("devices.revoked", &value)
            .unwrap();
        assert!(f.mute(true).await.is_err());
        assert_eq!(f.counts(), before);
    }
    f.client
        .lock()
        .store
        .meta_delete("devices.revoked")
        .unwrap();
    f.client.leave_space(f.space.clone()).unwrap();
    let after_leaving = f.counts();
    assert!(f.mute(true).await.is_err());
    assert_eq!(f.counts(), after_leaving);
}

#[tokio::test]
async fn unknown_chat_and_full_preference_fields_are_rejected_without_writes() {
    let f = Fixture::new();
    let before = f.counts();
    let mut command = f.command(true);
    command["space_id"] = "sp_missing".into();
    assert!(f.client.client_request(command.to_string()).await.is_err());
    let mut command = f.command(true);
    command["pinned"] = true.into();
    assert!(f.client.client_request(command.to_string()).await.is_err());
    assert_eq!(f.counts(), before);
}

#[tokio::test]
async fn expired_mute_is_absent_from_the_canonical_directory_and_unmute_preserves_latest_prefs() {
    let f = Fixture::new();
    f.client
        .set_chat_prefs(f.space.clone(), true, true, Some(1))
        .unwrap();
    let directory: serde_json::Value = serde_json::from_str(
        &f.client
            .client_request(json!({"op": "snapshot"}).to_string())
            .await
            .unwrap(),
    )
    .unwrap();
    let summary = directory["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|chat| chat["id"] == f.space)
        .unwrap();
    assert_eq!(summary["muted"], false);
    assert_eq!(summary["muted_until_ms"], serde_json::Value::Null);
    f.mute(false).await.unwrap();
    let current = f.summary();
    assert!(current.pinned && current.archived && !current.muted);
}
