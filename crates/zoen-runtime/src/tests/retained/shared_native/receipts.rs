//! Real source receipts and cold SQL/FDB cuts, never inferred transport ACKs.
use super::*;
use native::testing::output as proof;

pub(in crate::tests) async fn run() -> TestResult {
    for prefix in [false, true] {
        for fault in [0, 159, 160, 161, 162, 163] {
            Box::pin(receipt_case(prefix, fault)).await?;
        }
    }
    Box::pin(lost_ack_and_revocation()).await?;
    Ok(())
}

async fn receipt_case(prefix: bool, fault: u8) -> TestResult {
    let (mut f, relay, run) = activation::ready().await?;
    f.world.runtime.prepare_reply_output(&run.run).await?;
    let original = proof::retained_publication(&f.world.runtime, &run.run).await?;
    assert_eq!(
        f.world.runtime.reconcile_reply_receipt(&run.run).await?,
        ReplyReceiptProgress::Waiting
    );
    if prefix {
        f.message("Another message before the response arrived.")
            .await?;
    }
    let before = checkpoint(&f).await?;
    let history = f
        .world
        .runtime
        .execution
        .transaction()
        .await?
        .get(
            &crate::continuity::history(&f.world.runtime.execution, before.sequence),
            false,
        )
        .await?
        .ok_or("publication history")?;
    eprintln!(
        "shared receipt prefix={prefix} fault={fault} publication history bytes={}",
        history.len()
    );
    f.world.runtime.fault.store(fault, Ordering::SeqCst);
    let result = if prefix {
        f.world.runtime.reconcile_reply_receipt(&run.run).await
    } else {
        f.world
            .runtime
            .deliver_reply_output(&run.run, relay.target(&f, relay.address))
            .await
    };
    f.world.runtime.fault.store(0, Ordering::SeqCst);
    if fault == 0 {
        assert!(result.is_ok(), "{result:?}");
    } else {
        assert_eq!(result, Err(RuntimeError::Unavailable));
    }
    let receipt_key = f.world.runtime.execution.run(&run.run, "native-receipt/1");
    let expected_ciphertext = if [159, 160, 161].contains(&fault) {
        f.world
            .runtime
            .runtime_continuity
            .as_ref()
            .ok_or("managed")?
            .lock()
            .await
            .test_pending_receipt_digest(&f.world.runtime)?
    } else {
        f.world
            .runtime
            .execution
            .transaction()
            .await?
            .get(&receipt_key, false)
            .await?
            .map(|bytes| crate::hash(&bytes))
    };
    // A later source append cannot replace the original checkpoint effect.
    f.message("Source advanced while the receipt commit was offline.")
        .await?;
    reopen(&mut f).await?;
    assert_eq!(checkpoint(&f).await?.sequence, before.sequence + 1);
    activation::restore_route(&mut f, &relay).await?;
    let tx = f.world.runtime.execution.transaction().await?;
    let receipt = tx.get(&receipt_key, false).await?.map(|v| v.to_vec());
    assert_eq!(
        receipt.as_ref().map(|bytes| crate::hash(bytes)),
        expected_ciphertext,
        "cold recovery must preserve the original receipt ciphertext"
    );
    if prefix {
        assert!(receipt.is_none());
        let mut accepted = false;
        for _ in 0..3 {
            let progress = f
                .world
                .runtime
                .deliver_reply_output(&run.run, relay.target(&f, relay.address))
                .await?;
            if matches!(progress, ReplyReceiptProgress::Accepted { .. }) {
                accepted = true;
                break;
            }
        }
        assert!(accepted);
    } else {
        let original_receipt = receipt.ok_or("retained receipt")?;
        let entries = f.log.read(&f.space, 1, 256).await?;
        let expected = entries
            .iter()
            .find(|entry| entry.env == original)
            .ok_or("actual response")?;
        assert!(
            matches!(f.world.runtime.reconcile_reply_receipt(&run.run).await?,
                    ReplyReceiptProgress::Accepted {accepted,..} if accepted.seq==expected.seq && accepted.hash==expected.hash)
        );
        // Clean removal cannot be silently rebuilt as another receipt.
        tx.clear(&receipt_key);
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        assert!(f.world.runtime.check_shared_runtime().await.is_err());
        let tx = f.world.runtime.execution.transaction().await?;
        tx.set(&receipt_key, &original_receipt);
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        f.world.runtime.check_shared_runtime().await?;
    }
    assert_eq!(
        f.log
            .read(&f.space, 1, 256)
            .await?
            .iter()
            .filter(|entry| entry.env == original)
            .count(),
        1
    );
    assert_eq!(
        proof::retained_publication(&f.world.runtime, &run.run).await?,
        original
    );
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    assert_eq!(f.world.balance().await.held_units, 0);
    let acknowledgements = f.world.runtime.reconcile(8).await?;
    assert_eq!(acknowledgements.transferred, 2);
    f.world.runtime.check_shared_runtime().await?;
    assert!(matches!(
        f.world.runtime.reconcile_reply_receipt(&run.run).await?,
        ReplyReceiptProgress::Accepted { .. }
    ));
    assert_eq!(
        f.world.runtime.continuity(),
        Err(RuntimeError::RestoreUnreconciled)
    );
    relay.stop().await;
    f.finish().await;
    println!("shared receipt prefix={prefix} fault={fault}: original native proof, cold exact metadata, source advancement, no resend/provider/charge and later financial ACK PASS");
    Ok(())
}

