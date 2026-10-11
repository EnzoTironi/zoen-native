//! Private fresh-namespace output transactions, never production activation.
use super::*;
use native::testing::output as proof;

async fn original() -> (Fixture, ReplyRun) {
    let mut fixture = Fixture::with_output().await;
    fixture.grant("output-owner-trust").await;
    fixture.message(ORIGINAL).await;
    let run = fixture.sync(false).await.unwrap().runs.remove(0);
    (fixture, run)
}

async fn revoke_device(fixture: &Fixture) {
    let mut tx = fixture.world.runtime.finance.pool.begin().await.unwrap();
    assert!(zoen_relay::db::revoke_device(
        &mut tx,
        &fixture.agent.id,
        fixture.world.agents[0].device.as_deref().unwrap()
    )
    .await
    .unwrap());
    tx.commit().await.unwrap();
}

async fn assert_one_bill(fixture: &Fixture) {
    assert_eq!(fixture.world.http.sends.load(Ordering::SeqCst), 1);
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
            .count("SELECT count(*) FROM runtime_dispositions WHERE kind='settle'")
            .await,
        1
    );
    assert_eq!(
        fixture.world.balance().await,
        PeriodBalance {
            held_units: 0,
            spent_units: 6
        }
    );
}

async fn cold(fixture: &mut Fixture) {
    let mut reopened = fixture.world.reopen().await;
    let custody = fixture.restore_custody();
    let capability = Arc::get_mut(&mut fixture.world.runtime)
        .unwrap()
        .output_fixture
        .take()
        .unwrap();
    let runtime = Arc::get_mut(&mut reopened).unwrap();
    runtime.native = Some(custody);
    // Move the original fixture capability. No store read reconstructs it.
    runtime.output_fixture = Some(capability);
    fixture.world.runtime.finance.pool.close().await;
    fixture.world.runtime = reopened;
}

async fn accepted_across_prefix_revocation_and_cold_replay() {
    let (mut fixture, run) = original().await;
    let runtime = &fixture.world.runtime;
    assert_eq!(
        runtime.prepare_reply_output(&run.run).await.unwrap_err(),
        RuntimeError::CoreAuthorityUnavailable
    );
    assert_eq!(
        runtime.reconcile_reply_receipt(&run.run).await.unwrap_err(),
        RuntimeError::CoreAuthorityUnavailable
    );
    runtime.run_model(&run.run).await.unwrap();
    let original = proof::fingerprints(runtime, &run.run).await;
    runtime.fault.store(43, Ordering::SeqCst);
    assert_eq!(
        proof::prepare(runtime, &run.run).await.unwrap_err(),
        RuntimeError::Unavailable
    );
    runtime.fault.store(0, Ordering::SeqCst);
    let first = proof::prepare(runtime, &run.run).await.unwrap();
    let env = proof::publication(runtime, &run.run).await;
    assert_eq!(first.client_id, env.client_id());
    proof::metadata_bounds(runtime, &run.run).await;
    let fingerprints = proof::fingerprints(runtime, &run.run).await;
    assert_eq!(
        (&fingerprints.0, &fingerprints.1, &fingerprints.2),
        (&original.0, &original.1, &original.2)
    );
    assert_eq!(
        proof::receive(runtime, &run.run).await.unwrap(),
        ReplyReceiptProgress::Waiting
    );
    cold(&mut fixture).await;
    assert_eq!(
        proof::prepare(&fixture.world.runtime, &run.run)
            .await
            .unwrap(),
        first
    );
    for _ in 0..65 {
        fixture
            .clear(EventBody::DeviceJoining {
                device: fixture.world.owner.device.clone().unwrap(),
            })
            .await;
    }
    let accepted = fixture.append(env).await;
    fixture
        .encrypted(EventBody::GrantRevoked {
            grant: "output-owner-trust".into(),
        })
        .await;
    revoke_device(&fixture).await;
    let ReplyReceiptProgress::CatchingUp {
        through,
        generation,
    } = proof::receive(&fixture.world.runtime, &run.run)
        .await
        .unwrap()
    else {
        panic!("target is beyond first64 actual positions");
    };
    assert!(through.seq < accepted.seq && generation > first.generation);
    assert_eq!(
        proof::fingerprints(&fixture.world.runtime, &run.run).await,
        fingerprints
    );
    cold(&mut fixture).await;
    fixture.world.runtime.fault.store(49, Ordering::SeqCst);
    assert_eq!(
        proof::receive(&fixture.world.runtime, &run.run)
            .await
            .unwrap_err(),
        RuntimeError::Unavailable
    );
    fixture.world.runtime.fault.store(0, Ordering::SeqCst);
    let retained = proof::receive(&fixture.world.runtime, &run.run)
        .await
        .unwrap();
    let ReplyReceiptProgress::Accepted {
        accepted: seen,
        generation,
    } = &retained
    else {
        panic!("actual target must produce AcceptedReply");
    };
    assert_eq!(
        seen,
        &Seen {
            seq: accepted.seq,
            hash: accepted.hash.clone()
        }
    );
    cold(&mut fixture).await;
    assert_eq!(
        proof::receive(&fixture.world.runtime, &run.run)
            .await
            .unwrap(),
        retained
    );
    assert_eq!(
        native::testing::generation(
            &fixture.world.runtime,
            &fixture.agent.id,
            fixture.world.agents[0].device.as_deref().unwrap()
        )
        .await,
        *generation
    );
    assert_eq!(
        proof::fingerprints(&fixture.world.runtime, &run.run).await,
        fingerprints
    );
    proof::assert_pinned(&fixture.world.runtime, &run.run).await;
    assert_one_bill(&fixture).await;
    assert_eq!(
        fixture.world.runtime.pending_reply_runs(64).await.unwrap(),
        vec![run]
    );
    fixture.finish().await;
    println!("native output journey: actual paid capture, uncertain preparation ACK, original CID/Publish, cold64-position prefix, grant/device revocation, uncertain receipt ACK and inert cold replay; one provider request/bill, unchanged originals and retained wake/refs PASS");
}

