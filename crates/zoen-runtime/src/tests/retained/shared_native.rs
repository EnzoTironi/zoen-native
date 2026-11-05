//! Shared actual policies/native capsules, source cursors and cold failure cuts.
mod cancellation;
pub(in crate::tests) mod capture;
mod claims;
mod discovery;
mod dispatch;
mod reservation;
use super::*;
use relay_admission::managed_images::AnchorReader;
type TestResult = Result<(), Box<dyn std::error::Error>>;

async fn reopen(f: &mut Fixture) -> TestResult {
    let mut reopened = f.world.reopen().await;
    let custody = f.restore_custody();
    let runtime = Arc::get_mut(&mut reopened).ok_or("new runtime unexpectedly shared")?;
    runtime.native = Some(custody);
    runtime.continuity.store(false, Ordering::SeqCst);
    Arc::get_mut(&mut f.world.runtime)
        .ok_or("old runtime unexpectedly shared")?
        .runtime_continuity = None;
    let runtime = Arc::try_unwrap(reopened)
        .map_err(|_| "new runtime unexpectedly shared")?
        .open_runtime_continuity(&f.shared_directory.as_ref().ok_or("shared directory")?.0)
        .await?;
    f.world.runtime.finance.pool.close().await;
    f.world.runtime = Arc::new(runtime);
    Ok(())
}
async fn checkpoint(
    f: &Fixture,
) -> Result<relay_admission::managed_images::Checkpoint, RuntimeError> {
    let managed = f
        .world
        .runtime
        .runtime_continuity
        .as_ref()
        .ok_or(RuntimeError::RestoreUnreconciled)?
        .lock()
        .await;
    let reader = AnchorReader::open_runtime(
        &f.shared_directory
            .as_ref()
            .ok_or(RuntimeError::RestoreUnreconciled)?
            .0,
        managed.test_deployment(),
    )
    .map_err(crate::managed_images::error)?;
    reader.checkpoint().map_err(crate::managed_images::error)
}
async fn refresh(f: &Fixture) -> Result<bool, RuntimeError> {
    let actor = &f.world.agents[0];
    f.world
        .runtime
        .native
        .as_ref()
        .ok_or(RuntimeError::CoreAuthorityUnavailable)?
        .refresh_source(
            &f.world.runtime,
            &actor.identity,
            actor
                .device
                .as_deref()
                .ok_or(RuntimeError::InvalidBinding)?,
            &f.space,
        )
        .await
        .map(|progress| progress.caught_up)
}
pub(in crate::tests) async fn run() -> TestResult {
    for fault in [0, 90, 91, 92] {
        let mut f = Fixture::with_shared_runtime().await?;
        f.grant("shared-source-owner").await?;
        f.message(ORIGINAL).await?;
        let before = checkpoint(&f).await?;
        let actor = f.world.agents[0].clone();
        let device = actor.device.as_deref().ok_or("fixture device")?;
        let key = f.world.runtime.execution.root.pack(&(
            "native-device",
            actor.identity.as_str(),
            device,
            "root",
        ));
        let tx = f.world.runtime.execution.transaction().await?;
        let original_root = tx.get(&key, false).await?.ok_or("original root")?.to_vec();
        drop(tx);
        f.world.runtime.fault.store(fault, Ordering::SeqCst);
        let result = refresh(&f).await;
        f.world.runtime.fault.store(0, Ordering::SeqCst);
        if fault == 0 {
            assert!(result?);
        } else {
            assert!(matches!(result, Err(RuntimeError::Unavailable)));
        }
        if fault != 0 {
            f.message("Later source survives original maintenance recovery.")
                .await?;
        }
        reopen(&mut f).await?;
        assert_eq!(checkpoint(&f).await?.sequence, before.sequence + 1);
        f.world.runtime.check_shared_runtime().await?;
        let tx = f.world.runtime.execution.transaction().await?;
        let recovered_root = tx.get(&key, false).await?.ok_or("recovered root")?.to_vec();
        assert_ne!(recovered_root, original_root);
        // Same detector pair with a valid old capsule is still an actual rewind.
        tx.set(&key, &original_root);
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        assert_eq!(
            f.world.runtime.check_shared_runtime().await,
            Err(RuntimeError::RestoreUnreconciled)
        );
        let tx = f.world.runtime.execution.transaction().await?;
        tx.set(&key, &recovered_root);
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        f.world.runtime.check_shared_runtime().await?;
        assert!(refresh(&f).await?);
        let cp = checkpoint(&f).await?;
        reopen(&mut f).await?;
        assert_eq!(checkpoint(&f).await?, cp);
        assert!(refresh(&f).await?);
        assert_eq!(
            checkpoint(&f).await?,
            cp,
            "read-only replay does not append a commitment"
        );
        assert_eq!(
            f.world.runtime.continuity(),
            Err(RuntimeError::RestoreUnreconciled)
        );
        assert!(f.world.runtime.production_output_scope().is_err());
        assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
        assert_eq!(
            f.world.count("SELECT count(*) FROM runtime_attempts").await,
            0
        );
        assert_eq!(
            f.world.count("SELECT count(*) FROM runtime_claims").await,
            0
        );
        f.finish().await;
        println!("shared native maintenance fault {fault}: original bootstrap/MLS packages/source cursor share policy anchor, cold exact recovery, valid old native root rejected despite unchanged detector pair, zero provider/financial claims PASS");
    }
    Box::pin(discovery::run()).await?;
    Box::pin(cancellation::run()).await?;
    Box::pin(reservation::run()).await?;
    Box::pin(claims::run()).await?;
    Box::pin(dispatch::run()).await?;
    Box::pin(capture::run()).await?;
    Ok(())
}
