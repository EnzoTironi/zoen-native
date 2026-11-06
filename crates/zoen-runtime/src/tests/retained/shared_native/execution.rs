//! The public API incurs, retains, signs, delivers and recognizes one reply.
use super::*;
use crate::tests::retained::image_tasks::delivery::relay::Relay;
use native::testing::output as proof;

pub(super) async fn unpaid() -> Result<(Fixture, Relay, crate::ReplyRun), Box<dyn std::error::Error>>
{
    let mut f = Fixture::with_shared_runtime().await?;
    f.grant("shared-public-execution-owner").await?;
    f.message(ORIGINAL).await?;
    let run = f.sync(false).await?.runs.into_iter().next().ok_or("run")?;
    assert!(matches!(
        Box::pin(f.world.runtime.run_model(&run.run)).await,
        Err(RuntimeError::CoreAuthorityUnavailable)
    ));
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    output_route::configured(&f, true).await?;
    let relay = Relay::start(&f).await?;
    let target = relay.target(&f, relay.address);
    let runtime = Arc::try_unwrap(f.world.runtime).map_err(|_| "exclusive runtime")?;
    f.world.runtime = Arc::new(runtime.qualify_shared_reply_output(target).await?);
    Ok((f, relay, run))
}
pub(super) async fn qualify(
    mut f: Fixture,
    relay: &Relay,
) -> Result<Fixture, Box<dyn std::error::Error>> {
    let target = relay.target(&f, relay.address);
    assert!(f.world.runtime.production_output_scope().is_err());
    let runtime = Arc::try_unwrap(f.world.runtime).map_err(|_| "exclusive runtime")?;
    f.world.runtime = Arc::new(runtime.qualify_shared_reply_output(target).await?);
    Ok(f)
}
pub(in crate::tests) async fn run() -> TestResult {
    Box::pin(public_cycle()).await?;
    Box::pin(unknown_cost()).await?;
    for kind in [0, 1, 2] {
        Box::pin(refused_before_call(kind)).await?;
    }
    for fault in [105, 109, 117, 119, 120] {
        Box::pin(public_cut(fault)).await?;
    }
    Ok(())
}
async fn public_cycle() -> TestResult {
    let (mut f, relay, run) = Box::pin(unpaid()).await?;
    let result = Box::pin(f.world.runtime.run_model(&run.run)).await?;
    assert_eq!(
        result.financial,
        crate::FinancialState::Settled { units: 6 }
    );
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        Box::pin(f.world.runtime.run_model(&run.run))
            .await?
            .financial,
        result.financial
    );
    let prepared = f.world.runtime.prepare_reply_output(&run.run).await?;
    let original = proof::retained_publication(&f.world.runtime, &run.run).await?;
    let delivered = f
        .world
        .runtime
        .deliver_reply_output(&run.run, relay.target(&f, relay.address))
        .await?;
    assert!(
        matches!(delivered, crate::ReplyReceiptProgress::Accepted { .. }),
        "{delivered:?}"
    );
    let before = checkpoint(&f).await?;
    reopen(&mut f).await?;
    assert!(matches!(
        Box::pin(f.world.runtime.run_model(&run.run)).await,
        Err(RuntimeError::CoreAuthorityUnavailable)
    ));
    f = Box::pin(qualify(f, &relay)).await?;
    assert_eq!(
        Box::pin(f.world.runtime.run_model(&run.run))
            .await?
            .financial,
        result.financial
    );
    assert_eq!(
        f.world.runtime.prepare_reply_output(&run.run).await?,
        prepared
    );
    assert_eq!(
        f.world.runtime.reconcile_reply_receipt(&run.run).await?,
        delivered
    );
    assert_eq!(
        proof::retained_publication(&f.world.runtime, &run.run).await?,
        original
    );
    assert_eq!(checkpoint(&f).await?, before);
    assert_eq!(
        f.log
            .read(&f.space, 1, 256)
            .await?
            .iter()
            .filter(|ev| ev.env == original)
            .count(),
        1
    );
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    assert_eq!(f.world.balance().await.held_units, 0);
    assert_eq!(
        f.world.count("SELECT count(*) FROM runtime_claims").await,
        1
    );
    assert_eq!(
        f.world.runtime.continuity(),
        Err(RuntimeError::RestoreUnreconciled)
    );
    relay.stop().await;
    f.finish().await;
    println!("shared public agent cycle: actual native trigger, public paid invocation, one provider/claim/charge, original delivered reply, cold route requalification and retained result/receipt PASS");
    Ok(())
}

