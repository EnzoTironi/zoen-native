use super::*;

pub(super) async fn run() {
    Box::pin(native_grant_expires_after_admission()).await;
    Box::pin(expired_permit()).await;
    Box::pin(unknown_admission_never_resends()).await;
    Box::pin(provider_reply_loss_and_late_bill()).await;
    Box::pin(continuity_is_still_closed()).await;
}

async fn native_grant_expires_after_admission() {
    let mut fixture = Fixture::new().await;
    let expiry = fixture.world.runtime.finance.reply_clock().await.unwrap() + 3000;
    fixture
        .grant_until("short-native-grant", Some(expiry))
        .await;
    fixture.message(ORIGINAL).await;
    let run = fixture.sync(false).await.unwrap().runs.remove(0);
    let runtime = fixture.world.runtime.clone();
    runtime.fault.store(24, Ordering::SeqCst);
    let paid = start(&runtime, &run);
    cut(&runtime).await;
    assert!(runtime.execution.has_admission(&run.attempt).await);
    sqlx::query("SELECT pg_sleep(GREATEST(0.0, ($1::BIGINT - floor(extract(epoch FROM clock_timestamp()) * 1000)) / 1000.0) + 0.025)")
        .bind(expiry).execute(&runtime.finance.pool).await.unwrap();
    runtime.native_cut_resume.notify_one();
    assert_eq!(paid.await.unwrap().unwrap_err(), RuntimeError::Denied);
    runtime.fault.store(0, Ordering::SeqCst);
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_admission_witnesses")
            .await,
        0
    );
    assert_eq!(
        fixture.world.balance().await.held_units,
        quote(&fixture, &run).await
    );
    assert_zero_send(&fixture).await;
    assert!(runtime.run_model(&run.run).await.is_err());
    let cleaner = without_native(&fixture).await;
    assert_eq!(
        cleaner.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Retained {
            financial: FinancialState::Claimed
        }
    );
    assert_eq!(
        fixture.world.balance().await.held_units,
        quote(&fixture, &run).await
    );
    cleaner.finance.pool.close().await;
    fixture.finish().await;
    println!("native admission deadline cut: actual native grant expires after FDB admission, SQL clock refuses witness and provider send PASS");
}

async fn expired_permit() {
    let (fixture, run) = original().await;
    let runtime = fixture.world.runtime.clone();
    runtime.fault.store(25, Ordering::SeqCst);
    let paid = start(&runtime, &run);
    cut(&runtime).await;
    tokio::time::sleep(Duration::from_millis(2050)).await;
    runtime.native_cut_resume.notify_one();
    assert_eq!(paid.await.unwrap().unwrap_err(), RuntimeError::Denied);
    runtime.fault.store(0, Ordering::SeqCst);
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_admission_witnesses")
            .await,
        1
    );
    assert_zero_send(&fixture).await;
    assert_eq!(
        fixture.world.balance().await.held_units,
        quote(&fixture, &run).await
    );
    assert_eq!(
        runtime.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Retained {
            financial: FinancialState::Claimed
        }
    );
    fixture.finish().await;
    println!("native admission deadline cut: actual finite permit consumption expires, admitted obligation remains nonrefundable PASS");
}

async fn unknown_admission_never_resends() {
    for fault in [1, 2, 3, 23] {
        let (fixture, run) = original().await;
        let runtime = fixture.world.runtime.clone();
        runtime.fault.store(fault, Ordering::SeqCst);
        assert!(runtime.run_model(&run.run).await.is_err());
        runtime.fault.store(0, Ordering::SeqCst);
        let cold = restored(&fixture).await;
        assert!(cold.run_model(&run.run).await.is_err());
        assert_eq!(
            fixture
                .world
                .count("SELECT count(*) FROM runtime_claims")
                .await,
            1
        );
        assert_zero_send(&fixture).await;
        if fault == 1 {
            assert!(!runtime.execution.has_admission(&run.attempt).await);
            assert_eq!(
                runtime.cancel_model(&run.run).await.unwrap(),
                ModelCancellation::Released
            );
            assert_one_release(&fixture).await;
        } else {
            assert!(runtime.execution.has_admission(&run.attempt).await);
            assert_eq!(
                runtime.cancel_model(&run.run).await.unwrap(),
                ModelCancellation::Retained {
                    financial: FinancialState::Claimed
                }
            );
            assert_eq!(
                fixture.world.balance().await.held_units,
                quote(&fixture, &run).await
            );
        }
        cold.finance.pool.close().await;
        fixture.finish().await;
    }
    println!("native admission ACK cuts: consumed claim, known FDB admission and rolled-back or suppressed SQL guard never reconstruct a send PASS");
}

