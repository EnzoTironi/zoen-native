use super::*;
use event_log::{Author, SpaceLog};
use std::{collections::HashMap, sync::Mutex};
use types::{EventBody, Privacy, SpaceKind};

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
fn install_link(core: &Client) -> Arc<Vault> {
    let vault = Arc::new(Vault::default());
    finish_test_link(core, vault.clone()).unwrap();
    vault
}
fn finish_test_link(core: &Client, vault: Arc<dyn SecretVault>) -> Result<AccountDto, CoreError> {
    let identity = Signer::from_secret(&[21; 32]);
    let device = Signer::from_secret(&[22; 32]);
    let (hpke_secret, hpke_public) = link::key_pair()?;
    let (_, primary_hpke) = link::key_pair()?;
    let pending = Pending {
        device_secret: hex::encode(device.secret()),
        hpke_secret: hex::encode(hpke_secret),
        hpke_public: hex::encode(hpke_public),
        secret: hex::encode([23; 32]),
        relay: "http://localhost:8787".into(),
    };
    let boxed = IdentityBox {
        identity_secret: hex::encode(identity.secret()),
        agreement_secret: hex::encode([24; 32]),
        cert: identity.sign(&event_log::device_cert_message(&device.id())),
        name: "Linked person".into(),
        handle: "linked_person".into(),
        relay: pending.relay.clone(),
        primary: Signer::from_secret(&[25; 32]).id(),
        primary_hpke: hex::encode(primary_hpke),
        profile_key: hex::encode([27; 32]),
        profile_key_version: 1,
        profile_seen_version: 1,
        profile_fields: hex::encode(
            event_log::profile::ProfileFields {
                name: "Linked person".into(),
                ..Default::default()
            }
            .to_bytes(),
        ),
    };
    core.finish_link(
        &pending,
        &device,
        boxed,
        vault,
        &ExpectedIdentity::parse(None).unwrap(),
    )
}
fn path() -> std::result::Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    Ok(std::env::temp_dir().join(format!(
        "zoen-initial-history-{}.sqlite",
        types::new_id("test")?
    )))
}
fn encrypted_record(
    core: &Client,
) -> std::result::Result<types::Event, Box<dyn std::error::Error>> {
    use event_log::content::{Sealed, SealedKind};
    let identity = Signer::from_secret(&[21; 32]);
    let author = Author::device(&identity, Signer::from_secret(&[26; 32]));
    let mut log = SpaceLog::new("sp_initial_history");
    let genesis = log
        .append(
            &identity,
            1,
            EventBody::SpaceCreated {
                title: "Private".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        )?
        .clone();
    let mut opened = author.sign_event(
        log.space(),
        "msg_initial",
        2,
        log.head(),
        EventBody::MessagePosted {
            message: "msg_initial".into(),
            text: "Actual durable history".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    )?;
    let (content, sig) = author.sign_sealed(
        log.space(),
        "msg_initial",
        2,
        log.head().as_ref(),
        Sealed::new(SealedKind::Application, 1, vec![1, 2, 3]),
    );
    let outer = event_log::event_from_content(
        content,
        sig,
        author.cert.clone(),
        0,
        String::new(),
        String::new(),
    )
    .unwrap();
    let local = log.sequence(outer)?.clone();
    opened.seq = local.seq;
    opened.sealed_wire = Some(event_log::content_hash_of(&local));
    let mut e = core.lock();
    e.store.append_events(&[genesis, local]).unwrap();
    e.reload().unwrap();
    Ok(opened)
}

fn owned_personal_spaces(core: &Client) -> Vec<String> {
    let e = core.lock();
    let me = e.me_id().unwrap();
    e.state
        .spaces
        .values()
        .filter(|space| {
            space.kind == SpaceKind::Personal
                && space
                    .members
                    .iter()
                    .any(|(id, role)| id == &me && *role == types::Role::Owner)
        })
        .map(|space| space.id.clone())
        .collect()
}

struct PreferencesFile(std::path::PathBuf);
impl Drop for PreferencesFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        for suffix in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}

async fn assert_linked_preferences_survive_history_and_reopen(
    legacy: bool,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let file = PreferencesFile(path()?);
    let core = Client::open(file.0.to_string_lossy().into(), "en-US".into()).unwrap();
    let vault = install_link(&core);
    let account = core.account().unwrap();
    let identity_key = vault.load(VAULT_IDENTITY.into());
    let device_key = vault.load(VAULT_DEVICE.into());
    if legacy {
        // Represents the persisted pre-fix linked database: identity/device and
        // history are valid, but no Personal log was ever installed. Only this
        // isolated fixture's local Personal rows are removed; no account/key,
        // target chat or relay event is rewritten.
        let personal = owned_personal_spaces(&core);
        let mut e = core.lock();
        for id in personal {
            e.store
                .conn()
                .execute("DELETE FROM events WHERE space = ?1", [&id])
                .unwrap();
        }
        e.reload().unwrap();
    } else {
        assert_eq!(owned_personal_spaces(&core).len(), 1);
    }
    let opened = encrypted_record(&core)?;
    let receive = core.history_receive.begin(5_000).unwrap();
    assert_eq!(
        core.commit_initial_history(vec![opened], 1, 0, &receive)
            .unwrap()
            .messages,
        1
    );
    drop(receive);
    assert!(!core.history_status().unwrap().initial_pending);
    let target = "sp_initial_history".to_owned();
    let before = core.lock().space_summary(&target).unwrap();
    assert!(before.members.iter().any(|member| member.is_me));
    assert!(!before.archived && !before.pinned && !before.muted);
    assert_eq!(core.timeline(target.clone()).unwrap().len(), 1);
    if legacy {
        assert!(owned_personal_spaces(&core).is_empty());
        // Reject a nonexistent target before a repair can create a resource.
        let failure = core
                .client_request(
                    r#"{"op":"set_chat_prefs","space_id":"sp_missing","pinned":true,"muted":true,"archived":true}"#.into(),
                )
                .await
                .unwrap_err();
        assert!(matches!(failure, CoreError::NotFound { what } if what == "Space"));
        assert!(owned_personal_spaces(&core).is_empty());
    }
    let prefs = |archived| {
        serde_json::json!({
            "op": "set_chat_prefs",
            "space_id": target,
            "pinned": true,
            "muted": true,
            "archived": archived,
        })
        .to_string()
    };
    // This exact public JSON path used to fail on legacy linked accounts with
    // NotFound{what:"Personal Space"}, despite the authorized readable chat.
    assert_eq!(core.client_request(prefs(false)).await.unwrap(), "null");
    assert_eq!(core.client_request(prefs(true)).await.unwrap(), "null");
    let personal = owned_personal_spaces(&core);
    assert_eq!(personal.len(), 1);
    let personal = personal[0].clone();
    {
        let e = core.lock();
        let events = e.logs[&personal].events();
        assert_eq!(events.len(), 3, "one genesis and two preference changes");
        assert!(events.iter().all(|event| {
            event.author == account.identity_id
                && event.device.as_deref() == Some(account.device_id.as_str())
                && event.cert.is_some()
        }));
        assert!(matches!(
            events[0].body,
            EventBody::SpaceCreated {
                kind: SpaceKind::Personal,
                ..
            }
        ));
        let current = e.space_summary(&target).unwrap();
        assert!(current.archived && current.pinned && current.muted);
    }
    assert_eq!(core.client_request(prefs(true)).await.unwrap(), "null");
    assert_eq!(core.lock().logs[&personal].events().len(), 3);
    assert_eq!(owned_personal_spaces(&core), vec![personal.clone()]);
    drop(core);
    let reopened = Client::open(file.0.to_string_lossy().into(), "en-US".into()).unwrap();
    assert!(reopened.unlock(vault.clone()).unwrap());
    assert_eq!(reopened.account().unwrap().identity_id, account.identity_id);
    assert_eq!(vault.load(VAULT_IDENTITY.into()), identity_key);
    assert_eq!(vault.load(VAULT_DEVICE.into()), device_key);
    assert!(!reopened.history_status().unwrap().initial_pending);
    assert_eq!(owned_personal_spaces(&reopened), vec![personal.clone()]);
    let after = reopened.lock().space_summary(&target).unwrap();
    assert!(after.archived && after.pinned && after.muted);
    assert_eq!(reopened.timeline(target.clone()).unwrap().len(), 1);
    assert_eq!(reopened.client_request(prefs(true)).await.unwrap(), "null");
    assert_eq!(reopened.lock().logs[&personal].events().len(), 3);

    Ok(())
}

#[tokio::test]
async fn linked_chat_preferences_bootstrap_is_signed_idempotent_and_survives_history_reopen() {
    preferences::linked_chat_preferences_bootstrap_is_signed_idempotent_and_survives_history_reopen(
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn linked_chat_preferences_repair_legacy_missing_personal_after_verified_history() {
    preferences::linked_chat_preferences_repair_legacy_missing_personal_after_verified_history()
        .await
        .unwrap()
}

#[test]
fn linked_chat_preferences_bootstrap_storage_failure_rolls_back_before_vault_and_can_retry() {
    preferences::linked_chat_preferences_bootstrap_storage_failure_rolls_back_before_vault_and_can_retry()
}

#[tokio::test]
async fn linked_chat_preferences_never_reuse_another_identity_personal_space() {
    preferences::linked_chat_preferences_never_reuse_another_identity_personal_space()
        .await
        .unwrap()
}

#[test]
fn linked_chat_preferences_vault_rejection_cleans_the_new_bootstrap() {
    preferences::linked_chat_preferences_vault_rejection_cleans_the_new_bootstrap()
}

#[tokio::test]
async fn link_then_cold_reopen_before_history_retains_pending_and_same_keys() {
    recovery::link_then_cold_reopen_before_history_retains_pending_and_same_keys()
        .await
        .unwrap()
}

#[tokio::test]
async fn missing_or_forged_history_cannot_clear_pending_or_partially_import() {
    recovery::missing_or_forged_history_cannot_clear_pending_or_partially_import()
        .await
        .unwrap()
}

#[tokio::test]
async fn durable_import_and_receipt_survive_reopen_before_relay_cleanup_or_ui_ack() {
    recovery::durable_import_and_receipt_survive_reopen_before_relay_cleanup_or_ui_ack()
        .await
        .unwrap()
}

#[tokio::test]
async fn cancellation_deadline_and_single_receiver_guard_leave_pending_retryable() {
    controls::cancellation_deadline_and_single_receiver_guard_leave_pending_retryable().await
}

#[test]
fn history_ready_is_a_content_free_authenticated_primary_hint_only_while_pending(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    controls::history_ready_is_a_content_free_authenticated_primary_hint_only_while_pending()
}

mod controls;
mod preferences;
mod recovery;