async fn unknown_cost() -> TestResult {
    let (f, relay, run) = Box::pin(unpaid()).await?;
    f.world
        .http
        .body
        .lock()
        .map_err(|_| "HTTP body")?
        .as_object_mut()
        .ok_or("HTTP object")?
        .remove("usage");
    let result = Box::pin(f.world.runtime.run_model(&run.run)).await?;
    assert_eq!(result.financial, crate::FinancialState::Claimed);
    let held = f.world.balance().await.held_units;
    assert!(held > 0);
    assert_eq!(
        Box::pin(f.world.runtime.run_model(&run.run))
            .await?
            .financial,
        result.financial
    );
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.held_units, held);
    assert_eq!(f.world.balance().await.spent_units, 0);
    relay.stop().await;
    f.finish().await;
    println!("shared public unknown cost: original completed result reused, one HTTP/claim, hold retained without invented usage or repeat charge PASS");
    Ok(())
}

async fn refused_before_call(kind: u8) -> TestResult {
    let (mut f, relay, run) = Box::pin(unpaid()).await?;
    match kind {
        0 => {
            sqlx::query("UPDATE devices SET revoked_at=clock_timestamp() WHERE device=$1")
                .bind(f.world.agents[0].device.as_deref().ok_or("device")?)
                .execute(&f.world.runtime.finance.pool)
                .await?;
        }
        1 => {
            f.world.runtime.cancel_model(&run.run).await?;
        }
        2 => {
            f.message("Source changed before any paid call.").await?;
        }
        _ => return Err("unknown refusal".into()),
    }
    assert!(matches!(
        Box::pin(f.world.runtime.run_model(&run.run)).await,
        Err(RuntimeError::Denied)
    ));
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(f.world.balance().await.spent_units, 0);
    assert_eq!(f.world.balance().await.held_units, 0);
    assert_eq!(
        f.world.count("SELECT count(*) FROM runtime_claims").await,
        0
    );
    relay.stop().await;
    f.finish().await;
    println!("shared public refusal kind={kind}: revoked device/cancelled run/advanced source creates no hold, claim, provider call or charge PASS");
    Ok(())
}

async fn public_cut(fault: u8) -> TestResult {
    let (mut f, relay, run) = Box::pin(unpaid()).await?;
    f.world.runtime.fault.store(fault, Ordering::SeqCst);
    assert!(matches!(
        Box::pin(f.world.runtime.run_model(&run.run)).await,
        Err(RuntimeError::Unavailable)
    ));
    f.world.runtime.fault.store(0, Ordering::SeqCst);
    let incurred = fault >= 119;
    assert_eq!(
        f.world.http.sends.load(Ordering::SeqCst),
        usize::from(incurred)
    );
    reopen(&mut f).await?;
    f = Box::pin(qualify(f, &relay)).await?;
    if incurred {
        // Ordinary public maintenance completes retained correspondence/ACKs.
        f.world.runtime.reconcile(8).await?;
    }
    let retry = Box::pin(f.world.runtime.run_model(&run.run)).await;
    if fault == 105 || incurred {
        assert_eq!(
            retry?.financial,
            crate::FinancialState::Settled { units: 6 }
        );
        assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
        assert_eq!(f.world.balance().await.spent_units, 6);
        assert_eq!(f.world.balance().await.held_units, 0);
        f.world.runtime.prepare_reply_output(&run.run).await?;
        assert!(matches!(
            f.world
                .runtime
                .deliver_reply_output(&run.run, relay.target(&f, relay.address))
                .await?,
            crate::ReplyReceiptProgress::Accepted { .. }
        ));
    } else {
        // A historical claim/witness cannot reconstruct the lost live permit.
        assert!(matches!(retry, Err(RuntimeError::Unavailable)));
        assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
        assert_eq!(f.world.balance().await.spent_units, 0);
        assert!(f.world.balance().await.held_units > 0);
    }
    assert_eq!(
        f.world.count("SELECT count(*) FROM runtime_claims").await,
        1
    );
    relay.stop().await;
    f.finish().await;
    println!("shared public cut={fault}: cold original, no recreated dispatch right, at most one provider/charge, public result recovery and delivery when incurred PASS");
    Ok(())
}
