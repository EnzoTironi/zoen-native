//! Actual shared stop requests survive crash cuts without claims or refunds.
use super::*;
pub(super) async fn run() -> TestResult {
    for fault in [0, 94, 95, 96, 97] {
        let mut f = Fixture::with_shared_runtime().await?;
        f.grant("shared-stop-owner").await?;
        f.message(ORIGINAL).await?;
        let sync = f.sync(false).await?;
        let run = sync.runs.first().ok_or("discovered original")?.clone();
        let before = checkpoint(&f).await?;
        let holder = if fault == 0 {
            Some(native::testing::retained_reply_holder(&f.world.runtime, &run.run).await?)
        } else {
            None
        };
        let lease_before = if holder.is_some() {
            Some(native::testing::reply_lease_snapshot(&f.world.runtime, &run.run).await)
        } else {
            None
        };
        f.world.runtime.fault.store(fault, Ordering::SeqCst);
        let result = f.world.runtime.cancel_model(&run.run).await;
        f.world.runtime.fault.store(0, Ordering::SeqCst);
        if fault == 0 {
            assert_eq!(result?, ModelCancellation::Requested);
        } else {
            assert!(matches!(result, Err(RuntimeError::Unavailable)));
        }
        if let Some(holder) = holder {
            let lease_after =
                native::testing::reply_lease_snapshot(&f.world.runtime, &run.run).await;
            assert_eq!(
                lease_before.ok_or("original paired leases")?.1,
                lease_after.1,
                "recording a stop must not steal either live lease"
            );
            assert!(lease_after
                .1
                .iter()
                .all(|(_, _, expires)| *expires > lease_after.0));
            let tx = f.world.runtime.execution.transaction().await?;
            assert_eq!(
                holder
                    .scope_in(&f.world.runtime.execution, &tx, &f.world.runtime.custody)
                    .await,
                Err(RuntimeError::Denied)
            );
            drop(tx);
            holder.release(&f.world.runtime.execution).await?;
        }
        if fault == 94 {
            let prepared = f
                .world
                .runtime
                .runtime_continuity
                .as_ref()
                .ok_or("shared writer")?
                .lock()
                .await
                .anchor
                .pending()
                .map_err(crate::managed_images::error)?
                .ok_or("original pending stop")?;
            for (checkpoint, must_refuse) in [(&prepared.next, true), (&prepared.previous, false)] {
                sqlx::query("UPDATE runtime_image_checkpoint SET generation=$1,sequence=$2,digest=$3 WHERE singleton AND profile=3")
                    .bind(i64::try_from(checkpoint.generation)?).bind(i64::try_from(checkpoint.sequence)?).bind(&checkpoint.digest)
                    .execute(&f.world.runtime.finance.pool).await?;
                if must_refuse {
                    assert!(
                        f.world.runtime.pending_reply_runs(64).await.is_err(),
                        "SQL-first contradicts the original FDB-first stop order"
                    );
                    let tx = f.world.runtime.execution.transaction().await?;
                    assert!(tx
                        .get(&f.world.runtime.execution.run(&run.run, "cancel"), false)
                        .await?
                        .is_none());
                }
            }
        }
        // Revocation and later history do not erase an already retained stop.
        f.encrypted(EventBody::GrantRevoked {
            grant: "shared-stop-owner".into(),
        })
        .await?;
        reopen(&mut f).await?;
        assert_eq!(checkpoint(&f).await?.sequence, before.sequence + 1);
        assert_eq!(
            f.world.runtime.inspect_reply_run(&run.run).await,
            Err(RuntimeError::Denied)
        );
        let cp = checkpoint(&f).await?;
        assert_eq!(
            f.world.runtime.cancel_model(&run.run).await?,
            ModelCancellation::Requested
        );
        assert_eq!(
            checkpoint(&f).await?,
            cp,
            "observing the original stop creates no successor"
        );
        let (left, right) = tokio::join!(
            f.world.runtime.cancel_model(&run.run),
            f.world.runtime.cancel_model(&run.run)
        );
        assert_eq!(left?, ModelCancellation::Requested);
        assert_eq!(right?, ModelCancellation::Requested);
        assert_eq!(checkpoint(&f).await?, cp);

        assert_eq!(
            f.world.runtime.pending_reply_runs(64).await?,
            vec![run.clone()],
            "request acknowledgement is not terminal queue consumption"
        );
        if fault == 95 {
            let key = f.world.runtime.execution.run(&run.run, "cancel");
            let tx = f.world.runtime.execution.transaction().await?;
            let original = tx
                .get(&key, false)
                .await?
                .ok_or("original stop bytes")?
                .to_vec();
            tx.clear(&key);
            tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
            assert!(f.world.runtime.cancel_model(&run.run).await.is_err());
            let tx = f.world.runtime.execution.transaction().await?;
            tx.set(&key, &original);
            tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
            assert_eq!(
                f.world.runtime.cancel_model(&run.run).await?,
                ModelCancellation::Requested
            );
            assert_eq!(checkpoint(&f).await?, cp);
        }
        let before_unknown = checkpoint(&f).await?;
        assert!(f
            .world
            .runtime
            .cancel_model("unknown-original-run")
            .await
            .is_err());
        assert_eq!(checkpoint(&f).await?, before_unknown);
        assert_eq!(
            f.world.runtime.continuity(),
            Err(RuntimeError::RestoreUnreconciled)
        );
        assert!(f.world.runtime.production_output_scope().is_err());
        assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
        for query in [
            "SELECT count(*) FROM runtime_attempts",
            "SELECT count(*) FROM runtime_claims",
            "SELECT count(*) FROM runtime_dispositions",
            "SELECT count(*) FROM runtime_predispatch_closures",
        ] {
            assert_eq!(f.world.count(query).await, 0);
        }
        reopen(&mut f).await?;
        assert_eq!(
            f.world.runtime.cancel_model(&run.run).await?,
            ModelCancellation::Requested
        );
        assert_eq!(checkpoint(&f).await?, cp);
        f.finish().await;
        println!("shared native stop fault {fault}: exact original stop survives cold/FDB-first cuts and later revoke, live paired leases unchanged, missing actual stop refused, one checkpoint/no refund/provider/financial effects PASS");
    }
    Ok(())
}
