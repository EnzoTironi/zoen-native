use super::*;

pub(super) async fn run() {
    Box::pin(no_reservation_creates_no_fake_finance()).await;
    Box::pin(valid_holder_before_reservation()).await;
    Box::pin(valid_holder_during_http()).await;
    Box::pin(request_conflicts_after_final_read()).await;
    Box::pin(request_after_known_admission()).await;
    Box::pin(actual_expiry_blocks_late_reservation()).await;
    Box::pin(independent_cleanup_after_permission_loss()).await;
    Box::pin(known_ack_losses()).await;
    Box::pin(exact_marker_replay()).await;
    Box::pin(both_fenced_orderings()).await;
}

async fn no_reservation_creates_no_fake_finance() {
    let (fixture, run) = original().await;
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
}

async fn valid_holder_before_reservation() {
    let (fixture, run) = original().await;
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
}

async fn valid_holder_during_http() {
    let (fixture, run) = original().await;
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
}

async fn request_conflicts_after_final_read() {
    let (fixture, run) = original().await;
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
}

async fn request_after_known_admission() {
    let (fixture, run) = original().await;
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
}

async fn actual_expiry_blocks_late_reservation() {
    let (fixture, run) = original().await;
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
}

async fn independent_cleanup_after_permission_loss() {
    for loss in 0..4 {
        let (mut fixture, run) = original().await;
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
                    .await;
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
                        roda_log::owner_budget::sign(&fixture.world.owner, policy).unwrap(),
                    )
                    .await
                    .unwrap();
            }
            _ => {
                fixture.message(LATER).await;
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
}

async fn known_ack_losses() {
    for fault in [26, 27, 28] {
        let (fixture, run) = original().await;
        let runtime = fixture.world.runtime.clone();
        runtime.fault.store(4, Ordering::SeqCst);
        assert!(runtime.run_model(&run.run).await.is_err());
        runtime.fault.store(fault, Ordering::SeqCst);
        assert_eq!(
            runtime.cancel_model(&run.run).await.unwrap_err(),
            RuntimeError::Unavailable
        );
        assert_zero_send(&fixture).await;
        assert_eq!(
            fixture.world.balance().await.held_units,
            if fault == 28 {
                0
            } else {
                quote(&fixture, &run).await
            }
        );
        assert_eq!(
            fixture
                .world
                .count("SELECT count(*) FROM runtime_dispositions WHERE kind='release'")
                .await,
            if fault == 28 { 1 } else { 0 }
        );
        runtime.fault.store(0, Ordering::SeqCst);
        assert_eq!(
            runtime.cancel_model(&run.run).await.unwrap(),
            ModelCancellation::Released
        );
        assert_eq!(
            runtime.cancel_model(&run.run).await.unwrap(),
            ModelCancellation::Released
        );
        assert_one_release(&fixture).await;
        fixture.finish().await;
    }
    println!("native cancellation ACK cuts: explicit suppression after known request, fenced closure or SQL release commit never duplicates release PASS");
}

