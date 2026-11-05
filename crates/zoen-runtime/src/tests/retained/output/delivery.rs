//! Actual text relay publication, ACK loss and cold source reconciliation.
use super::relay_fences::late_relay_fence_races;
use super::*;
use crate::tests::retained::image_tasks::delivery::{proxy::Proxy, relay::Relay};

pub(in crate::tests) async fn run() -> std::result::Result<(), Box<dyn std::error::Error>> {
    cancellation_after_rebind().await?;
    late_relay_fence_races().await?;
    for lose_ack in [false, true] {
        let mut fixture = Fixture::with_managed_output().await?;
        fixture.grant("text-delivery-trust").await?;
        fixture.message(ORIGINAL).await?;
        let run = fixture.sync(false).await?.runs.remove(0);
        fixture.world.runtime.run_model(&run.run).await?;
        let prepared = proof::prepare(&fixture.world.runtime, &run.run).await?;
        let original = proof::publication(&fixture.world.runtime, &run.run).await;
        let relay = Relay::start(&fixture).await?;
        let proxy = Proxy::start(relay.address).await;
        let address = if lose_ack {
            proxy.address
        } else {
            relay.address
        };
        let target = relay.target(&fixture, address);
        let scope = native::testing::qualify_output(&fixture.world.runtime, target.clone()).await?;
        assert_eq!(
            fixture
                .world
                .runtime
                .deliver_reply_output(&run.run, target.clone())
                .await,
            Err(RuntimeError::CoreAuthorityUnavailable)
        );
        let native = fixture
            .world
            .runtime
            .native
            .as_ref()
            .ok_or("native custody")?;
        let result = native
            .deliver_output(&fixture.world.runtime, &run.run, &target, &scope)
            .await;
        if lose_ack {
            assert_eq!(result, Err(RuntimeError::Unavailable));
            assert_eq!(proxy.publish_dropped.load(Ordering::SeqCst), 1);
        } else {
            assert!(
                matches!(result, Ok(ReplyReceiptProgress::Accepted { .. })),
                "{result:?}"
            );
        }
        let entries = fixture.log.read(&fixture.space, 1, 256).await?;
        assert_eq!(entries.iter().filter(|p| p.env == original).count(), 1);
        let accepted = entries.last().ok_or("accepted source position")?.clone();
        assert_eq!(accepted.env, original);
        drop(scope);
        cold(&mut fixture).await;
        // Lost transport ACK resolves from the actual sequencer, not a second
        // socket publication. Model and paid capture are not entered again.
        let target = relay.target(&fixture, address);
        let scope = native::testing::qualify_output(&fixture.world.runtime, target.clone()).await?;
        let native = fixture
            .world
            .runtime
            .native
            .as_ref()
            .ok_or("native custody")?;
        let result = native
            .deliver_output(&fixture.world.runtime, &run.run, &target, &scope)
            .await?;
        assert!(
            matches!(result, ReplyReceiptProgress::Accepted { accepted: position, .. }
            if position.seq == accepted.seq && position.hash == accepted.hash)
        );
        assert_eq!(
            proof::prepare(&fixture.world.runtime, &run.run).await?,
            prepared
        );
        assert_eq!(
            proof::publication(&fixture.world.runtime, &run.run).await,
            original
        );
        assert_one_bill(&fixture).await;
        assert_eq!(
            fixture.log.read(&fixture.space, 1, 256).await?.len(),
            entries.len()
        );
        assert_eq!(proxy.blob_dropped.load(Ordering::SeqCst), 0);
        drop(scope);
        relay.stop().await;
        fixture.world.finish().await;
        println!("managed text actual relay delivery lose_ack={lose_ack}: exact one sealed response, cold original-source receipt, one bill PASS");
    }
    Ok(())
}

async fn cancellation_after_rebind() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::with_managed_output().await?;
    f.grant("text-delivery-cancel").await?;
    f.message(ORIGINAL).await?;
    let run = f.sync(false).await?.runs.remove(0);
    f.world.runtime.run_model(&run.run).await?;
    proof::prepare(&f.world.runtime, &run.run).await?;
    let original = proof::publication(&f.world.runtime, &run.run).await;
    let relay = Relay::start(&f).await?;
    let target = relay.target(&f, relay.address);
    let scope = native::testing::qualify_output(&f.world.runtime, target.clone()).await?;
    let before = f.log.read(&f.space, 1, 256).await?;
    let runtime = &f.world.runtime;
    let native = runtime.native.as_ref().ok_or("native custody")?;
    runtime.fault.store(72, Ordering::SeqCst);
    let (delivery, cancellation) = tokio::join!(
        native.deliver_output(runtime, &run.run, &target, &scope),
        async {
            let result = async {
                tokio::time::timeout(
                    Duration::from_secs(10),
                    runtime.native_cut_entered.notified(),
                )
                .await?;
                runtime.cancel_model(&run.run).await?;
                Ok::<(), Box<dyn std::error::Error>>(())
            }
            .await;
            runtime.native_cut_resume.notify_one();
            result
        }
    );
    runtime.fault.store(0, Ordering::SeqCst);
    cancellation?;
    assert_eq!(delivery, Err(RuntimeError::Denied));
    assert_eq!(f.log.read(&f.space, 1, 256).await?, before);
    assert_eq!(
        relay.state.metrics.events_sequenced.load(Ordering::SeqCst),
        0
    );
    assert_eq!(proof::publication(runtime, &run.run).await, original);
    assert_one_bill(&f).await;
    drop(scope);
    relay.stop().await;
    f.world.finish().await;
    println!("managed text actual post-rebind cancellation: fresh FDB authorization refuses before Publish, zero sequenced events, original response and bill retained PASS");
    Ok(())
}
