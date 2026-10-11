use super::*;

pub(super) async fn native_device_custody() {
    let mut w = World::new().await;
    let actor = w.agents[0].clone();
    let device = actor.device.clone().unwrap();
    let agent: sqlx::types::Json<Identity> =
        sqlx::query_scalar("SELECT profile FROM identities WHERE id=$1")
            .bind(&actor.identity)
            .fetch_one(&w.runtime.finance.pool)
            .await
            .unwrap();
    let owner: sqlx::types::Json<Identity> =
        sqlx::query_scalar("SELECT profile FROM identities WHERE id=$1")
            .bind(&w.owner.identity)
            .fetch_one(&w.runtime.finance.pool)
            .await
            .unwrap();
    assert_eq!(
        w.runtime
            .inspect_native_device(&actor.identity, &device)
            .await
            .unwrap_err(),
        RuntimeError::CoreAuthorityUnavailable
    );
    Arc::get_mut(&mut w.runtime).unwrap().native = Some(native::testing::custody(
        &w.config_namespace,
        agent.0.clone(),
        owner.0.clone(),
        actor.cert.clone().unwrap(),
        actor.key.secret(),
    ));
    native::testing::provision(&w.runtime, &actor.identity, &device).await;
    let original = w
        .runtime
        .inspect_native_device(&actor.identity, &device)
        .await
        .unwrap();
    assert_eq!(original.generation, 1);
    let (worker, worker_agent, worker_device) =
        (w.runtime.clone(), actor.identity.clone(), device.clone());
    let packed = tokio::spawn(async move {
        worker
            .repack_native_device(&worker_agent, &worker_device)
            .await
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(packed.generation, 2);
    assert_eq!(packed.image_bytes, original.image_bytes);
    let mut reopened = w.reopen().await;
    assert_eq!(
        reopened
            .inspect_native_device(&actor.identity, &device)
            .await
            .unwrap_err(),
        RuntimeError::CoreAuthorityUnavailable
    );
    Arc::get_mut(&mut reopened).unwrap().native = Some(native::testing::custody(
        &w.config_namespace,
        agent.0,
        owner.0,
        actor.cert.clone().unwrap(),
        actor.key.secret(),
    ));
    assert_eq!(
        reopened
            .inspect_native_device(&actor.identity, &device)
            .await
            .unwrap(),
        packed
    );
    native::testing::authorization_cuts(&reopened, &actor.identity, &device).await;
    native::testing::storage_cuts(&reopened, &actor.identity, &device).await;
    assert_eq!(
        reopened.run_model("fixture-run").await.unwrap_err(),
        RuntimeError::Denied
    );
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    let before_cut = native::testing::generation(&reopened, &actor.identity, &device).await;
    reopened.fault.store(14, Ordering::SeqCst);
    let (worker, worker_agent, worker_device) =
        (reopened.clone(), actor.identity.clone(), device.clone());
    let paused = tokio::spawn(async move {
        worker
            .repack_native_device(&worker_agent, &worker_device)
            .await
    });
    tokio::time::timeout(
        Duration::from_secs(3),
        reopened.native_cut_entered.notified(),
    )
    .await
    .unwrap();
    // The real 5s SQL idle timeout terminates the paused guard and releases its
    // FOR SHARE locks. This revocation actually commits before the worker resumes.
    tokio::time::timeout(
        Duration::from_secs(8),
        sqlx::query("UPDATE devices SET revoked_at=clock_timestamp() WHERE device=$1")
            .bind(&device)
            .execute(&reopened.finance.pool),
    )
    .await
    .unwrap()
    .unwrap();
    reopened.native_cut_resume.notify_one();
    assert_eq!(
        paused.await.unwrap().unwrap_err(),
        RuntimeError::Unavailable
    );
    let after_cut = native::testing::generation(&reopened, &actor.identity, &device).await;
    println!("native SQL idle revocation cut: before={before_cut}, after={after_cut}");
    assert_eq!(
        after_cut, before_cut,
        "a lost SQL guard must not advance the native root before returning an error"
    );
    reopened.fault.store(0, Ordering::SeqCst);
    assert_eq!(
        reopened
            .inspect_native_device(&actor.identity, &device)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert_eq!(
        reopened
            .repack_native_device(&actor.identity, &device)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    reopened.finance.pool.close().await;
    w.finish().await;

    let mut admitted = World::new().await;
    let actor = admitted.agents[0].clone();
    let device = actor.device.clone().unwrap();
    let agent: sqlx::types::Json<Identity> =
        sqlx::query_scalar("SELECT profile FROM identities WHERE id=$1")
            .bind(&actor.identity)
            .fetch_one(&admitted.runtime.finance.pool)
            .await
            .unwrap();
    let owner: sqlx::types::Json<Identity> =
        sqlx::query_scalar("SELECT profile FROM identities WHERE id=$1")
            .bind(&admitted.owner.identity)
            .fetch_one(&admitted.runtime.finance.pool)
            .await
            .unwrap();
    Arc::get_mut(&mut admitted.runtime).unwrap().native = Some(native::testing::custody(
        &admitted.config_namespace,
        agent.0,
        owner.0,
        actor.cert.clone().unwrap(),
        actor.key.secret(),
    ));
    native::testing::provision(&admitted.runtime, &actor.identity, &device).await;
    native::testing::admitted_before_revocation(&admitted.runtime, &actor.identity, &device).await;
    assert_eq!(
        admitted
            .count("SELECT count(*) FROM runtime_attempts")
            .await,
        0
    );
    assert_eq!(admitted.http.sends.load(Ordering::SeqCst), 0);
    admitted.finish().await;
}

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
            .inspect_attempt(&step.request().context.attempt_id)
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
            .inspect_attempt(&step.request().context.attempt_id)
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
        .prepared_binding(&step.request().context.attempt_id, &w.runtime.custody)
        .await
        .unwrap();
    // Actually remove an admission in this throwaway namespace, simulating a
    // rewound inventory. Explicit external declaration closes the barrier.
    let trx = w.runtime.execution.db.create_trx().unwrap();
    trx.clear(&w.runtime.execution.root.pack(&(
        "attempt",
        step.request().context.attempt_id.as_str(),
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
        .prepared_binding(&step.request().context.attempt_id, &w.runtime.custody)
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
