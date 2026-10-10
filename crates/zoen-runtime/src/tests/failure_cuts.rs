use super::*;

pub(super) async fn reservation_ack_lost_reuses_one_hold() {
    let w = World::new().await;
    let step = w.step(0).await;
    w.runtime.fault.store(4, Ordering::SeqCst);
    assert!(w.runtime.complete_verified(&step).await.is_err());
    assert_eq!(
        w.runtime
            .inspect_attempt(&step.request.context.attempt_id)
            .await
            .unwrap(),
        Some(FinancialState::Reserved)
    );
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    let reopened = w.reopen().await;
    let (_, state) = reopened.complete_verified(&step).await.unwrap();
    assert_eq!(state, FinancialState::Settled { units: 6 });
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 1);
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_journals WHERE kind='reserve'")
            .await,
        1
    );
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 1);
    reopened.finance.pool.close().await;
    w.finish().await;
}

pub(super) async fn claim_ack_lost_never_recreates_claim() {
    let w = World::new().await;
    let step = w.step(0).await;
    w.runtime.fault.store(1, Ordering::SeqCst);
    assert!(w.runtime.complete_verified(&step).await.is_err());
    let reopened = w.reopen().await;
    assert!(reopened.complete_verified(&step).await.is_err());
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(w.balance().await.held_units, 13);
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
    assert_eq!(
        w.runtime.finance.release(proof, &w.runtime).await.unwrap(),
        FinancialState::Released
    );
    let replay = w
        .runtime
        .execution
        .close_before_dispatch(&step, &binding, &w.runtime.custody, &w.runtime)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        w.runtime.finance.release(replay, &w.runtime).await.unwrap(),
        FinancialState::Released
    );
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_dispositions").await,
        1
    );
    assert_eq!(w.count("SELECT count(*) FROM runtime_claims").await, 1);
    assert_eq!(w.balance().await.held_units, 0);
    reopened.finance.pool.close().await;
    w.finish().await;
}

pub(super) async fn admission_and_guard_reply_losses_keep_hold() {
    for fault in [2, 3] {
        let w = World::new().await;
        let step = w.step(0).await;
        w.runtime.fault.store(fault, Ordering::SeqCst);
        assert!(w.runtime.complete_verified(&step).await.is_err());
        assert!(
            w.runtime
                .execution
                .has_admission(&step.request.context.attempt_id)
                .await
        );
        let reopened = w.reopen().await;
        assert!(reopened.complete_verified(&step).await.is_err());
        assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
        assert_eq!(w.balance().await.held_units, 13);
        let binding = w
            .runtime
            .execution
            .prepared_binding(&step.request.context.attempt_id, &w.runtime.custody)
            .await
            .unwrap();
        assert!(w
            .runtime
            .execution
            .close_before_dispatch(&step, &binding, &w.runtime.custody, &w.runtime)
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            w.count("SELECT count(*) FROM runtime_dispositions").await,
            0
        );
        assert_eq!(
            w.count("SELECT count(*) FROM runtime_admission_witnesses")
                .await,
            0
        );
        reopened.finance.pool.close().await;
        w.finish().await;
    }
}

pub(super) async fn admission_tombstone_race_and_old_worker() {
    let w = World::new().await;
    for _ in 0..12 {
        let step = w.step(0).await;
        let binding = w.binding(&step);
        w.runtime
            .execution
            .prepare(&step, &binding, &w.runtime.custody)
            .await
            .unwrap();
        w.runtime.finance.reserve(&binding).await.unwrap();
        let claim = w.runtime.finance.claim(&binding).await.unwrap().unwrap();
        let guard = w.runtime.finance.guard(&binding).await.unwrap();
        let (admitted, closed) = tokio::join!(
            w.runtime
                .execution
                .admit_once(&step, claim, &binding, &w.runtime.custody),
            w.runtime.execution.close_before_dispatch(
                &step,
                &binding,
                &w.runtime.custody,
                &w.runtime
            ),
        );
        match admitted {
            Ok(admission) => {
                assert!(!matches!(closed, Ok(Some(_))));
                guard.finish(admission).await.unwrap().consume();
                assert!(w
                    .runtime
                    .execution
                    .close_before_dispatch(&step, &binding, &w.runtime.custody, &w.runtime)
                    .await
                    .unwrap()
                    .is_none());
            }
            Err(_) => {
                guard.rollback().await.unwrap();
                let proof = match closed {
                    Ok(Some(proof)) => proof,
                    _ => w
                        .runtime
                        .execution
                        .close_before_dispatch(&step, &binding, &w.runtime.custody, &w.runtime)
                        .await
                        .unwrap()
                        .unwrap(),
                };
                w.runtime.finance.release(proof, &w.runtime).await.unwrap();
                assert!(
                    !w.runtime
                        .execution
                        .has_admission(&binding.context.attempt_id)
                        .await
                );
                assert!(w.runtime.complete_verified(&step).await.is_err());
            }
        }
    }
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    w.finish().await;
}

