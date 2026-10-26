use super::*;
use crate::{PrivacyDto, SecretVault};
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
    client: Arc<Client>,
    vault: Arc<Vault>,
    scope: ForwardScope,
}
fn fixture(path: &str) -> Fixture {
    let client = Client::open(path.into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    client
        .create_account(
            "Forward author".into(),
            "forward_author".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )
        .unwrap();
    let source = client
        .create_group_with("Source".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let target = client
        .create_group_with("Target".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let message = client
        .send_message(source.clone(), "Private forward 雨".into())
        .unwrap();
    let account = client.account().unwrap();
    Fixture {
        client,
        vault,
        scope: ForwardScope {
            expected_identity_id: account.identity_id,
            expected_device_id: account.device_id,
            source_space: source,
            message_id: message.id,
            target_space: target,
        },
    }
}
fn counts(f: &Fixture) -> (u64, usize) {
    let e = f.client.lock();
    (
        e.store.event_count().unwrap(),
        e.store.outbox().unwrap().len(),
    )
}
fn confirm(f: &Fixture, review: &ForwardReview) -> Result<ForwardReview, CoreError> {
    f.client.confirm_forward(
        f.scope.clone(),
        review.operation_id.clone(),
        review.source_revision.clone(),
    )
}

#[test]
fn review_and_recovery_never_send_and_cancel_fences_old_operations() {
    let f = fixture(":memory:");
    let before = counts(&f);
    assert!(f.client.forward_status(f.scope.source()).unwrap().is_none());
    let review = f.client.forward_review(f.scope.clone()).unwrap();
    assert!(review.accepted.is_none());
    assert_eq!(f.client.forward_review(f.scope.clone()).unwrap(), review);
    assert_eq!(
        f.client.forward_status(f.scope.source()).unwrap(),
        Some(review.clone())
    );
    assert_eq!(counts(&f), before);
    f.client
        .dismiss_forward(f.scope.clone(), review.operation_id.clone(), None)
        .unwrap();
    assert!(matches!(confirm(&f, &review), Err(CoreError::Stale { .. })));
    let new = f.client.forward_review(f.scope.clone()).unwrap();
    assert_ne!(new.operation_id, review.operation_id);
    assert!(confirm(&f, &review).is_err());
    assert_eq!(counts(&f), before);
    assert!(confirm(&f, &new).unwrap().accepted.is_some());
}
#[test]
fn changed_or_deleted_source_is_rejected_under_the_same_engine_lock() {
    for deleted in [false, true] {
        let f = fixture(":memory:");
        let review = f.client.forward_review(f.scope.clone()).unwrap();
        if deleted {
            f.client
                .delete_message(f.scope.source_space.clone(), f.scope.message_id.clone())
                .unwrap();
        } else {
            f.client
                .edit_message(
                    f.scope.source_space.clone(),
                    f.scope.message_id.clone(),
                    "Changed".into(),
                )
                .unwrap();
        }
        let before = counts(&f);
        assert!(matches!(confirm(&f, &review), Err(CoreError::Stale { .. })));
        assert_eq!(counts(&f), before);
        assert!(f
            .client
            .forward_status(f.scope.source())
            .unwrap()
            .unwrap()
            .accepted
            .is_none());
    }
}
#[test]
fn accepted_receipt_is_idempotent_even_after_source_deletion() {
    let f = fixture(":memory:");
    let review = f.client.forward_review(f.scope.clone()).unwrap();
    let accepted = confirm(&f, &review).unwrap().accepted.unwrap();
    let before = counts(&f);
    assert_eq!(
        confirm(&f, &review).unwrap().accepted.unwrap().id,
        accepted.id
    );
    assert_eq!(counts(&f), before);
    f.client
        .delete_message(f.scope.source_space.clone(), f.scope.message_id.clone())
        .unwrap();
    let before = counts(&f);
    assert_eq!(
        confirm(&f, &review).unwrap().accepted.unwrap().id,
        accepted.id
    );
    assert_eq!(counts(&f), before);
    assert_eq!(
        f.client
            .forward_review(f.scope.clone())
            .unwrap()
            .accepted
            .unwrap()
            .id,
        accepted.id
    );
}
#[test]
fn receipt_failure_rolls_back_signed_forward_and_outbox_then_retry_is_single() {
    let f = fixture(":memory:");
    let review = f.client.forward_review(f.scope.clone()).unwrap();
    let before = counts(&f);
    f.client.lock().store.conn().execute_batch("CREATE TRIGGER fail_forward BEFORE UPDATE ON meta WHEN NEW.key LIKE 'message.forward.%' BEGIN SELECT RAISE(ABORT,'forward receipt failed'); END;").unwrap();
    assert!(confirm(&f, &review).is_err());
    assert_eq!(counts(&f), before);
    assert!(f
        .client
        .forward_status(f.scope.source())
        .unwrap()
        .unwrap()
        .accepted
        .is_none());
    f.client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_forward")
        .unwrap();
    let accepted = confirm(&f, &review).unwrap().accepted.unwrap();
    assert_eq!(
        confirm(&f, &review).unwrap().accepted.unwrap().id,
        accepted.id
    );
}
#[test]
fn foreign_owner_or_altered_scope_cannot_confirm_or_read_a_receipt() {
    let f = fixture(":memory:");
    let review = f.client.forward_review(f.scope.clone()).unwrap();
    let before = counts(&f);
    for scope in [
        ForwardScope {
            expected_device_id: "foreign".into(),
            ..f.scope.clone()
        },
        ForwardScope {
            expected_identity_id: "foreign".into(),
            ..f.scope.clone()
        },
        ForwardScope {
            target_space: f.scope.source_space.clone(),
            ..f.scope.clone()
        },
    ] {
        assert!(f
            .client
            .confirm_forward(
                scope,
                review.operation_id.clone(),
                review.source_revision.clone()
            )
            .is_err());
    }
    assert_eq!(counts(&f), before);
}
#[test]
fn cold_reopen_recovers_exact_pending_or_accepted_operation_without_resending() {
    for send in [false, true] {
        let dir = std::env::temp_dir().join(types::new_id("forward-test").unwrap());
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("account.sqlite");
        let f = fixture(path.to_str().unwrap());
        let review = f.client.forward_review(f.scope.clone()).unwrap();
        let expected = if send {
            confirm(&f, &review).unwrap()
        } else {
            review.clone()
        };
        let before = counts(&f);
        let scope = f.scope.clone();
        let vault = f.vault.clone();
        drop(f);
        let e = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
        assert!(e.forward_status(scope.source()).is_err());
        assert!(e.unlock(vault).unwrap());
        assert_eq!(
            e.forward_status(scope.source()).unwrap(),
            Some(expected.clone())
        );
        let f = Fixture {
            client: e,
            vault: Arc::new(Vault::default()),
            scope,
        };
        assert_eq!(counts(&f), before);
        if send {
            assert_eq!(confirm(&f, &review).unwrap(), expected);
            assert_eq!(counts(&f), before);
        }
        let cipher: String = f
            .client
            .lock()
            .store
            .conn()
            .query_row(
                "SELECT value FROM meta WHERE key LIKE 'message.forward.%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(cipher.starts_with("v1:"));
        assert!(!cipher.contains("Private forward"));
        drop(f);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn pending_cancel_cannot_erase_a_concurrently_accepted_forward() {
    let f = fixture(":memory:");
    let review = f.client.forward_review(f.scope.clone()).unwrap();
    let accepted = confirm(&f, &review).unwrap().accepted.unwrap();
    assert!(f
        .client
        .dismiss_forward(f.scope.clone(), review.operation_id.clone(), None)
        .is_err());
    assert_eq!(
        f.client
            .forward_status(f.scope.source())
            .unwrap()
            .unwrap()
            .accepted
            .unwrap()
            .id,
        accepted.id
    );
    f.client
        .dismiss_forward(
            f.scope.clone(),
            review.operation_id.clone(),
            Some(accepted.id),
        )
        .unwrap();
    let next = f.client.forward_review(f.scope.clone()).unwrap();
    assert_ne!(next.operation_id, review.operation_id);
    let before = counts(&f);
    assert!(confirm(&f, &review).is_err());
    assert!(f
        .client
        .dismiss_forward(f.scope.clone(), review.operation_id.clone(), None)
        .is_err());
    assert_eq!(
        f.client
            .forward_status(f.scope.source())
            .unwrap()
            .unwrap()
            .operation_id,
        next.operation_id
    );
    assert_eq!(counts(&f), before);
}

#[test]
fn file_copy_and_receipt_are_atomic_and_a_changed_attachment_requires_new_review() {
    let mut f = fixture(":memory:");
    let source = f
        .client
        .lock()
        .send_file_message(
            &f.scope.source_space,
            "proof.pdf",
            "application/pdf",
            &[7u8; 2100],
            Some(&[5u8; 125]),
            None,
            false,
            "source-file",
        )
        .unwrap();
    f.scope.message_id = source.id;
    let crate::EntryKind::Message {
        card: Some(card), ..
    } = source.kind
    else {
        panic!("file card");
    };
    let review = f.client.forward_review(f.scope.clone()).unwrap();
    f.client
        .file_new_version_typed(
            card.item_id.clone(),
            vec![8u8; 2100],
            None,
            "Edit".into(),
            "new.pdf".into(),
            "application/pdf".into(),
        )
        .unwrap();
    let before = counts(&f);
    assert!(confirm(&f, &review).is_err());
    assert_eq!(counts(&f), before);
    f.client
        .dismiss_forward(f.scope.clone(), review.operation_id, None)
        .unwrap();
    let review = f.client.forward_review(f.scope.clone()).unwrap();
    f.client.lock().store.conn().execute_batch("CREATE TRIGGER fail_forward BEFORE UPDATE ON meta WHEN NEW.key LIKE 'message.forward.%' BEGIN SELECT RAISE(ABORT,'receipt failure'); END;").unwrap();
    let before = counts(&f);
    assert!(confirm(&f, &review).is_err());
    assert_eq!(counts(&f), before);
    f.client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_forward")
        .unwrap();
    let accepted = confirm(&f, &review).unwrap().accepted.unwrap();
    let crate::EntryKind::Message {
        card: Some(copy), ..
    } = &accepted.kind
    else {
        panic!("forwarded card");
    };
    assert_ne!(copy.item_id, card.item_id);
    assert_eq!(
        f.client.file_bytes(copy.item_id.clone(), None).unwrap(),
        Some(vec![8u8; 2100])
    );
    let before = counts(&f);
    assert_eq!(
        confirm(&f, &review).unwrap().accepted.unwrap().id,
        accepted.id
    );
    assert_eq!(counts(&f), before);
}

#[test]
fn forwarding_never_consumes_either_conversation_draft() {
    let f = fixture(":memory:");
    let mut drafts = Vec::new();
    for (space, text) in [
        (&f.scope.source_space, "Source unsent"),
        (&f.scope.target_space, "Target unsent"),
    ] {
        let scope = crate::message_drafts::DraftScope {
            expected_identity_id: f.scope.expected_identity_id.clone(),
            expected_device_id: f.scope.expected_device_id.clone(),
            space_id: space.clone(),
            thread_root_id: None,
        };
        let draft = f
            .client
            .lock()
            .save_message_draft(&scope, "0", text.into(), None)
            .unwrap();
        drafts.push((scope, draft));
    }
    let review = f.client.forward_review(f.scope.clone()).unwrap();
    confirm(&f, &review).unwrap();
    for (scope, draft) in drafts {
        assert_eq!(f.client.lock().message_draft(&scope).unwrap(), draft);
    }
}

#[test]
fn corrupt_receipt_fails_closed_instead_of_minting_another_send() {
    let f = fixture(":memory:");
    let review = f.client.forward_review(f.scope.clone()).unwrap();
    confirm(&f, &review).unwrap();
    let before = counts(&f);
    f.client
        .lock()
        .store
        .conn()
        .execute(
            "UPDATE meta SET value = 'unreadable' WHERE key LIKE 'message.forward.%'",
            [],
        )
        .unwrap();
    assert!(f.client.forward_status(f.scope.source()).is_err());
    assert!(f.client.forward_review(f.scope.clone()).is_err());
    assert!(confirm(&f, &review).is_err());
    assert_eq!(counts(&f), before);
}

#[test]
fn simultaneous_confirmations_share_one_signed_acceptance() {
    let f = fixture(":memory:");
    let review = f.client.forward_review(f.scope.clone()).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let (client, scope, review, barrier) = (
            f.client.clone(),
            f.scope.clone(),
            review.clone(),
            barrier.clone(),
        );
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            client
                .confirm_forward(scope, review.operation_id, review.source_revision)
                .unwrap()
                .accepted
                .unwrap()
                .id
        }));
    }
    let a = workers.remove(0).join().unwrap();
    let b = workers.remove(0).join().unwrap();
    assert_eq!(a, b);
    assert_eq!(
        f.client
            .timeline(f.scope.target_space.clone())
            .unwrap()
            .iter()
            .filter(|e| e.forwarded)
            .count(),
        1
    );
}

#[tokio::test]
async fn json_bridge_recovers_pending_and_accepted_without_another_forward() {
    let f = fixture(":memory:");
    let before = counts(&f);
    let request = serde_json::json!({"op":"forward_review","scope":f.scope}).to_string();
    let review: serde_json::Value =
        serde_json::from_str(&f.client.client_request(request).await.unwrap()).unwrap();
    assert!(review["accepted"].is_null());
    assert_eq!(counts(&f), before);
    let request = serde_json::json!({"op":"confirm_forward","scope":f.scope,"operation_id":review["operation_id"],"source_revision":review["source_revision"]}).to_string();
    let accepted: serde_json::Value =
        serde_json::from_str(&f.client.client_request(request.clone()).await.unwrap()).unwrap();
    let before = counts(&f);
    let again: serde_json::Value =
        serde_json::from_str(&f.client.client_request(request).await.unwrap()).unwrap();
    assert_eq!(accepted["accepted"]["id"], again["accepted"]["id"]);
    assert_eq!(counts(&f), before);
    let status = serde_json::json!({"op":"forward_status","source":f.scope.source()}).to_string();
    let status: serde_json::Value =
        serde_json::from_str(&f.client.client_request(status).await.unwrap()).unwrap();
    assert_eq!(status["accepted"]["id"], accepted["accepted"]["id"]);
    assert_eq!(counts(&f), before);
}

#[test]
fn recovery_discovers_original_destination_and_new_selection_cannot_overwrite_it() {
    let f = fixture(":memory:");
    let review = f.client.forward_review(f.scope.clone()).unwrap();
    let target2 = f
        .client
        .create_group_with("Another target".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let other = ForwardScope {
        target_space: target2.clone(),
        ..f.scope.clone()
    };
    let retained = f.client.forward_review(other.clone()).unwrap();
    assert_eq!(retained.operation_id, review.operation_id);
    assert_eq!(retained.target_space, f.scope.target_space);
    assert_eq!(
        f.client.forward_status(f.scope.source()).unwrap().unwrap(),
        retained
    );
    assert!(f
        .client
        .confirm_forward(
            other.clone(),
            retained.operation_id,
            retained.source_revision
        )
        .is_err());
    f.client
        .dismiss_forward(f.scope.clone(), review.operation_id, None)
        .unwrap();
    let next = f.client.forward_review(other).unwrap();
    assert_eq!(next.target_space, target2);
    assert!(next.accepted_message_id.is_none());
}

#[test]
fn revoked_destination_never_leaks_entry_but_keeps_safe_receipt_or_cancel_recovery() {
    for send in [false, true] {
        let f = fixture(":memory:");
        let review = f.client.forward_review(f.scope.clone()).unwrap();
        let accepted = if send {
            confirm(&f, &review).unwrap().accepted_message_id
        } else {
            None
        };
        f.client.leave_space(f.scope.target_space.clone()).unwrap();
        assert!(!f
            .client
            .lock()
            .is_member(&f.scope.target_space, &f.scope.expected_identity_id));
        let recovered = f.client.forward_status(f.scope.source()).unwrap().unwrap();
        assert_eq!(recovered.accepted_message_id, accepted);
        assert!(
            recovered.accepted.is_none(),
            "receipt does not grant destination read permission"
        );
        let before = counts(&f);
        assert!(confirm(&f, &review).is_err());
        assert_eq!(counts(&f), before);
        f.client
            .dismiss_forward(f.scope.clone(), review.operation_id, accepted)
            .unwrap();
        assert!(f.client.forward_status(f.scope.source()).unwrap().is_none());
    }
}
