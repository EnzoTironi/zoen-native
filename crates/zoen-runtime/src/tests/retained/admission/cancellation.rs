use super::*;

pub(super) async fn run() -> std::result::Result<(), Box<dyn std::error::Error>> {
    Box::pin(no_reservation_creates_no_fake_finance()).await?;
    Box::pin(valid_holder_before_reservation()).await?;
    Box::pin(valid_holder_during_http()).await?;
    Box::pin(request_conflicts_after_final_read()).await?;
    Box::pin(request_after_known_admission()).await?;
    Box::pin(actual_expiry_blocks_late_reservation()).await?;
    Box::pin(independent_cleanup_after_permission_loss()).await?;
    Box::pin(known_ack_losses()).await?;
    Box::pin(exact_marker_replay()).await?;
    Box::pin(both_fenced_orderings()).await?;
    Ok(())
}

async fn no_reservation_creates_no_fake_finance(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (fixture, run) = original().await?;
    let cleaner = without_native(&fixture).await;
    let policies = fixture
        .world
        .count("SELECT count(*) FROM runtime_policies")
        .await;
    let periods = fixture
        .world
        .count("SELECT count(*) FROM runtime_periods")
        .await;
    assert_eq!(periods, 1);
    for _ in 0..2 {
        assert_eq!(
            cleaner.cancel_model(&run.run).await.unwrap(),
            ModelCancellation::Released
        );
    }
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_predispatch_closures")
            .await,
        1
    );
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
            .count("SELECT count(*) FROM runtime_postings")
            .await,
        0
    );
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_dispositions")
            .await,
        0
    );
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_policies")
            .await,
        policies
    );
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_periods")
            .await,
        periods
    );
    assert_eq!(cleaner.inspect_attempt(&run.attempt).await.unwrap(), None);
    assert_zero_send(&fixture).await;
    cleaner.finance.pool.close().await;
    fixture.finish().await;
    println!("native cancellation journey: installed policy supplies original period, cancellation before reserve creates no attempt or posting PASS");
    Ok(())
}

async fn valid_holder_before_reservation() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (fixture, run) = original().await?;
    let runtime = fixture.world.runtime.clone();
    let cleaner = without_native(&fixture).await;
    runtime.fault.store(21, Ordering::SeqCst);
    let paid = start(&runtime, &run);
    cut(&runtime).await;
    let (version, before) = native::testing::reply_lease_snapshot(&runtime, &run.run).await;
    assert!(before.iter().all(|(_, _, expires)| *expires > version));
    assert_eq!(
        cleaner.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Requested
    );
    let (_, after) = native::testing::reply_lease_snapshot(&runtime, &run.run).await;
    assert_eq!(before, after);
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_predispatch_closures")
            .await,
        0
    );
    runtime.native_cut_resume.notify_one();
    assert!(paid.await.unwrap().is_err());
    runtime.fault.store(0, Ordering::SeqCst);
    assert!(!runtime.execution.has_admission(&run.attempt).await);
    assert_eq!(
        fixture.world.balance().await.held_units,
        quote(&fixture, &run).await
    );
    assert_eq!(
        cleaner.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Released
    );
    assert_one_release(&fixture).await;
    cleaner.finance.pool.close().await;
    fixture.finish().await;
    println!("native cancellation journey: valid holder receives permanent request before reserve, final admission stops and later cleanup releases once PASS");
    Ok(())
}

async fn valid_holder_during_http() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (fixture, run) = original().await?;
    let runtime = fixture.world.runtime.clone();
    let cleaner = without_native(&fixture).await;
    fixture.world.http.paused.store(true, Ordering::SeqCst);
    let paid = start(&runtime, &run);
    fixture.world.http.wait().await;
    let (version, before) = native::testing::reply_lease_snapshot(&runtime, &run.run).await;
    assert!(before.iter().all(|(_, _, expires)| *expires > version));
    cleaner.fault.store(26, Ordering::SeqCst);
    assert_eq!(
        cleaner.cancel_model(&run.run).await.unwrap_err(),
        RuntimeError::Unavailable
    );
    cleaner.fault.store(0, Ordering::SeqCst);
    assert_eq!(
        cleaner.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Requested
    );
    let (_, after) = native::testing::reply_lease_snapshot(&runtime, &run.run).await;
    assert_eq!(before, after);
    assert_eq!(
        fixture.world.balance().await.held_units,
        quote(&fixture, &run).await
    );
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_predispatch_closures")
            .await,
        0
    );
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_dispositions")
            .await,
        0
    );
    fixture.world.http.release.notify_one();
    assert_eq!(
        paid.await.unwrap().unwrap().financial,
        FinancialState::Settled { units: 6 }
    );
    assert_eq!(
        cleaner.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Retained {
            financial: FinancialState::Settled { units: 6 }
        }
    );
    assert_eq!(fixture.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_dispositions WHERE kind='release'")
            .await,
        0
    );
    cleaner.finance.pool.close().await;
    fixture.finish().await;
    println!("native cancellation HTTP cut: lost known request ACK and exact replay preserve valid worker leases, admitted bill never refunds or resends PASS");
    Ok(())
}

async fn request_conflicts_after_final_read() -> std::result::Result<(), Box<dyn std::error::Error>>
{
    let (fixture, run) = original().await?;
    let runtime = fixture.world.runtime.clone();
    let cleaner = without_native(&fixture).await;
    runtime.fault.store(22, Ordering::SeqCst);
    let paid = start(&runtime, &run);
    cut(&runtime).await;
    assert_eq!(
        cleaner.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Requested
    );
    runtime.native_cut_resume.notify_one();
    assert!(paid.await.unwrap().is_err());
    runtime.fault.store(0, Ordering::SeqCst);
    assert!(!runtime.execution.has_admission(&run.attempt).await);
    assert_eq!(
        cleaner.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Released
    );
    assert_one_release(&fixture).await;
    cleaner.finance.pool.close().await;
    fixture.finish().await;
    println!("native cancellation admission race: actual request write conflicts after final admission read, no HTTP and one later release PASS");
    Ok(())
}

