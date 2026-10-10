use super::*;

pub(super) async fn closed_sql_pool_retains_fdb_evidence() {
    let mut w = World::new().await;
    let step = Arc::new(w.step(0).await);
    w.http.paused.store(true, Ordering::SeqCst);
    let (runtime, task_step) = (w.runtime.clone(), step.clone());
    let call = tokio::spawn(async move { runtime.complete_verified(&task_step).await });
    w.http.wait().await;
    w.runtime.finance.pool.close().await;
    w.http.release.notify_one();
    assert_eq!(call.await.unwrap().unwrap().1, FinancialState::Claimed);
    let reopened = w.reopen().await;
    assert_eq!(
        reopened
            .inspect_attempt(&step.request.context.attempt_id)
            .await
            .unwrap(),
        Some(FinancialState::Claimed)
    );
    assert!(!reopened
        .execution
        .pending_evidence(64)
        .await
        .unwrap()
        .is_empty());
    assert!(reopened.reconcile(64).await.unwrap().transferred > 0);
    assert_eq!(
        reopened
            .inspect_attempt(&step.request.context.attempt_id)
            .await
            .unwrap(),
        Some(FinancialState::Settled { units: 6 })
    );
    assert_eq!(reopened.reconcile(64).await.unwrap().inspected, 0);
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 1);
    w.runtime = reopened;
    w.finish().await;
}

pub(super) async fn settlement_ack_loss_and_outbox_ack_selection() {
    let w = World::new().await;
    let step = w.step(0).await;
    w.runtime.fault.store(5, Ordering::SeqCst);
    // Real SQL settlement commits; its return is suppressed, not a DB wire fault.
    assert_eq!(
        w.runtime.complete_verified(&step).await.unwrap().1,
        FinancialState::Claimed
    );
    assert_eq!(w.balance().await.spent_units, 6);
    let reopened = w.reopen().await;
    assert!(reopened.complete_verified(&step).await.is_err());
    for _ in 0..3 {
        reopened.reconcile(64).await.unwrap();
    }
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_dispositions").await,
        1
    );
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_journals WHERE kind='settle'")
            .await,
        1
    );
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_financial_outbox")
            .await,
        2
    );
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_financial_acks").await,
        2
    );
    reopened.finance.pool.close().await;
    w.runtime.fault.store(0, Ordering::SeqCst);
    w.runtime.complete_verified(&w.step(1).await).await.unwrap();
    let pending = w.runtime.finance.pending_outbox(64).await.unwrap();
    assert_eq!(pending.len(), 2);
    // Acknowledge the later sorted entry first. Missing-ACK selection must still
    // return the earlier entry; no advancing sequence watermark is involved.
    let (attempt, kind, digest, sealed) = pending.last().unwrap();
    w.runtime
        .execution
        .financial_reference(attempt, kind, digest, sealed.as_deref())
        .await
        .unwrap();
    w.runtime
        .finance
        .ack_outbox(attempt, kind, digest)
        .await
        .unwrap();
    assert_eq!(w.runtime.finance.pending_outbox(64).await.unwrap().len(), 1);
    w.runtime.reconcile(64).await.unwrap();
    assert!(w
        .runtime
        .finance
        .pending_outbox(64)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 2);
    w.finish().await;
}

pub(super) async fn declared_restore_closes_dispatch_and_release() {
    let w = World::new().await;
    let step = w.step(0).await;
    w.runtime.fault.store(2, Ordering::SeqCst);
    assert!(w.runtime.complete_verified(&step).await.is_err());
    let binding = w
        .runtime
        .execution
        .prepared_binding(&step.request.context.attempt_id, &w.runtime.custody)
        .await
        .unwrap();
    // Actually remove an admission in this throwaway namespace, simulating a
    // rewound inventory. Explicit external declaration closes the barrier.
    let trx = w.runtime.execution.db.create_trx().unwrap();
    trx.clear(&w.runtime.execution.root.pack(&(
        "attempt",
        step.request.context.attempt_id.as_str(),
        "admitted",
    )));
    trx.commit().await.unwrap();
    w.runtime.continuity.store(false, Ordering::SeqCst);
    assert_eq!(
        w.runtime.complete_verified(&step).await.unwrap_err(),
        RuntimeError::RestoreUnreconciled
    );
    assert!(matches!(
        w.runtime
            .execution
            .close_before_dispatch(&step, &binding, &w.runtime.custody, &w.runtime)
            .await,
        Err(RuntimeError::RestoreUnreconciled)
    ));
    assert_eq!(w.balance().await.held_units, 13);
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    w.finish().await;

    let w = World::new().await;
    let step = w.step(0).await;
    w.runtime.fault.store(1, Ordering::SeqCst);
    assert!(w.runtime.complete_verified(&step).await.is_err());
    let binding = w
        .runtime
        .execution
        .prepared_binding(&step.request.context.attempt_id, &w.runtime.custody)
        .await
        .unwrap();
    let proof = w
        .runtime
        .execution
        .close_before_dispatch(&step, &binding, &w.runtime.custody, &w.runtime)
        .await
        .unwrap()
        .unwrap();
    w.runtime.continuity.store(false, Ordering::SeqCst);
    assert_eq!(
        w.runtime
            .finance
            .release(proof, &w.runtime)
            .await
            .unwrap_err(),
        RuntimeError::RestoreUnreconciled
    );
    assert_eq!(w.balance().await.held_units, 13);
    w.finish().await;
}

pub(super) async fn sealed_store_canaries() {
    let w = World::new().await;
    w.runtime.complete_verified(&w.step(0).await).await.unwrap();
    let sql: Vec<String>=sqlx::query_scalar("SELECT row_to_json(r)::TEXT FROM runtime_attempts r UNION ALL SELECT row_to_json(r)::TEXT FROM runtime_evidence r")
        .fetch_all(&w.runtime.finance.pool).await.unwrap();
    for row in sql {
        for canary in [PROMPT, API_KEY, BODY] {
            assert!(!row.contains(canary));
        }
    }
    let trx = w.runtime.execution.db.create_trx().unwrap();
    let range = foundationdb::RangeOption::from(w.runtime.execution.root.range());
    let values = trx.get_range(&range, 1, true).await.unwrap();
    for value in values.iter() {
        for canary in [PROMPT, API_KEY, BODY] {
            assert!(!value
                .key()
                .windows(canary.len())
                .any(|v| v == canary.as_bytes()));
            assert!(!value
                .value()
                .windows(canary.len())
                .any(|v| v == canary.as_bytes()));
        }
    }
    // Authenticated custody also refuses a different subject/namespace/digest.
    let (digest, sealed) = w
        .runtime
        .custody
        .seal("subject", &serde_json::json!({"body":BODY}))
        .unwrap();
    assert!(w
        .runtime
        .custody
        .open::<serde_json::Value>("wrong", Some(&digest), &sealed)
        .is_err());
    assert!(w
        .runtime
        .custody
        .open::<serde_json::Value>("subject", Some(&hash(b"wrong")), &sealed)
        .is_err());
    w.finish().await;
}
