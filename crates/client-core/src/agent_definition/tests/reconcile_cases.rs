//! reconcile cases scenarios; existing shared fixture remains in the parent.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
pub(super) async fn typed_update_persists_before_dispatch_reconciles_lost_ack_and_never_resends() {
    let dir = Directory::new().unwrap();
    let mut r = relay().await;
    let (c, _) = owner(dir.path(), &r.url);
    online(&c).await;
    let p = preview(&c, &mut r).await;
    let task = update(&c, &p);
    call(&mut r.calls).await.status(&record(1, BASE));
    let define = call(&mut r.calls).await;
    let own_target = target(&c);
    let scope = Scope::capture(&c.lock(), &own_target).unwrap();
    let held = Receipt::load(&c.lock(), &scope).unwrap().unwrap();
    let AgentRequest::Define {
        version,
        definition,
        ..
    } = &define.op
    else {
        panic!("Define");
    };
    assert_eq!(*version, 2);
    assert_eq!(*definition, held.after.bytes.as_bytes());
    define
        .response
        .send(Err("synthetic lost acknowledgement after acceptance".into()))
        .unwrap();
    call(&mut r.calls).await.status(&held.after);
    assert_eq!(
        response(task.await.unwrap().unwrap()).outcome,
        AgentDefinitionUpdateOutcome::Confirmed
    );
    let duplicate = update(&c, &p);
    call(&mut r.calls).await.status(&held.after); // No second Define is allowed.
    assert_eq!(
        response(duplicate.await.unwrap().unwrap()).receipt_id,
        held.id
    );
    assert!(!c
        .agent_definition_dismiss(target(&c), "old-receipt", &p.preview_digest)
        .unwrap());
    assert!(!c
        .agent_definition_dismiss(target(&c), &held.id, &"0".repeat(64))
        .unwrap());
    assert!(c
        .agent_definition_dismiss(target(&c), &held.id, &p.preview_digest)
        .unwrap());
    assert!(c
        .agent_definition_reconcile(target(&c))
        .await
        .unwrap()
        .is_none());
    c.stop_sync();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
pub(super) async fn old_base_after_unknown_stays_held_across_cold_reopen_until_late_exact_commit() {
    let dir = Directory::new().unwrap();
    let mut r = relay().await;
    let (c, key) = owner(dir.path(), &r.url);
    online(&c).await;
    let p = preview(&c, &mut r).await;
    let task = update(&c, &p);
    call(&mut r.calls).await.status(&record(1, BASE));
    let define = call(&mut r.calls).await;
    define
        .response
        .send(Err("synthetic reply lost, request may arrive later".into()))
        .unwrap();
    let own_target = target(&c);
    let own_scope = Scope::capture(&c.lock(), &own_target).unwrap();
    let held = Receipt::load(&c.lock(), &own_scope).unwrap().unwrap();
    call(&mut r.calls).await.status(&held.before);
    assert_eq!(
        response(task.await.unwrap().unwrap()).outcome,
        AgentDefinitionUpdateOutcome::Unknown
    );
    c.stop_sync();
    drop(c);
    let cold = Client::open(dir.path(), "en".into()).unwrap();
    assert!(cold.lock().unlock(Some(key.to_vec())).unwrap());
    let offline = cold
        .agent_definition_reconcile(target(&cold))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offline.outcome, AgentDefinitionUpdateOutcome::Unknown);
    assert_eq!(offline.receipt_id, held.id);
    online(&cold).await;
    let c2 = cold.clone();
    let input = command(&cold, "agent_definition_reconcile");
    let task = tokio::spawn(async move { c2.client_request(input.to_string()).await });
    call(&mut r.calls).await.status(&held.after);
    assert_eq!(
        response(task.await.unwrap().unwrap()).outcome,
        AgentDefinitionUpdateOutcome::Confirmed
    );
    cold.stop_sync();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
pub(super) async fn concurrent_forward_versions_conflict_without_a_version_bump() {
    let dir = Directory::new().unwrap();
    let mut r = relay().await;
    let (c, _) = owner(dir.path(), &r.url);
    online(&c).await;
    for concurrent in [2, 3] {
        let p = preview(&c, &mut r).await;
        let task = update(&c, &p);
        call(&mut r.calls).await.status(&record(1, BASE));
        let define = call(&mut r.calls).await;
        assert!(matches!(define.op, AgentRequest::Define { version: 2, .. }));
        define
            .response
            .send(Err("version must increase".into()))
            .unwrap();
        let bytes = BASE
            .replace("\"version\":1", &format!("\"version\":{concurrent}"))
            .replace("Enrollment validation only.", "Concurrent owner change.");
        call(&mut r.calls).await.status(&record(concurrent, &bytes));
        let result = response(task.await.unwrap().unwrap());
        assert_eq!(result.outcome, AgentDefinitionUpdateOutcome::Conflict);
        assert_eq!(result.current.unwrap().version, concurrent);
        assert!(c
            .agent_definition_dismiss(target(&c), &result.receipt_id, &p.preview_digest)
            .unwrap());
    }
    c.stop_sync();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
pub(super) async fn old_relay_missing_bytes_and_device_change_fail_closed() {
    let dir = Directory::new().unwrap();
    let mut r = relay().await;
    let (c, _) = owner(dir.path(), &r.url);
    online(&c).await;
    let input = command(&c, "agent_definition_read");
    let c2 = c.clone();
    let read = tokio::spawn(async move { c2.client_request(input.to_string()).await });
    call(&mut r.calls).await.status(&record(1, ""));
    assert!(read.await.unwrap().is_err());
    let input = command(&c, "agent_definition_read");
    let c2 = c.clone();
    let read = tokio::spawn(async move { c2.client_request(input.to_string()).await });
    let request = call(&mut r.calls).await;
    c.lock().net.author = None;
    request.status(&record(1, BASE));
    assert!(read.await.unwrap().is_err());
    c.stop_sync();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
pub(super) async fn failed_sqlite_receipt_write_refuses_dispatch() {
    let dir = Directory::new().unwrap();
    let mut r = relay().await;
    let (c, _) = owner(dir.path(), &r.url);
    online(&c).await;
    let p = preview(&c, &mut r).await;
    for trigger in [
        "CREATE TEMP TRIGGER refuse_definition_receipt BEFORE INSERT ON meta WHEN NEW.key LIKE 'agent.definition.owner-update.v1:%' BEGIN SELECT RAISE(ABORT, 'synthetic receipt write failure'); END;",
        "CREATE TEMP TRIGGER refuse_definition_receipt AFTER INSERT ON meta WHEN NEW.key LIKE 'agent.definition.owner-update.v1:%' BEGIN DELETE FROM meta WHERE key=NEW.key; END;",
    ] {
        c.lock().store.conn().execute_batch(trigger).unwrap();
        let task = update(&c, &p);
        call(&mut r.calls).await.status(&record(1, BASE));
        assert!(task.await.unwrap().is_err());
        assert!(r.calls.try_recv().is_err()); // No Define without retained/read-back review.
        let t = target(&c);
        let scope = Scope::capture(&c.lock(), &t).unwrap();
        assert!(Receipt::load(&c.lock(), &scope).unwrap().is_none());
        c.lock().store.conn().execute_batch("DROP TRIGGER refuse_definition_receipt;").unwrap();
    }
    c.stop_sync();
}

#[test]
pub(super) fn dismiss_cas_preserves_newer_receipt_and_foreign_device_cannot_restore_it() {
    let dir = Directory::new().unwrap();
    let (c, _) = owner(dir.path(), "https://relay.example");
    let t = target(&c);
    let scope = Scope::capture(&c.lock(), &t).unwrap();
    let old = Receipt::prepare(scope.clone(), record(1, BASE), edit()).unwrap();
    let newer = Receipt::prepare(scope.clone(), record(1, BASE), edit()).unwrap();
    assert_ne!(old.id, newer.id);
    c.lock()
        .store
        .set_meta(
            &scope.key().unwrap(),
            &serde_json::to_string(&newer).unwrap(),
        )
        .unwrap();
    assert!(!c
        .agent_definition_dismiss(target(&c), &old.id, &old.preview_digest)
        .unwrap());
    assert_eq!(
        Receipt::load(&c.lock(), &scope).unwrap().unwrap().id,
        newer.id
    );
    let mut foreign = target(&c);
    foreign.device = event_log::Signer::from_secret(&[8; 32]).id();
    assert!(c
        .agent_definition_dismiss(foreign, &newer.id, &newer.preview_digest)
        .is_err());
    assert_eq!(
        Receipt::load(&c.lock(), &scope).unwrap().unwrap().id,
        newer.id
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
pub(super) async fn paused_definition_edit_preserves_status_and_malformed_receipt_never_dispatches()
{
    let dir = Directory::new().unwrap();
    let mut r = relay().await;
    let (c, _) = owner(dir.path(), &r.url);
    online(&c).await;
    let mut paused = record(1, BASE);
    paused.status = "paused".into();
    let p = preview_record(&c, &mut r, &paused).await;
    assert_eq!(p.current.status, AgentStatus::Paused);
    assert_eq!(p.proposed.status, AgentStatus::Paused);
    let task = update(&c, &p);
    call(&mut r.calls).await.status(&paused);
    let define = call(&mut r.calls).await;
    let t = target(&c);
    let scope = Scope::capture(&c.lock(), &t).unwrap();
    let receipt = Receipt::load(&c.lock(), &scope).unwrap().unwrap();
    assert!(matches!(define.op, AgentRequest::Define { version: 2, .. }));
    define.response.send(Ok(Reply::Done)).unwrap();
    call(&mut r.calls).await.status(&receipt.after);
    let result = response(task.await.unwrap().unwrap());
    assert_eq!(result.outcome, AgentDefinitionUpdateOutcome::Confirmed);
    assert_eq!(result.current.unwrap().status, AgentStatus::Paused);
    c.lock()
        .store
        .set_meta(&scope.key().unwrap(), "malformed retained recovery context")
        .unwrap();
    assert!(update(&c, &p).await.unwrap().is_err());
    assert!(r.calls.try_recv().is_err()); // Neither Status nor Define after load error.
    assert_eq!(
        c.lock().store.meta(&scope.key().unwrap()).unwrap().unwrap(),
        "malformed retained recovery context"
    );
    c.stop_sync();
}