async fn request_after_known_admission() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (fixture, run) = original().await?;
    let runtime = fixture.world.runtime.clone();
    let cleaner = without_native(&fixture).await;
    runtime.fault.store(24, Ordering::SeqCst);
    let paid = start(&runtime, &run);
    cut(&runtime).await;
    assert!(runtime.execution.has_admission(&run.attempt).await);
    assert_eq!(
        cleaner.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Requested
    );
    assert_zero_send(&fixture).await;
    assert_eq!(
        fixture.world.balance().await.held_units,
        quote(&fixture, &run).await
    );
    runtime.native_cut_resume.notify_one();
    assert_eq!(
        paid.await.unwrap().unwrap().financial,
        FinancialState::Settled { units: 6 }
    );
    runtime.fault.store(0, Ordering::SeqCst);
    assert_eq!(
        cleaner.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Retained {
            financial: FinancialState::Settled { units: 6 }
        }
    );
    assert_eq!(fixture.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_dispositions WHERE kind='release'")
            .await,
        0
    );
    cleaner.finance.pool.close().await;
    fixture.finish().await;
    println!("native cancellation admission race: known admission precedes request, finite dispatch settles once and cannot refund PASS");
    Ok(())
}

async fn actual_expiry_blocks_late_reservation(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (fixture, run) = original().await?;
    let runtime = fixture.world.runtime.clone();
    let cleaner = without_native(&fixture).await;
    runtime.fault.store(21, Ordering::SeqCst);
    let paid = start(&runtime, &run);
    cut(&runtime).await;
    let (version, leases) = native::testing::reply_lease_snapshot(&runtime, &run.run).await;
    let expiry = leases.iter().map(|(_, _, expires)| *expires).max().unwrap();
    assert!(expiry > version);
    println!("native cancellation expiry control: waiting for actual unchanged 60000000-version paired lease deadline");
    let deadline = std::time::Instant::now() + Duration::from_secs(100);
    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "real lease did not expire within bounded fixture observation"
        );
        let trx = runtime.execution.transaction().await.unwrap();
        if trx.get_read_version().await.unwrap() >= expiry {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert_eq!(
        cleaner.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Released
    );
    runtime.native_cut_resume.notify_one();
    assert!(paid.await.unwrap().is_err());
    runtime.fault.store(0, Ordering::SeqCst);
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
        1
    );
    assert_eq!(fixture.world.balance().await.held_units, 0);
    assert_zero_send(&fixture).await;
    cleaner.finance.pool.close().await;
    fixture.finish().await;
    println!("native cancellation expiry cut: real lease expiry hands off cleanup, permanent SQL gate rejects late old reservation PASS");
    Ok(())
}

async fn independent_cleanup_after_permission_loss(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for loss in 0..4 {
        let (mut fixture, run) = original().await?;
        let runtime = fixture.world.runtime.clone();
        runtime.fault.store(4, Ordering::SeqCst);
        assert!(runtime.run_model(&run.run).await.is_err());
        runtime.fault.store(0, Ordering::SeqCst);
        assert_eq!(
            fixture.world.balance().await.held_units,
            quote(&fixture, &run).await
        );
        match loss {
            0 => {
                fixture
                    .encrypted(EventBody::GrantRevoked {
                        grant: "admitted-owner-trust".into(),
                    })
                    .await?;
                fixture.sync(false).await.unwrap();
            }
            1 => {
                let mut tx = runtime.finance.pool.begin().await.unwrap();
                assert!(zoen_relay::db::revoke_device(
                    &mut tx,
                    &fixture.agent.id,
                    fixture.world.agents[0].device.as_deref().unwrap()
                )
                .await
                .unwrap());
                tx.commit().await.unwrap();
            }
            2 => {
                let mut policy = fixture.world.signed.policy.clone();
                policy.version += 1;
                policy.previous_digest = Some(fixture.world.policy_digest.clone());
                policy.enabled = false;
                runtime
                    .install_policy(
                        event_log::owner_budget::sign(&fixture.world.owner, policy).unwrap(),
                    )
                    .await
                    .unwrap();
            }
            _ => {
                fixture.message(LATER).await?;
                // Actual relay is ahead; no native hydrate/currentness is available.
            }
        }
        assert!(runtime.run_model(&run.run).await.is_err());
        let cleaner = without_native(&fixture).await;
        let periods = fixture
            .world
            .count("SELECT count(*) FROM runtime_periods")
            .await;
        let policies = fixture
            .world
            .count("SELECT count(*) FROM runtime_policies")
            .await;
        assert_eq!(
            cleaner.cancel_model(&run.run).await.unwrap(),
            ModelCancellation::Released
        );
        assert_eq!(
            cleaner.cancel_model(&run.run).await.unwrap(),
            ModelCancellation::Released
        );
        assert_one_release(&fixture).await;
        assert_eq!(
            fixture
                .world
                .count("SELECT count(*) FROM runtime_periods")
                .await,
            periods
        );
        assert_eq!(
            fixture
                .world
                .count("SELECT count(*) FROM runtime_policies")
                .await,
            policies
        );
        cleaner.finance.pool.close().await;
        fixture.finish().await;
    }
    println!("native cancellation permission cuts: revoked grant or device, disabled latest policy and unavailable current native head still permit exact unsent cleanup PASS");
    Ok(())
}

include!("cancellation/replay.rs");
