//! Only a live original SQL insertion may return a one-shot claim. Recovering
//! the exact original nonce is evidence, never a reconstructed provider right.
use super::*;
use crate::tests::shared_policy_continuity::snapshot::{self, Snapshot};
pub(super) async fn run() -> TestResult {
    for fault in [0, 106, 107, 108, 109] {
        let mut f = Fixture::with_shared_runtime().await?;
        f.grant("shared-claim-owner").await?;
        f.message(ORIGINAL).await?;
        let report = f.sync(false).await?;
        let run = report.runs.first().ok_or("original run")?.clone();
        let binding = native::testing::reserve_retained_reply(&f.world.runtime, &run.run).await?;
        let backup = if fault == 0 {
            Some(Snapshot::capture(&f.world)?)
        } else {
            None
        };
        let before = checkpoint(&f).await?;
        f.world.runtime.fault.store(fault, Ordering::SeqCst);
        let result = native::testing::claim_retained_reply(&f.world.runtime, &run.run).await;
        f.world.runtime.fault.store(0, Ordering::SeqCst);
        let fresh = if fault == 0 {
            Some(result?.ok_or("live original claim")?)
        } else {
            assert!(matches!(result, Err(RuntimeError::Unavailable)));
            None
        };
        reopen(&mut f).await?;
        let cp = checkpoint(&f).await?;
        assert_eq!(cp.sequence, before.sequence + 1);
        assert_eq!(
            f.world.count("SELECT count(*) FROM runtime_claims").await,
            1
        );
        let nonce: String = sqlx::query_scalar("SELECT nonce FROM runtime_claims WHERE attempt=$1")
            .bind(&run.attempt)
            .fetch_one(&f.world.runtime.finance.pool)
            .await?;
        if let Some(fresh) = fresh {
            let (attempt, original, financial_binding, image_checkpoint) = fresh.consume();
            assert_eq!(attempt, run.attempt);
            assert_eq!(original, nonce);
            assert_eq!(financial_binding, binding.digest()?);
            assert!(image_checkpoint.is_none());
        }
        assert!(
            native::testing::claim_retained_reply(&f.world.runtime, &run.run)
                .await?
                .is_none(),
            "observing a retained SQL nonce never creates a fresh claim"
        );
        assert_eq!(checkpoint(&f).await?, cp);
        assert_eq!(
            f.world.runtime.inspect_attempt(&run.attempt).await?,
            Some(FinancialState::Claimed)
        );
        assert_eq!(f.world.balance().await.held_units, binding.hold_units);
        assert_eq!(f.world.balance().await.spent_units, 0);
        assert!(matches!(
            f.world.runtime.finance.claim(&binding).await,
            Err(RuntimeError::RestoreUnreconciled)
        ));
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
                0,
                0,
            )
            .await?;
            reopen(&mut f).await?;
            assert!(
                native::testing::claim_retained_reply(&f.world.runtime, &run.run)
                    .await?
                    .is_none()
            );
            assert_eq!(checkpoint(&f).await?, cp);
        }
        assert_eq!(
            f.world.runtime.continuity(),
            Err(RuntimeError::RestoreUnreconciled)
        );
        assert!(f.world.runtime.production_output_scope().is_err());
        assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
        for query in [
            "SELECT count(*) FROM runtime_admission_witnesses",
            "SELECT count(*) FROM runtime_evidence",
            "SELECT count(*) FROM runtime_dispositions",
        ] {
            assert_eq!(f.world.count(query).await, 0);
        }
        reopen(&mut f).await?;
        let recovered: String =
            sqlx::query_scalar("SELECT nonce FROM runtime_claims WHERE attempt=$1")
                .bind(&run.attempt)
                .fetch_one(&f.world.runtime.finance.pool)
                .await?;
        assert_eq!(recovered, nonce);
        assert!(
            native::testing::claim_retained_reply(&f.world.runtime, &run.run)
                .await?
                .is_none()
        );
        assert_eq!(checkpoint(&f).await?, cp);
        assert_eq!(
            f.world.runtime.cancel_model(&run.run).await?,
            ModelCancellation::Requested
        );
        assert_eq!(
            f.world.balance().await.held_units,
            binding.hold_units,
            "a stop request cannot refund an existing claim"
        );
        assert_eq!(
            f.world
                .count("SELECT count(*) FROM runtime_dispositions")
                .await,
            0
        );
        f.finish().await;
        println!("shared native claim fault {fault}: one exact original nonce through SQL-first crash/ACK cuts, cold repeat never fresh, old actual SQL claim backup refused despite matching markers, stop request retains hold, zero admission/HTTP/bill/refund PASS");
    }
    Box::pin(race()).await?;
    Ok(())
}
async fn race() -> TestResult {
    let mut f = Fixture::with_shared_runtime().await?;
    f.grant("shared-claim-race").await?;
    f.message(ORIGINAL).await?;
    let report = f.sync(false).await?;
    let run = report.runs.first().ok_or("race run")?;
    let binding = native::testing::reserve_retained_reply(&f.world.runtime, &run.run).await?;
    let before = checkpoint(&f).await?;
    let (left, right) = tokio::join!(
        native::testing::claim_retained_reply(&f.world.runtime, &run.run),
        native::testing::claim_retained_reply(&f.world.runtime, &run.run)
    );
    let mut fresh = 0;
    for result in [left, right] {
        match result {
            Ok(Some(_)) => fresh += 1,
            // FDB optimistic commit contention is conservatively Unavailable
            // at this API; it grants no permission. The exact one-winner, SQL,
            // checkpoint and subsequent non-fresh observations remain required.
            Ok(None) | Err(RuntimeError::Denied | RuntimeError::Unavailable) => {}
            Err(error) => return Err(error.into()),
        }
    }
    assert_eq!(fresh, 1);
    assert_eq!(checkpoint(&f).await?.sequence, before.sequence + 1);
    assert_eq!(
        f.world.count("SELECT count(*) FROM runtime_claims").await,
        1
    );
    assert_eq!(f.world.balance().await.held_units, binding.hold_units);
    assert!(
        native::testing::claim_retained_reply(&f.world.runtime, &run.run)
            .await?
            .is_none()
    );
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    f.finish().await;
    println!("shared native claim race: real paired leases and SQL uniqueness return exactly one fresh claim; later observation cannot recreate it PASS");
    Ok(())
}
