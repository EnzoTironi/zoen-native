use super::*;

pub(super) async fn matching_workers_and_immutable_pairing() {
    let w = World::with_initial_workers(4, false).await;
    let mut workers = Vec::new();
    for _ in 0..4 {
        let (url, namespace, base) = (
            w.db_url.clone(),
            w.config_namespace.clone(),
            w.http.base.clone(),
        );
        workers.push(tokio::spawn(async move {
            RuntimeAuthority::open(config(&url, &namespace, &base))
                .await
                .unwrap()
        }));
    }
    for worker in workers {
        let worker = worker.await.unwrap();
        assert_eq!(worker.continuity(), Err(RuntimeError::RestoreUnreconciled));
        worker.finance.pool.close().await;
    }
    let mut changed_key = config(&w.db_url, &w.config_namespace, &w.http.base);
    changed_key.evidence_key = [72; 32];
    assert!(matches!(
        RuntimeAuthority::open(changed_key).await,
        Err(RuntimeError::DeploymentMismatch)
    ));
    for query in [
        "UPDATE runtime_deployment_binding SET namespace='replacement'",
        "DELETE FROM runtime_deployment_binding",
        "TRUNCATE runtime_deployment_binding CASCADE",
    ] {
        let error = sqlx::query(query)
            .execute(&w.runtime.finance.pool)
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("23514")
        );
    }
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_deployment_binding")
            .await,
        1
    );
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    w.finish().await;
    let w = World::with_initial_workers(4, true).await;
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_deployment_binding")
            .await,
        1
    );
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    w.finish().await;
}

pub(super) async fn failed_initial_sql_commit_keeps_orphan_closed() {
    let w = World::new().await;
    // Reset only the initial store pairing in this empty-attempt fixture. All
    // owner profiles and signed policies remain; no financial obligation is lost.
    sqlx::query("ALTER TABLE runtime_deployment_binding DISABLE TRIGGER immutable_deployment")
        .execute(&w.runtime.finance.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM runtime_deployment_binding")
        .execute(&w.runtime.finance.pool)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE runtime_deployment_binding ENABLE TRIGGER immutable_deployment")
        .execute(&w.runtime.finance.pool)
        .await
        .unwrap();
    let key = w.runtime.execution.root.pack(&("deployment",));
    let trx = w.runtime.execution.db.create_trx().unwrap();
    trx.clear(&key);
    trx.commit().await.unwrap();
    sqlx::query("CREATE CONSTRAINT TRIGGER refuse_pairing_commit AFTER INSERT ON runtime_deployment_binding DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION runtime_history_immutable()")
        .execute(&w.runtime.finance.pool).await.unwrap();
    assert!(matches!(
        RuntimeAuthority::open(config(&w.db_url, &w.config_namespace, &w.http.base)).await,
        Err(RuntimeError::Unavailable)
    ));
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_deployment_binding")
            .await,
        0
    );
    let trx = w.runtime.execution.db.create_trx().unwrap();
    assert!(trx.get(&key, false).await.unwrap().is_some());
    sqlx::query("DROP TRIGGER refuse_pairing_commit ON runtime_deployment_binding")
        .execute(&w.runtime.finance.pool)
        .await
        .unwrap();
    assert!(matches!(
        RuntimeAuthority::open(config(&w.db_url, &w.config_namespace, &w.http.base)).await,
        Err(RuntimeError::DeploymentMismatch)
    ));
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_deployment_binding")
            .await,
        0
    );
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    w.finish().await;
}

pub(super) async fn changed_marker_closes_open_and_execution() {
    let w = World::new().await;
    let step = w.step(0).await;
    w.runtime.fault.store(4, Ordering::SeqCst);
    assert!(w.runtime.complete_verified(&step).await.is_err());
    w.runtime.fault.store(0, Ordering::SeqCst);
    let binding = w
        .runtime
        .execution
        .prepared_binding(&step.request.context.attempt_id, &w.runtime.custody)
        .await
        .unwrap();
    let witness: String = sqlx::query_scalar("SELECT witness FROM runtime_deployment_binding")
        .fetch_one(&w.runtime.finance.pool)
        .await
        .unwrap();
    let key = w.runtime.execution.root.pack(&("deployment",));
    for altered in [None, Some("0".repeat(64))] {
        let trx = w.runtime.execution.db.create_trx().unwrap();
        match altered {
            None => trx.clear(&key),
            Some(value) => trx.set(&key, value.as_bytes()),
        }
        trx.commit().await.unwrap();
        assert!(matches!(
            RuntimeAuthority::open(config(&w.db_url, &w.config_namespace, &w.http.base)).await,
            Err(RuntimeError::DeploymentMismatch)
        ));
        assert!(w.runtime.complete_verified(&step).await.is_err());
        assert!(matches!(
            w.runtime
                .execution
                .close_before_dispatch(&step, &binding, &w.runtime.custody, &w.runtime)
                .await,
            Err(RuntimeError::DeploymentMismatch)
        ));
        assert!(matches!(
            w.runtime.reconcile(64).await,
            Err(RuntimeError::DeploymentMismatch)
        ));
        assert_eq!(w.balance().await.held_units, 13);
        assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
        let trx = w.runtime.execution.db.create_trx().unwrap();
        trx.set(&key, witness.as_bytes());
        trx.commit().await.unwrap();
    }
    w.finish().await;
}

pub(super) async fn orphaned_marker_is_not_adopted() {
    let w = World::new().await;
    // Explicitly remove the SQL side in this isolated fixture. This is an
    // orphan/restore cut, not a production repair operation or wire ACK fault.
    sqlx::query("ALTER TABLE runtime_deployment_binding DISABLE TRIGGER immutable_deployment")
        .execute(&w.runtime.finance.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM runtime_deployment_binding")
        .execute(&w.runtime.finance.pool)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE runtime_deployment_binding ENABLE TRIGGER immutable_deployment")
        .execute(&w.runtime.finance.pool)
        .await
        .unwrap();
    assert!(matches!(
        RuntimeAuthority::open(config(&w.db_url, &w.config_namespace, &w.http.base)).await,
        Err(RuntimeError::DeploymentMismatch)
    ));
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_deployment_binding")
            .await,
        0
    );
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    w.finish().await;
}

pub(super) async fn unbound_attempts_cannot_be_reassigned() {
    let w = World::new().await;
    let step = w.step(0).await;
    w.runtime.fault.store(4, Ordering::SeqCst);
    assert!(w.runtime.complete_verified(&step).await.is_err());
    // Model a retained attempt from the original unbound SQL format. Do not
    // delete its obligation, claim or journal to make startup look empty.
    sqlx::query("ALTER TABLE runtime_attempts DROP CONSTRAINT runtime_attempt_deployment")
        .execute(&w.runtime.finance.pool)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE runtime_deployment_binding DISABLE TRIGGER immutable_deployment")
        .execute(&w.runtime.finance.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM runtime_deployment_binding")
        .execute(&w.runtime.finance.pool)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE runtime_deployment_binding ENABLE TRIGGER immutable_deployment")
        .execute(&w.runtime.finance.pool)
        .await
        .unwrap();
    assert!(matches!(
        RuntimeAuthority::open(config(
            &w.db_url,
            &roda_types::new_id("replacement"),
            &w.http.base
        ))
        .await,
        Err(RuntimeError::DeploymentMismatch)
    ));
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_deployment_binding")
            .await,
        0
    );
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 1);
    assert_eq!(w.balance().await.held_units, 13);
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    w.finish().await;
}
