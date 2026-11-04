//! Actual FDB admission and actual SQL witness have separate crash boundaries.
//! Recovery may observe originals, never recreate live permission or HTTP.
use super::*;
use crate::tests::shared_policy_continuity::snapshot::{self, Snapshot};
pub(super) async fn run() -> TestResult {
    for fault in [0, 110, 111, 112, 113, 114, 115, 116, 117] {
        let mut f = Fixture::with_shared_runtime().await?;
        f.grant("shared-dispatch-owner").await?;
        f.message(ORIGINAL).await?;
        let report = f.sync(false).await?;
        let run = report.runs.first().ok_or("original run")?.clone();
        let binding = native::testing::reserve_retained_reply(&f.world.runtime, &run.run).await?;
        let claim = native::testing::claim_retained_reply(&f.world.runtime, &run.run)
            .await?
            .ok_or("original live claim")?;
        let nonce = claim.checkpoint_reference().nonce;
        let backup = if fault == 0 {
            Some(Snapshot::capture(&f.world)?)
        } else {
            None
        };
        let before = checkpoint(&f).await?;
        f.world.runtime.fault.store(fault, Ordering::SeqCst);
        let result =
            native::testing::dispatch_retained_reply(&f.world.runtime, &run.run, claim).await;
        f.world.runtime.fault.store(0, Ordering::SeqCst);
        if fault == 0 {
            assert!(result?);
        } else {
            assert!(
                matches!(result, Err(RuntimeError::Unavailable)),
                "fault {fault}: {result:?}"
            );
        }
        let expected_witnesses = i64::from(fault == 0 || fault >= 115);
        assert_eq!(
            f.world
                .count("SELECT count(*) FROM runtime_admission_witnesses")
                .await,
            expected_witnesses
        );
        assert_eq!(
            f.world.count("SELECT count(*) FROM runtime_claims").await,
            1
        );
        assert_eq!(f.world.balance().await.held_units, binding.hold_units);
        assert_eq!(f.world.balance().await.spent_units, 0);
        assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
        for query in [
            "SELECT count(*) FROM runtime_evidence",
            "SELECT count(*) FROM runtime_dispositions",
        ] {
            assert_eq!(f.world.count(query).await, 0);
        }
        let admitted_key = f.world.runtime.execution.attempt(&run.attempt, "admitted");
        let tx = f.world.runtime.execution.transaction().await?;
        let admitted = tx.get(&admitted_key, false).await?.map(|v| v.to_vec());
        drop(tx);
        assert_eq!(
            admitted.as_deref(),
            if fault == 110 {
                None
            } else {
                Some(nonce.as_bytes())
            }
        );
        if fault == 110 || fault == 114 {
            // Both-before cannot create the missing original commit. A retained
            // nonce or prepared intent is not the lost live receipt.
            for _ in 0..2 {
                assert_eq!(
                    f.world.runtime.check_shared_runtime().await,
                    Err(RuntimeError::RestoreUnreconciled)
                );
            }
            assert!(reopen(&mut f).await.is_err());
            assert_eq!(
                f.world
                    .count("SELECT count(*) FROM runtime_admission_witnesses")
                    .await,
                0
            );
            assert_eq!(f.world.balance().await.held_units, binding.hold_units);
            f.finish().await;
            println!("shared dispatch fault {fault}: missing actual admission/witness remains refused across cold reopen; no manufactured permission, HTTP or refund PASS");
            continue;
        }
        reopen(&mut f).await?;
        let cp = checkpoint(&f).await?;
        assert_eq!(
            cp.sequence,
            before.sequence + if expected_witnesses == 1 { 2 } else { 1 }
        );
        f.world.runtime.check_shared_runtime().await?;
        if expected_witnesses == 1 {
            let actual: String = sqlx::query_scalar(
                "SELECT nonce FROM runtime_admission_witnesses WHERE attempt=$1",
            )
            .bind(&run.attempt)
            .fetch_one(&f.world.runtime.finance.pool)
            .await?;
            assert_eq!(actual, nonce);
        }
        assert!(
            native::testing::claim_retained_reply(&f.world.runtime, &run.run)
                .await?
                .is_none(),
            "original claim never becomes fresh after recovery"
        );
        assert_eq!(checkpoint(&f).await?, cp);
        // A valid detector pair cannot hide a missing or changed actual FDB
        // admission. Put back the exact original fixture bytes afterward.
        let tx = f.world.runtime.execution.transaction().await?;
        tx.clear(&admitted_key);
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        assert_eq!(
            f.world.runtime.check_shared_runtime().await,
            Err(RuntimeError::RestoreUnreconciled)
        );
        let tx = f.world.runtime.execution.transaction().await?;
        tx.set(&admitted_key, b"different-original-nonce");
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        assert!(f.world.runtime.check_shared_runtime().await.is_err());
        let tx = f.world.runtime.execution.transaction().await?;
        tx.set(&admitted_key, nonce.as_bytes());
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        f.world.runtime.check_shared_runtime().await?;
        if let Some(backup) = backup {
            Arc::get_mut(&mut f.world.runtime)
                .ok_or("exclusive runtime")?
                .runtime_continuity = None;
            snapshot::restored_finance_refuses(
                &f.world,
                &f.shared_directory.as_ref().ok_or("shared directory")?.0,
                &backup,
                &cp,
                1,
                1,
                0,
            )
            .await?;
            reopen(&mut f).await?;
        }
        assert_eq!(
            f.world.runtime.continuity(),
            Err(RuntimeError::RestoreUnreconciled)
        );
        assert!(f.world.runtime.production_output_scope().is_err());
        assert_eq!(
            f.world.runtime.cancel_model(&run.run).await?,
            ModelCancellation::Requested
        );
        assert_eq!(
            f.world.balance().await.held_units,
            binding.hold_units,
            "a stop request never refunds an admitted claim"
        );
        assert_eq!(
            f.world
                .count("SELECT count(*) FROM runtime_admission_witnesses")
                .await,
            expected_witnesses
        );
        assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
        f.finish().await;
        println!("shared dispatch fault {fault}: exact original FDB admission and distinct SQL witness, cold recovery never fresh, actual admitted mutation and old SQL witness backup refused, stop retains hold, zero HTTP/bill/refund PASS");
    }
    Ok(())
}
