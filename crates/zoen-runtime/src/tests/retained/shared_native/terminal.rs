//! Delivery receipts, not scheduling flags, finish work and release old images.
use super::*;
use crate::tests::retained::image_tasks::delivery::relay::Relay;
use native::testing::output as proof;

type Rows = Vec<(Vec<u8>, Vec<u8>)>;
async fn retained(f: &Fixture, rows: &Rows, expected: &[bool]) -> TestResult {
    let tx = f.world.runtime.execution.transaction().await?;
    for ((image, reference), expected) in rows.iter().zip(expected) {
        assert_eq!(tx.get(image, false).await?.is_some(), *expected);
        assert!(
            tx.get(reference, false).await?.is_some(),
            "immutable audit row is retained"
        );
    }
    Ok(())
}
async fn paid(f: &Fixture, run: &crate::ReplyRun) -> TestResult {
    Box::pin(f.world.runtime.run_model(&run.run)).await?;
    f.world.runtime.prepare_reply_output(&run.run).await?;
    Ok(())
}
async fn deliver(f: &Fixture, relay: &Relay, run: &crate::ReplyRun) -> TestResult {
    assert!(matches!(
        f.world
            .runtime
            .deliver_reply_output(&run.run, relay.target(f, relay.address))
            .await?,
        crate::ReplyReceiptProgress::Accepted { .. }
    ));
    Ok(())
}
pub(in crate::tests) async fn run() -> TestResult {
    Box::pin(completed_prefix_and_shared_root()).await?;
    Box::pin(unknown_cost_retains_the_bill()).await?;
    Ok(())
}
async fn completed_prefix_and_shared_root() -> TestResult {
    let mut f = Fixture::with_shared_runtime().await?;
    f.grant("terminal-owner").await?;
    f.message("First source").await?;
    f.message("Second source").await?;
    let runs = f.sync(false).await?.runs;
    assert_eq!(runs.len(), 2);
    assert!(runs[0].wake < runs[1].wake);
    output_route::configured(&f, true).await?;
    let relay = Relay::start(&f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    Box::pin(paid(&f, &runs[0])).await?;
    let rows = proof::retention_rows(&f.world.runtime, &runs[0].run).await?;
    proof::collect_reply_images(&f.world.runtime, &runs[0].run).await?;
    retained(&f, &rows, &[true, true]).await?;
    assert_eq!(f.world.runtime.pending_reply_runs(64).await?.len(), 2);
    Box::pin(deliver(&f, &relay, &runs[0])).await?;
    // The shared original image stays protected by the other unfinished run.
    proof::collect_reply_images(&f.world.runtime, &runs[0].run).await?;
    retained(&f, &rows, &[true, false]).await?;
    let page = f.world.runtime.pending_reply_page(1, None).await?;
    assert_eq!(page.scanned, 1);
    assert!(page.runs.is_empty());
    assert_eq!(page.next.as_deref(), Some(runs[0].wake.as_str()));
    let page2 = f
        .world
        .runtime
        .pending_reply_page(1, page.next.as_deref())
        .await?;
    assert_eq!(page2.runs, vec![runs[1].clone()]);
    assert!(page2.next.is_none());
    assert_eq!(
        f.world.runtime.pending_reply_runs(64).await?,
        vec![runs[1].clone()]
    );
    for (limit, cursor) in [(0, None), (65, None), (1, Some("invalid cursor"))] {
        assert_eq!(
            f.world.runtime.pending_reply_page(limit, cursor).await,
            Err(RuntimeError::InvalidBinding)
        );
    }
    // Corruption cannot turn a reply into a scheduling ACK or a GC permit.
    let key = f
        .world
        .runtime
        .execution
        .run(&runs[0].run, "native-receipt/1");
    let tx = f.world.runtime.execution.transaction().await?;
    let original = tx.get(&key, false).await?.ok_or("receipt")?.to_vec();
    let mut changed = original.clone();
    let byte = changed.last_mut().ok_or("receipt byte")?;
    *byte ^= 1;
    tx.set(&key, &changed);
    tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
    assert!(f.world.runtime.pending_reply_page(1, None).await.is_err());
    assert!(proof::collect_reply_images(&f.world.runtime, &runs[0].run)
        .await
        .is_err());
    let tx = f.world.runtime.execution.transaction().await?;
    tx.set(&key, &original);
    tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
    Box::pin(paid(&f, &runs[1])).await?;
    Box::pin(deliver(&f, &relay, &runs[1])).await?;
    let second = proof::retention_rows(&f.world.runtime, &runs[1].run).await?;
    proof::collect_reply_images(&f.world.runtime, &runs[1].run).await?;
    retained(&f, &rows, &[false, false]).await?;
    retained(&f, &second, &[false, false]).await?;
    assert!(f.world.runtime.pending_reply_runs(64).await?.is_empty());
    let before = checkpoint(&f).await?;
    reopen(&mut f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    for run in &runs {
        assert_eq!(
            Box::pin(f.world.runtime.run_model(&run.run))
                .await?
                .financial,
            crate::FinancialState::Settled { units: 6 }
        );
        f.world.runtime.prepare_reply_output(&run.run).await?;
        Box::pin(deliver(&f, &relay, run)).await?;
    }
    assert!(f.world.runtime.pending_reply_runs(64).await?.is_empty());
    assert_eq!(checkpoint(&f).await?, before);
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 2);
    assert_eq!(f.world.balance().await.spent_units, 12);
    assert_eq!(
        f.world.count("SELECT count(*) FROM runtime_claims").await,
        2
    );
    relay.stop().await;
    f.finish().await;
    println!("terminal replies: empty completed page traverses to real later work, shared unfinished root stays pinned, corruption refuses GC/ACK, completed images collected before timer, immutable rows/cold result/receipt retain one charge PASS");
    Ok(())
}
async fn unknown_cost_retains_the_bill() -> TestResult {
    let (mut f, relay, run) = Box::pin(execution::unpaid()).await?;
    f.world
        .http
        .body
        .lock()
        .map_err(|_| "body")?
        .as_object_mut()
        .ok_or("object")?
        .remove("usage");
    let result = Box::pin(f.world.runtime.run_model(&run.run)).await?;
    assert_eq!(result.financial, crate::FinancialState::Claimed);
    let held = f.world.balance().await.held_units;
    assert!(held > 0);
    f.world.runtime.prepare_reply_output(&run.run).await?;
    let rows = proof::retention_rows(&f.world.runtime, &run.run).await?;
    Box::pin(deliver(&f, &relay, &run)).await?;
    proof::collect_reply_images(&f.world.runtime, &run.run).await?;
    retained(&f, &rows, &[false, false]).await?;
    assert!(f.world.runtime.pending_reply_runs(64).await?.is_empty());
    reopen(&mut f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    assert_eq!(
        Box::pin(f.world.runtime.run_model(&run.run))
            .await?
            .financial,
        result.financial
    );
    assert_eq!(f.world.balance().await.held_units, held);
    assert_eq!(f.world.balance().await.spent_units, 0);
    assert_eq!(
        f.world
            .count("SELECT count(*) FROM runtime_dispositions")
            .await,
        0
    );
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    relay.stop().await;
    f.finish().await;
    println!("terminal reply with unknown usage: delivered work ends, obsolete images collect, original claimed hold/evidence survive cold recovery with no invented usage/refund/retry PASS");
    Ok(())
}
