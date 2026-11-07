//! A relay head can advance while discovery seals its next native image.
use super::*;

pub(in crate::tests) async fn run() -> TestResult {
    let mut f = Fixture::with_shared_runtime().await?;
    f.grant("discovery-race-owner").await?;
    f.message(ORIGINAL).await?;
    let before = checkpoint(&f).await?;
    let runtime = f.world.runtime.clone();
    let agent = f.world.agents[0].identity.clone();
    let device = f.world.agents[0].device.clone().ok_or("device")?;
    let space = f.space.clone();
    runtime.fault.store(187, Ordering::SeqCst);
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
    assert!(matches!(stale, Err(RuntimeError::Denied)));
    assert_eq!(checkpoint(&f).await?, before);
    assert!(runtime.pending_reply_runs(64).await?.is_empty());
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(f.world.balance().await.spent_units, 0);

    // Retry immediately against actual current history, without expiring any
    // lease/stage or deleting recovery evidence to make the retry succeed.
    let sync = f.sync(false).await.map_err(|error| {
        format!("immediate current-head retry must not inherit a refused staging attempt: {error}")
    })?;
    assert!(sync.caught_up && sync.discovery_complete);
    assert_eq!(sync.runs.len(), 1);
    assert_eq!(checkpoint(&f).await?.sequence, before.sequence + 1);
    assert_eq!(runtime.pending_reply_runs(64).await?, sync.runs);
    assert!(f.sync(false).await?.runs.is_empty());
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(f.world.balance().await.spent_units, 0);
    drop(runtime);
    f.finish().await;
    println!("stale discovery: refused without a staged capsule or paid effect; immediate current-head retry retains one original PASS");
    Ok(())
}
