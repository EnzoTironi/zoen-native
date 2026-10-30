use super::*;
use crate::{Client, PrivacyDto, SecretVault, TimelineEntry};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

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
    engine: Arc<Client>,
    vault: Arc<Vault>,
    scope: DraftScope,
}
fn fixture(path: &str) -> Fixture {
    let engine = Client::open(path.into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    engine
        .create_account(
            "Draft author".into(),
            "draft_author".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )
        .unwrap();
    let space = engine
        .create_group_with("Drafts QA".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let account = engine.account().unwrap();
    let scope = DraftScope {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: space,
        thread_root_id: None,
    };
    Fixture {
        engine,
        vault,
        scope,
    }
}
fn read(f: &Fixture) -> MessageDraftDto {
    f.engine.lock().message_draft(&f.scope).unwrap()
}
fn save(f: &Fixture, revision: &str, text: &str, reply: Option<String>) -> MessageDraftDto {
    f.engine
        .lock()
        .save_message_draft(&f.scope, revision, text.into(), reply)
        .unwrap()
}
fn counts(e: &Client) -> (usize, usize) {
    let core = e.lock();
    (
        core.store.event_count().unwrap() as usize,
        core.store.outbox().unwrap().len(),
    )
}
fn row(f: &Fixture) -> (String, String) {
    let core = f.engine.lock();
    let storage = core.draft_storage_scope(&f.scope).unwrap();
    (
        storage.id.clone(),
        core.store.meta(&storage.id).unwrap().unwrap(),
    )
}
fn temp() -> std::result::Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let p = std::env::temp_dir().join(types::new_id("draft-test")?);
    std::fs::create_dir_all(&p).unwrap();
    Ok(p)
}

#[test]
fn cas_and_even_an_empty_clear_fence_old_writers_without_losing_receipts() {
    let f = fixture(":memory:");
    let initial = read(&f);
    assert_eq!(initial.revision, "0");
    let clear = save(&f, "0", "", None);
    assert_ne!(clear.revision, "0");
    assert!(matches!(
        f.engine
            .lock()
            .save_message_draft(&f.scope, "0", "Old".into(), None),
        Err(CoreError::Stale { .. })
    ));
    let draft = save(&f, &clear.revision, "Latest", None);
    assert_eq!(
        save(&f, &draft.revision, "Latest", None),
        draft,
        "unchanged nonempty save is a no-op"
    );
    let before = row(&f);
    assert!(f
        .engine
        .lock()
        .save_message_draft(&f.scope, &clear.revision, "Stale".into(), None)
        .is_err());
    assert_eq!(row(&f), before);
    let clear = save(&f, &draft.revision, "", None);
    assert!(f
        .engine
        .lock()
        .save_message_draft(&f.scope, &draft.revision, "Resurrect".into(), None)
        .is_err());
    assert_eq!(read(&f), clear);
}

#[tokio::test]
async fn thread_send_revalidates_reply_membership_device_and_account() {
    let f = fixture(":memory:");
    let root = f
        .engine
        .send_message(f.scope.space_id.clone(), "Root".into())
        .unwrap();
    let another = f
        .engine
        .send_message(f.scope.space_id.clone(), "Another".into())
        .unwrap();
    let mut scope = f.scope.clone();
    scope.thread_root_id = Some(root.id.clone());
    assert!(f
        .engine
        .lock()
        .save_message_draft(&scope, "0", "Wrong thread".into(), Some(another.id))
        .is_err());
    let draft = f
        .engine
        .lock()
        .save_message_draft(&scope, "0", "Inside thread".into(), None)
        .unwrap();
    let accepted = f
        .engine
        .lock()
        .send_message_draft(&scope, &draft.revision)
        .unwrap();
    assert_eq!(
        visible_acceptance(&accepted).in_thread.as_deref(),
        Some(root.id.as_str())
    );
    for device in [false, true] {
        let mut foreign = f.scope.clone();
        if device {
            foreign.expected_device_id = "other".into();
        } else {
            foreign.expected_identity_id = "other".into();
        }
        assert!(matches!(
            f.engine.lock().message_draft(&foreign),
            Err(CoreError::Forbidden { .. })
        ));
        assert!(f
            .engine
            .lock()
            .save_message_draft(&foreign, "0", "bad".into(), None)
            .is_err());
        assert!(f
            .engine
            .lock()
            .send_message_draft(&foreign, &draft.revision)
            .is_err());
    }
    let saved = save(&f, "0", "Deleted target", Some(root.id.clone()));
    f.engine
        .delete_message(f.scope.space_id.clone(), root.id)
        .unwrap();
    let before = counts(&f.engine);
    let command = serde_json::json!({"op":"send_message_draft","expected_identity_id":f.scope.expected_identity_id,"expected_device_id":f.scope.expected_device_id,"space_id":f.scope.space_id,"thread_root_id":null,"expected_revision":saved.revision});
    assert!(matches!(
        f.engine.client_request(command.to_string()).await,
        Err(CoreError::DraftRejected { .. })
    ));
    assert_eq!(counts(&f.engine), before);
    assert_eq!(read(&f), saved);
    let me = f.scope.expected_identity_id.clone();
    f.engine
        .lock()
        .append(
            &f.scope.space_id,
            &me,
            types::EventBody::MemberRemoved {
                identity: me.clone(),
            },
        )
        .unwrap();
    assert!(f.engine.lock().message_draft(&f.scope).is_err());
    assert!(f
        .engine
        .lock()
        .save_message_draft(&f.scope, &saved.revision, "bad".into(), None)
        .is_err());
}

#[test]
fn ciphertext_corruption_and_cross_scope_copy_fail_closed_without_overwrite() {
    let f = fixture(":memory:");
    let saved = save(&f, "0", "Retain private text", None);
    let (key, cipher) = row(&f);
    let other = f
        .engine
        .create_group_with("Other".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let mut scope = f.scope.clone();
    scope.space_id = other;
    let other_key = f.engine.lock().draft_storage_scope(&scope).unwrap().id;
    f.engine.lock().store.set_meta(&other_key, &cipher).unwrap();
    assert!(f.engine.lock().message_draft(&scope).is_err());
    for bad in [
        "locked ciphertext".into(),
        "v1:".to_owned() + &"A".repeat(MAX_CIPHER_BYTES),
    ] {
        f.engine.lock().store.set_meta(&key, &bad).unwrap();
        assert!(matches!(
            f.engine.lock().message_draft(&f.scope),
            Err(CoreError::Storage { .. })
        ));
        assert!(f
            .engine
            .lock()
            .save_message_draft(&f.scope, &saved.revision, "Overwrite".into(), None)
            .is_err());
        assert_eq!(f.engine.lock().store.meta(&key).unwrap(), Some(bad));
    }
}

#[tokio::test]
async fn actual_app_commands_keep_old_send_budget_and_restore_exact_draft() {
    let f = fixture(":memory:");
    let command = serde_json::json!({"op":"send_message","space_id":f.scope.space_id,"text":""});
    let overhead = serde_json::to_vec(&command).unwrap().len();
    let text = "x".repeat(crate::client_api::MAX_REQUEST_BYTES - overhead);
    let send = serde_json::json!({"op":"send_message","space_id":f.scope.space_id,"text":text});
    assert_eq!(
        serde_json::to_vec(&send).unwrap().len(),
        crate::client_api::MAX_REQUEST_BYTES
    );
    let save = serde_json::json!({"op":"save_message_draft","expected_identity_id":f.scope.expected_identity_id,"expected_device_id":f.scope.expected_device_id,"space_id":f.scope.space_id,"thread_root_id":null,"expected_revision":"0","text":text,"reply_to":null});
    assert!(serde_json::to_vec(&save).unwrap().len() > crate::client_api::MAX_REQUEST_BYTES);
    let saved: MessageDraftDto =
        serde_json::from_str(&f.engine.client_request(save.to_string()).await.unwrap()).unwrap();
    assert_eq!(saved.text, text);
    let mut oversized = send;
    oversized["text"] = format!("{text}x").into();
    assert!(f
        .engine
        .client_request(oversized.to_string())
        .await
        .is_err());
    let read = serde_json::json!({"op":"message_draft","expected_identity_id":f.scope.expected_identity_id,"expected_device_id":f.scope.expected_device_id,"space_id":f.scope.space_id,"thread_root_id":null});
    let loaded: MessageDraftDto =
        serde_json::from_str(&f.engine.client_request(read.to_string()).await.unwrap()).unwrap();
    assert_eq!(loaded, saved);
    let send = serde_json::json!({"op":"send_message_draft","expected_identity_id":f.scope.expected_identity_id,"expected_device_id":f.scope.expected_device_id,"space_id":f.scope.space_id,"thread_root_id":null,"expected_revision":saved.revision});
    let accepted: serde_json::Value =
        serde_json::from_str(&f.engine.client_request(send.to_string()).await.unwrap()).unwrap();
    assert_eq!(accepted["draft"]["text"], "");
    assert_eq!(accepted["entry"]["kind"]["Message"]["text"], text);
}

#[test]
fn bounds_never_evict_and_erase_removes_drafts() {
    let f = fixture(":memory:");
    let saved = save(&f, "0", "Keep", None);
    let (key, cipher) = row(&f);
    {
        let e = f.engine.lock();
        for n in 0..MAX_SCOPES - 1 {
            e.store
                .set_meta(&format!("{PREFIX}fixture-{n}"), &cipher)
                .unwrap();
        }
    }
    let other = f
        .engine
        .create_group_with("Full".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let mut scope = f.scope.clone();
    scope.space_id = other;
    assert!(f
        .engine
        .lock()
        .save_message_draft(&scope, "0", "New".into(), None)
        .is_err());
    assert_eq!(read(&f), saved);
    f.engine
        .lock()
        .store
        .conn()
        .execute(
            "DELETE FROM meta WHERE key LIKE 'message.draft.fixture-%'",
            [],
        )
        .unwrap();
    f.engine
        .lock()
        .store
        .set_meta(
            &format!("{PREFIX}fixture-large"),
            &"A".repeat(MAX_RETAINED_BYTES),
        )
        .unwrap();
    let before = f.engine.lock().store.meta(&key).unwrap();
    assert!(f
        .engine
        .lock()
        .save_message_draft(&f.scope, &saved.revision, "Changed".into(), None)
        .is_err());
    assert_eq!(f.engine.lock().store.meta(&key).unwrap(), before);
    f.engine.erase_device(f.vault.clone()).unwrap();
    assert_eq!(f.engine.lock().store.meta(&key).unwrap(), None);
}

#[test]
fn reader_can_keep_private_text_but_cannot_send_it() {
    let f = fixture(":memory:");
    let me = f.scope.expected_identity_id.clone();
    let draft = save(&f, "0", "Private text after role change", None);
    f.engine
        .lock()
        .append(
            &f.scope.space_id,
            &me,
            types::EventBody::MemberAdded {
                identity: me.clone(),
                role: types::Role::Reader,
            },
        )
        .unwrap();
    assert_eq!(read(&f), draft);
    let draft = save(&f, &draft.revision, "Still private", None);
    let before = counts(&f.engine);
    assert!(matches!(
        f.engine
            .lock()
            .send_message_draft(&f.scope, &draft.revision),
        Err(CoreError::DraftRejected { .. })
    ));
    assert_eq!(counts(&f.engine), before);
    assert_eq!(read(&f), draft);
}

#[test]
fn accepted_retry_is_checked_before_changed_reply_or_writer_preflight() {
    let f = fixture(":memory:");
    let root = f
        .engine
        .send_message(f.scope.space_id.clone(), "Root".into())
        .unwrap();
    let draft = save(&f, "0", "Reply accepted", Some(root.id.clone()));
    let accepted = f
        .engine
        .lock()
        .send_message_draft(&f.scope, &draft.revision)
        .unwrap();
    f.engine
        .delete_message(f.scope.space_id.clone(), root.id)
        .unwrap();
    let me = f.scope.expected_identity_id.clone();
    f.engine
        .lock()
        .append(
            &f.scope.space_id,
            &me,
            types::EventBody::MemberAdded {
                identity: me.clone(),
                role: types::Role::Reader,
            },
        )
        .unwrap();
    let before = counts(&f.engine);
    let retry = f
        .engine
        .lock()
        .send_message_draft(&f.scope, &draft.revision)
        .unwrap();
    assert_eq!(
        visible_acceptance(&retry).id,
        visible_acceptance(&accepted).id
    );
    assert_eq!(counts(&f.engine), before);
    let current = save(&f, &retry.draft.revision, "New, unaccepted text", None);
    assert!(matches!(
        f.engine
            .lock()
            .send_message_draft(&f.scope, &current.revision),
        Err(CoreError::DraftRejected { .. })
    ));
    assert!(matches!(
        f.engine.lock().send_message_draft(&f.scope, "not-current"),
        Err(CoreError::Stale { .. })
    ));
}

#[test]
fn storage_errors_and_evicted_accepted_receipts_never_claim_definitive_refusal() {
    let f = fixture(":memory:");
    let one = save(&f, "0", "One", None);
    let accepted = f
        .engine
        .lock()
        .send_message_draft(&f.scope, &one.revision)
        .unwrap();
    let two = save(&f, &accepted.draft.revision, "Two", None);
    f.engine
        .lock()
        .send_message_draft(&f.scope, &two.revision)
        .unwrap();
    let before = counts(&f.engine);
    assert!(matches!(
        f.engine.lock().send_message_draft(&f.scope, &one.revision),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(counts(&f.engine), before);
    let saved = read(&f);
    let three = save(&f, &saved.revision, "Three", None);
    f.engine.lock().store.conn().execute_batch("CREATE TRIGGER fail_draft BEFORE UPDATE ON meta WHEN NEW.key LIKE 'message.draft.%' BEGIN SELECT RAISE(ABORT,'draft failed'); END;").unwrap();
    assert!(matches!(
        f.engine
            .lock()
            .send_message_draft(&f.scope, &three.revision),
        Err(CoreError::Storage { .. })
    ));
    assert_eq!(read(&f), three);
    let demo = Client::open(":memory:".into(), "en".into()).unwrap();
    demo.seed_demo_if_empty().unwrap();
    assert!(matches!(
        demo.lock().message_draft(&f.scope),
        Err(CoreError::Invalid { .. })
    ));
}

mod recovery_cases;

mod selected_quotes;

mod shared_replies;
mod shared_selections;

mod editing;

mod receipt_visibility;

fn visible_acceptance(result: &MessageDraftSendResult) -> &TimelineEntry {
    result
        .entry
        .as_ref()
        .expect("this assertion requires a still-visible accepted source")
}