async fn provider_reply_loss_and_late_bill() {
    let (fixture, run) = original().await;
    fixture.world.http.lost.store(true, Ordering::SeqCst);
    assert_eq!(
        fixture
            .world
            .runtime
            .run_model(&run.run)
            .await
            .unwrap()
            .financial,
        FinancialState::Claimed
    );
    let cold = restored(&fixture).await;
    assert!(cold.run_model(&run.run).await.is_err());
    assert_eq!(
        cold.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Retained {
            financial: FinancialState::Claimed
        }
    );
    assert_eq!(fixture.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        fixture.world.balance().await.held_units,
        quote(&fixture, &run).await
    );
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_dispositions")
            .await,
        0
    );
    cold.finance.pool.close().await;
    fixture.finish().await;
    println!("native provider ACK cut: actual HTTP acknowledgement is lost, absent usage retains hold and cold retry cannot resend PASS");

    let (mut fixture, run) = original().await;
    let runtime = fixture.world.runtime.clone();
    fixture.world.http.paused.store(true, Ordering::SeqCst);
    let paid = start(&runtime, &run);
    fixture.world.http.wait().await;
    fixture
        .encrypted(EventBody::GrantRevoked {
            grant: "admitted-owner-trust".into(),
        })
        .await;
    assert_eq!(
        runtime.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Requested
    );
    fixture.world.http.release.notify_one();
    assert_eq!(
        paid.await.unwrap().unwrap().financial,
        FinancialState::Settled { units: 6 }
    );
    fixture.sync(false).await.unwrap();
    assert!(runtime.run_model(&run.run).await.is_err());
    assert_eq!(
        runtime.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Retained {
            financial: FinancialState::Settled { units: 6 }
        }
    );
    assert_eq!(fixture.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        fixture.world.balance().await,
        PeriodBalance {
            held_units: 0,
            spent_units: 6
        }
    );
    fixture.finish().await;
    println!("native late evidence journey: actual revocation commits during HTTP, original incurred bill settles without repeat or output progress PASS");
}

async fn continuity_is_still_closed() {
    let (fixture, run) = original().await;
    let mut cfg = config(
        &fixture.world.db_url,
        &fixture.world.config_namespace,
        &fixture.world.http.base,
    );
    cfg.relay_cell = fixture.world.relay_cell.clone();
    let mut reopened = RuntimeAuthority::open(cfg).await.unwrap();
    reopened.native = Some(fixture.restore_custody());
    assert_eq!(
        reopened.run_model(&run.run).await.unwrap_err(),
        RuntimeError::RestoreUnreconciled
    );
    assert_eq!(
        reopened.cancel_model(&run.run).await.unwrap_err(),
        RuntimeError::RestoreUnreconciled
    );
    let trx = reopened.execution.transaction().await.unwrap();
    assert!(trx
        .get(&reopened.execution.run(&run.run, "cancel"), false)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_attempts")
            .await,
        0
    );
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_predispatch_closures")
            .await,
        0
    );
    assert_zero_send(&fixture).await;
    reopened.finance.pool.close().await;
    fixture.finish().await;
    println!("native continuity journey: matching source/deployment and real custody after reopen still authorize neither send nor refund PASS");
}