async fn exact_marker_replay() {
    let (mut fixture, run) = original().await;
    let runtime = fixture.world.runtime.clone();
    runtime
        .execution
        .request_reply_cancel(&run.run, &runtime.custody, &runtime)
        .await
        .unwrap();
    let key = runtime.execution.run(&run.run, "cancel");
    let trx = runtime.execution.transaction().await.unwrap();
    let sealed = trx.get(&key, false).await.unwrap().unwrap().to_vec();
    let original: serde_json::Value = runtime
        .custody
        .open(&format!("native-cancel/1/{}", run.run), None, &sealed)
        .unwrap();
    for field in ["run", "attempt", "record_digest", "binding_digest"] {
        let mut wrong = original.clone();
        wrong[field] = serde_json::Value::String(if field.ends_with("digest") {
            hash(b"wrong cancellation original")
        } else {
            new_id("wrong")
        });
        let (_, forged) = runtime
            .custody
            .seal(&format!("native-cancel/1/{}", run.run), &wrong)
            .unwrap();
        let trx = runtime.execution.transaction().await.unwrap();
        trx.set(&key, &forged);
        trx.commit().await.unwrap();
        assert_eq!(
            runtime.cancel_model(&run.run).await.unwrap_err(),
            RuntimeError::InvalidBinding
        );
    }
    let trx = runtime.execution.transaction().await.unwrap();
    trx.set(&key, &sealed);
    trx.commit().await.unwrap();
    runtime
        .execution
        .request_reply_cancel(&run.run, &runtime.custody, &runtime)
        .await
        .unwrap();
    let trx = runtime.execution.transaction().await.unwrap();
    assert_eq!(
        trx.get(&key, false).await.unwrap().unwrap().as_ref(),
        sealed.as_slice()
    );
    fixture.message(LATER).await;
    let other = fixture.sync(false).await.unwrap().runs.remove(0);
    let other_key = runtime.execution.run(&other.run, "cancel");
    let trx = runtime.execution.transaction().await.unwrap();
    trx.set(&other_key, &sealed);
    trx.commit().await.unwrap();
    assert_eq!(
        runtime.cancel_model(&other.run).await.unwrap_err(),
        RuntimeError::InvalidBinding
    );
    assert_eq!(
        runtime.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Released
    );
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_postings")
            .await,
        0
    );
    assert_zero_send(&fixture).await;
    fixture.finish().await;
    println!("native cancellation marker cuts: four bound fields and cross-run ciphertext swap reject substitution, exact replay retains same ciphertext PASS");
}

async fn both_fenced_orderings() {
    for close_first in [true, false] {
        let (fixture, run) = original().await;
        let runtime = fixture.world.runtime.clone();
        let step = runtime.verified_step(&run.run).await.unwrap();
        let original = binding(&runtime, &step);
        runtime.finance.reserve(&original).await.unwrap();
        let claim = runtime.finance.claim(&original).await.unwrap().unwrap();
        let guard = runtime.finance.guard(&original, &step).await.unwrap();
        if !close_first {
            let admission = runtime
                .execution
                .admit_once(&step, claim, &original, &runtime.custody, &runtime)
                .await
                .unwrap();
            guard.finish(admission).await.unwrap().consume().unwrap();
            runtime
                .execution
                .request_reply_cancel(&run.run, &runtime.custody, &runtime)
                .await
                .unwrap();
            let VerifiedStep::Native(native) = &step else {
                panic!()
            };
            let cleanup = native
                .cleanup_for_known_holder(&runtime.execution, &runtime.custody)
                .await
                .unwrap();
            assert!(runtime
                .execution
                .close_reply_before_dispatch(&cleanup, &runtime.custody, &runtime)
                .await
                .unwrap()
                .is_none());
            assert_eq!(
                fixture.world.balance().await.held_units,
                quote(&fixture, &run).await
            );
        } else {
            runtime
                .execution
                .request_reply_cancel(&run.run, &runtime.custody, &runtime)
                .await
                .unwrap();
            let VerifiedStep::Native(native) = &step else {
                panic!()
            };
            let cleanup = native
                .cleanup_for_known_holder(&runtime.execution, &runtime.custody)
                .await
                .unwrap();
            let proof = runtime
                .execution
                .close_reply_before_dispatch(&cleanup, &runtime.custody, &runtime)
                .await
                .unwrap()
                .unwrap();
            assert!(runtime
                .execution
                .admit_once(&step, claim, &original, &runtime.custody, &runtime)
                .await
                .is_err());
            guard.rollback().await.unwrap();
            runtime.finance.release(proof, &runtime).await.unwrap();
            assert_one_release(&fixture).await;
        }
        step.release(&runtime.execution).await.unwrap();
        assert_zero_send(&fixture).await;
        fixture.finish().await;
    }
    println!("native cancellation orderings: exact actual holder closure excludes later admission, admitted winner retains hold PASS");
}
