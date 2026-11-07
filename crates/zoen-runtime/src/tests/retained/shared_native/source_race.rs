//! New arrivals after signing require reconciliation, never another inference.
use super::*;
use native::testing::output as proof;

pub(in crate::tests) async fn run() -> TestResult {
    let (mut f, relay, run) = Box::pin(activation::ready()).await?;
    let before = f.head.clone().ok_or("source head")?;
    let runtime = f.world.runtime.clone();
    runtime.fault.store(189, Ordering::SeqCst);
    let change = async {
        tokio::time::timeout(
            Duration::from_secs(10),
            runtime.native_cut_entered.notified(),
        )
        .await?;
        let changed = f
            .message("Another message arrived while the reply was being signed.")
            .await;
        runtime.fault.store(0, Ordering::SeqCst);
        runtime.native_cut_resume.notify_one();
        changed
    };
    let (stale, changed) = tokio::join!(Box::pin(runtime.prepare_reply_output(&run.run)), change);
    changed?;
    assert_eq!(stale, Err(RuntimeError::NativeSourceAdvanced));
    assert_eq!(
        native::testing::test_source_guard(&runtime, &f.space, before.clone(), before.clone())
            .await,
        Err(RuntimeError::NativeSourceAdvanced)
    );
    let mut invalid = before.clone();
    invalid.hash = "00".repeat(32);
    assert_eq!(
        native::testing::test_source_guard(&runtime, &f.space, invalid.clone(), before.clone())
            .await,
        Err(RuntimeError::InvalidBinding)
    );
    assert_eq!(
        native::testing::test_source_guard(&runtime, &f.space, before.clone(), invalid).await,
        Err(RuntimeError::InvalidBinding)
    );
    let mut missing = before.clone();
    missing.seq = 0;
    assert_eq!(
        native::testing::test_source_guard(&runtime, &f.space, missing, before.clone()).await,
        Err(RuntimeError::Denied)
    );
    let mut future = before.clone();
    future.seq = future.seq.checked_add(100).ok_or("sequence")?;
    assert_eq!(
        native::testing::test_source_guard(&runtime, &f.space, future, before).await,
        Err(RuntimeError::Denied)
    );
    // The signing intent is retained. Recovery must reconcile its exact bytes,
    // not expire the lease, remove a stage or generate another model result.
    let original = runtime
        .runtime_continuity
        .as_ref()
        .ok_or("managed")?
        .lock()
        .await
        .test_pending_publication(&runtime)?;
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    drop(runtime);
    reopen(&mut f).await?;
    activation::restore_route(&mut f, &relay).await?;
    let inspection = Box::pin(f.world.runtime.prepare_reply_output(&run.run)).await?;
    let envelope = proof::retained_publication(&f.world.runtime, &run.run).await?;
    assert_eq!(
        relay_admission::managed_images::publish_digest(&f.world.config_namespace, &envelope)?,
        original.0
    );
    let tip = checkpoint(&f).await?;
    assert_eq!(
        Box::pin(f.world.runtime.prepare_reply_output(&run.run)).await?,
        inspection
    );
    assert_eq!(checkpoint(&f).await?, tip);
    assert_eq!(
        proof::retained_publication(&f.world.runtime, &run.run).await?,
        envelope
    );
    let mut accepted = None;
    for _ in 0..3 {
        let progress = Box::pin(
            f.world
                .runtime
                .deliver_reply_output(&run.run, relay.target(&f, relay.address)),
        )
        .await?;
        if matches!(progress, ReplyReceiptProgress::Accepted { .. }) {
            accepted = Some(progress);
            break;
        }
    }
    let accepted = accepted.ok_or("original response not accepted")?;
    assert_eq!(
        Box::pin(
            f.world
                .runtime
                .deliver_reply_output(&run.run, relay.target(&f, relay.address))
        )
        .await?,
        accepted
    );
    assert_eq!(
        f.log
            .read(&f.space, 1, 256)
            .await?
            .iter()
            .filter(|entry| entry.env == envelope)
            .count(),
        1
    );
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    assert_eq!(f.world.balance().await.held_units, 0);
    assert_eq!(
        f.world.count("SELECT count(*) FROM runtime_claims").await,
        1
    );
    relay.stop().await;
    f.finish().await;
    println!("source arrival after signing: exact cold publication recovery, one inference/claim/charge PASS");
    Ok(())
}
