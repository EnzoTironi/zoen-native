//! Actual original discovery survives SQL/FDB cuts without minting new runs.
use super::*;
pub(super) async fn run() -> TestResult {
    for fault in [0, 90, 91, 92] {
        let mut f = Fixture::with_shared_runtime().await?;
        f.grant("shared-discovery-owner").await?;
        let input = if fault == 91 {
            ORIGINAL.repeat(600)
        } else {
            ORIGINAL.into()
        };
        f.message(&input).await?;
        let before = checkpoint(&f).await?;
        f.world.runtime.fault.store(fault, Ordering::SeqCst);
        let result = f.sync(false).await;
        f.world.runtime.fault.store(0, Ordering::SeqCst);
        let returned = if fault == 0 {
            let sync = result?;
            assert!(sync.caught_up && sync.discovery_complete);
            assert_eq!(sync.runs.len(), 1);
            Some(sync.runs[0].clone())
        } else {
            assert!(matches!(result, Err(RuntimeError::Unavailable)));
            None
        };
        // This is a clear membership event, not another reply trigger. Recovery
        // must retain the original source and later catch up independently.
        f.clear(EventBody::DeviceJoining {
            device: f.world.owner.device.clone().ok_or("owner device")?,
        })
        .await?;
        reopen(&mut f).await?;
        assert_eq!(checkpoint(&f).await?.sequence, before.sequence + 1);
        let pending = f.world.runtime.pending_reply_runs(64).await?;
        assert_eq!(pending.len(), 1);
        if let Some(returned) = returned {
            assert_eq!(returned, pending[0]);
        }
        let original = pending[0].clone();
        let (text, _) = native::testing::reply_original(&f.world.runtime, &original.run).await;
        assert_eq!(text, format!("{input}\n"));
        // Missing any actual immutable/control row cannot be covered up by
        // unchanged SQL/FDB checkpoint markers or the independent intent.
        if fault == 91 {
            let prefix = f
                .world
                .runtime
                .execution
                .root
                .subspace(&("shared-native-discovery/1",));
            let tx = f.world.runtime.execution.transaction().await?;
            let chunks = f
                .world
                .runtime
                .execution
                .root
                .subspace(&("evidence-chunk/1",));
            let mut chunk_range = foundationdb::RangeOption::from(chunks.range());
            chunk_range.limit = Some(2);
            chunk_range.mode = foundationdb::options::StreamingMode::WantAll;
            assert_eq!(
                tx.get_range(&chunk_range, 1, false).await?.len(),
                2,
                "large original really spans independently committed physical chunks"
            );
            let mut range = foundationdb::RangeOption::from(prefix.range());
            range.limit = Some(1);
            let rows = tx.get_range(&range, 1, false).await?;
            let index = rows
                .iter()
                .next()
                .ok_or("original discovery index")?
                .key()
                .to_vec();
            drop(tx);
            let keys = [
                f.world.runtime.execution.run(&original.run, "record/1"),
                f.world
                    .runtime
                    .execution
                    .attempt(&original.attempt, "prepared"),
                f.world
                    .runtime
                    .execution
                    .root
                    .pack(&("native-wake/1", original.wake.as_str())),
                index,
            ];
            for key in keys {
                let tx = f.world.runtime.execution.transaction().await?;
                let bytes = tx.get(&key, false).await?.ok_or("original row")?.to_vec();
                tx.clear(&key);
                tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
                assert!(f.world.runtime.pending_reply_runs(64).await.is_err());
                let tx = f.world.runtime.execution.transaction().await?;
                tx.set(&key, &bytes);
                tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
                assert_eq!(f.world.runtime.pending_reply_runs(64).await?, pending);
            }
        }
        let first = f.sync(false).await?;
        assert!(first.caught_up && first.discovery_complete && first.runs.is_empty());
        let other = f.sync(true).await?;
        assert!(other.caught_up && other.discovery_complete);
        assert_eq!(
            other.runs, pending,
            "other certified device observes the same original trigger"
        );
        assert_eq!(f.world.runtime.pending_reply_runs(64).await?, pending);
        let cp = checkpoint(&f).await?;
        reopen(&mut f).await?;
        assert_eq!(checkpoint(&f).await?, cp);
        assert!(f.sync(false).await?.runs.is_empty());
        assert_eq!(checkpoint(&f).await?, cp);
        assert!(f.world.runtime.production_output_scope().is_err());
        assert_eq!(
            f.world.runtime.continuity(),
            Err(RuntimeError::RestoreUnreconciled)
        );
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
        println!("shared native discovery fault {fault}: original run/binding/trigger/wake/GC reference and capsule recover together, later source and cross-device dedupe preserve IDs, missing actual evidence refused, zero provider/financial effects PASS");
    }
    Ok(())
}
