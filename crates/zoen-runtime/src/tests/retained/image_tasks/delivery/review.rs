//! Actual original review publication, interrupted before send and after ACK.
use super::*;

pub(in crate::tests::retained::image_tasks) async fn run(
    f: &mut Fixture,
    intent: crate::ImageReviewIntent,
) -> (String, String, Envelope) {
    let relay = relay::Relay::start(f).await;
    Box::pin(binding::genesis(f, &relay)).await;
    let proxy = proxy::Proxy::start(relay.address).await;
    let target = relay.target(f, proxy.address);
    let unavailable = crate::ImageDeliveryTarget::configured(
        "http://127.0.0.1:1".into(),
        relay.state.relay_name.clone(),
        f.world.relay_cell.clone().unwrap(),
    )
    .unwrap();
    let before = head(f).await;
    assert_eq!(
        proof::review::stage(&f.world.runtime, &intent, &unavailable)
            .await
            .unwrap_err(),
        RuntimeError::Unavailable
    );
    let original = proof::review::original(&f.world.runtime, &intent).await;
    let checkpoint = proof::review::checkpoint(&f.world.runtime, &intent)
        .await
        .unwrap();
    crate::tests::managed_images::assert_publish_checkpoint(
        &f.world.runtime,
        f.managed_directory.as_ref().unwrap(),
        &original.2,
        &checkpoint,
    )
    .await;
    let before_publish = restore::Snapshot::capture(f).await;
    assert_eq!(head(f).await, before);
    output::cold(f).await;
    assert_eq!(
        proof::review::original(&f.world.runtime, &intent).await,
        original
    );

    // The real relay sequences the retained exact original and emits Accepted;
    // the proxy drops it. No replacement successful frame is generated.
    assert_eq!(
        proof::review::stage(&f.world.runtime, &intent, &target)
            .await
            .unwrap_err(),
        RuntimeError::Unavailable
    );
    assert_eq!(proxy.publish_dropped.load(Ordering::SeqCst), 1);
    let after = head(f).await;
    assert_eq!(after.seq, before.seq + 1);
    let sequenced = f.log.read(&f.space, after.seq, 1).await.unwrap().remove(0);
    assert_eq!(sequenced.env, original.2);
    assert_eq!(
        relay.state.metrics.events_sequenced.load(Ordering::SeqCst),
        1
    );

    output::cold(f).await;
    let progress = proof::review::stage(&f.world.runtime, &intent, &target)
        .await
        .unwrap();
    assert_eq!(
        progress,
        crate::ImageReviewProgress::Published {
            request: original.0.clone(),
            content_hash: original.1.clone(),
            accepted: after.clone(),
        }
    );
    let generation =
        native::testing::generation(&f.world.runtime, &intent.agent, &intent.device).await;
    output::cold(f).await;
    assert_eq!(
        proof::review::stage(&f.world.runtime, &intent, &target)
            .await
            .unwrap(),
        progress
    );
    assert_eq!(
        native::testing::generation(&f.world.runtime, &intent.agent, &intent.device).await,
        generation
    );
    assert_eq!(
        proof::review::original(&f.world.runtime, &intent).await,
        original
    );
    assert_eq!(head(f).await, after);
    assert_eq!(
        relay.state.metrics.events_duplicate.load(Ordering::SeqCst),
        0
    );
    assert_eq!(
        f.world.count("SELECT count(*) FROM runtime_claims").await,
        0
    );
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    let direct = relay.target(f, relay.address);
    restore::refuses_consumed_original(f, &intent, &direct, &before_publish, &original).await;
    assert_eq!(head(f).await, after);
    assert_eq!(
        relay.state.metrics.events_sequenced.load(Ordering::SeqCst),
        1
    );
    f.head = Some(after);
    relay.stop().await;
    println!("native image review: durable original before send, real lost WS Accepted, cold exact-wire recovery and one review/no paid attempt PASS (production gate closed)");
    original
}
