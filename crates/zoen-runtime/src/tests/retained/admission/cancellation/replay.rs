async fn known_ack_losses() -> std::result::Result<(), Box<dyn std::error::Error>> {
    for fault in [26, 27, 28] {
        let (fixture, run) = original().await?;
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
    Ok(())
}

async fn exact_marker_replay() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut fixture, run) = original().await?;
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
            new_id("wrong")?
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
    fixture.message(LATER).await?;
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
    Ok(())
}

async fn both_fenced_orderings() -> std::result::Result<(), Box<dyn std::error::Error>> {
    for close_first in [true, false] {
        let (fixture, run) = original().await?;
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
    Ok(())
}