pub(super) async fn both_admission_closure_orderings() {
    for closure_first in [true, false] {
        let w = World::new().await;
        let step = w.step(0).await;
        let binding = w.binding(&step);
        w.runtime
            .execution
            .prepare(&step, &binding, &w.runtime.custody)
            .await
            .unwrap();
        w.runtime.finance.reserve(&binding).await.unwrap();
        let claim = w.runtime.finance.claim(&binding).await.unwrap().unwrap();
        let guard = w.runtime.finance.guard(&binding).await.unwrap();
        if closure_first {
            let closure = w
                .runtime
                .execution
                .close_before_dispatch(&step, &binding, &w.runtime.custody, &w.runtime)
                .await
                .unwrap()
                .unwrap();
            // A worker already holding a known-fresh claim still loses to the
            // durable tombstone. Reading or replaying cannot resurrect it.
            assert!(w
                .runtime
                .execution
                .admit_once(&step, claim, &binding, &w.runtime.custody,)
                .await
                .is_err());
            guard.rollback().await.unwrap();
            w.runtime
                .finance
                .release(closure, &w.runtime)
                .await
                .unwrap();
            assert_eq!(w.balance().await.held_units, 0);
            assert!(
                !w.runtime
                    .execution
                    .has_admission(&binding.context.attempt_id)
                    .await
            );
        } else {
            let admission = w
                .runtime
                .execution
                .admit_once(&step, claim, &binding, &w.runtime.custody)
                .await
                .unwrap();
            guard.finish(admission).await.unwrap().consume();
            assert!(w
                .runtime
                .execution
                .close_before_dispatch(&step, &binding, &w.runtime.custody, &w.runtime,)
                .await
                .unwrap()
                .is_none());
            assert_eq!(w.balance().await.held_units, 13);
            assert_eq!(
                w.count("SELECT count(*) FROM runtime_dispositions").await,
                0
            );
        }
        assert!(w.runtime.complete_verified(&step).await.is_err());
        assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
        w.finish().await;
    }
}

pub(super) async fn revocation_does_not_hold_http_lock() {
    let w = World::new().await;
    let step = Arc::new(w.step(0).await);
    w.http.paused.store(true, Ordering::SeqCst);
    let (runtime, task_step) = (w.runtime.clone(), step.clone());
    let call = tokio::spawn(async move { runtime.complete_verified(&task_step).await });
    w.http.wait().await;
    tokio::time::timeout(Duration::from_secs(1), async {
        let mut tx = w.runtime.finance.pool.begin().await.unwrap();
        assert!(zoen_relay::db::revoke_device(
            &mut tx,
            &w.agents[0].identity,
            w.agents[0].device.as_deref().unwrap()
        )
        .await
        .unwrap());
        tx.commit().await.unwrap();
    })
    .await
    .expect("provider latency must not hold the directory lock");
    assert!(!call.is_finished());
    w.http.release.notify_one();
    let (_, state) = call.await.unwrap().unwrap();
    assert_eq!(state, FinancialState::Settled { units: 6 });
    assert!(w.runtime.complete_verified(&step).await.is_err());
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 1);
    w.finish().await;
}

pub(super) async fn takeover_retains_late_bill_but_denies_progress() {
    let w = World::new().await;
    let step = Arc::new(w.step(0).await);
    w.http.paused.store(true, Ordering::SeqCst);
    let (runtime, task_step) = (w.runtime.clone(), step.clone());
    let call = tokio::spawn(async move { runtime.complete_verified(&task_step).await });
    w.http.wait().await;
    w.runtime.execution.take_device(&step).await;
    w.http.release.notify_one();
    assert_eq!(
        call.await.unwrap().unwrap().1,
        FinancialState::Settled { units: 6 }
    );
    assert!(w.runtime.complete_verified(&step).await.is_err());
    let binding = w
        .runtime
        .execution
        .prepared_binding(&step.request.context.attempt_id, &w.runtime.custody)
        .await
        .unwrap();
    assert_eq!(
        w.runtime
            .execution
            .prepare(&step, &binding, &w.runtime.custody)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 1);
    w.finish().await;
}

pub(super) async fn cancellation_and_lost_http_never_repeat() {
    for lost in [false, true] {
        let w = World::new().await;
        let step = Arc::new(w.step(0).await);
        w.http.paused.store(!lost, Ordering::SeqCst);
        w.http.lost.store(lost, Ordering::SeqCst);
        let (runtime, task_step) = (w.runtime.clone(), step.clone());
        let call = tokio::spawn(async move { runtime.complete_verified(&task_step).await });
        w.http.wait().await;
        if !lost {
            call.abort();
            assert!(call.await.unwrap_err().is_cancelled());
            w.http.release.notify_one();
        } else {
            assert_eq!(call.await.unwrap().unwrap().1, FinancialState::Claimed);
        }
        let reopened = w.reopen().await;
        assert!(reopened.complete_verified(&step).await.is_err());
        assert_eq!(w.balance().await.held_units, 13);
        assert_eq!(w.http.sends.load(Ordering::SeqCst), 1);
        reopened.finance.pool.close().await;
        w.finish().await;
    }
}