async fn association_failure_does_not_block_incurred_evidence() {
    let (fixture, run) = original().await;
    fixture.world.runtime.fault.store(40, Ordering::SeqCst);
    fixture.world.runtime.run_model(&run.run).await.unwrap();
    fixture.world.runtime.fault.store(0, Ordering::SeqCst);
    assert!(proof::association(&fixture.world.runtime, &run.attempt)
        .await
        .is_none());
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_evidence")
            .await,
        1
    );
    assert_eq!(
        fixture
            .world
            .runtime
            .execution
            .pending_evidence(64)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        proof::prepare(&fixture.world.runtime, &run.run)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert!(fixture.world.runtime.run_model(&run.run).await.is_err());
    assert_one_bill(&fixture).await;
    fixture.finish().await;
    println!("native output association cut: producer refusal occurs first, independent SQL and FDB incurred evidence remain; unindexed capture cannot be adopted or trigger a second provider request PASS");
}

async fn exact_output_and_missing_usage() {
    for case in ["missing-usage", "partial", "tool", "malformed", "empty"] {
        let (fixture, run) = original().await;
        {
            let mut response = fixture.world.http.body.lock().unwrap();
            match case {
                "missing-usage" => {
                    response.as_object_mut().unwrap().remove("usage");
                }
                "partial" => response["choices"][0]["finish_reason"] = "length".into(),
                "tool" => {
                    response["choices"][0]["message"] = serde_json::json!({"role":"assistant","tool_calls":[{"id":"proposal","type":"function","function":{"name":"unallowed","arguments":"{}"}}]})
                }
                "malformed" => {
                    response.as_object_mut().unwrap().remove("choices");
                }
                "empty" => response["choices"][0]["message"]["content"] = "   ".into(),
                _ => unreachable!(),
            }
        }
        fixture.world.runtime.run_model(&run.run).await.unwrap();
        assert!(proof::association(&fixture.world.runtime, &run.attempt)
            .await
            .is_some());
        let result = proof::prepare(&fixture.world.runtime, &run.run).await;
        if case == "missing-usage" {
            result.unwrap();
            assert_eq!(fixture.world.balance().await.held_units, 18);
            assert_eq!(fixture.world.balance().await.spent_units, 0);
        } else {
            assert!(result.is_err(), "{case}");
        }
        assert_eq!(fixture.world.http.sends.load(Ordering::SeqCst), 1);
        assert_eq!(
            fixture
                .world
                .count("SELECT count(*) FROM runtime_evidence")
                .await,
            1
        );
        fixture.finish().await;
    }
    println!("native output shape journey: exact supported text with missing usage retains hold and can prepare separately; partial/tool/malformed/empty replies retain incurred evidence and create no publication PASS");
}

