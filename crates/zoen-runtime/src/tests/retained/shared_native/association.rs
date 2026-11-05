//! Original retained result -> historical reply correspondence -> cold recovery.
use super::*;

async fn subjects(f: &Fixture) -> Result<usize, Box<dyn std::error::Error>> {
    Ok(f.world
        .runtime
        .runtime_continuity
        .as_ref()
        .ok_or("managed")?
        .lock()
        .await
        .anchor
        .reader()
        .subject_commitments()?
        .len())
}
pub(in crate::tests) async fn run() -> TestResult {
    for (fault, unknown) in [
        (0, false),
        (140, false),
        (141, false),
        (142, false),
        (143, false),
        (141, true),
    ] {
        let mut f = Fixture::with_shared_runtime().await?;
        let (attempt, digest, sealed, hold) = ack::captured(&mut f, unknown, false).await?;
        let captured =
            crate::capture_codec::open(&f.world.runtime.custody, &attempt, &digest, &sealed)?;
        let run = captured.binding.context.run_id.clone();
        let expected = f.world.http.body.lock().map_err(|_| "body")?["choices"][0]["message"]
            ["content"]
            .as_str()
            .ok_or("text")?
            .to_owned();
        let before = checkpoint(&f).await?;
        let count = subjects(&f).await?;
        let journals = f.world.count("SELECT count(*) FROM runtime_journals").await;
        let postings = f.world.count("SELECT count(*) FROM runtime_postings").await;
        assert!(
            native::testing::output::retained_result(&f.world.runtime, &run)
                .await
                .is_err()
        );
        assert!(f
            .world
            .runtime
            .associate_shared_capture(&attempt, &"f".repeat(64))
            .await
            .is_err());
        assert_eq!(checkpoint(&f).await?, before);
        f.world.runtime.fault.store(fault, Ordering::SeqCst);
        let result = f
            .world
            .runtime
            .associate_shared_capture(&attempt, &digest)
            .await;
        f.world.runtime.fault.store(0, Ordering::SeqCst);
        if fault == 0 {
            result?;
        } else {
            assert_eq!(result, Err(RuntimeError::Unavailable));
        }
        if (140..=142).contains(&fault) {
            let managed = f
                .world
                .runtime
                .runtime_continuity
                .as_ref()
                .ok_or("managed")?
                .lock()
                .await;
            let pending = managed.anchor.pending()?.ok_or("original association")?;
            f.world.runtime.test_association_intent(&pending)?;
            for variant in 0..3 {
                let mut changed = pending.clone();
                match variant {
                    0 => changed.effect_digest = "f".repeat(64),
                    1 => changed.subject_updates[0].next = "f".repeat(64),
                    _ => changed.retirement = true,
                }
                assert!(f.world.runtime.test_association_intent(&changed).is_err());
            }
        }
        reopen(&mut f).await?;
        assert_eq!(checkpoint(&f).await?.sequence, before.sequence + 1);
        assert_eq!(subjects(&f).await?, count);
        assert_eq!(
            native::testing::output::retained_result(&f.world.runtime, &run).await?,
            (digest.clone(), expected.clone())
        );
        let tip = checkpoint(&f).await?;
        f.world
            .runtime
            .associate_shared_capture(&attempt, &digest)
            .await?;
        assert_eq!(
            checkpoint(&f).await?,
            tip,
            "exact retry does not reseal or advance"
        );
        let key = f
            .world
            .runtime
            .execution
            .attempt(&attempt, "native-capture/1");
        let tx = f.world.runtime.execution.transaction().await?;
        let original = tx
            .get(&key, false)
            .await?
            .ok_or("actual association")?
            .to_vec();
        if fault == 0 {
            let subject = format!("native-capture/1/{attempt}");
            let plain = f
                .world
                .runtime
                .custody
                .open_bytes(&subject, None, &original)?;
            let metadata: serde_json::Value = serde_json::from_slice(&plain)?;
            let nonce = metadata["admitted_nonce"].as_str().ok_or("nonce")?;
            for field in ["binding", "original", "run", "evidence", "admitted_nonce"] {
                let mut changed = metadata.clone();
                changed[field] = serde_json::Value::String("f".repeat(64));
                let (_, bytes) = f
                    .world
                    .runtime
                    .custody
                    .seal_bytes(&subject, &serde_json::to_vec(&changed)?)?;
                assert!(
                    f.world
                        .runtime
                        .execution
                        .validate_capture_association_in(
                            &tx,
                            &captured.binding,
                            (&digest, nonce),
                            &bytes,
                            &f.world.runtime.custody,
                        )
                        .await
                        .is_err(),
                    "a valid seal does not excuse mismatched {field}"
                );
            }
        }
        tx.clear(&key);
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        assert!(
            f.world.runtime.reconcile(8).await.is_err(),
            "clean missing metadata must not be recreated"
        );
        let tx = f.world.runtime.execution.transaction().await?;
        assert!(tx.get(&key, false).await?.is_none());
        let mut corrupted = original.clone();
        *corrupted.first_mut().ok_or("metadata")? ^= 1;
        tx.set(&key, &corrupted);
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        assert!(f.world.runtime.check_shared_runtime().await.is_err());
        let tx = f.world.runtime.execution.transaction().await?;
        tx.set(&key, &original);
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        // ACKs after association must refine, not erase, its commitment.
        f.world.runtime.reconcile(8).await?;
        assert_eq!(subjects(&f).await?, count);
        let tip = checkpoint(&f).await?;
        assert_eq!(f.world.runtime.reconcile(8).await?.inspected, 0);
        assert_eq!(checkpoint(&f).await?, tip);
        assert_eq!(
            native::testing::output::retained_result(&f.world.runtime, &run).await?,
            (digest, expected)
        );
        assert_eq!(
            f.world.count("SELECT count(*) FROM runtime_journals").await,
            journals
        );
        assert_eq!(
            f.world.count("SELECT count(*) FROM runtime_postings").await,
            postings
        );
        assert_eq!(
            f.world.balance().await.held_units,
            if unknown { hold } else { 0 }
        );
        assert_eq!(
            f.world.balance().await.spent_units,
            if unknown { 0 } else { 6 }
        );
        assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
        assert!(f.world.runtime.production_output_scope().is_err());
        assert_eq!(
            f.world.runtime.continuity(),
            Err(RuntimeError::RestoreUnreconciled)
        );
        reopen(&mut f).await?;
        assert_eq!(checkpoint(&f).await?, tip);
        f.finish().await;
        println!("shared association fault={fault} unknown={unknown}: exact original output reader, cold recovery, no new subject/charge/send, clean missing/corrupt refusal and stable ACK composition PASS");
    }
    // Public reconciliation discovers correspondence from existing captures,
    // respects its budget and does not depend on a new durable queue.
    let mut f = Fixture::with_shared_runtime().await?;
    let (attempt, digest, _, _) = ack::captured(&mut f, false, false).await?;
    let before = checkpoint(&f).await?;
    assert!(matches!(
        f.world.runtime.reconcile(0).await,
        Err(RuntimeError::InvalidBinding)
    ));
    assert_eq!(checkpoint(&f).await?, before);
    for _ in 0..3 {
        let report = f.world.runtime.reconcile(1).await?;
        assert_eq!(report.inspected, 1);
        assert_eq!(report.transferred, 1);
    }
    assert_eq!(f.world.runtime.reconcile(1).await?.inspected, 0);
    let tip = checkpoint(&f).await?;
    f.world
        .runtime
        .associate_shared_capture(&attempt, &digest)
        .await?;
    assert_eq!(checkpoint(&f).await?, tip);
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    f.finish().await;
    Ok(())
}
