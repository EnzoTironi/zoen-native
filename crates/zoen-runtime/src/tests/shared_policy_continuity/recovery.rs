use super::*;
use relay_admission::managed_images::Checkpoint;

pub(super) async fn orphan_period_refuses_bootstrap() -> TestResult {
    let w = empty_world().await?;
    let (start, end) = finance::period(w.signed.policy.year, w.signed.policy.month)?;
    sqlx::query("INSERT INTO runtime_periods(owner,period_start,period_end,currency,scale) VALUES($1,$2,$3,'USD',6)")
        .bind(&w.owner.identity).bind(start).bind(end).execute(&w.runtime.finance.pool).await?;
    let home = Home::new()?;
    assert!(matches!(
        fresh(&w)
            .await?
            .initialize_runtime_continuity(&home.0)
            .await,
        Err(RuntimeError::RestoreUnreconciled)
    ));
    assert!(!home.0.exists());
    assert_eq!(w.count("SELECT count(*) FROM runtime_periods").await, 1);
    no_financial_effects(&w).await?;
    w.finish().await;
    Ok(())
}

async fn set_markers(w: &World, cp: &Checkpoint) -> TestResult {
    sqlx::query("UPDATE runtime_image_checkpoint SET generation=$1,sequence=$2,digest=$3 WHERE singleton AND profile=3")
        .bind(i64::try_from(cp.generation)?).bind(i64::try_from(cp.sequence)?).bind(&cp.digest).execute(&w.runtime.finance.pool).await?;
    let tx = w.runtime.execution.transaction().await?;
    tx.set(
        &w.runtime
            .execution
            .root
            .pack(&("managed-images/checkpoint/1",)),
        &serde_json::to_vec(cp)?,
    );
    tx.commit().await?;
    Ok(())
}

pub(super) async fn matching_old_pair(
    w: &World,
    home: &Home,
    genesis: &Checkpoint,
    tip: &Checkpoint,
) -> TestResult {
    set_markers(w, genesis).await?;
    assert!(matches!(
        fresh(w).await?.open_runtime_continuity(&home.0).await,
        Err(RuntimeError::RestoreUnreconciled)
    ));
    set_markers(w, tip).await?;
    let restored = fresh(w).await?.open_runtime_continuity(&home.0).await?;
    assert_eq!(reader(&restored, &home.0).await?.checkpoint()?, *tip);
    drop(restored);
    Ok(())
}

pub(super) async fn original_policy(cut: u8) -> TestResult {
    let w = empty_world().await?;
    let home = Home::new()?;
    let runtime = fresh(&w)
        .await?
        .initialize_runtime_continuity(&home.0)
        .await?;
    runtime.fault.store(cut, Ordering::SeqCst);
    assert_eq!(
        runtime.install_policy(w.signed.clone()).await,
        Err(RuntimeError::Unavailable)
    );
    let (retained, decoded) = runtime
        .runtime_continuity
        .as_ref()
        .ok_or("continuity")?
        .lock()
        .await
        .test_original_policy(&runtime)?;
    assert_eq!(decoded, w.signed);
    assert_eq!(retained.subject_updates.len(), 1);
    assert!(retained
        .intent
        .as_ref()
        .is_some_and(|bytes| !bytes.contains("limit_units")));
    drop(runtime);
    let runtime = fresh(&w).await?.open_runtime_continuity(&home.0).await?;
    let cp = reader(&runtime, &home.0).await?.checkpoint()?;
    assert_eq!(cp, retained.next);
    assert_eq!(cp.sequence, 1);
    assert_eq!(w.count("SELECT count(*) FROM runtime_policies").await, 1);
    assert_eq!(
        runtime.install_policy(w.signed.clone()).await?,
        w.policy_digest
    );
    assert_eq!(reader(&runtime, &home.0).await?.checkpoint()?, cp);
    assert_eq!(runtime.continuity(), Err(RuntimeError::RestoreUnreconciled));
    assert!(runtime.production_output_scope().is_err());
    no_financial_effects(&w).await?;
    drop(runtime);
    w.finish().await;
    Ok(())
}

pub(super) async fn revoked_owner_cannot_recover_policy() -> TestResult {
    let w = empty_world().await?;
    let home = Home::new()?;
    let runtime = fresh(&w)
        .await?
        .initialize_runtime_continuity(&home.0)
        .await?;
    runtime.fault.store(81, Ordering::SeqCst);
    assert_eq!(
        runtime.install_policy(w.signed.clone()).await,
        Err(RuntimeError::Unavailable)
    );
    let original = std::fs::read(home.0.join("checkpoint.json"))?;
    drop(runtime);
    let device = w.owner.device.as_deref().ok_or("owner device")?;
    sqlx::query("UPDATE devices SET revoked_at=clock_timestamp() WHERE device=$1")
        .bind(device)
        .execute(&w.runtime.finance.pool)
        .await?;
    assert!(matches!(
        fresh(&w).await?.open_runtime_continuity(&home.0).await,
        Err(RuntimeError::Denied)
    ));
    assert_eq!(std::fs::read(home.0.join("checkpoint.json"))?, original);
    assert_eq!(w.count("SELECT count(*) FROM runtime_policies").await, 0);
    no_financial_effects(&w).await?;
    // Restore only this fixture's deliberate revocation to prove that the
    // original pending intent, rather than a replacement policy, can finish.
    sqlx::query("UPDATE devices SET revoked_at=NULL WHERE device=$1")
        .bind(device)
        .execute(&w.runtime.finance.pool)
        .await?;
    let runtime = fresh(&w).await?.open_runtime_continuity(&home.0).await?;
    assert_eq!(reader(&runtime, &home.0).await?.checkpoint()?.sequence, 1);
    assert_eq!(
        runtime.install_policy(w.signed.clone()).await?,
        w.policy_digest
    );
    no_financial_effects(&w).await?;
    drop(runtime);
    w.finish().await;
    Ok(())
}