async fn current_permission_and_source_refuse_new_output() {
    for case in ["source", "grant", "directory", "cancel"] {
        let (mut fixture, run) = original().await;
        fixture.world.runtime.run_model(&run.run).await.unwrap();
        let original = proof::fingerprints(&fixture.world.runtime, &run.run).await;
        match case {
            "source" => {
                fixture.message(LATER).await;
            }
            "grant" => {
                fixture
                    .encrypted(EventBody::GrantRevoked {
                        grant: "output-owner-trust".into(),
                    })
                    .await;
                fixture.sync(false).await.unwrap();
            }
            "directory" => revoke_device(&fixture).await,
            "cancel" => {
                fixture.world.runtime.cancel_model(&run.run).await.unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            proof::prepare(&fixture.world.runtime, &run.run)
                .await
                .is_err(),
            "{case}"
        );
        assert_eq!(
            proof::fingerprints(&fixture.world.runtime, &run.run).await,
            original
        );
        assert_one_bill(&fixture).await;
        fixture.finish().await;
    }
    println!("native output permission cuts: actual source drift, native grant revoke, SQL device revoke and cancellation refuse preparation while preserving original paid capture/bill PASS");
}

async fn finite_admission_and_failed_activation() {
    for cut in [42, 45, 46, 50] {
        let mut fixture = Fixture::with_output().await;
        if cut == 50 {
            let expiry = fixture.world.runtime.finance.reply_clock().await.unwrap() + 1700;
            fixture
                .grant_until("output-owner-trust", Some(expiry))
                .await;
        } else {
            fixture.grant("output-owner-trust").await;
        }
        fixture.message(ORIGINAL).await;
        let run = fixture.sync(false).await.unwrap().runs.remove(0);
        fixture.world.runtime.run_model(&run.run).await.unwrap();
        let original = proof::fingerprints(&fixture.world.runtime, &run.run).await;
        let actor = fixture.world.agents[0].clone();
        let generation = native::testing::generation(
            &fixture.world.runtime,
            &actor.identity,
            actor.device.as_deref().unwrap(),
        )
        .await;
        fixture.world.runtime.fault.store(cut, Ordering::SeqCst);
        assert!(
            proof::prepare(&fixture.world.runtime, &run.run)
                .await
                .is_err(),
            "{cut}"
        );
        fixture.world.runtime.fault.store(0, Ordering::SeqCst);
        assert_eq!(
            proof::fingerprints(&fixture.world.runtime, &run.run).await,
            original
        );
        assert_eq!(
            native::testing::generation(
                &fixture.world.runtime,
                &actor.identity,
                actor.device.as_deref().unwrap()
            )
            .await,
            generation
        );
        assert_one_bill(&fixture).await;
        fixture.finish().await;
    }
    println!("native output admission cuts: uncommitted activation, suppressed real SQL ACK, late ACK and shorter actual native expiry yield no publication or active replacement image; incurred bill survives PASS");
}

async fn source_changes_before_final_output_and_receipt() {
    for receipt in [false, true] {
        let (mut fixture, run) = original().await;
        fixture.world.runtime.run_model(&run.run).await.unwrap();
        if receipt {
            proof::prepare(&fixture.world.runtime, &run.run)
                .await
                .unwrap();
            let env = proof::publication(&fixture.world.runtime, &run.run).await;
            fixture.append(env).await;
        }
        let before = proof::fingerprints(&fixture.world.runtime, &run.run).await;
        let runtime = fixture.world.runtime.clone();
        runtime
            .fault
            .store(if receipt { 47 } else { 41 }, Ordering::SeqCst);
        let actor = fixture.world.agents[0].clone();
        let generation = native::testing::generation(
            &runtime,
            &actor.identity,
            actor.device.as_deref().unwrap(),
        )
        .await;
        let run_id = run.run.clone();
        let worker = runtime.clone();
        let paused = tokio::spawn(async move {
            if receipt {
                proof::receive(&worker, &run_id).await.map(|_| ())
            } else {
                proof::prepare(&worker, &run_id).await.map(|_| ())
            }
        });
        tokio::time::timeout(
            Duration::from_secs(3),
            runtime.native_cut_entered.notified(),
        )
        .await
        .unwrap();
        fixture
            .clear(EventBody::DeviceJoining {
                device: fixture.world.owner.device.clone().unwrap(),
            })
            .await;
        runtime.native_cut_resume.notify_one();
        assert_eq!(paused.await.unwrap().unwrap_err(), RuntimeError::Denied);
        runtime.fault.store(0, Ordering::SeqCst);
        assert_eq!(proof::fingerprints(&runtime, &run.run).await, before);
        assert_eq!(
            native::testing::generation(
                &runtime,
                &actor.identity,
                actor.device.as_deref().unwrap()
            )
            .await,
            generation
        );
        assert_one_bill(&fixture).await;
        drop(runtime);
        fixture.finish().await;
    }
    println!("native output/source cuts: actual relay append before final preparation or historical receipt transaction refuses stale activation with original image/capture and one bill preserved PASS");
}

pub(in crate::tests) async fn run() {
    Box::pin(capture_substitutions_refuse()).await;
    Box::pin(late_paid_capture_remains_independent()).await;
    Box::pin(association_failure_does_not_block_incurred_evidence()).await;
    Box::pin(accepted_across_prefix_revocation_and_cold_replay()).await;
    Box::pin(exact_output_and_missing_usage()).await;
    Box::pin(current_permission_and_source_refuse_new_output()).await;
    Box::pin(finite_admission_and_failed_activation()).await;
    Box::pin(source_changes_before_final_output_and_receipt()).await;
    Box::pin(paired_takeover_and_root_replacement()).await;
    Box::pin(historical_replay_refuses_changed_source_position()).await;
}

async fn paired_takeover_and_root_replacement() {
    for receipt in [false, true] {
        let (mut fixture, run) = original().await;
        fixture.world.runtime.run_model(&run.run).await.unwrap();
        if receipt {
            proof::prepare(&fixture.world.runtime, &run.run)
                .await
                .unwrap();
            let env = proof::publication(&fixture.world.runtime, &run.run).await;
            fixture.append(env).await;
        }
        let before = proof::fingerprints(&fixture.world.runtime, &run.run).await;
        let runtime = fixture.world.runtime.clone();
        runtime
            .fault
            .store(if receipt { 47 } else { 41 }, Ordering::SeqCst);
        let worker = runtime.clone();
        let run_id = run.run.clone();
        let paused = tokio::spawn(async move {
            if receipt {
                proof::receive(&worker, &run_id).await.map(|_| ())
            } else {
                proof::prepare(&worker, &run_id).await.map(|_| ())
            }
        });
        tokio::time::timeout(
            Duration::from_secs(3),
            runtime.native_cut_entered.notified(),
        )
        .await
        .unwrap();
        proof::takeover(&runtime, &run.run).await;
        runtime.native_cut_resume.notify_one();
        assert_eq!(paused.await.unwrap().unwrap_err(), RuntimeError::Denied);
        runtime.fault.store(0, Ordering::SeqCst);
        assert_eq!(proof::fingerprints(&runtime, &run.run).await, before);
        assert_one_bill(&fixture).await;
        drop(runtime);
        fixture.finish().await;
    }
    let (fixture, run) = original().await;
    fixture.world.runtime.run_model(&run.run).await.unwrap();
    let before = proof::fingerprints(&fixture.world.runtime, &run.run).await;
    fixture.world.runtime.fault.store(54, Ordering::SeqCst);
    assert_eq!(
        proof::prepare(&fixture.world.runtime, &run.run)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    fixture.world.runtime.fault.store(0, Ordering::SeqCst);
    assert_eq!(
        proof::fingerprints(&fixture.world.runtime, &run.run).await,
        before
    );
    assert_one_bill(&fixture).await;
    fixture.finish().await;
    println!("native output root/pair cuts: known maintenance repack invalidates loaded root; explicit real FDB expiry followed by genuine paired reacquisition refuses stale preparation and receipt; no fabricated fence or relaxed lease window PASS");
}

async fn historical_replay_refuses_changed_source_position() {
    let (mut fixture, run) = original().await;
    fixture.world.runtime.run_model(&run.run).await.unwrap();
    proof::prepare(&fixture.world.runtime, &run.run)
        .await
        .unwrap();
    let env = proof::publication(&fixture.world.runtime, &run.run).await;
    let accepted = fixture.append(env).await;
    let retained = proof::receive(&fixture.world.runtime, &run.run)
        .await
        .unwrap();
    let before = proof::fingerprints(&fixture.world.runtime, &run.run).await;
    let source = foundationdb::tuple::Subspace::all()
        .subspace(&("zoen", fixture.world.relay_cell.as_deref().unwrap()));
    let key = source.pack(&("s", fixture.space.as_str(), "log", accepted.seq as i64));
    for changed in [true, false] {
        let trx = fixture.world.runtime.execution.transaction().await.unwrap();
        if changed {
            let mut pruned = accepted.clone();
            pruned.env = pruned.env.pruned().unwrap();
            trx.set(&key, &pruned.encode());
        } else {
            trx.clear(&key);
        }
        trx.commit().await.unwrap();
        assert_eq!(
            proof::receive(&fixture.world.runtime, &run.run)
                .await
                .unwrap_err(),
            RuntimeError::Denied
        );
        assert_eq!(
            proof::fingerprints(&fixture.world.runtime, &run.run).await,
            before
        );
        let trx = fixture.world.runtime.execution.transaction().await.unwrap();
        trx.set(&key, &accepted.encode());
        trx.commit().await.unwrap();
    }
    assert_eq!(
        proof::receive(&fixture.world.runtime, &run.run)
            .await
            .unwrap(),
        retained
    );
    assert_one_bill(&fixture).await;
    fixture.finish().await;
    println!("native receipt source-position cuts: existing retained receipt cannot replace missing or pruned actual accepted source bytes; exact restored source allows inert replay with no generation/bill change PASS");
}

async fn late_paid_capture_remains_independent() {
    let (mut fixture, run) = original().await;
    fixture.world.http.paused.store(true, Ordering::SeqCst);
    let runtime = fixture.world.runtime.clone();
    let worker = runtime.clone();
    let run_id = run.run.clone();
    let paid = tokio::spawn(async move { worker.run_model(&run_id).await });
    fixture.world.http.wait().await;
    assert_eq!(
        runtime.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Requested
    );
    revoke_device(&fixture).await;
    fixture
        .encrypted(EventBody::GrantRevoked {
            grant: "output-owner-trust".into(),
        })
        .await;
    proof::takeover(&runtime, &run.run).await;
    fixture.world.http.release.notify_one();
    assert_eq!(
        paid.await.unwrap().unwrap().financial,
        FinancialState::Settled { units: 6 }
    );
    assert!(proof::association(&runtime, &run.attempt).await.is_some());
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_evidence")
            .await,
        1
    );
    assert!(proof::prepare(&runtime, &run.run).await.is_err());
    assert_one_bill(&fixture).await;
    drop(runtime);
    fixture.finish().await;
    println!("native capture late-retention cut: actual admitted HTTP finishes after cancellation, SQL/native revocation and real pair takeover; original nonce/capture association and bill survive without authorizing output PASS");
}

async fn capture_substitutions_refuse() {
    let (fixture, run) = original().await;
    let step = fixture.world.runtime.verified_step(&run.run).await.unwrap();
    fixture
        .world
        .runtime
        .complete_verified(&step)
        .await
        .unwrap();
    step.release(&fixture.world.runtime.execution)
        .await
        .unwrap();
    let fixture_step = fixture.world.step(1).await;
    proof::substitutions(&fixture.world.runtime, &step, &fixture_step).await;
    assert_one_bill(&fixture).await;
    fixture.finish().await;
    println!("native capture identity cuts: actual binding/request/attempt or another capture digest and financial FixtureStep refuse association; exact original converges without replacing index or another bill PASS");
}