pub(in crate::tests) async fn lost_ack_and_revocation() -> TestResult {
    for fault in [0, 161] {
        Box::pin(lost_ack_case(fault)).await?;
    }
    Box::pin(revoked_before_send()).await?;
    Ok(())
}

async fn lost_ack_case(fault: u8) -> TestResult {
    use crate::tests::retained::image_tasks::delivery::proxy::Proxy;
    let (mut f, relay, run) = activation::ready().await?;
    f.world.runtime.prepare_reply_output(&run.run).await?;
    let original = proof::retained_publication(&f.world.runtime, &run.run).await?;
    let proxy = Proxy::start(relay.address).await;
    let target = relay.target(&f, proxy.address);
    // Drop the old route and qualify the real proxy route through the public builder.
    Arc::get_mut(&mut f.world.runtime)
        .ok_or("exclusive runtime")?
        .reply_output = None;
    let runtime = Arc::try_unwrap(f.world.runtime).map_err(|_| "exclusive runtime")?;
    f.world.runtime = Arc::new(runtime.qualify_shared_reply_output(target.clone()).await?);
    assert_eq!(
        f.world.runtime.deliver_reply_output(&run.run, target).await,
        Err(RuntimeError::Unavailable)
    );
    assert_eq!(proxy.publish_dropped.load(Ordering::SeqCst), 1);
    sqlx::query("UPDATE devices SET revoked_at=clock_timestamp() WHERE device=$1")
        .bind(f.world.agents[0].device.as_deref().ok_or("device")?)
        .execute(&f.world.runtime.finance.pool)
        .await?;
    f.world.runtime.fault.store(fault, Ordering::SeqCst);
    let result = f.world.runtime.reconcile_reply_receipt(&run.run).await;
    f.world.runtime.fault.store(0, Ordering::SeqCst);
    if fault == 0 {
        assert!(
            matches!(result, Ok(ReplyReceiptProgress::Accepted { .. })),
            "{result:?}"
        );
    } else {
        assert_eq!(result, Err(RuntimeError::Unavailable));
    }
    let receipt_key = f.world.runtime.execution.run(&run.run, "native-receipt/1");
    let expected_digest = if fault == 161 {
        f.world
            .runtime
            .runtime_continuity
            .as_ref()
            .ok_or("managed")?
            .lock()
            .await
            .test_pending_receipt_digest(&f.world.runtime)?
            .ok_or("original receipt")?
    } else {
        crate::hash(
            &f.world
                .runtime
                .execution
                .transaction()
                .await?
                .get(&receipt_key, false)
                .await?
                .ok_or("original receipt")?,
        )
    };
    reopen(&mut f).await?;
    f.world.runtime.check_shared_runtime().await?;
    let tx = f.world.runtime.execution.transaction().await?;
    assert_eq!(
        crate::hash(&tx.get(&receipt_key, false).await?.ok_or("cold receipt")?),
        expected_digest
    );
    assert!(tx
        .get(
            &f.world.runtime.execution.run(&run.run, "native-receipt/1"),
            false
        )
        .await?
        .is_some());
    assert_eq!(
        f.log
            .read(&f.space, 1, 256)
            .await?
            .iter()
            .filter(|entry| entry.env == original)
            .count(),
        1
    );
    assert_eq!(proxy.publish_dropped.load(Ordering::SeqCst), 1);
    assert_eq!(proxy.blob_dropped.load(Ordering::SeqCst), 0);
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    assert_eq!(f.world.balance().await.held_units, 0);
    drop(proxy);
    relay.stop().await;
    f.finish().await;
    println!("shared actual lost WS ACK fault={fault}: one original response, historical receipt and cold recovery after device revocation, no resend/provider/charge PASS");
    Ok(())
}

async fn revoked_before_send() -> TestResult {
    let (f, relay, run) = activation::ready().await?;
    f.world.runtime.prepare_reply_output(&run.run).await?;
    let before = f.log.read(&f.space, 1, 256).await?;
    sqlx::query("UPDATE devices SET revoked_at=clock_timestamp() WHERE device=$1")
        .bind(f.world.agents[0].device.as_deref().ok_or("device")?)
        .execute(&f.world.runtime.finance.pool)
        .await?;
    assert_eq!(
        f.world
            .runtime
            .deliver_reply_output(&run.run, relay.target(&f, relay.address))
            .await,
        Err(RuntimeError::Denied)
    );
    assert_eq!(f.log.read(&f.space, 1, 256).await?, before);
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    assert_eq!(f.world.balance().await.held_units, 0);
    relay.stop().await;
    f.finish().await;
    println!("shared revoked device before delivery: no new response, original incurred cost retained PASS");
    Ok(())
}
