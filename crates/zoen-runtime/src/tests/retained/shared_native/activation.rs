//! Actual native signing/activation under the shared checkpoint. The paid
//! transport is the explicit loopback fixture, never a productive gate override.
use super::*;
use crate::tests::retained::image_tasks::delivery::relay::Relay;
use native::testing::output as proof;
async fn ready() -> Result<(Fixture, Relay, crate::ReplyRun), Box<dyn std::error::Error>> {
    let mut f = Fixture::with_shared_runtime().await?;
    f.grant("shared-activation-owner").await?;
    f.message(ORIGINAL).await?;
    let run = f.sync(false).await?.runs.into_iter().next().ok_or("run")?;
    native::testing::reserve_retained_reply(&f.world.runtime, &run.run).await?;
    let claim = native::testing::claim_retained_reply(&f.world.runtime, &run.run)
        .await?
        .ok_or("claim")?;
    let capture =
        native::testing::complete_retained_for_capture(&f.world.runtime, &run.run, claim).await?;
    let (digest, sealed) = crate::capture_codec::seal(&f.world.runtime.custody, &capture)?;
    f.world
        .runtime
        .capture_shared(&run.attempt, &digest, &sealed)
        .await?;
    f.world
        .runtime
        .associate_shared_capture(&run.attempt, &digest)
        .await?;
    output_route::configured(&f, true).await?;
    let relay = Relay::start(&f).await?;
    let target = relay.target(&f, relay.address);
    let runtime = Arc::try_unwrap(f.world.runtime).map_err(|_| "exclusive runtime")?;
    f.world.runtime = Arc::new(runtime.qualify_shared_reply_output(target).await?);
    Ok((f, relay, run))
}
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
async fn restore_route(f: &mut Fixture, relay: &Relay) -> TestResult {
    let target = relay.target(f, relay.address);
    // The normal cold constructor deliberately returns no live route.
    assert!(f.world.runtime.production_output_scope().is_err());
    let scope = native::testing::qualify_shared_output(&f.world.runtime, target).await?;
    Arc::get_mut(&mut f.world.runtime)
        .ok_or("exclusive runtime")?
        .reply_output = Some(scope);
    Ok(())
}
pub(in crate::tests) async fn run() -> TestResult {
    for fault in [0, 90, 91, 92, 45, 46, 150] {
        let (mut f, relay, run) = ready().await?;
        eprintln!("shared activation case={fault} phase=prepare");
        let before = checkpoint(&f).await?;
        let count = subjects(&f).await?;
        let journals = f.world.count("SELECT count(*) FROM runtime_journals").await;
        let principal = &f.world.agents[0];
        let generation = native::testing::generation(
            &f.world.runtime,
            &principal.identity,
            principal.device.as_deref().ok_or("device")?,
        )
        .await;
        f.world.runtime.fault.store(fault, Ordering::SeqCst);
        let prepared = f.world.runtime.prepare_reply_output(&run.run).await;
        f.world.runtime.fault.store(0, Ordering::SeqCst);
        if fault == 0 {
            assert!(prepared.is_ok(), "{prepared:?}");
        } else if fault == 46 {
            assert_eq!(prepared, Err(RuntimeError::Denied));
        } else {
            assert_eq!(prepared, Err(RuntimeError::Unavailable));
        }
        let (effect, original) = if fault == 0 || fault == 150 {
            let env = proof::retained_publication(&f.world.runtime, &run.run).await?;
            let tx = f.world.runtime.execution.transaction().await?;
            let bytes = tx
                .get(
                    &f.world
                        .runtime
                        .execution
                        .run(&run.run, "native-publication/1"),
                    false,
                )
                .await?
                .ok_or("publication")?;
            (
                relay_admission::managed_images::publish_digest(&f.world.config_namespace, &env)?,
                crate::hash(&bytes),
            )
        } else {
            f.world
                .runtime
                .runtime_continuity
                .as_ref()
                .ok_or("managed")?
                .lock()
                .await
                .test_pending_publication(&f.world.runtime)?
        };
        // An unrelated real source append cannot cause a second signature.
        if fault == 91 {
            f.message("Source advanced after original signing.").await?;
        }
        eprintln!("shared activation case={fault} phase=cold-open");
        reopen(&mut f).await?;
        assert_eq!(checkpoint(&f).await?.sequence, before.sequence + 1);
        assert_eq!(
            subjects(&f).await?,
            count,
            "root and capture commitments are refined, not duplicated"
        );
        eprintln!("shared activation case={fault} phase=route");
        restore_route(&mut f, &relay).await?;
        let inspection = f.world.runtime.prepare_reply_output(&run.run).await?;
        assert_eq!(inspection.generation, generation + 1);
        let envelope = proof::retained_publication(&f.world.runtime, &run.run).await?;
        assert_eq!(
            relay_admission::managed_images::publish_digest(&f.world.config_namespace, &envelope)?,
            effect
        );
        let key = f
            .world
            .runtime
            .execution
            .run(&run.run, "native-publication/1");
        let tx = f.world.runtime.execution.transaction().await?;
        let bytes = tx.get(&key, false).await?.ok_or("publication")?.to_vec();
        assert_eq!(
            crate::hash(&bytes),
            original,
            "recovery must not reseal publication metadata"
        );
        let cp = proof::checkpoint(&f.world.runtime, &run.run)
            .await?
            .ok_or("shared publication checkpoint")?;
        let tip = checkpoint(&f).await?;
        assert_eq!(cp, tip);
        assert_eq!(
            f.world.runtime.prepare_reply_output(&run.run).await?,
            inspection
        );
        assert_eq!(checkpoint(&f).await?, tip);
        // Clean deletion refuses instead of generating another response.
        tx.clear(&key);
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        assert!(f.world.runtime.check_shared_runtime().await.is_err());
        let tx = f.world.runtime.execution.transaction().await?;
        assert!(tx.get(&key, false).await?.is_none());
        tx.set(&key, &bytes);
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        let history = crate::continuity::history(&f.world.runtime.execution, cp.sequence);
        let tx = f.world.runtime.execution.transaction().await?;
        let original_history = tx
            .get(&history, false)
            .await?
            .ok_or("actual history")?
            .to_vec();
        tx.clear(&history);
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        assert!(f.world.runtime.check_shared_runtime().await.is_err());
        let tx = f.world.runtime.execution.transaction().await?;
        tx.set(&history, &original_history);
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        f.world.runtime.check_shared_runtime().await?;
        assert_eq!(
            f.world.runtime.prepare_reply_output(&run.run).await?,
            inspection
        );
        // Completion can precede outbox ACKs. Their later checkpoints must
        // preserve this historical publication and its exact native bytes.
        let acknowledgements = f.world.runtime.reconcile(8).await?;
        assert_eq!(acknowledgements.transferred, 2);
        assert_eq!(subjects(&f).await?, count);
        assert!(checkpoint(&f).await?.sequence > cp.sequence);
        assert_eq!(
            proof::checkpoint(&f.world.runtime, &run.run).await?,
            Some(cp.clone())
        );
        assert_eq!(
            f.world.runtime.prepare_reply_output(&run.run).await?,
            inspection
        );

        assert_eq!(
            proof::retained_publication(&f.world.runtime, &run.run).await?,
            envelope
        );
        assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
        assert_eq!(f.world.balance().await.spent_units, 6);
        assert_eq!(f.world.balance().await.held_units, 0);
        assert_eq!(
            f.world.count("SELECT count(*) FROM runtime_journals").await,
            journals
        );
        assert_eq!(
            f.world.count("SELECT count(*) FROM runtime_claims").await,
            1
        );
        assert_eq!(
            f.world.runtime.continuity(),
            Err(RuntimeError::RestoreUnreconciled)
        );
        assert_eq!(
            f.world
                .runtime
                .deliver_reply_output(&run.run, relay.target(&f, relay.address))
                .await,
            Err(RuntimeError::RestoreUnreconciled)
        );
        relay.stop().await;
        f.finish().await;
        println!("shared reply activation fault={fault}: actual native original, one committed generation/checkpoint, cold exact ciphertext/wire hash, no new subject/claim/provider/charge, clean missing metadata/history refusal PASS");
    }
    denied_directory().await?;
    Ok(())
}
async fn denied_directory() -> TestResult {
    for prepared in [false, true] {
        let (mut f, relay, run) = ready().await?;
        let before = checkpoint(&f).await?;
        if prepared {
            f.world.runtime.fault.store(90, Ordering::SeqCst);
            assert_eq!(
                f.world.runtime.prepare_reply_output(&run.run).await,
                Err(RuntimeError::Unavailable)
            );
            f.world.runtime.fault.store(0, Ordering::SeqCst);
        }
        sqlx::query("UPDATE devices SET revoked_at=clock_timestamp() WHERE device=$1")
            .bind(f.world.agents[0].device.as_deref().ok_or("device")?)
            .execute(&f.world.runtime.finance.pool)
            .await?;
        if prepared {
            assert!(
                reopen(&mut f).await.is_err(),
                "a pending original is not current directory authority"
            );
        } else {
            assert_eq!(
                f.world.runtime.prepare_reply_output(&run.run).await,
                Err(RuntimeError::Denied)
            );
            assert_eq!(checkpoint(&f).await?, before);
        }
        let tx = f.world.runtime.execution.transaction().await?;
        assert!(tx
            .get(
                &f.world
                    .runtime
                    .execution
                    .run(&run.run, "native-publication/1"),
                false
            )
            .await?
            .is_none());
        assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
        assert_eq!(f.world.balance().await.spent_units, 6);
        relay.stop().await;
        f.finish().await;
    }
    Ok(())
}
