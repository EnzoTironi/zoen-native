use super::*;

pub(super) async fn run() {
    Box::pin(frozen_original_and_restart()).await;
    Box::pin(source_advances_after_refresh()).await;
    Box::pin(source_conflicts_after_final_read()).await;
    Box::pin(current_root_is_required()).await;
    Box::pin(paired_takeover_is_required()).await;
    Box::pin(concurrent_workers()).await;
    Box::pin(gateway_drift()).await;
}

async fn frozen_original_and_restart() {
    let (mut fixture, run) = original().await;
    let before = native::testing::reply_original(&fixture.world.runtime, &run.run).await;
    assert_eq!(before.0, format!("{ORIGINAL}\n"));
    fixture.message(LATER).await;
    fixture.sync(false).await.unwrap();
    let retained = fixture.world.runtime.run_model(&run.run).await.unwrap();
    assert_eq!(retained.financial, FinancialState::Settled { units: 6 });
    {
        let requests = fixture.world.http.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["messages"][0]["content"], before.0);
    }
    assert_eq!(
        native::testing::reply_original(&fixture.world.runtime, &run.run).await,
        before
    );
    let cold = restored(&fixture).await;
    assert!(cold.run_model(&run.run).await.is_err());
    assert_eq!(fixture.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        fixture.world.balance().await,
        PeriodBalance {
            held_units: 0,
            spent_units: 6
        }
    );
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_claims")
            .await,
        1
    );
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_admission_witnesses")
            .await,
        1
    );
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_dispositions")
            .await,
        1
    );
    cold.finance.pool.close().await;
    fixture.finish().await;
    println!("native admission journey: actual paid reply keeps original text and binding across later head and cold replay PASS");
}

async fn source_advances_after_refresh() {
    let (mut fixture, run) = original().await;
    let runtime = fixture.world.runtime.clone();
    let step = runtime.verified_step(&run.run).await.unwrap();
    let original = binding(&runtime, &step);
    runtime
        .execution
        .prepare(&step, &original, &runtime.custody)
        .await
        .unwrap();
    runtime.finance.reserve(&original).await.unwrap();
    let claim = runtime.finance.claim(&original).await.unwrap().unwrap();
    let guard = runtime.finance.guard(&original, &step).await.unwrap();
    fixture.message(LATER).await;
    assert!(matches!(
        runtime
            .execution
            .admit_once(&step, claim, &original, &runtime.custody, &runtime)
            .await,
        Err(RuntimeError::Denied)
    ));
    guard.rollback().await.unwrap();
    assert!(!runtime.execution.has_admission(&run.attempt).await);
    step.release(&runtime.execution).await.unwrap();
    assert_eq!(
        runtime.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Released
    );
    assert_one_release(&fixture).await;
    fixture.finish().await;
    println!("native admission source cut: actual relay append after refreshed facts refuses final admission and releases unsent hold once PASS");
}

async fn source_conflicts_after_final_read() {
    let (mut fixture, run) = original().await;
    let runtime = fixture.world.runtime.clone();
    runtime.fault.store(22, Ordering::SeqCst);
    let paid = start(&runtime, &run);
    cut(&runtime).await;
    fixture.message(LATER).await;
    runtime.native_cut_resume.notify_one();
    assert!(paid.await.unwrap().is_err());
    runtime.fault.store(0, Ordering::SeqCst);
    assert!(!runtime.execution.has_admission(&run.attempt).await);
    assert_eq!(
        runtime.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Released
    );
    assert_one_release(&fixture).await;
    fixture.finish().await;
    println!("native admission source cut: real source write conflicts after final read, no automatic commit retry or HTTP PASS");
}

async fn current_root_is_required() {
    let (fixture, run) = original().await;
    let runtime = fixture.world.runtime.clone();
    let step = runtime.verified_step(&run.run).await.unwrap();
    let original = binding(&runtime, &step);
    let (_, before) = native::testing::reply_lease_snapshot(&runtime, &run.run).await;
    native::testing::repack_reply_holder(&runtime, &step).await;
    let (version, after) = native::testing::reply_lease_snapshot(&runtime, &run.run).await;
    assert_eq!(before, after);
    assert!(after.iter().all(|(_, _, expires)| *expires > version));
    runtime.finance.reserve(&original).await.unwrap();
    let claim = runtime.finance.claim(&original).await.unwrap().unwrap();
    let guard = runtime.finance.guard(&original, &step).await.unwrap();
    assert!(matches!(
        runtime
            .execution
            .admit_once(&step, claim, &original, &runtime.custody, &runtime)
            .await,
        Err(RuntimeError::Denied)
    ));
    guard.rollback().await.unwrap();
    step.release(&runtime.execution).await.unwrap();
    assert_eq!(
        runtime.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Released
    );
    assert_one_release(&fixture).await;
    fixture.finish().await;
    println!("native admission root cut: actual maintenance changes active capsule under unchanged valid paired leases, old step denied PASS");
}

async fn paired_takeover_is_required() {
    let (fixture, run) = original().await;
    let runtime = fixture.world.runtime.clone();
    let step = runtime.verified_step(&run.run).await.unwrap();
    native::testing::stale_reply_admission(&runtime, &step).await;
    assert!(!runtime.execution.has_admission(&run.attempt).await);
    assert_eq!(
        runtime.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Released
    );
    assert_one_release(&fixture).await;
    fixture.finish().await;
    println!("native admission fence cut: forced expiry then actual paired takeover rejects final stale admission and stale release PASS");
}

async fn concurrent_workers() {
    let (fixture, run) = original().await;
    let first = fixture.world.runtime.clone();
    let second = restored(&fixture).await;
    fixture.world.http.paused.store(true, Ordering::SeqCst);
    let paid = start(&first, &run);
    fixture.world.http.wait().await;
    assert_eq!(
        second.run_model(&run.run).await.unwrap_err(),
        RuntimeError::Denied
    );
    let (version, leases) = native::testing::reply_lease_snapshot(&first, &run.run).await;
    assert!(leases.iter().all(|(_, _, expires)| *expires > version));
    fixture.world.http.release.notify_one();
    assert_eq!(
        paid.await.unwrap().unwrap().financial,
        FinancialState::Settled { units: 6 }
    );
    assert_eq!(fixture.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_claims")
            .await,
        1
    );
    second.finance.pool.close().await;
    fixture.finish().await;
    println!("native admission journey: two actual runtimes compete for real paired leases, one claim and provider send PASS");
}

async fn gateway_drift() {
    let (fixture, run) = original().await;
    let mut altered = config(
        &fixture.world.db_url,
        &fixture.world.config_namespace,
        &fixture.world.http.base,
    );
    altered.relay_cell = fixture.world.relay_cell.clone();
    altered.gateway.model = "unapproved-model".into();
    let mut runtime = RuntimeAuthority::open(altered).await.unwrap();
    runtime.continuity.store(true, Ordering::SeqCst);
    runtime.native = Some(fixture.restore_custody());
    assert!(runtime.run_model(&run.run).await.is_err());
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_attempts")
            .await,
        0
    );
    assert_zero_send(&fixture).await;
    assert_eq!(
        runtime.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Released
    );
    runtime.finance.pool.close().await;
    fixture.finish().await;
    println!("native admission profile cut: changed gateway refuses original dispatch while independent cleanup remains usable PASS");
}
