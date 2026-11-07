//! A relay head can advance while discovery seals its next native image.
use super::*;

pub(in crate::tests) async fn run() -> TestResult {
    for cut in [187, 188] {
        Box::pin(race(cut)).await?;
    }
    Ok(())
}
async fn race(cut: u8) -> TestResult {
    let mut f = Fixture::with_shared_runtime().await?;
    f.grant("discovery-race-owner").await?;
    f.message(ORIGINAL).await?;
    let before = checkpoint(&f).await?;
    let runtime = f.world.runtime.clone();
    let agent = f.world.agents[0].identity.clone();
    let device = f.world.agents[0].device.clone().ok_or("device")?;
    let space = f.space.clone();
    runtime.fault.store(cut, Ordering::SeqCst);
    let change = async {
        tokio::time::timeout(
            Duration::from_secs(10),
            runtime.native_cut_entered.notified(),
        )
        .await?;
        let changed = f
            .clear(EventBody::DeviceJoining {
                device: f.world.owner.device.clone().ok_or("owner device")?,
            })
            .await;
        runtime.fault.store(0, Ordering::SeqCst);
        runtime.native_cut_resume.notify_one();
        changed
    };
    let (stale, changed) = tokio::join!(
        Box::pin(runtime.sync_reply_runs(&agent, &device, &space)),
        change
    );
    changed?;
    assert!(
        matches!(&stale, Err(RuntimeError::NativeSourceAdvanced)),
        "{stale:?}"
    );
    if cut == 187 {
        assert_eq!(checkpoint(&f).await?, before);
        assert!(runtime.pending_reply_runs(64).await?.is_empty());
    }
    // A post-stage interruption can retain a prepared intent. Opening a fresh
    // anchor reader intentionally refuses that half-commit; use the normal
    // Runtime reconciliation path instead of treating it as corrupt history.
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(f.world.balance().await.spent_units, 0);

    // Retry immediately against actual current history, without expiring any
    // lease/stage or deleting recovery evidence to make the retry succeed.
    let sync = f.sync(false).await.map_err(|error| {
        format!("cut {cut}: immediate current-head reconciliation must recover the retained original: {error}")
    })?;
    assert!(sync.caught_up && sync.discovery_complete);
    let pending = runtime.pending_reply_runs(64).await?;
    assert_eq!(
        pending.len(),
        1,
        "Reconciliation must retain exactly one original"
    );
    if cut == 187 {
        assert_eq!(sync.runs, pending);
        assert_eq!(checkpoint(&f).await?.sequence, before.sequence + 1);
    } else {
        assert!(checkpoint(&f).await?.sequence > before.sequence);
    }
    assert!(f.sync(false).await?.runs.is_empty());
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(f.world.balance().await.spent_units, 0);
    drop(runtime);
    f.finish().await;
    println!("stale discovery cut={cut}: ordinary reconciliation retains one original without paid work PASS");
    Ok(())
}
