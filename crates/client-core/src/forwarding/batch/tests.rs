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
fn fixture(path: &str) -> (Arc<Client>, Arc<Vault>, ForwardBatchScope) {
    let client = Client::open(path.into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    client
        .create_account(
            "Batch author".into(),
            "batch_author".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )
        .unwrap();
    let source_space = client
        .create_group_with("Source".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let target_space = client
        .create_group_with("Target".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let a = client
        .send_message(source_space.clone(), "First private text".into())
        .unwrap();
    let b = client
        .send_message(source_space.clone(), "Second private text".into())
        .unwrap();
    let account = client.account().unwrap();
    (
        client,
        vault,
        ForwardBatchScope {
            source: ForwardBatchSource {
                expected_identity_id: account.identity_id,
                expected_device_id: account.device_id,
                source_space,
                message_ids: vec![a.id, b.id],
            },
            target_space,
        },
    )
}
fn count(client: &Client) -> u64 {
    client.lock().store.outbox().unwrap().len() as u64
}
fn confirm(
    client: &Client,
    scope: &ForwardBatchScope,
    review: &ForwardBatchReview,
) -> Result<ForwardBatchReview, CoreError> {
    client.confirm_forward_batch(
        scope.source.clone(),
        review.batch_id.clone(),
        review.review_revision.clone(),
    )
}
#[test]
fn one_confirmation_accepts_exact_set_and_replays_are_idempotent() {
    let (client, _, scope) = fixture(":memory:");
    let before = count(&client);
    assert!(client
        .forward_batch_status(scope.source.clone())
        .unwrap()
        .is_none());
    let review = client.forward_batch_review(scope.clone()).unwrap();
    assert_eq!(count(&client), before);
    assert_eq!(client.forward_batch_review(scope.clone()).unwrap(), review);
    let accepted = confirm(&client, &scope, &review).unwrap();
    assert!(accepted
        .messages
        .iter()
        .all(|r| r.accepted_message_id.is_some()));
    assert_eq!(count(&client), before + 2);
    assert_eq!(confirm(&client, &scope, &review).unwrap(), accepted);
    assert_eq!(count(&client), before + 2);
    assert!(client
        .forward_status(scope.source.item(&scope.source.message_ids[0]))
        .unwrap()
        .is_none());
}
#[test]
fn stale_source_preflights_whole_set_then_only_explicit_review_renews_it() {
    let (client, _, scope) = fixture(":memory:");
    let review = client.forward_batch_review(scope.clone()).unwrap();
    client
        .edit_message(
            scope.source.source_space.clone(),
            scope.source.message_ids[1].clone(),
            "Edited before Send".into(),
        )
        .unwrap();
    let before = count(&client);
    let stale = confirm(&client, &scope, &review).unwrap();
    assert!(!stale.ready);
    assert!(stale.error.is_some());
    assert!(stale
        .messages
        .iter()
        .all(|r| r.accepted_message_id.is_none()));
    assert_eq!(count(&client), before);
    assert!(confirm(&client, &scope, &review).is_err());
    let renewed = client.forward_batch_review(scope.clone()).unwrap();
    assert_ne!(renewed.review_revision, review.review_revision);
    assert!(confirm(&client, &scope, &review).is_err());
    assert!(confirm(&client, &scope, &renewed)
        .unwrap()
        .messages
        .iter()
        .all(|r| r.accepted_message_id.is_some()));
}
#[test]
fn partial_transaction_failure_preserves_prior_receipt_and_resume_never_duplicates() {
    let (client, _, scope) = fixture(":memory:");
    let review = client.forward_batch_review(scope.clone()).unwrap();
    let before = count(&client);
    // The first publication commits; the second signed publication is refused.
    client.lock().store.conn().execute_batch(&format!("CREATE TRIGGER fail_batch_second BEFORE UPDATE ON meta WHEN NEW.key LIKE 'message.forwardbatch.%' AND (SELECT COUNT(*) FROM outbox) > {} BEGIN SELECT RAISE(ABORT,'receipt failure'); END;",before+1)).unwrap();
    let failed = confirm(&client, &scope, &review).unwrap();
    assert!(!failed.ready);
    assert!(failed.error.is_some());
    client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_batch_second")
        .unwrap();
    let retained = client
        .forward_batch_status(scope.source.clone())
        .unwrap()
        .unwrap();
    assert_eq!(
        retained
            .messages
            .iter()
            .filter(|r| r.accepted_message_id.is_some())
            .count(),
        1
    );
    assert_eq!(count(&client), before + 1);
    let accepted_id = retained.messages[0].accepted_message_id.clone();
    let renewed = client.forward_batch_review(scope.clone()).unwrap();
    let result = confirm(&client, &scope, &renewed).unwrap();
    assert_eq!(result.messages[0].accepted_message_id, accepted_id);
    assert_eq!(count(&client), before + 2);
}
#[test]
fn completed_intent_is_durable_and_new_explicit_review_can_forward_again() {
    let (client, _, scope) = fixture(":memory:");
    let first = client.forward_batch_review(scope.clone()).unwrap();
    let accepted = confirm(&client, &scope, &first).unwrap();
    let done = client
        .finish_forward_batch(scope.source.clone(), first.batch_id.clone())
        .unwrap();
    assert!(done.completed);
    assert_eq!(
        client.forward_batch_status(scope.source.clone()).unwrap(),
        Some(done.clone())
    );
    assert_eq!(
        client
            .finish_forward_batch(scope.source.clone(), first.batch_id.clone())
            .unwrap(),
        done
    );
    let second = client.forward_batch_review(scope.clone()).unwrap();
    assert_ne!(second.batch_id, first.batch_id);
    assert!(second
        .messages
        .iter()
        .all(|r| r.accepted_message_id.is_none()));
    assert!(confirm(&client, &scope, &accepted).is_err());
    let before = count(&client);
    assert!(confirm(&client, &scope, &second)
        .unwrap()
        .messages
        .iter()
        .all(|r| r.accepted_message_id.is_some()));
    assert_eq!(count(&client), before + 2);
}
#[test]
fn retarget_owner_and_duplicate_inputs_fail_without_mutating_retained_intent() {
    let (client, _, scope) = fixture(":memory:");
    let review = client.forward_batch_review(scope.clone()).unwrap();
    let mut changed = scope.clone();
    changed.target_space = changed.source.source_space.clone();
    let retargeted = client.forward_batch_review(changed).unwrap();
    assert_ne!(retargeted.review_revision, review.review_revision);
    assert!(confirm(&client, &scope, &review).is_err());
    let mut wrong = scope.source.clone();
    wrong.expected_device_id = "another-device".into();
    assert!(client.forward_batch_status(wrong).is_err());
    let mut duplicate = scope.clone();
    duplicate
        .source
        .message_ids
        .push(duplicate.source.message_ids[0].clone());
    assert!(client.forward_batch_review(duplicate).is_err());
    assert_eq!(
        client.forward_batch_status(scope.source.clone()).unwrap(),
        Some(retargeted)
    );
}
#[test]
fn cold_partial_resume_and_explicit_retarget_keep_each_accepted_output_exact() {
    let dir = std::env::temp_dir().join(types::new_id("batch-test").unwrap());
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("account.sqlite");
    let (client, vault, mut scope) = fixture(path.to_str().unwrap());
    let review = client.forward_batch_review(scope.clone()).unwrap();
    let before = count(&client);
    client.lock().store.conn().execute_batch(&format!("CREATE TRIGGER fail_batch_second BEFORE UPDATE ON meta WHEN NEW.key LIKE 'message.forwardbatch.%' AND (SELECT COUNT(*) FROM outbox) > {} BEGIN SELECT RAISE(ABORT,'receipt failure'); END;",before+1)).unwrap();
    let partial = confirm(&client, &scope, &review).unwrap();
    assert!(!partial.ready);
    assert!(partial.messages[0].accepted_message_id.is_some());
    assert!(partial.messages[1].accepted_message_id.is_none());
    drop(client);
    let client = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(client.forward_batch_status(scope.source.clone()).is_err());
    assert!(client.unlock(vault).unwrap());
    let count_before_recover = count(&client);
    assert_eq!(
        client.forward_batch_status(scope.source.clone()).unwrap(),
        Some(partial.clone())
    );
    assert_eq!(count(&client), count_before_recover);
    client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_batch_second")
        .unwrap();
    let old_target = scope.target_space.clone();
    scope.target_space = scope.source.source_space.clone();
    let renewed = client.forward_batch_review(scope.clone()).unwrap();
    assert_eq!(
        renewed.messages[0].accepted_message_id,
        partial.messages[0].accepted_message_id
    );
    assert_eq!(renewed.messages[0].target_space, old_target);
    assert_eq!(renewed.messages[1].target_space, scope.target_space);
    assert!(confirm(&client, &scope, &review).is_err());
    let accepted = confirm(&client, &scope, &renewed).unwrap();
    assert_eq!(
        accepted.messages[0].accepted_message_id,
        partial.messages[0].accepted_message_id
    );
    assert_eq!(count(&client), count_before_recover + 1);
    let done = client
        .finish_forward_batch(scope.source.clone(), accepted.batch_id.clone())
        .unwrap();
    assert!(done.completed);
    let fresh = client.forward_batch_review(scope.clone()).unwrap();
    assert_ne!(fresh.batch_id, done.batch_id);
    assert!(fresh
        .messages
        .iter()
        .all(|r| r.accepted_message_id.is_none()));
    drop(client);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn concurrent_confirmations_share_exact_durable_acceptances() {
    let (client, _, scope) = fixture(":memory:");
    let review = client.forward_batch_review(scope.clone()).unwrap();
    let before = count(&client);
    let workers = (0..2)
        .map(|_| {
            let client = client.clone();
            let scope = scope.clone();
            let review = review.clone();
            std::thread::spawn(move || confirm(&client, &scope, &review).unwrap())
        })
        .collect::<Vec<_>>();
    let results = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results[0], results[1]);
    assert_eq!(count(&client), before + 2);
}
#[test]
fn ledger_is_sealed_and_foreign_ciphertext_fails_closed() {
    let (client, _, scope) = fixture(":memory:");
    client.forward_batch_review(scope.clone()).unwrap();
    let engine = client.lock();
    let (key, value): (String, String) = engine
        .store
        .conn()
        .query_row(
            "SELECT key,value FROM meta WHERE key LIKE 'message.forwardbatch.%'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(!value.contains("private text"));
    engine
        .store
        .conn()
        .execute("UPDATE meta SET value='corrupt' WHERE key=?1", [key])
        .unwrap();
    drop(engine);
    assert!(client.forward_batch_status(scope.source).is_err());
}
#[test]
fn bounded_compaction_evicts_completed_or_zero_output_only_and_fences_old_tokens() {
    for completed in [false, true] {
        let (client, _, scope) = fixture(":memory:");
        let review = client.forward_batch_review(scope.clone()).unwrap();
        let engine = client.lock();
        let template = engine.load_forward_batch(&scope.source).unwrap().unwrap();
        let mut partial = template.clone();
        partial.records[0].accepted_message = Some("signed-partial-output".into());
        partial.ready = false;
        engine
            .persist_forward_batch(&scope.source, &partial)
            .unwrap();
        for index in 0..storage::MAX_SCOPES - 1 {
            let mut source = scope.source.clone();
            source.message_ids = vec![format!("retained-{index}")];
            let mut batch = template.clone();
            batch.records.truncate(1);
            batch.records[0].message.id = source.message_ids[0].clone();
            batch.completed = completed;
            if completed {
                batch.records[0].accepted_message = Some(format!("accepted-{index}"));
            }
            engine.persist_forward_batch(&source, &batch).unwrap();
        }
        drop(engine);
        let mut fresh = scope.clone();
        fresh.source.message_ids.truncate(1);
        let fresh_review = client.forward_batch_review(fresh.clone()).unwrap();
        assert_ne!(fresh_review.batch_id, review.batch_id);
        let retained = client
            .forward_batch_status(scope.source.clone())
            .unwrap()
            .unwrap();
        assert_eq!(
            retained.messages[0].accepted_message_id,
            Some("signed-partial-output".into())
        );
        let engine = client.lock();
        let rows: usize = engine
            .store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key LIKE 'message.forwardbatch.%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rows, storage::MAX_SCOPES);
        let mut evicted = None;
        for index in 0..storage::MAX_SCOPES - 1 {
            let mut source = scope.source.clone();
            source.message_ids = vec![format!("retained-{index}")];
            if engine.load_forward_batch(&source).unwrap().is_none() {
                evicted = Some(source);
                break;
            }
        }
        drop(engine);
        let evicted = evicted.expect("one safely evictable ledger was removed");
        assert!(client
            .confirm_forward_batch(
                evicted,
                review.batch_id.clone(),
                review.review_revision.clone()
            )
            .is_err());
    }
}
#[test]
fn unavailable_pending_can_be_explicitly_abandoned_without_erasing_accepted_output() {
    let dir = std::env::temp_dir().join(types::new_id("abandon-test").unwrap());
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("account.sqlite");
    let (client, vault, scope) = fixture(path.to_str().unwrap());
    let review = client.forward_batch_review(scope.clone()).unwrap();
    let before = count(&client);
    client.lock().store.conn().execute_batch(&format!("CREATE TRIGGER fail_batch_second BEFORE UPDATE ON meta WHEN NEW.key LIKE 'message.forwardbatch.%' AND (SELECT COUNT(*) FROM outbox) > {} BEGIN SELECT RAISE(ABORT,'receipt failure'); END;",before+1)).unwrap();
    let partial = confirm(&client, &scope, &review).unwrap();
    client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_batch_second")
        .unwrap();
    client
        .delete_message(
            scope.source.source_space.clone(),
            scope.source.message_ids[1].clone(),
        )
        .unwrap();
    assert!(client.forward_batch_review(scope.clone()).is_err());
    assert!(client
        .finish_forward_batch(scope.source.clone(), partial.batch_id.clone())
        .is_err());
    drop(client);
    let client = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(client.unlock(vault.clone()).unwrap());
    assert_eq!(
        client.forward_batch_status(scope.source.clone()).unwrap(),
        Some(partial.clone())
    );
    let before_stop = count(&client);
    let stopped = client
        .abandon_forward_batch(
            scope.source.clone(),
            partial.batch_id.clone(),
            partial.review_revision.clone(),
        )
        .unwrap();
    assert!(stopped.completed);
    assert_eq!(
        stopped.skipped_message_ids,
        vec![scope.source.message_ids[1].clone()]
    );
    assert_eq!(
        stopped.messages[0].accepted_message_id,
        partial.messages[0].accepted_message_id
    );
    assert!(stopped.messages[1].accepted_message_id.is_none());
    assert_eq!(count(&client), before_stop);
    drop(client);
    let client = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(client.unlock(vault).unwrap());
    assert_eq!(
        client.forward_batch_status(scope.source.clone()).unwrap(),
        Some(stopped.clone())
    );
    assert_eq!(
        client
            .abandon_forward_batch(
                scope.source.clone(),
                stopped.batch_id.clone(),
                stopped.review_revision.clone()
            )
            .unwrap(),
        stopped
    );
    assert_eq!(confirm(&client, &scope, &review).unwrap(), stopped);
    assert_eq!(count(&client), before_stop);
    let mut fresh = scope.clone();
    fresh.source.message_ids.truncate(1);
    let new = client.forward_batch_review(fresh.clone()).unwrap();
    assert_ne!(new.batch_id, stopped.batch_id);
    assert!(new.skipped_message_ids.is_empty());
    assert!(confirm(&client, &fresh, &new).unwrap().messages[0]
        .accepted_message_id
        .is_some());
    assert_eq!(count(&client), before_stop + 1);
    drop(client);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn explicit_stop_of_available_pending_allows_a_new_same_set_intent() {
    let (client, _, scope) = fixture(":memory:");
    let old = client.forward_batch_review(scope.clone()).unwrap();
    let stopped = client
        .abandon_forward_batch(
            scope.source.clone(),
            old.batch_id.clone(),
            old.review_revision.clone(),
        )
        .unwrap();
    assert_eq!(stopped.skipped_message_ids, scope.source.message_ids);
    let new = client.forward_batch_review(scope.clone()).unwrap();
    assert_ne!(new.batch_id, old.batch_id);
    assert!(new.skipped_message_ids.is_empty());
    assert!(confirm(&client, &scope, &old).is_err());
    assert!(client
        .abandon_forward_batch(scope.source.clone(), old.batch_id, old.review_revision)
        .is_err());
    assert!(confirm(&client, &scope, &new)
        .unwrap()
        .messages
        .iter()
        .all(|r| r.accepted_message_id.is_some()));
}
